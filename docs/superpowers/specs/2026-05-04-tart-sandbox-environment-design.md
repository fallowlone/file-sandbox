# Isolated open-file environment via Tart VM

**Date:** 2026-05-04
**Author:** brainstorm session, file-sandbox
**Status:** design — awaiting implementation plan

## Goal

Provide a user-triggered, fully isolated macOS environment in which arbitrary files can be opened without risk to the host. The host daemon orchestrates Tart VM clones from a base image: each session gets a fresh clone, the user opens the file inside the guest, and the clone is discarded when the session ends. Network access is off by default; clipboard, USB, and bidirectional file transfer are all gated.

## Non-goals

- Automatic sandboxing of every download. The user explicitly chooses to sandbox a file. (Existing watcher pipeline is untouched by this feature.)
- Cross-platform isolation. Linux containers are out of scope; this targets macOS guest content (Office files, .dmg, .app, .pdf, archives).
- Bundling the macOS guest image. The user fetches it once via `tart pull`.
- Persisting changes to the base VM. Sessions are always fresh clones.

## Prerequisites (manual, documented)

```sh
brew install cirruslabs/cli/tart
tart pull ghcr.io/cirruslabs/macos-sequoia-base:latest      # one-time, ~30 GB
tart clone ghcr.io/cirruslabs/macos-sequoia-base:latest filesandbox-base
```

The daemon validates `tart --version` at startup if `sandboxEnabled=true`. If `tart` is absent, sandbox endpoints return 503 and the rest of the daemon continues to function.

## Configuration

Additions to `RawConfig` in `src/config.ts`:

```ts
sandboxEnabled?: boolean              // default false (off until prereqs done)
sandboxBaseVm?: string                // default "filesandbox-base"
sandboxIdleTimeoutMinutes?: number    // default 240
sandboxNetworkDefault?: boolean       // default false
sandboxSessionsDir?: string           // default "~/Library/Application Support/FileSandbox/sandbox-sessions"
sandboxOutRetentionDays?: number      // default 7
```

## Schema

New table in the existing `jobs.sqlite`:

```sql
CREATE TABLE IF NOT EXISTS sandbox_sessions (
  id                TEXT    PRIMARY KEY,
  vm_name           TEXT    NOT NULL UNIQUE,           -- "fsbx-<uuid8>"
  source_job_id     TEXT,                              -- nullable, FK-shaped to jobs.id
  source_file_path  TEXT    NOT NULL,
  session_dir       TEXT    NOT NULL,
  pid               INTEGER,
  network_enabled   INTEGER NOT NULL DEFAULT 0,
  status            TEXT    NOT NULL,                  -- 'starting'|'running'|'stopped'|'failed'|'discarded'
  detail            TEXT,
  created_at        TEXT    NOT NULL,
  last_active_at    TEXT    NOT NULL,
  exited_at         TEXT
);

CREATE INDEX IF NOT EXISTS idx_sandbox_status ON sandbox_sessions(status);
CREATE INDEX IF NOT EXISTS idx_sandbox_created ON sandbox_sessions(created_at);
```

Created idempotently at startup, alongside the existing `jobs` table.

## New module: `src/sandbox-manager.ts`

```ts
export type SessionStatus = "starting" | "running" | "stopped" | "failed" | "discarded";

export interface SandboxSession {
  id: string;
  vmName: string;
  sourceJobId: string | null;
  sourceFilePath: string;
  sessionDir: string;
  pid: number | null;
  networkEnabled: boolean;
  status: SessionStatus;
  detail: string | null;
  createdAt: string;
  lastActiveAt: string;
  exitedAt: string | null;
}

export interface CreateSessionInput {
  filePath: string;
  sourceJobId?: string;
  network?: boolean;
}

export class SandboxManager {
  constructor(opts: { db: Database, config: SandboxConfig });
  static probe(): Promise<{ tartInstalled: boolean; baseImagePresent: boolean }>;
  init(): Promise<void>;                                 // create sessions dir, run reconcile
  createSession(input: CreateSessionInput): Promise<SandboxSession>;
  listSessions(opts?: { limit?: number }): SandboxSession[];
  getSession(id: string): SandboxSession | null;
  showSession(id: string): Promise<void>;                // focuses VM viewer window
  discardSession(id: string): Promise<void>;             // kill PID, tart delete clone, mark discarded
  shutdownAll(): Promise<void>;                          // graceful daemon stop
}
```

Internal flow for `createSession`:

1. Validate `filePath` (absolute, exists, readable, not a directory, not symlinked outside allowed roots).
2. Generate `id = uuid`, `vmName = "fsbx-" + id.slice(0,8)`.
3. Create `sessionDir = sandboxSessionsDir + "/" + id`, with `in/` (mode 0700) and `out/` (mode 0700) sub-folders.
4. Copy source file into `in/`, then `chmod 0444` so the guest can't write back.
5. Insert row with `status='starting'`.
6. `tart clone <baseVm> <vmName>` (Promise wrapping execFile).
7. `tart run <vmName> --dir=in:in/:ro --dir=out:out/` (network flags conditional). Stdout/stderr captured for diagnostics.
8. Track child PID; update row `status='running'`, `pid`, `last_active_at`.
9. Attach exit listener: when the child exits, run cleanup (delete clone, set status `stopped`, optionally retain `out/`).
10. Idle watchdog (interval timer in the manager) discards sessions whose `last_active_at` is older than `sandboxIdleTimeoutMinutes`. Activity is updated by any successful `getSession` / `showSession` / `/api/sandbox/sessions/:id` call.

Reconcile-on-startup: any row with status `starting` or `running` whose VM is not in `tart list` output is marked `discarded` with detail "stale on daemon restart". Orphan VM clones whose row is missing or `discarded` are deleted from disk.

## File flow

**Open in sandbox:**

1. User picks a file (menu bar list or "+ New sandbox" file picker).
2. `POST /api/sandbox/sessions` with `filePath` (and optional `sourceJobId`).
3. Daemon copies file into `<sessionsDir>/<id>/in/`, spawns Tart, returns the row.
4. Tart's viewer window appears. Inside the guest, the shared folder mounts at `/Volumes/My Shared Files/in/` (read-only).
5. User opens the file with whatever app they want, inside the VM.

**Export from sandbox to host:**

1. User saves a file to `/Volumes/My Shared Files/out/` inside the guest (writable).
2. Back on the host, user clicks "Export from session" in the menu bar, picks a file.
3. The picked file is **moved into the watch folder** — it goes through the normal pompelmi+VT pipeline → quarantine → restore-if-clean. The user does not bypass scanning to escape the sandbox.
4. The original session is left running (or can be discarded by the user).

**Discard:**

- User clicks "Discard" or closes the VM window, or idle timeout fires.
- Daemon kills child PID (SIGTERM, then SIGKILL after 10s grace).
- `tart delete <vmName>`.
- `in/` is wiped immediately. `out/` is retained for `sandboxOutRetentionDays` days, then auto-purged by a daily cleanup.
- Row marked `status='discarded'`, `exited_at` set.

## API surface

`POST /api/sandbox/sessions`

```json
{ "filePath": "/abs/path/to/file.docx", "sourceJobId": "optional", "network": false }
```

Validates path, copies file, spawns VM. Returns the new session row. Auth: same Bearer token as the rest of the daemon.

`GET /api/sandbox/sessions?limit=50`

Returns active and recent sessions, newest first.

`GET /api/sandbox/sessions/:id`

Returns one session row. Updates `last_active_at` to defer idle timeout.

`DELETE /api/sandbox/sessions/:id`

Discards.

`POST /api/sandbox/sessions/:id/show`

Focuses the VM viewer window (via `osascript` `tell application "Tart" to activate` or by `pid`).

`POST /api/sandbox/sessions/:id/export`

```json
{ "fileName": "report.docx" }    // file inside the session's out/ dir
```

Moves the picked output file into the watch folder. Returns the `jobId` of the resulting normal-pipeline job for the user to follow.

`/api/health` adds:

```json
{
  "sandbox": {
    "enabled": true,
    "tartInstalled": true,
    "baseImagePresent": true,
    "activeSessions": 1
  }
}
```

## Menu bar UI

A new "Sandbox" section in the menu bar dropdown, alongside the existing jobs list:

- Header with "+ New sandbox" button (opens NSOpenPanel; on selection, posts to API).
- Active sessions list:
  - VM name, source filename, status badge, age, network on/off icon.
  - Per-row actions: [Show] focuses VM, [Export…] picks a file from `out/`, [Discard].
- Recently discarded sessions appear collapsed below.

On any quarantined job row in the main list: an "Open in sandbox" button posts to the API with `sourceJobId`. The quarantined file path is the source.

`SettingsView.swift` gains a "Sandbox" section:

- Enable toggle (`sandboxEnabled`).
- Base VM name (`sandboxBaseVm`).
- Idle timeout in minutes (`sandboxIdleTimeoutMinutes`).
- Default-network checkbox (`sandboxNetworkDefault`).
- Output retention days (`sandboxOutRetentionDays`).
- Status indicators: `tart` installed (✓/✗), base image present (✓/✗). When either is missing, show install instructions inline.

## Security guards

- Sandbox is **opt-in** (`sandboxEnabled=false` by default).
- Network is **off by default** (`--net-softnet` not passed). User flips per-session toggle if needed.
- Source file mounts **read-only** into the guest (`--dir=...:ro`).
- Path validation on `POST /api/sandbox/sessions`: disallow `..`, disallow paths outside `watchPath`, `quarantinePath`, or the user's home directory. Resolve symlinks before checking.
- Source file is **copied** into the session dir, never linked. The original is unaffected by guest activity.
- `out/` files are not auto-imported. Re-entering the host requires the explicit Export action, which goes through the full scan pipeline.
- Clipboard sharing host↔guest: **disabled** (do not pass `--vmnet-shared` or any clipboard-enabling flag).
- USB/peripheral pass-through: **disabled**.
- API auth: same Bearer-token middleware as existing endpoints.
- Disk usage: each clone is several GB. The daily cleanup purges orphan clones; the UI shows total disk used by `~/.tart/vms/fsbx-*`.

## Failure modes

| Situation | Behavior |
|---|---|
| `tart` not installed | All sandbox endpoints return 503; UI shows install instructions. Rest of daemon unaffected. |
| Base VM missing | `createSession` fails with status `failed`, detail says how to `tart pull`. |
| Disk full during clone | `tart clone` exits non-zero; row marked `failed` with detail. |
| Tart child crashes | exit listener marks row `stopped`. No auto-restart. |
| Daemon restart with running VMs | Reconcile on startup: missing-from-`tart list` rows are marked `discarded`; orphan VMs (in `tart list` but no row) are deleted. |
| Host hibernates | VM suspends with the host. On resume, daemon does not auto-clean unless idle timeout has elapsed since the row's `last_active_at`. |
| User attempts to open path outside allowed roots | API returns 400 with the validated allowed roots list. |
| User discards a session that was already discarded | API returns 200, idempotent. |
| Apple's 2-VM-concurrent limit hit | `tart run` fails; row marked `failed`; UI surfaces the error so user can discard another session first. |

## Open work

- Manual test: install Tart, pull base, drop a benign file in watch folder, click "Open in sandbox", verify VM window opens with the file accessible at `/Volumes/My Shared Files/in/<name>`.
- Manual test: save a file to `out/` in guest, run Export, verify it shows up as a new job in the watch pipeline.
- Manual test: kill `tart` process externally, daemon reconcile marks session `stopped` and cleans up.
- Manual test: enable network on a session, confirm guest has DNS + outbound; disable, confirm isolated.
- Future polish: macOS Services menu integration so the user can right-click a Finder file → "Open in FileSandbox".
- Future polish: per-session memory and CPU caps via Tart flags.
- Future polish: remote display via VNC for power users (currently Tart's built-in window suffices).
