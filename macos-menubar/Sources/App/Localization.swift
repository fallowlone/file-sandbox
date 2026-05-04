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
enum L {
    static func verdict(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("verdict.\(raw.lowercased())")
    }
    /// Bigger pill variant in the expanded job row (sentence-cased values).
    static func verdictBig(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("verdict.big.\(raw.lowercased())")
    }
    static func session(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("session.\(raw.lowercased())")
    }
    static func mode(_ m: WatcherMode) -> LocalizedStringKey {
        LocalizedStringKey("mode.\(m.rawValue)")
    }
}
