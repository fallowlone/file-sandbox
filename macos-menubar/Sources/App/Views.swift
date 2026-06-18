import SwiftUI
import AppKit

struct MenuBarContentView: View {
    @ObservedObject var store: JobStore
    @ObservedObject var settingsStore: SettingsStore

    @AppStorage("filesandbox.selectedTab") private var selectedTab: Int = 0

    var body: some View {
        VStack(spacing: 0) {
            AppHeader(store: store)
            AppTabs(
                selection: $selectedTab,
                counts: [
                    .jobs: store.visibleJobCount
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
            JobsTabView(store: store, settingsStore: settingsStore)
                .onAppear { store.fetch() }
        case .settings:
            SettingsTabView(settingsStore: settingsStore, store: store)
        }
    }
}
