//! Port of `src/watcher.ts` + `src/semaphore.ts`.
//!
//! Watches the intake directory, locks new files, moves them to quarantine, and
//! runs the scan pipeline (local clamd → VirusTotal) updating the job store at
//! each stage. chokidar is replaced by `notify` plus a hand-rolled stability
//! tracker that mirrors chokidar's `awaitWriteFinish` (emit a file only once its
//! size has been stable for `WATCH_STABILITY_MS`, polled every `WATCH_POLL_MS`).
//! The per-scan `AbortController` map becomes a `CancellationToken` map; the
//! async semaphore becomes `tokio::sync::Semaphore`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::FailureMode;
use crate::file_mover::FileMover;
use crate::file_permissions;
use crate::job_store::{JobStore, ScanResult};
use crate::local_scanner::{LocalScanner, LocalVerdict};
use crate::metrics::Metrics;
use crate::mode::WatcherMode;
use crate::virus_checker::{VirusChecker, VirusVerdict, VtStage};

const BROWSER_TEMP_EXTENSIONS: [&str; 5] =
    [".crdownload", ".download", ".part", ".opdownload", ".tmp"];

const QUARANTINE_XATTR_VALUE: &str = "0083;00000000;FileSandbox;";

/// How long after restoring a file to the watch folder we ignore every event for
/// that path. A restore writes content and then adjusts metadata (chmod 0o644 +
/// clearing the quarantine xattr), each of which surfaces as a filesystem event;
/// without this settle window the freshly-cleaned file is re-detected and
/// re-quarantined, looping Moved→Restored→Moved forever. Generous enough to cover
/// FSEvents coalescing/latency, short enough not to mask a genuine re-download.
const RESTORE_SUPPRESS_WINDOW: Duration = Duration::from_secs(5);

fn is_browser_temp(name: &str) -> bool {
    BROWSER_TEMP_EXTENSIONS
        .iter()
        .any(|ext| name.ends_with(ext))
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// True if `path` is inside its post-restore settle window, so its own
/// restore-generated events must be ignored. The window slides: every suppressed
/// event refreshes the timestamp, so a slow restore (a large file whose copy
/// outlives a single window, plus the trailing chmod/xattr) stays suppressed for
/// its whole duration and only expires once events stop for the full window. The
/// marker is evicted once expired.
fn restore_suppressed(inner: &WatcherInner, path: &Path) -> bool {
    let mut rp = inner.restoring_paths.lock().unwrap();
    match rp.get_mut(path) {
        Some(t) if t.elapsed() < RESTORE_SUPPRESS_WINDOW => {
            *t = std::time::Instant::now();
            true
        }
        Some(_) => {
            rp.remove(path);
            false
        }
        None => false,
    }
}

fn parse_verdict(s: &str) -> VirusVerdict {
    match s {
        "clean" => VirusVerdict::Clean,
        "infected" => VirusVerdict::Infected,
        "oversized" => VirusVerdict::Oversized,
        _ => VirusVerdict::Inconclusive,
    }
}

#[cfg(unix)]
async fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await {
        eprintln!("[chmod] failed to set {mode:o} on {}: {e}", path.display());
    }
}

#[cfg(not(unix))]
async fn chmod(_path: &Path, _mode: u32) {}

async fn set_quarantine_xattr(path: &Path) {
    let status = tokio::process::Command::new("xattr")
        .arg("-w")
        .arg("com.apple.quarantine")
        .arg(QUARANTINE_XATTR_VALUE)
        .arg(path)
        .status()
        .await;
    if let Ok(s) = status {
        if !s.success() {
            eprintln!(
                "[xattr] Failed to set quarantine xattr on {}",
                path.display()
            );
        }
    }
}

pub struct WatcherOptions {
    pub watch_recursive: bool,
    pub max_scan_bytes: u64,
    pub max_concurrent_scans: u32,
    pub use_separate_vt_process: bool,
    pub local_scanner: Option<LocalScanner>,
    pub pompelmi_failure_mode: FailureMode,
    pub initial_mode: WatcherMode,
    pub vt_enabled: bool,
    pub vt_hash_only: bool,
    pub on_mode_change: Option<Box<dyn Fn(WatcherMode) + Send + Sync>>,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            watch_recursive: true,
            max_scan_bytes: 400 * 1024 * 1024,
            max_concurrent_scans: 2,
            use_separate_vt_process: false,
            local_scanner: None,
            pompelmi_failure_mode: FailureMode::Bypass,
            initial_mode: WatcherMode::Active,
            vt_enabled: true,
            vt_hash_only: true,
            on_mode_change: None,
        }
    }
}

struct WatcherInner {
    watch_path: PathBuf,
    ignored: Vec<String>,
    mover: FileMover,
    virus_checker: VirusChecker,
    job_store: Arc<JobStore>,
    watch_recursive: bool,
    max_scan_bytes: u64,
    scan_semaphore: Arc<Semaphore>,
    mode: Mutex<WatcherMode>,
    on_mode_change: Option<Box<dyn Fn(WatcherMode) + Send + Sync>>,
    vt_enabled: bool,
    local_scanner: Option<LocalScanner>,
    pompelmi_failure_mode: FailureMode,
    /// Paths we just restored → when. Events for these are ignored for
    /// [`RESTORE_SUPPRESS_WINDOW`] so a restored clean file is not re-quarantined.
    restoring_paths: Mutex<HashMap<PathBuf, std::time::Instant>>,
    processing_paths: Mutex<HashSet<PathBuf>>,
    scan_controllers: Mutex<HashMap<String, CancellationToken>>,
    metrics: Arc<Metrics>,
}

#[derive(Clone)]
pub struct Watcher(Arc<WatcherInner>);

/// Keeps the notify watcher and consumer task alive. Dropping it stops watching.
pub struct WatcherHandles {
    _watcher: notify::RecommendedWatcher,
    _task: tokio::task::JoinHandle<()>,
}

impl Watcher {
    pub fn new(
        watch_path: impl Into<PathBuf>,
        ignored: Vec<String>,
        quarantine_path: impl Into<PathBuf>,
        api_key: impl Into<String>,
        job_store: Arc<JobStore>,
        metrics: Arc<Metrics>,
        opts: WatcherOptions,
    ) -> Self {
        let quarantine_path = quarantine_path.into();
        let concurrent = opts.max_concurrent_scans.max(1) as usize;
        let virus_checker = VirusChecker::new(
            api_key,
            Some(opts.max_scan_bytes),
            opts.use_separate_vt_process,
            opts.vt_hash_only,
        );
        Self(Arc::new(WatcherInner {
            watch_path: watch_path.into(),
            ignored,
            mover: FileMover::new(quarantine_path),
            virus_checker,
            job_store,
            watch_recursive: opts.watch_recursive,
            max_scan_bytes: opts.max_scan_bytes,
            scan_semaphore: Arc::new(Semaphore::new(concurrent)),
            mode: Mutex::new(opts.initial_mode),
            on_mode_change: opts.on_mode_change,
            vt_enabled: opts.vt_enabled,
            local_scanner: opts.local_scanner,
            pompelmi_failure_mode: opts.pompelmi_failure_mode,
            restoring_paths: Mutex::new(HashMap::new()),
            processing_paths: Mutex::new(HashSet::new()),
            scan_controllers: Mutex::new(HashMap::new()),
            metrics,
        }))
    }

    /// Skip re-scan when restoring from the API or a clean-pipeline restore.
    pub fn mark_restoring(&self, dest_path: PathBuf) {
        self.0
            .restoring_paths
            .lock()
            .unwrap()
            .insert(dest_path, std::time::Instant::now());
    }

    pub fn get_mode(&self) -> WatcherMode {
        *self.0.mode.lock().unwrap()
    }

    pub fn set_mode(&self, next: WatcherMode) {
        let prev = {
            let mut m = self.0.mode.lock().unwrap();
            if *m == next {
                return;
            }
            let prev = *m;
            *m = next;
            prev
        };
        // Leaving active aborts any in-flight scans.
        if prev == WatcherMode::Active && next != WatcherMode::Active {
            for token in self.0.scan_controllers.lock().unwrap().values() {
                token.cancel();
            }
        }
        if let Some(cb) = &self.0.on_mode_change {
            cb(next);
        }
    }

    pub fn cancel(&self, job_id: &str) {
        if let Some(token) = self.0.scan_controllers.lock().unwrap().remove(job_id) {
            token.cancel();
        }
    }

    /// Start watching. The returned handles must be kept alive.
    pub fn start(&self) -> Result<WatcherHandles> {
        let stability_ms: u64 = std::env::var("WATCH_STABILITY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2000);
        let poll_ms: u64 = std::env::var("WATCH_POLL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();

        let mut watcher = {
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            })?
        };
        {
            use notify::Watcher as _;
            let mode = if self.0.watch_recursive {
                notify::RecursiveMode::Recursive
            } else {
                notify::RecursiveMode::NonRecursive
            };
            watcher.watch(&self.0.watch_path, mode)?;
        }

        let inner = self.0.clone();
        let task = tokio::spawn(async move {
            let stability = Duration::from_millis(stability_ms);
            let mut pending: HashMap<PathBuf, (u64, Instant)> = HashMap::new();
            let mut ticker = tokio::time::interval(Duration::from_millis(poll_ms));

            loop {
                tokio::select! {
                    maybe_event = rx.recv() => {
                        match maybe_event {
                            Some(event) => Self::on_raw_event(&inner, &mut pending, event).await,
                            None => break, // sender dropped → watcher gone
                        }
                    }
                    _ = ticker.tick() => {
                        Self::flush_stable(&inner, &mut pending, stability).await;
                    }
                }
            }
        });

        Ok(WatcherHandles {
            _watcher: watcher,
            _task: task,
        })
    }

    /// React to a raw notify event: immediately lock newly-created files and
    /// (re)arm the stability tracker for create/modify paths.
    async fn on_raw_event(
        inner: &Arc<WatcherInner>,
        pending: &mut HashMap<PathBuf, (u64, Instant)>,
        event: notify::Event,
    ) {
        use notify::EventKind;
        if *inner.mode.lock().unwrap() == WatcherMode::MonitoringDisabled {
            return;
        }
        let is_change = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_));
        let is_remove = matches!(event.kind, EventKind::Remove(_));

        for path in event.paths {
            let name = base_name(&path);
            if name.is_empty() || is_browser_temp(&name) {
                continue;
            }
            if inner.ignored.iter().any(|ign| name.ends_with(ign)) {
                continue;
            }
            if is_remove {
                pending.remove(&path);
                continue;
            }
            // Ignore a path we just restored, for the settle window: the restore's
            // own content/chmod/xattr events must not re-lock and re-quarantine the
            // freshly-cleaned file (the Moved→Restored→Moved duplicate loop).
            if restore_suppressed(inner, &path) {
                continue;
            }
            // Reject symlinks and directories. `symlink_metadata` does NOT follow
            // the link, so a symlink dropped into the watch folder is detected and
            // skipped — we must never chmod, copy, or scan the file it points at
            // (that would let an attacker lock down or exfiltrate an arbitrary file
            // such as ~/.ssh/id_ed25519 simply by naming a symlink after it).
            let meta = match tokio::fs::symlink_metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                eprintln!(
                    "[watcher] ignoring symlink (will not lock or scan its target): {}",
                    path.display()
                );
                continue;
            }
            if meta.is_dir() {
                continue;
            }
            let skip_lock = inner.processing_paths.lock().unwrap().contains(&path);

            // Lock on the FIRST event of any kind for this path (Create OR a
            // rename-in Modify), not only Create. The atomic write-temp-then-rename
            // pattern used by browsers/editors surfaces as Modify; gating on Create
            // alone left such files readable/executable until the ~2s stability
            // timer fired. `pending` membership marks an already-seen path so a
            // re-modify never re-chmods.
            if is_change && !skip_lock && !pending.contains_key(&path) {
                chmod(&path, 0o000).await;
                set_quarantine_xattr(&path).await;
            }

            if is_change {
                let size = meta.len();
                pending
                    .entry(path.clone())
                    .and_modify(|(prev_size, since)| {
                        if *prev_size != size {
                            *prev_size = size;
                            *since = Instant::now();
                        }
                    })
                    .or_insert((size, Instant::now()));
            }
        }
    }

    /// Emit files whose size has been stable for `stability`.
    async fn flush_stable(
        inner: &Arc<WatcherInner>,
        pending: &mut HashMap<PathBuf, (u64, Instant)>,
        stability: Duration,
    ) {
        // Evict expired restore markers so the map can't grow unbounded across a
        // long-running session with many restores.
        inner
            .restoring_paths
            .lock()
            .unwrap()
            .retain(|_, t| t.elapsed() < RESTORE_SUPPRESS_WINDOW);

        let now = Instant::now();
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, (_, since))| now.duration_since(*since) >= stability)
            .map(|(p, _)| p.clone())
            .collect();
        for path in ready {
            pending.remove(&path);
            let w = Watcher(inner.clone());
            tokio::spawn(async move { w.handle_file(path).await });
        }
    }

    async fn handle_file(&self, filepath: PathBuf) {
        let inner = &self.0;
        if *inner.mode.lock().unwrap() == WatcherMode::MonitoringDisabled {
            return;
        }
        let fname = base_name(&filepath);
        if is_browser_temp(&fname) {
            return;
        }
        if inner.ignored.contains(&fname) {
            return;
        }
        // Re-check at handle time: a symlink (or a dir) must never enter the
        // quarantine/scan pipeline, even if one was swapped in after the event.
        match tokio::fs::symlink_metadata(&filepath).await {
            Ok(m) if m.file_type().is_symlink() => return,
            Ok(m) if m.is_dir() => return,
            Err(_) => return,
            Ok(_) => {}
        }
        // Skip a path inside its post-restore settle window (defense in depth;
        // on_raw_event already drops these before they reach pending).
        if restore_suppressed(inner, &filepath) {
            return;
        }
        // Dedupe concurrent add/change for the same path.
        if !inner
            .processing_paths
            .lock()
            .unwrap()
            .insert(filepath.clone())
        {
            return;
        }

        self.run_pipeline(&filepath).await;

        inner.processing_paths.lock().unwrap().remove(&filepath);
    }

    async fn run_pipeline(&self, filepath: &Path) {
        let inner = &self.0;
        set_quarantine_xattr(filepath).await;

        let job_id = Uuid::new_v4().to_string();
        let original_name = base_name(filepath);
        let _ =
            inner
                .job_store
                .insert_received(&job_id, &filepath.to_string_lossy(), &original_name);
        let _ = inner.job_store.set_stage(&job_id, "received");

        if let Err(e) = self.pipeline_inner(&job_id, filepath).await {
            let _ = inner.job_store.set_stage(&job_id, "error");
            inner.metrics.set_last_error(Some(e.to_string()));
            let _ = inner.job_store.fail(&job_id, &e.to_string());
            eprintln!("Failed processing {}: {e}", filepath.display());
        }
    }

    async fn pipeline_inner(&self, job_id: &str, filepath: &Path) -> Result<()> {
        let inner = &self.0;
        let js = &inner.job_store;

        chmod(filepath, 0o444).await;

        let moved = inner.mover.move_in(filepath).await?;
        let quarantine = moved.quarantine_file_path;
        let original = moved.original_base_name;
        let qstr = quarantine.to_string_lossy().into_owned();
        js.set_in_quarantine(job_id, &qstr)?;
        file_permissions::change_permissions(&quarantine, 0o444).await;

        // scan_paused → keep quarantined without scanning.
        if *inner.mode.lock().unwrap() == WatcherMode::ScanPaused {
            js.set_stage(job_id, "done")?;
            js.set_scan_result(
                job_id,
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "Scanning paused at intake".into(),
                },
            )?;
            return Ok(());
        }

        let no_scanners = inner.local_scanner.is_none() && !inner.vt_enabled;
        if no_scanners {
            js.set_stage(job_id, "done")?;
            js.set_scan_result(
                job_id,
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "No active scanners - kept in quarantine".into(),
                },
            )?;
            return Ok(());
        }

        js.set_scanning(job_id)?;

        // Local clamd stage (defense-in-depth).
        if let Some(scanner) = &inner.local_scanner {
            js.set_stage(job_id, "local_scan")?;
            let token = CancellationToken::new();
            inner
                .scan_controllers
                .lock()
                .unwrap()
                .insert(job_id.to_string(), token.clone());
            let local = tokio::select! {
                r = scanner.check(&quarantine) => r,
                _ = token.cancelled() => crate::local_scanner::LocalScanResult {
                    verdict: LocalVerdict::Error,
                    message: "clamd exception: cancelled".into(),
                },
            };
            inner.scan_controllers.lock().unwrap().remove(job_id);
            js.set_pompelmi_verdict(job_id, local.verdict.as_str(), Some(&local.message))?;

            match local.verdict {
                LocalVerdict::Malicious => {
                    js.set_stage(job_id, "done")?;
                    js.set_scan_result(
                        job_id,
                        &ScanResult {
                            verdict: "infected".into(),
                            message: format!("Local scanner: {}", local.message),
                        },
                    )?;
                    return Ok(());
                }
                LocalVerdict::Error => {
                    if inner.pompelmi_failure_mode == FailureMode::Inconclusive {
                        js.set_stage(job_id, "done")?;
                        js.set_scan_result(
                            job_id,
                            &ScanResult {
                                verdict: "inconclusive".into(),
                                message: format!("Local scanner failed: {}", local.message),
                            },
                        )?;
                        return Ok(());
                    }
                    // bypass → fall through to VT
                }
                LocalVerdict::Clean => {}
            }
        }

        if !inner.vt_enabled {
            js.set_stage(job_id, "done")?;
            js.set_scan_result(
                job_id,
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "VT disabled - no cloud scan ran".into(),
                },
            )?;
            return Ok(());
        }

        // Oversized guard.
        let meta = tokio::fs::metadata(&quarantine).await?;
        if meta.len() > inner.max_scan_bytes {
            js.set_stage(job_id, "done")?;
            js.set_scan_result(job_id, &ScanResult {
                verdict: "oversized".into(),
                message: format!(
                    "File exceeds scan limit ({} bytes); not sent to VirusTotal. Restore or delete from the UI.",
                    inner.max_scan_bytes
                ),
            })?;
            return Ok(());
        }

        // SHA-256 cache check.
        js.set_stage(job_id, "cache_check")?;
        if let Some(cached) = crate::vt_cache::check(&quarantine) {
            let verdict = parse_verdict(&cached);
            js.set_stage(job_id, "done")?;
            js.set_scan_result(
                job_id,
                &ScanResult {
                    verdict: verdict.as_str().into(),
                    message: "From local cache (SHA-256 match)".into(),
                },
            )?;
            if verdict == VirusVerdict::Clean {
                self.restore_clean(job_id, &quarantine, &original).await?;
            }
            return Ok(());
        }

        // VirusTotal scan, bounded by the semaphore.
        let token = CancellationToken::new();
        inner
            .scan_controllers
            .lock()
            .unwrap()
            .insert(job_id.to_string(), token.clone());

        let permit = inner
            .scan_semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        inner.metrics.inc_scan();
        let result = {
            let on_stage = |stage: VtStage| {
                let s = match stage {
                    VtStage::HashLookup => "vt_lookup",
                    VtStage::Upload => "vt_upload",
                    VtStage::Poll => "vt_poll",
                };
                let _ = js.set_stage(job_id, s);
            };
            inner
                .virus_checker
                .check(&quarantine, Some(&token), on_stage)
                .await
        };
        inner.metrics.dec_scan();
        drop(permit);
        inner.scan_controllers.lock().unwrap().remove(job_id);

        if result.verdict != VirusVerdict::Inconclusive && result.verdict != VirusVerdict::Oversized
        {
            crate::vt_cache::store(&quarantine, result.verdict.as_str());
        }

        if result.message == "Cancelled by user" {
            js.set_stage(job_id, "done")?;
            js.cancel_job(job_id)?;
            return Ok(());
        }

        js.set_stage(job_id, "done")?;
        js.set_scan_result(
            job_id,
            &ScanResult {
                verdict: result.verdict.as_str().into(),
                message: result.message.clone(),
            },
        )?;

        if result.verdict == VirusVerdict::Clean {
            self.restore_clean(job_id, &quarantine, &original).await?;
        }
        Ok(())
    }

    async fn restore_clean(&self, job_id: &str, quarantine: &Path, original: &str) -> Result<()> {
        let inner = &self.0;
        let dest = inner
            .mover
            .resolve_restore_destination(&inner.watch_path, original)
            .await;
        inner
            .restoring_paths
            .lock()
            .unwrap()
            .insert(dest.clone(), std::time::Instant::now());
        // Restore to the exact path we marked so the watcher skips its own write.
        // restore_to_path normalizes mode (0o644) + clears the quarantine xattr.
        let restored = inner
            .mover
            .restore_to_path(&inner.watch_path, quarantine, dest)
            .await?;
        inner
            .job_store
            .set_restored(job_id, &restored.to_string_lossy())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_watcher(initial_mode: WatcherMode) -> Watcher {
        let store = Arc::new(JobStore::new(":memory:").unwrap());
        Watcher::new(
            "/tmp/watch-stub",
            vec![],
            "/tmp/quarantine-stub",
            "test-key",
            store,
            Metrics::new(),
            WatcherOptions {
                initial_mode,
                ..Default::default()
            },
        )
    }

    #[test]
    fn set_mode_aborts_controllers_when_leaving_active() {
        let w = make_watcher(WatcherMode::Active);
        let c1 = CancellationToken::new();
        let c2 = CancellationToken::new();
        w.0.scan_controllers
            .lock()
            .unwrap()
            .insert("a".into(), c1.clone());
        w.0.scan_controllers
            .lock()
            .unwrap()
            .insert("b".into(), c2.clone());

        w.set_mode(WatcherMode::ScanPaused);

        assert!(c1.is_cancelled());
        assert!(c2.is_cancelled());
    }

    #[test]
    fn set_mode_same_state_is_noop() {
        let w = make_watcher(WatcherMode::ScanPaused);
        let c = CancellationToken::new();
        w.0.scan_controllers
            .lock()
            .unwrap()
            .insert("a".into(), c.clone());
        w.set_mode(WatcherMode::ScanPaused);
        assert!(!c.is_cancelled());
    }

    #[test]
    fn restore_marker_suppresses_within_window_and_evicts_after() {
        let w = make_watcher(WatcherMode::Active);
        let p = PathBuf::from("/tmp/watch-stub/restored.bin");

        // A freshly restored path is suppressed so its own restore events
        // (content + chmod + xattr) never re-quarantine it.
        w.mark_restoring(p.clone());
        assert!(
            restore_suppressed(&w.0, &p),
            "freshly restored path must be suppressed"
        );

        // An unrelated path is never suppressed.
        assert!(!restore_suppressed(
            &w.0,
            Path::new("/tmp/watch-stub/other.bin")
        ));

        // Once the window elapses the marker no longer suppresses and is evicted.
        let stale = std::time::Instant::now()
            .checked_sub(RESTORE_SUPPRESS_WINDOW + Duration::from_secs(1))
            .expect("monotonic clock far enough past boot");
        w.0.restoring_paths.lock().unwrap().insert(p.clone(), stale);
        assert!(
            !restore_suppressed(&w.0, &p),
            "expired marker must not suppress"
        );
        assert!(
            !w.0.restoring_paths.lock().unwrap().contains_key(&p),
            "expired marker must be evicted"
        );
    }

    #[test]
    fn get_mode_reflects_set_mode() {
        let w = make_watcher(WatcherMode::Active);
        w.set_mode(WatcherMode::MonitoringDisabled);
        assert_eq!(w.get_mode(), WatcherMode::MonitoringDisabled);
    }

    #[test]
    fn cancel_aborts_and_removes_controller() {
        let w = make_watcher(WatcherMode::Active);
        let c = CancellationToken::new();
        w.0.scan_controllers
            .lock()
            .unwrap()
            .insert("job".into(), c.clone());
        w.cancel("job");
        assert!(c.is_cancelled());
        assert!(w.0.scan_controllers.lock().unwrap().is_empty());
    }

    #[test]
    fn on_mode_change_callback_fires() {
        let store = Arc::new(JobStore::new(":memory:").unwrap());
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag2 = flag.clone();
        let w = Watcher::new(
            "/tmp/w",
            vec![],
            "/tmp/q",
            "k",
            store,
            Metrics::new(),
            WatcherOptions {
                initial_mode: WatcherMode::Active,
                on_mode_change: Some(Box::new(move |_m| {
                    flag2.store(true, std::sync::atomic::Ordering::SeqCst);
                })),
                ..Default::default()
            },
        );
        w.set_mode(WatcherMode::ScanPaused);
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst));
    }
}
