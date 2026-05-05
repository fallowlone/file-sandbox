import Foundation

private let bytesPerMiB = 1_048_576

struct DaemonConfig: Codable {
    var vtApiKey: String
    var apiToken: String?
    var watchPath: String
    var quarantinePath: String
    var databasePath: String
    var httpPort: String
    var httpHost: String
    var watchRecursive: Bool?
    var maxScanBytes: Int?
    var maxConcurrentScans: Int?
    var useSeparateVtProcess: Bool?
    var inconclusiveRetentionDays: Int?
    var configEncryptedAtRest: Bool?
    var watcherMode: String?
    var vtEnabled: Bool?
    var pompelmiEnabled: Bool?
    var pompelmiSocketPath: String?
    var pompelmiFailureMode: String?
}

enum DaemonLocalStorage {
    private static let projectPathKey = "filesandboxDaemonProjectPath"
    private static let nodeBinKey = "filesandboxDaemonNodeBin"

    static var projectPath: String {
        get { UserDefaults.standard.string(forKey: projectPathKey) ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: projectPathKey) }
    }
    static var nodeBin: String {
        get { UserDefaults.standard.string(forKey: nodeBinKey) ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: nodeBinKey) }
    }
}

class SettingsStore: ObservableObject {
    @Published var vtApiKey: String = ""
    @Published var watchPath: String = ""
    @Published var quarantinePath: String = ""
    @Published var databasePath: String = ""
    @Published var httpPort: String = ""
    @Published var httpHost: String = ""
    /// Same value the daemon expects in Authorization: Bearer (optional).
    @Published var apiAuthToken: String = ""
    @Published var watchRecursive: Bool = true
    /// VirusTotal upload limit, shown/edited as MiB in the UI; saved as bytes.
    @Published var maxScanMegabytes: Int = 400
    @Published var maxConcurrentScans: Int = 2
    @Published var useSeparateVtProcess: Bool = false
    @Published var inconclusiveRetentionDays: Int = 0
    @Published var watcherMode: WatcherMode = .active
    @Published var vtEnabled: Bool = true
    @Published var pompelmiEnabled: Bool = true
    @Published var pompelmiSocketPath: String = "/tmp/clamd.sock"
    @Published var pompelmiFailureMode: String = "bypass"

    // Stored locally — no daemon required (used when daemon is offline)
    @Published var daemonProjectPath: String = ""
    @Published var daemonNodeBin: String = ""

    @Published var isLoading = false
    @Published var isSaving = false
    @Published var saveResult: String? = nil
    @Published var loadError: String? = nil

    private let port: String

    init() {
        self.port = ProcessInfo.processInfo.environment["FILE_SANDBOX_PORT"] ?? "3847"
        self.apiAuthToken = ClientAuthStorage.token
        self.daemonProjectPath = DaemonLocalStorage.projectPath
        self.daemonNodeBin = DaemonLocalStorage.nodeBin
    }

    func saveDaemonLocal() {
        DaemonLocalStorage.projectPath = daemonProjectPath
        DaemonLocalStorage.nodeBin = daemonNodeBin
    }

    private func authorizedConfigRequest(url: URL) -> URLRequest {
        var request = URLRequest(url: url)
        let t = ClientAuthStorage.token
        if !t.isEmpty {
            request.setValue("Bearer \(t)", forHTTPHeaderField: "Authorization")
        }
        return request
    }

    private static func megabytesFromBytes(_ bytes: Int) -> Int {
        max(1, bytes / bytesPerMiB)
    }

    func fetch() {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/config") else { return }
        isLoading = true
        loadError = nil
        let request = authorizedConfigRequest(url: url)
        URLSession.shared.dataTask(with: request) { [weak self] data, _, error in
            DispatchQueue.main.async {
                guard let self else { return }
                self.isLoading = false
                guard error == nil, let data,
                      let decoded = try? JSONDecoder().decode(DaemonConfig.self, from: data)
                else {
                    self.loadError = error?.localizedDescription ?? "Failed to load config"
                    return
                }
                self.vtApiKey = decoded.vtApiKey
                self.watchPath = decoded.watchPath
                self.quarantinePath = decoded.quarantinePath
                self.databasePath = decoded.databasePath
                self.httpPort = decoded.httpPort
                self.httpHost = decoded.httpHost
                self.watchRecursive = decoded.watchRecursive ?? true
                if let b = decoded.maxScanBytes, b > 0 {
                    self.maxScanMegabytes = Self.megabytesFromBytes(b)
                } else {
                    self.maxScanMegabytes = 400
                }
                if let m = decoded.maxConcurrentScans, m >= 1 {
                    self.maxConcurrentScans = m
                } else {
                    self.maxConcurrentScans = 2
                }
                self.useSeparateVtProcess = decoded.useSeparateVtProcess ?? false
                self.inconclusiveRetentionDays = decoded.inconclusiveRetentionDays ?? 0
                self.watcherMode = WatcherMode(rawValue: decoded.watcherMode ?? "active") ?? .active
                self.vtEnabled = decoded.vtEnabled ?? true
                self.pompelmiEnabled = decoded.pompelmiEnabled ?? true
                self.pompelmiSocketPath = decoded.pompelmiSocketPath ?? "/tmp/clamd.sock"
                self.pompelmiFailureMode = decoded.pompelmiFailureMode ?? "bypass"
            }
        }.resume()
    }

    func save() {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/config") else { return }
        isSaving = true
        saveResult = nil

        ClientAuthStorage.token = apiAuthToken

        let scanBytes = maxScanMegabytes * bytesPerMiB

        var body: [String: Any] = [
            "watchPath": watchPath,
            "quarantinePath": quarantinePath,
            "databasePath": databasePath,
            "httpPort": httpPort,
            "httpHost": httpHost,
            "watchRecursive": watchRecursive,
            "useSeparateVtProcess": useSeparateVtProcess,
            "apiToken": apiAuthToken,
            "maxScanBytes": scanBytes,
            "maxConcurrentScans": maxConcurrentScans,
            "inconclusiveRetentionDays": inconclusiveRetentionDays,
            "watcherMode": watcherMode.rawValue,
            "vtEnabled": vtEnabled,
            "pompelmiEnabled": pompelmiEnabled,
            "pompelmiSocketPath": pompelmiSocketPath,
            "pompelmiFailureMode": pompelmiFailureMode,
        ]
        if !vtApiKey.isEmpty { body["vtApiKey"] = vtApiKey }

        var request = authorizedConfigRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)

        URLSession.shared.dataTask(with: request) { [weak self] _, _, error in
            DispatchQueue.main.async {
                guard let self else { return }
                self.isSaving = false
                if let error {
                    self.saveResult = "err:\(error.localizedDescription)"
                    return
                }
                self.saveResult = "ok"
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                    if self.saveResult == "ok" { self.saveResult = nil }
                }
            }
        }.resume()
    }
}
