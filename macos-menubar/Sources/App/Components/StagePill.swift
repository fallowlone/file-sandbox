import SwiftUI

/// A small icon+label pill rendered in the collapsed job row while a scan is
/// in flight (`stageEnum != nil && stageEnum != .done && stageEnum != .error`).
/// Replaces the verdict mini-pill until the scan finishes.
struct StagePill: View {
    let stage: ScanStage

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: stage.symbol)
                .font(.system(size: 9, weight: .semibold))
            Text(stage.label)
                .font(.system(size: 9, weight: .semibold))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 2)
        .background(backgroundTint)
        .foregroundColor(stage.tint)
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var backgroundTint: Color {
        switch stage {
        case .localScan: return Theme.verdictOrangeBg
        case .done:      return Theme.verdictGreenBg
        case .error:     return Theme.verdictRedBg
        default:         return Theme.verdictBlueBg
        }
    }
}
