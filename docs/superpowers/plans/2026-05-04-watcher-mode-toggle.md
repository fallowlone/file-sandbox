# watcher mode toggle and per-engine controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the watcher's single `paused` boolean with a richer `WatcherMode` (active / scan_paused / monitoring_disabled) and add orthogonal per-engine toggles (`pompelmiEnabled`, `vtEnabled`), persisted in `config.json` and surfaced in the menu bar app.

**Architecture:** A new `WatcherMode` type drives the file pipeline branching. A new `POST /api/watcher/mode` endpoint is the canonical control surface; `/pause` and `/resume` become deprecated aliases. The watcher's `setMode` aborts in-flight scans on transition out of `active`. Per-engine flags live in config and gate the local-scan and VT phases independently. The macOS menu bar app gets a three-way mode menu, an icon tint, and a "Scanners" settings section.

**Tech Stack:** Node 22, TypeScript via `--experimental-strip-types`, `better-sqlite3`, `express`, SwiftUI for the menu bar app, `UNUserNotificationCenter`.

---

## Reference

- Spec: `docs/superpowers/specs/2026-05-04-watcher-mode-toggle-design.md`
- Related: `docs/superpowers/specs/2026-05-04-pompelmi-local-scanner-design.md` (this plan assumes pompelmi plan landed first)
- Affected files: `src/watcher.ts`, `src/ui-server.ts`, `src/config.ts`, `src/index.ts`, `macos-menubar/Sources/App/{Views.swift,SettingsView.swift,SettingsStore.swift,JobStore.swift,App.swift}`.

## File Structure

| File | Responsibility | Status |
|---|---|---|
| `src/watcher-mode.ts` | `WatcherMode` type, `parseMode`, `MODES` array. | Create |
| `src/watcher.ts` | Replace `paused: boolean` with `mode: WatcherMode`. New `setMode`, abort in-flight scans on transition. Engine-aware scan branching. | Modify |
| `src/watcher-mode.test.ts` | Unit tests for `parseMode` fallback. | Create |
| `src/watcher.test.ts` | Tests for mode-driven `handleFile` branches using a stub job store. | Create |
| `src/ui-server.ts` | New `/api/watcher/mode` route; deprecated `/pause` `/resume`; `mode` and `scannersEnabled` in `/api/health`. | Modify |
| `src/config.ts` | Add `watcherMode`, `vtEnabled`. (`pompelmiEnabled` already added in pompelmi plan.) | Modify |
| `src/index.ts` | Pass `watcherMode`, `vtEnabled` into `Watcher`. Persist mode changes via `writeConfig`. | Modify |
| `macos-menubar/Sources/App/JobStore.swift` | Replace `isPaused` with `mode`; add `setMode(_:)`. | Modify |
| `macos-menubar/Sources/App/SettingsStore.swift` | Add `watcherMode`, `vtEnabled`, `pompelmiEnabled`, `pompelmiSocketPath`, `pompelmiFailureMode` codable + published. | Modify |
| `macos-menubar/Sources/App/Views.swift` | Replace play/pause Button with Menu of three modes. Tint menu bar icon. New "No active scanners" banner. | Modify |
| `macos-menubar/Sources/App/SettingsView.swift` | Add "Watcher" and "Scanners" sections. | Modify |
| `macos-menubar/Sources/App/App.swift` | On first health response, post `UNUserNotificationCenter` notification when mode != active. | Modify |

---

## Task 1: `WatcherMode` type module

**Files:**

- Create: `src/watcher-mode.ts`
- Create: `src/watcher-mode.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
// src/watcher-mode.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { parseMode, MODES, type WatcherMode } from "./watcher-mode.ts";

test("parseMode returns valid mode unchanged", () => {
  for (const m of MODES) assert.equal(parseMode(m), m);
});

test("parseMode falls back to active for invalid input", () => {
  assert.equal(parseMode("nope"), "active");
  assert.equal(parseMode(undefined), "active");
  assert.equal(parseMode(null), "active");
  assert.equal(parseMode(""), "active");
});

test("MODES contains exactly the three documented modes", () => {
  assert.deepEqual([...MODES].sort(), ["active", "monitoring_disabled", "scan_paused"]);
});
```

- [ ] **Step 2: Run, see fail**

```bash
yarn test
```

- [ ] **Step 3: Implement `src/watcher-mode.ts`**

```ts
export type WatcherMode = "active" | "scan_paused" | "monitoring_disabled";

export const MODES: readonly WatcherMode[] = [
  "active",
  "scan_paused",
  "monitoring_disabled",
] as const;

export function parseMode(input: unknown): WatcherMode {
  if (typeof input !== "string") return "active";
  return (MODES as readonly string[]).includes(input)
    ? (input as WatcherMode)
    : "active";
}
```

- [ ] **Step 4: Run, see pass**

```bash
yarn test
```

- [ ] **Step 5: Commit**

```bash
git add src/watcher-mode.ts src/watcher-mode.test.ts
git commit -m "feat: WatcherMode type with safe parser"
```

---

## Task 2: Config additions

**Files:**

- Modify: `src/config.ts`

- [ ] **Step 1: Extend `RawConfig`**

In `src/config.ts`, add to the `RawConfig` interface:

```ts
import type { WatcherMode } from "./watcher-mode.ts";

// ... existing fields ...

watcherMode?: WatcherMode | string;
/** Run VirusTotal scan stage. Defaults to true. */
vtEnabled?: boolean;
```

- [ ] **Step 2: Extend `config` literal**

Append to the `export const config = { ... }` literal:

```ts
import { parseMode } from "./watcher-mode.ts";

watcherMode: parseMode(file.watcherMode ?? process.env.WATCHER_MODE),
vtEnabled: file.vtEnabled ?? envBool("VT_ENABLED", true),
```

- [ ] **Step 3: Tests still pass**

```bash
yarn test
```

- [ ] **Step 4: Commit**

```bash
git add src/config.ts
git commit -m "feat(config): add watcherMode and vtEnabled"
```

---

## Task 3: Watcher state — replace `paused` with `mode`

**Files:**

- Modify: `src/watcher.ts`

- [ ] **Step 1: Add the field and initial mode option**

In `src/watcher.ts`:

```ts
import { type WatcherMode } from "./watcher-mode.ts";

export interface WatcherOptions {
  // ... existing ...
  /** Initial mode. Defaults to "active". */
  initialMode?: WatcherMode;
  /** Called when setMode persists a change (write to config). */
  onModeChange?: (mode: WatcherMode) => void;
  /** Whether VirusTotal scan stage runs. */
  vtEnabled?: boolean;
}
```

Replace `private paused = false;` with:

```ts
private mode: WatcherMode = "active";
private readonly onModeChange?: (mode: WatcherMode) => void;
private readonly vtEnabled: boolean;
```

In the constructor:

```ts
this.mode = opts?.initialMode ?? "active";
this.onModeChange = opts?.onModeChange;
this.vtEnabled = opts?.vtEnabled ?? true;
```

- [ ] **Step 2: Replace `pause` / `resume` / `isPaused` with mode methods**

Remove the existing `pause`, `resume`, and `isPaused` methods. Add:

```ts
getMode(): WatcherMode {
  return this.mode;
}

setMode(next: WatcherMode): void {
  if (next === this.mode) return;
  const prev = this.mode;
  this.mode = next;
  if (prev === "active" && next !== "active") {
    for (const c of this.scanControllers.values()) {
      c.abort();
    }
  }
  this.onModeChange?.(next);
}

/** @deprecated kept only for legacy /api/watcher/pause callers */
pause(): void { this.setMode("scan_paused"); }
/** @deprecated kept only for legacy /api/watcher/resume callers */
resume(): void { this.setMode("active"); }
get isPaused(): boolean { return this.mode !== "active"; }
```

- [ ] **Step 3: Branch the file callbacks on mode**

In the `fsWatch` raw callback (where `if (this.paused) return;` lives):

```ts
if (this.mode === "monitoring_disabled") return;
// rest of callback runs for both active and scan_paused
```

In `handleFile`:

Replace `if (this.paused) return;` at the top with:

```ts
if (this.mode === "monitoring_disabled") return;
```

After `setQuarantineXattr`, after `chmodAsync`, after `fileMover.move`, after `setInQuarantine`, after `changePermissions`, **before** `setScanning`, add the scan-paused short-circuit:

```ts
if (this.mode === "scan_paused") {
  this.jobStore?.setScanResult(jobId, {
    verdict: "inconclusive",
    message: "Scanning paused at intake",
  });
  console.log(`[mode=scan_paused] skipped scan, kept quarantined: ${quarantineFilePath}`);
  return;
}
```

- [ ] **Step 4: Gate VT stage with `vtEnabled`**

In the existing VT path, wrap the entire block from the `oversized` check through `setScanResult` for VT in:

```ts
if (!this.vtEnabled) {
  // No VT: if pompelmi already returned clean, treat as restored; otherwise inconclusive.
  // We rely on Task 6 of the pompelmi plan having already returned for malicious cases.
  // If we reached here without VT, file is clean enough for restore only when both engines were off
  // would be impossible (we'd have early-returned). So we mark inconclusive here.
  this.jobStore?.setScanResult(jobId, {
    verdict: "inconclusive",
    message: "VT disabled — no cloud scan ran",
  });
  console.log(`[vtEnabled=false] kept quarantined: ${quarantineFilePath}`);
  return;
}
// ... existing VT path ...
```

When **both** `pompelmiEnabled=false` and `vtEnabled=false` (no scanners at all), nothing has run by the time we reach this point. To keep code paths uniform, add an early no-scanner check right after the scan-paused short-circuit:

```ts
const noScanners = !this.localScanner && !this.vtEnabled;
if (noScanners) {
  this.jobStore?.setScanResult(jobId, {
    verdict: "inconclusive",
    message: "No active scanners — kept in quarantine",
  });
  console.log(`[no-scanners] kept quarantined: ${quarantineFilePath}`);
  return;
}
```

(`this.localScanner` is null when `pompelmiEnabled=false`, set in `src/index.ts` from the pompelmi plan.)

- [ ] **Step 5: Build the project (no test framework yet for watcher) — confirm types**

```bash
node --experimental-strip-types --check src/watcher.ts
```

Expected: no output (success).

- [ ] **Step 6: Commit**

```bash
git add src/watcher.ts
git commit -m "feat(watcher): WatcherMode replaces paused; vtEnabled gates VT stage"
```

---

## Task 4: Watcher tests for mode and engine branches

**Files:**

- Create: `src/watcher.test.ts`

- [ ] **Step 1: Write tests using a small stub `JobStore`**

Tests must avoid touching the filesystem. We test `setMode` and the abort behavior directly.

```ts
// src/watcher.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import Watcher from "./watcher.ts";

class StubJobStore {
  results: unknown[] = [];
  insertReceived() {}
  setInQuarantine() {}
  setScanning() {}
  setScanResult(_jobId: string, r: unknown) { this.results.push(r); }
  setRestored() {}
  setPompelmiVerdict() {}
  cancelJob() {}
  fail() {}
}

function makeWatcher(initialMode: "active" | "scan_paused" | "monitoring_disabled") {
  const store = new StubJobStore();
  const w = new Watcher(
    "/tmp/watch-stub",
    [],
    "/tmp/quarantine-stub",
    "test-key",
    store as any,
    {
      initialMode,
      vtEnabled: true,
      pompelmiFailureMode: "bypass",
    },
  );
  return { w, store };
}

test("setMode aborts existing controllers when leaving active", () => {
  const { w } = makeWatcher("active");
  const c1 = new AbortController();
  const c2 = new AbortController();
  // @ts-expect-error: poke private map for the test
  w.scanControllers.set("a", c1);
  // @ts-expect-error
  w.scanControllers.set("b", c2);

  w.setMode("scan_paused");

  assert.equal(c1.signal.aborted, true);
  assert.equal(c2.signal.aborted, true);
});

test("setMode same-state is a no-op", () => {
  const { w } = makeWatcher("scan_paused");
  const c = new AbortController();
  // @ts-expect-error
  w.scanControllers.set("a", c);
  w.setMode("scan_paused");
  assert.equal(c.signal.aborted, false);
});

test("getMode reflects setMode", () => {
  const { w } = makeWatcher("active");
  w.setMode("monitoring_disabled");
  assert.equal(w.getMode(), "monitoring_disabled");
});
```

- [ ] **Step 2: Run, see pass**

```bash
yarn test
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/watcher.test.ts
git commit -m "test(watcher): mode transition aborts in-flight scans"
```

---

## Task 5: HTTP — canonical `/api/watcher/mode` and deprecated aliases

**Files:**

- Modify: `src/ui-server.ts`

- [ ] **Step 1: Extend `WatcherControl` interface**

```ts
import type { WatcherMode } from "./watcher-mode.ts";
import { MODES, parseMode } from "./watcher-mode.ts";

export interface WatcherControl {
  getMode: () => WatcherMode;
  setMode: (mode: WatcherMode) => void;
  // Legacy passthroughs (kept for back-compat):
  pause: () => void;
  resume: () => void;
  isPaused: () => boolean;
}
```

- [ ] **Step 2: Replace `paused` field with `mode` in `/api/jobs`, keep `paused` for one release**

```ts
res.json({
  jobs: store.listRecent(200),
  mode: watcherControl?.getMode() ?? "active",
  paused: (watcherControl?.getMode() ?? "active") !== "active",
});
```

- [ ] **Step 3: Add `POST /api/watcher/mode`**

```ts
app.post("/api/watcher/mode", (req, res) => {
  if (!watcherControl) {
    res.status(501).json({ error: "watcher control not configured" });
    return;
  }
  const requested = parseMode(req.body?.mode);
  // parseMode falls back to "active" silently. Reject unknown explicitly.
  if (req.body?.mode && requested !== req.body.mode) {
    res.status(400).json({
      error: "unknown mode",
      allowed: [...MODES],
      received: req.body?.mode,
    });
    return;
  }
  watcherControl.setMode(requested);
  res.json({ ok: true, mode: requested });
});
```

- [ ] **Step 4: Make `/api/watcher/pause` and `/api/watcher/resume` aliases**

Replace bodies of the existing handlers:

```ts
app.post("/api/watcher/pause", (_req, res) => {
  if (!watcherControl) return res.status(501).json({ error: "watcher control not configured" });
  console.warn("[deprecated] POST /api/watcher/pause — use /api/watcher/mode");
  watcherControl.setMode("scan_paused");
  res.json({ ok: true, paused: true, mode: "scan_paused" });
});

app.post("/api/watcher/resume", (_req, res) => {
  if (!watcherControl) return res.status(501).json({ error: "watcher control not configured" });
  console.warn("[deprecated] POST /api/watcher/resume — use /api/watcher/mode");
  watcherControl.setMode("active");
  res.json({ ok: true, paused: false, mode: "active" });
});
```

- [ ] **Step 5: Augment `/api/health`**

Add to the health response object:

```ts
mode: watcherControl?.getMode() ?? "active",
scannersEnabled: {
  pompelmi: config.pompelmiEnabled,
  vt: config.vtEnabled,
},
```

- [ ] **Step 6: Manual test**

```bash
curl -sX POST -H "content-type: application/json" -d '{"mode":"scan_paused"}' http://127.0.0.1:3847/api/watcher/mode -H "Authorization: Bearer $TOKEN"
curl -s http://127.0.0.1:3847/api/health | jq .mode
```

Expected: `"scan_paused"`. Then:

```bash
curl -sX POST -H "content-type: application/json" -d '{"mode":"banana"}' http://127.0.0.1:3847/api/watcher/mode -H "Authorization: Bearer $TOKEN"
```

Expected: HTTP 400 with `{"error":"unknown mode","allowed":[...],"received":"banana"}`.

- [ ] **Step 7: Commit**

```bash
git add src/ui-server.ts
git commit -m "feat(api): /api/watcher/mode canonical + deprecated /pause /resume aliases"
```

---

## Task 6: Wire mode + persistence through `src/index.ts`

**Files:**

- Modify: `src/index.ts`

- [ ] **Step 1: Read mode from config; pass to Watcher; persist on change**

In `src/index.ts`:

```ts
import { writeConfig } from "./config.ts";

// when constructing the Watcher:
const watcher = new Watcher(watchPath, ignored, quarantinePath, vtApiKey, jobStore, {
  // ... existing options ...
  initialMode: config.watcherMode,
  vtEnabled: config.vtEnabled,
  localScanner,
  pompelmiFailureMode: config.pompelmiFailureMode,
  onModeChange: (m) => {
    try {
      writeConfig({ watcherMode: m });
    } catch (e) {
      console.error(`[config] failed to persist mode: ${(e as Error).message}`);
    }
  },
});
```

- [ ] **Step 2: Build the watcher control surface for the HTTP server**

```ts
const watcherControl: WatcherControl = {
  getMode: () => watcher.getMode(),
  setMode: (m) => watcher.setMode(m),
  pause: () => watcher.setMode("scan_paused"),
  resume: () => watcher.setMode("active"),
  isPaused: () => watcher.getMode() !== "active",
};

startUiServer(jobStore, port, ..., watcherControl);
```

- [ ] **Step 3: Manual test — restart preserves mode**

1. Set mode `scan_paused` via the API.
2. Stop the daemon (Ctrl-C / `stopDaemon`).
3. `cat config.json` (or read encrypted via the API) — `watcherMode: "scan_paused"` is present.
4. Restart daemon. `/api/health` returns `"mode":"scan_paused"`.

- [ ] **Step 4: Commit**

```bash
git add src/index.ts
git commit -m "feat(daemon): persist watcherMode and feed Watcher constructor"
```

---

## Task 7: SwiftUI — `JobStore` mode field

**Files:**

- Modify: `macos-menubar/Sources/App/JobStore.swift`

- [ ] **Step 1: Add a `WatcherMode` enum**

At the top of `JobStore.swift`:

```swift
enum WatcherMode: String, Codable, CaseIterable {
    case active
    case scanPaused = "scan_paused"
    case monitoringDisabled = "monitoring_disabled"

    var displayName: String {
        switch self {
        case .active: return "Active"
        case .scanPaused: return "Scanning paused"
        case .monitoringDisabled: return "Monitoring disabled"
        }
    }

    var symbolName: String {
        switch self {
        case .active: return "play.circle.fill"
        case .scanPaused: return "pause.circle.fill"
        case .monitoringDisabled: return "eye.slash.fill"
        }
    }
}
```

- [ ] **Step 2: Replace `@Published var isPaused` with `@Published var mode`**

```swift
@Published var mode: WatcherMode = .active
var isPaused: Bool { mode != .active }
```

- [ ] **Step 3: Decode mode from `/api/jobs`**

In whatever struct decodes the `/api/jobs` response (search for the existing `paused` field):

Add `let mode: String?` and after decoding, set:

```swift
self.mode = WatcherMode(rawValue: decoded.mode ?? "active") ?? .active
```

If `mode` is missing in the response (older daemon), fall back to interpreting `paused`:

```swift
self.mode = decoded.mode.flatMap(WatcherMode.init(rawValue:)) ?? (decoded.paused ? .scanPaused : .active)
```

- [ ] **Step 4: Replace `pauseWatcher` / `resumeWatcher` with `setMode`**

Remove the two methods. Add:

```swift
func setMode(_ next: WatcherMode) {
    guard let url = URL(string: "http://127.0.0.1:\(port)/api/watcher/mode") else { return }
    var req = authorizedRequest(url: url) // existing helper used elsewhere
    req.httpMethod = "POST"
    req.setValue("application/json", forHTTPHeaderField: "Content-Type")
    req.httpBody = try? JSONSerialization.data(withJSONObject: ["mode": next.rawValue])
    URLSession.shared.dataTask(with: req) { [weak self] _, _, error in
        DispatchQueue.main.async {
            if error == nil { self?.mode = next }
        }
    }.resume()
}
```

(If `authorizedRequest` for non-config endpoints does not exist, copy the bearer-attaching pattern from `SettingsStore.authorizedConfigRequest`.)

- [ ] **Step 5: Build the menu bar app**

```bash
cd macos-menubar
swift build
```

Expected: build succeeds. Fix any callers of `pauseWatcher`/`resumeWatcher` revealed in the next task.

- [ ] **Step 6: Commit**

```bash
git add macos-menubar/Sources/App/JobStore.swift
git commit -m "feat(menubar): JobStore exposes WatcherMode and setMode"
```

---

## Task 8: SwiftUI — replace play/pause button with three-mode menu

**Files:**

- Modify: `macos-menubar/Sources/App/Views.swift`

- [ ] **Step 1: Replace the Button at the original `Views.swift:209-217` location**

```swift
if store.isConnected {
    Menu {
        ForEach(WatcherMode.allCases, id: \.self) { m in
            Button {
                store.setMode(m)
            } label: {
                Label {
                    Text(m.displayName)
                } icon: {
                    if store.mode == m {
                        Image(systemName: "checkmark")
                    }
                }
            }
        }
    } label: {
        Image(systemName: store.mode.symbolName)
            .font(.system(size: 11))
            .foregroundColor(modeTint(store.mode))
    }
    .menuStyle(.borderlessButton)
    .frame(width: 24)
    .help("Watcher mode")
}
```

Add a helper at the bottom of the file:

```swift
private func modeTint(_ mode: WatcherMode) -> Color {
    switch mode {
    case .active: return .secondary
    case .scanPaused: return .orange
    case .monitoringDisabled: return .red
    }
}
```

- [ ] **Step 2: Replace the `Paused` badge with a per-mode chip**

Find the existing `if store.isConnected && store.isPaused { Text("Paused") ... }` block. Replace with:

```swift
if store.isConnected {
    Text(store.mode.displayName)
        .font(.system(size: 10, weight: .semibold))
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .background(modeTint(store.mode).opacity(0.18))
        .foregroundColor(modeTint(store.mode))
        .clipShape(RoundedRectangle(cornerRadius: 4))
}
```

- [ ] **Step 3: Tint the connection dot to match mode when not active**

Replace:

```swift
.fill(store.isConnected ? (store.isPaused ? Color.orange : Color.green) : Color.red)
```

with:

```swift
.fill(store.isConnected ? (store.mode == .active ? Color.green : modeTint(store.mode)) : Color.red)
```

- [ ] **Step 4: Build**

```bash
swift build --package-path macos-menubar
```

Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Views.swift
git commit -m "feat(menubar): three-mode menu replaces play/pause button"
```

---

## Task 9: Tint the menu bar status icon globally

**Files:**

- Modify: `macos-menubar/Sources/App/App.swift`

- [ ] **Step 1: Locate the `NSStatusItem.button.image` assignment**

Search for `NSStatusItem` or `NSStatusBar.system.statusItem` in `App.swift`. Find where the icon is set.

- [ ] **Step 2: React to mode changes by re-tinting**

Add a Combine sink (or `objectWillChange` observer) on `JobStore.mode`:

```swift
import Combine
// inside AppDelegate or @main App:
private var modeCancellable: AnyCancellable?

func setupModeIconBinding(store: JobStore, statusItem: NSStatusItem) {
    modeCancellable = store.$mode.sink { mode in
        DispatchQueue.main.async {
            guard let image = NSImage(named: "MenuBarIcon") ?? NSImage(systemSymbolName: "shield", accessibilityDescription: nil) else { return }
            let tinted = image.copy() as! NSImage
            tinted.isTemplate = false
            switch mode {
            case .active: tinted.lockFocus(); NSColor.labelColor.set(); NSRect(origin: .zero, size: tinted.size).fill(using: .sourceAtop); tinted.unlockFocus()
            case .scanPaused: tinted.lockFocus(); NSColor.systemOrange.set(); NSRect(origin: .zero, size: tinted.size).fill(using: .sourceAtop); tinted.unlockFocus()
            case .monitoringDisabled: tinted.lockFocus(); NSColor.systemRed.set(); NSRect(origin: .zero, size: tinted.size).fill(using: .sourceAtop); tinted.unlockFocus()
            }
            statusItem.button?.image = tinted
        }
    }
}
```

Call `setupModeIconBinding(store:statusItem:)` after the status item is created.

- [ ] **Step 3: Build, run, verify icon tints**

```bash
swift run --package-path macos-menubar
```

Toggle modes from the Menu (Task 8). Expected: icon changes color promptly.

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/App.swift
git commit -m "feat(menubar): tint status item icon based on watcher mode"
```

---

## Task 10: Settings UI — Watcher and Scanners sections

**Files:**

- Modify: `macos-menubar/Sources/App/SettingsStore.swift`
- Modify: `macos-menubar/Sources/App/SettingsView.swift`

- [ ] **Step 1: Extend `DaemonConfig` and `SettingsStore`**

In `SettingsStore.swift`:

```swift
struct DaemonConfig: Codable {
    // ... existing ...
    var watcherMode: String?
    var vtEnabled: Bool?
    var pompelmiEnabled: Bool?
    var pompelmiSocketPath: String?
    var pompelmiFailureMode: String?
}

class SettingsStore: ObservableObject {
    // ... existing ...
    @Published var watcherMode: WatcherMode = .active
    @Published var vtEnabled: Bool = true
    @Published var pompelmiEnabled: Bool = true
    @Published var pompelmiSocketPath: String = "/tmp/clamd.sock"
    @Published var pompelmiFailureMode: String = "bypass"
}
```

In `fetch()`, after the existing decode block:

```swift
self.watcherMode = WatcherMode(rawValue: decoded.watcherMode ?? "active") ?? .active
self.vtEnabled = decoded.vtEnabled ?? true
self.pompelmiEnabled = decoded.pompelmiEnabled ?? true
self.pompelmiSocketPath = decoded.pompelmiSocketPath ?? "/tmp/clamd.sock"
self.pompelmiFailureMode = decoded.pompelmiFailureMode ?? "bypass"
```

In `save()`, in the body dict literal:

```swift
"watcherMode": watcherMode.rawValue,
"vtEnabled": vtEnabled,
"pompelmiEnabled": pompelmiEnabled,
"pompelmiSocketPath": pompelmiSocketPath,
"pompelmiFailureMode": pompelmiFailureMode,
```

- [ ] **Step 2: Add new sections to `SettingsView.swift`**

Insert two new `Section` blocks alongside the existing ones:

```swift
Section("Watcher") {
    Picker("Mode", selection: $store.watcherMode) {
        ForEach(WatcherMode.allCases, id: \.self) { m in
            Text(m.displayName).tag(m)
        }
    }
    .pickerStyle(.segmented)
    Text(modeExplainer(store.watcherMode))
        .font(.caption)
        .foregroundColor(.secondary)
}

Section("Scanners") {
    Toggle("Local scanner (pompelmi/ClamAV)", isOn: $store.pompelmiEnabled)
    if store.pompelmiEnabled {
        TextField("clamd socket path", text: $store.pompelmiSocketPath)
        Picker("On scan error", selection: $store.pompelmiFailureMode) {
            Text("Bypass to VT").tag("bypass")
            Text("Mark inconclusive").tag("inconclusive")
        }
    }
    Toggle("VirusTotal cloud", isOn: $store.vtEnabled)
    if !store.pompelmiEnabled && !store.vtEnabled {
        Text("⚠️ No active scanners — every new file will be quarantined as inconclusive.")
            .font(.caption)
            .foregroundColor(.red)
    }
}
```

Add helper:

```swift
private func modeExplainer(_ m: WatcherMode) -> String {
    switch m {
    case .active: return "Files are quarantined and scanned."
    case .scanPaused: return "Files are quarantined but not scanned. Restore manually after review."
    case .monitoringDisabled: return "Watcher ignores new files entirely. Advanced — files are not protected."
    }
}
```

- [ ] **Step 3: Build**

```bash
swift build --package-path macos-menubar
```

Expected: build succeeds.

- [ ] **Step 4: Manual test**

Open Settings, verify Watcher mode picker reflects the daemon's current mode and persists on Save.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/SettingsStore.swift macos-menubar/Sources/App/SettingsView.swift
git commit -m "feat(menubar): Watcher mode and Scanners sections in Settings"
```

---

## Task 11: Notification when daemon starts in non-active mode

**Files:**

- Modify: `macos-menubar/Sources/App/App.swift`

- [ ] **Step 1: Add notification permission request once**

In `applicationDidFinishLaunching` (or `@main App.init`):

```swift
import UserNotifications
UNUserNotificationCenter.current().requestAuthorization(options: [.alert]) { _, _ in }
```

- [ ] **Step 2: Post notification on first non-active mode observation**

```swift
private var notifiedAtLaunch = false

func observeForLaunchNotification(store: JobStore) {
    modeCancellable = store.$mode
        .filter { _ in !self.notifiedAtLaunch }
        .sink { mode in
            self.notifiedAtLaunch = true
            guard mode != .active else { return }
            let content = UNMutableNotificationContent()
            content.title = "FileSandbox started in \(mode.displayName)"
            content.body = mode == .scanPaused
                ? "New files are quarantined but not scanned. Open the menu bar to resume."
                : "New files are not being monitored. Open the menu bar to resume."
            let req = UNNotificationRequest(identifier: "filesandbox.launch.mode", content: content, trigger: nil)
            UNUserNotificationCenter.current().add(req)
        }
}
```

Replace the previous `setupModeIconBinding` Combine sink with one that does both icon tinting and the launch notification, or chain two sinks on the same publisher.

- [ ] **Step 3: Manual test**

Set mode `scan_paused` via API, restart daemon and menu bar app. Expected: a macOS notification appears once on launch.

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/App.swift
git commit -m "feat(menubar): launch notification when watcher mode is not active"
```

---

## Task 12: End-to-end manual verification

**No file changes. Verification gate.**

- [ ] **Step 1: Mode persistence**

1. Daemon starts. `/api/health` `mode == "active"`.
2. POST `/api/watcher/mode` with `scan_paused`.
3. Restart daemon. `/api/health` `mode == "scan_paused"`.

- [ ] **Step 2: scan_paused intake**

1. Set mode `scan_paused`. Drop a file in the watch folder.
2. Inspect DB: `select status, vt_verdict, pompelmi_verdict, detail from jobs order by created_at desc limit 1`.
3. Expected: `quarantine_kept | <null> | <null> | "Scanning paused at intake"`.

- [ ] **Step 3: monitoring_disabled intake**

1. Set mode `monitoring_disabled`. Drop a file. Wait 10s.
2. The file is NOT in the quarantine folder, NOT chmodded, no DB row inserted.
3. Verify directly: `ls -l "$WATCH_PATH/<file>"` shows original perms.

- [ ] **Step 4: per-engine toggles**

1. Set `vtEnabled=false`, `pompelmiEnabled=true`. Drop a clean file. Expected: `pompelmi_verdict='clean'`, `vt_verdict=NULL`, `status='quarantine_kept'`, detail says VT disabled.
2. Set both off. Drop a file. Expected: `inconclusive`, detail "No active scanners".

- [ ] **Step 5: Mode change cancels in-flight scan**

1. Set mode `active`, drop a large file (≤400 MB) so VT polling is in progress.
2. Within 30s, POST `/api/watcher/mode` with `scan_paused`.
3. Inspect DB row for that job. Expected: `status='cancelled'`, detail "Cancelled by user".

---

## Self-Review Checklist (already run)

- Spec coverage:
  - Watcher modes (3): Tasks 1, 3, 4, 7, 8, 10, 12.
  - Engine flags: Tasks 2, 3, 5, 10, 12.
  - API: Task 5.
  - Persistence: Task 6.
  - Menu bar UI: Tasks 7, 8, 9, 10, 11.
  - Notification: Task 11.
- No placeholders. Every code change is shown verbatim.
- Type names consistent: `WatcherMode`, `setMode`, `getMode`, `parseMode`, `MODES` everywhere.
- All tasks end in commit; Task 12 is verification only.
