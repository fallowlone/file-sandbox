import SwiftUI

/// A single badge in the `StageRow`. The state controls colour, strikethrough,
/// and whether a verdict text suffix is rendered.
struct StageBadge: View {
    enum State {
        case done(verdictText: String?)
        case current(stage: ScanStage)  // stage carries the localScan-orange override
        case pending
        case skipped
        case error(detail: String?)
    }

    let stage: ScanStage
    let state: State

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: leadingSymbol)
                .font(.system(size: 9, weight: .semibold))
            Text(displayText)
                .font(.system(size: 10, weight: .medium))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .strikethrough(isSkipped, color: Theme.separator)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 3)
        .background(backgroundTint)
        .foregroundColor(foregroundTint)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(borderTint, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .fixedSize(horizontal: true, vertical: false)
    }

    private var displayText: String {
        switch state {
        case .done(let verdict):
            if let v = verdict, !v.isEmpty { return "\(stage.label) · \(v)" }
            return stage.label
        case .current:                       return stage.label
        case .pending:                       return stage.label
        case .skipped:                       return stage.label
        case .error:                         return "\(stage.label) error"
        }
    }

    private var leadingSymbol: String {
        switch state {
        case .done:    return "checkmark"
        case .current: return "hourglass"
        case .pending: return "circle"
        case .skipped: return "minus"
        case .error:   return "exclamationmark.triangle.fill"
        }
    }

    private var isSkipped: Bool {
        if case .skipped = state { return true }
        return false
    }

    // MARK: - Tints

    private var backgroundTint: Color {
        switch state {
        case .done(let verdict):
            return verdictIsBad(verdict) ? Theme.verdictRedBg : Theme.verdictGreenBg
        case .current(let s):
            return s == .localScan ? Theme.verdictOrangeBg : Theme.verdictBlueBg
        case .pending, .skipped:
            return Theme.subtleBg
        case .error:
            return Theme.verdictRedBg
        }
    }

    private var foregroundTint: Color {
        switch state {
        case .done(let verdict):
            return verdictIsBad(verdict) ? Theme.verdictRedFg : Theme.verdictGreenFg
        case .current(let s):
            return s == .localScan ? Theme.verdictOrangeFg : Theme.verdictBlueFg
        case .pending:
            return Color.secondary.opacity(0.7)
        case .skipped:
            return Color.secondary.opacity(0.5)
        case .error:
            return Theme.verdictRedFg
        }
    }

    private var borderTint: Color {
        switch state {
        case .done(let verdict):
            return verdictIsBad(verdict) ? Theme.verdictRedFg.opacity(0.4) : Theme.verdictGreenFg.opacity(0.4)
        case .current(let s):
            return s == .localScan
                ? Theme.verdictOrangeFg.opacity(0.4)
                : Theme.verdictBlueFg.opacity(0.4)
        case .pending, .skipped:
            return Theme.separator
        case .error:
            return Theme.verdictRedFg.opacity(0.4)
        }
    }

    /// True when a "done" verdict text indicates a bad finding (infected,
    /// inconclusive). Falsy values, "clean", and pure stage progress markers
    /// (e.g. "miss", "hit clean") render in green.
    private func verdictIsBad(_ verdict: String?) -> Bool {
        guard let v = verdict?.lowercased() else { return false }
        return v.contains("infect") || v == "inconclusive" || v.contains("malic")
    }
}
