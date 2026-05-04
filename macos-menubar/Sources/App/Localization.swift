import SwiftUI

/// User-selected UI language.
/// `auto` defers to the system locale; otherwise the chosen identifier wins.
enum AppLocale: String, CaseIterable, Identifiable {
    case auto = "auto"
    case en   = "en"
    case ru   = "ru"

    var id: String { rawValue }

    /// Label shown in the Settings → Language picker.
    var displayName: LocalizedStringKey {
        switch self {
        case .auto: return "Auto"
        case .en:   return "English"
        case .ru:   return "Russian"
        }
    }
}

/// Returns nil for `.auto` so SwiftUI falls back to the system locale.
/// Otherwise returns an explicit Locale so the UI ignores system preferences.
func resolvedLocale(for app: AppLocale) -> Locale? {
    switch app {
    case .auto: return nil
    case .en:   return Locale(identifier: "en")
    case .ru:   return Locale(identifier: "ru")
    }
}

/// Type-safe helpers that produce LocalizedStringKey values for daemon-emitted
/// enum strings. Keeps the catalog keys (verdict.*, session.*, mode.*) in one
/// place — renaming a daemon string only requires touching this file.
///
/// IMPORTANT: every helper returns a LITERAL `LocalizedStringKey` via switch.
/// SwiftUI's `Text(_ key:)` only resolves keys constructed via the
/// ExpressibleByStringLiteral path; runtime `LocalizedStringKey(_ value:)`
/// initializers render the raw key string instead of looking it up in the
/// strings catalog. That's why each branch hard-codes its own key.
enum L {
    static func verdict(_ raw: String) -> LocalizedStringKey {
        switch raw.lowercased() {
        case "scanning":     return "verdict.scanning"
        case "queued":       return "verdict.queued"
        case "infected":     return "verdict.infected"
        case "malicious":    return "verdict.malicious"
        case "inconclusive": return "verdict.inconclusive"
        case "oversized":    return "verdict.oversized"
        case "clean":        return "verdict.clean"
        default:             return LocalizedStringKey(raw)
        }
    }
    /// Bigger pill variant in the expanded job row (sentence-cased values).
    static func verdictBig(_ raw: String) -> LocalizedStringKey {
        switch raw.lowercased() {
        case "infected":     return "verdict.big.infected"
        case "malicious":    return "verdict.big.malicious"
        case "inconclusive": return "verdict.big.inconclusive"
        case "oversized":    return "verdict.big.oversized"
        case "clean":        return "verdict.big.clean"
        default:             return LocalizedStringKey(raw)
        }
    }
    static func session(_ raw: String) -> LocalizedStringKey {
        switch raw.lowercased() {
        case "running":   return "session.running"
        case "starting":  return "session.starting"
        case "stopped":   return "session.stopped"
        case "failed":    return "session.failed"
        case "discarded": return "session.discarded"
        default:          return LocalizedStringKey(raw)
        }
    }
    static func mode(_ m: WatcherMode) -> LocalizedStringKey {
        switch m {
        case .active:             return "mode.active"
        case .scanPaused:         return "mode.scan_paused"
        case .monitoringDisabled: return "mode.monitoring_disabled"
        }
    }
}
