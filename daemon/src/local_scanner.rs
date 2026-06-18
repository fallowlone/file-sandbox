//! Port of `src/local-scanner.ts`.
//!
//! The TS version delegated to the `pompelmi` npm package, which itself spoke
//! the clamd INSTREAM protocol over a UNIX socket. This port talks that
//! protocol directly — no third-party scanner dependency — preserving the same
//! `clean | malicious | error` verdict semantics and the PING/PONG liveness
//! probe.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

const INSTREAM_CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalVerdict {
    Clean,
    Malicious,
    Error,
}

impl LocalVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            LocalVerdict::Clean => "clean",
            LocalVerdict::Malicious => "malicious",
            LocalVerdict::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalScanResult {
    pub verdict: LocalVerdict,
    pub message: String,
}

/// Map a raw clamd reply to a verdict. `FOUND` wins over `OK`; anything else
/// (including `ERROR`) is treated as a scan error.
pub fn classify(raw: &str) -> LocalVerdict {
    if raw.contains("FOUND") {
        LocalVerdict::Malicious
    } else if raw.contains("OK") {
        LocalVerdict::Clean
    } else {
        LocalVerdict::Error
    }
}

pub struct LocalScanner {
    socket_path: String,
}

impl LocalScanner {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Liveness check: the socket must exist AND clamd must answer PING with
    /// PONG. A stale/orphaned socket file would pass an existence check but
    /// fail the handshake, which would otherwise let a dead scanner silently
    /// bypass scanning.
    pub async fn probe(socket_path: &Path, timeout_ms: u64) -> Result<()> {
        tokio::fs::metadata(socket_path)
            .await
            .with_context(|| format!("clamd socket unreachable at {}", socket_path.display()))?;

        let dur = Duration::from_millis(timeout_ms);
        let handshake = async {
            let mut sock = UnixStream::connect(socket_path)
                .await
                .with_context(|| format!("clamd PING failed at {}", socket_path.display()))?;
            sock.write_all(b"nPING\n").await?;
            let mut response = String::new();
            let mut buf = [0u8; 256];
            loop {
                let n = sock.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                response.push_str(&String::from_utf8_lossy(&buf[..n]));
                if response.contains("PONG") {
                    return Ok(());
                }
            }
            if response.contains("PONG") {
                Ok(())
            } else {
                Err(anyhow!(
                    "clamd did not answer PING at {} (got: {response:?})",
                    socket_path.display()
                ))
            }
        };

        match timeout(dur, handshake).await {
            Ok(res) => res,
            Err(_) => Err(anyhow!(
                "clamd PING timed out after {timeout_ms}ms at {}",
                socket_path.display()
            )),
        }
    }

    pub async fn check(&self, file_path: &Path) -> LocalScanResult {
        match self.scan_instream(file_path).await {
            Ok(raw) => {
                let verdict = classify(&raw);
                let message = if verdict == LocalVerdict::Error {
                    format!("clamd ScanError on {}", file_path.display())
                } else {
                    format!("clamd {}", verdict.as_str())
                };
                LocalScanResult { verdict, message }
            }
            Err(e) => LocalScanResult {
                verdict: LocalVerdict::Error,
                message: format!("clamd exception: {e}"),
            },
        }
    }

    /// Stream the file to clamd via the INSTREAM command and return its raw reply.
    async fn scan_instream(&self, file_path: &Path) -> Result<String> {
        let bytes = tokio::fs::read(file_path)
            .await
            .with_context(|| format!("read {} for scan", file_path.display()))?;

        let mut sock = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("connect clamd at {}", self.socket_path))?;

        sock.write_all(b"zINSTREAM\0").await?;
        for chunk in bytes.chunks(INSTREAM_CHUNK) {
            sock.write_all(&(chunk.len() as u32).to_be_bytes()).await?;
            sock.write_all(chunk).await?;
        }
        // Zero-length chunk terminates the stream.
        sock.write_all(&0u32.to_be_bytes()).await?;
        sock.flush().await?;

        let mut reply = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            let n = sock.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            reply.extend_from_slice(&buf[..n]);
            // clamd terminates a z-command reply with a NUL byte.
            if reply.contains(&0) {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&reply)
            .trim_end_matches('\0')
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    #[test]
    fn classify_maps_clamd_replies() {
        assert_eq!(classify("stream: OK"), LocalVerdict::Clean);
        assert_eq!(
            classify("stream: Eicar-Test-Signature FOUND"),
            LocalVerdict::Malicious
        );
        assert_eq!(
            classify("stream: INSTREAM size limit exceeded ERROR"),
            LocalVerdict::Error
        );
        assert_eq!(classify("garbage"), LocalVerdict::Error);
    }

    #[tokio::test]
    async fn probe_rejects_when_socket_missing() {
        let err = LocalScanner::probe(
            Path::new("/tmp/definitely-not-a-real-socket-987654.sock"),
            300,
        )
        .await
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("unreachable"), "got {msg}");
    }

    #[tokio::test]
    async fn probe_rejects_stale_socket_that_never_answers() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("stale.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        // Accept connections but never speak the clamd protocol.
        tokio::spawn(async move {
            while let Ok((_conn, _)) = listener.accept().await {
                // hold the connection open, say nothing
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });

        let err = LocalScanner::probe(&sock_path, 300).await.unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("timed out") || msg.contains("did not answer"),
            "got {msg}"
        );
    }

    #[tokio::test]
    async fn probe_resolves_when_listener_answers_pong() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("pong.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        tokio::spawn(async move {
            if let Ok((mut conn, _)) = listener.accept().await {
                let mut buf = [0u8; 64];
                let _ = conn.read(&mut buf).await;
                let _ = conn.write_all(b"PONG\n").await;
            }
        });

        LocalScanner::probe(&sock_path, 1000).await.unwrap();
    }

    /// Minimal fake clamd that consumes an INSTREAM upload and replies `reply`.
    async fn fake_clamd(sock_path: std::path::PathBuf, reply: &'static [u8]) {
        let listener = UnixListener::bind(&sock_path).unwrap();
        tokio::spawn(async move {
            if let Ok((mut conn, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                // Drain INSTREAM command + chunks until the connection goes quiet.
                loop {
                    match timeout(Duration::from_millis(200), conn.read(&mut buf)).await {
                        Ok(Ok(n)) if n > 0 => continue,
                        _ => break,
                    }
                }
                let _ = conn.write_all(reply).await;
            }
        });
    }

    #[tokio::test]
    async fn check_returns_clean_on_ok_reply() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("clamd-ok.sock");
        fake_clamd(sock_path.clone(), b"stream: OK\0").await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        let file = dir.path().join("sample.bin");
        tokio::fs::write(&file, b"hello").await.unwrap();

        let scanner = LocalScanner::new(sock_path.to_string_lossy().to_string());
        let res = scanner.check(&file).await;
        assert_eq!(res.verdict, LocalVerdict::Clean, "msg: {}", res.message);
    }

    #[tokio::test]
    async fn check_returns_malicious_on_found_reply() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("clamd-found.sock");
        fake_clamd(sock_path.clone(), b"stream: Eicar-Test-Signature FOUND\0").await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        let file = dir.path().join("evil.bin");
        tokio::fs::write(&file, b"X5O!P%@AP").await.unwrap();

        let scanner = LocalScanner::new(sock_path.to_string_lossy().to_string());
        let res = scanner.check(&file).await;
        assert_eq!(res.verdict, LocalVerdict::Malicious, "msg: {}", res.message);
    }

    #[tokio::test]
    async fn check_returns_error_when_socket_absent() {
        let scanner = LocalScanner::new("/tmp/no-such-clamd-123456.sock".to_string());
        let res = scanner.check(Path::new("/etc/hostname")).await;
        assert_eq!(res.verdict, LocalVerdict::Error);
    }
}
