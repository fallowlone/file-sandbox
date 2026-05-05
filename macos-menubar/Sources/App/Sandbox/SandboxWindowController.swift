import AppKit
import Virtualization

@MainActor
public final class SandboxWindowController: NSWindowController {
    public init(
        sessionID: UUID, vm: VZVirtualMachine, outDir: URL,
        onDiscard: @escaping () -> Void, onExport: @escaping (String) -> Void
    ) {
        super.init(window: NSWindow())
    }
    required init?(coder: NSCoder) { fatalError() }
}
