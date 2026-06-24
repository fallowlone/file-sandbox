//! Port of `src/ui-server.ts`. The Express app becomes an axum `Router`.
//!
//! Routes, JSON shapes, auth rules (bearer or `x-filesandbox-token`, with
//! `/api/health` + `/health` public), and the dashboard HTML are preserved
//! verbatim so the menu bar and browser UI keep working unchanged.

use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::config::{self, mask_secret, Config, RawConfig};
use crate::file_mover::FileMover;
use crate::job_store::{JobConflictError, JobStore};
use crate::launch_agent_monitor::LaunchAgentEvent;
use crate::local_scanner::LocalScanner;
use crate::metrics::Metrics;
use crate::mode::{parse_mode, WatcherMode};
use crate::secret_store::{SecretStore, ACCOUNT_API_TOKEN, ACCOUNT_VT_API_KEY};
use crate::watcher::Watcher;

const MASK_TAIL: usize = 4;
const ALLOWED_MODES: [&str; 3] = ["active", "scan_paused", "monitoring_disabled"];
const SYSTEM_DIRS: [&str; 6] = ["/", "/etc", "/bin", "/usr", "/System", "/Library"];

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as i64
}

pub struct AppState {
    pub store: Arc<JobStore>,
    pub metrics: Arc<Metrics>,
    pub config: Arc<Config>,
    pub watcher: Watcher,
    pub file_mover: FileMover,
    pub security_events: Arc<Mutex<Vec<LaunchAgentEvent>>>,
    /// Secret backend for write routing. `Some` when secrets live outside
    /// `config.json` (Keychain); `None` keeps secret writes in the file.
    pub secret_store: Option<Arc<dyn SecretStore>>,
}

impl AppState {
    /// Port of `index.ts deleteQuarantineJob`. Shared by the HTTP delete route
    /// and the inconclusive sweeper.
    pub async fn delete_quarantine_job(&self, id: &str, detail: &str) -> Result<()> {
        let job = self
            .store
            .get_job(id)?
            .ok_or_else(|| anyhow!("Job {id} not found"))?;
        if job.status != "quarantine_kept" {
            bail!("Job {id} is not in quarantine_kept status");
        }
        let q = job
            .quarantine_path
            .ok_or_else(|| anyhow!("Job {id} has no quarantine path"))?;
        self.file_mover.delete_file(FsPath::new(&q)).await?;
        self.store.set_deleted(id, detail)?; // may yield JobConflictError
        Ok(())
    }

    /// Port of `index.ts restoreQuarantineJob`.
    async fn restore_quarantine_job(&self, id: &str) -> Result<()> {
        let job = self
            .store
            .get_job(id)?
            .ok_or_else(|| anyhow!("Job {id} not found"))?;
        if job.status != "quarantine_kept" {
            bail!("Job {id} is not in quarantine_kept status");
        }
        let q = job
            .quarantine_path
            .ok_or_else(|| anyhow!("Job {id} has no quarantine path"))?;
        let watch = FsPath::new(&self.config.watch_path);
        let dest = self
            .file_mover
            .resolve_restore_destination(watch, &job.original_name)
            .await;
        self.watcher.mark_restoring(dest);
        let restored = self
            .file_mover
            .restore_to_watch(watch, FsPath::new(&q), &job.original_name)
            .await?;
        self.store.set_restored(id, &restored.to_string_lossy())?;
        Ok(())
    }
}

// ── secret / path helpers (ported from ui-server.ts) ────────────────────────

fn should_update_secret_field(incoming: Option<&str>, current_real: &str) -> bool {
    let incoming = match incoming {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };
    if !current_real.is_empty() && incoming == mask_secret(current_real, MASK_TAIL) {
        return false;
    }
    let trimmed = incoming.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|c| c == '*') {
        return false;
    }
    if incoming.starts_with("****") && incoming.len() <= 12 {
        return false;
    }
    true
}

/// Lexically normalize to an absolute path (no filesystem access), mirroring
/// Node's `path.resolve` closely enough for the system-directory guard.
fn resolve_path(raw: &str) -> PathBuf {
    let base = if FsPath::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(raw)
    };
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    for comp in base.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir => out.clear(),
            Component::Normal(s) => out.push(s.to_os_string()),
            Component::Prefix(_) => {}
        }
    }
    let mut p = PathBuf::from("/");
    for c in out {
        p.push(c);
    }
    p
}

fn is_under_system_dir(resolved: &str) -> bool {
    if resolved == "/" {
        return true;
    }
    SYSTEM_DIRS
        .iter()
        .any(|dir| resolved == *dir || resolved.starts_with(&format!("{dir}/")))
}

// ── handlers ────────────────────────────────────────────────────────────────

async fn health(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let scanning = st
        .store
        .list_recent(500)
        .unwrap_or_default()
        .iter()
        .filter(|j| j.status == "scanning" || j.status == "in_quarantine")
        .count();

    let local_scanner = if st.config.pompelmi_enabled {
        match LocalScanner::probe(FsPath::new(&st.config.pompelmi_socket_path), 2000).await {
            Ok(_) => json!({ "enabled": true, "socketReachable": true }),
            Err(_) => json!({ "enabled": true, "socketReachable": false }),
        }
    } else {
        json!({ "enabled": false, "socketReachable": false })
    };

    Json(json!({
        "ok": true,
        "uptimeSec": (now_ms() - st.metrics.started_at_ms) / 1000,
        "activeScans": st.metrics.active_scans(),
        "scanningOrQueuedJobs": scanning,
        "lastError": st.metrics.last_error(),
        "apiAuthEnabled": !st.config.api_token.trim().is_empty(),
        "configEncryptedAtRest": st.config.config_encrypted_at_rest,
        "secretsBackend": st.config.secrets_backend.as_str(),
        "mode": st.watcher.get_mode().as_str(),
        "scannersEnabled": { "pompelmi": st.config.pompelmi_enabled, "vt": st.config.vt_enabled },
        "localScanner": local_scanner,
    }))
}

async fn jobs(State(st): State<Arc<AppState>>) -> Response {
    match st.store.list_recent(200) {
        Ok(rows) => {
            let mode = st.watcher.get_mode();
            Json(json!({
                "jobs": rows,
                "mode": mode.as_str(),
                "paused": mode != WatcherMode::Active,
            }))
            .into_response()
        }
        Err(e) => server_error(e.to_string()),
    }
}

async fn security_events(State(st): State<Arc<AppState>>) -> Response {
    let events = st.security_events.lock().unwrap().clone();
    Json(json!({ "events": events })).into_response()
}

async fn pause(State(st): State<Arc<AppState>>) -> Response {
    eprintln!("[deprecated] POST /api/watcher/pause - use /api/watcher/mode");
    st.watcher.set_mode(WatcherMode::ScanPaused);
    Json(json!({ "ok": true, "paused": true, "mode": "scan_paused" })).into_response()
}

async fn resume(State(st): State<Arc<AppState>>) -> Response {
    eprintln!("[deprecated] POST /api/watcher/resume - use /api/watcher/mode");
    st.watcher.set_mode(WatcherMode::Active);
    Json(json!({ "ok": true, "paused": false, "mode": "active" })).into_response()
}

async fn set_mode(State(st): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let requested_raw = body.get("mode").and_then(|v| v.as_str());
    let parsed = parse_mode(requested_raw);
    // parseMode falls back to "active" silently; reject unknown explicitly.
    if let Some(m) = requested_raw {
        if parsed.as_str() != m {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "unknown mode", "allowed": ALLOWED_MODES, "received": m })),
            )
                .into_response();
        }
    }
    st.watcher.set_mode(parsed);
    Json(json!({ "ok": true, "mode": parsed.as_str() })).into_response()
}

async fn delete_all_jobs(State(st): State<Arc<AppState>>) -> Response {
    match st.store.clear_all() {
        Ok(res) => Json(json!({ "ok": true, "deleted": res.deleted, "skipped": res.skipped }))
            .into_response(),
        Err(e) => server_error(e.to_string()),
    }
}

async fn cancel(State(st): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    st.watcher.cancel(&id);
    Json(json!({ "ok": true })).into_response()
}

async fn restore(State(st): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match st.restore_quarantine_job(&id).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_config(State(st): State<Arc<AppState>>) -> Response {
    let c = &st.config;
    let mask = |v: &str| {
        if v.is_empty() {
            String::new()
        } else {
            mask_secret(v, MASK_TAIL)
        }
    };
    Json(json!({
        "vtApiKey": mask(&c.vt_api_key),
        "apiToken": mask(&c.api_token),
        "watchPath": c.watch_path,
        "quarantinePath": c.quarantine_path,
        "databasePath": c.database_path,
        "httpPort": c.http_port.map(|p| p.to_string()).unwrap_or_default(),
        "httpHost": c.http_host,
        "watchRecursive": c.watch_recursive,
        "maxScanBytes": c.max_scan_bytes,
        "maxConcurrentScans": c.max_concurrent_scans,
        "useSeparateVtProcess": c.use_separate_vt_process,
        "inconclusiveRetentionDays": c.inconclusive_retention_days,
        "vtHashOnly": c.vt_hash_only,
        "configEncryptedAtRest": c.config_encrypted_at_rest,
        "secretsBackend": c.secrets_backend.as_str(),
    }))
    .into_response()
}

/// Route secret updates to `store` instead of `config.json`. Consumes the
/// secret fields from `updates` (`take`) so the subsequent file write never
/// persists them. An empty value clears the secret from the store.
fn route_secrets_to_store(store: &dyn SecretStore, updates: &mut RawConfig) -> Result<()> {
    if let Some(v) = updates.vt_api_key.take() {
        if v.is_empty() {
            store.delete(ACCOUNT_VT_API_KEY)?;
        } else {
            store.set(ACCOUNT_VT_API_KEY, &v)?;
        }
    }
    if let Some(v) = updates.api_token.take() {
        if v.is_empty() {
            store.delete(ACCOUNT_API_TOKEN)?;
        } else {
            store.set(ACCOUNT_API_TOKEN, &v)?;
        }
    }
    Ok(())
}

async fn post_config(State(st): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let s = |k: &str| body.get(k).and_then(|v| v.as_str());
    let as_bool = |k: &str| match body.get(k) {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::String(v)) if v == "true" => Some(true),
        Some(Value::String(v)) if v == "false" => Some(false),
        _ => None,
    };
    let as_int = |k: &str| -> Option<i64> {
        match body.get(k) {
            Some(Value::Number(n)) => n.as_f64().map(|f| f.floor() as i64),
            Some(Value::String(v)) => v.parse::<f64>().ok().map(|f| f.floor() as i64),
            _ => None,
        }
    };

    let mut updates = RawConfig::default();

    if should_update_secret_field(s("vtApiKey"), &st.config.vt_api_key) {
        updates.vt_api_key = s("vtApiKey").map(String::from);
    }
    if let Some(tok) = body.get("apiToken") {
        if tok.as_str() == Some("") {
            updates.api_token = Some(String::new());
        } else if should_update_secret_field(tok.as_str(), &st.config.api_token) {
            updates.api_token = tok.as_str().map(String::from);
        }
    }

    // Path fields: validate (resolve + reject system dirs) before writing.
    for (key, raw) in [
        ("watchPath", s("watchPath")),
        ("quarantinePath", s("quarantinePath")),
    ] {
        if let Some(p) = raw.filter(|p| !p.is_empty()) {
            let resolved = resolve_path(p);
            let rs = resolved.to_string_lossy().into_owned();
            if !rs.starts_with('/') {
                return bad_request(format!("{key} must be an absolute path"));
            }
            if is_under_system_dir(&rs) {
                return bad_request(format!(
                    "{key} cannot be / or under system directories ({})",
                    SYSTEM_DIRS.join(", ")
                ));
            }
            if key == "watchPath" {
                updates.watch_path = Some(rs);
            } else {
                updates.quarantine_path = Some(rs);
            }
        }
    }
    if let Some(p) = s("databasePath").filter(|p| !p.is_empty()) {
        let resolved = resolve_path(p);
        let rs = resolved.to_string_lossy().into_owned();
        if !rs.ends_with(".sqlite") && !rs.ends_with(".db") {
            return bad_request("databasePath must end with .sqlite or .db".into());
        }
        updates.database_path = Some(rs);
    }

    if let Some(v) = s("httpHost").filter(|v| !v.is_empty()) {
        updates.http_host = Some(v.to_string());
    }
    if let Some(v) = s("httpPort").filter(|v| !v.is_empty()) {
        if let Ok(n) = v.parse::<i64>() {
            if (1..=65535).contains(&n) {
                updates.http_port = Some(n as u32);
            }
        }
    }
    if let Some(b) = as_bool("watchRecursive") {
        updates.watch_recursive = Some(b);
    }
    if let Some(n) = as_int("maxScanBytes").filter(|&n| n >= 1) {
        updates.max_scan_bytes = Some(n as u64);
    }
    if let Some(n) = as_int("maxConcurrentScans").filter(|&n| n >= 1) {
        updates.max_concurrent_scans = Some(n as u32);
    }
    if let Some(b) = as_bool("useSeparateVtProcess") {
        updates.use_separate_vt_process = Some(b);
    }
    if let Some(b) = as_bool("vtHashOnly") {
        updates.vt_hash_only = Some(b);
    }
    if let Some(n) = as_int("inconclusiveRetentionDays").filter(|&n| n >= 0) {
        updates.inconclusive_retention_days = Some(n as u32);
    }
    if let Some(v) = s("secretsBackend").filter(|v| !v.is_empty()) {
        let v = v.to_lowercase();
        if v != "file" && v != "keychain" {
            return bad_request("secretsBackend must be 'file' or 'keychain'".into());
        }
        updates.secrets_backend = Some(v);
    }

    // When a secret store is active, secrets are written to it and stripped from
    // `updates` so they never land in config.json as plaintext.
    if let Some(store) = st.secret_store.as_deref() {
        if let Err(e) = route_secrets_to_store(store, &mut updates) {
            return server_error(e.to_string());
        }
    }

    match config::write_config(updates) {
        Ok(()) => Json(json!({ "ok": true, "restartRequired": true })).into_response(),
        Err(e) => server_error(e.to_string()),
    }
}

async fn delete_quarantine(State(st): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match st.delete_quarantine_job(&id, "Deleted by user").await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => {
            let is_conflict = e.downcast_ref::<JobConflictError>().is_some();
            let status = if is_conflict {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(json!({ "error": e.to_string() }))).into_response()
        }
    }
}

async fn root() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

fn server_error(msg: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
        .into_response()
}

fn bad_request(msg: String) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}

// ── auth ──────────────────────────────────────────────────────────────────

async fn auth(State(st): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path == "/api/health" || path == "/health" {
        return next.run(req).await;
    }
    let token = st.config.api_token.trim();
    if token.is_empty() {
        return next.run(req).await;
    }
    let headers = req.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.to_lowercase().starts_with("bearer "))
        .map(|v| v[7..].trim());
    let header_tok = headers
        .get("x-filesandbox-token")
        .and_then(|v| v.to_str().ok());
    if bearer == Some(token) || header_tok == Some(token) {
        return next.run(req).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "Unauthorized" })),
    )
        .into_response()
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/health",
            get(|| async { Redirect::permanent("/api/health") }),
        )
        .route("/api/jobs", get(jobs).delete(delete_all_jobs))
        .route("/api/security-events", get(security_events))
        .route("/api/watcher/pause", post(pause))
        .route("/api/watcher/resume", post(resume))
        .route("/api/watcher/mode", post(set_mode))
        .route("/api/jobs/:id/cancel", post(cancel))
        .route("/api/jobs/:id/restore", post(restore))
        .route("/api/config", get(get_config).post(post_config))
        .route("/api/jobs/:id/quarantine", delete(delete_quarantine))
        .route("/", get(root))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

/// Bind and serve the UI. Runs until the server stops.
pub async fn serve(state: Arc<AppState>, host: &str, port: u16) -> Result<()> {
    let app = build_router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("UI http://{addr}/");
    axum::serve(listener, app).await?;
    Ok(())
}

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>file-sandbox jobs</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 1rem; background: #111; color: #e6e6e6; }
    table { border-collapse: collapse; width: 100%; font-size: 14px; }
    th, td { border: 1px solid #333; padding: 6px 8px; text-align: left; vertical-align: top; }
    th { background: #1a1a1a; }
    tr:nth-child(even) { background: #161616; }
    h1 { font-size: 1.1rem; }
    a { color: #8cb4ff; }
    .oversized { color: #ffb347; font-weight: 600; }
    .refresh-status { color: #666; font-size: 12px; margin-left: 0.5rem; }
  </style>
</head>
<body>
  <h1>Quarantine / VirusTotal job queue</h1>
  <p><a href="/api/jobs">JSON</a> · <a href="/api/health">health</a> · refreshes every 15s<span id="status" class="refresh-status"></span></p>
  <table>
    <thead><tr><th>id</th><th>file</th><th>status</th><th>VT</th><th>detail</th><th>final path</th><th>actions</th></tr></thead>
    <tbody id="jobs"><tr><td colspan="7">loading…</td></tr></tbody>
  </table>
  <script>
    const tbody = document.getElementById('jobs');
    const status = document.getElementById('status');

    function cell(text) {
      const td = document.createElement('td');
      td.textContent = text;
      return td;
    }

    function rowFor(j) {
      const tr = document.createElement('tr');
      tr.appendChild(cell(j.id.slice(0, 8) + '…'));
      tr.appendChild(cell(j.original_name));
      tr.appendChild(cell(j.status));

      const vt = document.createElement('td');
      if (!j.vt_verdict) {
        vt.textContent = '—';
      } else if (j.vt_verdict === 'oversized') {
        const span = document.createElement('span');
        span.className = 'oversized';
        span.textContent = j.vt_verdict;
        vt.appendChild(span);
      } else {
        vt.textContent = j.vt_verdict;
      }
      tr.appendChild(vt);

      const det = document.createElement('td');
      const full = j.detail ?? '';
      det.title = full;
      det.textContent = full.length > 80 ? full.slice(0, 80) + '…' : full;
      tr.appendChild(det);

      tr.appendChild(cell(j.final_path ?? '—'));

      const acts = document.createElement('td');
      if (j.status === 'quarantine_kept') {
        const r = document.createElement('button');
        r.type = 'button';
        r.textContent = 'Restore';
        r.addEventListener('click', () => restoreFile(j.id));
        acts.appendChild(r);
        acts.appendChild(document.createTextNode(' '));
        const d = document.createElement('button');
        d.type = 'button';
        d.textContent = 'Delete';
        d.addEventListener('click', () => deleteFile(j.id));
        acts.appendChild(d);
      }
      tr.appendChild(acts);
      return tr;
    }

    function render(jobs) {
      tbody.replaceChildren();
      if (!jobs.length) {
        const tr = document.createElement('tr');
        const td = document.createElement('td');
        td.colSpan = 7;
        td.textContent = 'no jobs yet';
        tr.appendChild(td);
        tbody.appendChild(tr);
        return;
      }
      for (const j of jobs) tbody.appendChild(rowFor(j));
    }

    async function refresh() {
      try {
        const res = await fetch('/api/jobs');
        if (!res.ok) {
          status.textContent = 'error ' + res.status;
          return;
        }
        const data = await res.json();
        render(data.jobs ?? []);
        status.textContent = '';
      } catch (e) {
        status.textContent = 'offline';
      }
    }

    async function deleteFile(id) {
      if (!confirm('Permanently delete quarantined file?')) return;
      const res = await fetch('/api/jobs/' + id + '/quarantine', { method: 'DELETE' });
      const data = await res.json();
      if (data.ok) refresh();
      else alert('Error: ' + data.error);
    }

    async function restoreFile(id) {
      if (!confirm('Restore this file to the watch folder?')) return;
      const res = await fetch('/api/jobs/' + id + '/restore', { method: 'POST' });
      const data = await res.json();
      if (data.ok) refresh();
      else alert('Error: ' + data.error);
    }

    refresh();
    setInterval(refresh, 15000);
  </script>
</body>
</html>"##;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::WatcherOptions;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state(api_token: Option<&str>) -> Arc<AppState> {
        let raw = RawConfig {
            api_token: api_token.map(String::from),
            ..Default::default()
        };
        let cfg = config::resolve(raw, &(|_: &str| None)).unwrap();
        let store = Arc::new(JobStore::new(":memory:").unwrap());
        let metrics = Metrics::new();
        let watcher = Watcher::new(
            "/tmp/w",
            vec![],
            "/tmp/q",
            "k",
            store.clone(),
            metrics.clone(),
            WatcherOptions::default(),
        );
        Arc::new(AppState {
            store,
            metrics,
            config: Arc::new(cfg),
            watcher,
            file_mover: FileMover::new("/tmp/q"),
            security_events: Arc::new(Mutex::new(Vec::new())),
            secret_store: None,
        })
    }

    #[test]
    fn should_update_secret_field_rules() {
        assert!(!should_update_secret_field(None, "real"));
        assert!(!should_update_secret_field(Some(""), "real"));
        assert!(!should_update_secret_field(Some("****cdef"), "abcdef")); // == mask of current
        assert!(!should_update_secret_field(Some("****"), "real")); // all stars
        assert!(!should_update_secret_field(Some("****1234"), "")); // masked-looking, <=12
        assert!(should_update_secret_field(
            Some("sk-realnewkey-1234567"),
            "old"
        ));
    }

    #[test]
    fn system_dirs_rejected() {
        assert!(is_under_system_dir("/"));
        assert!(is_under_system_dir("/etc"));
        assert!(is_under_system_dir("/usr/local/bin"));
        assert!(is_under_system_dir("/Library/Foo"));
        assert!(!is_under_system_dir("/Users/me/intake"));
        assert!(!is_under_system_dir("/tmp/watch"));
    }

    #[tokio::test]
    async fn health_is_public() {
        let app = build_router(test_state(Some("secret")));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["apiAuthEnabled"], true);
    }

    #[tokio::test]
    async fn health_reports_secrets_backend() {
        let app = build_router(test_state(None));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["secretsBackend"], "file");
    }

    #[tokio::test]
    async fn config_reports_secrets_backend() {
        let app = build_router(test_state(None));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["secretsBackend"], "file");
    }

    #[test]
    fn route_secrets_writes_to_store_and_clears_updates() {
        use crate::secret_store::MemoryStore;
        let store = MemoryStore::new();
        let mut updates = RawConfig {
            vt_api_key: Some("vt-new".into()),
            api_token: Some("tok-new".into()),
            ..Default::default()
        };
        route_secrets_to_store(&store, &mut updates).unwrap();
        assert_eq!(
            store.get(ACCOUNT_VT_API_KEY).unwrap().as_deref(),
            Some("vt-new")
        );
        assert_eq!(
            store.get(ACCOUNT_API_TOKEN).unwrap().as_deref(),
            Some("tok-new")
        );
        // Stripped from updates → never written to config.json.
        assert!(updates.vt_api_key.is_none());
        assert!(updates.api_token.is_none());
    }

    #[test]
    fn route_secrets_empty_value_clears_store() {
        use crate::secret_store::MemoryStore;
        let store = MemoryStore::new();
        store.set(ACCOUNT_API_TOKEN, "old").unwrap();
        let mut updates = RawConfig {
            api_token: Some(String::new()), // explicit clear
            ..Default::default()
        };
        route_secrets_to_store(&store, &mut updates).unwrap();
        assert_eq!(store.get(ACCOUNT_API_TOKEN).unwrap(), None);
        assert!(updates.api_token.is_none());
    }

    #[tokio::test]
    async fn protected_route_requires_token() {
        let app = build_router(test_state(Some("secret")));
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/jobs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let ok = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs")
                    .header("x-filesandbox-token", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_token_allows_protected_route() {
        let app = build_router(test_state(None));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
