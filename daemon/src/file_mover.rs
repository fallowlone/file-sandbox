//! Port of `src/file-mover.ts`.
//!
//! Moves files into quarantine under a unique name and restores them back to
//! the watch folder, mirroring the TS collision-avoidance rules.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
        Self { destination: destination.into() }
    }

    /// Copy `source` into quarantine under a unique `{uuid}_{basename}` name,
    /// then remove the original.
    pub async fn move_in(&self, source: &Path) -> Result<QuarantineMoveResult> {
        let original_base_name = base_name(source);
        let quarantine_name = format!("{}_{}", Uuid::new_v4(), original_base_name);
        let quarantine_file_path = self.destination.join(&quarantine_name);

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

        Ok(QuarantineMoveResult { quarantine_file_path, original_base_name })
    }

    /// Copy a quarantined file back to the watch folder under its original
    /// basename (or a `_restored_*` fallback if the target exists), then remove
    /// it from quarantine.
    pub async fn restore_to_watch(
        &self,
        watch_path: &Path,
        quarantine_file_path: &Path,
        original_base_name: &str,
    ) -> Result<PathBuf> {
        let restored_path =
            self.resolve_restore_destination(watch_path, original_base_name).await;

        tokio::fs::copy(quarantine_file_path, &restored_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to restore {} to {}",
                    quarantine_file_path.display(),
                    restored_path.display()
                )
            })?;
        tokio::fs::remove_file(quarantine_file_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to restore {} to {}",
                    quarantine_file_path.display(),
                    restored_path.display()
                )
            })?;

        Ok(restored_path)
    }

    /// Resolve a non-clobbering restore path: prefer the original name, then a
    /// timestamped `_restored_` variant, finally one with a short uuid suffix.
    pub async fn resolve_restore_destination(
        &self,
        watch_path: &Path,
        original_base_name: &str,
    ) -> PathBuf {
        let primary = watch_path.join(original_base_name);
        if !exists(&primary).await {
            return primary;
        }

        let (name, ext) = split_name_ext(original_base_name);
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
        tokio::fs::create_dir_all(&self.destination).await.with_context(|| {
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
        assert!(res.quarantine_file_path.file_name().unwrap().to_string_lossy().ends_with("_evil.bin"));
        assert_eq!(tokio::fs::read(&res.quarantine_file_path).await.unwrap(), b"payload");
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
        let restored = mover.restore_to_watch(&watch, &quarantined, "file.txt").await.unwrap();

        assert_eq!(restored, watch.join("file.txt"));
        assert_eq!(tokio::fs::read(&restored).await.unwrap(), b"data");
        assert!(!exists(&quarantined).await, "quarantine copy must be removed");
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
        let restored = mover.restore_to_watch(&watch, &quarantined, "doc.pdf").await.unwrap();

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
        mover.delete_file(&dir.path().join("ghost.bin")).await.unwrap();
    }

    #[test]
    fn split_name_ext_matches_node_path_parse() {
        assert_eq!(split_name_ext("a.bin"), ("a".into(), ".bin".into()));
        assert_eq!(split_name_ext("archive.tar.gz"), ("archive.tar".into(), ".gz".into()));
        assert_eq!(split_name_ext("noext"), ("noext".into(), "".into()));
        assert_eq!(split_name_ext(".bashrc"), (".bashrc".into(), "".into()));
    }
}
