import SwiftUI

/// Header chip: shows current WatcherMode; click opens a Menu for switching.
/// Daemon-offline state is rendered as `disconnected` (red) and disabled.
struct StatusChip: View {
    let mode: WatcherMode
    let isConnected: Bool
    let onSelect: (WatcherMode) -> Void

    private var tints: (bg: Color, fg: Color) {
        guard isConnected else { return (Theme.verdictRedBg, Theme.verdictRedFg) }
        switch mode {
        case .active:              return (Theme.verdictGreenBg,  Theme.verdictGreenFg)
        case .scanPaused:          return (Theme.verdictOrangeBg, Theme.verdictOrangeFg)
        case .monitoringDisabled:  return (Theme.verdictRedBg,    Theme.verdictRedFg)
        }
    }

    private var label: String {
        isConnected ? mode.displayName : "Disconnected"
    }

    var body: some View {
        Menu {
            ForEach(WatcherMode.allCases, id: \.self) { m in
                Button {
                    onSelect(m)
                } label: {
                    Label {
                        Text(m.displayName)
                    } icon: {
                        if mode == m { Image(systemName: "checkmark") }
                    }
                }
            }
        } label: {
            HStack(spacing: 4) {
                Text(label)
                    .font(.system(size: 10, weight: .semibold))
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 2)
            .background(tints.bg)
            .foregroundColor(tints.fg)
            .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusChip))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(!isConnected)
        .help("Watcher mode")
    }
}
