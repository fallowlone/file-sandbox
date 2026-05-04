import SwiftUI

/// 30×16 pill toggle styled to match the redesign mockups.
/// Drop-in replacement for `Toggle("", isOn:)` where the visual must match the design spec.
struct AppSwitch: View {
    @Binding var isOn: Bool
    var body: some View {
        ZStack(alignment: isOn ? .trailing : .leading) {
            RoundedRectangle(cornerRadius: 8)
                .fill(isOn ? Theme.verdictGreenFg : Color.gray.opacity(0.4))
                .frame(width: 30, height: 16)
            Circle()
                .fill(.white)
                .frame(width: 12, height: 12)
                .shadow(color: .black.opacity(0.2), radius: 0.5, x: 0, y: 1)
                .padding(.horizontal, 2)
        }
        .animation(.easeInOut(duration: 0.15), value: isOn)
        .contentShape(Rectangle())
        .onTapGesture { isOn.toggle() }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityValue(isOn ? "on" : "off")
    }
}
