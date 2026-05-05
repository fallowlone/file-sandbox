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
