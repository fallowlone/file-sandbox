# Menu bar UI redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the menu-bar dropdown's flat header + jobs list + sandbox section + footer with a tabbed shadcn-style layout (Jobs / Sandbox / Settings) inside a 420 pt frame, driven by a shared `Theme` and a small set of reusable view components.

**Architecture:** Add a `Components/` folder of small, focused SwiftUI views (StatusChip, Tabs, MetaPill, VerdictPill, EngineCard, Switch, SettingRow). Add per-tab views in `Tabs/` (JobsTabView, SandboxTabView, SettingsTabView). Re-shell `MenuBarContentView` (in `Views.swift`) to compose `Header` + tab-switched body + `Footer`. Delete `SandboxView.swift` and `SettingsView.swift` and the standalone `Settings { ... }` Scene. Stores (`JobStore`, `SettingsStore`, `SandboxStore`) keep their public APIs; we add only `SandboxStore.canOpen` and tab-count derived properties on `JobStore`.

**Tech Stack:** SwiftUI (macOS 14+), AppKit (NSOpenPanel / NSWorkspace / NSMenu), SwiftPM. No new dependencies.

---

## Pre-flight

- The codebase has **no XCTest target**. Verification per task is `swift build` clean compile + commit. A final manual acceptance pass (spec § Acceptance checklist) is run after the last task — this is captured in Task 17.
- Run all `swift build` commands from `macos-menubar/` (the SwiftPM root). Expected baseline output before any task: `Build complete!`.
- Working tree at start of plan execution should be on a feature branch off `main`. Create it before Task 1 if not present:

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git switch -c feat/menubar-ui-redesign
git add docs/superpowers/specs/2026-05-04-menubar-ui-redesign-design.md
git commit -m "docs: finalize menubar UI redesign spec (sandbox row + SF Symbol rules)"
```

---

## File structure (recap)

| File | Status |
|---|---|
| `macos-menubar/Sources/App/Theme.swift` | Create (Task 1) |
| `macos-menubar/Sources/App/SandboxStore.swift` | Modify (Task 2) |
| `macos-menubar/Sources/App/JobStore.swift` | Modify (Task 2) |
| `macos-menubar/Sources/App/Components/Switch.swift` | Create (Task 3) |
| `macos-menubar/Sources/App/Components/VerdictPill.swift` | Create (Task 4) |
| `macos-menubar/Sources/App/Components/MetaPill.swift` | Create (Task 5) |
| `macos-menubar/Sources/App/Components/EngineCard.swift` | Create (Task 6) |
| `macos-menubar/Sources/App/Components/StatusChip.swift` | Create (Task 7) |
| `macos-menubar/Sources/App/Components/Tabs.swift` | Create (Task 8) |
| `macos-menubar/Sources/App/Components/SettingRow.swift` | Create (Task 9) |
| `macos-menubar/Sources/App/Header.swift` | Create (Task 10) |
| `macos-menubar/Sources/App/Footer.swift` | Create (Task 11) |
| `macos-menubar/Sources/App/Tabs/JobsTabView.swift` | Create (Task 12) |
| `macos-menubar/Sources/App/Tabs/SandboxTabView.swift` | Create (Task 13) |
| `macos-menubar/Sources/App/Tabs/SettingsTabView.swift` | Create (Task 14) |
| `macos-menubar/Sources/App/Views.swift` | Modify (Task 15) |
| `macos-menubar/Sources/App/App.swift` | Modify (Task 15) |
| `macos-menubar/Sources/App/SandboxView.swift` | Delete (Task 16) |
| `macos-menubar/Sources/App/SettingsView.swift` | Delete (Task 16) |

---

## Task 1: Theme tokens

**Files:**

- Create: `macos-menubar/Sources/App/Theme.swift`

- [ ] **Step 1: Write `Theme.swift`**

```swift
import SwiftUI

/// Centralised design tokens for the menu-bar redesign.
/// Values come from the design spec (docs/superpowers/specs/2026-05-04-menubar-ui-redesign-design.md, § Theme tokens).
enum Theme {
    // Type sizes
    static let chipFontSize: CGFloat = 10
    static let smallFontSize: CGFloat = 11
    static let bodyFontSize: CGFloat = 12

    // Radii
    static let cornerRadiusPanel: CGFloat = 12
    static let cornerRadiusChip: CGFloat = 6
    static let cornerRadiusPill: CGFloat = 8
    static let cornerRadiusButton: CGFloat = 7

    // Surfaces / borders
    static let separator = Color(nsColor: .separatorColor)
    static let panelBg = Color(nsColor: .windowBackgroundColor)
    static let subtleBg = Color(nsColor: .controlBackgroundColor)

    // Verdict / status tints
    static let verdictRedBg    = Color(red: 0.99, green: 0.91, blue: 0.91)
    static let verdictRedFg    = Color(red: 0.64, green: 0.15, blue: 0.15)
    static let verdictOrangeBg = Color(red: 1.00, green: 0.95, blue: 0.88)
    static let verdictOrangeFg = Color(red: 0.65, green: 0.35, blue: 0.00)
    static let verdictGreenBg  = Color(red: 0.90, green: 0.97, blue: 0.92)
    static let verdictGreenFg  = Color(red: 0.11, green: 0.49, blue: 0.23)
    static let verdictBlueBg   = Color(red: 0.93, green: 0.95, blue: 0.98)
    static let verdictBlueFg   = Color(red: 0.20, green: 0.27, blue: 0.33)
    static let verdictGreyBg   = Color(red: 0.94, green: 0.94, blue: 0.95)
    static let verdictGreyFg   = Color(red: 0.40, green: 0.40, blue: 0.42)

    // Discard button hover (sandbox row)
    static let discardHoverBg     = Color(red: 0.99, green: 0.92, blue: 0.92)
    static let discardHoverBorder = Color(red: 0.95, green: 0.79, blue: 0.79)
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Theme.swift
git commit -m "feat(menubar): add Theme tokens (verdict tints, radii, type sizes)"
```

---

## Task 2: Store helpers (`canOpen` + tab counts)

**Files:**

- Modify: `macos-menubar/Sources/App/SandboxStore.swift`
- Modify: `macos-menubar/Sources/App/JobStore.swift`

- [ ] **Step 1: Add `canOpen` to `SandboxStore`**

Open `macos-menubar/Sources/App/SandboxStore.swift`. Inside `class SandboxStore`, after the `@Published var loadError: String? = nil` line and before `private let port: String`, add the published flags + computed property:

```swift
    @Published var sandboxEnabled: Bool = false
    @Published var tartInstalled: Bool = true
    @Published var baseImagePresent: Bool = true

    /// True only if every prerequisite for spawning a session is in place.
    /// Used by the Jobs tab "Open in sandbox" button and the Sandbox tab "+ New session" button.
    var canOpen: Bool {
        sandboxEnabled && tartInstalled && baseImagePresent
    }

    /// Number of running/starting sessions, for the Sandbox tab count chip.
    var activeCount: Int {
        sessions.filter { $0.status == "running" || $0.status == "starting" }.count
    }
```

- [ ] **Step 2: Add tab-count helpers to `JobStore`**

Open `macos-menubar/Sources/App/JobStore.swift`. After the existing `var threatCount: Int { activeThreats.count }` line (around line 185), insert:

```swift
    /// Count for the Jobs tab pill (scanning + quarantined; restored hidden).
    var visibleJobCount: Int {
        jobs.filter { ["scanning", "received", "in_quarantine", "quarantine_kept"].contains($0.status) }.count
    }

    /// Quick lookups used by the grouped jobs view.
    var scanningJobs: [SandboxJob] {
        jobs.filter { $0.status == "scanning" || $0.status == "received" || $0.status == "in_quarantine" }
    }
    var quarantinedJobs: [SandboxJob] {
        jobs.filter { $0.status == "quarantine_kept" }
    }
    var restoredJobs: [SandboxJob] {
        jobs.filter { $0.status == "restored" }
    }
```

- [ ] **Step 3: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/SandboxStore.swift macos-menubar/Sources/App/JobStore.swift
git commit -m "feat(menubar): add canOpen + tab-count helpers to stores"
```

> **Note:** `tartInstalled` and `baseImagePresent` start `true`. A future task can wire these to a daemon probe; for the redesign they default to "OK" so buttons render. The spec acceptance does not require live probing.

---

## Task 3: Switch component

**Files:**

- Create: `macos-menubar/Sources/App/Components/Switch.swift`

- [ ] **Step 1: Write `Switch.swift`**

```swift
import SwiftUI

/// 30×16 pill toggle styled to match the redesign mockups.
/// Drop-in replacement for `Toggle("", isOn:)` where the visual must match the design spec.
struct AppSwitch: View {
    @Binding var isOn: Bool
    var body: some View {
        ZStack(alignment: isOn ? .trailing : .leading) {
            RoundedRectangle(cornerRadius: 8)
                .fill(isOn ? Theme.verdictGreenFg : Color.gray.opacity(0.4))
                .frame(width: 30, height: 16)
            Circle()
                .fill(.white)
                .frame(width: 12, height: 12)
                .shadow(color: .black.opacity(0.2), radius: 0.5, x: 0, y: 1)
                .padding(.horizontal, 2)
        }
        .animation(.easeInOut(duration: 0.15), value: isOn)
        .contentShape(Rectangle())
        .onTapGesture { isOn.toggle() }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityValue(isOn ? "on" : "off")
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/Switch.swift
git commit -m "feat(menubar): AppSwitch component (30x16 pill toggle)"
```

---

## Task 4: VerdictPill component

**Files:**

- Create: `macos-menubar/Sources/App/Components/VerdictPill.swift`

- [ ] **Step 1: Write `VerdictPill.swift`**

```swift
import SwiftUI

/// Coloured lozenge used for verdict / state labels.
/// `.mini` is the row-collapsed variant. `.big` is the expanded-row header variant.
struct VerdictPill: View {
    enum Size { case mini, big }
    enum Variant { case red, orange, green, blue, grey }

    let text: String
    let variant: Variant
    let size: Size
    var symbol: String? = nil

    var body: some View {
        let colors = tints(for: variant)
        HStack(spacing: 4) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: size == .mini ? 8 : 10, weight: .semibold))
            }
            Text(text)
                .font(.system(size: size == .mini ? 9 : 11, weight: .semibold))
        }
        .padding(.horizontal, size == .mini ? 8 : 10)
        .padding(.vertical, size == .mini ? 1 : 3)
        .foregroundColor(colors.fg)
        .background(colors.bg)
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusPill))
    }

    private func tints(for v: Variant) -> (bg: Color, fg: Color) {
        switch v {
        case .red:    return (Theme.verdictRedBg, Theme.verdictRedFg)
        case .orange: return (Theme.verdictOrangeBg, Theme.verdictOrangeFg)
        case .green:  return (Theme.verdictGreenBg, Theme.verdictGreenFg)
        case .blue:   return (Theme.verdictBlueBg, Theme.verdictBlueFg)
        case .grey:   return (Theme.verdictGreyBg, Theme.verdictGreyFg)
        }
    }
}

extension VerdictPill {
    /// Map a job's `vt_verdict` string + status to a pill variant + label.
    static func forJobVerdict(verdict: String?, status: String) -> VerdictPill? {
        if status == "scanning" || status == "received" {
            return VerdictPill(text: "scanning", variant: .blue, size: .mini, symbol: "hourglass")
        }
        if status == "in_quarantine" {
            return VerdictPill(text: "queued", variant: .blue, size: .mini, symbol: "tray")
        }
        guard let v = verdict?.lowercased() else { return nil }
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: "infected", variant: .red, size: .mini, symbol: "exclamationmark.triangle.fill")
        case "inconclusive", "unclear":
            return VerdictPill(text: "inconclusive", variant: .orange, size: .mini, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: "oversized", variant: .grey, size: .mini, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: "clean", variant: .green, size: .mini, symbol: "checkmark.circle.fill")
        default:
            return VerdictPill(text: v, variant: .grey, size: .mini)
        }
    }
}

/// Sandbox session state pill (running / starting / stopped / failed / discarded).
/// Same look as VerdictPill mini; separate factory for clarity.
struct SessionStatePill: View {
    let status: String
    var body: some View {
        switch status {
        case "running":   VerdictPill(text: "running",   variant: .green, size: .mini)
        case "starting":  VerdictPill(text: "starting",  variant: .blue,  size: .mini)
        case "stopped":   VerdictPill(text: "stopped",   variant: .red,   size: .mini)
        case "failed":    VerdictPill(text: "failed",    variant: .red,   size: .mini)
        case "discarded": VerdictPill(text: "discarded", variant: .grey,  size: .mini)
        default:          EmptyView()
        }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/VerdictPill.swift
git commit -m "feat(menubar): VerdictPill + SessionStatePill components"
```

---

## Task 5: MetaPill component

**Files:**

- Create: `macos-menubar/Sources/App/Components/MetaPill.swift`

- [ ] **Step 1: Write `MetaPill.swift`**

```swift
import SwiftUI

/// Reusable white-bg / 1-pt-border pill with a leading SF Symbol and a label.
/// Used by the expanded job row meta strip (filename / age / size).
/// Optional `tooltip` shows a help-tag on hover (e.g. full file path).
struct MetaPill: View {
    let symbol: String
    let text: String
    var tooltip: String? = nil

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .regular))
                .foregroundColor(.secondary)
            Text(text)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(Theme.panelBg)
        .overlay(
            RoundedRectangle(cornerRadius: Theme.cornerRadiusChip)
                .strokeBorder(Theme.separator, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusChip))
        .help(tooltip ?? "")
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/MetaPill.swift
git commit -m "feat(menubar): MetaPill component"
```

---

## Task 6: EngineCard component

**Files:**

- Create: `macos-menubar/Sources/App/Components/EngineCard.swift`

- [ ] **Step 1: Write `EngineCard.swift`**

```swift
import SwiftUI

/// Small engine result card used inside an expanded job row.
/// Layout: dot (status) + label (engine name) + value (verdict / count).
struct EngineCard: View {
    enum Status { case clean, malicious, warn, neutral }
    let label: String
    let value: String
    let status: Status

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(dotColor)
                .frame(width: 6, height: 6)
            Text(label)
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(.primary)
            Text(value)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 4)
        .background(Theme.panelBg)
        .overlay(
            RoundedRectangle(cornerRadius: Theme.cornerRadiusPill)
                .strokeBorder(Theme.separator, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusPill))
    }

    private var dotColor: Color {
        switch status {
        case .clean:     return Theme.verdictGreenFg
        case .malicious: return Theme.verdictRedFg
        case .warn:      return Theme.verdictOrangeFg
        case .neutral:   return Theme.verdictGreyFg
        }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/EngineCard.swift
git commit -m "feat(menubar): EngineCard component"
```

---

## Task 7: StatusChip component (header mode chip)

**Files:**

- Create: `macos-menubar/Sources/App/Components/StatusChip.swift`

- [ ] **Step 1: Write `StatusChip.swift`**

```swift
import SwiftUI

/// Header chip: shows current WatcherMode; click opens a Menu for switching.
/// Daemon-offline state is rendered as `disconnected` (red) and disabled.
struct StatusChip: View {
    let mode: WatcherMode
    let isConnected: Bool
    let onSelect: (WatcherMode) -> Void

    private var tints: (bg: Color, fg: Color) {
        guard isConnected else { return (Theme.verdictRedBg, Theme.verdictRedFg) }
        switch mode {
        case .active:              return (Theme.verdictGreenBg,  Theme.verdictGreenFg)
        case .scanPaused:          return (Theme.verdictOrangeBg, Theme.verdictOrangeFg)
        case .monitoringDisabled:  return (Theme.verdictRedBg,    Theme.verdictRedFg)
        }
    }

    private var label: String {
        isConnected ? mode.displayName : "Disconnected"
    }

    var body: some View {
        Menu {
            ForEach(WatcherMode.allCases, id: \.self) { m in
                Button {
                    onSelect(m)
                } label: {
                    Label {
                        Text(m.displayName)
                    } icon: {
                        if mode == m { Image(systemName: "checkmark") }
                    }
                }
            }
        } label: {
            HStack(spacing: 4) {
                Text(label)
                    .font(.system(size: 10, weight: .semibold))
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 2)
            .background(tints.bg)
            .foregroundColor(tints.fg)
            .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusChip))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(!isConnected)
        .help("Watcher mode")
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/StatusChip.swift
git commit -m "feat(menubar): StatusChip component (header mode chip with Menu)"
```

---

## Task 8: Tabs component (pill segmented)

**Files:**

- Create: `macos-menubar/Sources/App/Components/Tabs.swift`

- [ ] **Step 1: Write `Tabs.swift`**

```swift
import SwiftUI

enum AppTab: Int, CaseIterable, Identifiable {
    case jobs = 0
    case sandbox = 1
    case settings = 2

    var id: Int { rawValue }

    var title: String {
        switch self {
        case .jobs:     return "Jobs"
        case .sandbox:  return "Sandbox"
        case .settings: return "Settings"
        }
    }
}

/// Pill segmented control. Active = white bg + 1pt soft shadow + 600 weight; inactive = transparent + secondary fg.
struct AppTabs: View {
    @Binding var selection: Int
    /// Optional count chips per tab (nil = no chip).
    var counts: [AppTab: Int] = [:]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(AppTab.allCases) { tab in
                let isOn = selection == tab.rawValue
                Button {
                    selection = tab.rawValue
                } label: {
                    HStack(spacing: 4) {
                        Text(tab.title)
                            .font(.system(size: 11, weight: isOn ? .semibold : .regular))
                            .foregroundColor(isOn ? .primary : .secondary)
                        if let c = counts[tab], c > 0 {
                            Text("\(c)")
                                .font(.system(size: 10, weight: .regular))
                                .foregroundColor(Color.secondary.opacity(0.7))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
                    .background(
                        Group {
                            if isOn {
                                RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                                    .fill(Theme.panelBg)
                                    .shadow(color: .black.opacity(0.07), radius: 1.5, x: 0, y: 1)
                            } else {
                                Color.clear
                            }
                        }
                    )
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Theme.subtleBg)
        .overlay(Divider(), alignment: .bottom)
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/Tabs.swift
git commit -m "feat(menubar): AppTabs pill segmented tab bar"
```

---

## Task 9: SettingRow component

**Files:**

- Create: `macos-menubar/Sources/App/Components/SettingRow.swift`

- [ ] **Step 1: Write `SettingRow.swift`**

```swift
import SwiftUI

/// Single label + control row for the Settings tab.
/// Vertical rhythm 9 pt, horizontal 14 pt. Label flexes; control is flush right.
struct SettingRow<Control: View>: View {
    let label: String
    var indent: CGFloat = 0
    @ViewBuilder let control: () -> Control

    var body: some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.system(size: 12))
                .foregroundColor(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
            control()
        }
        .padding(.leading, 14 + indent)
        .padding(.trailing, 14)
        .padding(.vertical, 9)
        .overlay(Divider(), alignment: .bottom)
    }
}

/// Group header for the Settings tab. Always visible (not collapsible per spec).
struct SettingGroupHeader: View {
    let title: String
    var body: some View {
        Text(title.uppercased())
            .font(.system(size: 10, weight: .semibold))
            .tracking(0.5)
            .foregroundColor(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .background(Theme.subtleBg)
            .overlay(Divider(), alignment: .bottom)
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Components/SettingRow.swift
git commit -m "feat(menubar): SettingRow + SettingGroupHeader components"
```

---

## Task 10: Header view

**Files:**

- Create: `macos-menubar/Sources/App/Header.swift`

- [ ] **Step 1: Write `Header.swift`**

```swift
import SwiftUI
import AppKit

struct AppHeader: View {
    @ObservedObject var store: JobStore

    var body: some View {
        HStack(spacing: 8) {
            Text("FileSandbox")
                .font(.system(size: 13, weight: .semibold))

            Spacer()

            StatusChip(
                mode: store.mode,
                isConnected: store.isConnected,
                onSelect: { store.setMode($0) }
            )

            Menu {
                Button("Refresh") { store.fetch() }
                if store.isConnected {
                    Button("Restart daemon") { restartDaemon() }
                    Button("View logs") { openLogs() }
                }
                Divider()
                if !store.jobs.isEmpty {
                    Button("Clear settled jobs") { store.clearJobs() }
                }
                Button("Quit FileSandbox") { NSApp.terminate(nil) }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundColor(.secondary)
                    .frame(width: 22, height: 22)
                    .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .overlay(Divider(), alignment: .bottom)
    }

    private func restartDaemon() {
        store.stopDaemon()
    }

    private func openLogs() {
        let url = URL(fileURLWithPath: NSString("~/Library/Logs/FileSandbox/daemon.log").expandingTildeInPath)
        if FileManager.default.fileExists(atPath: url.path) {
            NSWorkspace.shared.open(url)
        } else {
            NSWorkspace.shared.open(url.deletingLastPathComponent())
        }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Header.swift
git commit -m "feat(menubar): AppHeader view (title + StatusChip + overflow menu)"
```

---

## Task 11: Footer view

**Files:**

- Create: `macos-menubar/Sources/App/Footer.swift`

- [ ] **Step 1: Write `Footer.swift`**

```swift
import SwiftUI
import AppKit

struct AppFooter: View {
    @ObservedObject var store: JobStore

    var body: some View {
        VStack(spacing: 4) {
            if let error = store.lastActionError {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14)
                    .transition(.opacity)
            }
            HStack(spacing: 0) {
                if store.isConnected {
                    LinkText(text: "Restart daemon") { store.stopDaemon() }
                    Text(" · ")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    LinkText(text: "View logs") { openLogs() }
                }
                Spacer()
                Button("Quit") {
                    NSApp.terminate(nil)
                }
                .buttonStyle(.plain)
                .font(.system(size: 11))
                .foregroundColor(Theme.verdictRedFg)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
        }
        .background(Theme.subtleBg)
        .overlay(Divider(), alignment: .top)
    }

    private func openLogs() {
        let url = URL(fileURLWithPath: NSString("~/Library/Logs/FileSandbox/daemon.log").expandingTildeInPath)
        if FileManager.default.fileExists(atPath: url.path) {
            NSWorkspace.shared.open(url)
        } else {
            NSWorkspace.shared.open(url.deletingLastPathComponent())
        }
    }
}

private struct LinkText: View {
    let text: String
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Text(text)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .underline(hovered, color: .secondary)
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Footer.swift
git commit -m "feat(menubar): AppFooter view (link-style ops + red Quit)"
```

---

## Task 12: Jobs tab view

**Files:**

- Create: `macos-menubar/Sources/App/Tabs/JobsTabView.swift`

- [ ] **Step 1: Create `Tabs/` folder**

```bash
mkdir -p /Users/artemmac/dev/personal/file-sandbox/macos-menubar/Sources/App/Tabs
```

- [ ] **Step 2: Write `JobsTabView.swift`**

```swift
import SwiftUI
import AppKit

struct JobsTabView: View {
    @ObservedObject var store: JobStore
    @ObservedObject var sandboxStore: SandboxStore
    @ObservedObject var settingsStore: SettingsStore

    @AppStorage("filesandbox.jobs.collapsed.scanning")  private var scanningCollapsed = false
    @AppStorage("filesandbox.jobs.collapsed.quarantine") private var quarantineCollapsed = false
    @AppStorage("filesandbox.jobs.collapsed.restored")   private var restoredCollapsed = true

    @State private var expandedJobId: String? = nil

    var body: some View {
        if store.jobs.isEmpty && store.isConnected {
            emptyPlaceholder
        } else if !store.isConnected {
            offlineMessage
        } else {
            ScrollView {
                LazyVStack(spacing: 0) {
                    JobsGroup(
                        title: "Scanning",
                        jobs: store.scanningJobs,
                        emptyMessage: "No active scans",
                        collapsed: $scanningCollapsed,
                        expandedJobId: $expandedJobId,
                        store: store,
                        sandboxStore: sandboxStore
                    )
                    JobsGroup(
                        title: "Quarantine",
                        jobs: store.quarantinedJobs,
                        emptyMessage: "Nothing quarantined",
                        collapsed: $quarantineCollapsed,
                        expandedJobId: $expandedJobId,
                        store: store,
                        sandboxStore: sandboxStore
                    )
                    if !store.restoredJobs.isEmpty {
                        JobsGroup(
                            title: "Restored",
                            jobs: store.restoredJobs,
                            emptyMessage: "",
                            collapsed: $restoredCollapsed,
                            expandedJobId: $expandedJobId,
                            store: store,
                            sandboxStore: sandboxStore
                        )
                    }
                }
            }
            .frame(maxHeight: 520)
        }
    }

    private var emptyPlaceholder: some View {
        VStack(spacing: 6) {
            Image(systemName: "tray.and.arrow.down")
                .font(.system(size: 26))
                .foregroundColor(.secondary)
            Text("Watching \(settingsStore.watchPath.isEmpty ? "—" : settingsStore.watchPath)")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Text("Drop files here or run a test scan")
                .font(.system(size: 10))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }

    private var offlineMessage: some View {
        VStack(spacing: 8) {
            Image(systemName: "wifi.exclamationmark")
                .font(.system(size: 22))
                .foregroundColor(.secondary)
            Text("Daemon offline")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            if !settingsStore.daemonProjectPath.isEmpty {
                Button("Start daemon") {
                    store.startDaemon(
                        projectPath: settingsStore.daemonProjectPath,
                        nodeBin: settingsStore.daemonNodeBin
                    )
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }
}

private struct JobsGroup: View {
    let title: String
    let jobs: [SandboxJob]
    let emptyMessage: String
    @Binding var collapsed: Bool
    @Binding var expandedJobId: String?
    @ObservedObject var store: JobStore
    @ObservedObject var sandboxStore: SandboxStore

    var body: some View {
        Button {
            collapsed.toggle()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: collapsed ? "chevron.right" : "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundColor(.secondary)
                Text(title.uppercased())
                    .font(.system(size: 10, weight: .semibold))
                    .tracking(0.5)
                    .foregroundColor(.secondary)
                Text("\(jobs.count)")
                    .font(.system(size: 10, weight: .regular))
                    .foregroundColor(Color.secondary.opacity(0.7))
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .background(Theme.subtleBg)
        }
        .buttonStyle(.plain)
        .overlay(Divider(), alignment: .bottom)

        if !collapsed {
            if jobs.isEmpty {
                if !emptyMessage.isEmpty {
                    Text(emptyMessage)
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                        .padding(.leading, 42)
                        .padding(.vertical, 9)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .overlay(Divider(), alignment: .bottom)
                }
            } else {
                ForEach(jobs) { job in
                    JobRow(
                        job: job,
                        expandedJobId: $expandedJobId,
                        store: store,
                        sandboxStore: sandboxStore
                    )
                }
            }
        }
    }
}

private struct JobRow: View {
    let job: SandboxJob
    @Binding var expandedJobId: String?
    @ObservedObject var store: JobStore
    @ObservedObject var sandboxStore: SandboxStore

    private var isExpanded: Bool { expandedJobId == job.id }

    var body: some View {
        VStack(spacing: 0) {
            collapsedRow
            if isExpanded {
                expandedDetail
            }
        }
        .overlay(Divider(), alignment: .bottom)
    }

    private var collapsedRow: some View {
        Button {
            expandedJobId = isExpanded ? nil : job.id
        } label: {
            HStack(spacing: 8) {
                Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundColor(.secondary)
                    .frame(width: 14)
                Image(systemName: fileSymbol(for: job.original_name))
                    .font(.system(size: 13))
                    .foregroundColor(.secondary)
                Text(job.original_name)
                    .font(.system(size: 12))
                    .foregroundColor(.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                if let pill = VerdictPill.forJobVerdict(verdict: job.vt_verdict, status: job.status) {
                    pill
                }
                Text(ageString(from: job.created_at))
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                    .frame(minWidth: 28, alignment: .trailing)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var expandedDetail: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let big = bigVerdictPill() {
                HStack(spacing: 8) {
                    big
                    if let detail = job.detail, !detail.isEmpty {
                        Text(detail)
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
            }

            HStack(spacing: 6) {
                EngineCard(
                    label: "VirusTotal",
                    value: job.vt_verdict ?? "—",
                    status: engineStatus(for: job.vt_verdict)
                )
            }

            HStack(spacing: 6) {
                MetaPill(
                    symbol: "doc",
                    text: URL(fileURLWithPath: job.final_path ?? job.original_name).lastPathComponent,
                    tooltip: job.final_path
                )
                MetaPill(symbol: "clock", text: ageString(from: job.created_at))
            }

            HStack(spacing: 6) {
                if job.status == "quarantine_kept" && sandboxStore.canOpen, let path = job.final_path {
                    Button {
                        sandboxStore.create(filePath: path, sourceJobId: job.id, network: false) { _ in }
                    } label: {
                        Label("Open in sandbox", systemImage: "shield.lefthalf.filled")
                            .font(.system(size: 11, weight: .medium))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .background(Color(nsColor: .labelColor))
                            .foregroundColor(Color(nsColor: .windowBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusButton))
                    }
                    .buttonStyle(.plain)
                }
                if job.status == "quarantine_kept" {
                    Button {
                        store.restoreFile(job.id)
                    } label: {
                        Label("Restore", systemImage: "arrow.uturn.backward")
                            .font(.system(size: 11))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .overlay(
                                RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                                    .strokeBorder(Theme.separator, lineWidth: 1)
                            )
                    }
                    .buttonStyle(.plain)

                    Button {
                        store.deleteFile(job.id)
                    } label: {
                        Label("Delete", systemImage: "trash")
                            .font(.system(size: 11))
                            .foregroundColor(Theme.verdictRedFg)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .overlay(
                                RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                                    .strokeBorder(Theme.verdictRedFg.opacity(0.4), lineWidth: 1)
                            )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .padding(.leading, 28)
        .background(Theme.subtleBg)
    }

    private func bigVerdictPill() -> VerdictPill? {
        let v = (job.vt_verdict ?? "").lowercased()
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: "Infected", variant: .red, size: .big, symbol: "exclamationmark.triangle.fill")
        case "inconclusive":
            return VerdictPill(text: "Inconclusive", variant: .orange, size: .big, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: "Oversized", variant: .grey, size: .big, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: "Clean", variant: .green, size: .big, symbol: "checkmark.circle.fill")
        default: return nil
        }
    }

    private func engineStatus(for verdict: String?) -> EngineCard.Status {
        switch (verdict ?? "").lowercased() {
        case "clean":                       return .clean
        case "infected", "malicious":       return .malicious
        case "inconclusive":                return .warn
        default:                            return .neutral
        }
    }

    private func fileSymbol(for name: String) -> String {
        let ext = (name as NSString).pathExtension.lowercased()
        switch ext {
        case "zip", "tar", "gz", "rar", "7z": return "archivebox"
        case "dmg", "iso":                    return "shippingbox"
        case "doc", "docx", "pdf", "txt":     return "doc.text"
        default:                              return "doc"
        }
    }
}

private func ageString(from epoch: Int) -> String {
    let now = Int(Date().timeIntervalSince1970)
    let delta = max(0, now - epoch)
    if delta < 60 { return "\(delta)s" }
    if delta < 3600 { return "\(delta / 60)m" }
    if delta < 86_400 { return "\(delta / 3600)h" }
    return "\(delta / 86_400)d"
}
```

- [ ] **Step 3: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/JobsTabView.swift
git commit -m "feat(menubar): JobsTabView (grouped + click-to-expand)"
```

---

## Task 13: Sandbox tab view

**Files:**

- Create: `macos-menubar/Sources/App/Tabs/SandboxTabView.swift`

- [ ] **Step 1: Write `SandboxTabView.swift`**

```swift
import SwiftUI
import AppKit

struct SandboxTabView: View {
    @ObservedObject var store: SandboxStore
    @ObservedObject var settingsStore: SettingsStore
    @State private var newSessionNetwork: Bool = false
    @State private var didInitNetwork = false

    var body: some View {
        VStack(spacing: 0) {
            topStrip
            Divider()
            if !settingsStore.sandboxEnabled {
                Text("Sandbox is disabled in Settings")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 28)
            } else if let err = store.loadError {
                Text(err)
                    .font(.caption)
                    .foregroundColor(.red)
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else if store.sessions.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(store.sessions) { s in
                            SandboxRowView(session: s) {
                                store.discard(s.id)
                            }
                        }
                    }
                }
                .frame(maxHeight: 520)
            }
        }
        .onAppear {
            store.fetch()
            if !didInitNetwork {
                newSessionNetwork = settingsStore.sandboxNetworkDefault
                didInitNetwork = true
            }
        }
    }

    private var topStrip: some View {
        HStack(spacing: 10) {
            Button {
                pickFileAndOpen()
            } label: {
                Label("New session", systemImage: "plus")
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 11)
                    .padding(.vertical, 6)
                    .overlay(
                        RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                            .strokeBorder(Theme.separator, lineWidth: 1)
                    )
            }
            .buttonStyle(.plain)
            .disabled(!store.canOpen)
            .help(store.canOpen ? "Pick a file and open it in a fresh sandbox VM" : "Install Tart to enable")

            Spacer()
            Text("Network")
                .font(.system(size: 11, weight: .medium))
            AppSwitch(isOn: $newSessionNetwork)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    private var emptyState: some View {
        VStack(spacing: 6) {
            Image(systemName: "shield")
                .font(.system(size: 22))
                .foregroundColor(.secondary)
            Text("No sandbox sessions")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            Text("Click + New session to spawn a VM")
                .font(.system(size: 10))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }

    private func pickFileAndOpen() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            store.create(filePath: url.path, sourceJobId: nil, network: newSessionNetwork) { _ in }
        }
    }
}

private struct SandboxRowView: View {
    let session: SandboxSession
    let onDiscard: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Image(systemName: fileSymbol(for: session.sourceFilePath))
                        .font(.system(size: 13))
                        .foregroundColor(.secondary)
                    Text((session.sourceFilePath as NSString).lastPathComponent)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                HStack(spacing: 6) {
                    Text(session.vmName)
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(Theme.subtleBg)
                        .overlay(
                            RoundedRectangle(cornerRadius: 4)
                                .strokeBorder(Theme.separator, lineWidth: 1)
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                    Text("·")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(ageString(from: session.lastActiveAt))
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text("·")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(session.networkEnabled ? "network on" : "network off")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: 4) {
                IconButton(symbol: "plus.viewfinder", help: "Show window") {
                    // Future: focus the VM window. Spec leaves stub OK.
                }
                IconButton(symbol: "square.and.arrow.up", help: "Export") {
                    // Future: export session output dir.
                }
                IconButton(symbol: "xmark.circle", help: "Discard", isDanger: true, action: onDiscard)
            }

            SessionStatePill(status: session.status)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .overlay(Divider(), alignment: .bottom)
    }

    private func fileSymbol(for path: String) -> String {
        let ext = (path as NSString).pathExtension.lowercased()
        switch ext {
        case "zip", "tar", "gz", "rar", "7z": return "archivebox"
        case "dmg", "iso":                    return "shippingbox"
        case "doc", "docx", "pdf", "txt":     return "doc.text"
        default:                              return "doc"
        }
    }
}

private struct IconButton: View {
    let symbol: String
    let help: String
    var isDanger: Bool = false
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .regular))
                .foregroundColor(isDanger ? Theme.verdictRedFg : .primary)
                .frame(width: 28, height: 28)
                .background(hovered && isDanger ? Theme.discardHoverBg : Theme.panelBg)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .strokeBorder(
                            hovered && isDanger ? Theme.discardHoverBorder : Theme.separator,
                            lineWidth: 1
                        )
                )
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help(help)
    }
}

private func ageString(from iso: String) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let date = formatter.date(from: iso) ?? ISO8601DateFormatter().date(from: iso)
    guard let d = date else { return iso }
    let delta = max(0, Int(Date().timeIntervalSince(d)))
    if delta < 60 { return "\(delta)s" }
    if delta < 3600 { return "\(delta / 60)m" }
    if delta < 86_400 { return "\(delta / 3600)h" }
    return "\(delta / 86_400)d"
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/SandboxTabView.swift
git commit -m "feat(menubar): SandboxTabView (top strip + two-line rows)"
```

---

## Task 14: Settings tab view

**Files:**

- Create: `macos-menubar/Sources/App/Tabs/SettingsTabView.swift`

- [ ] **Step 1: Write `SettingsTabView.swift`**

```swift
import SwiftUI
import Combine

struct SettingsTabView: View {
    @ObservedObject var settingsStore: SettingsStore
    @ObservedObject var store: JobStore

    /// Debounced auto-save: any @Published change triggers `save()` 400 ms later.
    @State private var saveTimer: AnyCancellable? = nil

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                SettingGroupHeader(title: "Watcher")
                SettingRow(label: "Mode") {
                    Picker("", selection: $store.mode) {
                        Text("Active").tag(WatcherMode.active)
                        Text("Paused").tag(WatcherMode.scanPaused)
                        Text("Off").tag(WatcherMode.monitoringDisabled)
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 180)
                    .onChange(of: store.mode) { _, m in store.setMode(m) }
                }
                SettingRow(label: "Watch path") {
                    TextField("", text: $settingsStore.watchPath, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
                SettingRow(label: "Quarantine path") {
                    TextField("", text: $settingsStore.quarantinePath, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }

                SettingGroupHeader(title: "Scanners")
                SettingRow(label: "Local (pompelmi)") {
                    AppSwitch(isOn: bind($settingsStore.pompelmiEnabled))
                }
                if settingsStore.pompelmiEnabled {
                    SettingRow(label: "clamd socket", indent: 16) {
                        TextField("", text: $settingsStore.pompelmiSocketPath, onCommit: scheduleSave)
                            .textFieldStyle(.roundedBorder)
                            .font(.system(size: 11, design: .monospaced))
                            .frame(width: 180)
                    }
                    SettingRow(label: "On scan error", indent: 16) {
                        Picker("", selection: bind($settingsStore.pompelmiFailureMode)) {
                            Text("Bypass to VT").tag("bypass")
                            Text("Mark inconclusive").tag("inconclusive")
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 180)
                    }
                }
                SettingRow(label: "VirusTotal") {
                    AppSwitch(isOn: bind($settingsStore.vtEnabled))
                }
                if !settingsStore.vtEnabled && !settingsStore.pompelmiEnabled {
                    Text("No active scanners — every new file will be quarantined as inconclusive.")
                        .font(.system(size: 10))
                        .foregroundColor(Theme.verdictRedFg)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 7)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .overlay(Divider(), alignment: .bottom)
                }

                SettingGroupHeader(title: "Sandbox")
                SettingRow(label: "Enable") {
                    AppSwitch(isOn: bind($settingsStore.sandboxEnabled))
                }
                if settingsStore.sandboxEnabled {
                    SettingRow(label: "Base VM name", indent: 16) {
                        TextField("", text: $settingsStore.sandboxBaseVm, onCommit: scheduleSave)
                            .textFieldStyle(.roundedBorder)
                            .font(.system(size: 11, design: .monospaced))
                            .frame(width: 180)
                    }
                    SettingRow(label: "Idle timeout (min)", indent: 16) {
                        Stepper(value: bind($settingsStore.sandboxIdleTimeoutMinutes), in: 5...10080, step: 5) {
                            Text("\(settingsStore.sandboxIdleTimeoutMinutes)")
                                .font(.system(size: 11, design: .monospaced))
                        }
                    }
                    SettingRow(label: "Network ON by default", indent: 16) {
                        AppSwitch(isOn: bind($settingsStore.sandboxNetworkDefault))
                    }
                    SettingRow(label: "Output retention (days)", indent: 16) {
                        Stepper(value: bind($settingsStore.sandboxOutRetentionDays), in: 0...90, step: 1) {
                            Text("\(settingsStore.sandboxOutRetentionDays)")
                                .font(.system(size: 11, design: .monospaced))
                        }
                    }
                }

                SettingGroupHeader(title: "Advanced")
                SettingRow(label: "Max scan size (MiB)") {
                    Stepper(value: bind($settingsStore.maxScanMegabytes), in: 1...10000, step: 50) {
                        Text("\(settingsStore.maxScanMegabytes)")
                            .font(.system(size: 11, design: .monospaced))
                    }
                }
                SettingRow(label: "Max concurrent VT scans") {
                    Stepper(value: bind($settingsStore.maxConcurrentScans), in: 1...8, step: 1) {
                        Text("\(settingsStore.maxConcurrentScans)")
                            .font(.system(size: 11, design: .monospaced))
                    }
                }
                SettingRow(label: "Use separate VT process") {
                    AppSwitch(isOn: bind($settingsStore.useSeparateVtProcess))
                }
                SettingRow(label: "Inconclusive retention (days)") {
                    Stepper(value: bind($settingsStore.inconclusiveRetentionDays), in: 0...90, step: 1) {
                        Text("\(settingsStore.inconclusiveRetentionDays)")
                            .font(.system(size: 11, design: .monospaced))
                    }
                }
                SettingRow(label: "API token") {
                    SecureField("", text: $settingsStore.apiAuthToken, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
                SettingRow(label: "VT API key") {
                    SecureField("", text: $settingsStore.vtApiKey, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
            }
        }
        .frame(maxHeight: 520)
        .onAppear { settingsStore.fetch() }
    }

    /// Wraps a Binding so any change schedules a debounced save.
    private func bind<V: Equatable>(_ source: Binding<V>) -> Binding<V> {
        Binding(
            get: { source.wrappedValue },
            set: { newValue in
                source.wrappedValue = newValue
                scheduleSave()
            }
        )
    }

    private func scheduleSave() {
        saveTimer?.cancel()
        saveTimer = Just(())
            .delay(for: .milliseconds(400), scheduler: DispatchQueue.main)
            .sink { _ in settingsStore.save() }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/SettingsTabView.swift
git commit -m "feat(menubar): SettingsTabView (dense list, debounced auto-save)"
```

---

## Task 15: Wire `MenuBarContentView` and drop the standalone Settings scene

**Files:**

- Modify: `macos-menubar/Sources/App/Views.swift`
- Modify: `macos-menubar/Sources/App/App.swift`

- [ ] **Step 1: Replace `MenuBarContentView` body in `Views.swift`**

Open `macos-menubar/Sources/App/Views.swift`. Replace the entire `MenuBarContentView` struct (lines 182-393 in the current file) with the version below. The old `JobRowView` (lines 4-180) and `private func modeTint(...)` (lines 395-401) are removed because their functionality lives in the new tab views and `StatusChip`.

Replace lines `1..401` (i.e. the whole file) with:

```swift
import SwiftUI
import AppKit

struct MenuBarContentView: View {
    @ObservedObject var store: JobStore
    @ObservedObject var settingsStore: SettingsStore
    @ObservedObject var sandboxStore: SandboxStore

    @AppStorage("filesandbox.selectedTab") private var selectedTab: Int = 0

    var body: some View {
        VStack(spacing: 0) {
            AppHeader(store: store)
            AppTabs(
                selection: $selectedTab,
                counts: [
                    .jobs: store.visibleJobCount,
                    .sandbox: sandboxStore.activeCount
                ]
            )
            tabBody
            AppFooter(store: store)
        }
        .frame(width: 420)
        .background(Theme.panelBg)
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusPanel))
    }

    @ViewBuilder
    private var tabBody: some View {
        switch AppTab(rawValue: selectedTab) ?? .jobs {
        case .jobs:
            JobsTabView(store: store, sandboxStore: sandboxStore, settingsStore: settingsStore)
                .onAppear { store.fetch() }
        case .sandbox:
            SandboxTabView(store: sandboxStore, settingsStore: settingsStore)
        case .settings:
            SettingsTabView(settingsStore: settingsStore, store: store)
        }
    }
}
```

- [ ] **Step 2: Drop the `Settings { }` Scene from `App.swift`**

Open `macos-menubar/Sources/App/App.swift`. Replace the entire `body` plus the helper at the bottom with:

```swift
    var body: some Scene {
        MenuBarExtra {
            MenuBarContentView(store: store, settingsStore: settingsStore, sandboxStore: sandboxStore)
        } label: {
            Image(systemName: store.iconName)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(menuBarIconColor(for: store.mode))
        }
        .menuBarExtraStyle(.window)
        .onChange(of: store.mode) { _, newMode in
            guard !notifiedAtLaunch else { return }
            notifiedAtLaunch = true
            guard newMode != .active else { return }
            let content = UNMutableNotificationContent()
            content.title = "FileSandbox started in \(newMode.displayName)"
            content.body = newMode == .scanPaused
                ? "New files are quarantined but not scanned. Open the menu bar to resume."
                : "New files are not being monitored. Open the menu bar to resume."
            let req = UNNotificationRequest(identifier: "filesandbox.launch.mode", content: content, trigger: nil)
            UNUserNotificationCenter.current().add(req)
        }
    }
}

private func menuBarIconColor(for mode: WatcherMode) -> Color {
    switch mode {
    case .active: return .primary
    case .scanPaused: return .orange
    case .monitoringDisabled: return .red
    }
}
```

(The `Settings { SettingsView(store: settingsStore) }` block is removed because the Settings tab inside the dropdown is the sole settings surface per spec.)

- [ ] **Step 3: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

> If the build fails with `Cannot find 'SettingsView' in scope`, that's because `App.swift` still references it. The fix is the Step 2 replacement above. Re-check the file.
> If the build fails with `Cannot find 'SandboxView' in scope`, you missed removing the call site. There should be no remaining `SandboxView(...)` references in `Views.swift` after Step 1.

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/Views.swift macos-menubar/Sources/App/App.swift
git commit -m "feat(menubar): wire tabbed dropdown; drop standalone Settings scene"
```

---

## Task 16: Delete `SandboxView.swift` and `SettingsView.swift`

**Files:**

- Delete: `macos-menubar/Sources/App/SandboxView.swift`
- Delete: `macos-menubar/Sources/App/SettingsView.swift`

- [ ] **Step 1: Remove the two files**

```bash
git rm macos-menubar/Sources/App/SandboxView.swift macos-menubar/Sources/App/SettingsView.swift
```

- [ ] **Step 2: Verify build**

Run: `cd macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git commit -m "chore(menubar): remove obsolete SandboxView + SettingsView"
```

---

## Task 17: Manual acceptance run

**Files:** none (manual verification)

This task does not write code. It runs the redesigned app and walks through the spec's Acceptance checklist (§ Acceptance checklist, lines 290-297 of the design doc).

- [ ] **Step 1: Build the app**

Run: `cd macos-menubar && swift build -c release`
Expected: `Build complete!`

- [ ] **Step 2: Launch the app**

Run: `cd macos-menubar && swift run FileSandboxMenuBar &`
Expected: a shield-style icon appears in the macOS menu bar.

- [ ] **Step 3: Walk the acceptance checklist**

Click the menu bar icon and verify each line:

1. Dropdown opens at 420 pt with rounded corners; Jobs tab is selected by default.
2. Header shows `FileSandbox` left, status chip + ⋯ right.
3. Tabs row shows Jobs / Sandbox / Settings with count chips.
4. Footer shows `Restart daemon · View logs` left, red `Quit` right.
5. Click the status chip → mode menu opens; pick `Paused` → chip flips to orange `Scanning paused`.
6. Switch to Settings tab → toggle `VirusTotal` off and on; verify it round-trips with `curl -s http://127.0.0.1:3847/api/config | jq .vtEnabled` (after 400 ms debounce).
7. Click a quarantined job row → expands inline; verdict pill, engine card, meta pills, and three buttons render.
8. Switch to Sandbox tab → `+ New session` button + Network switch render. Action buttons in a session row are 28×28, always visible. State pill is far right.
9. Click `Discard` on a sandbox row → row disappears after API call.
10. Close the dropdown, click the menu bar icon again → it re-opens to the last selected tab.
11. No em dashes appear anywhere in the rendered UI (verify visually).

- [ ] **Step 4: Stop the app and record results**

```bash
pkill -f FileSandboxMenuBar || true
```

If anything failed, file the failures as follow-up issues — do not patch in this plan unless they reflect a regression introduced here.

- [ ] **Step 5: Final commit (acceptance trail)**

If everything passed and there are no further code changes, leave the commit history as-is. Otherwise, fix the regression in a small follow-up commit:

```bash
git add <fix files>
git commit -m "fix(menubar): <regression caught in acceptance>"
```

---

## Self-review

Performed against `docs/superpowers/specs/2026-05-04-menubar-ui-redesign-design.md`:

- **Frame (420 pt, panel bg, 12 pt corner)** → Task 15 (`.frame(width: 420)` + `RoundedRectangle(cornerRadius: Theme.cornerRadiusPanel)`).
- **Header chip + ⋯ overflow** → Tasks 7 + 10. Overflow includes Refresh / Restart daemon / View logs / Clear settled jobs / Quit. ✓
- **Pill segmented tabs with counts** → Task 8 + Task 15 (counts wired). ✓
- **Selected-tab `@AppStorage`** → Task 15. ✓
- **Jobs tab grouped (Scanning / Quarantine / Restored)** → Task 12. ✓ (`scanningJobs`, `quarantinedJobs`, `restoredJobs` in Task 2.)
- **Click-to-expand row with verdict pill, engine cards, meta pills, action buttons** → Task 12 (`expandedDetail`). ✓
- **Pre-empty placeholder using `settingsStore.watchPath`** → Task 12 (`emptyPlaceholder`). ✓
- **Sandbox tab top strip + Network switch** → Task 13. ✓
- **Sandbox row layout: HStack center alignment, 28×28 buttons always visible, state pill far right** → Task 13. ✓
- **Settings tab dense list, auto-save 400 ms debounce** → Task 14 (`scheduleSave`). ✓
- **Footer link-style ops + red Quit** → Task 11. ✓
- **Removed components** (Settings scene, header Refresh / Trash, standalone Quit) → Tasks 15 + 16. ✓
- **Theme tokens** → Task 1. ✓
- **`SandboxStore.canOpen`** → Task 2. ✓
- **Failure modes (daemon offline / sandbox disabled / both engines off / save failure)** → Tasks 12, 13, 14 (offline message / disabled message / red caption / save round-trip via debounce; spec's red-border-3s on save fail is intentionally simplified to "save call retried on next change" — flagged in spec §Open work, not a plan failure).
- **Restart daemon endpoint:** Spec calls for POST /api/restart; that endpoint does not exist in the daemon today (`grep -n "restart" src/ui-server.ts` returns nothing). The plan falls back to `store.stopDaemon()` per spec line 192's "otherwise" branch. ✓ (Tasks 10 + 11.)

**Type consistency:** `Theme`, `AppSwitch`, `VerdictPill`, `SessionStatePill`, `MetaPill`, `EngineCard`, `StatusChip`, `AppTabs`, `AppTab`, `SettingRow`, `SettingGroupHeader`, `AppHeader`, `AppFooter`, `JobsTabView`, `SandboxTabView`, `SettingsTabView`, `MenuBarContentView` — names used in later tasks match the names defined earlier. ✓

**Placeholder scan:** searched the plan for `TBD`, `TODO`, `implement later`, `add appropriate`, `handle edge cases`. Two `// Future:` comments appear in the Sandbox row's Show window / Export icon buttons — these are intentional stubs the spec does not require to be functional in this iteration (the spec's acceptance only checks `Discard`). They are documented inline so the implementer doesn't try to invent behaviour.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-04-menubar-ui-redesign.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
