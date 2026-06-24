//! Port of `src/file-mover.ts`.
//!
//! Moves files into quarantine under a unique name and restores them back to
//! the watch folder, mirroring the TS collision-avoidance rules.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use uuid::Uuid;

pub struct QuarantineMoveResult {
    pub quarantine_file_path: PathBuf,
    pub original_base_name: String,
}

#[derive(Clone)]
pub struct FileMover {
    destination: PathBuf,
}

impl FileMover {
    pub fn new(destination: impl Into<PathBuf>) -> Self {
        Self {
            destination: destination.into(),
        }
    }

    /// Copy `source` into quarantine under a unique `{uuid}_{basename}` name,
    /// then remove the original.
    pub async fn move_in(&self, source: &Path) -> Result<QuarantineMoveResult> {
        let original_base_name = base_name(source);
        let quarantine_name = format!("{}_{}", Uuid::new_v4(), original_base_name);
        let quarantine_file_path = self.destination.join(&quarantine_name);

        // Never quarantine through a symlink: if the watched path was swapped for a
        // symlink after the watcher's check, refuse rather than copy an arbitrary
        // file the link points at into quarantine (and onward to a scanner).
        let meta = tokio::fs::symlink_metadata(source)
            .await
            .with_context(|| format!("stat {} before quarantine", source.display()))?;
        if meta.file_type().is_symlink() {
            bail!("refusing to quarantine symlink {}", source.display());
        }

        self.ensure_directory().await?;
        tokio::fs::copy(source, &quarantine_file_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to move {} to {}",
                    source.display(),
                    self.destination.display()
                )
            })?;
        tokio::fs::remove_file(source).await.with_context(|| {
            format!(
                "Failed to move {} to {}",
                source.display(),
                self.destination.display()
            )
        })?;

        Ok(QuarantineMoveResult {
            quarantine_file_path,
            original_base_name,
        })
    }

    /// Copy a quarantined file back to the watch folder under its original
    /// basename (or a `_restored_*` fallback if the target exists), then remove
    /// it from quarantine. Resolves the destination once and delegates to
    /// [`restore_to_path`]; callers that must mark the exact restored path for the
    /// watcher should resolve via [`resolve_restore_destination`] and call
    /// [`restore_to_path`] directly so the marked and written paths cannot diverge.
    pub async fn restore_to_watch(
        &self,
        watch_path: &Path,
        quarantine_file_path: &Path,
        original_base_name: &str,
    ) -> Result<PathBuf> {
        let dest = self
            .resolve_restore_destination(watch_path, original_base_name)
            .await;
        self.restore_to_path(watch_path, quarantine_file_path, dest)
            .await
    }

    /// Restore a quarantined file to an already-resolved destination. The target
    /// is created with `O_EXCL` so a file that appeared in the resolve→write
    /// window is never clobbered (on collision a uuid-suffixed sibling is used and
    /// the caller's restore marker may miss, causing a harmless re-scan). The
    /// restored file is normalized to a usable mode (0o644) with its quarantine
    /// xattr cleared, and the destination is asserted to stay inside `watch_path`.
    pub async fn restore_to_path(
        &self,
        watch_path: &Path,
        quarantine_file_path: &Path,
        preferred_dest: PathBuf,
    ) -> Result<PathBuf> {
        if !preferred_dest.starts_with(watch_path) {
            bail!(
                "refusing restore: resolved path {} escapes watch folder {}",
                preferred_dest.display(),
                watch_path.display()
            );
        }

        let dest = match create_new_exclusive(&preferred_dest).await {
            Ok(()) => preferred_dest,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let suffix = Uuid::new_v4().simple().to_string();
                let alt = sibling_with_suffix(&preferred_dest, &suffix[..8]);
                create_new_exclusive(&alt)
                    .await
                    .with_context(|| format!("create restore target {}", alt.display()))?;
                eprintln!(
                    "[restore] {} already exists; restored to {} instead (it may be re-scanned)",
                    preferred_dest.display(),
                    alt.display()
                );
                alt
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("create restore target {}", preferred_dest.display()))
            }
        };

        tokio::fs::copy(quarantine_file_path, &dest)
            .await
            .with_context(|| {
                format!(
                    "Failed to restore {} to {}",
                    quarantine_file_path.display(),
                    dest.display()
                )
            })?;
        tokio::fs::remove_file(quarantine_file_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to restore {} to {}",
                    quarantine_file_path.display(),
                    dest.display()
                )
            })?;

        normalize_restored(&dest).await;
        Ok(dest)
    }

    /// Resolve a non-clobbering restore path: prefer the original name, then a
    /// timestamped `_restored_` variant, finally one with a short uuid suffix.
    pub async fn resolve_restore_destination(
        &self,
        watch_path: &Path,
        original_base_name: &str,
    ) -> PathBuf {
        // Defense in depth against a crafted/legacy original_name: strip it to a
        // single path component so it can never contain `/` or `..` that would let
        // a restore escape the watch folder. `restore_to_path` re-asserts this.
        let safe = Path::new(original_base_name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| n != "." && n != "..")
            .unwrap_or_else(|| "restored_file".to_string());

        let primary = watch_path.join(&safe);
        if !exists(&primary).await {
            return primary;
        }

        let (name, ext) = split_name_ext(&safe);
        let fallback = watch_path.join(format!("{name}_restored_{}{ext}", now_ms()));
        if !exists(&fallback).await {
            return fallback;
        }

        let suffix = &Uuid::new_v4().simple().to_string()[..8];
        watch_path.join(format!("{name}_restored_{}_{suffix}{ext}", now_ms()))
    }

    pub async fn delete_file(&self, quarantine_file_path: &Path) -> Result<()> {
        // Idempotent: an already-absent file counts as deleted so the job record
        // can still be cleaned up (otherwise the UI gets a 400 on a missing file).
        match tokio::fs::remove_file(quarantine_file_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e)
                .with_context(|| format!("Failed to delete {}", quarantine_file_path.display())),
        }
    }

    pub async fn ensure_directory(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.destination)
            .await
            .with_context(|| {
                format!("Failed to create directory {}", self.destination.display())
            })?;
        Ok(())
    }
}

async fn exists(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Split a basename into (stem, extension) matching Node's `path.parse`:
/// a leading dot is not treated as an extension separator.
fn split_name_ext(base: &str) -> (String, String) {
    match base.rfind('.') {
        Some(i) if i > 0 => (base[..i].to_string(), base[i..].to_string()),
        _ => (base.to_string(), String::new()),
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as i64
}

/// Create `path` exclusively (`O_CREAT | O_EXCL`). Returns `AlreadyExists` if the
/// path is taken, so a restore can detect a collision instead of clobbering an
/// unrelated file that appeared in the resolve→write window.
async fn create_new_exclusive(path: &Path) -> std::io::Result<()> {
    tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .map(|_| ())
}

/// `foo.pdf` + `ab12cd34` → sibling `foo_ab12cd34.pdf` in the same directory.
fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = split_name_ext(&name);
    parent.join(format!("{stem}_{suffix}{ext}"))
}

/// Make a restored file usable and Gatekeeper-clean: a quarantine copy is 0o444
/// and carries `com.apple.quarantine`, both of which `fs::copy` propagates. Reset
/// the mode to 0o644 so the owner can write it, and clear the quarantine xattr so
/// a restored clean file is no longer treated as freshly downloaded.
async fn normalize_restored(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).await
        {
            eprintln!("[restore] failed to chmod 0o644 {}: {e}", path.display());
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = tokio::process::Command::new("xattr")
            .arg("-c")
            .arg(path)
            .status()
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn write_file(path: &Path, contents: &[u8]) {
        tokio::fs::write(path, contents).await.unwrap();
    }

    #[tokio::test]
    async fn move_in_copies_with_uuid_prefix_and_removes_source() {
        let dir = tempdir().unwrap();
        let quarantine = dir.path().join("quarantine");
        let source = dir.path().join("evil.bin");
        write_file(&source, b"payload").await;

        let mover = FileMover::new(&quarantine);
        let res = mover.move_in(&source).await.unwrap();

        assert_eq!(res.original_base_name, "evil.bin");
        assert!(res.quarantine_file_path.starts_with(&quarantine));
        assert!(res
            .quarantine_file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("_evil.bin"));
        assert_eq!(
            tokio::fs::read(&res.quarantine_file_path).await.unwrap(),
            b"payload"
        );
        assert!(!exists(&source).await, "source must be removed");
    }

    #[tokio::test]
    async fn restore_uses_original_name_when_free() {
        let dir = tempdir().unwrap();
        let watch = dir.path().join("watch");
        tokio::fs::create_dir_all(&watch).await.unwrap();
        let quarantined = dir.path().join("q_file.txt");
        write_file(&quarantined, b"data").await;

        let mover = FileMover::new(dir.path().join("quarantine"));
        let restored = mover
            .restore_to_watch(&watch, &quarantined, "file.txt")
            .await
            .unwrap();

        assert_eq!(restored, watch.join("file.txt"));
        assert_eq!(tokio::fs::read(&restored).await.unwrap(), b"data");
        assert!(
            !exists(&quarantined).await,
            "quarantine copy must be removed"
        );
    }

    #[tokio::test]
    async fn restore_falls_back_when_original_exists() {
        let dir = tempdir().unwrap();
        let watch = dir.path().join("watch");
        tokio::fs::create_dir_all(&watch).await.unwrap();
        // Occupy the primary destination.
        write_file(&watch.join("doc.pdf"), b"existing").await;

        let quarantined = dir.path().join("q_doc.pdf");
        write_file(&quarantined, b"restored").await;

        let mover = FileMover::new(dir.path().join("quarantine"));
        let restored = mover
            .restore_to_watch(&watch, &quarantined, "doc.pdf")
            .await
            .unwrap();

        assert_ne!(restored, watch.join("doc.pdf"));
        let name = restored.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("doc_restored_"), "got {name}");
        assert!(name.ends_with(".pdf"), "got {name}");
        assert_eq!(tokio::fs::read(&restored).await.unwrap(), b"restored");
    }

    #[tokio::test]
    async fn delete_file_removes_quarantined() {
        let dir = tempdir().unwrap();
        let q = dir.path().join("q.bin");
        write_file(&q, b"x").await;
        let mover = FileMover::new(dir.path().join("quarantine"));
        mover.delete_file(&q).await.unwrap();
        assert!(!exists(&q).await);
    }

    #[tokio::test]
    async fn delete_file_is_idempotent_when_missing() {
        let dir = tempdir().unwrap();
        let mover = FileMover::new(dir.path().join("quarantine"));
        // Deleting a file that was already removed must succeed, not error.
        mover
            .delete_file(&dir.path().join("ghost.bin"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn resolve_restore_strips_path_traversal_in_original_name() {
        let dir = tempdir().unwrap();
        let watch = dir.path().join("watch");
        tokio::fs::create_dir_all(&watch).await.unwrap();
        let mover = FileMover::new(dir.path().join("quarantine"));

        // A crafted/legacy original_name must not escape the watch folder.
        let resolved = mover
            .resolve_restore_destination(&watch, "../../etc/cron.d/evil")
            .await;
        assert!(
            resolved.starts_with(&watch),
            "resolved {resolved:?} escaped {watch:?}"
        );
        assert_eq!(resolved.file_name().unwrap(), "evil");
    }

    #[tokio::test]
    async fn restore_to_path_rejects_destination_outside_watch() {
        let dir = tempdir().unwrap();
        let watch = dir.path().join("watch");
        tokio::fs::create_dir_all(&watch).await.unwrap();
        let quarantined = dir.path().join("q.bin");
        write_file(&quarantined, b"x").await;
        let mover = FileMover::new(dir.path().join("quarantine"));

        let escape = dir.path().join("outside.bin"); // sibling of watch, not under it
        let err = mover
            .restore_to_path(&watch, &quarantined, escape)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("escapes watch folder"), "{err}");
        // Source must remain untouched on refusal.
        assert!(exists(&quarantined).await);
    }

    #[tokio::test]
    async fn restore_to_path_does_not_clobber_existing_collision() {
        let dir = tempdir().unwrap();
        let watch = dir.path().join("watch");
        tokio::fs::create_dir_all(&watch).await.unwrap();
        let victim = watch.join("doc.pdf");
        write_file(&victim, b"precious existing data").await;

        let quarantined = dir.path().join("q.pdf");
        write_file(&quarantined, b"restored").await;
        let mover = FileMover::new(dir.path().join("quarantine"));

        // Force the pre-resolved path to collide with the existing victim file.
        let restored = mover
            .restore_to_path(&watch, &quarantined, victim.clone())
            .await
            .unwrap();
        assert_ne!(restored, victim, "must not clobber the existing file");
        assert_eq!(
            tokio::fs::read(&victim).await.unwrap(),
            b"precious existing data",
            "existing file content must be preserved"
        );
        assert_eq!(tokio::fs::read(&restored).await.unwrap(), b"restored");
    }

    #[test]
    fn split_name_ext_matches_node_path_parse() {
        assert_eq!(split_name_ext("a.bin"), ("a".into(), ".bin".into()));
        assert_eq!(
            split_name_ext("archive.tar.gz"),
            ("archive.tar".into(), ".gz".into())
        );
        assert_eq!(split_name_ext("noext"), ("noext".into(), "".into()));
        assert_eq!(split_name_ext(".bashrc"), (".bashrc".into(), "".into()));
    }
}
