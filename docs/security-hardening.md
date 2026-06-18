# Security hardening (macOS Seatbelt, Linux seccomp, egress, split)

Threat-model gaps the app does **not** mitigate (same-UID trust, HTTP surface, VT limits, races, etc.) are summarized in [security-gaps.md](security-gaps.md) (RU).

## Conflict: strict isolation vs VirusTotal

Process that **uploads bytes to VirusTotal needs outbound HTTPS**. A container or process with **literally no egress** cannot call the VT API from the same runtime. Practical patterns:

1. **Single process (this repo default)** — watcher reads quarantined file, POSTs to VT. Isolation = separate FS path + `chmod` + no execution of samples. Egress exists for Node only.
2. **Split (stronger)** — **Stage A (no network):** move file to quarantine, optional hash/locally safe transforms, append row to shared DB. **Stage B (network):** separate service/user/container reads quarantine path from DB, uploads to VT, writes verdict. Malware in file never touches the network-capable stage unless that stage reads bytes (it still does not _execute_ the sample).
3. **Egress allowlist** — Linux: iptables/nftables or Cilium policy allowing only `www.virustotal.com:443` from the uploader. Docker alone does not provide fine-grained DNS allowlists; use orchestration or host firewall.

## macOS Seatbelt (App Sandbox)

Node.js has **no supported first-class Seatbelt API**. Options:

- **Don’t execute** the watched files; keep current model (data-only quarantine).
- **Small native helper** (Swift/ObjC) that calls `sandbox_init` and only passes file descriptors/paths according to entitlements — out of scope for this TypeScript repo but the right place for true sandbox rules.
- Historical `sandbox-exec` CLI is **not a supported path** on current macOS for new designs.

## Linux seccomp

Docker’s **default seccomp profile** already blocks many dangerous syscalls. Tightening further for Node + `better-sqlite3` + `fetch` is brittle (allow-list must include `epoll`, `futex`, socket syscalls, etc.).

Recommended steps:

- Use **`security_opt: [no-new-privileges:true]`** and **`cap_drop: [ALL]`** (see `docker-compose.yml`).
- Optionally pass a **custom seccomp JSON** only after auditing with your kernel and Node version:  
  `docker run --security-opt seccomp=/path/to/profile.json ...`

## Container “no egress” for the sample

If the **ingest** container has `--network none`, it cannot talk to VT. Use **split** (above): ingest container writes to a **shared volume**; **uploader** container has network and only reads files that are already in quarantine.

## UI / DB / queue

- SQLite (`DATABASE_PATH`) is the **durable job log** for the UI and ops.
- Bind UI to **localhost** (`httpHost` / `HTTP_HOST`). Binding to `0.0.0.0` or another non-loopback address requires **`FILESANDBOX_ALLOW_LAN=1`** (explicit opt-in).
- Set **`apiToken`** in `config.json` or **`FILESANDBOX_API_TOKEN`** so clients must send `Authorization: Bearer …` or `X-Filesandbox-Token` (health endpoints stay public).
- Optional **`FILESANDBOX_MASTER_KEY`**: encrypts `config.json` at rest (AES-256-GCM). SQLite is still a plain file unless you add SQLCipher or OS-level encryption.
- Do not expose VT API keys in the UI or client-side code; the daemon masks them in `GET /api/config`.

## Secrets in the Keychain (macOS)

Set **`secretsBackend: "keychain"`** in `config.json` (or **`SECRETS_BACKEND=keychain`**) to keep `vtApiKey` and `apiToken` in the macOS Keychain instead of plaintext in `config.json`.

- **Service** `dev.artemmac.filesandbox`, accounts `vtApiKey` / `apiToken` (generic-password items).
- **One-time migration at startup:** any plaintext secret in `config.json` is written to the Keychain, **read back to verify**, then blanked from the file. A verify failure aborts before blanking, so a failed write never loses the secret.
- **Precedence:** Keychain > `config.json` > env var.
- **Fail-safe:** default is `"file"` (legacy behaviour). If the Keychain is unreadable (e.g. a locked login keychain in a headless LaunchAgent session) the daemon logs and falls back to file/env values rather than crashing. The login keychain must be unlocked in the GUI session for Keychain access to succeed.
- The menu bar app stores its own client bearer token in the Keychain too (account `clientApiToken`), migrating off `UserDefaults` on first launch.
