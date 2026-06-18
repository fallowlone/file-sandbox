//! Port of `src/inconclusive-sweeper.ts`.
//!
//! Hourly purge of inconclusive quarantine jobs older than `retention_days`.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::task::JoinHandle;

use crate::job_store::JobStore;

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as i64
}

/// One sweep pass: delete every inconclusive quarantine job older than
/// `cutoff_ms`. Returns the count successfully deleted. A failed delete is
/// logged and skipped, never aborting the pass.
pub async fn sweep_once<F, Fut>(
    job_store: &JobStore,
    cutoff_ms: i64,
    mut delete_quarantined: F,
) -> usize
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let rows = job_store
        .list_inconclusive_older_than(cutoff_ms)
        .unwrap_or_default();
    let mut deleted = 0;
    for row in rows {
        let id = row.id.clone();
        match delete_quarantined(row.id).await {
            Ok(()) => deleted += 1,
            Err(e) => eprintln!("Inconclusive sweeper skip {id}: {e}"),
        }
    }
    deleted
}

/// Start the hourly sweeper as a background task. Returns its `JoinHandle`; drop
/// or abort it to stop. A zero/negative retention disables the sweeper.
pub fn start_inconclusive_sweeper<F, Fut>(
    retention_days: i64,
    job_store: Arc<JobStore>,
    delete_quarantined: F,
) -> Option<JoinHandle<()>>
where
    F: Fn(String) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    if retention_days <= 0 {
        return None;
    }
    let max_age_ms = retention_days * DAY_MS;

    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(HOUR_MS as u64));
        loop {
            interval.tick().await;
            let cutoff = now_ms() - max_age_ms;
            let del = delete_quarantined.clone();
            sweep_once(&job_store, cutoff, del).await;
        }
    });
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_store::ScanResult;
    use std::cell::RefCell;

    #[tokio::test]
    async fn sweep_once_deletes_only_inconclusive_rows() {
        let store = JobStore::new(":memory:").unwrap();

        // Inconclusive → quarantine_kept + vt_verdict='inconclusive' → swept.
        store.insert_received("stale", "/s", "s").unwrap();
        store
            .set_scan_result(
                "stale",
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "?".into(),
                },
            )
            .unwrap();
        // Malicious → quarantine_kept but vt_verdict='malicious' → filtered out.
        store.insert_received("bad", "/b", "b").unwrap();
        store
            .set_scan_result(
                "bad",
                &ScanResult {
                    verdict: "malicious".into(),
                    message: "x".into(),
                },
            )
            .unwrap();

        // Future cutoff: every row's created_at precedes it, so only the
        // verdict/status filter decides what gets swept.
        let cutoff = now_ms() + 10_000;
        let deleted_ids = RefCell::new(Vec::new());
        let n = sweep_once(&store, cutoff, |id| {
            deleted_ids.borrow_mut().push(id.clone());
            async move { Ok(()) }
        })
        .await;

        assert_eq!(n, 1);
        assert_eq!(deleted_ids.borrow().as_slice(), &["stale".to_string()]);
    }

    #[tokio::test]
    async fn sweep_once_skips_rows_when_delete_fails() {
        let store = JobStore::new(":memory:").unwrap();
        store.insert_received("stale", "/s", "s").unwrap();
        store
            .set_scan_result(
                "stale",
                &ScanResult {
                    verdict: "inconclusive".into(),
                    message: "?".into(),
                },
            )
            .unwrap();

        let cutoff = now_ms() + 10_000;
        let n = sweep_once(&store, cutoff, |_id| async move {
            Err(anyhow::anyhow!("delete blew up"))
        })
        .await;

        assert_eq!(n, 0, "failed deletes are not counted");
    }
}
