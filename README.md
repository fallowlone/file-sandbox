# FileSandbox

Automatic quarantine and VirusTotal scanning for files dropped into a watched directory.  
Catches threats before they reach your system — with a native macOS menu bar app for real-time monitoring.

---

## Features

- **Sub-5ms lockdown** — `fs.watch` (kqueue) fires instantly; file gets `chmod 0o000` + `com.apple.quarantine` xattr before anything else can run it
- **VirusTotal scanning** — uploads to VT API, polls until verdict
- **SHA-256 verdict cache** — Rust binary skips re-uploading known files (saves VT API quota)
- **Quarantine pipeline** — infected/inconclusive files stay locked; clean files restored
- **Scan cancellation** — cancel in-progress scan from menu bar; file stays in quarantine
- **macOS menu bar app** — native SwiftUI, live status per file, scanning animation, threat counter
- **Auto-start** — LaunchAgent runs daemon at login, restarts on crash
- **LaunchAgent monitor** — detects new persistence entries in `~/Library/LaunchAgents` and system dirs
- **Endpoint Security daemon** (optional) — kernel-level `AUTH_EXEC` deny for files in watch dir

---

## Architecture

```
 Drop file
     │
     ▼
 fs.watch (kqueue, ~1–5ms)
     │
     ├─ chmod 0o000          ← no read / no exec
     └─ quarantine xattr     ← Gatekeeper blocks if user tries to run

 chokidar (awaitWriteFinish, ~2s)
     │
     ├─ chmod 0o444          ← read-only for processing
     ├─ vt-cache check       ← Rust: SHA-256 lookup
     │     hit ──────────────────────────────────► use cached verdict
     │     miss
     │       │
     │       ▼
     │   VirusTotal API
     │       │  upload + poll
     │       ▼
     ├─ vt-cache store       ← Rust: persist SHA-256 → verdict
     │
     ├─ clean   ──► restore to watch dir (skip re-scan via restoringPaths set)
     └─ infected ─► keep in quarantine

 SQLite (better-sqlite3)     ← job log, survives restarts
 Express /api/jobs           ← JSON API
 SwiftUI MenuBarExtra        ← polls API every 5s
```

---

## Requirements

| Tool               | Version                    |
| ------------------ | -------------------------- |
| Node.js            | 20.6+                      |
| Rust / Cargo       | 1.70+                      |
| macOS              | 13 Ventura+ (menu bar app) |
| VirusTotal API key | Free tier works            |

---

## Installation

```bash
git clone https://github.com/your-username/file-sandbox.git
cd file-sandbox
npm install

# Build Rust verdict cache
cd vt-cache && cargo build --release && cd ..

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
  "inconclusiveRetentionDays": 0
}
```

| Field                       | Meaning                                                                                                    |
| --------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `apiToken`                  | If non-empty, HTTP API requires `Authorization: Bearer …` or `X-Filesandbox-Token` (except `/api/health`). |
| `watchRecursive`            | Watch subfolders (`false` = only direct children of `watchPath`).                                          |
| `maxScanBytes`              | Skip VT upload above this size; file stays quarantined as oversized (default 400 MiB).                     |
| `maxConcurrentScans`        | Parallel VT pipelines (minimum 1).                                                                         |
| `useSeparateVtProcess`      | Run VT upload/analysis in a child Node process.                                                            |
| `inconclusiveRetentionDays` | `0` = never auto-delete inconclusive quarantine; `N` = hourly sweep deletes after N days.                  |

Get a free VirusTotal API key at [virustotal.com](https://www.virustotal.com/gui/join-us).

> **env fallback** — Docker / CI can override: `VT_API_KEY`, `FILESANDBOX_API_TOKEN`, `WATCH_PATH`, `QUARANTINE_PATH`, `DATABASE_PATH`, `HTTP_PORT`, `HTTP_HOST`, `WATCH_RECURSIVE`, `MAX_SCAN_BYTES`, `MAX_CONCURRENT_SCANS`, `USE_SEPARATE_VT_PROCESS`, `INCONCLUSIVE_RETENTION_DAYS`. Optional: `FILESANDBOX_MASTER_KEY` (encrypt `config.json` at rest), `FILESANDBOX_ALLOW_LAN=1` (allow binding `httpHost` to non-loopback).

---

## Usage

### Development

```bash
node src/index.ts          # uses config.json
```

### Auto-start (recommended)

```bash
# Install as LaunchAgent — starts at login, restarts on crash
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

Settings has two tabs: **General** (paths, network, `apiToken`, VT key) and **Watch & scan** (`watchRecursive`, limits, child process, inconclusive retention).  
Use env `FILE_SANDBOX_PORT` if the daemon HTTP port is not `3847`.

### Docker

```bash
cp config.example.json .env.docker  # fill in keys
docker compose up
```

---

## Verdict cache (Rust)

The `vt-cache` binary computes SHA-256 of each file and caches VT verdicts locally.  
Files with the same content are never uploaded twice.

```bash
# Manual use
vt-cache/target/release/vt-cache check  /path/to/file   # → clean / infected / miss
vt-cache/target/release/vt-cache store  /path/to/file clean
vt-cache/target/release/vt-cache list
vt-cache/target/release/vt-cache clear

# Custom DB path
VT_CACHE_DB=/tmp/test.db vt-cache check /path/to/file
```

---

## Security model

| Layer                        | Mechanism                                   | Gap closed                                 |
| ---------------------------- | ------------------------------------------- | ------------------------------------------ |
| `chmod 0o000`                | Blocks all access ~1–5ms after file appears | Accidental double-click, browser auto-open |
| `com.apple.quarantine` xattr | Gatekeeper prompts before execution         | Standard delivery vectors                  |
| VirusTotal scan              | 70+ AV engines                              | Known malware signatures                   |
| Quarantine directory         | 0o444 read-only, separate path              | Lateral movement from quarantine           |
| LaunchAgent monitor          | chokidar on `~/Library/LaunchAgents`        | Persistence detection                      |
| Endpoint Security daemon     | Kernel `AUTH_EXEC` deny (optional)          | Targeted execution bypass                  |

### Endpoint Security daemon (optional)

Requires SIP disabled (dev) or [Apple ES entitlement](https://developer.apple.com/contact/request/system-extension/) (production):

```bash
cd es-daemon && bash build.sh && cd ..
sudo bash scripts/install-es-daemon.sh
```

---

## Project structure

```
file-sandbox/
├── src/                    TypeScript daemon
│   ├── index.ts            entrypoint, wires all modules
│   ├── config.ts           config.json + env var loader
│   ├── watcher.ts          fs.watch + chokidar pipeline
│   ├── virus-checker.ts    VirusTotal upload + polling
│   ├── vt-cache.ts         Rust binary wrapper
│   ├── file-mover.ts       quarantine / restore
│   ├── job-store.ts        SQLite job log
│   ├── ui-server.ts        Express REST API + HTML dashboard
│   └── launch-agent-monitor.ts  persistence detection
├── vt-cache/               Rust — SHA-256 verdict cache
├── macos-menubar/          Swift — native menu bar app
├── es-daemon/              Swift — Endpoint Security daemon
├── scripts/                install / uninstall helpers
├── docker-compose.yml
└── config.example.json     copy → config.json, fill in keys
```

---

## License

MIT

## Local scanner (pompelmi / ClamAV)

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
