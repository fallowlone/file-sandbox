import Foundation
import Darwin
import SwiftUI

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

    /// Localized display name for views (`StatusChip`, picker labels).
    /// Routes through the catalog `mode.<rawValue>` keys.
    var displayKey: LocalizedStringKey {
        L.mode(self)
    }

    var symbolName: String {
        switch self {
        case .active: return "play.circle.fill"
        case .scanPaused: return "pause.circle.fill"
        case .monitoringDisabled: return "eye.slash.fill"
        }
    }
}

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

struct JobsResponse: Codable {
    let jobs: [SandboxJob]
    let paused: Bool?
    let mode: String?
}

enum ClientAuthStorage {
    private static let key = "filesandboxClientAPIToken"
    static var token: String {
        get { UserDefaults.standard.string(forKey: key) ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: key) }
    }
}

class JobStore: ObservableObject {
    @Published var jobs: [SandboxJob] = []
    @Published var isConnected = false
    @Published var mode: WatcherMode = .active
    @Published var lastActionError: String? = nil
    @Published var daemonLaunchError: String? = nil

    var isPaused: Bool { mode != .active }

    private var timer: Timer?
    private let apiURL: URL
    private let port: String
    private var daemonProcess: Process?
    private let decoder = JSONDecoder()
    private let decodeQueue = DispatchQueue(label: "filesandbox.decode", qos: .utility)
    private let portCheckQueue = DispatchQueue(label: "filesandbox.portcheck", qos: .utility)
    private var currentPollInterval: TimeInterval = 0
    private var lastETag: String?

    private var targetPollInterval: TimeInterval {
        let hasActive = jobs.contains { $0.status == "scanning" || $0.status == "in_quarantine" }
        return hasActive ? 2.0 : 10.0
    }

    var isDaemonRunning: Bool { daemonProcess?.isRunning == true }

    init() {
        self.port = ProcessInfo.processInfo.environment["FILE_SANDBOX_PORT"] ?? "3847"
        self.apiURL = URL(string: "http://127.0.0.1:\(self.port)/api/jobs")!
        startPolling()
    }

    func startDaemon(projectPath: String, nodeBin: String) {
        daemonLaunchError = nil
        if isConnected { return }
        let portNum = UInt16(port) ?? 3847
        portCheckQueue.async { [weak self] in
            let inUse = Self.isPortInUse(port: portNum)
            DispatchQueue.main.async {
                guard let self else { return }
                if inUse {
                    self.fetch()
                    return
                }
                self.spawnDaemonProcess(projectPath: projectPath, nodeBin: nodeBin)
            }
        }
    }

    private func spawnDaemonProcess(projectPath: String, nodeBin: String) {
        let proc = Process()
        let logPath = "\(projectPath)/logs/daemon-ui.log"
        let nodeCmd = nodeBin.isEmpty ? "node" : nodeBin
        let shellCmd = "\(nodeCmd) src/index.ts >> \"\(logPath)\" 2>&1"

        // GUI apps get a stripped PATH — launch via login shell so nvm/Homebrew/etc are loaded
        proc.executableURL = URL(fileURLWithPath: "/bin/zsh")
        proc.arguments = ["-l", "-c", shellCmd]
        proc.currentDirectoryURL = URL(fileURLWithPath: projectPath)

        // Inherit environment so PATH/nvm etc. are available
        var env = ProcessInfo.processInfo.environment
        env["NODE_NO_WARNINGS"] = "1"
        proc.environment = env

        // Ensure logs/ dir exists
        try? FileManager.default.createDirectory(
            atPath: "\(projectPath)/logs",
            withIntermediateDirectories: true
        )

        proc.terminationHandler = { [weak self] p in
            DispatchQueue.main.async {
                guard let self else { return }
                if p.terminationStatus != 0 {
                    self.daemonLaunchError = "Exit \(p.terminationStatus) — see \(logPath)"
                }
                self.daemonProcess = nil
            }
        }

        do {
            try proc.run()
            daemonProcess = proc
        } catch {
            daemonLaunchError = "Failed to start: \(error.localizedDescription)"
        }
    }

    private static func isPortInUse(port: UInt16) -> Bool {
        let sock = socket(AF_INET, SOCK_STREAM, 0)
        guard sock >= 0 else { return false }
        defer { close(sock) }
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let result = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(sock, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        return result == 0
    }

    func stopDaemon() {
        daemonProcess?.terminate()
        daemonProcess = nil
    }

    private func authorizedRequest(url: URL) -> URLRequest {
        var request = URLRequest(url: url)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        let t = ClientAuthStorage.token
        if !t.isEmpty {
            request.setValue("Bearer \(t)", forHTTPHeaderField: "Authorization")
        }
        return request
    }

    var activeThreats: [SandboxJob] {
        jobs.filter { $0.vt_verdict == "infected" && $0.status == "quarantine_kept" }
    }

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

    var threatCount: Int { activeThreats.count }

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

    func startPolling() {
        fetch()
        rescheduleTimer(interval: targetPollInterval)
    }

    private func rescheduleTimer(interval: TimeInterval) {
        guard interval != currentPollInterval else { return }
        timer?.invalidate()
        currentPollInterval = interval
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            self?.fetch()
        }
    }

    private func performAction(url: URL, method: String) {
        var request = authorizedRequest(url: url)
        request.httpMethod = method
        URLSession.shared.dataTask(with: request) { [weak self] _, response, error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error {
                    self.lastActionError = "Network error: \(error.localizedDescription)"
                    return
                }
                if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                    self.lastActionError = "Request failed (\(http.statusCode))"
                    return
                }
                self.lastActionError = nil
                self.fetch()
            }
        }.resume()
    }

    func clearJobs() {
        performAction(url: apiURL, method: "DELETE")
    }

    func cancelJob(_ id: String) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/jobs/\(id)/cancel") else { return }
        performAction(url: url, method: "POST")
    }

    func deleteFile(_ id: String) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/jobs/\(id)/quarantine") else { return }
        performAction(url: url, method: "DELETE")
    }

    func restoreFile(_ id: String) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/jobs/\(id)/restore") else { return }
        performAction(url: url, method: "POST")
    }

    func setMode(_ next: WatcherMode) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/watcher/mode") else { return }
        var req = authorizedRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: ["mode": next.rawValue])
        URLSession.shared.dataTask(with: req) { [weak self] _, _, error in
            DispatchQueue.main.async {
                if error == nil { self?.mode = next }
            }
        }.resume()
    }

    func fetch() {
        var request = authorizedRequest(url: apiURL)
        if let tag = lastETag {
            request.setValue(tag, forHTTPHeaderField: "If-None-Match")
        }
        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            guard let self else { return }
            guard error == nil, let http = response as? HTTPURLResponse else {
                DispatchQueue.main.async { self.isConnected = false }
                return
            }
            if http.statusCode == 304 {
                DispatchQueue.main.async {
                    self.isConnected = true
                    self.lastActionError = nil
                    self.rescheduleTimer(interval: self.targetPollInterval)
                }
                return
            }
            guard http.statusCode == 200, let data else {
                DispatchQueue.main.async { self.isConnected = false }
                return
            }
            let etag = http.value(forHTTPHeaderField: "ETag")
            self.decodeQueue.async {
                let decoded = try? self.decoder.decode(JobsResponse.self, from: data)
                DispatchQueue.main.async {
                    guard let decoded else {
                        self.isConnected = false
                        return
                    }
                    self.isConnected = true
                    self.lastETag = etag
                    if self.jobs != decoded.jobs {
                        self.jobs = decoded.jobs
                    }
                    self.mode = decoded.mode.flatMap(WatcherMode.init(rawValue:)) ?? (decoded.paused == true ? .scanPaused : .active)
                    self.lastActionError = nil
                    self.rescheduleTimer(interval: self.targetPollInterval)
                }
            }
        }.resume()
    }

    deinit {
        timer?.invalidate()
        daemonProcess?.terminate()
    }
}
