import SwiftUI
import AppKit

struct MenuBarContentView: View {
    @ObservedObject var store: JobStore
    @ObservedObject var settingsStore: SettingsStore
    @ObservedObject var sandboxStore: SandboxStore

    @AppStorage("filesandbox.selectedTab") private var selectedTab: Int = 0

    var body: some View {
        VStack(spacing: 0) {
            AppHeader(store: store)
            AppTabs(
                selection: $selectedTab,
                counts: [
                    .jobs: store.visibleJobCount,
                    .sandbox: sandboxStore.activeCount
                ]
            )
            tabBody
                .frame(minHeight: 520, maxHeight: 600)
            AppFooter(store: store, settingsStore: settingsStore)
        }
        .frame(width: 420)
        .background(Theme.panelBg)
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusPanel))
    }

    @ViewBuilder
    private var tabBody: some View {
        switch AppTab(rawValue: selectedTab) ?? .jobs {
        case .jobs:
            JobsTabView(store: store, sandboxStore: sandboxStore, settingsStore: settingsStore)
                .onAppear { store.fetch() }
        case .sandbox:
            SandboxTabView(store: sandboxStore, settingsStore: settingsStore)
        case .settings:
            SettingsTabView(settingsStore: settingsStore, store: store)
        }
    }
}
