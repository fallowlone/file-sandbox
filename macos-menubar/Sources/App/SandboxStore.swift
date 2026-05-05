import Foundation
import Combine

@MainActor
final class SandboxStore: ObservableObject {
    @Published var sessions: [SessionRecord] = []
    @Published var sandboxEnabled: Bool = false

    /// Alias kept for existing call-sites (JobsTabView "Open in sandbox" button gating).
    var canOpen: Bool { sandboxEnabled }

    /// Active session count — for the Sandbox tab badge.
    var activeCount: Int { sessions.filter { $0.status == .running }.count }

    private var cancellables = Set<AnyCancellable>()
    private let manager: SandboxManager

    init(manager: SandboxManager = .shared) {
        self.manager = manager
        manager.$sessions
            .receive(on: DispatchQueue.main)
            .assign(to: \.sessions, on: self)
            .store(in: &cancellables)
        refreshEnabled()
    }

    /// Read enabled flag from on-disk config. Call after the user toggles enabled in Settings.
    func refreshEnabled() {
        sandboxEnabled = manager.currentConfig().enabled
    }

    /// Push allowed roots into SandboxManager (typically called when SettingsStore loads /api/config).
    func configurePaths(watchPath: String, quarantinePath: String) {
        manager.configure(watchPath: watchPath, quarantinePath: quarantinePath)
    }

    /// Open a sandbox session for the given file. Errors are logged; UI surfaces enabled/disabled state.
    func openSandbox(filePath: String) {
        do { _ = try manager.openSession(filePath: filePath) }
        catch { NSLog("openSandbox failed: \(error)") }
    }

    func discard(id: UUID) {
        manager.discardSession(id: id)
    }
}
