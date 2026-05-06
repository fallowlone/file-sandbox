# Menubar scan visibility — design

**Date:** 2026-05-06
**Author:** brainstorm session, file-sandbox
**Status:** design — awaiting implementation plan
**Supersedes parts of:** `2026-05-04-menubar-ui-redesign-design.md` (Jobs expanded row engine cards section)

## Goal

Make the menu bar UI surface, in real time, **what file is being checked and by which scanner right now**. Current expanded row shows only a VirusTotal `EngineCard` and ignores `pompelmi_verdict` despite the daemon already returning it. There is also no live indicator of which pipeline stage a job is in (cache check / local scan / VT upload / VT poll), so a user staring at a `scanning` job has no way to tell where it is stuck or how long it should still take.

This spec covers two related but separable improvements, delivered as two PRs:

- **PR1 — pompelmi card visibility.** Swift-only. Add `pompelmi_verdict` to `SandboxJob`, render a Local engine card alongside the VirusTotal one, fix `VerdictPill` to consider both verdicts.
- **PR2 — live scan stage tracking.** Daemon adds a `scan_stage` column and writes it on every pipeline transition. Swift renders a single horizontal row of stage badges in the expanded row, swaps the menu-bar icon per current stage, and shows a stage-coloured pill in the collapsed row while a job is in flight.

## Non-goals

- Cancelling a job from a specific stage (the existing `/cancel` endpoint is enough).
- Stage-by-stage timing telemetry / metrics (would be useful, but separate work).
- Sandbox UI changes. The user explicitly deferred sandbox tightening.
- Cross-platform UI / new daemon endpoints / Linux sandbox image work.
- Removing the existing `EngineCard` component — it stays for the case where we render simple verdict cards elsewhere; PR2 simply stops using it inside the Jobs expanded row.

---

## PR1 — pompelmi card visibility

### What changes

`pompelmi_verdict` is already a column in `jobs` (job-store.ts adds it via `ALTER TABLE`) and is already returned in the `/api/jobs` payload. The Swift `SandboxJob` struct does not declare the field, so `JSONDecoder` discards it. The expanded row in `JobsTabView` then has nothing to show, and renders only a VT `EngineCard`.

The fix is purely client-side.

### Files

| File | Change |
|---|---|
| `macos-menubar/Sources/App/JobStore.swift` | `SandboxJob`: add `let pompelmi_verdict: String?`. Order: after `vt_verdict`. |
| `macos-menubar/Sources/App/Components/VerdictPill.swift` | `forJobVerdict(...)`: accept both `vt_verdict` and `pompelmi_verdict`, pick the highest-priority one (see priority table below). |
| `macos-menubar/Sources/App/Tabs/JobsTabView.swift` | `expandedDetail`: render two `EngineCard`s in a single `HStack(spacing: 6)`. Local hidden when `pompelmi_verdict == nil`; VT hidden when `vt_verdict == nil`. |

### EngineCard mapping

Keep the existing `EngineCard.Status` enum. New mapping for the Local card:

| `pompelmi_verdict` | `EngineCard.Status` | label |
|---|---|---|
| `clean` | `.clean` (green dot) | "Local clean" |
| `malicious` | `.malicious` (red dot) | "Local infected" |
| `error` | `.warn` (orange dot) | "Local error" |
| `null` | (card hidden) | — |

VT card mapping is unchanged from today's behaviour.

### VerdictPill collapsed-row priority

`VerdictPill.forJobVerdict(vt:pompelmi:status:)` returns the first match from the top:

1. Either verdict equals `infected` or `malicious` → red pill, text "infected".
2. Status in `{scanning, received, in_quarantine}` → blue pill, text "scanning".
3. `vt_verdict == "inconclusive"` → orange pill, text "inconclusive".
4. `vt_verdict == "oversized"` → grey pill, text "oversized".
5. Status is `restored` → green pill, text "clean".
6. Otherwise → return `nil` (no pill).

Note: this priority survives PR2. The collapsed-row pill *is replaced* by a stage pill while a scan is in flight (see PR2), but for terminal / settled jobs the verdict pill above is what renders.

### Verification

- `cd macos-menubar && swift build` passes clean.
- Manual: drop a file in the watch path with `pompelmiEnabled=true` and a working VT API key. Open the menu bar dropdown; click the row to expand. Both cards (Local + VirusTotal) render side by side. Disable pompelmi in Settings; new jobs render only the VT card.

### Size

3 files, ~30 LOC delta, 1 commit.

---

## PR2 — live scan stage tracking

### What changes

Add a `scan_stage` column to `jobs`. The daemon writes it on every transition. The Swift app reads it, swaps the menu-bar icon per stage, shows a stage-coloured pill in the collapsed row while in flight, and renders a horizontal row of badges in the expanded row that always shows all five pipeline stages.

### Daemon — TS changes

**Files:**

| File | Change |
|---|---|
| `src/job-store.ts` | Add `scan_stage TEXT` via idempotent `ALTER TABLE` (matching the existing `pompelmi_verdict` pattern). Add `scan_stage` to all SELECT lists. Add `setStage(id: string, stage: ScanStage)` method. |
| `src/watcher.ts` | Call `jobStore.setStage(id, "received")` after `chmod 0o000` succeeds. |
| `src/virus-checker.ts` | Call `setStage` at the start of cache check, local scan (only if pompelmi enabled), VT upload, VT poll, and at `done`. |
| `src/types/analysis.ts` (or wherever `SandboxJob`-shaped types live) | Add the `scan_stage` field to the row type. |

**Stage enum (TypeScript):**

```ts
export type ScanStage =
  | "received"      // fs.watch caught + chmod 0o000 done
  | "cache_check"   // vt-cache lookup running
  | "local_scan"    // pompelmi/clamd running
  | "vt_upload"     // POST file to VT
  | "vt_poll"       // polling /analyses/{id}
  | "done"          // verdict written
  | "error";        // any stage errored; detail carries the message
```

`skipped` is **not** a stored value — a stage is skipped simply by never being set. The Swift side detects this by comparing the current `scan_stage` to the pipeline order.

**Stage write points:**

| Pipeline action | Stage written |
|---|---|
| `chmod 0o000` succeeds | `received` |
| About to call `vt-cache check` | `cache_check` |
| About to invoke pompelmi (only if enabled) | `local_scan` |
| About to upload to VT | `vt_upload` |
| Upload done, polling loop starts | `vt_poll` |
| Restore / keep / oversized decision made | `done` |
| Any uncaught error in the catch block | `error` (and detail = error message) |

**No new HTTP endpoints.** `/api/jobs` already returns the full row.

### Daemon — verification

- `yarn test` passes.
- Manual: `tail -f` the SQLite jobs table (`sqlite3 ~/.file-sandbox/jobs.sqlite "select id, status, scan_stage from jobs order by created_at desc limit 10"`) while dropping a file. Stage walks `received → cache_check → (local_scan?) → vt_upload → vt_poll → done` for a fresh file; cache hit walks `received → cache_check → done`.

### Swift — model

**Files:**

| File | Change |
|---|---|
| `macos-menubar/Sources/App/JobStore.swift` | `SandboxJob`: add `let scan_stage: String?` after `pompelmi_verdict` (PR1). Add `var stageEnum: ScanStage? { scan_stage.flatMap(ScanStage.init(rawValue:)) }`. |
| `macos-menubar/Sources/App/ScanStage.swift` (new) | `ScanStage` enum with `label`, `symbol`, `tint`, and `pipeline` order. |

**Enum (Swift):**

```swift
enum ScanStage: String, CaseIterable {
    case received
    case cacheCheck = "cache_check"
    case localScan  = "local_scan"
    case vtUpload   = "vt_upload"
    case vtPoll     = "vt_poll"
    case done
    case error

    /// Stages that appear as badges in the expanded row, in pipeline order.
    /// `received` and `error` are not in this list — `received` is too short-lived to
    /// merit a badge, and `error` replaces the *current* stage's colour rather than
    /// adding a new badge.
    static let pipeline: [ScanStage] = [.cacheCheck, .localScan, .vtUpload, .vtPoll, .done]

    var label: String {
        switch self {
        case .received:   return "Received"
        case .cacheCheck: return "Cache"
        case .localScan:  return "Local"
        case .vtUpload:   return "VT upload"
        case .vtPoll:     return "VT poll"
        case .done:       return "Done"
        case .error:      return "Error"
        }
    }

    var symbol: String {
        switch self {
        case .received:   return "tray.and.arrow.down"
        case .cacheCheck: return "magnifyingglass"
        case .localScan:  return "shield.lefthalf.filled"
        case .vtUpload:   return "arrow.up.circle"
        case .vtPoll:     return "arrow.triangle.2.circlepath"
        case .done:       return "checkmark.shield.fill"
        case .error:      return "exclamationmark.triangle.fill"
        }
    }

    /// Tint used by `StagePill` (collapsed-row pill) and as the foreground
    /// colour of a `current`-state `StageBadge`.
    var tint: Color {
        switch self {
        case .received, .cacheCheck, .vtUpload, .vtPoll: return Theme.verdictBlueFg
        case .localScan:                                  return Theme.verdictOrangeFg
        case .done:                                       return Theme.verdictGreenFg
        case .error:                                      return Theme.verdictRedFg
        }
    }
}
```

### Swift — UI

**New components:**

| File | Responsibility |
|---|---|
| `Sources/App/Components/StagePill.swift` | A small pill for the collapsed row. Shows `<icon> <label>` in the stage's tint colour. Used while a scan is in flight (any `stageEnum` other than `.done` / `.error`). |
| `Sources/App/Components/StageBadge.swift` | A single badge in the expanded-row row. State enum: `done(verdictText: String?)`, `current`, `pending`, `skipped`, `error(detail: String?)`. Verdict text (e.g. "clean", "0/72", "infected", "miss", "hit clean") is rendered after a `·` separator. |
| `Sources/App/Components/StageRow.swift` | Horizontal `HStack(spacing: 6)` (with `.flexible` wrap on overflow) of `StageBadge`s, one per `ScanStage.pipeline`. Computes each badge's state from the job's current `stageEnum`, `pompelmi_verdict`, and `vt_verdict`. |

**Modified:**

| File | Change |
|---|---|
| `Sources/App/Tabs/JobsTabView.swift` | `collapsedRow`: when `stageEnum != nil && stageEnum != .done && stageEnum != .error`, render `StagePill` instead of the verdict pill. Otherwise unchanged (PR1's verdict pill renders). `expandedDetail`: replace the existing `EngineCard` row with a single `StageRow(job:)`. Meta pills + action buttons unchanged. |
| `Sources/App/JobStore.swift` | `iconName` getter: prefer the latest active job's stage symbol; threats override (`exclamationmark.shield.fill`); offline override (`shield.slash`); default green shield when idle. See snippet below. |

**iconName logic:**

```swift
var iconName: String {
    guard isConnected else { return "shield.slash" }
    if !activeThreats.isEmpty { return "exclamationmark.shield.fill" }
    let activeStage = jobs
        .compactMap(\.stageEnum)
        .filter { ![.done, .error].contains($0) }
        .last
    return activeStage?.symbol ?? "checkmark.shield.fill"
}
```

`Header.swift` is unchanged — the icon is owned by `MenuBarExtra` in `App.swift`, which already binds `store.iconName`.

### StageBadge state computation (per pipeline stage)

Given `(job: SandboxJob, badge: ScanStage)`:

```
let current = job.stageEnum
let order = ScanStage.pipeline.firstIndex(of: badge)!
let currentOrder = current.flatMap { ScanStage.pipeline.firstIndex(of: $0) }

// Error replaces the current stage badge with red.
if current == .error && currentOrder == order { return .error(job.detail) }

// After done — every stage in the pipeline is either done or skipped.
if current == .done || job.status == "quarantine_kept" || job.status == "restored" {
    if stageWasExecuted(badge, job) {
        return .done(verdictText: verdictText(badge, job))
    } else {
        return .skipped
    }
}

// Mid-flight: stages strictly before the current are done, equal is current,
// strictly after are pending.
guard let cur = currentOrder else { return .pending }
if order < cur { return .done(verdictText: verdictText(badge, job)) }
if order == cur { return .current }
return .pending
```

`stageWasExecuted(badge, job)` decides whether a stage was actually run versus skipped:

| Badge | Was executed if |
|---|---|
| `cacheCheck` | always — every job goes through cache lookup |
| `localScan` | `pompelmi_verdict != nil` |
| `vtUpload` | `vt_verdict != nil` (or `vt_verdict == "oversized"`, which means we considered it but skipped upload — see below) |
| `vtPoll` | same as `vtUpload`, except `oversized` counts as skipped |
| `done` | `status` is one of `quarantine_kept`, `restored`, or `vt_verdict != nil` |

Edge case: `oversized`. `vtUpload` is shown as `done · oversized` (we considered the file but bailed); `vtPoll` is `skipped`. This keeps the row honest — the user sees which scanner chose to bail.

`verdictText(badge, job)`:

| Badge | Text |
|---|---|
| `cacheCheck` | "hit `clean`" / "hit `infected`" if cache had a verdict; "miss" if it didn't (heuristic: if `pompelmi_verdict` or `vt_verdict` is set, this was a miss) |
| `localScan` | `pompelmi_verdict` (`clean` / `infected` / `error`) |
| `vtUpload` | nothing, or `oversized` |
| `vtPoll` | the engine count if we have it (`14/72` style); otherwise `vt_verdict` |
| `done` | the final verdict (`clean` / `infected` / `inconclusive`) |

### Visual layout

The expanded-row body, after PR2, is:

```
[verdict-line: optional, only for infected/inconclusive — bold pill + threat name]
[<-- StageRow: 5 badges in a single horizontal row -->]
[meta-pills row: filename · age]
[action-buttons row: Open in sandbox / Restore / Delete]
```

Mid-scan (cache miss, local clean, VT uploading):

```
✓ Cache · miss   ✓ Local · clean   ⏳ VT upload   ○ VT poll   ○ Done
```

Cache hit:

```
✓ Cache · hit clean   — Local   — VT upload   — VT poll   ✓ Done · clean
```

Done, infected (Trojan.Win32.Generic):

```
[⚠ Infected]  Trojan.Win32.Generic
✓ Cache · miss   ⚠ Local · infected   ✓ VT upload   ⚠ VT · 14/72   ⚠ Done · infected
```

Skipped badges use a strike-through label and the muted grey tint. Pending badges use a muted grey but no strike-through (to distinguish "not run yet" from "definitely skipped").

### Tints

| Badge state | Background | Text | Border |
|---|---|---|---|
| `done(clean)` | `Theme.verdictGreenBg` | `Theme.verdictGreenFg` | `Theme.verdictGreenFg.opacity(0.4)` |
| `done(infected/inconclusive)` | `Theme.verdictRedBg` | `Theme.verdictRedFg` | `Theme.verdictRedFg.opacity(0.4)` |
| `current` (non-local stages) | `Theme.verdictBlueBg` | `Theme.verdictBlueFg` | `Theme.verdictBlueFg.opacity(0.4)` |
| `current` (`localScan` only) | `Theme.verdictOrangeBg` | `Theme.verdictOrangeFg` | `Theme.verdictOrangeFg.opacity(0.4)` |
| `pending` | `Theme.subtleBg` | secondary 60% | `Theme.separator` |
| `skipped` | `Theme.subtleBg` | secondary 50% (strikethrough) | `Theme.separator` |
| `error` | `Theme.verdictRedBg` | `Theme.verdictRedFg` | `Theme.verdictRedFg.opacity(0.4)` |

Local-scan-specific override is captured in the `current (localScan only)` row above — orange family instead of blue, to match the colour system established in earlier mockups.

### Header rotate icon — full mapping

`iconName` returns the SF Symbol of the **latest active stage across all jobs**, with overrides:

| Condition | SF Symbol |
|---|---|
| `!isConnected` | `shield.slash` |
| Any active threat (`vt_verdict == "infected"` and `status == "quarantine_kept"`) | `exclamationmark.shield.fill` |
| Any job with `stageEnum == .received` (newest wins) | `tray.and.arrow.down` |
| Any job with `stageEnum == .cacheCheck` (newest wins) | `magnifyingglass` |
| Any job with `stageEnum == .localScan` | `shield.lefthalf.filled` |
| Any job with `stageEnum == .vtUpload` | `arrow.up.circle` |
| Any job with `stageEnum == .vtPoll` | `arrow.triangle.2.circlepath` |
| Any job with `stageEnum == .error` | `exclamationmark.triangle.fill` |
| Otherwise | `checkmark.shield.fill` |

If multiple stages are active across multiple jobs, the latest in pipeline order wins (so a `vt_poll` job dominates a parallel `cache_check` job). This matches the user's mental model — "what is the most committed work happening right now."

### Edge cases

| Situation | Behaviour |
|---|---|
| `scan_stage == nil` (legacy job, no migration backfill) | Treated as `done` — every badge is either done or skipped using the `stageWasExecuted` heuristic. The collapsed row uses the verdict pill, not the stage pill. |
| Cache hit | Daemon writes `cache_check` then jumps to `done`. Badges: `done(hit-verdict) · skipped · skipped · skipped · done(verdict)`. Stage pill in collapsed row never appears (the transition is sub-100ms). |
| Pompelmi disabled | Daemon never writes `local_scan`. `localScan` badge is `skipped`. |
| VT disabled | Daemon never writes `vt_upload` / `vt_poll`. Both badges are `skipped`. The `done` badge still renders with the local verdict. |
| Both engines disabled | All three middle badges are `skipped`, `done` shows verdict `inconclusive`. |
| Job stuck (e.g. VT API timeout) | Stage stays at `vt_poll` for as long as it stays. The badge stays `current` and the menu-bar icon stays `arrow.triangle.2.circlepath`. The pending poll loop already retries. |
| Daemon offline mid-scan | `iconName` returns `shield.slash`; `Header` shows red `Disconnected` chip. Each tab body shows the offline message (existing behaviour). The last-known stage stays in the row — we don't try to fake a "stale" state, since the disconnected chip is the global signal. |
| `error` stage | The current pipeline badge becomes red `error`; subsequent badges are `pending` (or stay `skipped` if they would have been). The `detail` field is shown as the threat-name caption above the row. |

### Verification

- `cd macos-menubar && swift build` passes clean.
- `yarn test` passes for daemon changes.
- Manual:
  - Drop a fresh file with both engines on; watch the badge row walk through all five stages live (poll interval already drops to 2s while a job is `scanning`, see `JobStore.targetPollInterval`).
  - Drop the same file again; cache hit produces `cache · hit clean — local — vt up — vt poll — done · clean` essentially instantly.
  - Disable pompelmi in Settings; drop a new file; confirm `local` badge is grey strike-through "skipped".
  - Pull the network plug during VT polling; confirm icon stays at `arrow.triangle.2.circlepath` and the connection chip flips to red.
  - Force an error in the daemon (kill clamd while pompelmi is enabled) and watch the row turn red at `local`.

### Size

- Daemon: 3 files (`job-store.ts`, `watcher.ts`, `virus-checker.ts`), ~50 LOC delta.
- Swift: 1 model file modified (`JobStore.swift`), 1 enum file added (`ScanStage.swift`), 3 components added (`StagePill.swift`, `StageBadge.swift`, `StageRow.swift`), 1 view modified (`JobsTabView.swift`).
- Total: ~8 files, 3-4 commits.

---

## Theme tokens

PR1 introduces no new tokens. PR2 reuses the existing `Theme` colour palette; no additions needed. If a future iteration wants a dedicated "in-flight network" tint distinct from `verdictBlueFg`, that would be additive and out of scope here.

---

## Acceptance checklist (manual, end-to-end after both PRs)

- Open the dropdown with a fresh file dropping in. Collapsed row shows a blue `⏳ VT upload` (or whichever current stage) pill. Menu-bar icon is `arrow.up.circle` while uploading.
- Click the row to expand. The horizontal badge row shows `✓ Cache` (green), `✓ Local · clean` (green), `⏳ VT upload` (blue, current), `○ VT poll` (grey), `○ Done` (grey).
- Wait for VT to finish. Badges progress to `✓ VT · X/72`, then `✓ Done · clean` or `⚠ Done · infected`. Collapsed row's pill swaps from stage pill to verdict pill.
- Disable pompelmi in Settings. Drop a fresh file. The `local` badge in the expanded row is grey with strike-through `— Local · skipped`. Done badge is `✓ Done · clean`.
- Drop the same file twice. Second drop renders `✓ Cache · hit clean` near-instantly with the rest skipped.
- Tear down VT API access (revoke key in Settings). Drop a file with both engines on. Pompelmi finishes; VT upload errors. Badge row: `✓ Cache · miss`, `✓ Local · clean`, red `⚠ VT upload error`, grey pending badges. Detail string shows the upload error message.

---

## Open work

- Decide whether `error` should be retryable from the UI (a small `↻` button on the error badge). Out of scope for this iteration, noted for follow-up.
- Stage timing telemetry (per-stage duration histograms) would be useful for tuning the VT poll interval. Out of scope.
- A per-stage cancel (e.g. "skip the local scan and go straight to VT") might be valuable for very large archives. Out of scope.
