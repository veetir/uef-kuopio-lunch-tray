import AppKit
import SwiftUI

enum AppAccent: String, CaseIterable, Identifiable {
    case system
    case blue
    case orange
    case graphite

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: "System"
        case .blue: "Blue"
        case .orange: "Orange"
        case .graphite: "Graphite"
        }
    }

    var color: Color {
        Color(nsColor: nsColor)
    }

    var overridesSystemAccent: Bool {
        self != .system
    }

    private var nsColor: NSColor {
        switch self {
        case .system: .controlAccentColor
        case .blue: .systemBlue
        case .orange: .systemOrange
        case .graphite: .systemGray
        }
    }
}

enum AppAppearance {
    static let preferredColorScheme: ColorScheme? = nil
}

extension View {
    @ViewBuilder
    func lunchTrayAppearance(accent: AppAccent) -> some View {
        if accent.overridesSystemAccent {
            tint(accent.color)
                .preferredColorScheme(AppAppearance.preferredColorScheme)
        } else {
            preferredColorScheme(AppAppearance.preferredColorScheme)
        }
    }
}
