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
