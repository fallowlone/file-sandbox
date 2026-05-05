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
