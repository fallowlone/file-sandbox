//! file-sandbox daemon — native Rust port of the TS/Node watcher.
//!
//! Phase 1 modules: configuration loading + encrypted-at-rest config + watcher
//! mode. Further modules (job store, watcher, scanning, HTTP UI) land in later
//! phases per `docs/RUST_MIGRATION_PLAN.md`.

pub mod config;
pub mod config_crypto;
pub mod file_mover;
pub mod file_permissions;
pub mod http_host_guard;
pub mod inconclusive_sweeper;
pub mod job_store;
pub mod launch_agent_monitor;
pub mod local_scanner;
pub mod metrics;
pub mod mode;
pub mod secret_store;
pub mod ui_server;
pub mod virus_checker;
pub mod vt_cache;
pub mod watcher;
