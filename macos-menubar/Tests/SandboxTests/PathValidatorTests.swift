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
        let messy = tmp.path + "/watch/./sub/c.txt"
        XCTAssertNoThrow(try validator.validate(path: messy))
    }
}
