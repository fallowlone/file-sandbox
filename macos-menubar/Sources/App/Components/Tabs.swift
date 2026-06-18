import SwiftUI

enum AppTab: Int, CaseIterable, Identifiable {
    case jobs = 0
    case settings = 1

    var id: Int { rawValue }

    var title: LocalizedStringKey {
        switch self {
        case .jobs:     return "Jobs"
        case .settings: return "Settings"
        }
    }
}

/// Pill segmented control. Active = white bg + 1pt soft shadow + 600 weight; inactive = transparent + secondary fg.
struct AppTabs: View {
    @Binding var selection: Int
    /// Optional count chips per tab (nil = no chip).
    var counts: [AppTab: Int] = [:]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(AppTab.allCases) { tab in
                let isOn = selection == tab.rawValue
                Button {
                    selection = tab.rawValue
                } label: {
                    HStack(spacing: 4) {
                        Text(tab.title)
                            .font(.system(size: 11, weight: isOn ? .semibold : .regular))
                            .foregroundColor(isOn ? .primary : .secondary)
                        if let c = counts[tab], c > 0 {
                            Text("\(c)")
                                .font(.system(size: 10, weight: .regular))
                                .foregroundColor(Color.secondary.opacity(0.7))
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 5)
                    .background(
                        Group {
                            if isOn {
                                RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                                    .fill(Theme.panelBg)
                                    .shadow(color: .black.opacity(0.07), radius: 1.5, x: 0, y: 1)
                            } else {
                                Color.clear
                            }
                        }
                    )
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Theme.subtleBg)
        .overlay(Divider(), alignment: .bottom)
    }
}
