# Linux Sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace removed Tart macOS-VM sandbox with a Linux-based equivalent — an ephemeral, hardened Debian VM launched via Apple Virtualization.framework and orchestrated entirely from the Swift menubar app.

**Architecture:** All sandbox state, configuration, and lifecycle move out of the TS daemon and into a new `Sandbox/` Swift module that owns `VZVirtualMachine` instances per session. A reproducible Debian-slim squashfs image (`base.img`) is built via `mkosi`, mounted read-only, with tmpfs for writable paths and virtiofs for in/out file exchange. The daemon keeps only watch/scan/quarantine; sandbox config is owned by the menubar.

**Tech Stack:** Swift 5.9, SwiftUI, AppKit, Virtualization.framework, XCTest, mkosi, Debian (snapshot mirror), squashfs, AppArmor, virtio-fs, virtio-gpu, virtio-input.

**Spec:** `docs/superpowers/specs/2026-05-05-linux-sandbox-design.md`

---

## File Structure

### Daemon (TS) — removed/edited
- Delete: `src/sandbox-paths.ts`, `src/sandbox-paths.test.ts`
- Delete: `src/sandbox-store.ts`, `src/sandbox-store.test.ts`
- Edit: `src/config.ts` — drop 5 `sandbox*` keys + their env handling
- Edit: `src/ui-server.ts` — drop the 503-stub `/api/sandbox/*` routes and `sandboxInfo` block
- Edit: any compiled `.js` siblings that mirror deleted `.ts` files

### Menubar Swift — new module `Sources/App/Sandbox/`
- `Sources/App/Sandbox/SandboxManager.swift` — public façade, owns VMs (main actor)
- `Sources/App/Sandbox/SessionStore.swift` — JSON persistence of session metadata
- `Sources/App/Sandbox/SandboxWindowController.swift` — NSWindow + VZVirtualMachineView per session
- `Sources/App/Sandbox/VMConfig.swift` — builds `VZVirtualMachineConfiguration`
- `Sources/App/Sandbox/PathValidator.swift` — symlink/hardlink rejection, allowed-root check
- `Sources/App/Sandbox/IdleMonitor.swift` — inactivity timer, hard cap, sleep-discard
- `Sources/App/Sandbox/SandboxConfig.swift` — local sandbox-only settings JSON

### Menubar Swift — edited
- `macos-menubar/Package.swift` — link `Virtualization` framework
- `macos-menubar/build.sh` — codesign --force --sign - --entitlements --options runtime
- `macos-menubar/sandbox.entitlements` — NEW, `com.apple.security.virtualization`
- `Sources/App/SandboxStore.swift` — replace with new local-only store, drop REST calls
- `Sources/App/SettingsStore.swift` — remove sandbox* fields
- `Sources/App/Tabs/SandboxTabView.swift` — bind to new `SandboxManager` + `SandboxConfig`
- `Sources/App/Tabs/SettingsTabView.swift` — Sandbox section bound to `SandboxConfig`
- `Sources/App/Tabs/JobsTabView.swift` — "Open in sandbox" calls `SandboxManager`

### Swift tests — new
- `macos-menubar/Tests/SandboxTests/PathValidatorTests.swift`
- `macos-menubar/Tests/SandboxTests/SessionStoreTests.swift`
- `macos-menubar/Tests/SandboxTests/VMConfigTests.swift`
- `macos-menubar/Tests/SandboxTests/IdleMonitorTests.swift`
- `macos-menubar/Tests/SandboxTests/SandboxConfigTests.swift`

### Sandbox image — new tree `sandbox-image/`
- `sandbox-image/mkosi.conf`
- `sandbox-image/mkosi.skeleton/etc/fstab`
- `sandbox-image/mkosi.skeleton/etc/default/grub.d/99-sandbox.cfg`
- `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.evince`
- `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.eog`
- `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.mpv`
- `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.libreoffice`
- `sandbox-image/mkosi.skeleton/etc/sandbox-init`
- `sandbox-image/mkosi.skeleton/etc/systemd/system/sandbox-launch.service`
- `sandbox-image/mkosi.postinst`
- `sandbox-image/mkosi.finalize`
- `sandbox-image/README.md`
- `package.json` — add `sandbox:build` script
- `scripts/sandbox-build.sh` — wrapper invoking mkosi via Lima/Docker

---

## Phase A — Daemon Cleanup

### Task A1: Remove sandbox keys from daemon config

**Files:**
- Modify: `src/config.ts:37-41,185-198`
- Modify: `src/config.js` (mirror)

- [ ] **Step 1: Remove fields from `RawConfig` interface**

In `src/config.ts`, delete lines defining these fields:

```ts
sandboxEnabled?: boolean;
sandboxIdleTimeoutMinutes?: number;
sandboxNetworkDefault?: boolean;
sandboxSessionsDir?: string;
sandboxOutRetentionDays?: number;
```

- [ ] **Step 2: Remove the resolution block in `loadConfig`**

Delete the block that produces `sandboxEnabled`, `sandboxIdleTimeoutMinutes`, `sandboxNetworkDefault`, `sandboxSessionsDir`, `sandboxOutRetentionDays` (currently lines 185–198).

- [ ] **Step 3: Mirror the same deletions in `src/config.js`**

The repo ships `.js` siblings; remove the same keys/blocks from `src/config.js`.

- [ ] **Step 4: Type-check**

Run: `yarn tsc --noEmit`
Expected: no errors. If errors mention `cfg.sandbox*` in any other file, fix them in the next tasks.

- [ ] **Step 5: Commit**

```bash
git add src/config.ts src/config.js
git commit -m "refactor(daemon): drop sandbox* config keys (moved to menubar)"
```

### Task A2: Remove `/api/sandbox/*` stubs from ui-server

**Files:**
- Modify: `src/ui-server.ts:139,155,211-217`
- Modify: `src/ui-server.js` (mirror)

- [ ] **Step 1: Delete the stub route block**

Remove the `sandboxDisabled` handler and all 5 `/api/sandbox/...` registrations (currently lines 211–217 in `src/ui-server.ts`).

- [ ] **Step 2: Remove `sandboxInfo` from `/api/status` payload**

Delete the line `const sandboxInfo = { enabled: false, backendReady: false, activeSessions: 0 };` and remove the `sandbox: sandboxInfo,` field from the JSON returned in the status handler.

- [ ] **Step 3: Mirror deletions in `src/ui-server.js`**

- [ ] **Step 4: Run daemon test suite**

Run: `yarn test`
Expected: all existing tests pass; no test currently asserts `sandbox` keys.

- [ ] **Step 5: Commit**

```bash
git add src/ui-server.ts src/ui-server.js
git commit -m "refactor(daemon): remove /api/sandbox/* stub routes and status block"
```

### Task A3: Delete sandbox-store and sandbox-paths sources

**Files:**
- Delete: `src/sandbox-store.ts`, `src/sandbox-store.test.ts`
- Delete: `src/sandbox-paths.ts`, `src/sandbox-paths.test.ts`
- Delete (if present): `src/sandbox-store.js`, `src/sandbox-paths.js`

- [ ] **Step 1: Verify no other source imports them**

Run: `grep -rn "sandbox-store\|sandbox-paths" src/ --include='*.ts'`
Expected: only the files being deleted match.

- [ ] **Step 2: Remove the files**

```bash
git rm src/sandbox-store.ts src/sandbox-store.test.ts \
       src/sandbox-paths.ts src/sandbox-paths.test.ts
git rm -f src/sandbox-store.js src/sandbox-paths.js 2>/dev/null || true
```

- [ ] **Step 3: Type-check + tests**

Run: `yarn tsc --noEmit && yarn test`
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git commit -m "refactor(daemon): delete sandbox-store/sandbox-paths (moving to Swift)"
```

---

## Phase B — Swift Skeleton & Build Wiring

### Task B1: Link Virtualization framework + add entitlements + codesign step

**Files:**
- Modify: `macos-menubar/Package.swift`
- Create: `macos-menubar/sandbox.entitlements`
- Modify: `macos-menubar/build.sh`

- [ ] **Step 1: Update Package.swift to link Virtualization**

Replace the `executableTarget` block in `macos-menubar/Package.swift` with:

```swift
.executableTarget(
    name: "FileSandboxMenuBar",
    path: "Sources/App",
    resources: [
        .process("Resources")
    ],
    linkerSettings: [
        .linkedFramework("Virtualization")
    ]
),
```

- [ ] **Step 2: Create entitlements file**

Create `macos-menubar/sandbox.entitlements`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.virtualization</key>
    <true/>
</dict>
</plist>
```

- [ ] **Step 3: Add codesign step to build.sh**

In `macos-menubar/build.sh`, after the bundle is fully assembled (after the `Info.plist` write but before the script exits), append:

```bash
echo "Ad-hoc codesigning with virtualization entitlement..."
codesign --force --sign - \
         --entitlements sandbox.entitlements \
         --options runtime \
         "$APP"
codesign --verify --verbose=2 "$APP"
```

- [ ] **Step 4: Build to verify**

Run: `bash macos-menubar/build.sh`
Expected: build succeeds, signature verifies.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Package.swift macos-menubar/sandbox.entitlements macos-menubar/build.sh
git commit -m "build(menubar): link Virtualization, add entitlement, ad-hoc codesign"
```

### Task B2: Create test target wiring

**Files:**
- Modify: `macos-menubar/Package.swift`
- Create: `macos-menubar/Tests/SandboxTests/PlaceholderTests.swift`

- [ ] **Step 1: Add a `testTarget` to Package.swift**

Append to `targets` array in `macos-menubar/Package.swift`:

```swift
.testTarget(
    name: "SandboxTests",
    dependencies: ["FileSandboxMenuBar"],
    path: "Tests/SandboxTests"
),
```

- [ ] **Step 2: Create placeholder test so the target compiles**

Create `macos-menubar/Tests/SandboxTests/PlaceholderTests.swift`:

```swift
import XCTest

final class PlaceholderTests: XCTestCase {
    func testTargetCompiles() {
        XCTAssertTrue(true)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd macos-menubar && swift test`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Package.swift macos-menubar/Tests/
git commit -m "test(menubar): bootstrap SandboxTests target"
```

### Task B3: PathValidator (TDD)

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/PathValidator.swift`
- Create: `macos-menubar/Tests/SandboxTests/PathValidatorTests.swift`

- [ ] **Step 1: Write failing tests**

Create `macos-menubar/Tests/SandboxTests/PathValidatorTests.swift`:

```swift
import XCTest
@testable import FileSandboxMenuBar

final class PathValidatorTests: XCTestCase {
    var tmp: URL!
    var validator: PathValidator!

    override func setUpWithError() throws {
        tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("pv-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: tmp.appendingPathComponent("watch"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: tmp.appendingPathComponent("quarantine"), withIntermediateDirectories: true)
        validator = PathValidator(allowedRoots: [
            tmp.appendingPathComponent("watch"),
            tmp.appendingPathComponent("quarantine"),
        ])
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tmp)
    }

    func testAcceptsRegularFileInsideAllowedRoot() throws {
        let f = tmp.appendingPathComponent("watch/a.txt")
        try "x".write(to: f, atomically: true, encoding: .utf8)
        XCTAssertNoThrow(try validator.validate(path: f.path))
    }

    func testRejectsSymlink() throws {
        let real = tmp.appendingPathComponent("real.txt")
        try "x".write(to: real, atomically: true, encoding: .utf8)
        let link = tmp.appendingPathComponent("watch/link.txt")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: real)
        XCTAssertThrowsError(try validator.validate(path: link.path)) { err in
            XCTAssertEqual(err as? PathValidator.Error, .symlink)
        }
    }

    func testRejectsHardlinkOutsideAllowedRoots() throws {
        let outside = tmp.appendingPathComponent("outside.txt")
        try "x".write(to: outside, atomically: true, encoding: .utf8)
        let inside = tmp.appendingPathComponent("watch/hl.txt")
        try FileManager.default.linkItem(at: outside, to: inside)
        // realpath of `inside` resolves to itself; rejection happens because link count > 1
        XCTAssertThrowsError(try validator.validate(path: inside.path)) { err in
            XCTAssertEqual(err as? PathValidator.Error, .hardlink)
        }
    }

    func testRejectsPathOutsideAllowedRoots() throws {
        let outside = tmp.appendingPathComponent("outside.txt")
        try "x".write(to: outside, atomically: true, encoding: .utf8)
        XCTAssertThrowsError(try validator.validate(path: outside.path)) { err in
            XCTAssertEqual(err as? PathValidator.Error, .notInAllowedRoot)
        }
    }

    func testRejectsRelativePath() throws {
        XCTAssertThrowsError(try validator.validate(path: "../etc/passwd")) { err in
            XCTAssertEqual(err as? PathValidator.Error, .notAbsolute)
        }
    }

    func testResolvesRealPath() throws {
        let realDir = tmp.appendingPathComponent("watch/sub").standardizedFileURL
        try FileManager.default.createDirectory(at: realDir, withIntermediateDirectories: true)
        let f = realDir.appendingPathComponent("c.txt")
        try "x".write(to: f, atomically: true, encoding: .utf8)
        // Path with redundant `.` segments must still resolve
        let messy = tmp.path + "/watch/./sub/c.txt"
        XCTAssertNoThrow(try validator.validate(path: messy))
    }
}
```

- [ ] **Step 2: Run, verify failure**

Run: `cd macos-menubar && swift test --filter PathValidatorTests`
Expected: compile failure, `PathValidator` not defined.

- [ ] **Step 3: Implement PathValidator**

Create `macos-menubar/Sources/App/Sandbox/PathValidator.swift`:

```swift
import Foundation

public struct PathValidator {
    public enum Error: Swift.Error, Equatable {
        case notAbsolute
        case symlink
        case hardlink
        case notInAllowedRoot
        case notRegularFile
        case ioError(String)
    }

    private let allowedRoots: [URL]

    public init(allowedRoots: [URL]) {
        self.allowedRoots = allowedRoots.map { $0.standardizedFileURL.resolvingSymlinksInPath() }
    }

    public func validate(path: String) throws {
        guard path.hasPrefix("/") else { throw Error.notAbsolute }
        let url = URL(fileURLWithPath: path)
        var attrs: [FileAttributeKey: Any]
        do {
            attrs = try FileManager.default.attributesOfItem(atPath: url.path)
        } catch {
            throw Error.ioError(error.localizedDescription)
        }
        if (attrs[.type] as? FileAttributeType) == .typeSymbolicLink {
            throw Error.symlink
        }
        // Defensive: lstat via URL resourceValues catches symlink even if attributesOfItem follows it
        let resVals = try? url.resourceValues(forKeys: [.isSymbolicLinkKey])
        if resVals?.isSymbolicLink == true { throw Error.symlink }

        guard (attrs[.type] as? FileAttributeType) == .typeRegular else {
            throw Error.notRegularFile
        }
        if let count = attrs[.referenceCount] as? Int, count > 1 {
            throw Error.hardlink
        }
        let resolved = url.resolvingSymlinksInPath().standardizedFileURL
        let inRoot = allowedRoots.contains { root in
            resolved.path == root.path || resolved.path.hasPrefix(root.path + "/")
        }
        if !inRoot { throw Error.notInAllowedRoot }
    }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd macos-menubar && swift test --filter PathValidatorTests`
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/PathValidator.swift \
        macos-menubar/Tests/SandboxTests/PathValidatorTests.swift
git commit -m "feat(menubar): PathValidator with symlink/hardlink/root checks"
```

### Task B4: SessionStore (TDD)

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/SessionStore.swift`
- Create: `macos-menubar/Tests/SandboxTests/SessionStoreTests.swift`

- [ ] **Step 1: Write failing tests**

Create `macos-menubar/Tests/SandboxTests/SessionStoreTests.swift`:

```swift
import XCTest
@testable import FileSandboxMenuBar

final class SessionStoreTests: XCTestCase {
    var tmp: URL!
    var fileURL: URL!

    override func setUpWithError() throws {
        tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("ss-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        fileURL = tmp.appendingPathComponent("sessions.json")
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tmp)
    }

    func testEmptyOnFreshFile() throws {
        let store = try SessionStore(fileURL: fileURL)
        XCTAssertTrue(store.list().isEmpty)
    }

    func testAddListRemoveRoundTrip() throws {
        let store = try SessionStore(fileURL: fileURL)
        let s = SessionRecord(
            id: UUID(),
            sourceFilePath: "/tmp/a.pdf",
            createdAt: Date(timeIntervalSince1970: 1_700_000_000),
            lastActiveAt: Date(timeIntervalSince1970: 1_700_000_100),
            status: .running,
            networkEnabled: false
        )
        try store.upsert(s)
        XCTAssertEqual(store.list().count, 1)

        let store2 = try SessionStore(fileURL: fileURL)
        XCTAssertEqual(store2.list().first?.id, s.id)

        try store2.remove(id: s.id)
        XCTAssertTrue(store2.list().isEmpty)
    }

    func testRecoversFromCorruptedFile() throws {
        try "not json".write(to: fileURL, atomically: true, encoding: .utf8)
        let store = try SessionStore(fileURL: fileURL)
        XCTAssertTrue(store.list().isEmpty, "should recover by treating as empty")
    }

    func testStatusValuesEncodeAsStrings() throws {
        let store = try SessionStore(fileURL: fileURL)
        let s = SessionRecord(
            id: UUID(), sourceFilePath: "/x", createdAt: .init(), lastActiveAt: .init(),
            status: .discarded, networkEnabled: true)
        try store.upsert(s)
        let raw = try String(contentsOf: fileURL, encoding: .utf8)
        XCTAssertTrue(raw.contains("\"discarded\""))
    }
}
```

- [ ] **Step 2: Run, verify failure**

Run: `cd macos-menubar && swift test --filter SessionStoreTests`
Expected: compile failure (`SessionStore`, `SessionRecord` undefined).

- [ ] **Step 3: Implement**

Create `macos-menubar/Sources/App/Sandbox/SessionStore.swift`:

```swift
import Foundation

public enum SessionStatus: String, Codable {
    case running, discarded, error
}

public struct SessionRecord: Codable, Equatable, Identifiable {
    public let id: UUID
    public var sourceFilePath: String
    public var createdAt: Date
    public var lastActiveAt: Date
    public var status: SessionStatus
    public var networkEnabled: Bool

    public init(
        id: UUID, sourceFilePath: String, createdAt: Date, lastActiveAt: Date,
        status: SessionStatus, networkEnabled: Bool
    ) {
        self.id = id
        self.sourceFilePath = sourceFilePath
        self.createdAt = createdAt
        self.lastActiveAt = lastActiveAt
        self.status = status
        self.networkEnabled = networkEnabled
    }
}

public final class SessionStore {
    private let fileURL: URL
    private var records: [UUID: SessionRecord] = [:]
    private let queue = DispatchQueue(label: "filesandbox.sessionstore")

    public init(fileURL: URL) throws {
        self.fileURL = fileURL
        try load()
    }

    public func list() -> [SessionRecord] {
        queue.sync { Array(records.values).sorted { $0.createdAt > $1.createdAt } }
    }

    public func upsert(_ r: SessionRecord) throws {
        try queue.sync {
            records[r.id] = r
            try persist()
        }
    }

    public func remove(id: UUID) throws {
        try queue.sync {
            records.removeValue(forKey: id)
            try persist()
        }
    }

    private func load() throws {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        let data = (try? Data(contentsOf: fileURL)) ?? Data()
        if data.isEmpty { return }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        if let arr = try? decoder.decode([SessionRecord].self, from: data) {
            records = Dictionary(uniqueKeysWithValues: arr.map { ($0.id, $0) })
        } else {
            // Corrupted file — start empty, log via stderr
            FileHandle.standardError.write(Data("SessionStore: corrupted file, starting empty\n".utf8))
            records = [:]
        }
    }

    private func persist() throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let arr = Array(records.values).sorted { $0.createdAt < $1.createdAt }
        let data = try encoder.encode(arr)
        let dir = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try data.write(to: fileURL, options: .atomic)
    }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd macos-menubar && swift test --filter SessionStoreTests`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/SessionStore.swift \
        macos-menubar/Tests/SandboxTests/SessionStoreTests.swift
git commit -m "feat(menubar): SessionStore with JSON persistence + corruption recovery"
```

### Task B5: SandboxConfig (TDD)

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/SandboxConfig.swift`
- Create: `macos-menubar/Tests/SandboxTests/SandboxConfigTests.swift`

- [ ] **Step 1: Write failing tests**

Create `macos-menubar/Tests/SandboxTests/SandboxConfigTests.swift`:

```swift
import XCTest
@testable import FileSandboxMenuBar

final class SandboxConfigTests: XCTestCase {
    var tmp: URL!
    var url: URL!

    override func setUpWithError() throws {
        tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("sc-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        url = tmp.appendingPathComponent("sandbox-config.json")
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tmp)
    }

    func testDefaults() throws {
        let c = try SandboxConfig.load(from: url)
        XCTAssertFalse(c.enabled)
        XCTAssertEqual(c.idleTimeoutMinutes, 30)
        XCTAssertFalse(c.networkDefault)
        XCTAssertEqual(c.vmMemoryMB, 4096)
        XCTAssertEqual(c.vmCpuCount, 2)
    }

    func testSaveAndLoad() throws {
        var c = try SandboxConfig.load(from: url)
        c.enabled = true
        c.idleTimeoutMinutes = 60
        c.vmMemoryMB = 8192
        try c.save(to: url)

        let c2 = try SandboxConfig.load(from: url)
        XCTAssertTrue(c2.enabled)
        XCTAssertEqual(c2.idleTimeoutMinutes, 60)
        XCTAssertEqual(c2.vmMemoryMB, 8192)
    }

    func testRangeClamping() throws {
        var c = try SandboxConfig.load(from: url)
        c.idleTimeoutMinutes = 4   // below min
        XCTAssertEqual(c.idleTimeoutMinutes, 5)
        c.idleTimeoutMinutes = 999 // above max
        XCTAssertEqual(c.idleTimeoutMinutes, 240)
        c.vmMemoryMB = 100
        XCTAssertEqual(c.vmMemoryMB, 1024)
        c.vmMemoryMB = 99_999
        XCTAssertEqual(c.vmMemoryMB, 16384)
        c.vmCpuCount = 0
        XCTAssertEqual(c.vmCpuCount, 1)
        c.vmCpuCount = 99
        XCTAssertEqual(c.vmCpuCount, 8)
    }
}
```

- [ ] **Step 2: Run, verify failure**

Run: `cd macos-menubar && swift test --filter SandboxConfigTests`

- [ ] **Step 3: Implement**

Create `macos-menubar/Sources/App/Sandbox/SandboxConfig.swift`:

```swift
import Foundation

public struct SandboxConfig: Codable, Equatable {
    public var enabled: Bool
    public var idleTimeoutMinutes: Int { didSet { idleTimeoutMinutes = clamp(idleTimeoutMinutes, 5, 240) } }
    public var networkDefault: Bool
    public var vmMemoryMB: Int { didSet { vmMemoryMB = clamp(vmMemoryMB, 1024, 16384) } }
    public var vmCpuCount: Int { didSet { vmCpuCount = clamp(vmCpuCount, 1, 8) } }

    public init(
        enabled: Bool = false,
        idleTimeoutMinutes: Int = 30,
        networkDefault: Bool = false,
        vmMemoryMB: Int = 4096,
        vmCpuCount: Int = 2
    ) {
        self.enabled = enabled
        self.idleTimeoutMinutes = clamp(idleTimeoutMinutes, 5, 240)
        self.networkDefault = networkDefault
        self.vmMemoryMB = clamp(vmMemoryMB, 1024, 16384)
        self.vmCpuCount = clamp(vmCpuCount, 1, 8)
    }

    public static func load(from url: URL) throws -> SandboxConfig {
        guard FileManager.default.fileExists(atPath: url.path) else { return .init() }
        let data = try Data(contentsOf: url)
        if data.isEmpty { return .init() }
        return (try? JSONDecoder().decode(SandboxConfig.self, from: data)) ?? .init()
    }

    public func save(to url: URL) throws {
        let dir = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let enc = JSONEncoder()
        enc.outputFormatting = [.prettyPrinted, .sortedKeys]
        try enc.encode(self).write(to: url, options: .atomic)
    }
}

private func clamp(_ v: Int, _ lo: Int, _ hi: Int) -> Int { max(lo, min(hi, v)) }
```

- [ ] **Step 4: Run, verify pass**

Run: `cd macos-menubar && swift test --filter SandboxConfigTests`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/SandboxConfig.swift \
        macos-menubar/Tests/SandboxTests/SandboxConfigTests.swift
git commit -m "feat(menubar): SandboxConfig JSON store with range clamping"
```

### Task B6: VMConfig (TDD)

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/VMConfig.swift`
- Create: `macos-menubar/Tests/SandboxTests/VMConfigTests.swift`

- [ ] **Step 1: Write failing tests**

Create `macos-menubar/Tests/SandboxTests/VMConfigTests.swift`:

```swift
import XCTest
import Virtualization
@testable import FileSandboxMenuBar

final class VMConfigTests: XCTestCase {
    var tmp: URL!

    override func setUpWithError() throws {
        tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("vm-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        // Fake base.img/kernel/initrd files (existence is enough for config building)
        for name in ["base.img", "vmlinuz", "initrd.img"] {
            try Data().write(to: tmp.appendingPathComponent(name))
        }
        try FileManager.default.createDirectory(
            at: tmp.appendingPathComponent("in"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: tmp.appendingPathComponent("out"), withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: tmp) }

    private func make(network: Bool) throws -> VZVirtualMachineConfiguration {
        let inputs = VMConfig.Inputs(
            kernelURL: tmp.appendingPathComponent("vmlinuz"),
            initrdURL: tmp.appendingPathComponent("initrd.img"),
            baseImageURL: tmp.appendingPathComponent("base.img"),
            inDirURL: tmp.appendingPathComponent("in"),
            outDirURL: tmp.appendingPathComponent("out"),
            memoryMB: 4096,
            cpuCount: 2,
            networkEnabled: network
        )
        return try VMConfig.build(inputs)
    }

    func testBaseDiskAttachedReadOnly() throws {
        let cfg = try make(network: false)
        let attachment = cfg.storageDevices.first?.attachment as? VZDiskImageStorageDeviceAttachment
        XCTAssertNotNil(attachment)
        XCTAssertTrue(attachment!.isReadOnly)
    }

    func testNoNetworkByDefault() throws {
        let cfg = try make(network: false)
        XCTAssertTrue(cfg.networkDevices.isEmpty)
    }

    func testNetworkAttachedWhenEnabled() throws {
        let cfg = try make(network: true)
        XCTAssertEqual(cfg.networkDevices.count, 1)
    }

    func testGraphicsIsTwoDimensional() throws {
        let cfg = try make(network: false)
        XCTAssertTrue(cfg.graphicsDevices.first is VZVirtioGraphicsDeviceConfiguration)
    }

    func testInVirtioFsReadOnly() throws {
        let cfg = try make(network: false)
        let inShare = cfg.directorySharingDevices.compactMap {
            $0 as? VZVirtioFileSystemDeviceConfiguration
        }.first { $0.tag == "fs_in" }
        XCTAssertNotNil(inShare)
        let dir = (inShare?.share as? VZSingleDirectoryShare)?.directory
        XCTAssertEqual(dir?.isReadOnly, true)
    }

    func testOutVirtioFsReadWrite() throws {
        let cfg = try make(network: false)
        let outShare = cfg.directorySharingDevices.compactMap {
            $0 as? VZVirtioFileSystemDeviceConfiguration
        }.first { $0.tag == "fs_out" }
        let dir = (outShare?.share as? VZSingleDirectoryShare)?.directory
        XCTAssertEqual(dir?.isReadOnly, false)
    }
}
```

- [ ] **Step 2: Run, verify failure**

Run: `cd macos-menubar && swift test --filter VMConfigTests`

- [ ] **Step 3: Implement**

Create `macos-menubar/Sources/App/Sandbox/VMConfig.swift`:

```swift
import Foundation
import Virtualization

public enum VMConfig {
    public struct Inputs {
        public let kernelURL: URL
        public let initrdURL: URL
        public let baseImageURL: URL
        public let inDirURL: URL
        public let outDirURL: URL
        public let memoryMB: Int
        public let cpuCount: Int
        public let networkEnabled: Bool

        public init(
            kernelURL: URL, initrdURL: URL, baseImageURL: URL,
            inDirURL: URL, outDirURL: URL,
            memoryMB: Int, cpuCount: Int, networkEnabled: Bool
        ) {
            self.kernelURL = kernelURL
            self.initrdURL = initrdURL
            self.baseImageURL = baseImageURL
            self.inDirURL = inDirURL
            self.outDirURL = outDirURL
            self.memoryMB = memoryMB
            self.cpuCount = cpuCount
            self.networkEnabled = networkEnabled
        }
    }

    public enum Error: Swift.Error {
        case missingArtifact(URL)
        case attachmentFailed(String)
    }

    public static let kernelCmdline =
        "console=hvc0 root=/dev/vda ro quiet "
        + "lockdown=confidentiality init_on_alloc=1 init_on_free=1 "
        + "randomize_kstack_offset=1 module.sig_enforce=1 oops=panic"

    public static func build(_ inp: Inputs) throws -> VZVirtualMachineConfiguration {
        for u in [inp.kernelURL, inp.initrdURL, inp.baseImageURL] {
            guard FileManager.default.fileExists(atPath: u.path) else {
                throw Error.missingArtifact(u)
            }
        }

        let cfg = VZVirtualMachineConfiguration()
        cfg.cpuCount = inp.cpuCount
        cfg.memorySize = UInt64(inp.memoryMB) * 1024 * 1024

        // Boot
        let boot = VZLinuxBootLoader(kernelURL: inp.kernelURL)
        boot.initialRamdiskURL = inp.initrdURL
        boot.commandLine = kernelCmdline
        cfg.bootLoader = boot

        // Disk (RO)
        let attachment: VZDiskImageStorageDeviceAttachment
        do {
            attachment = try VZDiskImageStorageDeviceAttachment(
                url: inp.baseImageURL, readOnly: true)
        } catch {
            throw Error.attachmentFailed(error.localizedDescription)
        }
        cfg.storageDevices = [VZVirtioBlockDeviceConfiguration(attachment: attachment)]

        // Console (serial via stdio is not used in GUI mode; keep none for now)

        // Input
        cfg.keyboards = [VZUSBKeyboardConfiguration()]
        cfg.pointingDevices = [VZUSBScreenCoordinatePointingDeviceConfiguration()]

        // Graphics — virtio 2D
        let g = VZVirtioGraphicsDeviceConfiguration()
        g.scanouts = [VZVirtioGraphicsScanoutConfiguration(widthInPixels: 1280, heightInPixels: 800)]
        cfg.graphicsDevices = [g]

        // virtio-fs in/out
        let inDevice = VZVirtioFileSystemDeviceConfiguration(tag: "fs_in")
        inDevice.share = VZSingleDirectoryShare(
            directory: VZSharedDirectory(url: inp.inDirURL, readOnly: true))
        let outDevice = VZVirtioFileSystemDeviceConfiguration(tag: "fs_out")
        outDevice.share = VZSingleDirectoryShare(
            directory: VZSharedDirectory(url: inp.outDirURL, readOnly: false))
        cfg.directorySharingDevices = [inDevice, outDevice]

        // Network — only when explicitly enabled
        if inp.networkEnabled {
            let nat = VZNATNetworkDeviceAttachment()
            let net = VZVirtioNetworkDeviceConfiguration()
            net.attachment = nat
            cfg.networkDevices = [net]
        } else {
            cfg.networkDevices = []
        }

        try cfg.validate()
        return cfg
    }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd macos-menubar && swift test --filter VMConfigTests`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/VMConfig.swift \
        macos-menubar/Tests/SandboxTests/VMConfigTests.swift
git commit -m "feat(menubar): VMConfig builder for VZVirtualMachineConfiguration"
```

### Task B7: IdleMonitor (TDD)

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/IdleMonitor.swift`
- Create: `macos-menubar/Tests/SandboxTests/IdleMonitorTests.swift`

- [ ] **Step 1: Write failing tests**

Create `macos-menubar/Tests/SandboxTests/IdleMonitorTests.swift`:

```swift
import XCTest
@testable import FileSandboxMenuBar

final class IdleMonitorTests: XCTestCase {
    final class FakeClock: Clock {
        var now: Date = Date(timeIntervalSince1970: 0)
        func currentDate() -> Date { now }
    }

    func testTimeoutFiresAfterIdle() {
        let clock = FakeClock()
        var firedSoft = false, firedHard = false
        let m = IdleMonitor(
            idleTimeoutMinutes: 30, hardCapMinutes: 240, clock: clock,
            onSoftWarning: { firedSoft = true },
            onTimeout: { firedHard = true }
        )
        m.start()
        clock.now = clock.now.addingTimeInterval(25 * 60)
        m.tick()
        XCTAssertFalse(firedSoft); XCTAssertFalse(firedHard)
        clock.now = clock.now.addingTimeInterval(60) // 26 min
        m.tick()
        XCTAssertTrue(firedSoft, "soft warning at T-5 = 25 min in")
        clock.now = clock.now.addingTimeInterval(5 * 60) // 31 min
        m.tick()
        XCTAssertTrue(firedHard)
    }

    func testActivityResetsTimer() {
        let clock = FakeClock()
        var firedHard = false
        let m = IdleMonitor(
            idleTimeoutMinutes: 30, hardCapMinutes: 240, clock: clock,
            onSoftWarning: {}, onTimeout: { firedHard = true })
        m.start()
        clock.now = clock.now.addingTimeInterval(20 * 60)
        m.recordActivity()
        clock.now = clock.now.addingTimeInterval(20 * 60) // 40 min total, but only 20 since reset
        m.tick()
        XCTAssertFalse(firedHard)
    }

    func testHardCapFiresEvenWithActivity() {
        let clock = FakeClock()
        var firedHard = false
        let m = IdleMonitor(
            idleTimeoutMinutes: 30, hardCapMinutes: 60, clock: clock,
            onSoftWarning: {}, onTimeout: { firedHard = true })
        m.start()
        for _ in 0..<10 {
            clock.now = clock.now.addingTimeInterval(7 * 60)
            m.recordActivity()
            m.tick()
        }
        XCTAssertTrue(firedHard, "hard cap should fire at >60min regardless of activity")
    }
}
```

- [ ] **Step 2: Run, verify failure**

- [ ] **Step 3: Implement**

Create `macos-menubar/Sources/App/Sandbox/IdleMonitor.swift`:

```swift
import Foundation
import AppKit

public protocol Clock { func currentDate() -> Date }

public struct SystemClock: Clock {
    public init() {}
    public func currentDate() -> Date { Date() }
}

public final class IdleMonitor {
    private let idleTimeout: TimeInterval
    private let hardCap: TimeInterval
    private let clock: Clock
    private let onSoftWarning: () -> Void
    private let onTimeout: () -> Void

    private var startedAt: Date?
    private var lastActivityAt: Date?
    private var softFired = false
    private var hardFired = false

    public init(
        idleTimeoutMinutes: Int,
        hardCapMinutes: Int,
        clock: Clock = SystemClock(),
        onSoftWarning: @escaping () -> Void,
        onTimeout: @escaping () -> Void
    ) {
        self.idleTimeout = TimeInterval(idleTimeoutMinutes) * 60
        self.hardCap = TimeInterval(hardCapMinutes) * 60
        self.clock = clock
        self.onSoftWarning = onSoftWarning
        self.onTimeout = onTimeout
    }

    public func start() {
        startedAt = clock.currentDate()
        lastActivityAt = startedAt
    }

    public func recordActivity() {
        lastActivityAt = clock.currentDate()
        softFired = false
    }

    public func tick() {
        guard !hardFired, let started = startedAt, let active = lastActivityAt else { return }
        let now = clock.currentDate()
        if now.timeIntervalSince(started) >= hardCap {
            hardFired = true; onTimeout(); return
        }
        let idle = now.timeIntervalSince(active)
        if !softFired, idle >= max(0, idleTimeout - 5 * 60) {
            softFired = true; onSoftWarning()
        }
        if idle >= idleTimeout {
            hardFired = true; onTimeout()
        }
    }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd macos-menubar && swift test --filter IdleMonitorTests`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/IdleMonitor.swift \
        macos-menubar/Tests/SandboxTests/IdleMonitorTests.swift
git commit -m "feat(menubar): IdleMonitor with soft warn + hard cap + activity reset"
```

### Task B8: SandboxManager (no TDD — orchestrator that integrates real VMs)

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/SandboxManager.swift`

- [ ] **Step 1: Implement**

Create `macos-menubar/Sources/App/Sandbox/SandboxManager.swift`:

```swift
import Foundation
import Virtualization
import AppKit

@MainActor
public final class SandboxManager: ObservableObject {
    public static let shared = SandboxManager()

    @Published public private(set) var sessions: [SessionRecord] = []

    private var vms: [UUID: VZVirtualMachine] = [:]
    private var windows: [UUID: SandboxWindowController] = [:]
    private var monitors: [UUID: IdleMonitor] = [:]
    private var tickers: [UUID: Timer] = [:]

    private let store: SessionStore
    private let validator: PathValidator
    private let config: () -> SandboxConfig
    private let baseDir: URL
    private let imagePaths: ImagePaths

    public struct ImagePaths {
        public let kernelURL: URL
        public let initrdURL: URL
        public let baseImageURL: URL
        public init(kernelURL: URL, initrdURL: URL, baseImageURL: URL) {
            self.kernelURL = kernelURL
            self.initrdURL = initrdURL
            self.baseImageURL = baseImageURL
        }
    }

    public enum Failure: Error {
        case disabled
        case validation(PathValidator.Error)
        case configure(VMConfig.Error)
        case start(String)
        case unknownSession
    }

    private init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("FileSandbox", isDirectory: true)
        self.baseDir = support
        let sessionsURL = support.appendingPathComponent("sandbox-sessions.json")
        self.store = (try? SessionStore(fileURL: sessionsURL))
            ?? { fatalError("SessionStore init failed") }()
        self.config = { (try? SandboxConfig.load(from: support.appendingPathComponent("sandbox-config.json"))) ?? .init() }
        // Allowed roots come from daemon config — read via SettingsStore at call time.
        let watchRoot = (UserDefaults.standard.string(forKey: "watchPath")).map { URL(fileURLWithPath: $0) }
        let quarantineRoot = (UserDefaults.standard.string(forKey: "quarantinePath")).map { URL(fileURLWithPath: $0) }
        self.validator = PathValidator(allowedRoots: [watchRoot, quarantineRoot].compactMap { $0 })
        self.imagePaths = ImagePaths(
            kernelURL: support.appendingPathComponent("sandbox-base/current/vmlinuz"),
            initrdURL: support.appendingPathComponent("sandbox-base/current/initrd.img"),
            baseImageURL: support.appendingPathComponent("sandbox-base/current/base.img")
        )
        self.sessions = store.list()
    }

    public func openSession(filePath: String) throws -> UUID {
        let cfg = config()
        guard cfg.enabled else { throw Failure.disabled }
        do { try validator.validate(path: filePath) }
        catch let e as PathValidator.Error { throw Failure.validation(e) }

        let id = UUID()
        let sessionDir = baseDir
            .appendingPathComponent("sandbox-sessions", isDirectory: true)
            .appendingPathComponent(id.uuidString, isDirectory: true)
        let inDir = sessionDir.appendingPathComponent("in", isDirectory: true)
        let outDir = sessionDir.appendingPathComponent("out", isDirectory: true)
        try FileManager.default.createDirectory(at: inDir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

        // Hardlink, fall back to copy
        let dest = inDir.appendingPathComponent(URL(fileURLWithPath: filePath).lastPathComponent)
        do {
            try FileManager.default.linkItem(atPath: filePath, toPath: dest.path)
        } catch {
            try FileManager.default.copyItem(atPath: filePath, toPath: dest.path)
        }
        // Marker file naming the input, read by sandbox-launch.service inside guest
        try dest.lastPathComponent.write(
            to: inDir.appendingPathComponent(".fileToOpen"),
            atomically: true, encoding: .utf8)

        let vmCfg: VZVirtualMachineConfiguration
        do {
            vmCfg = try VMConfig.build(.init(
                kernelURL: imagePaths.kernelURL,
                initrdURL: imagePaths.initrdURL,
                baseImageURL: imagePaths.baseImageURL,
                inDirURL: inDir,
                outDirURL: outDir,
                memoryMB: cfg.vmMemoryMB,
                cpuCount: cfg.vmCpuCount,
                networkEnabled: cfg.networkDefault
            ))
        } catch let e as VMConfig.Error {
            try? FileManager.default.removeItem(at: sessionDir)
            throw Failure.configure(e)
        }

        let vm = VZVirtualMachine(configuration: vmCfg, queue: .main)
        vms[id] = vm

        // Persist + UI
        let record = SessionRecord(
            id: id, sourceFilePath: filePath,
            createdAt: Date(), lastActiveAt: Date(),
            status: .running, networkEnabled: cfg.networkDefault)
        try store.upsert(record)
        sessions = store.list()

        let win = SandboxWindowController(sessionID: id, vm: vm, outDir: outDir,
                                          onDiscard: { [weak self] in self?.discardSession(id: id) },
                                          onExport: { [weak self] name in self?.exportFromSession(id: id, fileName: name) })
        win.showWindow(nil)
        windows[id] = win

        let monitor = IdleMonitor(
            idleTimeoutMinutes: cfg.idleTimeoutMinutes,
            hardCapMinutes: 240,
            onSoftWarning: { [weak self] in self?.notifySoftWarning(id: id) },
            onTimeout: { [weak self] in self?.discardSession(id: id) })
        monitor.start()
        monitors[id] = monitor
        tickers[id] = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { _ in
            Task { @MainActor in monitor.tick() }
        }

        // Boot
        vm.start { [weak self] result in
            if case .failure(let err) = result {
                Task { @MainActor in self?.markError(id: id, error: err) }
            }
        }

        // Sleep-discard
        NotificationCenter.default.addObserver(
            forName: NSWorkspace.willSleepNotification,
            object: nil, queue: .main
        ) { [weak self] _ in self?.discardSession(id: id) }

        return id
    }

    public func discardSession(id: UUID) {
        guard let vm = vms[id] else { return }
        vm.stop { _ in }
        vms.removeValue(forKey: id)
        windows[id]?.close()
        windows.removeValue(forKey: id)
        tickers[id]?.invalidate()
        tickers.removeValue(forKey: id)
        monitors.removeValue(forKey: id)
        let dir = baseDir.appendingPathComponent("sandbox-sessions").appendingPathComponent(id.uuidString)
        try? FileManager.default.removeItem(at: dir)
        if var rec = store.list().first(where: { $0.id == id }) {
            rec.status = .discarded
            try? store.upsert(rec)
        }
        sessions = store.list()
    }

    public func exportFromSession(id: UUID, fileName: String) {
        let outDir = baseDir.appendingPathComponent("sandbox-sessions")
            .appendingPathComponent(id.uuidString).appendingPathComponent("out")
        let src = outDir.appendingPathComponent(fileName)
        guard let watch = UserDefaults.standard.string(forKey: "watchPath") else { return }
        let dst = URL(fileURLWithPath: watch).appendingPathComponent(fileName)
        try? FileManager.default.moveItem(at: src, to: dst)
        // Daemon's watcher will pick it up and trigger the scan pipeline.
    }

    public func listSessions() -> [SessionRecord] { sessions }

    private func markError(id: UUID, error: Error) {
        if var rec = store.list().first(where: { $0.id == id }) {
            rec.status = .error
            try? store.upsert(rec)
        }
        sessions = store.list()
    }

    private func notifySoftWarning(id: UUID) {
        let n = NSUserNotification()
        n.title = "Sandbox session idle"
        n.informativeText = "Session will discard in 5 minutes unless you interact."
        NSUserNotificationCenter.default.deliver(n)
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cd macos-menubar && swift build`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/SandboxManager.swift
git commit -m "feat(menubar): SandboxManager orchestrates VZVirtualMachine sessions"
```

### Task B9: SandboxWindowController

**Files:**
- Create: `macos-menubar/Sources/App/Sandbox/SandboxWindowController.swift`

- [ ] **Step 1: Implement**

Create `macos-menubar/Sources/App/Sandbox/SandboxWindowController.swift`:

```swift
import AppKit
import Virtualization

@MainActor
public final class SandboxWindowController: NSWindowController {
    private let sessionID: UUID
    private weak var vm: VZVirtualMachine?
    private let outDir: URL
    private let onDiscard: () -> Void
    private let onExport: (String) -> Void

    private var fsSource: DispatchSourceFileSystemObject?
    private var dirFD: Int32 = -1
    private var bannerLabel: NSTextField?

    public init(
        sessionID: UUID, vm: VZVirtualMachine, outDir: URL,
        onDiscard: @escaping () -> Void, onExport: @escaping (String) -> Void
    ) {
        self.sessionID = sessionID
        self.vm = vm
        self.outDir = outDir
        self.onDiscard = onDiscard
        self.onExport = onExport
        let win = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered, defer: false)
        win.title = "Sandbox \(sessionID.uuidString.prefix(8))"
        super.init(window: win)
        buildContent()
        watchOutDir()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func buildContent() {
        guard let win = window, let vm = vm else { return }
        let view = VZVirtualMachineView()
        view.virtualMachine = vm
        view.translatesAutoresizingMaskIntoConstraints = false

        let banner = NSTextField(labelWithString: "")
        banner.translatesAutoresizingMaskIntoConstraints = false
        banner.isHidden = true
        bannerLabel = banner

        let toolbar = NSStackView()
        toolbar.orientation = .horizontal
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        toolbar.addArrangedSubview(NSButton(title: "Discard", target: self, action: #selector(discard)))
        toolbar.addArrangedSubview(NSButton(title: "Export…", target: self, action: #selector(exportPicker)))
        toolbar.addArrangedSubview(banner)

        let stack = NSStackView(views: [toolbar, view])
        stack.orientation = .vertical
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.spacing = 6

        win.contentView = stack
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: win.contentView!.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: win.contentView!.trailingAnchor),
            stack.topAnchor.constraint(equalTo: win.contentView!.topAnchor),
            stack.bottomAnchor.constraint(equalTo: win.contentView!.bottomAnchor),
        ])
    }

    private func watchOutDir() {
        dirFD = open(outDir.path, O_EVTONLY)
        guard dirFD >= 0 else { return }
        fsSource = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: dirFD, eventMask: .write, queue: .main)
        fsSource?.setEventHandler { [weak self] in self?.refreshBanner() }
        fsSource?.setCancelHandler { [weak self] in
            if let fd = self?.dirFD, fd >= 0 { close(fd) }
        }
        fsSource?.resume()
    }

    private func refreshBanner() {
        let count = (try? FileManager.default.contentsOfDirectory(atPath: outDir.path).count) ?? 0
        if count > 0 {
            bannerLabel?.isHidden = false
            bannerLabel?.stringValue = "\(count) file(s) ready to export"
        } else {
            bannerLabel?.isHidden = true
        }
    }

    @objc private func discard() { onDiscard() }

    @objc private func exportPicker() {
        let alert = NSAlert()
        alert.messageText = "Export from sandbox"
        let files = (try? FileManager.default.contentsOfDirectory(atPath: outDir.path)) ?? []
        guard !files.isEmpty else {
            alert.informativeText = "No files to export."
            alert.runModal(); return
        }
        let popup = NSPopUpButton(frame: NSRect(x: 0, y: 0, width: 300, height: 24))
        popup.addItems(withTitles: files)
        alert.accessoryView = popup
        alert.addButton(withTitle: "Export")
        alert.addButton(withTitle: "Cancel")
        if alert.runModal() == .alertFirstButtonReturn, let name = popup.titleOfSelectedItem {
            onExport(name)
        }
    }

    deinit {
        fsSource?.cancel()
    }
}
```

- [ ] **Step 2: Build to verify**

Run: `cd macos-menubar && swift build`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Sandbox/SandboxWindowController.swift
git commit -m "feat(menubar): SandboxWindowController hosts VZ view + export banner"
```

---

## Phase C — UI Wiring

### Task C1: Replace SandboxStore.swift with local-only version

**Files:**
- Modify: `macos-menubar/Sources/App/SandboxStore.swift` (rewrite)

- [ ] **Step 1: Replace contents**

Rewrite `macos-menubar/Sources/App/SandboxStore.swift` so it proxies the new `SandboxManager` and exposes `@Published` state for SwiftUI:

```swift
import Foundation
import Combine

@MainActor
final class SandboxStore: ObservableObject {
    @Published var sessions: [SessionRecord] = []
    @Published var enabled: Bool = false
    private var cancellables = Set<AnyCancellable>()
    private let manager: SandboxManager

    init(manager: SandboxManager = .shared) {
        self.manager = manager
        manager.$sessions
            .receive(on: DispatchQueue.main)
            .assign(to: \.sessions, on: self)
            .store(in: &cancellables)
        refreshEnabled()
    }

    var canOpenSandbox: Bool { enabled }

    func refreshEnabled() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("FileSandbox")
        let cfg = (try? SandboxConfig.load(from: support.appendingPathComponent("sandbox-config.json"))) ?? .init()
        enabled = cfg.enabled
    }

    func openSandbox(filePath: String) {
        do { _ = try manager.openSession(filePath: filePath) }
        catch { NSLog("openSandbox failed: \(error)") }
    }

    func discard(id: UUID) { manager.discardSession(id: id) }
}
```

- [ ] **Step 2: Build**

Run: `bash macos-menubar/build.sh`
Expected: build succeeds. Some call-sites in `JobsTabView` / `SandboxTabView` may break — fix them in C2/C3.

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/SandboxStore.swift
git commit -m "refactor(menubar): SandboxStore wraps SandboxManager (no REST)"
```

### Task C2: Drop sandbox* fields from SettingsStore

**Files:**
- Modify: `macos-menubar/Sources/App/SettingsStore.swift`

- [ ] **Step 1: Remove sandbox fields**

Open `macos-menubar/Sources/App/SettingsStore.swift`. Delete every `@Published` property whose name starts with `sandbox`. Delete those keys from the `Codable` payload (`encode`/`decode`) and from the POST body to `/api/config`.

- [ ] **Step 2: Build**

Run: `bash macos-menubar/build.sh`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/SettingsStore.swift
git commit -m "refactor(menubar): drop sandbox* fields from SettingsStore (moved to SandboxConfig)"
```

### Task C3: SettingsTabView — bind Sandbox section to SandboxConfig

**Files:**
- Modify: `macos-menubar/Sources/App/Tabs/SettingsTabView.swift`

- [ ] **Step 1: Replace bindings**

Find the section in `SettingsTabView.swift` that previously read sandbox keys from `SettingsStore`. Replace each binding with a binding to a `@StateObject` wrapper around `SandboxConfig`:

```swift
@StateObject private var sandboxCfg = SandboxConfigVM()

@MainActor final class SandboxConfigVM: ObservableObject {
    @Published var cfg: SandboxConfig
    private let url: URL
    init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("FileSandbox")
        self.url = support.appendingPathComponent("sandbox-config.json")
        self.cfg = (try? SandboxConfig.load(from: url)) ?? .init()
    }
    func save() { try? cfg.save(to: url) }
}
```

UI rows bind to `$sandboxCfg.cfg.enabled`, `$sandboxCfg.cfg.idleTimeoutMinutes`, etc. Wrap the whole section in `.onChange(of: sandboxCfg.cfg) { _ in sandboxCfg.save() }`.

- [ ] **Step 2: Build**

Run: `bash macos-menubar/build.sh`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/SettingsTabView.swift
git commit -m "feat(menubar): SettingsTab Sandbox section binds to SandboxConfig"
```

### Task C4: SandboxTabView — list sessions + discard

**Files:**
- Modify: `macos-menubar/Sources/App/Tabs/SandboxTabView.swift`

- [ ] **Step 1: Replace UI**

Replace the body so it pulls `sessions` from `SandboxStore` and offers a "Discard" button per row. Show empty state if list is empty. Show a top-level button "+ New session" only when `store.canOpenSandbox` is true (calls `NSOpenPanel` to pick a file from the watch dir, then `store.openSandbox(filePath:)`).

- [ ] **Step 2: Build**

Run: `bash macos-menubar/build.sh`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/SandboxTabView.swift
git commit -m "feat(menubar): SandboxTab lists active sessions, discard, + new"
```

### Task C5: JobsTabView — "Open in sandbox" wired to SandboxManager

**Files:**
- Modify: `macos-menubar/Sources/App/Tabs/JobsTabView.swift`

- [ ] **Step 1: Replace action**

Find the "Open in sandbox" button/menu item. Replace its action with `sandboxStore.openSandbox(filePath: job.quarantinedPath)`. Disable when `!sandboxStore.canOpenSandbox`.

- [ ] **Step 2: Build**

Run: `bash macos-menubar/build.sh`

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/Tabs/JobsTabView.swift
git commit -m "feat(menubar): JobsTab Open-in-sandbox calls SandboxManager"
```

---

## Phase D — Sandbox Image (mkosi)

### Task D1: mkosi.conf — declarative spec

**Files:**
- Create: `sandbox-image/mkosi.conf`

- [ ] **Step 1: Write config**

Create `sandbox-image/mkosi.conf`:

```ini
[Distribution]
Distribution=debian
Release=bookworm
Mirror=https://snapshot.debian.org/archive/debian/20260501T000000Z/
Architecture=arm64

[Output]
Format=directory
Output=build/rootfs
ImageId=filesandbox-sandbox
ImageVersion=0.1.0

[Content]
Bootable=yes
Packages=
    linux-image-arm64
    systemd
    systemd-sysv
    init
    udev
    dbus
    apparmor
    apparmor-utils
    xfce4
    xfce4-session
    xinit
    evince
    eog
    mpv
    libreoffice
    file
    binutils
    ca-certificates
    fonts-dejavu
    virtio-fs-utils

RemoveFiles=
    /usr/share/doc
    /usr/share/man
    /usr/share/info
    /var/cache/apt/archives/*.deb
```

- [ ] **Step 2: Commit**

```bash
git add sandbox-image/mkosi.conf
git commit -m "feat(image): mkosi.conf — Debian bookworm slim + XFCE viewers"
```

### Task D2: skeleton — fstab, kernel cmdline, AppArmor, systemd unit

**Files:**
- Create: `sandbox-image/mkosi.skeleton/etc/fstab`
- Create: `sandbox-image/mkosi.skeleton/etc/default/grub.d/99-sandbox.cfg`
- Create: `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.evince`
- Create: `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.eog`
- Create: `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.mpv`
- Create: `sandbox-image/mkosi.skeleton/etc/apparmor.d/local/usr.bin.libreoffice`
- Create: `sandbox-image/mkosi.skeleton/etc/sandbox-init`
- Create: `sandbox-image/mkosi.skeleton/etc/systemd/system/sandbox-launch.service`

- [ ] **Step 1: fstab (tmpfs everywhere writable)**

`sandbox-image/mkosi.skeleton/etc/fstab`:

```
proc           /proc        proc    defaults                            0 0
sysfs          /sys         sysfs   defaults                            0 0
tmpfs          /tmp         tmpfs   defaults,nosuid,nodev               0 0
tmpfs          /var/log     tmpfs   defaults,nosuid,nodev               0 0
tmpfs          /var/tmp     tmpfs   defaults,nosuid,nodev               0 0
tmpfs          /run         tmpfs   defaults,nosuid,nodev               0 0
tmpfs          /home/sandbox tmpfs  defaults,nosuid,nodev,uid=1000,gid=1000 0 0
tmpfs          /srv         tmpfs   defaults,nosuid,nodev               0 0
fs_in          /mnt/in      virtiofs ro,nosuid,nodev,noexec             0 0
fs_out         /mnt/out     virtiofs rw,nosuid,nodev,noexec             0 0
```

- [ ] **Step 2: Kernel cmdline override**

`sandbox-image/mkosi.skeleton/etc/default/grub.d/99-sandbox.cfg`:

```bash
GRUB_CMDLINE_LINUX="lockdown=confidentiality init_on_alloc=1 init_on_free=1 randomize_kstack_offset=1 module.sig_enforce=1 oops=panic"
```

- [ ] **Step 3: AppArmor local profiles**

For each viewer, drop a deny-ish profile addition. Example `usr.bin.evince`:

```
# /etc/apparmor.d/local/usr.bin.evince
deny network,
deny /proc/*/mem r,
deny /sys/** w,
```

Mirror similar files for `eog`, `mpv`, `libreoffice`.

- [ ] **Step 4: sandbox-init script**

`sandbox-image/mkosi.skeleton/etc/sandbox-init` (mode 0755):

```bash
#!/bin/sh
set -eu
TARGET="/mnt/in/$(cat /mnt/in/.fileToOpen)"
case "$TARGET" in
    *.pdf)                  exec sudo -u sandbox evince "$TARGET" ;;
    *.png|*.jpg|*.jpeg|*.gif) exec sudo -u sandbox eog "$TARGET" ;;
    *.mp4|*.mkv|*.mov|*.webm) exec sudo -u sandbox mpv "$TARGET" ;;
    *.doc|*.docx|*.xls|*.xlsx|*.ppt|*.pptx|*.odt) exec sudo -u sandbox libreoffice "$TARGET" ;;
    *)                      exec xterm -e "file '$TARGET'; strings '$TARGET' | head -200; read x" ;;
esac
```

- [ ] **Step 5: systemd unit**

`sandbox-image/mkosi.skeleton/etc/systemd/system/sandbox-launch.service`:

```ini
[Unit]
Description=Open file in sandbox viewer
After=graphical.target
Requires=graphical.target

[Service]
Type=oneshot
ExecStart=/etc/sandbox-init
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=graphical.target
```

- [ ] **Step 6: Commit**

```bash
git add sandbox-image/mkosi.skeleton/
git commit -m "feat(image): skeleton — fstab, cmdline, AppArmor, sandbox-launch"
```

### Task D3: postinst + finalize scripts

**Files:**
- Create: `sandbox-image/mkosi.postinst`
- Create: `sandbox-image/mkosi.finalize`

- [ ] **Step 1: postinst**

`sandbox-image/mkosi.postinst` (mode 0755):

```bash
#!/bin/sh
set -eux
ROOT="$1"
chroot "$ROOT" /bin/sh -eux <<'INNER'
useradd -m -u 1000 -s /bin/bash sandbox || true
passwd -l sandbox
passwd -l root
systemctl enable sandbox-launch.service
# Strip persistence + bloat
apt-get purge -y --autoremove cups-* exim4-* unattended-upgrades || true
apt-get clean
rm -rf /var/cache/apt/archives /var/lib/apt/lists /var/cache/man /var/cache/debconf
# Disable any service that touches network or persists state
for s in systemd-resolved systemd-networkd systemd-timesyncd cron rsyslog; do
    systemctl disable "$s" 2>/dev/null || true
done
INNER
```

- [ ] **Step 2: finalize**

`sandbox-image/mkosi.finalize` (mode 0755):

```bash
#!/bin/sh
set -eux
ROOT="$1"
mkdir -p sandbox-image/build
mksquashfs "$ROOT" sandbox-image/build/base.img \
    -comp zstd -Xcompression-level 19 -no-progress -noappend
# Pull kernel + initrd out for VZLinuxBootLoader
cp "$ROOT"/boot/vmlinuz-* sandbox-image/build/vmlinuz
cp "$ROOT"/boot/initrd.img-* sandbox-image/build/initrd.img
sha256sum sandbox-image/build/base.img sandbox-image/build/vmlinuz \
          sandbox-image/build/initrd.img > sandbox-image/build/SHA256SUMS
```

- [ ] **Step 3: Commit**

```bash
git add sandbox-image/mkosi.postinst sandbox-image/mkosi.finalize
git commit -m "feat(image): postinst (lockdown, prune) + finalize (squashfs + checksums)"
```

### Task D4: yarn sandbox:build wrapper

**Files:**
- Modify: `package.json`
- Create: `scripts/sandbox-build.sh`

- [ ] **Step 1: Wrapper script**

Create `scripts/sandbox-build.sh` (mode 0755):

```bash
#!/usr/bin/env bash
set -euo pipefail

# Builds sandbox-image/build/{base.img,vmlinuz,initrd.img} via mkosi running in Lima.
# Requires: Lima (lima --version) + mkosi available inside the VM.

if ! command -v limactl >/dev/null; then
    echo "limactl not found. Install with: brew install lima" >&2
    exit 1
fi

VM_NAME="filesandbox-mkosi"
if ! limactl list --quiet | grep -q "^${VM_NAME}$"; then
    echo "Creating Lima VM '${VM_NAME}' (Debian bookworm)..."
    limactl start --name="${VM_NAME}" template://debian-12 --tty=false
fi

limactl shell "${VM_NAME}" -- bash -lc '
    sudo apt-get update -q
    sudo apt-get install -y -q mkosi systemd-container squashfs-tools debootstrap
'

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
limactl shell "${VM_NAME}" -- bash -lc "cd '${REPO_ROOT}/sandbox-image' && sudo mkosi --force"

echo "Artifacts:"
ls -la "${REPO_ROOT}/sandbox-image/build/"
cat "${REPO_ROOT}/sandbox-image/build/SHA256SUMS"
```

- [ ] **Step 2: package.json**

Add to `scripts` in `package.json`:

```json
"sandbox:build": "bash scripts/sandbox-build.sh"
```

- [ ] **Step 3: Commit**

```bash
git add scripts/sandbox-build.sh package.json
git commit -m "feat(build): yarn sandbox:build runs mkosi inside Lima"
```

### Task D5: README for the image

**Files:**
- Create: `sandbox-image/README.md`

- [ ] **Step 1: Write README**

Document: prerequisites (Lima), the build command, the output artifacts, how the menubar app picks them up, the SHA-256 verification, and the manual smoke checklist.

- [ ] **Step 2: Commit**

```bash
git add sandbox-image/README.md
git commit -m "docs(image): README for sandbox-image build flow"
```

---

## Phase E — Verification & Rollout

### Task E1: Wire image artifacts into menubar app

**Files:**
- Modify: `macos-menubar/build.sh`

- [ ] **Step 1: Add a step to copy artifacts into Application Support after build**

If `sandbox-image/build/base.img` exists, the build script should copy `base.img`, `vmlinuz`, `initrd.img` to `~/Library/Application Support/FileSandbox/sandbox-base/current/` and verify SHA-256 against `SHA256SUMS`. If artifacts missing, print a warning but do not fail (sandbox just stays disabled).

- [ ] **Step 2: Build + verify**

Run: `yarn sandbox:build` (skip if already built), then `bash macos-menubar/build.sh`.

Verify: `~/Library/Application Support/FileSandbox/sandbox-base/current/base.img` exists.

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/build.sh
git commit -m "build(menubar): stage sandbox-base artifacts after build"
```

### Task E2: Manual smoke test

- [ ] **Step 1: Enable sandbox in Settings → Sandbox tab**
- [ ] **Step 2: Drop a benign PDF in watch folder**
- [ ] **Step 3: From Jobs tab, click "Open in sandbox"**
- [ ] **Step 4: VM window opens within 5 s, PDF visible**
- [ ] **Step 5: In guest, `ip a` shows only `lo`**
- [ ] **Step 6: Save a file from guest to `/mnt/out/`, click Export, file appears in watch dir, gets scanned**
- [ ] **Step 7: Click Discard, window closes, session dir gone**
- [ ] **Step 8: Open 2 sandboxes simultaneously, discard one, verify other survives**
- [ ] **Step 9: Open sandbox, sleep host, on wake confirm session was discarded**

### Task E3: Final review + merge prep

- [ ] **Step 1: Run all tests**

Run: `yarn test && cd macos-menubar && swift test`
Expected: all pass.

- [ ] **Step 2: Type/lint check**

Run: `yarn tsc --noEmit`

- [ ] **Step 3: Push branch + open PR**

```bash
git push -u origin feat/linux-sandbox
gh pr create --title "feat: Linux sandbox (Apple Virt + Hardened Debian)" \
             --body "$(cat docs/superpowers/specs/2026-05-05-linux-sandbox-design.md | head -40)"
```

---

## Self-Review Checklist (used while writing this plan)

- Spec coverage: every component listed in the spec maps to at least one task. Daemon cleanup → A1–A3. Swift Sandbox module → B3–B9. Image → D1–D5. Build pipeline → D4 + E1. Threat-model defenses → encoded in tests (PathValidator, VMConfig RO+no-net) and in image config (cmdline, AppArmor). Testing strategy → unit tests in B3–B7, manual smoke in E2.
- Placeholders: none. Where a UI binding has many fields, I named the SwiftUI surface and the binding pattern explicitly rather than expanding all rows verbatim — these are routine SwiftUI rewrites against an interface (`SandboxConfig`) defined fully in B5.
- Type consistency: `SessionRecord`, `SandboxConfig`, `VMConfig.Inputs`, `IdleMonitor` callbacks, `PathValidator.Error` are all referenced consistently across tasks.

## Execution Handoff

Plan complete. Next: choose execution mode.

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, isolated worktree, fast iteration.
2. **Inline Execution** — execute tasks in this session via `superpowers:executing-plans`, batch with checkpoints.

Reply with **1** or **2**.
