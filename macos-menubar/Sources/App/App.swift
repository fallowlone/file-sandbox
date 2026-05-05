import SwiftUI
import UserNotifications

@main
struct FileSandboxMenuBarApp: App {
    @StateObject private var store = JobStore()
    @StateObject private var settingsStore = SettingsStore()
    @StateObject private var sandboxStore = SandboxStore()
    @State private var notifiedAtLaunch = false

    @AppStorage("filesandbox.locale") private var localeRaw: String = AppLocale.auto.rawValue

    init() {
        // UNUserNotificationCenter.current() asserts when there's no CFBundleIdentifier
        // (raw `swift run` from .build/). Skip request when running unbundled in dev.
        if Bundle.main.bundleIdentifier != nil {
            UNUserNotificationCenter.current().requestAuthorization(options: [.alert]) { _, _ in }
        }
    }

    private var appLocale: AppLocale {
        AppLocale(rawValue: localeRaw) ?? .auto
    }

    var body: some Scene {
        MenuBarExtra {
            MenuBarContentView(store: store, settingsStore: settingsStore, sandboxStore: sandboxStore)
                .environment(\.locale, resolvedLocale(for: appLocale) ?? Locale.current)
                .textSelection(.disabled)
                .onAppear {
                    sandboxStore.refreshEnabled()
                }
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
            postLaunchNotification(for: newMode)
        }
    }

    private func postLaunchNotification(for newMode: WatcherMode) {
        let locale = resolvedLocale(for: appLocale) ?? Locale.current
        let modeName: String = {
            switch newMode {
            case .active:              return String(localized: "mode.active",              locale: locale)
            case .scanPaused:          return String(localized: "mode.scan_paused",          locale: locale)
            case .monitoringDisabled:  return String(localized: "mode.monitoring_disabled",  locale: locale)
            }
        }()
        let titleFormat = String(localized: "FileSandbox started in %@", locale: locale)
        let bodyKey: String.LocalizationValue = newMode == .scanPaused
            ? "New files are quarantined but not scanned. Open the menu bar to resume."
            : "New files are not being monitored. Open the menu bar to resume."

        let content = UNMutableNotificationContent()
        content.title = String(format: titleFormat, modeName)
        content.body  = String(localized: bodyKey, locale: locale)

        let req = UNNotificationRequest(identifier: "filesandbox.launch.mode", content: content, trigger: nil)
        if Bundle.main.bundleIdentifier != nil {
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
