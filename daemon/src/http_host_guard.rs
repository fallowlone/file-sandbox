//! Port of `src/http-host-guard.ts`.
//!
//! Refuses to bind the HTTP UI to a non-loopback address unless the operator
//! opts in via `FILESANDBOX_ALLOW_LAN=1`.

const LOOPBACK: [&str; 4] = ["127.0.0.1", "::1", "localhost", "::ffff:127.0.0.1"];

/// True when binding `host` would expose the API beyond loopback.
pub fn is_non_loopback_binding(host: &str) -> bool {
    let h = host.trim().to_lowercase();
    if h == "0.0.0.0" || h == "::" || h == "*" || h.is_empty() {
        return true;
    }
    !LOOPBACK.contains(&h.as_str())
}

/// Returns `Err` with an operator-facing message when the host is non-loopback
/// and `FILESANDBOX_ALLOW_LAN` is not `1`. The caller decides how to abort
/// (the TS version called `process.exit(1)`).
pub fn assert_safe_http_host(host: &str) -> Result<(), String> {
    if !is_non_loopback_binding(host) {
        return Ok(());
    }
    let allow = std::env::var("FILESANDBOX_ALLOW_LAN")
        .map(|v| v.trim() == "1")
        .unwrap_or(false);
    if allow {
        eprintln!(
            "[security] HTTP bound to {host} — API is reachable on the network. Set FILESANDBOX_API_TOKEN and never expose without reverse-proxy auth."
        );
        return Ok(());
    }
    Err(format!(
        "[security] Refusing to bind HTTP to {host} (non-loopback). Set HTTP_HOST to 127.0.0.1 or set FILESANDBOX_ALLOW_LAN=1 if you accept the risk."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts_are_safe() {
        for h in ["127.0.0.1", "::1", "localhost", "::ffff:127.0.0.1", " LocalHost "] {
            assert!(!is_non_loopback_binding(h), "{h} should be loopback");
        }
    }

    #[test]
    fn wildcard_and_lan_hosts_are_non_loopback() {
        for h in ["0.0.0.0", "::", "*", "", "192.168.1.10", "10.0.0.5"] {
            assert!(is_non_loopback_binding(h), "{h} should be non-loopback");
        }
    }
}
