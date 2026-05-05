import Foundation
import Virtualization

public enum VMConfig {
    public struct Inputs {
        public let kernelURL: URL
        public let initrdURL: URL
        public let baseImageURL: URL
        public let inDirURL: URL
        public let outDirURL: URL
        public let memoryMB: Int
        public let cpuCount: Int
        public let networkEnabled: Bool

        public init(
            kernelURL: URL, initrdURL: URL, baseImageURL: URL,
            inDirURL: URL, outDirURL: URL,
            memoryMB: Int, cpuCount: Int, networkEnabled: Bool
        ) {
            self.kernelURL = kernelURL
            self.initrdURL = initrdURL
            self.baseImageURL = baseImageURL
            self.inDirURL = inDirURL
            self.outDirURL = outDirURL
            self.memoryMB = memoryMB
            self.cpuCount = cpuCount
            self.networkEnabled = networkEnabled
        }
    }

    public enum Error: Swift.Error {
        case missingArtifact(URL)
        case attachmentFailed(String)
    }

    public static let kernelCmdline =
        "console=hvc0 root=/dev/vda ro quiet "
        + "lockdown=confidentiality init_on_alloc=1 init_on_free=1 "
        + "randomize_kstack_offset=1 module.sig_enforce=1 oops=panic"

    public static func build(_ inp: Inputs) throws -> VZVirtualMachineConfiguration {
        for u in [inp.kernelURL, inp.initrdURL, inp.baseImageURL] {
            guard FileManager.default.fileExists(atPath: u.path) else {
                throw Error.missingArtifact(u)
            }
        }

        let cfg = VZVirtualMachineConfiguration()
        cfg.cpuCount = inp.cpuCount
        cfg.memorySize = UInt64(inp.memoryMB) * 1024 * 1024

        let boot = VZLinuxBootLoader(kernelURL: inp.kernelURL)
        boot.initialRamdiskURL = inp.initrdURL
        boot.commandLine = kernelCmdline
        cfg.bootLoader = boot

        let attachment: VZDiskImageStorageDeviceAttachment
        do {
            attachment = try VZDiskImageStorageDeviceAttachment(
                url: inp.baseImageURL, readOnly: true)
        } catch {
            throw Error.attachmentFailed(error.localizedDescription)
        }
        cfg.storageDevices = [VZVirtioBlockDeviceConfiguration(attachment: attachment)]

        cfg.keyboards = [VZUSBKeyboardConfiguration()]
        cfg.pointingDevices = [VZUSBScreenCoordinatePointingDeviceConfiguration()]

        let g = VZVirtioGraphicsDeviceConfiguration()
        g.scanouts = [VZVirtioGraphicsScanoutConfiguration(widthInPixels: 1280, heightInPixels: 800)]
        cfg.graphicsDevices = [g]

        let inDevice = VZVirtioFileSystemDeviceConfiguration(tag: "fs_in")
        inDevice.share = VZSingleDirectoryShare(
            directory: VZSharedDirectory(url: inp.inDirURL, readOnly: true))
        let outDevice = VZVirtioFileSystemDeviceConfiguration(tag: "fs_out")
        outDevice.share = VZSingleDirectoryShare(
            directory: VZSharedDirectory(url: inp.outDirURL, readOnly: false))
        cfg.directorySharingDevices = [inDevice, outDevice]

        if inp.networkEnabled {
            let nat = VZNATNetworkDeviceAttachment()
            let net = VZVirtioNetworkDeviceConfiguration()
            net.attachment = nat
            cfg.networkDevices = [net]
        } else {
            cfg.networkDevices = []
        }

        return cfg
    }
}
