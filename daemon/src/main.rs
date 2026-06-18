//! Daemon entry point — native Rust port of `src/index.ts`.
//!
//! Loads config, validates required fields, probes the local clamd scanner,
//! starts the watcher, the LaunchAgent persistence monitor, the HTTP UI, and
//! the inconclusive sweeper, then runs until Ctrl-C / SIGTERM.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};

use file_sandbox_daemon::config::{self, RawConfig};
use file_sandbox_daemon::file_mover::FileMover;
use file_sandbox_daemon::http_host_guard::assert_safe_http_host;
use file_sandbox_daemon::inconclusive_sweeper::start_inconclusive_sweeper;
use file_sandbox_daemon::job_store::JobStore;
use file_sandbox_daemon::launch_agent_monitor::{
    default_agent_paths, start_launch_agent_monitor, LaunchAgentEvent,
};
use file_sandbox_daemon::local_scanner::LocalScanner;
use file_sandbox_daemon::metrics::Metrics;
use file_sandbox_daemon::mode::WatcherMode;
use file_sandbox_daemon::ui_server::{self, AppState};
use file_sandbox_daemon::watcher::{Watcher, WatcherOptions};

const SECURITY_EVENT_CAP: usize = 200;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::load()?;
    if cfg.vt_api_key.is_empty() {
        bail!("vtApiKey not set (config.json or VT_API_KEY)");
    }
    if cfg.watch_path.is_empty() {
        bail!("watchPath not set (config.json or WATCH_PATH)");
    }
    if cfg.quarantine_path.is_empty() {
        bail!("quarantinePath not set (config.json or QUARANTINE_PATH)");
    }

    // Local clamd scanner: probe before enabling, refuse to start on a dead socket.
    let local_scanner = if cfg.pompelmi_enabled {
        match LocalScanner::probe(Path::new(&cfg.pompelmi_socket_path), 2000).await {
            Ok(()) => {
                eprintln!("[pompelmi] enabled, socket={}", cfg.pompelmi_socket_path);
                Some(LocalScanner::new(cfg.pompelmi_socket_path.clone()))
            }
            Err(e) => {
                eprintln!(
                    "[pompelmi] enabled but probe failed ({e}). Refusing to start. Disable with pompelmiEnabled=false or fix clamd."
                );
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("[pompelmi] disabled by config");
        None
    };

    let job_store = Arc::new(JobStore::new(&cfg.database_path)?);
    let metrics = Metrics::new();
    let file_mover = FileMover::new(&cfg.quarantine_path);

    let watcher = Watcher::new(
        cfg.watch_path.clone(),
        vec![".DS_Store".to_string()],
        cfg.quarantine_path.clone(),
        cfg.vt_api_key.clone(),
        job_store.clone(),
        metrics.clone(),
        WatcherOptions {
            watch_recursive: cfg.watch_recursive,
            max_scan_bytes: cfg.max_scan_bytes,
            max_concurrent_scans: cfg.max_concurrent_scans,
            use_separate_vt_process: cfg.use_separate_vt_process,
            local_scanner,
            pompelmi_failure_mode: cfg.pompelmi_failure_mode,
            initial_mode: cfg.watcher_mode,
            vt_enabled: cfg.vt_enabled,
            on_mode_change: Some(Box::new(|m: WatcherMode| {
                let updates = RawConfig { watcher_mode: Some(m.as_str().to_string()), ..Default::default() };
                if let Err(e) = config::write_config(updates) {
                    eprintln!("[config] failed to persist mode: {e}");
                }
            })),
        },
    );
    let _watch_handles = watcher.start()?;

    // In-memory ring buffer of persistence-monitor alerts, polled by the menu
    // bar via /api/security-events. Bounded so a long-running daemon can't grow
    // unbounded.
    let security_events: Arc<Mutex<Vec<LaunchAgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let se = security_events.clone();
    let _agent_monitor = match start_launch_agent_monitor(default_agent_paths(), move |event| {
        let mut buf = se.lock().unwrap();
        buf.push(event);
        if buf.len() > SECURITY_EVENT_CAP {
            buf.remove(0);
        }
    }) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("[SECURITY] launch agent monitor failed to start: {e}");
            None
        }
    };

    let app_state = Arc::new(AppState {
        store: job_store.clone(),
        metrics: metrics.clone(),
        config: Arc::new(cfg.clone()),
        watcher: watcher.clone(),
        file_mover: file_mover.clone(),
        security_events: security_events.clone(),
    });

    // HTTP UI.
    if let Some(port) = cfg.http_port {
        let bind_host = cfg.http_host.clone();
        if let Err(msg) = assert_safe_http_host(&bind_host) {
            eprintln!("{msg}");
            std::process::exit(1);
        }
        let state = app_state.clone();
        tokio::spawn(async move {
            if let Err(e) = ui_server::serve(state, &bind_host, port).await {
                eprintln!("[http] server error: {e}");
            }
        });
    }

    // Inconclusive sweeper.
    let _sweeper = if cfg.inconclusive_retention_days > 0 {
        let days = cfg.inconclusive_retention_days as i64;
        let st = app_state.clone();
        start_inconclusive_sweeper(days, job_store.clone(), move |id| {
            let st = st.clone();
            let detail = format!("Auto-deleted after {days} day(s) (inconclusive)");
            async move { st.delete_quarantine_job(&id, &detail).await }
        })
    } else {
        None
    };

    tokio::signal::ctrl_c().await?;
    eprintln!("[file-sandbox] shutting down");
    Ok(())
}
