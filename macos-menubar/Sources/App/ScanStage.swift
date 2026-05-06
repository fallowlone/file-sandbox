import SwiftUI

/// Mirrors the daemon `ScanStage` union (src/job-store.ts).
///
/// The pipeline order used to render the badge row in `JobsTabView` is
/// available as `ScanStage.pipeline`. Note that `received` and `error` are
/// not in `pipeline`: `received` is too short-lived to merit a badge, and
/// `error` replaces the *current* pipeline badge's tint rather than adding
/// its own column.
enum ScanStage: String, CaseIterable {
    case received
    case cacheCheck = "cache_check"
    case localScan  = "local_scan"
    case vtUpload   = "vt_upload"
    case vtPoll     = "vt_poll"
    case done
    case error

    static let pipeline: [ScanStage] = [.cacheCheck, .localScan, .vtUpload, .vtPoll, .done]

    /// Short label rendered inside `StagePill` and `StageBadge`.
    var label: String {
        switch self {
        case .received:   return "Received"
        case .cacheCheck: return "Cache"
        case .localScan:  return "Local"
        case .vtUpload:   return "VT upload"
        case .vtPoll:     return "VT poll"
        case .done:       return "Done"
        case .error:      return "Error"
        }
    }

    /// SF Symbol name used by `JobStore.iconName` and `StagePill`.
    var symbol: String {
        switch self {
        case .received:   return "tray.and.arrow.down"
        case .cacheCheck: return "magnifyingglass"
        case .localScan:  return "shield.lefthalf.filled"
        case .vtUpload:   return "arrow.up.circle"
        case .vtPoll:     return "arrow.triangle.2.circlepath"
        case .done:       return "checkmark.shield.fill"
        case .error:      return "exclamationmark.triangle.fill"
        }
    }

    /// Foreground tint for `StagePill` and for a `current`-state `StageBadge`.
    var tint: Color {
        switch self {
        case .received, .cacheCheck, .vtUpload, .vtPoll: return Theme.verdictBlueFg
        case .localScan:                                  return Theme.verdictOrangeFg
        case .done:                                       return Theme.verdictGreenFg
        case .error:                                      return Theme.verdictRedFg
        }
    }
}

extension SandboxJob {
    /// Parsed `scan_stage` field, or `nil` for legacy rows.
    var stageEnum: ScanStage? {
        scan_stage.flatMap(ScanStage.init(rawValue:))
    }
}
