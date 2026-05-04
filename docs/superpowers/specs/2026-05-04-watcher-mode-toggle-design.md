# Watcher mode toggle and per-engine controls

**Date:** 2026-05-04
**Author:** brainstorm session, file-sandbox
**Status:** design — awaiting implementation plan

## Goal

Replace the existing single-bit `paused` watcher state with a richer two-axis control surface:

1. **Watcher mode** — when to scan (active / scan-paused / monitoring-disabled).
2. **Engines** — which scanners run (pompelmi on/off, VirusTotal on/off).

The two axes are orthogonal. Mode controls the file pipeline; engines control which scanner phases the pipeline calls. State persists across daemon restarts and surfaces clearly in the menu bar.

## Non-goals

- Auto-resume timers ("pause for N minutes"). YAGNI; cron/launchd is enough for power users.
- Replaying files dropped while monitoring was disabled. Discarded events stay discarded.
- Migrating existing job rows. The `status` enum is reused; no schema change.

## Watcher modes

Three values for `watcherMode`:

| Mode | Behavior on new file |
|---|---|
| `active` | Existing flow: chmod 0o000 → quarantine xattr → move to quarantine folder → run enabled scanners → restore if clean. |
| `scan_paused` | chmod 0o000 → quarantine xattr → move to quarantine folder → mark `inconclusive` with detail `"Scanning paused at intake"`. **No scan calls.** Files are safely contained until the user resumes and uses the existing restore endpoint. |
| `monitoring_disabled` | Drop the event entirely. Files are not chmodded, not xattr'd, not moved. **Equivalent to today's `paused=true` behavior**, but explicitly opt-in and visually marked as advanced. |

Single field, mutually exclusive. The watcher's `private mode: WatcherMode` replaces `private paused: boolean`.

`setMode(mode)` aborts all in-flight scans (uses existing `scanControllers` map) when transitioning out of `active`. The existing "Cancelled by user" path handles the affected jobs cleanly.

## Engines

Two booleans:

```ts
pompelmiEnabled: boolean   // default true (also covered in pompelmi spec)
vtEnabled:       boolean   // default true (NEW)
```

Engine selection is independent of mode. When `mode === active`:

```
if (pompelmiEnabled) → run local scan
  if (verdict === malicious)  → quarantine, skip VT
  if (verdict === error && pompelmiFailureMode === bypass) → fall through to VT
  if (verdict === error && pompelmiFailureMode === inconclusive) → keep quarantined
  if (verdict === clean) → fall through to VT

if (vtEnabled && eligible-for-VT-from-pompelmi-stage) → run VT (existing path)

if (!pompelmiEnabled && !vtEnabled) → mark inconclusive at intake, keep quarantined
```

When both engines are disabled, every new file ends as `inconclusive`. The UI must signal this prominently (banner) so the user does not assume scanning is happening.

## Configuration

Additions to `RawConfig`:

```ts
watcherMode?: "active" | "scan_paused" | "monitoring_disabled"  // default "active"
vtEnabled?:    boolean   // default true
// pompelmiEnabled, pompelmiFailureMode, pompelmiSocketPath are defined in the pompelmi spec
```

Persistence: any mode change writes through `writeConfig`. The daemon reads `watcherMode` at startup and initializes the watcher accordingly. Unknown values fall back to `active` with a warning log.

## API

New canonical endpoint:

```
POST /api/watcher/mode
Body: { "mode": "active" | "scan_paused" | "monitoring_disabled" }
Returns: { ok: true, mode }
Persists to config.json. Aborts in-flight scans on transition out of active.
```

Deprecated aliases (kept for one release, log a deprecation warning):

```
POST /api/watcher/pause   → maps to mode "scan_paused"
POST /api/watcher/resume  → maps to mode "active"
```

`/api/jobs` response gains `mode: WatcherMode`. The pre-existing `paused: boolean` field is kept for one release as `mode !== "active"` for backwards compatibility.

`/api/health` gains `mode: WatcherMode` and `scannersEnabled: { pompelmi, vt }`.

## Watcher implementation notes

`src/watcher.ts`:

- Replace `private paused: boolean` with `private mode: WatcherMode`.
- Replace `pause()` / `resume()` / `isPaused` with `setMode(mode)` / `getMode()`. Keep wrapper methods for the deprecated HTTP routes.
- `setMode` writes to a state field, then iterates `scanControllers` and aborts each active controller when leaving `active`.
- `fsWatch` and `chokidar` callbacks branch on mode:
  - `active` → existing flow with engine-selection logic.
  - `scan_paused` → existing chmod + xattr + move + setScanning, then immediately `setScanResult({ verdict: "inconclusive", message: "Scanning paused at intake" })`.
  - `monitoring_disabled` → return early as today's `paused === true` does.

`src/job-store.ts`:

- No schema change. Reuse `inconclusive` verdict with the specific detail string for paused intake. UI badges differentiate by detail prefix.

## Menu bar UI

`Views.swift`:

- Replace the single play/pause button with a `Menu` showing three radio-style modes with checkmark on the current one. Use distinct SF Symbols: `play.circle.fill` (active), `pause.circle.fill` (scan_paused), `eye.slash.fill` (monitoring_disabled).
- Tint the menu bar status icon globally based on mode: default (active), orange (scan_paused), red (monitoring_disabled). User sees state without opening the dropdown.
- Replace the existing single `Paused` badge with a per-mode chip: `Active` (subtle), `Scanning paused` (orange), `Monitoring disabled` (red). Chip is always present so user can't miss the state.
- Existing `Refresh`, `Stop daemon`, and `Clear logs` buttons unchanged.

`SettingsView.swift`:

- Add a "Watcher" section explaining the three modes in plain language, with the same `Menu` control.
- Add a "Scanners" section with two `Toggle` rows:
  - "Local scanner (pompelmi/ClamAV)" → `pompelmiEnabled`
  - "VirusTotal cloud" → `vtEnabled`
  - Show socket-path field for pompelmi when enabled.
  - Show a red banner when both toggles are off: "No active scanners — all incoming files will be quarantined as inconclusive."

`SettingsStore.swift`:

- Extend `DaemonConfig` codable struct with `watcherMode`, `vtEnabled`, `pompelmiEnabled`, `pompelmiSocketPath`, `pompelmiFailureMode`.
- New `@Published` fields, fetch+save round-trips them.

`JobStore.swift`:

- Replace `@Published var isPaused: Bool` with `@Published var mode: WatcherMode`.
- Computed `isPaused: Bool { mode != .active }` for back-compat with header badge.
- Replace `pauseWatcher()` / `resumeWatcher()` with `setMode(_:)` calling the new API.

## Notification on launch

When the daemon starts in any mode other than `active`, post a macOS user notification via `UNUserNotificationCenter`:

- Title: "FileSandbox started in <mode>"
- Body: "New files are <not being scanned | not being monitored>. Open the menu bar to resume."

Implemented in the menu bar app's `App.swift` startup, comparing the first successful `/api/health` response.

## Failure modes

| Situation | Behavior |
|---|---|
| Unknown `watcherMode` in `config.json` | Daemon logs warning, falls back to `active`. |
| Both engines disabled, new file arrives | File is moved to quarantine, marked `inconclusive`. UI banner already warns the user. |
| `pompelmiEnabled=true` but clamd unavailable | Per pompelmi spec — daemon refuses to start. |
| API client posts unknown mode value | 400 with allowed values listed. |
| Mode persisted while config encrypted, master key absent at restart | `writeConfig` already throws; surface in /api/config error response. |

## Migration

Configs without these keys default to `active`, `vtEnabled=true`, `pompelmiEnabled=true` — preserving today's behavior. No migration script required.

## Open work

- Manual test: cycle through modes via API, verify in-flight scans are aborted and that paused-intake jobs land as `inconclusive` with the expected detail.
- Manual test: disable both engines, drop a file, expect `inconclusive` verdict.
- Document the deprecation timeline for `/api/watcher/pause` and `/api/watcher/resume` in the API reference.
