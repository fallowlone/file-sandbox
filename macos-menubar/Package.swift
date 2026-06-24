// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "FileSandboxMenuBar",
    defaultLocalization: "en",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "FileSandboxMenuBar",
            path: "Sources/App",
            resources: [
                .process("Resources")
            ]
        ),
        .testTarget(
            name: "SandboxTests",
            dependencies: ["FileSandboxMenuBar"],
            path: "Tests/SandboxTests"
        ),
    ]
)
