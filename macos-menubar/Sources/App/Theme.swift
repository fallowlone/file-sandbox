import SwiftUI

/// Centralised design tokens for the menu-bar redesign.
/// Values come from the design spec (docs/superpowers/specs/2026-05-04-menubar-ui-redesign-design.md, § Theme tokens).
enum Theme {
    // Type sizes
    static let chipFontSize: CGFloat = 10
    static let smallFontSize: CGFloat = 11
    static let bodyFontSize: CGFloat = 12

    // Radii
    static let cornerRadiusPanel: CGFloat = 12
    static let cornerRadiusChip: CGFloat = 6
    static let cornerRadiusPill: CGFloat = 8
    static let cornerRadiusButton: CGFloat = 7

    // Surfaces / borders
    static let separator = Color(nsColor: .separatorColor)
    static let panelBg = Color(nsColor: .windowBackgroundColor)
    static let subtleBg = Color(nsColor: .controlBackgroundColor)

    // Verdict / status tints
    static let verdictRedBg    = Color(red: 0.99, green: 0.91, blue: 0.91)
    static let verdictRedFg    = Color(red: 0.64, green: 0.15, blue: 0.15)
    static let verdictOrangeBg = Color(red: 1.00, green: 0.95, blue: 0.88)
    static let verdictOrangeFg = Color(red: 0.65, green: 0.35, blue: 0.00)
    static let verdictGreenBg  = Color(red: 0.90, green: 0.97, blue: 0.92)
    static let verdictGreenFg  = Color(red: 0.11, green: 0.49, blue: 0.23)
    static let verdictBlueBg   = Color(red: 0.93, green: 0.95, blue: 0.98)
    static let verdictBlueFg   = Color(red: 0.20, green: 0.27, blue: 0.33)
    static let verdictGreyBg   = Color(red: 0.94, green: 0.94, blue: 0.95)
    static let verdictGreyFg   = Color(red: 0.40, green: 0.40, blue: 0.42)

    // Discard button hover (sandbox row)
    static let discardHoverBg     = Color(red: 0.99, green: 0.92, blue: 0.92)
    static let discardHoverBorder = Color(red: 0.95, green: 0.79, blue: 0.79)
}
