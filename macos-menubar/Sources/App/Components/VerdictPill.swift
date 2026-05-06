import SwiftUI

/// Coloured lozenge used for verdict / state labels.
/// `.mini` is the row-collapsed variant. `.big` is the expanded-row header variant.
struct VerdictPill: View {
    enum Size { case mini, big }
    enum Variant { case red, orange, green, blue, grey }

    let text: LocalizedStringKey
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
    /// Map both engine verdicts + job status to a collapsed-row pill.
    ///
    /// Priority:
    ///   1. Either verdict is `infected` / `malicious`             → red "infected"
    ///   2. status is `scanning` / `received` / `in_quarantine`    → blue "scanning"
    ///   3. vt_verdict == `inconclusive` / `unclear`               → orange "inconclusive"
    ///   4. vt_verdict == `oversized`                              → grey "oversized"
    ///   5. status == `restored` or vt_verdict == `clean`          → green "clean"
    ///   6. otherwise                                              → nil
    static func forJobVerdict(vt: String?, pompelmi: String?, status: String) -> VerdictPill? {
        let v = (vt ?? "").lowercased()
        let p = (pompelmi ?? "").lowercased()

        if v == "infected" || v == "malicious" || p == "malicious" {
            return VerdictPill(text: L.verdict("infected"), variant: .red, size: .mini, symbol: "exclamationmark.triangle.fill")
        }
        if status == "scanning" || status == "received" {
            return VerdictPill(text: L.verdict("scanning"), variant: .blue, size: .mini, symbol: "hourglass")
        }
        if status == "in_quarantine" {
            return VerdictPill(text: L.verdict("queued"), variant: .blue, size: .mini, symbol: "tray")
        }
        if v == "inconclusive" || v == "unclear" {
            return VerdictPill(text: L.verdict("inconclusive"), variant: .orange, size: .mini, symbol: "questionmark.circle.fill")
        }
        if v == "oversized" {
            return VerdictPill(text: L.verdict("oversized"), variant: .grey, size: .mini, symbol: "arrow.down.circle")
        }
        if v == "clean" || status == "restored" {
            return VerdictPill(text: L.verdict("clean"), variant: .green, size: .mini, symbol: "checkmark.circle.fill")
        }
        return nil
    }
}

/// Sandbox session state pill (running / starting / stopped / failed / discarded / error).
/// Same look as VerdictPill mini; separate factory for clarity.
struct SessionStatePill: View {
    let status: String
    var body: some View {
        switch status {
        case "running":   VerdictPill(text: L.session("running"),   variant: .green, size: .mini)
        case "starting":  VerdictPill(text: L.session("starting"),  variant: .blue,  size: .mini)
        case "stopped":   VerdictPill(text: L.session("stopped"),   variant: .red,   size: .mini)
        case "failed":    VerdictPill(text: L.session("failed"),    variant: .red,   size: .mini)
        case "discarded": VerdictPill(text: L.session("discarded"), variant: .grey,  size: .mini)
        case "error":     VerdictPill(text: L.session("error"),     variant: .red,   size: .mini)
        default:          EmptyView()
        }
    }
}
