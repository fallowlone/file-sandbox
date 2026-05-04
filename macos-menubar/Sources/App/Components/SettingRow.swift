import SwiftUI

/// Single label + control row for the Settings tab.
/// Vertical rhythm 9 pt, horizontal 14 pt. Label flexes; control is flush right.
struct SettingRow<Control: View>: View {
    let label: LocalizedStringKey
    var indent: CGFloat = 0
    @ViewBuilder let control: () -> Control

    var body: some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.system(size: 12))
                .foregroundColor(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
            control()
        }
        .padding(.leading, 14 + indent)
        .padding(.trailing, 14)
        .padding(.vertical, 9)
        .overlay(Divider(), alignment: .bottom)
    }
}

/// Group header for the Settings tab. Always visible (not collapsible per spec).
struct SettingGroupHeader: View {
    let title: LocalizedStringKey
    var body: some View {
        Text(title)
            .textCase(.uppercase)
            .font(.system(size: 10, weight: .semibold))
            .tracking(0.5)
            .foregroundColor(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .background(Theme.subtleBg)
            .overlay(Divider(), alignment: .bottom)
    }
}
