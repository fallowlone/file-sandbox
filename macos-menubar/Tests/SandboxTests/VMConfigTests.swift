import XCTest
import Virtualization
@testable import FileSandboxMenuBar

final class VMConfigTests: XCTestCase {
    var tmp: URL!

    override func setUpWithError() throws {
        tmp = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("vm-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        for name in ["base.img", "vmlinuz", "initrd.img"] {
            try Data().write(to: tmp.appendingPathComponent(name))
        }
        try FileManager.default.createDirectory(
            at: tmp.appendingPathComponent("in"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: tmp.appendingPathComponent("out"), withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: tmp) }

    private func make(network: Bool) throws -> VZVirtualMachineConfiguration {
        let inputs = VMConfig.Inputs(
            kernelURL: tmp.appendingPathComponent("vmlinuz"),
            initrdURL: tmp.appendingPathComponent("initrd.img"),
            baseImageURL: tmp.appendingPathComponent("base.img"),
            inDirURL: tmp.appendingPathComponent("in"),
            outDirURL: tmp.appendingPathComponent("out"),
            memoryMB: 4096,
            cpuCount: 2,
            networkEnabled: network
        )
        return try VMConfig.build(inputs)
    }

    func testBaseDiskAttachedReadOnly() throws {
        let cfg = try make(network: false)
        let attachment = cfg.storageDevices.first?.attachment as? VZDiskImageStorageDeviceAttachment
        XCTAssertNotNil(attachment)
        XCTAssertTrue(attachment!.isReadOnly)
    }

    func testNoNetworkByDefault() throws {
        let cfg = try make(network: false)
        XCTAssertTrue(cfg.networkDevices.isEmpty)
    }

    func testNetworkAttachedWhenEnabled() throws {
        let cfg = try make(network: true)
        XCTAssertEqual(cfg.networkDevices.count, 1)
    }

    func testGraphicsIsTwoDimensional() throws {
        let cfg = try make(network: false)
        XCTAssertTrue(cfg.graphicsDevices.first is VZVirtioGraphicsDeviceConfiguration)
    }

    func testInVirtioFsReadOnly() throws {
        let cfg = try make(network: false)
        let inShare = cfg.directorySharingDevices.compactMap {
            $0 as? VZVirtioFileSystemDeviceConfiguration
        }.first { $0.tag == "fs_in" }
        XCTAssertNotNil(inShare)
        let dir = (inShare?.share as? VZSingleDirectoryShare)?.directory
        XCTAssertEqual(dir?.isReadOnly, true)
    }

    func testOutVirtioFsReadWrite() throws {
        let cfg = try make(network: false)
        let outShare = cfg.directorySharingDevices.compactMap {
            $0 as? VZVirtioFileSystemDeviceConfiguration
        }.first { $0.tag == "fs_out" }
        let dir = (outShare?.share as? VZSingleDirectoryShare)?.directory
        XCTAssertEqual(dir?.isReadOnly, false)
    }
}
