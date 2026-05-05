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
    private var sleepObservers: [UUID: NSObjectProtocol] = [:]

    private let store: SessionStore
    private let baseDir: URL
    private let imagePaths: ImagePaths
    private var validator: PathValidator
    private let configURL: URL
    private var lastWatchPath: String = ""

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
        case notConfigured
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
        self.configURL = support.appendingPathComponent("sandbox-config.json")
        self.validator = PathValidator(allowedRoots: [])
        self.imagePaths = ImagePaths(
            kernelURL: support.appendingPathComponent("sandbox-base/current/vmlinuz"),
            initrdURL: support.appendingPathComponent("sandbox-base/current/initrd.img"),
            baseImageURL: support.appendingPathComponent("sandbox-base/current/base.img")
        )
        self.sessions = store.list()
    }

    /// Inject the allowed roots once paths are known (typically from SettingsStore /api/config).
    public func configure(watchPath: String, quarantinePath: String) {
        var roots: [URL] = []
        if !watchPath.isEmpty { roots.append(URL(fileURLWithPath: watchPath)) }
        if !quarantinePath.isEmpty { roots.append(URL(fileURLWithPath: quarantinePath)) }
        self.validator = PathValidator(allowedRoots: roots)
        self.lastWatchPath = watchPath
    }

    public func currentConfig() -> SandboxConfig {
        (try? SandboxConfig.load(from: configURL)) ?? .init()
    }

    public func openSession(filePath: String) throws -> UUID {
        let cfg = currentConfig()
        guard cfg.enabled else { throw Failure.disabled }
        guard !lastWatchPath.isEmpty else { throw Failure.notConfigured }
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

        let dest = inDir.appendingPathComponent(URL(fileURLWithPath: filePath).lastPathComponent)
        do {
            try FileManager.default.linkItem(atPath: filePath, toPath: dest.path)
        } catch {
            try FileManager.default.copyItem(atPath: filePath, toPath: dest.path)
        }
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

        let record = SessionRecord(
            id: id, sourceFilePath: filePath,
            createdAt: Date(), lastActiveAt: Date(),
            status: .running, networkEnabled: cfg.networkDefault)
        try store.upsert(record)
        sessions = store.list()

        let win = SandboxWindowController(
            sessionID: id, vm: vm, outDir: outDir,
            onDiscard: { [weak self] in self?.discardSession(id: id) },
            onExport: { [weak self] name in self?.exportFromSession(id: id, fileName: name) }
        )
        win.showWindow(nil)
        windows[id] = win

        let monitor = IdleMonitor(
            idleTimeoutMinutes: cfg.idleTimeoutMinutes,
            hardCapMinutes: 240,
            onSoftWarning: { [weak self] in self?.notifySoftWarning(id: id) },
            onTimeout: { [weak self] in
                Task { @MainActor in self?.discardSession(id: id) }
            }
        )
        monitor.start()
        monitors[id] = monitor
        tickers[id] = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { _ in
            Task { @MainActor in monitor.tick() }
        }

        let observer = NotificationCenter.default.addObserver(
            forName: NSWorkspace.willSleepNotification,
            object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.discardSession(id: id) }
        }
        sleepObservers[id] = observer

        vm.start { [weak self] result in
            if case .failure(let err) = result {
                Task { @MainActor in self?.markError(id: id, error: err) }
            }
        }

        return id
    }

    public func discardSession(id: UUID) {
        if let vm = vms[id] {
            vm.stop { _ in }
        }
        vms.removeValue(forKey: id)
        windows[id]?.close()
        windows.removeValue(forKey: id)
        tickers[id]?.invalidate()
        tickers.removeValue(forKey: id)
        monitors.removeValue(forKey: id)
        if let obs = sleepObservers.removeValue(forKey: id) {
            NotificationCenter.default.removeObserver(obs)
        }
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
        guard !lastWatchPath.isEmpty else { return }
        let dst = URL(fileURLWithPath: lastWatchPath).appendingPathComponent(fileName)
        try? FileManager.default.moveItem(at: src, to: dst)
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
