import AppKit
import Virtualization
import Darwin

@MainActor
public final class SandboxWindowController: NSWindowController {
    private let sessionID: UUID
    private weak var vm: VZVirtualMachine?
    private let outDir: URL
    private let onDiscard: () -> Void
    private let onExport: (String) -> Void

    private var fsSource: DispatchSourceFileSystemObject?
    private var dirFD: Int32 = -1
    private var bannerLabel: NSTextField?

    public init(
        sessionID: UUID, vm: VZVirtualMachine, outDir: URL,
        onDiscard: @escaping () -> Void, onExport: @escaping (String) -> Void
    ) {
        self.sessionID = sessionID
        self.vm = vm
        self.outDir = outDir
        self.onDiscard = onDiscard
        self.onExport = onExport
        let win = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered, defer: false)
        win.title = "Sandbox \(sessionID.uuidString.prefix(8))"
        super.init(window: win)
        buildContent()
        watchOutDir()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func buildContent() {
        guard let win = window, let vm = vm else { return }
        let view = VZVirtualMachineView()
        view.virtualMachine = vm
        view.translatesAutoresizingMaskIntoConstraints = false

        let banner = NSTextField(labelWithString: "")
        banner.translatesAutoresizingMaskIntoConstraints = false
        banner.isHidden = true
        bannerLabel = banner

        let toolbar = NSStackView()
        toolbar.orientation = .horizontal
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        toolbar.addArrangedSubview(NSButton(title: "Discard", target: self, action: #selector(discard)))
        toolbar.addArrangedSubview(NSButton(title: "Export…", target: self, action: #selector(exportPicker)))
        toolbar.addArrangedSubview(banner)

        let stack = NSStackView(views: [toolbar, view])
        stack.orientation = .vertical
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.spacing = 6

        win.contentView = stack
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: win.contentView!.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: win.contentView!.trailingAnchor),
            stack.topAnchor.constraint(equalTo: win.contentView!.topAnchor),
            stack.bottomAnchor.constraint(equalTo: win.contentView!.bottomAnchor),
        ])
    }

    private func watchOutDir() {
        dirFD = open(outDir.path, O_EVTONLY)
        guard dirFD >= 0 else { return }
        fsSource = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: dirFD, eventMask: .write, queue: .main)
        fsSource?.setEventHandler { [weak self] in
            Task { @MainActor in self?.refreshBanner() }
        }
        let capturedFD = dirFD
        fsSource?.setCancelHandler {
            if capturedFD >= 0 { Darwin.close(capturedFD) }
        }
        fsSource?.resume()
    }

    private func refreshBanner() {
        let count = (try? FileManager.default.contentsOfDirectory(atPath: outDir.path).count) ?? 0
        if count > 0 {
            bannerLabel?.isHidden = false
            bannerLabel?.stringValue = "\(count) file(s) ready to export"
        } else {
            bannerLabel?.isHidden = true
        }
    }

    @objc private func discard() { onDiscard() }

    @objc private func exportPicker() {
        let alert = NSAlert()
        alert.messageText = "Export from sandbox"
        let files = (try? FileManager.default.contentsOfDirectory(atPath: outDir.path)) ?? []
        guard !files.isEmpty else {
            alert.informativeText = "No files to export."
            alert.runModal(); return
        }
        let popup = NSPopUpButton(frame: NSRect(x: 0, y: 0, width: 300, height: 24))
        popup.addItems(withTitles: files)
        alert.accessoryView = popup
        alert.addButton(withTitle: "Export")
        alert.addButton(withTitle: "Cancel")
        if alert.runModal() == .alertFirstButtonReturn, let name = popup.titleOfSelectedItem {
            onExport(name)
        }
    }

    deinit {
        fsSource?.cancel()
    }
}
