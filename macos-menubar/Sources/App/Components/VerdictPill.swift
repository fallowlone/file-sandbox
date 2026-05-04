import SwiftUI

/// Coloured lozenge used for verdict / state labels.
/// `.mini` is the row-collapsed variant. `.big` is the expanded-row header variant.
struct VerdictPill: View {
    enum Size { case mini, big }
    enum Variant { case red, orange, green, blue, grey }

    let text: String
    let variant: Variant
    let size: Size
    var symbol: String? = nil

    var body: some View {
        let colors = tints(for: variant)
        HStack(spacing: 4) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: size == .mini ? 8 : 10, weight: .semibold))
            }
            Text(text)
                .font(.system(size: size == .mini ? 9 : 11, weight: .semibold))
        }
        .padding(.horizontal, size == .mini ? 8 : 10)
        .padding(.vertical, size == .mini ? 1 : 3)
        .foregroundColor(colors.fg)
        .background(colors.bg)
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadiusPill))
    }

    private func tints(for v: Variant) -> (bg: Color, fg: Color) {
        switch v {
        case .red:    return (Theme.verdictRedBg, Theme.verdictRedFg)
        case .orange: return (Theme.verdictOrangeBg, Theme.verdictOrangeFg)
        case .green:  return (Theme.verdictGreenBg, Theme.verdictGreenFg)
        case .blue:   return (Theme.verdictBlueBg, Theme.verdictBlueFg)
        case .grey:   return (Theme.verdictGreyBg, Theme.verdictGreyFg)
        }
    }
}

extension VerdictPill {
    /// Map a job's `vt_verdict` string + status to a pill variant + label.
    static func forJobVerdict(verdict: String?, status: String) -> VerdictPill? {
        if status == "scanning" || status == "received" {
            return VerdictPill(text: "scanning", variant: .blue, size: .mini, symbol: "hourglass")
        }
        if status == "in_quarantine" {
            return VerdictPill(text: "queued", variant: .blue, size: .mini, symbol: "tray")
        }
        guard let v = verdict?.lowercased() else { return nil }
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: "infected", variant: .red, size: .mini, symbol: "exclamationmark.triangle.fill")
        case "inconclusive", "unclear":
            return VerdictPill(text: "inconclusive", variant: .orange, size: .mini, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: "oversized", variant: .grey, size: .mini, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: "clean", variant: .green, size: .mini, symbol: "checkmark.circle.fill")
        default:
            return VerdictPill(text: v, variant: .grey, size: .mini)
        }
    }
}

/// Sandbox session state pill (running / starting / stopped / failed / discarded).
/// Same look as VerdictPill mini; separate factory for clarity.
struct SessionStatePill: View {
    let status: String
    var body: some View {
        switch status {
        case "running":   VerdictPill(text: "running",   variant: .green, size: .mini)
        case "starting":  VerdictPill(text: "starting",  variant: .blue,  size: .mini)
        case "stopped":   VerdictPill(text: "stopped",   variant: .red,   size: .mini)
        case "failed":    VerdictPill(text: "failed",    variant: .red,   size: .mini)
        case "discarded": VerdictPill(text: "discarded", variant: .grey,  size: .mini)
        default:          EmptyView()
        }
    }
}
