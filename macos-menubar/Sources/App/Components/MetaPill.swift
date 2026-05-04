import SwiftUI

/// Reusable white-bg / 1-pt-border pill with a leading SF Symbol and a label.
/// Used by the expanded job row meta strip (filename / age / size).
/// Optional `tooltip` shows a help-tag on hover (e.g. full file path).
struct MetaPill: View {
    let symbol: String
    let text: String
    var tooltip: String? = nil

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .regular))
                .foregroundColor(.secondary)
            Text(text)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(Theme.panelBg)
        .overlay(
            RoundedRectangle(cornerRadius: Theme.cornerRadiusChip)
                .strokeBorder(Theme.separator, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusChip))
        .help(tooltip ?? "")
    }
}
