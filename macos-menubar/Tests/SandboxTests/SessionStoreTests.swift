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
