import SwiftUI
import UserNotifications

@main
struct FileSandboxMenuBarApp: App {
    @StateObject private var store = JobStore()
    @StateObject private var settingsStore = SettingsStore()
    @StateObject private var sandboxStore = SandboxStore()
    @State private var notifiedAtLaunch = false

    init() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert]) { _, _ in }
    }

    var body: some Scene {
        MenuBarExtra {
            MenuBarContentView(store: store, settingsStore: settingsStore, sandboxStore: sandboxStore)
        } label: {
            Image(systemName: store.iconName)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(menuBarIconColor(for: store.mode))
        }
        .menuBarExtraStyle(.window)
        .onChange(of: store.mode) { _, newMode in
            guard !notifiedAtLaunch else { return }
            notifiedAtLaunch = true
            guard newMode != .active else { return }
            let content = UNMutableNotificationContent()
            content.title = "FileSandbox started in \(newMode.displayName)"
            content.body = newMode == .scanPaused
                ? "New files are quarantined but not scanned. Open the menu bar to resume."
                : "New files are not being monitored. Open the menu bar to resume."
            let req = UNNotificationRequest(identifier: "filesandbox.launch.mode", content: content, trigger: nil)
            UNUserNotificationCenter.current().add(req)
        }
    }
}

private func menuBarIconColor(for mode: WatcherMode) -> Color {
    switch mode {
    case .active: return .primary
    case .scanPaused: return .orange
    case .monitoringDisabled: return .red
    }
}
