# Rust Migration Plan — file-sandbox daemon

Phase 1 deliverable. Full rewrite of the TS/Node daemon (`src/`) into a single
native Rust binary. Swift companions (`macos-menubar/`, `es-daemon/`) stay
untouched. The Docker/mkosi sandbox part is removed.

## Why (root cause that triggered this)

`scripts/install-launchagent.sh` writes a LaunchAgent plist whose
`ProgramArguments` are `[node, src/index.ts]` — **no `--experimental-strip-types`
flag**, and launchd does not source the shell `.env`. So launchd's `node` tries
to execute a `.ts` file and the daemon never starts. A single native binary
removes the runtime, the flag, the `node`-path discovery, and the `.env`
dependency in one move: the plist points at one executable.

## Target scope (decision)

Port the **entire `src/` daemon**, including the HTTP UI, to Rust. Rationale:
the HTTP endpoints are part of the required behavioral parity (the Swift menubar
polls them), and `ui-server` is small. Recommended scope = **whole Node part**.

Dropped in the rewrite:
- `useSeparateVtProcess` / `vt-worker.ts` — replaced by in-process tokio tasks +
  a concurrency semaphore. No child-process VT scanning.
- Docker / mkosi **sandbox** detonation: `sandbox-image/`, `scripts/sandbox-build.sh`,
  `package.json` `sandbox:build` script, `Dockerfile` (if sandbox-only), and the
  `SandboxManager` code path (locate in Phase 2 and remove).

Kept untouched: `macos-menubar/`, `es-daemon/` (Swift Endpoint Security daemon).

## Module → Rust map

| TS module | Rust target | Replacement dep |
|---|---|---|
| `index.ts` (startup wiring) | `src/main.rs` + `app.rs` | tokio runtime |
| `config.ts` | `config.rs` | serde + serde_json; env overlay |
| `config-crypto.ts` (AES-256-GCM, scrypt, `FSENC1:`) | `config_crypto.rs` | `aes-gcm`, `scrypt`, `base64` — preserve exact `FSENC1:` salt[16]\|iv[12]\|tag[16]\|ct format |
| `job-store.ts` (better-sqlite3) | `job_store.rs` | `rusqlite` (already in `vt-cache/`) — **same schema, verbatim** |
| `watcher.ts` + `watcher-mode.ts` (chokidar + fs.watch) | `watcher.rs` + `mode.rs` | `notify` crate |
| `local-scanner.ts` (clamd UNIX socket / pompelmi) | `local_scanner.rs` | clamd `INSTREAM` over UnixStream (`clamav-client` crate or hand-rolled) |
| `virus-checker.ts` + `vt-worker.ts` (VirusTotal) | `virus_checker.rs` | `reqwest` (multipart upload + poll) |
| `vt-cache.ts` (bridges to Rust binary) | **absorb existing `vt-cache/` crate** | already rusqlite + sha2 — fold into the daemon, drop the child-process bridge |
| `ui-server.ts` (Express) | `ui_server.rs` | `axum` (+ `tower`) — same routes, same port, same auth headers |
| `file-mover.ts` (copy→quarantine, restore, delete) | `file_mover.rs` | std::fs + `uuid` |
| `file-permissions.ts` (chmod) | `file_permissions.rs` | std::os::unix::fs |
| `inconclusive-sweeper.ts` (hourly) | `sweeper.rs` | tokio interval |
| `metrics.ts` | `metrics.rs` | atomics / `OnceLock` |
| `semaphore.ts` | drop | `tokio::sync::Semaphore` |
| `http-host-guard.ts` (reject non-loopback) | `host_guard.rs` | preserve `FILESANDBOX_ALLOW_LAN` flag |
| `launch-agent-monitor.ts` (watch LaunchAgent dirs) | `launch_agent_monitor.rs` | `notify` |

## Scanning decision (pompelmi — TS-only, no Rust analog)

`local-scanner.ts` does **not** depend on pompelmi's scanning engine for the hot
path — it speaks the **clamd `INSTREAM` protocol over a UNIX socket**
(`pompelmiSocketPath`, default `/tmp/clamd.sock`). That protocol is trivial to
speak from Rust.

- **Recommended:** talk clamd `INSTREAM` directly from Rust (`clamav-client`
  crate, or ~40 lines hand-rolled over `UnixStream`). Preserve `pompelmiEnabled`,
  `pompelmiSocketPath`, `pompelmiFailureMode` ("bypass" | "inconclusive") config
  and startup socket probe (fail hard if unreachable when enabled).
- VirusTotal stays as-is via `reqwest`.
- No TS scan-worker subprocess needed.

This means **no functional capability is lost** dropping the npm `pompelmi` dep.

## Behavioral parity contract (must hold)

- **SQLite schema verbatim** — `jobs` table columns: `id, source_path,
  original_name, quarantine_path, final_path, status, vt_verdict, detail,
  created_at, updated_at` + `pompelmi_verdict`, `scan_stage`; index
  `idx_jobs_created`. Same status/verdict/stage string enums.
- **HTTP routes verbatim** on `httpHost:httpPort` (default `127.0.0.1:3847`):
  `GET /api/health`, `GET /health`->301, `GET /api/jobs`, `GET /api/security-events`,
  `POST /api/watcher/{pause,resume,mode}`, `DELETE /api/jobs`,
  `POST /api/jobs/:id/{cancel,restore}`, `GET|POST /api/config`,
  `DELETE /api/jobs/:id/quarantine` (409 on conflict), `GET /` dashboard.
- **Auth verbatim** — `Authorization: Bearer <token>` or `X-FileSandbox-Token`;
  gate bypassed when `apiToken` unset.
- **config.json shape + env overrides verbatim** (see `config.example.json` and
  the env table) — file > env precedence; `FILESANDBOX_MASTER_KEY` encryption.
- **Swift menubar contract** — `GET /api/security-events` returns
  `{ events: [{ kind, path, at }] }`; menubar polls every 5s. `onModeChange`
  persists mode back to `config.json`.

## Repository layout

Promote the daemon to a workspace. `vt-cache/` already lives at repo root as a
crate; create a root `Cargo.toml` workspace with the new `daemon` crate that
absorbs `vt-cache`'s cache logic. Build artifacts already gitignored
(`vt-cache/target/`, add the new target dir).

## launchd integration (closes the original symptom)

Rewrite `scripts/install-launchagent.sh`:
- `ProgramArguments` = `[<PROJECT_DIR>/target/release/file-sandbox-daemon]` — one
  binary, no node, no flag, no `.ts`.
- Drop node-path/nvm discovery and the Node-20 check.
- Inject config via `EnvironmentVariables` in the plist (or rely on `config.json`
  in `WorkingDirectory`) — no shell `.env` dependency.
- Keep `RunAtLoad`, `KeepAlive{SuccessfulExit:false}`, `ThrottleInterval`, log paths.
- `uninstall-launchagent.sh` unchanged (label `dev.artemmac.filesandbox`).

## Phases (each independently verifiable)

1. **Workspace + config** — root `Cargo.toml` workspace; `config.rs` +
   `config_crypto.rs`. Verify: load `config.example.json`, round-trip encrypt/decrypt
   matches a TS-produced `FSENC1:` payload. `cargo test`.
2. **Job store** — `job_store.rs` on rusqlite, schema verbatim. Verify: open an
   existing TS-written `jobs.sqlite`, read/write all statuses; port job-store
   tests. `cargo test`.
3. **File ops + scanning** — `file_mover.rs`, `file_permissions.rs`,
   `local_scanner.rs` (clamd), `virus_checker.rs` (reqwest) + absorbed vt-cache.
   Verify: clamd probe, EICAR test file -> "malicious"; VT mock/poll path.
4. **Watcher + sweeper + monitor** — `watcher.rs` (notify), `mode.rs`, `sweeper.rs`,
   `launch_agent_monitor.rs`, `metrics.rs`, `host_guard.rs`. Verify: drop a file ->
   job row created -> scan verdict -> quarantine decision.
5. **HTTP UI** — `ui_server.rs` on axum; all routes + auth + dashboard. Verify:
   `curl` every endpoint, compare contract to TS; security-events shape.
6. **launchd + cleanup** — rewrite installer to point at the binary; remove the
   Docker/mkosi sandbox part and `SandboxManager`; delete migrated `src/*.ts`.
   Verify: `launchctl bootstrap` -> process stays alive, no `EX_*`; end-to-end run.

> **Status (2026-06-18): all phases ✅ complete.** Installer points at
> `daemon/target/release/file-sandbox-daemon`; node daemon (`src/*.ts`, `package.json`,
> Docker), standalone `vt-cache` crate, mkosi `sandbox-image/`, and the Swift VM sandbox
> (`SandboxManager`, sandbox tab) are all removed. `cargo test` 64 passed; `swift build` +
> `swift test` green.

## Acceptance criteria

- `cargo build --release` green; binary starts under launchd without node, no `EX_*` failure.
- Drop a file in watch dir -> job in the **same** SQLite schema -> scan verdict -> menubar alert.
- `curl` on the same port returns the same contract for every endpoint.
- `cargo test` passes; each ported module has tests equivalent to the old `node --test`.
- Sandbox part removed; no dead references.

## Verification commands

```
cargo build --release
cargo test
bash scripts/install-launchagent.sh && launchctl list | grep dev.artemmac.filesandbox
curl -s 127.0.0.1:3847/api/health
# drop EICAR into watchPath -> check /api/jobs verdict + /api/security-events
```
