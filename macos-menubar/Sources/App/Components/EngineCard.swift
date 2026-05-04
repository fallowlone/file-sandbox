import SwiftUI

/// Small engine result card used inside an expanded job row.
/// Layout: dot (status) + label (engine name) + value (verdict / count).
struct EngineCard: View {
    enum Status { case clean, malicious, warn, neutral }
    let label: String
    let value: String
    let status: Status

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(dotColor)
                .frame(width: 6, height: 6)
            Text(label)
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(.primary)
            Text(value)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 4)
        .background(Theme.panelBg)
        .overlay(
            RoundedRectangle(cornerRadius: Theme.cornerRadiusPill)
                .strokeBorder(Theme.separator, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusPill))
    }

    private var dotColor: Color {
        switch status {
        case .clean:     return Theme.verdictGreenFg
        case .malicious: return Theme.verdictRedFg
        case .warn:      return Theme.verdictOrangeFg
        case .neutral:   return Theme.verdictGreyFg
        }
    }
}
