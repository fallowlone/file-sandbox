//! Port of `src/launch-agent-monitor.ts`.
//!
//! Watches macOS LaunchAgent/LaunchDaemon directories for persistence changes
//! and emits an event per change. The menu bar polls these via
//! `/api/security-events`, so the serialized shape `{kind, path, at}` must stay
//! stable. Uses `notify` in place of chokidar.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchAgentEventKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchAgentEvent {
    pub kind: LaunchAgentEventKind,
    pub path: String,
    pub at: i64,
}

/// The three standard persistence directories, matching the TS `AGENT_PATHS`.
pub fn default_agent_paths() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    vec![
        Path::new(&home).join("Library/LaunchAgents"),
        PathBuf::from("/Library/LaunchAgents"),
        PathBuf::from("/Library/LaunchDaemons"),
    ]
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as i64
}

/// Map a notify event kind to a security event kind. Ignores events that are
/// neither create, modify, nor remove.
fn classify_event(kind: &EventKind) -> Option<LaunchAgentEventKind> {
    match kind {
        EventKind::Create(_) => Some(LaunchAgentEventKind::Added),
        EventKind::Modify(_) => Some(LaunchAgentEventKind::Modified),
        EventKind::Remove(_) => Some(LaunchAgentEventKind::Removed),
        _ => None,
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

fn log_kind(kind: LaunchAgentEventKind, path: &str) {
    match kind {
        LaunchAgentEventKind::Added => eprintln!("[SECURITY] New launch agent registered: {path}"),
        LaunchAgentEventKind::Modified => eprintln!("[SECURITY] Launch agent modified: {path}"),
        LaunchAgentEventKind::Removed => eprintln!("[SECURITY] Launch agent removed: {path}"),
    }
}

/// Start watching `paths` for persistence changes. The returned watcher must be
/// kept alive for monitoring to continue. `on_event` is called for every
/// add/modify/remove (hidden files are ignored, matching the TS dotfile filter).
pub fn start_launch_agent_monitor<F>(
    paths: Vec<PathBuf>,
    on_event: F,
) -> Result<RecommendedWatcher>
where
    F: Fn(LaunchAgentEvent) + Send + Sync + 'static,
{
    let on_event = Arc::new(on_event);
    let cb = on_event.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let event = match res {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[SECURITY] Launch agent monitor error: {e}");
                return;
            }
        };
        let Some(kind) = classify_event(&event.kind) else { return };
        for path in event.paths {
            if is_hidden(&path) {
                continue;
            }
            let path_str = path.to_string_lossy().into_owned();
            log_kind(kind, &path_str);
            cb(LaunchAgentEvent { kind, path: path_str, at: now_ms() });
        }
    })?;

    for dir in &paths {
        // Non-recursive (depth 0), matching the TS config. Missing dirs are
        // skipped rather than fatal.
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            eprintln!("[SECURITY] cannot watch {}: {e}", dir.display());
        }
    }

    let joined: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    eprintln!("[SECURITY] Watching launch agent dirs: {}", joined.join(", "));
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind};

    #[test]
    fn classify_maps_notify_kinds() {
        assert_eq!(
            classify_event(&EventKind::Create(CreateKind::File)),
            Some(LaunchAgentEventKind::Added)
        );
        assert_eq!(
            classify_event(&EventKind::Modify(ModifyKind::Any)),
            Some(LaunchAgentEventKind::Modified)
        );
        assert_eq!(
            classify_event(&EventKind::Remove(RemoveKind::File)),
            Some(LaunchAgentEventKind::Removed)
        );
        assert_eq!(classify_event(&EventKind::Access(notify::event::AccessKind::Any)), None);
    }

    #[test]
    fn event_serializes_to_menubar_shape() {
        let ev = LaunchAgentEvent {
            kind: LaunchAgentEventKind::Added,
            path: "/Library/LaunchAgents/evil.plist".into(),
            at: 1718000000000,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["kind"], "added");
        assert_eq!(json["path"], "/Library/LaunchAgents/evil.plist");
        assert_eq!(json["at"], 1718000000000i64);
    }

    #[test]
    fn hidden_files_detected() {
        assert!(is_hidden(Path::new("/Library/LaunchAgents/.DS_Store")));
        assert!(!is_hidden(Path::new("/Library/LaunchAgents/com.foo.plist")));
    }
}
