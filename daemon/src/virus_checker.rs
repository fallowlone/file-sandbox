//! Port of `src/virus-checker.ts`.
//!
//! Uploads a file to VirusTotal and polls the analysis until it settles,
//! returning a `clean | infected | inconclusive | oversized` verdict.
//!
//! Differences from the TS version, by design:
//!   * `useSeparateVtProcess` is a no-op. The TS daemon forked a child Node
//!     process so the `--experimental-strip-types` parse + the file bytes lived
//!     in throwaway memory; a native binary has no such reason, so both paths
//!     run in-process. The flag is still accepted for config parity.
//!   * Upload retry policy matches the TS default (`shouldRetryUpload` → false,
//!     zero backoff). The loop structure is preserved for future tuning.

use std::path::Path;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

const API_URL: &str = "https://www.virustotal.com/api/v3";
const MAX_UPLOAD_ATTEMPTS: u32 = 4;
const DEFAULT_MAX_BYTES: u64 = 400 * 1024 * 1024;
/// VT's standard `/files` endpoint rejects uploads larger than 32 MiB with HTTP
/// 413. Files above this size use the `/files/upload_url` large-file flow.
const LARGE_FILE_THRESHOLD: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirusVerdict {
    Clean,
    Infected,
    Inconclusive,
    Oversized,
}

impl VirusVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            VirusVerdict::Clean => "clean",
            VirusVerdict::Infected => "infected",
            VirusVerdict::Inconclusive => "inconclusive",
            VirusVerdict::Oversized => "oversized",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirusCheckResult {
    pub verdict: VirusVerdict,
    pub message: String,
    pub malicious: Option<u64>,
    pub suspicious: Option<u64>,
}

impl VirusCheckResult {
    fn inconclusive(message: impl Into<String>) -> Self {
        Self {
            verdict: VirusVerdict::Inconclusive,
            message: message.into(),
            malicious: None,
            suspicious: None,
        }
    }
}

// ── VT JSON shapes ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct VtUploadResponse {
    data: Option<VtUploadData>,
    error: Option<VtError>,
}

#[derive(Deserialize)]
struct VtUploadData {
    id: String,
}

/// Response of `GET /files/upload_url` — `data` is the one-time large-file URL.
#[derive(Deserialize)]
struct VtUploadUrlResponse {
    data: Option<String>,
    error: Option<VtError>,
}

#[derive(Deserialize)]
struct VtError {
    message: Option<String>,
}

#[derive(Deserialize)]
struct VtAnalysisResponse {
    data: VtAnalysisData,
}

#[derive(Deserialize)]
struct VtAnalysisData {
    attributes: VtAnalysisAttributes,
}

#[derive(Deserialize)]
struct VtAnalysisAttributes {
    status: String,
    #[serde(default)]
    stats: VtStats,
}

#[derive(Deserialize, Default)]
struct VtStats {
    #[serde(default)]
    malicious: u64,
    #[serde(default)]
    suspicious: u64,
    #[serde(default)]
    harmless: u64,
    #[serde(default)]
    undetected: u64,
}

/// VT pipeline stage, surfaced to the caller so the job's `scan_stage` can be
/// updated (mirrors the TS `onStage` callback values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtStage {
    Upload,
    Poll,
}

/// Map a completed analysis's stats to a verdict. Extracted so the decision
/// logic is testable without hitting the network.
pub fn verdict_from_stats(
    malicious: u64,
    suspicious: u64,
    harmless: u64,
    undetected: u64,
) -> VirusCheckResult {
    let total = malicious + suspicious + harmless + undetected;
    if malicious > 0 || suspicious > 0 {
        VirusCheckResult {
            verdict: VirusVerdict::Infected,
            message: format!("Threats: malicious={malicious}, suspicious={suspicious} (engines reporting: {total})"),
            malicious: Some(malicious),
            suspicious: Some(suspicious),
        }
    } else {
        VirusCheckResult {
            verdict: VirusVerdict::Clean,
            message: format!("No malicious or suspicious flags ({total} engines with verdicts)"),
            malicious: Some(0),
            suspicious: Some(0),
        }
    }
}

/// Retry policy mirror of the TS default — never retry (legacy behavior).
fn should_retry_upload() -> bool {
    false
}

/// Whether a file of `len` bytes must use VT's large-file upload flow.
fn use_upload_url(len: u64) -> bool {
    len > LARGE_FILE_THRESHOLD
}

/// Fetch a one-time upload URL for files larger than 32 MiB.
/// `None` = cancelled; `Some(Err)` = failure detail; `Some(Ok)` = the URL.
async fn fetch_upload_url(
    client: &Client,
    api_key: &str,
    cancel: Option<&CancellationToken>,
) -> Option<Result<String, String>> {
    let send = client
        .get(format!("{API_URL}/files/upload_url"))
        .header("x-apikey", api_key)
        .send();

    match cancellable(send, cancel).await {
        None => None,
        Some(Err(e)) => Some(Err(format!("upload_url network error: {e}"))),
        Some(Ok(r)) => {
            if !r.status().is_success() {
                let status = r.status().as_u16();
                let body = r.text().await.unwrap_or_default();
                let snippet: String = body.chars().take(300).collect();
                return Some(Err(format!("upload_url HTTP {status}: {snippet}")));
            }
            match r.json::<VtUploadUrlResponse>().await {
                Ok(j) => {
                    if let Some(err) = j.error {
                        return Some(Err(format!(
                            "upload_url API error: {}",
                            err.message.unwrap_or_else(|| "unknown".into())
                        )));
                    }
                    match j.data {
                        Some(u) => Some(Ok(u)),
                        None => Some(Err("upload_url response had no data".into())),
                    }
                }
                Err(_) => Some(Err("invalid JSON in upload_url response".into())),
            }
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

/// Core VT scan. Respects `max_bytes` before reading the whole file into RAM.
pub async fn virus_check_file(
    api_key: &str,
    path: &Path,
    cancel: Option<&CancellationToken>,
    max_bytes: u64,
    mut on_stage: impl FnMut(VtStage),
) -> VirusCheckResult {
    // Size guard.
    match tokio::fs::metadata(path).await {
        Ok(meta) if meta.is_file() && meta.len() > max_bytes => {
            return VirusCheckResult {
                verdict: VirusVerdict::Oversized,
                message: format!(
                    "File exceeds scan limit ({max_bytes} bytes); not uploaded to VirusTotal. You can restore or delete from the UI."
                ),
                malicious: None,
                suspicious: None,
            };
        }
        Ok(_) => {}
        Err(_) => return VirusCheckResult::inconclusive("Failed to stat file before scan"),
    }

    let file = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(_) => return VirusCheckResult::inconclusive("Failed to read file for upload"),
    };

    let client = Client::new();
    let mut last_failure: Option<VirusCheckResult> = None;
    let mut analysis_id: Option<String> = None;
    let large = use_upload_url(file.len() as u64);

    for attempt in 1..=MAX_UPLOAD_ATTEMPTS {
        on_stage(VtStage::Upload);

        // Files over 32 MiB must go through a one-time URL; `/files` 413s them.
        let target_url = if large {
            match fetch_upload_url(&client, api_key, cancel).await {
                None => return VirusCheckResult::inconclusive("Cancelled by user"),
                Some(Err(msg)) => {
                    last_failure = Some(VirusCheckResult::inconclusive(format!(
                        "{msg} (attempt {attempt}/{MAX_UPLOAD_ATTEMPTS})"
                    )));
                    if !should_retry_upload() {
                        break;
                    }
                    continue;
                }
                Some(Ok(url)) => url,
            }
        } else {
            format!("{API_URL}/files")
        };

        let part = reqwest::multipart::Part::bytes(file.clone()).file_name("file");
        let form = reqwest::multipart::Form::new().part("file", part);
        let send = client
            .post(&target_url)
            .header("x-apikey", api_key)
            .multipart(form)
            .send();

        let resp = match cancellable(send, cancel).await {
            None => return VirusCheckResult::inconclusive("Cancelled by user"),
            Some(Err(e)) => {
                last_failure = Some(VirusCheckResult::inconclusive(format!(
                    "Upload network error (attempt {attempt}/{MAX_UPLOAD_ATTEMPTS}): {e}"
                )));
                if !should_retry_upload() {
                    break;
                }
                continue;
            }
            Some(Ok(r)) => r,
        };

        if resp.status().is_success() {
            match resp.json::<VtUploadResponse>().await {
                Ok(json) => {
                    if let Some(err) = json.error {
                        return VirusCheckResult::inconclusive(format!(
                            "Upload API error: {}",
                            err.message.unwrap_or_else(|| "unknown".into())
                        ));
                    }
                    match json.data.map(|d| d.id) {
                        Some(id) => {
                            analysis_id = Some(id);
                            break;
                        }
                        None => {
                            return VirusCheckResult::inconclusive(
                                "No analysis id in upload response",
                            )
                        }
                    }
                }
                Err(_) => return VirusCheckResult::inconclusive("Invalid JSON in upload response"),
            }
        }

        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(500).collect();
        last_failure = Some(VirusCheckResult::inconclusive(format!(
            "Upload failed HTTP {status} (attempt {attempt}/{MAX_UPLOAD_ATTEMPTS}): {snippet}"
        )));
        if !should_retry_upload() {
            break;
        }
    }

    let analysis_id = match analysis_id {
        Some(id) => id,
        None => {
            return last_failure
                .unwrap_or_else(|| VirusCheckResult::inconclusive("Upload failed with no details"))
        }
    };

    let max_polls = env_u64("VT_MAX_POLLS", 20);
    let poll_ms = env_u64("VT_POLL_INTERVAL_MS", 15000);

    on_stage(VtStage::Poll);

    for _ in 0..max_polls {
        if sleep_or_cancel(poll_ms, cancel).await.is_err() {
            return VirusCheckResult::inconclusive("Cancelled by user");
        }

        let get = client
            .get(format!("{API_URL}/analyses/{analysis_id}"))
            .header("x-apikey", api_key)
            .send();

        let resp = match cancellable(get, cancel).await {
            None => return VirusCheckResult::inconclusive("Cancelled by user"),
            Some(Err(e)) => {
                return VirusCheckResult::inconclusive(format!("Analysis poll network error: {e}"))
            }
            Some(Ok(r)) => r,
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(500).collect();
            return VirusCheckResult::inconclusive(format!(
                "Analysis poll HTTP {status}: {snippet}"
            ));
        }

        let parsed = match resp.json::<VtAnalysisResponse>().await {
            Ok(p) => p,
            Err(_) => return VirusCheckResult::inconclusive("Invalid JSON in analysis response"),
        };

        match parsed.data.attributes.status.as_str() {
            "queued" | "in-progress" => continue,
            "completed" => {
                let s = parsed.data.attributes.stats;
                return verdict_from_stats(s.malicious, s.suspicious, s.harmless, s.undetected);
            }
            other => {
                return VirusCheckResult::inconclusive(format!(
                    "Unexpected analysis status: {other}"
                ))
            }
        }
    }

    VirusCheckResult::inconclusive(format!(
        "Polling timeout after {max_polls} attempts ({poll_ms}ms interval)"
    ))
}

/// Await `fut`, or return `None` if the cancellation token fires first.
async fn cancellable<F: std::future::Future>(
    fut: F,
    cancel: Option<&CancellationToken>,
) -> Option<F::Output> {
    match cancel {
        Some(token) => tokio::select! {
            out = fut => Some(out),
            _ = token.cancelled() => None,
        },
        None => Some(fut.await),
    }
}

/// Sleep `ms`, or return `Err(())` if cancelled meanwhile.
async fn sleep_or_cancel(ms: u64, cancel: Option<&CancellationToken>) -> Result<(), ()> {
    let dur = Duration::from_millis(ms);
    match cancel {
        Some(token) => tokio::select! {
            _ = sleep(dur) => Ok(()),
            _ = token.cancelled() => Err(()),
        },
        None => {
            sleep(dur).await;
            Ok(())
        }
    }
}

pub struct VirusChecker {
    api_key: String,
    max_scan_bytes: u64,
    /// Accepted for config parity; the Rust port always scans in-process.
    #[allow(dead_code)]
    use_separate_vt_process: bool,
}

impl VirusChecker {
    pub fn new(
        api_key: impl Into<String>,
        max_scan_bytes: Option<u64>,
        use_separate_vt_process: bool,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            max_scan_bytes: max_scan_bytes.unwrap_or(DEFAULT_MAX_BYTES),
            use_separate_vt_process,
        }
    }

    pub async fn check(
        &self,
        path: &Path,
        cancel: Option<&CancellationToken>,
        on_stage: impl FnMut(VtStage),
    ) -> VirusCheckResult {
        virus_check_file(&self.api_key, path, cancel, self.max_scan_bytes, on_stage).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_from_stats_flags_threats() {
        let r = verdict_from_stats(3, 1, 10, 50);
        assert_eq!(r.verdict, VirusVerdict::Infected);
        assert_eq!(r.malicious, Some(3));
        assert_eq!(r.suspicious, Some(1));
        assert!(r.message.contains("malicious=3"));
        assert!(r.message.contains("engines reporting: 64"));
    }

    #[test]
    fn verdict_from_stats_clean_when_no_threats() {
        let r = verdict_from_stats(0, 0, 12, 40);
        assert_eq!(r.verdict, VirusVerdict::Clean);
        assert_eq!(r.malicious, Some(0));
        assert!(r.message.contains("52 engines"));
    }

    #[test]
    fn use_upload_url_switches_at_32_mib() {
        assert!(!use_upload_url(0));
        assert!(!use_upload_url(32 * 1024 * 1024)); // exactly 32 MiB → standard endpoint
        assert!(use_upload_url(32 * 1024 * 1024 + 1)); // one byte over → large-file flow
        assert!(use_upload_url(200 * 1024 * 1024));
    }

    #[tokio::test]
    async fn oversized_file_short_circuits_before_upload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.bin");
        tokio::fs::write(&file, vec![0u8; 1024]).await.unwrap();

        let res = virus_check_file("fake-key", &file, None, 100, |_| {}).await;
        assert_eq!(res.verdict, VirusVerdict::Oversized);
        assert!(res.message.contains("exceeds scan limit"));
    }

    #[tokio::test]
    async fn missing_file_is_inconclusive() {
        let res = virus_check_file(
            "fake-key",
            Path::new("/no/such/file/abc"),
            None,
            1000,
            |_| {},
        )
        .await;
        assert_eq!(res.verdict, VirusVerdict::Inconclusive);
        assert!(res.message.contains("Failed to stat file"));
    }

    #[tokio::test]
    async fn precancelled_token_yields_cancelled_message() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ok.bin");
        tokio::fs::write(&file, b"small").await.unwrap();

        let token = CancellationToken::new();
        token.cancel(); // already cancelled before any network call

        let res = virus_check_file("fake-key", &file, Some(&token), 1_000_000, |_| {}).await;
        assert_eq!(res.verdict, VirusVerdict::Inconclusive);
        assert_eq!(res.message, "Cancelled by user");
    }
}
