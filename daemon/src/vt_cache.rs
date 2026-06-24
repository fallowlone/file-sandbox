//! In-process port of the `vt-cache` crate + `src/vt-cache.ts` bridge.
//!
//! A SHA-256 → verdict cache so a file already scanned is not re-uploaded to
//! VirusTotal. The TS daemon shelled out to a separate `vt-cache` binary; the
//! Rust daemon embeds the same logic. The DB path, schema, and verdict strings
//! are reproduced verbatim so an existing `vt-cache.db` keeps working.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

/// Resolve the cache DB path: `$VT_CACHE_DB` or `$HOME/.config/filesandbox/vt-cache.db`.
pub fn db_path() -> String {
    if let Ok(p) = std::env::var("VT_CACHE_DB") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/.config/filesandbox/vt-cache.db")
}

fn open_db(path: &str) -> Result<Connection> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }
    let conn = Connection::open(path).with_context(|| format!("open cache DB {path}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cache (
            sha256     TEXT PRIMARY KEY,
            verdict    TEXT NOT NULL,
            cached_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_sha256 ON cache (sha256);",
    )
    .context("init cache schema")?;
    Ok(conn)
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("hash {}", path.display()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Look up a cached verdict for the file's SHA-256. Returns `None` on miss or
/// any failure (hash error, DB error) — a cache lookup never blocks scanning.
pub fn check(file_path: &Path) -> Option<String> {
    check_in(&db_path(), file_path)
}

/// Persist a verdict for the file's SHA-256. Fire-and-forget: errors are
/// swallowed so caching never aborts the pipeline.
pub fn store(file_path: &Path, verdict: &str) {
    let _ = store_in(&db_path(), file_path, verdict);
}

/// Testable core of [`check`] against an explicit DB path.
pub fn check_in(db: &str, file_path: &Path) -> Option<String> {
    let sha = hash_file(file_path).ok()?;
    let conn = open_db(db).ok()?;
    conn.query_row(
        "SELECT verdict FROM cache WHERE sha256 = ?1",
        params![sha],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// Testable core of [`store`] against an explicit DB path.
pub fn store_in(db: &str, file_path: &Path, verdict: &str) -> Result<()> {
    let sha = hash_file(file_path)?;
    let conn = open_db(db)?;
    conn.execute(
        "INSERT OR REPLACE INTO cache (sha256, verdict, cached_at) VALUES (?1, ?2, ?3)",
        params![sha, verdict, now_secs()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_then_check_round_trips_verdict() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cache.db");
        let db = db.to_string_lossy().to_string();
        let file = dir.path().join("a.bin");
        std::fs::write(&file, b"content").unwrap();

        assert_eq!(check_in(&db, &file), None, "cold cache misses");

        store_in(&db, &file, "clean").unwrap();
        assert_eq!(check_in(&db, &file).as_deref(), Some("clean"));
    }

    #[test]
    fn identical_content_shares_cache_entry() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cache.db").to_string_lossy().to_string();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::write(&a, b"same bytes").unwrap();
        std::fs::write(&b, b"same bytes").unwrap();

        store_in(&db, &a, "infected").unwrap();
        // Same SHA-256 → cache hit for the differently-named file.
        assert_eq!(check_in(&db, &b).as_deref(), Some("infected"));
    }

    #[test]
    fn check_misses_on_unreadable_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cache.db").to_string_lossy().to_string();
        assert_eq!(check_in(&db, Path::new("/no/such/file/xyz")), None);
    }
}
