//! Port of `src/metrics.ts`.
//!
//! Process-wide counters shared between the watcher (which increments around a
//! VT scan) and the HTTP UI (which reports them). The TS module was a mutable
//! singleton; here it is an `Arc<Metrics>` with atomics so it can be shared
//! across tasks.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

pub struct Metrics {
    pub started_at_ms: i64,
    active_scans: AtomicI64,
    last_error: Mutex<Option<String>>,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started_at_ms: now_ms(),
            active_scans: AtomicI64::new(0),
            last_error: Mutex::new(None),
        })
    }

    pub fn inc_scan(&self) {
        self.active_scans.fetch_add(1, Ordering::SeqCst);
    }

    pub fn dec_scan(&self) {
        // Clamp at zero, matching the TS `Math.max(0, activeScans - 1)`.
        let _ = self
            .active_scans
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some((v - 1).max(0)));
    }

    pub fn active_scans(&self) -> i64 {
        self.active_scans.load(Ordering::SeqCst)
    }

    pub fn set_last_error(&self, msg: Option<String>) {
        *self.last_error.lock().expect("metrics last_error poisoned") = msg;
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .expect("metrics last_error poisoned")
            .clone()
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

    #[test]
    fn inc_dec_tracks_active_scans_and_clamps_at_zero() {
        let m = Metrics::new();
        assert_eq!(m.active_scans(), 0);
        m.inc_scan();
        m.inc_scan();
        assert_eq!(m.active_scans(), 2);
        m.dec_scan();
        assert_eq!(m.active_scans(), 1);
        m.dec_scan();
        m.dec_scan(); // would go negative
        assert_eq!(m.active_scans(), 0, "must clamp at zero");
    }

    #[test]
    fn last_error_round_trips() {
        let m = Metrics::new();
        assert_eq!(m.last_error(), None);
        m.set_last_error(Some("boom".into()));
        assert_eq!(m.last_error().as_deref(), Some("boom"));
        m.set_last_error(None);
        assert_eq!(m.last_error(), None);
    }
}
