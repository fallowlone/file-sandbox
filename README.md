# FileSandbox

Automatic quarantine and VirusTotal scanning for files dropped into a watched directory.  
Catches threats before they reach your system — with a native macOS menu bar app for real-time monitoring.

The daemon is a single native **Rust** binary (`file-sandbox-daemon`). No Node.js runtime required.

---

## Features

- **Sub-5ms lockdown** — the file watcher fires instantly; file gets `chmod 0o000` + `com.apple.quarantine` xattr before anything else can run it
- **Local ClamAV scan** — every quarantined file is streamed to `clamd` before VirusTotal
- **VirusTotal scanning** — uploads to VT API, polls until verdict
- **SHA-256 verdict cache** — in-process cache skips re-uploading known files (saves VT API quota)
- **Quarantine pipeline** — infected/inconclusive files stay locked; clean files restored
- **Scan cancellation** — cancel in-progress scan from menu bar; file stays in quarantine
- **macOS menu bar app** — native SwiftUI, live status per file, scanning animation, threat counter
- **Auto-start** — LaunchAgent runs the binary at login, restarts on crash
- **LaunchAgent monitor** — detects new persistence entries in `~/Library/LaunchAgents` and system dirs
- **Endpoint Security daemon** (optional) — kernel-level `AUTH_EXEC` deny for files in watch dir

---

## Architecture

```
 Drop file
     │
     ▼
 file watcher (notify, ~1–5ms)
     │
     ├─ chmod 0o000          ← no read / no exec
     └─ quarantine xattr     ← Gatekeeper blocks if user tries to run

 stability gate (~2s, size stable)
     │
     ├─ chmod 0o444          ← read-only for processing
     ├─ move to quarantine
     ├─ local clamd scan     ← INSTREAM over unix socket
     ├─ vt-cache check       ← in-process SHA-256 lookup
     │     hit ──────────────────────────────────► use cached verdict
     │     miss
     │       │
     │       ▼
     │   VirusTotal API
     │       │  upload + poll
     │       ▼
     ├─ vt-cache store       ← persist SHA-256 → verdict
     │
     ├─ clean   ──► restore to watch dir
     └─ infected ─► keep in quarantine

 SQLite (rusqlite)           ← job log, survives restarts
 axum /api/jobs              ← JSON API + HTML dashboard
 SwiftUI MenuBarExtra        ← polls API every 5s
```

---

## Requirements

| Tool               | Version                    |
| ------------------ | -------------------------- |
| Rust / Cargo       | 1.70+                      |
| ClamAV (`clamd`)   | local scanner (optional)   |
| macOS              | 13 Ventura+ (menu bar app) |
| VirusTotal API key | Free tier works            |

---

## Installation

```bash
git clone https://github.com/your-username/file-sandbox.git
cd file-sandbox

# Build the daemon
cd daemon && cargo build --release && cd ..

# Build menu bar app
cd macos-menubar && bash build.sh && cd ..
```

---

## Configuration

Copy the example config and fill in your details:

```bash
cp config.example.json config.json
```

Full template (same as `config.example.json`):

```json
{
  "vtApiKey": "YOUR_VIRUSTOTAL_API_KEY",
  "apiToken": "",
  "watchPath": "/Users/yourname/Downloads",
  "quarantinePath": "/Users/yourname/.file-sandbox/quarantine",
  "databasePath": "/Users/yourname/.file-sandbox/jobs.sqlite",
  "httpPort": 3847,
  "httpHost": "127.0.0.1",
  "watchRecursive": true,
  "maxScanBytes": 419430400,
  "maxConcurrentScans": 2,
  "useSeparateVtProcess": false,
  "inconclusiveRetentionDays": 0,
  "secretsBackend": "file"
}
```

| Field                       | Meaning                                                                                                    |
| --------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `apiToken`                  | If non-empty, HTTP API requires `Authorization: Bearer …` or `X-Filesandbox-Token` (except `/api/health`). |
| `watchRecursive`            | Watch subfolders (`false` = only direct children of `watchPath`).                                          |
| `maxScanBytes`              | Skip VT upload above this size; file stays quarantined as oversized (default 400 MiB).                     |
| `maxConcurrentScans`        | Parallel VT pipelines (minimum 1).                                                                         |
| `useSeparateVtProcess`      | No-op in the Rust daemon (VT runs in-process). Kept for config compatibility.                              |
| `inconclusiveRetentionDays` | `0` = never auto-delete inconclusive quarantine; `N` = hourly sweep deletes after N days.                  |
| `secretsBackend`            | `file` (default) keeps secrets in `config.json`; `keychain` stores `vtApiKey`/`apiToken` in the macOS Keychain (migrated on startup). See [docs/security-hardening.md](docs/security-hardening.md). |

Get a free VirusTotal API key at [virustotal.com](https://www.virustotal.com/gui/join-us).

> **env fallback** — CI can override: `VT_API_KEY`, `FILESANDBOX_API_TOKEN`, `WATCH_PATH`, `QUARANTINE_PATH`, `DATABASE_PATH`, `HTTP_PORT`, `HTTP_HOST`, `WATCH_RECURSIVE`, `MAX_SCAN_BYTES`, `MAX_CONCURRENT_SCANS`, `INCONCLUSIVE_RETENTION_DAYS`. Optional: `FILESANDBOX_MASTER_KEY` (encrypt `config.json` at rest), `FILESANDBOX_ALLOW_LAN=1` (allow binding `httpHost` to non-loopback), `SECRETS_BACKEND=keychain` (store secrets in the macOS Keychain).

---

## Usage

### Development

```bash
./daemon/target/release/file-sandbox-daemon   # uses config.json in cwd
```

### Auto-start (recommended)

```bash
# Install as LaunchAgent — builds the release binary if needed,
# starts at login, restarts on crash
bash scripts/install-launchagent.sh

# Logs
tail -f logs/filesandbox.log

# Stop / start
launchctl stop dev.artemmac.filesandbox
launchctl start dev.artemmac.filesandbox

# Uninstall
bash scripts/uninstall-launchagent.sh
```

### Menu bar app

Rebuild after pulling (bundles your current Swift sources):

```bash
cd macos-menubar && bash build.sh && cd ..
open macos-menubar/FileSandboxMenuBar.app
```

Two tabs: **Jobs** (live per-file status) and **Settings** (paths, scanners, limits, tokens).

---

## Verdict cache

The daemon computes the SHA-256 of each file and caches VT verdicts in a local SQLite DB
(`$VT_CACHE_DB` or `$HOME/.config/filesandbox/vt-cache.db`). Files with the same content are
never uploaded twice. The cache is fully in-process — no separate binary to run.

---

## Security model

| Layer                        | Mechanism                                   | Gap closed                                 |
| ---------------------------- | ------------------------------------------- | ------------------------------------------ |
| `chmod 0o000`                | Blocks all access ~1–5ms after file appears | Accidental double-click, browser auto-open |
| `com.apple.quarantine` xattr | Gatekeeper prompts before execution         | Standard delivery vectors                  |
| Local ClamAV scan            | `clamd` signature scan before VT            | Known malware, offline                     |
| VirusTotal scan              | 70+ AV engines                              | Known malware signatures                   |
| Quarantine directory         | 0o444 read-only, separate path              | Lateral movement from quarantine           |
| LaunchAgent monitor          | watches `~/Library/LaunchAgents`            | Persistence detection                      |
| Endpoint Security daemon     | Kernel `AUTH_EXEC` deny (optional)          | Targeted execution bypass                  |

### Endpoint Security daemon (optional)

Requires SIP disabled (dev) or [Apple ES entitlement](https://developer.apple.com/contact/request/system-extension/) (production):

```bash
cd es-daemon && bash build.sh && cd ..
sudo bash scripts/install-es-daemon.sh
```

---

## Local scanner (ClamAV)

By default the daemon runs a local ClamAV scan on every quarantined file before sending it to VirusTotal. Install ClamAV on the host:

```bash
brew install clamav
freshclam
# Edit /opt/homebrew/etc/clamav/clamd.conf:
#   uncomment LocalSocket /tmp/clamd.sock
#   set MaxFileSize 4000M
#   set MaxScanSize 4000M
#   set StreamMaxLength 4000M
brew services start clamav
```

To disable the local scanner, set `pompelmiEnabled: false` in `config.json`. To require strict failure handling instead of falling back to VT on local-scan errors, set `pompelmiFailureMode: "inconclusive"`.

The daemon refuses to start if `pompelmiEnabled=true` and the configured socket is unreachable. Check `/api/health` for `localScanner.socketReachable`.

---

## Project structure

```
file-sandbox/
├── daemon/                 Rust daemon
│   └── src/
│       ├── main.rs             entrypoint, wires all modules
│       ├── config.rs           config.json + env var loader
│       ├── watcher.rs          notify + stability pipeline
│       ├── virus_checker.rs    VirusTotal upload + polling
│       ├── vt_cache.rs         in-process SHA-256 verdict cache
│       ├── local_scanner.rs    clamd INSTREAM client
│       ├── file_mover.rs       quarantine / restore
│       ├── job_store.rs        SQLite job log
│       ├── ui_server.rs        axum REST API + HTML dashboard
│       └── launch_agent_monitor.rs  persistence detection
├── macos-menubar/          Swift — native menu bar app
├── es-daemon/              Swift — Endpoint Security daemon
├── scripts/                install / uninstall helpers
└── config.example.json     copy → config.json, fill in keys
```

---

## License

MIT
