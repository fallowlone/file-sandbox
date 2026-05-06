# Menubar scan visibility — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface in real time which file is being checked and by which scanner. Render `pompelmi_verdict` alongside the VirusTotal verdict, add a `scan_stage` field that walks the pipeline (cache → local → vt-upload → vt-poll → done), and visualise it as a row of badges in the expanded job row plus a stage-coloured pill in the collapsed row plus a per-stage menu-bar icon.

**Architecture:** Two PRs. PR1 is Swift-only and adds the missing `pompelmi_verdict` field plus a Local engine card. PR2 adds a `scan_stage TEXT` column to the daemon's `jobs` table, writes it on every pipeline transition, and consumes it in three new SwiftUI components (`StagePill`, `StageBadge`, `StageRow`) plus an updated `JobStore.iconName` getter that swaps the menu-bar SF Symbol per stage.

**Tech Stack:** TypeScript / Node `--test` runner / better-sqlite3 (daemon side). SwiftUI / AppKit / SwiftPM (menu bar app). No new dependencies on either side.

---

## Pre-flight

- The `macos-menubar/` SwiftPM module has **no XCTest target**. Verification per Swift task is `swift build` clean + manual smoke checks. The acceptance pass at the end of each PR runs the spec's manual checklist.
- The `src/` daemon has Node `--test` files (`*.test.ts`). Use them for TDD on every daemon change.
- All `swift build` commands run from `macos-menubar/`.
- All daemon commands run from the repo root.
- Working tree should be clean before starting.

---

## Pre-flight Task: commit the design spec

**Files:**
- Already exists: `docs/superpowers/specs/2026-05-06-menubar-scan-visibility-design.md`

- [ ] **Step 1: Verify the spec file exists**

Run: `ls -la docs/superpowers/specs/2026-05-06-menubar-scan-visibility-design.md`
Expected: file is present, non-empty.

- [ ] **Step 2: Commit the spec on `main`**

```bash
git add docs/superpowers/specs/2026-05-06-menubar-scan-visibility-design.md
git commit -m "docs: spec for menubar scan visibility (pompelmi card + scan_stage)"
```

---

# PR1 — pompelmi card visibility

Three Swift-only files, ~30 LOC delta, one feature branch off `main`.

## Task 1: Branch + add `pompelmi_verdict` to `SandboxJob`

**Files:**
- Modify: `macos-menubar/Sources/App/JobStore.swift` (around line 33-41, the `SandboxJob` struct)

- [ ] **Step 1: Create the feature branch**

```bash
git switch -c feat/menubar-pompelmi-card
```

Expected: `Switched to a new branch 'feat/menubar-pompelmi-card'`.

- [ ] **Step 2: Add the field to `SandboxJob`**

Open `macos-menubar/Sources/App/JobStore.swift`. Find the struct:

```swift
struct SandboxJob: Codable, Identifiable, Equatable {
    let id: String
    let original_name: String
    let status: String
    let vt_verdict: String?
    let detail: String?
    let final_path: String?
    let created_at: Int
}
```

Replace with:

```swift
struct SandboxJob: Codable, Identifiable, Equatable {
    let id: String
    let original_name: String
    let status: String
    let vt_verdict: String?
    let pompelmi_verdict: String?
    let detail: String?
    let final_path: String?
    let created_at: Int
}
```

The daemon already returns this field; `JSONDecoder` was silently dropping it.

- [ ] **Step 3: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/JobStore.swift
git commit -m "feat(menubar): decode pompelmi_verdict in SandboxJob"
```

---

## Task 2: Render Local engine card alongside VirusTotal

**Files:**
- Modify: `macos-menubar/Sources/App/Tabs/JobsTabView.swift` (the `expandedDetail` block, around lines 213-296)

- [ ] **Step 1: Read the existing `expandedDetail`**

Open the file and locate the section that renders engine cards. Today it's a single `HStack(spacing: 6)` with a single `EngineCard` for VirusTotal:

```swift
HStack(spacing: 6) {
    EngineCard(
        label: "VirusTotal",
        value: job.vt_verdict ?? "-",
        status: engineStatus(for: job.vt_verdict)
    )
}
```

- [ ] **Step 2: Replace with a two-card row**

Replace that block with:

```swift
HStack(spacing: 6) {
    if let pompelmi = job.pompelmi_verdict, !pompelmi.isEmpty {
        EngineCard(
            label: "Local",
            value: pompelmi,
            status: localEngineStatus(for: pompelmi)
        )
    }
    if let vt = job.vt_verdict, !vt.isEmpty {
        EngineCard(
            label: "VirusTotal",
            value: vt,
            status: engineStatus(for: vt)
        )
    }
}
```

- [ ] **Step 3: Add the helper that maps pompelmi verdict → status**

Below the existing `private func engineStatus(for verdict: String?) -> EngineCard.Status` add:

```swift
    private func localEngineStatus(for verdict: String) -> EngineCard.Status {
        switch verdict.lowercased() {
        case "clean":     return .clean
        case "malicious": return .malicious
        case "error":     return .warn
        default:          return .neutral
        }
    }
```

- [ ] **Step 4: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 5: Manual smoke**

```bash
cd macos-menubar && bash build.sh && cd ..
open macos-menubar/FileSandboxMenuBar.app
```

With `pompelmiEnabled=true` and a working VT API key, drop a file in the watch path. Open the dropdown, expand the row. Both cards (Local + VirusTotal) should render side by side. Disable pompelmi in Settings, drop another file — only the VT card renders for new jobs.

- [ ] **Step 6: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/JobsTabView.swift
git commit -m "feat(menubar): render Local engine card alongside VirusTotal"
```

---

## Task 3: Update `VerdictPill.forJobVerdict` to consider both verdicts

**Files:**
- Modify: `macos-menubar/Sources/App/Components/VerdictPill.swift`

- [ ] **Step 1: Read the existing `forJobVerdict`**

The current static helper takes `vt_verdict` and `status`. Find the function signature.

- [ ] **Step 2: Replace the existing `forJobVerdict(verdict:status:)` with a two-verdict variant**

Open `macos-menubar/Sources/App/Components/VerdictPill.swift`. Replace the existing extension block (`extension VerdictPill { static func forJobVerdict(verdict:status:) -> VerdictPill? { ... } }`) with this single new implementation:

```swift
extension VerdictPill {
    /// Map both engine verdicts + job status to a collapsed-row pill.
    ///
    /// Priority:
    ///   1. Either verdict is `infected` / `malicious`             → red "infected"
    ///   2. status is `scanning` / `received` / `in_quarantine`    → blue "scanning"
    ///   3. vt_verdict == `inconclusive` / `unclear`               → orange "inconclusive"
    ///   4. vt_verdict == `oversized`                              → grey "oversized"
    ///   5. status == `restored` or vt_verdict == `clean`          → green "clean"
    ///   6. otherwise                                              → nil
    static func forJobVerdict(vt: String?, pompelmi: String?, status: String) -> VerdictPill? {
        let v = (vt ?? "").lowercased()
        let p = (pompelmi ?? "").lowercased()

        if v == "infected" || v == "malicious" || p == "malicious" {
            return VerdictPill(text: L.verdict("infected"), variant: .red, size: .mini, symbol: "exclamationmark.triangle.fill")
        }
        if status == "scanning" || status == "received" {
            return VerdictPill(text: L.verdict("scanning"), variant: .blue, size: .mini, symbol: "hourglass")
        }
        if status == "in_quarantine" {
            return VerdictPill(text: L.verdict("queued"), variant: .blue, size: .mini, symbol: "tray")
        }
        if v == "inconclusive" || v == "unclear" {
            return VerdictPill(text: L.verdict("inconclusive"), variant: .orange, size: .mini, symbol: "questionmark.circle.fill")
        }
        if v == "oversized" {
            return VerdictPill(text: L.verdict("oversized"), variant: .grey, size: .mini, symbol: "arrow.down.circle")
        }
        if v == "clean" || status == "restored" {
            return VerdictPill(text: L.verdict("clean"), variant: .green, size: .mini, symbol: "checkmark.circle.fill")
        }
        return nil
    }
}
```

Note: the existing helper used `L.verdict(...)` — keep that exact identifier, do not invent `L.verdictMini`.

- [ ] **Step 3: Migrate the call site in `JobsTabView`**

In `macos-menubar/Sources/App/Tabs/JobsTabView.swift`, find:

```swift
if let pill = VerdictPill.forJobVerdict(verdict: job.vt_verdict, status: job.status) {
    pill
}
```

Replace with:

```swift
if let pill = VerdictPill.forJobVerdict(
    vt: job.vt_verdict,
    pompelmi: job.pompelmi_verdict,
    status: job.status
) {
    pill
}
```

- [ ] **Step 4: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Components/VerdictPill.swift macos-menubar/Sources/App/Tabs/JobsTabView.swift
git commit -m "feat(menubar): VerdictPill considers pompelmi verdict in collapsed row"
```

---

## Task 4: PR1 acceptance + push

- [ ] **Step 1: Re-build the app and run end-to-end**

```bash
cd macos-menubar && bash build.sh && cd ..
open macos-menubar/FileSandboxMenuBar.app
```

- [ ] **Step 2: Walk the spec's PR1 acceptance**

Per `docs/superpowers/specs/2026-05-06-menubar-scan-visibility-design.md` § PR1 § Verification:
- Drop a file with both engines on; expand the row → both cards visible.
- Disable pompelmi; drop another file; expand → only VT card.
- A job that finishes with `pompelmi=malicious` shows a red `infected` mini pill in the collapsed row even when VT comes back clean.

- [ ] **Step 3: Push the branch and open a PR**

```bash
git push -u origin feat/menubar-pompelmi-card
```

PR title: `feat(menubar): pompelmi engine card in expanded job row`
PR body: link to the spec and to this plan, list the three commits.

---

# PR2 — live scan stage tracking

Daemon adds a `scan_stage` column and writes it on every pipeline transition. Swift consumes it via three new components plus updated `iconName`. ~8 files, 3-4 commits.

## Task 5: Branch from main

- [ ] **Step 1: Return to `main`**

```bash
git switch main
```

(Assumes PR1 has been merged or is in review on its own branch. If PR1 is still open, branch from `main` regardless — the two PRs touch overlapping files but conflicts are trivial.)

- [ ] **Step 2: Create the PR2 branch**

```bash
git switch -c feat/menubar-scan-stage
```

---

## Task 6: Add `scan_stage` column + `setStage` to `JobStore` (TS)

**Files:**
- Modify: `src/job-store.ts` (the constructor, `JobRow` type, every SELECT, plus a new `setStage` method)
- Test: `src/job-store.test.ts` (append a new test)

- [ ] **Step 1: Write the failing test**

Append to `src/job-store.test.ts`:

```ts
test("scan_stage starts null and persists when set", () => {
  const store = freshStore();
  store.insertReceived("job-stage-1", "/a.bin", "a.bin");

  const before = store.getJob("job-stage-1");
  assert.equal(before?.scan_stage, null);

  store.setStage("job-stage-1", "cache_check");
  const afterCache = store.getJob("job-stage-1");
  assert.equal(afterCache?.scan_stage, "cache_check");

  store.setStage("job-stage-1", "done");
  const afterDone = store.getJob("job-stage-1");
  assert.equal(afterDone?.scan_stage, "done");
});

test("scan_stage rejects unknown values via type system", () => {
  // Compile-time check: this is here so the reviewer remembers ScanStage is
  // a closed string union. Runtime check is not required — DB column is TEXT.
  const valid: import("./job-store.ts").ScanStage = "vt_poll";
  assert.equal(valid, "vt_poll");
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
node --experimental-strip-types --test 'src/job-store.test.ts'
```

Expected: failure mentioning `setStage` is not a function.

- [ ] **Step 3: Add the `ScanStage` type and migrate the column**

In `src/job-store.ts`, after the `JobStatus` type union, add:

```ts
export type ScanStage =
  | "received"
  | "cache_check"
  | "local_scan"
  | "vt_upload"
  | "vt_poll"
  | "done"
  | "error";
```

Add `scan_stage: ScanStage | null;` to the `JobRow` type after `pompelmi_verdict`.

In the constructor, after the existing `pompelmi_verdict` ALTER, add the same idempotent migration for `scan_stage`:

```ts
    // Idempotent column add for scan_stage
    try {
      this.db.exec("ALTER TABLE jobs ADD COLUMN scan_stage TEXT");
    } catch (e) {
      if (!String(e).includes("duplicate column name")) throw e;
    }
```

- [ ] **Step 4: Add the `setStage` method**

After `setPompelmiVerdict`, add:

```ts
  setStage(jobId: string, stage: ScanStage): void {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET scan_stage = ?, updated_at = ? WHERE id = ?`,
      )
      .run(stage, now, jobId);
  }
```

- [ ] **Step 5: Add `scan_stage` to every SELECT**

There are three SELECT statements (`listRecent`, `getJob`, `listInconclusiveOlderThan`). In each, append `, scan_stage` to the column list. Example for `listRecent`:

```ts
  listRecent(limit = 100): JobRow[] {
    const rows = this.db
      .prepare(
        `SELECT id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, scan_stage, detail, created_at, updated_at
         FROM jobs ORDER BY created_at DESC LIMIT ?`,
      )
      .all(limit) as JobRow[];
    return rows;
  }
```

Apply the same pattern to `getJob` and `listInconclusiveOlderThan`.

- [ ] **Step 6: Run the tests**

```bash
node --experimental-strip-types --test 'src/job-store.test.ts'
```

Expected: all tests pass, including both new ones.

- [ ] **Step 7: Run the full daemon test suite**

```bash
yarn test
```

Expected: every test passes (no regressions).

- [ ] **Step 8: Commit**

```bash
git add src/job-store.ts src/job-store.test.ts
git commit -m "feat(daemon): add scan_stage column + setStage to JobStore"
```

---

## Task 7: Wire `setStage` into the watcher (received)

**Files:**
- Modify: `src/watcher.ts` — call `setStage(id, "received")` immediately after `chmod 0o000` succeeds.

- [ ] **Step 1: Locate the chmod call**

Open `src/watcher.ts`. Find the place that does `chmod(filePath, 0o000)` for a freshly-detected file. It is in the same handler that calls `jobStore.insertReceived(...)`.

- [ ] **Step 2: Call `setStage` after the lock-down**

Right after the `chmod` succeeds (and after `insertReceived` has run, so the row exists), add:

```ts
      jobStore.setStage(jobId, "received");
```

If the chmod call already runs inside a try/catch where success branches into a follow-up block, add the `setStage` call to the success branch.

- [ ] **Step 3: Verify the daemon starts**

```bash
yarn start:local
```

(Cancel after a few seconds with Ctrl+C — we just want to confirm it boots.) Expected: no errors in stdout.

- [ ] **Step 4: Run the watcher tests**

```bash
node --experimental-strip-types --test 'src/watcher.test.ts'
```

Expected: all tests pass. (No new test required — `received` wiring is verified end-to-end in the acceptance task.)

- [ ] **Step 5: Commit**

```bash
git add src/watcher.ts
git commit -m "feat(daemon): mark scan_stage=received after chmod 0o000"
```

---

## Task 8: Wire `setStage` into virus-checker (cache, local, vt-upload, vt-poll, done, error)

**Files:**
- Modify: `src/virus-checker.ts` — write `scan_stage` at every pipeline transition.

- [ ] **Step 1: Map pipeline branches to stages**

Open `src/virus-checker.ts`. The high-level pipeline (current code) is:

```
checkVtCache(...)           // cache_check
runPompelmi(...)             // local_scan (only if pompelmi enabled)
vtScanFile(...)              // vt_upload
pollAnalysis(...)             // vt_poll
returnVerdict(...)            // done
```

For each branch, insert a `jobStore.setStage(jobId, "<stage>")` call immediately before the network/IO work.

- [ ] **Step 2: Wrap the entire pipeline in a try/catch that writes `error`**

Around the function that runs the pipeline, wrap the body in:

```ts
try {
  jobStore.setStage(jobId, "cache_check");
  // ... existing cache check code ...

  if (pompelmiEnabled) {
    jobStore.setStage(jobId, "local_scan");
    // ... existing pompelmi call ...
  }

  jobStore.setStage(jobId, "vt_upload");
  // ... existing VT upload code ...

  jobStore.setStage(jobId, "vt_poll");
  // ... existing VT poll loop ...

  jobStore.setStage(jobId, "done");
  // ... existing verdict write ...
} catch (err) {
  jobStore.setStage(jobId, "error");
  throw err; // re-raise so existing failure handling still runs
}
```

The exact placement of each `setStage` call must be **before** the corresponding work begins, so the UI shows the stage *while* the work is happening, not *after* it completes.

- [ ] **Step 3: Verify the daemon test suite**

```bash
yarn test
```

Expected: all tests pass.

- [ ] **Step 4: Manual end-to-end smoke**

```bash
yarn start:local
```

In a second terminal:

```bash
echo "test" > ~/Downloads/test-stage-$(date +%s).txt
sleep 1
sqlite3 ~/.file-sandbox/jobs.sqlite "select id, status, scan_stage from jobs order by created_at desc limit 5"
```

Expected: rows show `scan_stage` walking forward (`received` → `cache_check` → either `local_scan` or `vt_upload` → ... → `done`).

- [ ] **Step 5: Commit**

```bash
git add src/virus-checker.ts
git commit -m "feat(daemon): write scan_stage on every virus-checker transition"
```

---

## Task 9: Add `scan_stage` to the Swift `SandboxJob`

**Files:**
- Modify: `macos-menubar/Sources/App/JobStore.swift` (the `SandboxJob` struct around line 33-42)

- [ ] **Step 1: Add the field**

Replace:

```swift
struct SandboxJob: Codable, Identifiable, Equatable {
    let id: String
    let original_name: String
    let status: String
    let vt_verdict: String?
    let pompelmi_verdict: String?
    let detail: String?
    let final_path: String?
    let created_at: Int
}
```

with:

```swift
struct SandboxJob: Codable, Identifiable, Equatable {
    let id: String
    let original_name: String
    let status: String
    let vt_verdict: String?
    let pompelmi_verdict: String?
    let scan_stage: String?
    let detail: String?
    let final_path: String?
    let created_at: Int
}
```

- [ ] **Step 2: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/JobStore.swift
git commit -m "feat(menubar): decode scan_stage in SandboxJob"
```

---

## Task 10: Create the `ScanStage` enum

**Files:**
- Create: `macos-menubar/Sources/App/ScanStage.swift`

- [ ] **Step 1: Write the enum**

Create the new file with this exact content:

```swift
import SwiftUI

/// Mirrors the daemon `ScanStage` union (src/job-store.ts).
///
/// The pipeline order used to render the badge row in `JobsTabView` is
/// available as `ScanStage.pipeline`. Note that `received` and `error` are
/// not in `pipeline`: `received` is too short-lived to merit a badge, and
/// `error` replaces the *current* pipeline badge's tint rather than adding
/// its own column.
enum ScanStage: String, CaseIterable {
    case received
    case cacheCheck = "cache_check"
    case localScan  = "local_scan"
    case vtUpload   = "vt_upload"
    case vtPoll     = "vt_poll"
    case done
    case error

    static let pipeline: [ScanStage] = [.cacheCheck, .localScan, .vtUpload, .vtPoll, .done]

    /// Short label rendered inside `StagePill` and `StageBadge`.
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

    /// SF Symbol name used by `JobStore.iconName` and `StagePill`.
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

    /// Foreground tint for `StagePill` and for a `current`-state `StageBadge`.
    var tint: Color {
        switch self {
        case .received, .cacheCheck, .vtUpload, .vtPoll: return Theme.verdictBlueFg
        case .localScan:                                  return Theme.verdictOrangeFg
        case .done:                                       return Theme.verdictGreenFg
        case .error:                                      return Theme.verdictRedFg
        }
    }
}

extension SandboxJob {
    /// Parsed `scan_stage` field, or `nil` for legacy rows.
    var stageEnum: ScanStage? {
        scan_stage.flatMap(ScanStage.init(rawValue:))
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/ScanStage.swift
git commit -m "feat(menubar): ScanStage enum + SandboxJob.stageEnum helper"
```

---

## Task 11: `StagePill` component (collapsed-row pill)

**Files:**
- Create: `macos-menubar/Sources/App/Components/StagePill.swift`

- [ ] **Step 1: Write the component**

```swift
import SwiftUI

/// A small icon+label pill rendered in the collapsed job row while a scan is
/// in flight (`stageEnum != nil && stageEnum != .done && stageEnum != .error`).
/// Replaces the verdict mini-pill until the scan finishes.
struct StagePill: View {
    let stage: ScanStage

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: stage.symbol)
                .font(.system(size: 9, weight: .semibold))
            Text(stage.label)
                .font(.system(size: 9, weight: .semibold))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 2)
        .background(backgroundTint)
        .foregroundColor(stage.tint)
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var backgroundTint: Color {
        switch stage {
        case .localScan: return Theme.verdictOrangeBg
        case .done:      return Theme.verdictGreenBg
        case .error:     return Theme.verdictRedBg
        default:         return Theme.verdictBlueBg
        }
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/StagePill.swift
git commit -m "feat(menubar): StagePill component for collapsed-row in-flight scans"
```

---

## Task 12: `StageBadge` component (single badge in expanded row)

**Files:**
- Create: `macos-menubar/Sources/App/Components/StageBadge.swift`

- [ ] **Step 1: Write the component**

```swift
import SwiftUI

/// A single badge in the `StageRow`. The state controls colour, strikethrough,
/// and whether a verdict text suffix is rendered.
struct StageBadge: View {
    enum State {
        case done(verdictText: String?)
        case current(stage: ScanStage)  // stage carries the localScan-orange override
        case pending
        case skipped
        case error(detail: String?)
    }

    let stage: ScanStage
    let state: State

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: leadingSymbol)
                .font(.system(size: 9, weight: .semibold))
            Text(displayText)
                .font(.system(size: 10, weight: .medium))
                .strikethrough(isSkipped, color: Theme.separator)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 3)
        .background(backgroundTint)
        .foregroundColor(foregroundTint)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(borderTint, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var displayText: String {
        switch state {
        case .done(let verdict):
            if let v = verdict, !v.isEmpty { return "\(stage.label) · \(v)" }
            return stage.label
        case .current:                       return stage.label
        case .pending:                       return stage.label
        case .skipped:                       return stage.label
        case .error:                         return "\(stage.label) error"
        }
    }

    private var leadingSymbol: String {
        switch state {
        case .done:    return "checkmark"
        case .current: return "hourglass"
        case .pending: return "circle"
        case .skipped: return "minus"
        case .error:   return "exclamationmark.triangle.fill"
        }
    }

    private var isSkipped: Bool {
        if case .skipped = state { return true }
        return false
    }

    // MARK: - Tints

    private var backgroundTint: Color {
        switch state {
        case .done(let verdict):
            return verdictIsBad(verdict) ? Theme.verdictRedBg : Theme.verdictGreenBg
        case .current(let s):
            return s == .localScan ? Theme.verdictOrangeBg : Theme.verdictBlueBg
        case .pending, .skipped:
            return Theme.subtleBg
        case .error:
            return Theme.verdictRedBg
        }
    }

    private var foregroundTint: Color {
        switch state {
        case .done(let verdict):
            return verdictIsBad(verdict) ? Theme.verdictRedFg : Theme.verdictGreenFg
        case .current(let s):
            return s == .localScan ? Theme.verdictOrangeFg : Theme.verdictBlueFg
        case .pending:
            return Color.secondary.opacity(0.7)
        case .skipped:
            return Color.secondary.opacity(0.5)
        case .error:
            return Theme.verdictRedFg
        }
    }

    private var borderTint: Color {
        switch state {
        case .done(let verdict):
            return verdictIsBad(verdict) ? Theme.verdictRedFg.opacity(0.4) : Theme.verdictGreenFg.opacity(0.4)
        case .current(let s):
            return s == .localScan
                ? Theme.verdictOrangeFg.opacity(0.4)
                : Theme.verdictBlueFg.opacity(0.4)
        case .pending, .skipped:
            return Theme.separator
        case .error:
            return Theme.verdictRedFg.opacity(0.4)
        }
    }

    /// True when a "done" verdict text indicates a bad finding (infected,
    /// inconclusive). Falsy values, "clean", and pure stage progress markers
    /// (e.g. "miss", "hit clean") render in green.
    private func verdictIsBad(_ verdict: String?) -> Bool {
        guard let v = verdict?.lowercased() else { return false }
        return v.contains("infect") || v == "inconclusive" || v.contains("malic")
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/StageBadge.swift
git commit -m "feat(menubar): StageBadge component (done/current/pending/skipped/error)"
```

---

## Task 13: `StageRow` component (5-badge row in expanded view)

**Files:**
- Create: `macos-menubar/Sources/App/Components/StageRow.swift`

- [ ] **Step 1: Write the component**

```swift
import SwiftUI

/// Always-five-badges row that renders the cache → local → vt-upload → vt-poll
/// → done pipeline. Each badge's state is computed from the job's
/// `stageEnum`, `pompelmi_verdict`, and `vt_verdict`.
struct StageRow: View {
    let job: SandboxJob

    var body: some View {
        HStack(spacing: 6) {
            ForEach(ScanStage.pipeline, id: \.self) { stage in
                StageBadge(stage: stage, state: state(for: stage))
            }
        }
    }

    // MARK: - State computation

    private func state(for badge: ScanStage) -> StageBadge.State {
        let current = job.stageEnum
        guard let order = ScanStage.pipeline.firstIndex(of: badge) else {
            return .pending  // unreachable
        }
        let currentOrder = current.flatMap { ScanStage.pipeline.firstIndex(of: $0) }

        // Error replaces the current stage badge with red.
        if current == .error, currentOrder == order {
            return .error(detail: job.detail)
        }

        // Terminal job — every badge is either `done` or `skipped`.
        if isTerminal(job) {
            return wasExecuted(badge) ? .done(verdictText: verdictText(for: badge)) : .skipped
        }

        // Mid-flight: stages strictly before current are done, equal is current,
        // strictly after are pending.
        guard let cur = currentOrder else { return .pending }
        if order < cur { return .done(verdictText: verdictText(for: badge)) }
        if order == cur { return .current(stage: badge) }
        return .pending
    }

    private func isTerminal(_ job: SandboxJob) -> Bool {
        if job.stageEnum == .done { return true }
        if job.status == "quarantine_kept" || job.status == "restored" { return true }
        return false
    }

    private func wasExecuted(_ badge: ScanStage) -> Bool {
        switch badge {
        case .cacheCheck:
            return true  // every job touches the cache
        case .localScan:
            return job.pompelmi_verdict != nil
        case .vtUpload:
            // oversized counts as "executed" so the user sees that VT was
            // considered but bailed.
            return job.vt_verdict != nil
        case .vtPoll:
            // oversized = upload phase decided to bail, no poll happened.
            let v = (job.vt_verdict ?? "").lowercased()
            return job.vt_verdict != nil && v != "oversized"
        case .done:
            return job.status == "quarantine_kept" || job.status == "restored" || job.vt_verdict != nil
        default:
            return false
        }
    }

    private func verdictText(for badge: ScanStage) -> String? {
        switch badge {
        case .cacheCheck:
            // Heuristic: if either engine produced a verdict, this was a cache miss.
            return (job.pompelmi_verdict != nil || job.vt_verdict != nil) ? "miss" : "hit"
        case .localScan:
            return job.pompelmi_verdict
        case .vtUpload:
            let v = (job.vt_verdict ?? "").lowercased()
            return v == "oversized" ? "oversized" : nil
        case .vtPoll:
            return job.vt_verdict
        case .done:
            return job.vt_verdict ?? job.pompelmi_verdict
        default:
            return nil
        }
    }
}
```

- [ ] **Step 2: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/StageRow.swift
git commit -m "feat(menubar): StageRow renders pipeline badges from job state"
```

---

## Task 14: Wire `StagePill` + `StageRow` into `JobsTabView`

**Files:**
- Modify: `macos-menubar/Sources/App/Tabs/JobsTabView.swift`

- [ ] **Step 1: Replace the engine-card row in `expandedDetail`**

Find the `HStack(spacing: 6)` block that currently renders the two `EngineCard`s (added in PR1 Task 2). Replace the entire block with:

```swift
            StageRow(job: job)
```

- [ ] **Step 2: Add the stage pill to `collapsedRow`**

Find the part of `collapsedRow` that renders the verdict mini pill:

```swift
                if let pill = VerdictPill.forJobVerdict(
                    vt: job.vt_verdict,
                    pompelmi: job.pompelmi_verdict,
                    status: job.status
                ) {
                    pill
                }
```

Replace with:

```swift
                if let stage = job.stageEnum, stage != .done, stage != .error {
                    StagePill(stage: stage)
                } else if let pill = VerdictPill.forJobVerdict(
                    vt: job.vt_verdict,
                    pompelmi: job.pompelmi_verdict,
                    status: job.status
                ) {
                    pill
                }
```

- [ ] **Step 3: Remove now-unused helpers**

The `private func engineStatus(for verdict: String?)` and `private func localEngineStatus(for verdict: String)` helpers in `JobsTabView.swift` are no longer called once `EngineCard` is gone from the expanded view. Delete both.

- [ ] **Step 4: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/JobsTabView.swift
git commit -m "feat(menubar): JobsTabView uses StagePill (collapsed) + StageRow (expanded)"
```

---

## Task 15: Update `JobStore.iconName` to rotate per stage

**Files:**
- Modify: `macos-menubar/Sources/App/JobStore.swift` (the `iconName` getter, around lines 181-190)

- [ ] **Step 1: Replace the getter**

Find:

```swift
    var iconName: String {
        guard isConnected else { return "shield.slash" }
        if !activeThreats.isEmpty {
            return "exclamationmark.shield.fill"
        }
        if jobs.contains(where: { $0.status == "scanning" || $0.status == "in_quarantine" }) {
            return "shield.lefthalf.filled"
        }
        return "checkmark.shield.fill"
    }
```

Replace with:

```swift
    var iconName: String {
        guard isConnected else { return "shield.slash" }
        if !activeThreats.isEmpty {
            return "exclamationmark.shield.fill"
        }
        // Newest in pipeline order wins so vt_poll dominates a parallel cache_check.
        let activeStage = jobs
            .compactMap(\.stageEnum)
            .filter { ![.done].contains($0) }
            .max(by: { lhs, rhs in
                ScanStage.pipeline.firstIndex(of: lhs) ?? -1
                  < ScanStage.pipeline.firstIndex(of: rhs) ?? -1
            })
        if let stage = activeStage { return stage.symbol }
        return "checkmark.shield.fill"
    }
```

Note: `error` stages keep their own SF Symbol (`exclamationmark.triangle.fill`) via `ScanStage.symbol`, so they participate in the comparison naturally — but we want errors to surface to the user. Since `error` is not in `ScanStage.pipeline`, `firstIndex(of:)` returns `nil` and the comparison falls back to `-1`, so a single error job's icon is just its symbol. That's the desired behaviour.

- [ ] **Step 2: Verify build**

```bash
cd macos-menubar && swift build
```

Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/JobStore.swift
git commit -m "feat(menubar): iconName rotates SF Symbol per active scan stage"
```

---

## Task 16: PR2 acceptance + push

- [ ] **Step 1: Re-build + open the app**

```bash
cd macos-menubar && bash build.sh && cd ..
open macos-menubar/FileSandboxMenuBar.app
```

- [ ] **Step 2: Walk the spec's PR2 acceptance**

Per `docs/superpowers/specs/2026-05-06-menubar-scan-visibility-design.md` § Acceptance checklist:

1. Drop a fresh file with both engines on. The collapsed row's pill walks `cache_check → local_scan → vt_upload → vt_poll`. The menu-bar icon rotates accordingly. Expand the row — the badge row shows progression with verdict text on each completed badge.
2. Drop the same file again. Cache hit: badge row is `✓ Cache · hit clean`, three middle badges `— skipped`, `✓ Done · clean`.
3. Disable pompelmi in Settings, drop a fresh file. The `Local` badge in the expanded row is grey strike-through `— Local`.
4. Pull the network plug during VT polling. Icon stays at `arrow.triangle.2.circlepath`. The header chip flips to red `Disconnected`.
5. Force a daemon error (kill clamd while pompelmi is enabled) — `Local` badge turns red `Local error`. Detail string surfaces in the row.

Take a screenshot of state (1) and attach it to the PR.

- [ ] **Step 3: Push the branch and open the PR**

```bash
git push -u origin feat/menubar-scan-stage
```

PR title: `feat: live scan stage tracking in menu bar`
PR body: link to the spec, link to PR1, list the four+ commits.

---

# Self-review notes

- Task 6's "rejects unknown values via type system" test is intentionally light — the column is `TEXT` and the daemon writes only union members. A runtime guard would be over-engineering; the type alias is the contract.
- Task 8 uses textual instructions instead of a unified diff because `src/virus-checker.ts` will receive ~6 small edits across distinct branches and a try/catch wrap. An executor reading this plan should open the file, locate each branch, and add the `setStage` call. If the file diverges from this description (e.g. the pipeline gains a new branch), the executor should ask before guessing.
- Task 12's `StageBadge` does not use the `pending` and `skipped` distinction in the leading symbol when both are `circle` / `minus` — that's intentional. The strikethrough on `skipped` is the visual difference; pending uses an empty circle outline.
- Task 14 deletes `engineStatus` and `localEngineStatus` because the `EngineCard` row is replaced wholesale by `StageRow`. If a follow-up PR re-introduces engine cards anywhere, the helpers can be re-added.
- Task 15's icon getter does not include `received` in its sort, even though `received` is a valid `stageEnum`. This is fine: `received` is sub-second in practice, the user will rarely catch a frame at that stage, and falling back to the green idle shield for that one frame is harmless. If a job does linger at `received` (e.g. the daemon hung between insert and chmod), the shield-slash from `!isConnected` will likely take over anyway.
