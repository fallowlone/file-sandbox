//! Port of `src/watcher-mode.ts`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherMode {
    Active,
    ScanPaused,
    MonitoringDisabled,
}

impl WatcherMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WatcherMode::Active => "active",
            WatcherMode::ScanPaused => "scan_paused",
            WatcherMode::MonitoringDisabled => "monitoring_disabled",
        }
    }
}

/// Mirror of the TS `parseMode`: unknown / missing input falls back to `active`.
pub fn parse_mode(input: Option<&str>) -> WatcherMode {
    match input {
        Some("active") => WatcherMode::Active,
        Some("scan_paused") => WatcherMode::ScanPaused,
        Some("monitoring_disabled") => WatcherMode::MonitoringDisabled,
        _ => WatcherMode::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_modes() {
        assert_eq!(parse_mode(Some("active")), WatcherMode::Active);
        assert_eq!(parse_mode(Some("scan_paused")), WatcherMode::ScanPaused);
        assert_eq!(
            parse_mode(Some("monitoring_disabled")),
            WatcherMode::MonitoringDisabled
        );
    }

    #[test]
    fn unknown_falls_back_to_active() {
        assert_eq!(parse_mode(Some("bogus")), WatcherMode::Active);
        assert_eq!(parse_mode(None), WatcherMode::Active);
    }

    #[test]
    fn as_str_round_trips() {
        for m in [
            WatcherMode::Active,
            WatcherMode::ScanPaused,
            WatcherMode::MonitoringDisabled,
        ] {
            assert_eq!(parse_mode(Some(m.as_str())), m);
        }
    }
}
