# Local scanner integration via pompelmi (ClamAV)

**Date:** 2026-05-04
**Author:** brainstorm session, file-sandbox
**Status:** design — awaiting implementation plan

## Goal

Add a local-first virus scanner upstream of the existing VirusTotal pipeline. Use the `pompelmi` npm package (ClamAV wrapper) to scan files via a long-running `clamd` daemon over a UNIX socket. The result is defense-in-depth: malicious files are caught locally before any cloud upload; clean files continue through the existing VirusTotal flow as a second opinion.

## Non-goals

- Replacing VirusTotal. The existing flow is preserved.
- Caching pompelmi verdicts. Local re-scan is cheap, and virus-definition refreshes (`freshclam`) would otherwise stale the cache.
- Bundling ClamAV. The user installs it manually; the daemon refuses to start without a reachable socket.
- Per-engine state in the job's `status` enum. Engine verdicts live in their own columns.

## User-facing prerequisites

The user installs ClamAV before enabling the feature:

```sh
brew install clamav
freshclam                                     # download virus definitions (~300 MB)
# edit /opt/homebrew/etc/clamav/clamd.conf:
#   uncomment LocalSocket /tmp/clamd.sock
#   raise MaxFileSize 4000M
#   raise MaxScanSize 4000M
#   raise StreamMaxLength 4000M
brew services start clamav
```

These steps are documented in the README. The daemon validates the socket is reachable on startup and refuses to start if `pompelmiEnabled=true` and the socket is unreachable.

## Package management

Switch the project from npm to yarn at the same time, per user preference:

- Delete `package-lock.json`.
- Run `yarn add pompelmi`.
- Commit `yarn.lock`.
- Update CI/scripts/docs to use `yarn`.

## Configuration

Additions to `RawConfig` in `src/config.ts`:

```ts
pompelmiEnabled?: boolean              // default true
pompelmiSocketPath?: string            // default "/tmp/clamd.sock"
pompelmiFailureMode?: "bypass" | "inconclusive"  // default "bypass"
```

`pompelmiFailureMode` controls what happens when pompelmi returns `Verdict.ScanError` (clamd unreachable mid-flight, file unreadable, etc):

- `bypass` — log a warning, fall through to the VT path. Local scanner failure does not block scanning. Default.
- `inconclusive` — keep the file in quarantine with verdict `inconclusive`, do not call VT. Strict opt-in for users who refuse cloud-only scanning.

## Schema migration

Single additive column on `jobs`:

```sql
ALTER TABLE jobs ADD COLUMN pompelmi_verdict TEXT;
```

Idempotent at startup (try/catch on already-exists). Old rows get `NULL`. The `status` enum is unchanged. The existing `vt_verdict` column is unchanged.

The `Job` row exposes `pompelmi_verdict: "clean" | "malicious" | "error" | null` alongside `vt_verdict`.

## New module: `src/local-scanner.ts`

Thin wrapper around `pompelmi.createScanner({ clamd: { socket: cfg.pompelmiSocketPath } })`.

```ts
export type LocalVerdict = "clean" | "malicious" | "error";

export interface LocalScanResult {
  verdict: LocalVerdict;
  message: string;
}

export interface LocalScannerOptions {
  socketPath: string;
}

export class LocalScanner {
  constructor(opts: LocalScannerOptions);
  /** Throws if the socket is unreachable. Used at daemon startup. */
  static probe(socketPath: string): Promise<void>;
  check(filePath: string, signal?: AbortSignal): Promise<LocalScanResult>;
}
```

`check` translates pompelmi's `Verdict` to our `LocalVerdict`, treating `Verdict.ScanError` as `error`. The pompelmi call respects `signal` for cancellation (best-effort; clamd does not support cancel — we just stop awaiting).

No size limit is enforced inside the scanner. Whatever ClamAV's `clamd.conf` allows is what gets scanned. The `oversized` short-circuit applies only to the VT phase.

## Pipeline change: `src/watcher.ts handleFile`

Insert a local-scan stage between `setScanning` and the existing VT path:

```
file → setQuarantineXattr → fileMover.move → setInQuarantine → setScanning
     → if (pompelmiEnabled):
         localScanner.check(quarantineFilePath, signal)
         ├─ malicious: setPompelmiVerdict('malicious'); setScanResult({verdict:'infected'}); keep quarantined; return
         ├─ clean:     setPompelmiVerdict('clean'); fall through to VT path
         └─ error:     setPompelmiVerdict('error');
                       if mode === 'bypass':       fall through to VT path with warn
                       if mode === 'inconclusive': setScanResult({verdict:'inconclusive'}); keep quarantined; return
       (existing) cache check → VT scan → cache store → restore-if-clean
```

Notes:

- The existing `scanSemaphore` continues to gate VT only. clamd handles its own concurrency. No new semaphore.
- The existing `useSeparateVtProcess` flag is unaffected — clamd already runs in its own process, so isolation is moot for the local stage.
- The existing `vt-cache` is unchanged. A pompelmi-clean file still hits the VT cache before re-uploading; cache hits skip VT as today.
- Cancellation: `controller.abort()` is checked between stages. If aborted before the VT stage, we skip directly to the existing cancelled-by-user path.
- When `pompelmiEnabled=false`, the local stage is skipped entirely (backward compatible with current behavior).

## API surface

`/api/jobs` response: each job row now includes `pompelmi_verdict` alongside `vt_verdict`. No new endpoints.

`/api/health` adds `localScanner: { enabled: boolean, socketReachable: boolean }`.

## UI

The menu bar adds a small badge per job row showing the local-scan verdict when present (`✓ local`, `✗ local`). VT badge unchanged. Settings UI gains the `pompelmiEnabled` and `pompelmiSocketPath` and `pompelmiFailureMode` controls. (Detailed UI wiring lives in the menu-bar toggle spec.)

## Failure modes

| Situation | Behavior |
|---|---|
| clamd not running at daemon startup, `pompelmiEnabled=true` | Daemon refuses to start; logs the socket path it expected. |
| clamd dies mid-flight | Pompelmi returns `Verdict.ScanError` → handled per `pompelmiFailureMode`. |
| `pompelmiEnabled=false` and clamd absent | Daemon starts normally; only VT runs. |
| ClamAV virus DB outdated (freshclam not run) | Pompelmi returns clean for new threats. Documented; user runs `freshclam` periodically. |
| ClamAV `MaxFileSize` exceeded | Pompelmi returns `Verdict.ScanError` (clamd reports limit) → handled per `pompelmiFailureMode`. |
| pompelmi npm version drift | `package.json` pins minor; CI runs `yarn install --frozen-lockfile`. |

## Open work

- Docs: README section on ClamAV setup and `clamd.conf` tweaks.
- Manual smoke test: drop EICAR test string into watch folder, expect `pompelmi_verdict='malicious'` and no VT upload.
- Manual smoke test: drop a known-clean file, expect both verdicts populated, file restored.
- Optional follow-up (not in this spec): exposing `localScanner.socketReachable` as a periodic ping to detect mid-flight clamd outages and surface them in the UI.
