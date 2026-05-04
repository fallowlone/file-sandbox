# Menu bar app UI redesign

**Date:** 2026-05-04
**Author:** brainstorm session, file-sandbox
**Status:** design — awaiting implementation plan

## Goal

Replace the current single-pane menu bar layout (header + jobs list + sandbox section + footer) with a tabbed dropdown that follows shadcn-style visual conventions: pill segmented tabs, colour-coded badges, click-to-expand rows, and an inline Settings tab. The redesign keeps the dropdown width at 430 pt, drops the separate Settings window, and tightens information density per macOS Human Interface Guidelines (compact spacing, monochrome status, hover-revealed actions).

## Non-goals

- Cross-platform UI. The app is macOS-only.
- Localisation. Strings remain English-only for this iteration.
- Re-architecting `JobStore` / `SettingsStore` / `SandboxStore`. Bindings stay; only the views and a couple of small properties change.
- New backend endpoints. The redesign reuses every existing daemon API.
- Implementing search, keyboard shortcuts, or notifications other than the existing launch notification.

## Visual reference

Mockups are saved under `.superpowers/brainstorm/41562-1777918458/content/`. The composite reference is `full-assembly.html`. Inline summary below.

### Frame

- Width: `420 pt` (matches today's `frame(width: 420)`).
- Background: `Color(.windowBackground)` / system. Borders: 1pt `Color(.separatorColor)`.
- Corner radius: 12 pt (matches macOS Sonoma+ menu-bar conventions).

### Header (always visible)

```
[ FileSandbox ]                              [ Active ▾ ]  [ ⋯ ]
```

- `FileSandbox` — `Text` 13 pt semibold.
- Status chip — clickable; opens the WatcherMode menu (`active` / `scan_paused` / `monitoring_disabled`). Chip background-tinted by mode:
  - active → `Color(red: 0.9, green: 0.97, blue: 0.92)` bg, `Color(red: 0.11, green: 0.49, blue: 0.23)` fg
  - scan_paused → orange-tinted equivalent
  - monitoring_disabled → red-tinted equivalent
  - Chip text = `mode.displayName`. Chip should hint at "click for menu" via a `▾` glyph or a chevron.
- Overflow `⋯` — opens an NSMenu with: `Refresh`, `Restart daemon`, `View logs`, divider, `Quit FileSandbox`.

### Tabs (pill segmented)

```
[ Jobs 5 ] [ Sandbox 2 ] [ Settings ]
```

- Pill segmented control. Active pill = white bg + 1 pt soft shadow + 600 weight; inactive = transparent + secondary fg.
- Each tab label may carry a count chip in muted weight (e.g. `Jobs 5`). Counts come from `store.jobs.count` (active+quarantine count) and `sandboxStore.sessions.filter { running/starting }.count`.
- Persisted selected tab via `@AppStorage("filesandbox.selectedTab")` so the dropdown re-opens to the user's last tab.

### Jobs tab content

Rows are grouped by job status into three collapsible sections, in order:

1. **Scanning** — jobs with `status == "scanning" || status == "received"`.
2. **Quarantine** — `status == "quarantine_kept"`. Verdicts: infected / inconclusive / oversized.
3. **Restored** — `status == "restored"`. Hidden by default (collapsed).

Group header:

- `▾`/`▸` chevron (left) + group name + count chip.
- Background `Color(.controlBackgroundColor)`; padding `7×14`.
- Click toggles `@State` collapsed flag (separate state per group).

Row (collapsed):

```
▸  📄  archive-2024-09.zip          [⚠ infected]  12m
```

- Padding: `9×14` with `28 pt` left indent so chevron aligns with group header.
- Filename: 12 pt regular, ellipsis on overflow.
- Verdict mini-pill: 9 pt 600 weight, padding `1×8`, radius 8.
  - infected → red-tinted
  - inconclusive / `?` → orange-tinted
  - scanning / `⏳` → blue-tinted
  - oversized → grey-tinted
  - clean → green-tinted (used in Restored group)
- Age: 11 pt secondary, formatted as `5m`, `1h`, `2d`.

Row (expanded):

```
▾  🗜  archive-2024-09.zip          [⚠ infected]  12m
   ┌─────────────────────────────────────────────────────┐
   │ [⚠ Infected]   Trojan.Win32.Generic                 │
   │                                                     │
   │ [● Local clean]   [● VirusTotal 14/72]              │
   │                                                     │
   │ [📁 archive-2024-09.zip] [⏱ 12 min ago] [⇧ 2.4 MB] │
   │                                                     │
   │ [🛡 Open in sandbox] [↻ Restore] [🗑 Delete]        │
   └─────────────────────────────────────────────────────┘
```

- Verdict header: `vbig` lozenge (11 pt 600, padding `3×10`, radius 8) + threat name caption (11 pt secondary).
- Engine cards: white bg, 1 pt border, radius 8, padding `4×9`. Inside: 6 pt round dot (green for clean, red for malicious, orange for error/warn) + label 10 pt 500 weight + value 10 pt secondary.
  - Local card uses `pompelmi_verdict`. Hidden when null.
  - VirusTotal card uses `vt_verdict`. Hidden when null. Shows engine count from result detail when available.
- Meta pills: white bg, 1 pt border, radius 6, padding `3×8`. Lucide-style line icons (folder / clock / upload). Filename pill shows `URL(fileURLWithPath: quarantinePath).lastPathComponent` and uses the full path as `tooltip(...)`.
- Action buttons: shadcn variants.
  - `Open in sandbox`: primary (filled black `Color(.labelColor)` bg, white fg). Visible only when `sandboxStore.canOpen` (sandbox enabled and at least `tartInstalled && baseImagePresent`). Calls `sandboxStore.create(filePath: job.quarantinePath, sourceJobId: job.id, network: false)`.
  - `Restore`: default (outline). Visible only for `quarantine_kept` jobs. Calls `store.restoreJob(job.id)`.
  - `Delete`: danger (red text on white outline). Visible only for `quarantine_kept`. Calls `store.deleteJob(job.id)` after a small confirm.

Empty states (per group):

- "Scanning" empty → group header reads `▾ Scanning 0` and shows a 10 pt secondary line `No active scans` indented in the section body.
- "Quarantine" empty → secondary line `Nothing quarantined`.
- "Restored" empty → group is auto-collapsed and not shown when count is zero.

Pre-empty placeholder (no jobs at all): replace the entire jobs body with a centred vertical block `📥 + "Watching ~/Downloads/steep" (12 pt secondary)` and a small `Drop files here or run a test scan` hint. The watch-path string comes from `settingsStore.watchPath`.

### Sandbox tab content

Top strip:

```
[+ New session]                                Network [○--]
```

- `+ New session` — outline button. Click opens an `NSOpenPanel` then `sandboxStore.create(filePath: url.path, sourceJobId: nil, network: networkSwitchValue)`.
- `Network` — global switch (toggle-style: 30×16, green on / grey off, 12 pt knob). State source: `@State private var newSessionNetwork: Bool = settingsStore.sandboxNetworkDefault` so the toggle initialises from the user's saved default. Toggling does NOT touch the daemon — it only sets the local intent for the next "+ New" click.
- Label is `Network` exactly. No "for new sessions" suffix.

Row (per `SandboxSession`):

```
📦  setup-1.4.dmg                                [⊕] [⤴] [⊘]   [running]
   fsbx-a8c1 · 5m active · network off
```

- Row is a single `HStack` with `alignment: .center` so labels, action buttons, and state pill all centre vertically against the row's full height.
- Padding `10×14`. Inner gap `10`.
- Left column (filename + meta) — `VStack(alignment: .leading, spacing: 4)`, `flex: 1`, takes all remaining horizontal space.
  - Top: `📦` (or document SF symbol per source extension) + filename in 12 pt 500-weight (`URL(fileURLWithPath: session.sourceFilePath).lastPathComponent`).
  - Bottom (10 pt secondary): VM tag monospace `fsbx-XXXXXXXX` chip (`Color(.controlBackgroundColor)` bg + 1 pt border + radius 4 + padding `1×5`) · age (`5m`, `1h`) · `network on` / `network off`.
- Action buttons — three borderless-with-border square buttons sit in their own `HStack(spacing: 4)`, **always visible** (no hover reveal). Each: `28×28 pt`, radius `6`, 1 pt `Color(.separatorColor)` border, `Color(.windowBackground)` fill, `Image(systemName:)` glyph at 14 pt.
  - Show window — `Image(systemName: "plus.viewfinder")` (placeholder glyph in mockup: `⊕`)
  - Export — `Image(systemName: "square.and.arrow.up")` (placeholder glyph in mockup: `⤴`)
  - Discard — `Image(systemName: "xmark.circle")` (placeholder glyph in mockup: `⊘`), red foreground (`Color(red: 0.64, green: 0.15, blue: 0.15)`), red hover bg `#fdecec`, red border `#f3caca`.
  - Production note: every action glyph is an `Image(systemName:)` SF Symbol — never a text codepoint. The mockups use Unicode glyphs only because the brainstorm companion is HTML.
- State pill — sits at the far right edge, after the action buttons. Variants: `running` (green), `starting` (blue), `stopped`/`failed` (red), `discarded` (grey). Hidden if status not in those.

Empty state: centred `🛡 + "No sandbox sessions" (12 pt secondary)` + caption `Click + New session to spawn a VM`. Hidden entirely when `!sandboxEnabled` (then a single `Sandbox is disabled in Settings` line replaces the body).

### Settings tab content

Dense list, all groups visible, no collapse. Auto-save on change (no save button). Mirrors the existing `SettingsStore` fields.

Groups and rows:

1. **Watcher**
   - `Mode` — segmented control bound to `store.mode` (Active / Paused / Off). Active pill uses the same chip green for visual consistency with the header.
   - `Watch path` — `TextField` bound to `settingsStore.watchPath`. Monospace 11 pt. Loses focus → POST to `/api/config`. Restart-required hint shown when value differs from the daemon's actual watchPath.
   - `Quarantine path` — `TextField` bound to `settingsStore.quarantinePath`. Same behaviour.

2. **Scanners**
   - `Local (pompelmi)` — switch bound to `settingsStore.pompelmiEnabled`.
   - `VirusTotal` — switch bound to `settingsStore.vtEnabled`.
   - When pompelmi enabled → an indented sub-row `clamd socket` with a `TextField` bound to `settingsStore.pompelmiSocketPath` and a sub-row `On scan error` with a 2-segment picker (`Bypass to VT` / `Mark inconclusive`) bound to `settingsStore.pompelmiFailureMode`.
   - When both off → a 10 pt red caption row `No active scanners — every new file will be quarantined as inconclusive.`

3. **Sandbox**
   - `Enable` — switch bound to `settingsStore.sandboxEnabled`.
   - When enabled → indented rows: `Base VM name` (`TextField`), `Idle timeout (min)` (Stepper, range 5...10080, step 5), `Network ON by default` (switch), `Output retention (days)` (Stepper, range 0...90, step 1).

4. **Advanced**
   - `Max scan size (MiB)` — Stepper / TextField bound to `settingsStore.maxScanMegabytes`.
   - `Max concurrent VT scans` — Stepper bound to `settingsStore.maxConcurrentScans`.
   - `Use separate VT process` — switch bound to `settingsStore.useSeparateVtProcess`.
   - `Inconclusive retention (days)` — Stepper bound to `settingsStore.inconclusiveRetentionDays`.
   - `API token` — secure `TextField` bound to `settingsStore.apiAuthToken`.
   - `VT API key` — secure `TextField` bound to `settingsStore.vtApiKey`.

Group header: 10 pt uppercase 600 weight secondary fg, `Color(.controlBackgroundColor)` bg, padding `7×14`. Always visible (not collapsible).

Row layout: label flex on left (12 pt regular, `.label` colour); control flush right. Vertical rhythm `9×14`. Field control widths: switch 30 pt, stepper auto, text input 120-180 pt.

Auto-save: each control's binding writes to `settingsStore` immediately, debounced 400 ms via a small wrapper (existing `save()` is already idempotent). Optimistic UI; if the save fails the affected field gets a 1 pt red border for 3 s.

### Footer (always visible)

```
Restart daemon · View logs                                [Quit]
```

- 7 pt vertical padding, `Color(.windowBackground)` lighter shade.
- `Restart daemon` and `View logs` rendered as link-style buttons (10 pt secondary, underline on hover). Behaviour:
  - `Restart daemon` → POST `/api/restart` if available; otherwise `store.stopDaemon()` followed by a `store.startDaemon()` if start is available; otherwise it shows an alert with the manual command.
  - `View logs` → opens the daemon's log file path (`~/Library/Logs/FileSandbox/daemon.log`) via `NSWorkspace.shared.open(...)`. If the file does not exist, opens the parent folder.
- `Quit` → red text button, `NSApp.terminate(nil)`.

### Removed components

- The standalone `Settings { SettingsView(...) }` Scene in `App.swift`. The `Settings` tab inside the dropdown is the sole settings surface.
- The "Refresh" header button — replaced by `.onAppear { store.fetch() }` on each tab's content view, plus the `Refresh` action inside the `⋯` overflow menu.
- The "Trash" header button (clear logs) — moved into the `⋯` overflow menu under `Clear settled jobs`.
- The bottom-right standalone `Quit` button without a footer bar — replaced by the new footer with link-style ops + Quit.

## File structure

| File | Responsibility | Status |
|---|---|---|
| `Sources/App/Theme.swift` | Centralise the redesigned colour tokens (chip tints, verdict colours, monospace tag style). One source of truth so Jobs/Sandbox/Settings views agree. | Create |
| `Sources/App/Components/StatusChip.swift` | The header status chip (clickable mode menu). | Create |
| `Sources/App/Components/Tabs.swift` | The pill segmented tab bar. | Create |
| `Sources/App/Components/MetaPill.swift` | Reusable white-bg/border-1pt pill with leading SF Symbol. Used by job expand and sandbox row. | Create |
| `Sources/App/Components/EngineCard.swift` | Small engine card (dot + label + value). Used in job expand. | Create |
| `Sources/App/Components/VerdictPill.swift` | The mini-pill (small lozenge) and the big verdict pill. | Create |
| `Sources/App/Components/Switch.swift` | The 30×16 toggle switch styled to match the mockups. | Create |
| `Sources/App/Components/SettingRow.swift` | Single label + control row used in Settings. | Create |
| `Sources/App/Views.swift` | Reorganised: keep `MenuBarContentView` shell only. Move existing `JobRowView`, sandbox row, settings rows into separate files. | Modify |
| `Sources/App/Tabs/JobsTabView.swift` | Grouped, click-to-expand jobs list. Replaces today's flat list inside `MenuBarContentView`. | Create |
| `Sources/App/Tabs/SandboxTabView.swift` | New top strip + two-line rows. Replaces `SandboxView.swift`. | Create |
| `Sources/App/Tabs/SettingsTabView.swift` | Dense list with auto-save. New. | Create |
| `Sources/App/SandboxView.swift` | Removed (content moved to `SandboxTabView`). | Delete |
| `Sources/App/SettingsView.swift` | Removed (content reused via `SettingsTabView`). | Delete |
| `Sources/App/App.swift` | Drop the `Settings { ... }` Scene. Inject `sandboxStore` and `settingsStore` into a single `MenuBarContentView`. | Modify |
| `Sources/App/JobStore.swift` | Add `restoreJob(id)` and `deleteJob(id)` if not present already (likely yes). Tab-count derived properties. | Modify (small) |
| `Sources/App/SandboxStore.swift` | Add `canOpen` computed property (`enabled && tartInstalled && baseImagePresent`). | Modify (small) |
| `Sources/App/Footer.swift` | The footer with link-style ops + Quit. | Create |
| `Sources/App/Header.swift` | The header with chip + ⋯ overflow. | Create |

This decomposition aims to keep each file focused and small. `Views.swift` had grown unwieldy (all views in one file); splitting by tab and component matches the redesign's structure.

## Behavioural details

- **Refreshing data:** each tab's body view runs `store.fetch()` and `sandboxStore.fetch()` on `.onAppear`. The `Refresh` action in `⋯` triggers both. There is no periodic poll added — the existing 2-second timer in `JobStore` is unchanged.
- **Selected tab persistence:** `@AppStorage("filesandbox.selectedTab")` stores the integer tab index. Defaults to `0` (Jobs).
- **Hover state on rows:** SwiftUI `.onHover { hovering in ... }`. Render hover-only icons inside an `HStack` whose opacity is bound to the hover flag.
- **Daemon offline state:** when `store.isConnected == false`, the body of every tab is replaced by a centred message `Daemon offline. Start it from the launch agent or run yarn start.` with a small `[Start daemon]` button when `store.daemonProjectPath` is set. Header chip becomes red and reads `Disconnected`. Footer hides `Restart daemon` (no daemon to restart).
- **Mode chip click:** opens an `NSMenu` (or SwiftUI `Menu`) anchored on the chip. Items are radio-style with a checkmark on the current mode.

## Theme tokens (concrete values)

```swift
enum Theme {
    static let chipFontSize: CGFloat = 10
    static let smallFontSize: CGFloat = 11
    static let bodyFontSize: CGFloat = 12

    static let cornerRadiusPanel: CGFloat = 12
    static let cornerRadiusChip: CGFloat = 6
    static let cornerRadiusPill: CGFloat = 8
    static let cornerRadiusButton: CGFloat = 7

    static let separator = Color(nsColor: .separatorColor)
    static let panelBg = Color(nsColor: .windowBackgroundColor)
    static let subtleBg = Color(nsColor: .controlBackgroundColor)

    static let verdictRedBg    = Color(red: 0.99, green: 0.91, blue: 0.91)
    static let verdictRedFg    = Color(red: 0.64, green: 0.15, blue: 0.15)
    static let verdictOrangeBg = Color(red: 1.00, green: 0.95, blue: 0.88)
    static let verdictOrangeFg = Color(red: 0.65, green: 0.35, blue: 0.00)
    static let verdictGreenBg  = Color(red: 0.90, green: 0.97, blue: 0.92)
    static let verdictGreenFg  = Color(red: 0.11, green: 0.49, blue: 0.23)
    static let verdictBlueBg   = Color(red: 0.93, green: 0.95, blue: 0.98)
    static let verdictBlueFg   = Color(red: 0.20, green: 0.27, blue: 0.33)
}
```

## Migration

- The redesign is purely client-side; no daemon changes.
- Two persisted settings keys are introduced: `filesandbox.selectedTab` and any per-group collapse flags (`filesandbox.jobs.collapsed.scanning`, etc).
- Existing `Settings { ... }` scene removal causes the user's `Cmd+,` to do nothing (or surfaces the system "no settings" beep). Acceptable — the menu bar always shows the Settings tab on demand. Document in the README.

## Failure modes

| Situation | Behavior |
|---|---|
| Daemon offline | Header chip turns red `Disconnected`. All tab bodies show "Daemon offline" with optional Start. Footer hides Restart. |
| Sandbox disabled | Sandbox tab body shows a single secondary line `Sandbox is disabled in Settings`. The `+ New session` button and switch are not rendered. |
| Tart missing while sandbox enabled | `+ New session` button is disabled with tooltip `Install Tart to enable`. Same for "Open in sandbox" buttons in Job rows. |
| Both engines disabled | Settings shows the existing red caption. Job rows in the Restored group will be empty over time. |
| Save failure | Field gets 1pt red border for 3s + a small alert in the footer area `Failed to save: <reason>`. |
| Window resize | Width is fixed; vertical content scrolls when overflow. Each tab uses its own `ScrollView`. |

## Open work

- Asset additions: any custom SF Symbol replacements should be checked against macOS 13 (project's deployment target). Specifically `arrow.up.forward.app` is fine; `xmark.circle` is fine.
- The "Restart daemon" footer link assumes a daemon-side endpoint or a known launchd label. If neither exists, the spec should be revisited to propose a thin endpoint OR fall back to "stop + ask user to run yarn start".
- This design assumes the existing project icon (`shield.checkmark.fill`) is fine in the menu bar; the dropdown does not show it.

## Acceptance checklist (manual)

- Open dropdown — Jobs tab is selected by default. Header shows chip and ⋯. Footer is link-style.
- Switch tab — Settings tab edits round-trip to the daemon (verified by `curl /api/config`).
- Click chip → mode menu opens; selecting `Paused` flips chip to orange `Scanning paused`. Tab content reflects "Daemon offline / scan paused" if applicable.
- Job row click → expands inline; verdict pill, engine cards, meta pills, and three buttons render.
- Hover sandbox row → action icons fade in; click `Discard` → row vanishes after API call.
- Click `Restart daemon` link → daemon restarts.
- Close-and-reopen dropdown → last selected tab is restored.
- All visible strings use ASCII hyphens (no em dashes).
