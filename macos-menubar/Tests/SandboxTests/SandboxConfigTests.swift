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
