//! Port of `src/job-store.ts`. SQLite-backed job store on rusqlite.
//!
//! Schema is reproduced verbatim from the TS version: the base `jobs` table is
//! created WITHOUT `pompelmi_verdict` / `scan_stage`, then two idempotent
//! `ALTER TABLE ADD COLUMN` migrations append them. This keeps a Rust-created
//! database byte-identical to a TS-created one and lets the daemon open an
//! existing `jobs.sqlite` written by the Node version.
//!
//! Status / verdict / stage values are kept as `Option<String>` because the DB
//! columns are plain `TEXT` and the TS layer performs no runtime validation —
//! storing strings round-trips any value a TS-written database may hold.

use std::fmt;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Raised when `set_deleted` is called on a job not in `quarantine_kept`.
/// Mirrors the TS `JobConflictError`.
#[derive(Debug)]
pub struct JobConflictError(pub String);

impl fmt::Display for JobConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for JobConflictError {}

/// One row of the `jobs` table. Field names match the SQLite columns (snake_case),
/// identical to the TS `JobRow` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRow {
    pub id: String,
    pub source_path: String,
    pub original_name: String,
    pub quarantine_path: Option<String>,
    pub final_path: Option<String>,
    pub status: String,
    pub vt_verdict: Option<String>,
    pub pompelmi_verdict: Option<String>,
    pub scan_stage: Option<String>,
    pub detail: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Minimal mirror of the TS `VirusCheckResult` consumed by `set_scan_result`.
pub struct ScanResult {
    pub verdict: String,
    pub message: String,
}

/// Result of `clear_all`: settled rows removed vs active rows kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearResult {
    pub deleted: usize,
    pub skipped: i64,
}

/// Columns selected by `list_recent` / `get_job` / `list_inconclusive_older_than`,
/// in the order the row mapper expects.
const SELECT_COLUMNS: &str = "id, source_path, original_name, quarantine_path, final_path, \
     status, vt_verdict, pompelmi_verdict, scan_stage, detail, created_at, updated_at";

pub struct JobStore {
    db: Mutex<Connection>,
}

impl JobStore {
    /// Open (or create) the store at `db_path`. Pass `":memory:"` for an
    /// in-memory database, matching the TS test helper.
    pub fn new(db_path: &str) -> Result<Self> {
        let db = if db_path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = Path::new(db_path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            Connection::open(db_path)?
        };

        db.pragma_update(None, "journal_mode", "WAL")?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY NOT NULL,
                source_path TEXT NOT NULL,
                original_name TEXT NOT NULL,
                quarantine_path TEXT,
                final_path TEXT,
                status TEXT NOT NULL,
                vt_verdict TEXT,
                detail TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_created ON jobs (created_at DESC);",
        )?;

        // Idempotent column adds, mirroring the TS try/catch on "duplicate column name".
        add_column_if_missing(&db, "ALTER TABLE jobs ADD COLUMN pompelmi_verdict TEXT")?;
        add_column_if_missing(&db, "ALTER TABLE jobs ADD COLUMN scan_stage TEXT")?;

        Ok(Self { db: Mutex::new(db) })
    }

    pub fn insert_received(
        &self,
        job_id: &str,
        source_path: &str,
        original_name: &str,
    ) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "INSERT INTO jobs (id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, detail, created_at, updated_at)
             VALUES (?, ?, ?, NULL, NULL, 'received', NULL, NULL, NULL, ?, ?)",
            params![job_id, source_path, original_name, now, now],
        )?;
        Ok(())
    }

    pub fn set_in_quarantine(&self, job_id: &str, quarantine_path: &str) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET quarantine_path = ?, status = 'in_quarantine', updated_at = ? WHERE id = ?",
            params![quarantine_path, now, job_id],
        )?;
        Ok(())
    }

    pub fn set_scanning(&self, job_id: &str) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET status = 'scanning', updated_at = ? WHERE id = ?",
            params![now, job_id],
        )?;
        Ok(())
    }

    pub fn set_scan_result(&self, job_id: &str, result: &ScanResult) -> Result<()> {
        let now = now_ms();
        if result.verdict == "clean" {
            self.db.lock().expect("job store mutex poisoned").execute(
                "UPDATE jobs SET vt_verdict = ?, detail = ?, updated_at = ? WHERE id = ?",
                params![result.verdict, result.message, now, job_id],
            )?;
        } else {
            self.db.lock().expect("job store mutex poisoned").execute(
                "UPDATE jobs SET vt_verdict = ?, detail = ?, status = 'quarantine_kept', updated_at = ? WHERE id = ?",
                params![result.verdict, result.message, now, job_id],
            )?;
        }
        Ok(())
    }

    pub fn set_pompelmi_verdict(
        &self,
        job_id: &str,
        verdict: &str,
        detail: Option<&str>,
    ) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET pompelmi_verdict = ?, detail = COALESCE(?, detail), updated_at = ? WHERE id = ?",
            params![verdict, detail, now, job_id],
        )?;
        Ok(())
    }

    pub fn set_stage(&self, job_id: &str, stage: &str) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET scan_stage = ?, updated_at = ? WHERE id = ?",
            params![stage, now, job_id],
        )?;
        Ok(())
    }

    pub fn set_restored(&self, job_id: &str, final_path: &str) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET final_path = ?, status = 'restored', updated_at = ? WHERE id = ?",
            params![final_path, now, job_id],
        )?;
        Ok(())
    }

    pub fn fail(&self, job_id: &str, message: &str) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET status = 'failed', detail = ?, updated_at = ? WHERE id = ?",
            params![message, now, job_id],
        )?;
        Ok(())
    }

    pub fn list_recent(&self, limit: i64) -> Result<Vec<JobRow>> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM jobs ORDER BY created_at DESC LIMIT ?");
        let db = self.db.lock().expect("job store mutex poisoned");
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt
            .query_map(params![limit], map_job_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<JobRow>> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM jobs WHERE id = ?");
        let row = self
            .db
            .lock()
            .expect("job store mutex poisoned")
            .query_row(&sql, params![job_id], map_job_row)
            .optional()?;
        Ok(row)
    }

    pub fn set_deleted(&self, job_id: &str, detail: &str) -> Result<()> {
        let now = now_ms();
        let changes = self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET status = 'deleted', detail = ?, updated_at = ? WHERE id = ? AND status = 'quarantine_kept'",
            params![detail, now, job_id],
        )?;
        if changes == 0 {
            return Err(JobConflictError(format!(
                "Job {job_id} cannot be deleted: not in quarantine_kept status (may have been deleted or processed)"
            ))
            .into());
        }
        Ok(())
    }

    /// Inconclusive rows kept in quarantine with `created_at` before `cutoff_ms`.
    pub fn list_inconclusive_older_than(&self, cutoff_ms: i64) -> Result<Vec<JobRow>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM jobs \
             WHERE status = 'quarantine_kept' AND vt_verdict = 'inconclusive' AND created_at < ?"
        );
        let db = self.db.lock().expect("job store mutex poisoned");
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt
            .query_map(params![cutoff_ms], map_job_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<()> {
        let now = now_ms();
        self.db.lock().expect("job store mutex poisoned").execute(
            "UPDATE jobs SET status = 'cancelled', detail = 'Cancelled by user', updated_at = ? WHERE id = ? AND status = 'scanning'",
            params![now, job_id],
        )?;
        Ok(())
    }

    /// Delete settled rows. Active rows (`received`, `in_quarantine`, `scanning`)
    /// are kept so in-flight pipeline stages retain their row reference.
    pub fn clear_all(&self) -> Result<ClearResult> {
        let skipped: i64 = self
            .db
            .lock()
            .expect("job store mutex poisoned")
            .query_row(
            "SELECT COUNT(*) FROM jobs WHERE status IN ('received', 'in_quarantine', 'scanning')",
            [],
            |row| row.get(0),
        )?;
        let deleted = self.db.lock().expect("job store mutex poisoned").execute(
            "DELETE FROM jobs WHERE status NOT IN ('received', 'in_quarantine', 'scanning')",
            [],
        )?;
        Ok(ClearResult { deleted, skipped })
    }
}

fn add_column_if_missing(db: &Connection, sql: &str) -> Result<()> {
    match db.execute(sql, []) {
        Ok(_) => Ok(()),
        Err(e) if e.to_string().contains("duplicate column name") => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn map_job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRow> {
    Ok(JobRow {
        id: row.get(0)?,
        source_path: row.get(1)?,
        original_name: row.get(2)?,
        quarantine_path: row.get(3)?,
        final_path: row.get(4)?,
        status: row.get(5)?,
        vt_verdict: row.get(6)?,
        pompelmi_verdict: row.get(7)?,
        scan_stage: row.get(8)?,
        detail: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
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

    fn fresh_store() -> JobStore {
        JobStore::new(":memory:").expect("open in-memory store")
    }

    #[test]
    fn pompelmi_verdict_starts_null_and_persists() {
        let store = fresh_store();
        store.insert_received("job-1", "/a.bin", "a.bin").unwrap();
        assert_eq!(
            store.get_job("job-1").unwrap().unwrap().pompelmi_verdict,
            None
        );

        store
            .set_pompelmi_verdict("job-1", "clean", Some("ok"))
            .unwrap();
        let after = store.get_job("job-1").unwrap().unwrap();
        assert_eq!(after.pompelmi_verdict.as_deref(), Some("clean"));
        assert_eq!(after.detail.as_deref(), Some("ok"));
    }

    #[test]
    fn scan_stage_starts_null_and_persists() {
        let store = fresh_store();
        store
            .insert_received("job-stage-1", "/a.bin", "a.bin")
            .unwrap();
        assert_eq!(
            store.get_job("job-stage-1").unwrap().unwrap().scan_stage,
            None
        );

        store.set_stage("job-stage-1", "cache_check").unwrap();
        assert_eq!(
            store
                .get_job("job-stage-1")
                .unwrap()
                .unwrap()
                .scan_stage
                .as_deref(),
            Some("cache_check")
        );

        store.set_stage("job-stage-1", "done").unwrap();
        assert_eq!(
            store
                .get_job("job-stage-1")
                .unwrap()
                .unwrap()
                .scan_stage
                .as_deref(),
            Some("done")
        );
    }

    #[test]
    fn insert_received_sets_received_status_and_timestamps() {
        let store = fresh_store();
        store
            .insert_received("j", "/src/file.bin", "file.bin")
            .unwrap();
        let row = store.get_job("j").unwrap().unwrap();
        assert_eq!(row.status, "received");
        assert_eq!(row.source_path, "/src/file.bin");
        assert_eq!(row.original_name, "file.bin");
        assert_eq!(row.quarantine_path, None);
        assert_eq!(row.final_path, None);
        assert_eq!(row.vt_verdict, None);
        assert!(row.created_at > 0);
        assert_eq!(row.created_at, row.updated_at);
    }

    #[test]
    fn scan_result_clean_leaves_status_but_malicious_quarantines() {
        let store = fresh_store();
        store.insert_received("clean-job", "/c", "c").unwrap();
        store.set_scanning("clean-job").unwrap();
        store
            .set_scan_result(
                "clean-job",
                &ScanResult {
                    verdict: "clean".into(),
                    message: "ok".into(),
                },
            )
            .unwrap();
        let clean = store.get_job("clean-job").unwrap().unwrap();
        assert_eq!(clean.vt_verdict.as_deref(), Some("clean"));
        assert_eq!(clean.status, "scanning"); // status untouched on clean

        store.insert_received("bad-job", "/b", "b").unwrap();
        store
            .set_scan_result(
                "bad-job",
                &ScanResult {
                    verdict: "malicious".into(),
                    message: "EICAR".into(),
                },
            )
            .unwrap();
        let bad = store.get_job("bad-job").unwrap().unwrap();
        assert_eq!(bad.vt_verdict.as_deref(), Some("malicious"));
        assert_eq!(bad.status, "quarantine_kept");
    }

    #[test]
    fn set_deleted_requires_quarantine_kept() {
        let store = fresh_store();
        store.insert_received("j", "/a", "a").unwrap();

        // Not in quarantine_kept → JobConflictError.
        let err = store.set_deleted("j", "Deleted by user").unwrap_err();
        assert!(err.downcast_ref::<JobConflictError>().is_some());

        store
            .set_scan_result(
                "j",
                &ScanResult {
                    verdict: "malicious".into(),
                    message: "x".into(),
                },
            )
            .unwrap();
        store.set_deleted("j", "Deleted by user").unwrap();
        assert_eq!(store.get_job("j").unwrap().unwrap().status, "deleted");
    }

    #[test]
    fn list_recent_orders_by_created_at_desc_and_respects_limit() {
        let store = fresh_store();
        for i in 0..3 {
            let id = format!("job-{i}");
            store.insert_received(&id, "/p", "p").unwrap();
            // Force distinct created_at ordering regardless of clock resolution.
            store
                .db
                .lock()
                .expect("job store mutex poisoned")
                .execute(
                    "UPDATE jobs SET created_at = ? WHERE id = ?",
                    params![1000 + i as i64, id],
                )
                .unwrap();
        }
        let rows = store.list_recent(2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "job-2");
        assert_eq!(rows[1].id, "job-1");
    }

    #[test]
    fn clear_all_keeps_active_rows() {
        let store = fresh_store();
        // active
        store.insert_received("active-1", "/a", "a").unwrap();
        store.set_scanning("active-1").unwrap();
        store.insert_received("active-2", "/a", "a").unwrap(); // status 'received'
                                                               // settled
        store.insert_received("done-1", "/d", "d").unwrap();
        store
            .set_scan_result(
                "done-1",
                &ScanResult {
                    verdict: "malicious".into(),
                    message: "x".into(),
                },
            )
            .unwrap(); // quarantine_kept

        let res = store.clear_all().unwrap();
        assert_eq!(res.deleted, 1);
        assert_eq!(res.skipped, 2);
        assert!(store.get_job("done-1").unwrap().is_none());
        assert!(store.get_job("active-1").unwrap().is_some());
        assert!(store.get_job("active-2").unwrap().is_some());
    }

    #[test]
    fn list_inconclusive_older_than_filters_by_verdict_status_and_age() {
        let store = fresh_store();
        store.insert_received("old", "/o", "o").unwrap();
        store
            .set_scan_result(
                "old",
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "?".into(),
                },
            )
            .unwrap();
        store
            .db
            .lock()
            .expect("job store mutex poisoned")
            .execute("UPDATE jobs SET created_at = 500 WHERE id = 'old'", [])
            .unwrap();

        store.insert_received("new", "/n", "n").unwrap();
        store
            .set_scan_result(
                "new",
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "?".into(),
                },
            )
            .unwrap();
        store
            .db
            .lock()
            .expect("job store mutex poisoned")
            .execute("UPDATE jobs SET created_at = 5000 WHERE id = 'new'", [])
            .unwrap();

        let stale = store.list_inconclusive_older_than(1000).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, "old");
    }
}
