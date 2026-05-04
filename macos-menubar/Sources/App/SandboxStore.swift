import Foundation
import Combine

struct SandboxSession: Codable, Identifiable, Equatable {
    let id: String
    let vmName: String
    let sourceJobId: String?
    let sourceFilePath: String
    let sessionDir: String
    let pid: Int?
    let networkEnabled: Bool
    let status: String
    let detail: String?
    let createdAt: String
    let lastActiveAt: String
    let exitedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, vmName, sourceJobId, sourceFilePath, sessionDir
        case pid, networkEnabled, status, detail, createdAt, lastActiveAt, exitedAt
    }
}

class SandboxStore: ObservableObject {
    @Published var sessions: [SandboxSession] = []
    @Published var loadError: String? = nil
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

    private let port: String

    init() {
        self.port = ProcessInfo.processInfo.environment["FILE_SANDBOX_PORT"] ?? "3847"
    }

    private func authorizedRequest(url: URL) -> URLRequest {
        var request = URLRequest(url: url)
        let t = ClientAuthStorage.token
        if !t.isEmpty {
            request.setValue("Bearer \(t)", forHTTPHeaderField: "Authorization")
        }
        return request
    }

    func fetch() {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/sandbox/sessions?limit=50") else { return }
        URLSession.shared.dataTask(with: authorizedRequest(url: url)) { [weak self] data, _, error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error {
                    self.loadError = error.localizedDescription
                    return
                }
                guard let data,
                      let decoded = try? JSONDecoder().decode([String: [SandboxSession]].self, from: data),
                      let arr = decoded["sessions"]
                else {
                    self.loadError = "decode failed"
                    return
                }
                self.sessions = arr
                self.loadError = nil
            }
        }.resume()
    }

    func create(filePath: String, sourceJobId: String?, network: Bool, completion: @escaping (Bool) -> Void) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/sandbox/sessions") else {
            completion(false)
            return
        }
        var req = authorizedRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        var body: [String: Any] = ["filePath": filePath, "network": network]
        if let sourceJobId {
            body["sourceJobId"] = sourceJobId
        }
        req.httpBody = try? JSONSerialization.data(withJSONObject: body)
        URLSession.shared.dataTask(with: req) { [weak self] _, _, error in
            DispatchQueue.main.async {
                completion(error == nil)
                self?.fetch()
            }
        }.resume()
    }

    func discard(_ id: String) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/sandbox/sessions/\(id)") else { return }
        var req = authorizedRequest(url: url)
        req.httpMethod = "DELETE"
        URLSession.shared.dataTask(with: req) { [weak self] _, _, _ in
            DispatchQueue.main.async {
                self?.fetch()
            }
        }.resume()
    }
}
