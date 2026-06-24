//! Port of `src/config.ts`. Resolves configuration from `config.json` (in the
//! process working directory) overlaid with environment variables, with the same
//! precedence (file value > env var > default) and the same defaults as the Node
//! implementation. Supports the `FSENC1:` encrypted-at-rest config.

use crate::config_crypto::{decrypt_config_json, encrypt_config_json, is_encrypted_config_payload};
use crate::mode::{parse_mode, WatcherMode};
use crate::secret_store::{self, SecretStore};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_MAX_SCAN: u64 = 400 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    Bypass,
    Inconclusive,
}

/// Where secrets (`vtApiKey`, `apiToken`) are read from. `File` keeps the legacy
/// plaintext-in-`config.json` behaviour; `Keychain` overlays the macOS Keychain
/// over the file/env values. Default is `File` so existing installs are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretsBackend {
    #[default]
    File,
    Keychain,
}

impl SecretsBackend {
    fn from_str(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("keychain") {
            SecretsBackend::Keychain
        } else {
            SecretsBackend::File
        }
    }

    /// Stable string form for config.json and API responses.
    pub fn as_str(&self) -> &'static str {
        match self {
            SecretsBackend::File => "file",
            SecretsBackend::Keychain => "keychain",
        }
    }
}

/// Raw, fully-optional shape as stored in `config.json` (camelCase keys).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawConfig {
    pub vt_api_key: Option<String>,
    pub watch_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub database_path: Option<String>,
    pub http_port: Option<u32>,
    pub http_host: Option<String>,
    pub api_token: Option<String>,
    pub watch_recursive: Option<bool>,
    pub max_scan_bytes: Option<u64>,
    pub max_concurrent_scans: Option<u32>,
    pub use_separate_vt_process: Option<bool>,
    pub inconclusive_retention_days: Option<u32>,
    pub pompelmi_enabled: Option<bool>,
    pub pompelmi_socket_path: Option<String>,
    pub pompelmi_failure_mode: Option<String>,
    pub watcher_mode: Option<String>,
    pub vt_enabled: Option<bool>,
    pub vt_hash_only: Option<bool>,
    pub secrets_backend: Option<String>,
}

/// Resolved configuration with concrete values and defaults applied.
#[derive(Debug, Clone)]
pub struct Config {
    pub vt_api_key: String,
    pub api_token: String,
    pub watch_path: String,
    pub quarantine_path: String,
    pub database_path: String,
    pub http_port: Option<u16>,
    pub http_host: String,
    pub watch_recursive: bool,
    pub max_scan_bytes: u64,
    pub max_concurrent_scans: u32,
    pub use_separate_vt_process: bool,
    pub inconclusive_retention_days: u32,
    pub pompelmi_enabled: bool,
    pub pompelmi_socket_path: String,
    pub pompelmi_failure_mode: FailureMode,
    pub watcher_mode: WatcherMode,
    pub vt_enabled: bool,
    /// When true (default), a file unknown to VirusTotal is looked up by SHA-256
    /// only and never uploaded — its content never leaves the machine.
    pub vt_hash_only: bool,
    pub config_encrypted_at_rest: bool,
    pub secrets_backend: SecretsBackend,
}

fn config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config.json")
}

fn master_key_from_env(env: &impl Fn(&str) -> Option<String>) -> Option<String> {
    env("FILESANDBOX_MASTER_KEY")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Decrypt (if needed) and parse a raw config file body into [`RawConfig`].
/// Mirrors the TS `loadFile` + `parseConfigJson` behaviour: malformed JSON
/// degrades to defaults, encrypted-without-key is a hard error.
pub fn decode_config(raw: &str, master_key: Option<&str>) -> Result<RawConfig> {
    let mut body = raw.to_string();
    if is_encrypted_config_payload(&body) {
        match master_key {
            Some(mk) => {
                body = decrypt_config_json(body.trim(), mk).map_err(|e| {
                    anyhow!("Failed to decrypt config.json (check FILESANDBOX_MASTER_KEY): {e}")
                })?;
            }
            None => bail!("config.json is encrypted; set FILESANDBOX_MASTER_KEY to decrypt."),
        }
    }
    match serde_json::from_str::<RawConfig>(&body) {
        Ok(cfg) => Ok(cfg),
        Err(e) => {
            eprintln!("[config] config.json is malformed — using defaults. Error: {e}");
            Ok(RawConfig::default())
        }
    }
}

fn get(file_val: Option<String>, env_val: Option<String>) -> String {
    file_val.or(env_val).unwrap_or_default()
}

fn env_int(env: &impl Fn(&str) -> Option<String>, name: &str, fallback: u64) -> u64 {
    match env(name) {
        Some(v) if !v.is_empty() => v
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map_or(fallback, |n| n as u64),
        _ => fallback,
    }
}

fn env_bool(env: &impl Fn(&str) -> Option<String>, name: &str, fallback: bool) -> bool {
    match env(name) {
        Some(v) => {
            let v = v.trim().to_lowercase();
            match v.as_str() {
                "" => fallback,
                "1" | "true" | "yes" | "on" => true,
                "0" | "false" | "no" | "off" => false,
                _ => fallback,
            }
        }
        None => fallback,
    }
}

/// Resolve a [`RawConfig`] against an environment lookup into a final [`Config`].
pub fn resolve(file: RawConfig, env: &impl Fn(&str) -> Option<String>) -> Result<Config> {
    let http_port = {
        let raw = file
            .http_port
            .map(|n| n.to_string())
            .or_else(|| env("HTTP_PORT"));
        match raw {
            None => None,
            Some(s) if s.is_empty() => None,
            Some(s) => {
                let n: f64 = s.parse().map_err(|_| anyhow!("httpPort must be 1-65535"))?;
                if !(1.0..=65535.0).contains(&n) {
                    bail!("httpPort must be 1-65535");
                }
                Some(n as u16)
            }
        }
    };

    let pompelmi_failure_mode = {
        let v = file
            .pompelmi_failure_mode
            .or_else(|| env("POMPELMI_FAILURE_MODE"))
            .unwrap_or_else(|| "bypass".to_string())
            .trim()
            .to_lowercase();
        if v == "inconclusive" {
            FailureMode::Inconclusive
        } else {
            FailureMode::Bypass
        }
    };

    let max_concurrent_scans = file
        .max_concurrent_scans
        .unwrap_or_else(|| env_int(env, "MAX_CONCURRENT_SCANS", 2) as u32)
        .max(1);

    Ok(Config {
        vt_api_key: get(file.vt_api_key, env("VT_API_KEY")),
        api_token: get(file.api_token, env("FILESANDBOX_API_TOKEN")),
        watch_path: get(file.watch_path, env("WATCH_PATH")),
        quarantine_path: get(file.quarantine_path, env("QUARANTINE_PATH")),
        database_path: file
            .database_path
            .or_else(|| env("DATABASE_PATH"))
            .unwrap_or_else(|| "./data/jobs.sqlite".to_string()),
        http_port,
        http_host: file
            .http_host
            .or_else(|| env("HTTP_HOST"))
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        watch_recursive: file
            .watch_recursive
            .unwrap_or_else(|| env_bool(env, "WATCH_RECURSIVE", true)),
        max_scan_bytes: file
            .max_scan_bytes
            .unwrap_or_else(|| env_int(env, "MAX_SCAN_BYTES", DEFAULT_MAX_SCAN)),
        max_concurrent_scans,
        use_separate_vt_process: file
            .use_separate_vt_process
            .unwrap_or_else(|| env_bool(env, "USE_SEPARATE_VT_PROCESS", false)),
        inconclusive_retention_days: file
            .inconclusive_retention_days
            .unwrap_or_else(|| env_int(env, "INCONCLUSIVE_RETENTION_DAYS", 0) as u32),
        pompelmi_enabled: file
            .pompelmi_enabled
            .unwrap_or_else(|| env_bool(env, "POMPELMI_ENABLED", true)),
        pompelmi_socket_path: file
            .pompelmi_socket_path
            .or_else(|| env("POMPELMI_SOCKET"))
            .unwrap_or_else(|| "/tmp/clamd.sock".to_string()),
        pompelmi_failure_mode,
        watcher_mode: parse_mode(file.watcher_mode.or_else(|| env("WATCHER_MODE")).as_deref()),
        vt_enabled: file
            .vt_enabled
            .unwrap_or_else(|| env_bool(env, "VT_ENABLED", true)),
        vt_hash_only: file
            .vt_hash_only
            .unwrap_or_else(|| env_bool(env, "VT_HASH_ONLY", true)),
        config_encrypted_at_rest: master_key_from_env(env).is_some(),
        secrets_backend: SecretsBackend::from_str(
            &file
                .secrets_backend
                .or_else(|| env("SECRETS_BACKEND"))
                .unwrap_or_default(),
        ),
    })
}

/// Overlay secrets from `store` onto an already-resolved `cfg`. A non-empty
/// stored value wins over the file/env value (Keychain > config.json > env); an
/// absent or empty stored secret leaves the resolved field untouched. Pure with
/// respect to the OS — call with any [`SecretStore`], including a test double.
pub fn apply_secret_store(cfg: &mut Config, store: &dyn SecretStore) -> Result<()> {
    if let Some(v) = store
        .get(secret_store::ACCOUNT_VT_API_KEY)?
        .filter(|v| !v.is_empty())
    {
        cfg.vt_api_key = v;
    }
    if let Some(v) = store
        .get(secret_store::ACCOUNT_API_TOKEN)?
        .filter(|v| !v.is_empty())
    {
        cfg.api_token = v;
    }
    Ok(())
}

/// Overlay the macOS Keychain over `cfg`. On read failure (e.g. a locked login
/// keychain in a headless LaunchAgent session) the file/env values are kept and
/// the error is logged — never fatal. A no-op announcing the limitation on
/// non-macOS platforms.
#[cfg(target_os = "macos")]
fn apply_keychain_secrets(cfg: &mut Config) {
    let store = secret_store::KeychainStore::new();
    if let Err(e) = apply_secret_store(cfg, &store) {
        eprintln!("[secrets] keychain read failed, using file/env values: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_keychain_secrets(_cfg: &mut Config) {
    eprintln!("[secrets] keychain backend unsupported on this platform; using file/env values");
}

/// Load configuration from `config.json` in the CWD overlaid with the process
/// environment.
pub fn load() -> Result<Config> {
    let env = |name: &str| std::env::var(name).ok();
    let path = config_path();
    let raw = read_config_file_raw(&path);
    let mk = master_key_from_env(&env);
    let file = decode_config(&raw, mk.as_deref())?;
    let mut cfg = resolve(file, &env)?;
    if cfg.secrets_backend == SecretsBackend::Keychain {
        apply_keychain_secrets(&mut cfg);
    }
    Ok(cfg)
}

fn read_config_file_raw(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string())
}

/// Mask a secret for API responses (`****` + last `visible_tail` chars).
pub fn mask_secret(value: &str, visible_tail: usize) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.chars().count() <= visible_tail {
        return "****".to_string();
    }
    let tail: String = value
        .chars()
        .rev()
        .take(visible_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("****{tail}")
}

/// Serialize config updates, re-encrypting when a master key is present.
pub fn serialize_for_disk(merged: &RawConfig, master_key: Option<&str>) -> Result<String> {
    let body = serde_json::to_string_pretty(merged)?;
    match master_key {
        Some(mk) => encrypt_config_json(&body, mk),
        None => Ok(body),
    }
}

/// Overlay the `Some` fields of `updates` onto `current` (port of the TS
/// `writeConfig` merge — only provided keys are changed).
fn merge_raw(current: &mut RawConfig, updates: RawConfig) {
    if updates.vt_api_key.is_some() {
        current.vt_api_key = updates.vt_api_key;
    }
    if updates.watch_path.is_some() {
        current.watch_path = updates.watch_path;
    }
    if updates.quarantine_path.is_some() {
        current.quarantine_path = updates.quarantine_path;
    }
    if updates.database_path.is_some() {
        current.database_path = updates.database_path;
    }
    if updates.http_port.is_some() {
        current.http_port = updates.http_port;
    }
    if updates.http_host.is_some() {
        current.http_host = updates.http_host;
    }
    if updates.api_token.is_some() {
        current.api_token = updates.api_token;
    }
    if updates.watch_recursive.is_some() {
        current.watch_recursive = updates.watch_recursive;
    }
    if updates.max_scan_bytes.is_some() {
        current.max_scan_bytes = updates.max_scan_bytes;
    }
    if updates.max_concurrent_scans.is_some() {
        current.max_concurrent_scans = updates.max_concurrent_scans;
    }
    if updates.use_separate_vt_process.is_some() {
        current.use_separate_vt_process = updates.use_separate_vt_process;
    }
    if updates.inconclusive_retention_days.is_some() {
        current.inconclusive_retention_days = updates.inconclusive_retention_days;
    }
    if updates.pompelmi_enabled.is_some() {
        current.pompelmi_enabled = updates.pompelmi_enabled;
    }
    if updates.pompelmi_socket_path.is_some() {
        current.pompelmi_socket_path = updates.pompelmi_socket_path;
    }
    if updates.pompelmi_failure_mode.is_some() {
        current.pompelmi_failure_mode = updates.pompelmi_failure_mode;
    }
    if updates.watcher_mode.is_some() {
        current.watcher_mode = updates.watcher_mode;
    }
    if updates.vt_enabled.is_some() {
        current.vt_enabled = updates.vt_enabled;
    }
    if updates.vt_hash_only.is_some() {
        current.vt_hash_only = updates.vt_hash_only;
    }
    if updates.secrets_backend.is_some() {
        current.secrets_backend = updates.secrets_backend;
    }
}

/// Read `config.json`, overlay `updates`, and write it back (re-encrypting when
/// `FILESANDBOX_MASTER_KEY` is set). Port of the TS `writeConfig`.
pub fn write_config(updates: RawConfig) -> Result<()> {
    let env = |name: &str| std::env::var(name).ok();
    let path = config_path();
    let raw = read_config_file_raw(&path);
    let mk = master_key_from_env(&env);
    let mut current = decode_config(&raw, mk.as_deref())?;
    merge_raw(&mut current, updates);
    let body = serialize_for_disk(&current, mk.as_deref())?;
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn store_has(store: &dyn SecretStore, account: &str) -> Result<bool> {
    Ok(store.get(account)?.filter(|v| !v.is_empty()).is_some())
}

/// Converge plaintext secrets from `config.json` into `store`: each plaintext
/// value is written, **verify-read back**, and only then is its plaintext copy
/// blanked from the file. A verify mismatch aborts before any blanking so a
/// failed write can never lose the secret. Stale plaintext already mirrored in
/// the store is blanked without rewriting. Returns the number of secrets written
/// to the store. Reads the same `config.json` and master key as [`load`].
pub fn migrate_secrets_to_keychain(store: &dyn SecretStore) -> Result<usize> {
    let env = |name: &str| std::env::var(name).ok();
    let path = config_path();
    let raw = read_config_file_raw(&path);
    let mk = master_key_from_env(&env);
    let file = decode_config(&raw, mk.as_deref())?;

    let vt = file.vt_api_key.clone();
    let tok = file.api_token.clone();
    let states = [
        secret_store::SecretState {
            account: secret_store::ACCOUNT_VT_API_KEY,
            file_value: vt.as_deref(),
            in_store: store_has(store, secret_store::ACCOUNT_VT_API_KEY)?,
        },
        secret_store::SecretState {
            account: secret_store::ACCOUNT_API_TOKEN,
            file_value: tok.as_deref(),
            in_store: store_has(store, secret_store::ACCOUNT_API_TOKEN)?,
        },
    ];
    let plan = secret_store::plan_secret_migration(&states);
    if plan.is_empty() {
        return Ok(0);
    }

    // Write each secret and read it back before trusting it. Bail before any
    // blanking so a half-written secret never gets stripped from the file.
    for (account, value) in &plan.writes {
        store
            .set(account, value)
            .with_context(|| format!("keychain set {account}"))?;
        if store.get(account)?.as_deref() != Some(value.as_str()) {
            bail!("keychain verify failed for {account}; leaving plaintext in config.json");
        }
    }

    // Every blank_file_fields entry is now confirmed in the store (just-written
    // and verified, or pre-existing). Clear their plaintext copies.
    let mut updates = RawConfig::default();
    for account in &plan.blank_file_fields {
        match account.as_str() {
            secret_store::ACCOUNT_VT_API_KEY => updates.vt_api_key = Some(String::new()),
            secret_store::ACCOUNT_API_TOKEN => updates.api_token = Some(String::new()),
            _ => {}
        }
    }
    write_config(updates)?;
    Ok(plan.writes.len())
}

/// Startup hook: when the keychain backend is active, migrate plaintext secrets
/// into the macOS Keychain. Never fatal — a failure logs and leaves the file as
/// is so the daemon can still start from plaintext. No-op on non-macOS.
#[cfg(target_os = "macos")]
pub fn run_secret_migration(backend: SecretsBackend) {
    if backend != SecretsBackend::Keychain {
        return;
    }
    let store = secret_store::KeychainStore::new();
    match migrate_secrets_to_keychain(&store) {
        Ok(0) => {}
        Ok(n) => eprintln!("[secrets] migrated {n} key(s) to keychain"),
        Err(e) => eprintln!("[secrets] migration skipped: {e}"),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn run_secret_migration(_backend: SecretsBackend) {}

/// The live [`SecretStore`] for the active backend, or `None` when secrets live
/// in `config.json` (the `File` backend). Callers route secret writes through
/// the returned store; `None` means write secrets to the file as before.
#[cfg(target_os = "macos")]
pub fn active_secret_store(backend: SecretsBackend) -> Option<std::sync::Arc<dyn SecretStore>> {
    match backend {
        SecretsBackend::Keychain => Some(std::sync::Arc::new(secret_store::KeychainStore::new())),
        SecretsBackend::File => None,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn active_secret_store(_backend: SecretsBackend) -> Option<std::sync::Arc<dyn SecretStore>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_from<'a>(
        map: &'a HashMap<&'static str, &'static str>,
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| map.get(name).map(|s| s.to_string())
    }

    #[test]
    fn defaults_when_empty() {
        let env_map = HashMap::new();
        let cfg = resolve(RawConfig::default(), &env_from(&env_map)).unwrap();
        assert_eq!(cfg.database_path, "./data/jobs.sqlite");
        assert_eq!(cfg.http_host, "127.0.0.1");
        assert_eq!(cfg.http_port, None);
        assert!(cfg.watch_recursive);
        assert_eq!(cfg.max_scan_bytes, DEFAULT_MAX_SCAN);
        assert_eq!(cfg.max_concurrent_scans, 2);
        assert_eq!(cfg.pompelmi_socket_path, "/tmp/clamd.sock");
        assert_eq!(cfg.pompelmi_failure_mode, FailureMode::Bypass);
        assert_eq!(cfg.watcher_mode, WatcherMode::Active);
        assert!(cfg.vt_enabled);
        assert!(cfg.vt_hash_only, "hash-only (privacy) is the default");
        assert!(cfg.pompelmi_enabled);
        assert!(!cfg.config_encrypted_at_rest);
    }

    #[test]
    fn env_overlay_applies() {
        let mut env_map = HashMap::new();
        env_map.insert("WATCH_PATH", "/tmp/watch");
        env_map.insert("HTTP_PORT", "3847");
        env_map.insert("VT_ENABLED", "false");
        env_map.insert("MAX_CONCURRENT_SCANS", "5");
        let cfg = resolve(RawConfig::default(), &env_from(&env_map)).unwrap();
        assert_eq!(cfg.watch_path, "/tmp/watch");
        assert_eq!(cfg.http_port, Some(3847));
        assert!(!cfg.vt_enabled);
        assert_eq!(cfg.max_concurrent_scans, 5);
    }

    #[test]
    fn file_wins_over_env() {
        let mut env_map = HashMap::new();
        env_map.insert("HTTP_HOST", "0.0.0.0");
        let file = RawConfig {
            http_host: Some("127.0.0.1".to_string()),
            ..Default::default()
        };
        let cfg = resolve(file, &env_from(&env_map)).unwrap();
        assert_eq!(cfg.http_host, "127.0.0.1");
    }

    #[test]
    fn max_concurrent_floor_is_one() {
        let file = RawConfig {
            max_concurrent_scans: Some(0),
            ..Default::default()
        };
        let cfg = resolve(file, &env_from(&HashMap::new())).unwrap();
        assert_eq!(cfg.max_concurrent_scans, 1);
    }

    #[test]
    fn invalid_http_port_errors() {
        let file = RawConfig {
            http_port: Some(99999),
            ..Default::default()
        };
        assert!(resolve(file, &env_from(&HashMap::new())).is_err());
    }

    #[test]
    fn decode_plain_json() {
        let raw = r#"{"watchPath":"/a","httpPort":3847,"pompelmiFailureMode":"inconclusive"}"#;
        let file = decode_config(raw, None).unwrap();
        assert_eq!(file.watch_path.as_deref(), Some("/a"));
        let cfg = resolve(file, &(|_: &str| None)).unwrap();
        assert_eq!(cfg.http_port, Some(3847));
        assert_eq!(cfg.pompelmi_failure_mode, FailureMode::Inconclusive);
    }

    #[test]
    fn decode_malformed_degrades_to_defaults() {
        let file = decode_config("{ not json", None).unwrap();
        assert!(file.watch_path.is_none());
    }

    #[test]
    fn decode_encrypted_requires_key() {
        let key = "0000000000000000000000000000000000000000000000000000000000000000";
        let blob = encrypt_config_json(r#"{"watchPath":"/secret"}"#, key).unwrap();
        assert!(decode_config(&blob, None).is_err());
        let file = decode_config(&blob, Some(key)).unwrap();
        assert_eq!(file.watch_path.as_deref(), Some("/secret"));
    }

    #[test]
    fn merge_raw_overlays_only_provided_fields() {
        let mut current = RawConfig {
            watch_path: Some("/old/watch".into()),
            http_host: Some("127.0.0.1".into()),
            vt_enabled: Some(true),
            ..Default::default()
        };
        let updates = RawConfig {
            watch_path: Some("/new/watch".into()),
            api_token: Some("".into()), // explicit clear
            ..Default::default()
        };
        merge_raw(&mut current, updates);
        assert_eq!(current.watch_path.as_deref(), Some("/new/watch"));
        assert_eq!(current.api_token.as_deref(), Some("")); // cleared
        assert_eq!(current.http_host.as_deref(), Some("127.0.0.1")); // untouched
        assert_eq!(current.vt_enabled, Some(true)); // untouched
    }

    #[test]
    fn mask_secret_behaviour() {
        assert_eq!(mask_secret("", 4), "");
        assert_eq!(mask_secret("abcd", 4), "****");
        assert_eq!(mask_secret("abcdefgh", 4), "****efgh");
    }

    #[test]
    fn secrets_backend_defaults_to_file() {
        let cfg = resolve(RawConfig::default(), &env_from(&HashMap::new())).unwrap();
        assert_eq!(cfg.secrets_backend, SecretsBackend::File);
    }

    #[test]
    fn secrets_backend_from_file_field() {
        let file = RawConfig {
            secrets_backend: Some("keychain".into()),
            ..Default::default()
        };
        let cfg = resolve(file, &env_from(&HashMap::new())).unwrap();
        assert_eq!(cfg.secrets_backend, SecretsBackend::Keychain);
    }

    #[test]
    fn secrets_backend_from_env() {
        let mut env_map = HashMap::new();
        env_map.insert("SECRETS_BACKEND", "keychain");
        let cfg = resolve(RawConfig::default(), &env_from(&env_map)).unwrap();
        assert_eq!(cfg.secrets_backend, SecretsBackend::Keychain);
    }

    #[test]
    fn secrets_backend_unknown_value_is_file() {
        let file = RawConfig {
            secrets_backend: Some("vault".into()),
            ..Default::default()
        };
        let cfg = resolve(file, &env_from(&HashMap::new())).unwrap();
        assert_eq!(cfg.secrets_backend, SecretsBackend::File);
    }

    #[test]
    fn apply_secret_store_overlays_both_secrets() {
        use crate::secret_store::{MemoryStore, ACCOUNT_API_TOKEN, ACCOUNT_VT_API_KEY};
        let mut cfg = resolve(RawConfig::default(), &env_from(&HashMap::new())).unwrap();
        let store = MemoryStore::new();
        store.set(ACCOUNT_VT_API_KEY, "vt-from-keychain").unwrap();
        store.set(ACCOUNT_API_TOKEN, "tok-from-keychain").unwrap();
        apply_secret_store(&mut cfg, &store).unwrap();
        assert_eq!(cfg.vt_api_key, "vt-from-keychain");
        assert_eq!(cfg.api_token, "tok-from-keychain");
    }

    #[test]
    fn apply_secret_store_keeps_resolved_value_when_store_empty() {
        use crate::secret_store::MemoryStore;
        let file = RawConfig {
            vt_api_key: Some("vt-from-file".into()),
            api_token: Some("tok-from-file".into()),
            ..Default::default()
        };
        let mut cfg = resolve(file, &env_from(&HashMap::new())).unwrap();
        let store = MemoryStore::new(); // empty
        apply_secret_store(&mut cfg, &store).unwrap();
        assert_eq!(cfg.vt_api_key, "vt-from-file");
        assert_eq!(cfg.api_token, "tok-from-file");
    }

    #[test]
    fn apply_secret_store_wins_over_file() {
        use crate::secret_store::{MemoryStore, ACCOUNT_VT_API_KEY};
        let file = RawConfig {
            vt_api_key: Some("vt-from-file".into()),
            ..Default::default()
        };
        let mut cfg = resolve(file, &env_from(&HashMap::new())).unwrap();
        let store = MemoryStore::new();
        store.set(ACCOUNT_VT_API_KEY, "vt-from-keychain").unwrap();
        apply_secret_store(&mut cfg, &store).unwrap();
        assert_eq!(cfg.vt_api_key, "vt-from-keychain"); // keychain wins
    }

    #[test]
    fn merge_raw_overlays_secrets_backend() {
        let mut current = RawConfig::default();
        let updates = RawConfig {
            secrets_backend: Some("keychain".into()),
            ..Default::default()
        };
        merge_raw(&mut current, updates);
        assert_eq!(current.secrets_backend.as_deref(), Some("keychain"));
    }
}
