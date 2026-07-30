import AppKit
import SwiftUI

@MainActor
final class PanelState: ObservableObject {
    @Published var isShowingSettings = false
    var onDismissPanel: (() -> Void)?

    func dismissPanel() {
        onDismissPanel?()
    }
}

struct MenuPopoverView: View {
    @EnvironmentObject private var appModel: AppModel
    @EnvironmentObject private var panelState: PanelState
    @State private var expandedMealID: String?
    @State private var isCloseButtonHovered = false

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .frame(width: 440, height: 560)
        .background(.ultraThickMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .lunchTrayAppearance(accent: appModel.accent)
        .onChange(of: appModel.selectedRestaurantCode) { _ in
            expandedMealID = nil
        }
    }

    private var header: some View {
        ZStack {
            settingsHeader
                .opacity(panelState.isShowingSettings ? 1 : 0)
                .allowsHitTesting(panelState.isShowingSettings)
                .accessibilityHidden(!panelState.isShowingSettings)

            menuHeader
                .opacity(panelState.isShowingSettings ? 0 : 1)
                .allowsHitTesting(!panelState.isShowingSettings)
                .accessibilityHidden(panelState.isShowingSettings)
        }
    }

    private var menuHeader: some View {
        ZStack {
            VStack(spacing: 1) {
                Text(appModel.selectedRestaurant.name)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                Text("\(appModel.selectedRestaurantIndex + 1) / \(appModel.restaurants.count)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .overlay(alignment: .trailing) {
                if appModel.isLoading {
                    ProgressView()
                        .controlSize(.small)
                        .offset(x: 18)
                        .accessibilityLabel(localized("Updating", "Päivitetään"))
                }
            }

            HStack(spacing: 8) {
                closeButton

                Button {
                    appModel.selectPreviousRestaurant()
                } label: {
                    Image(systemName: "chevron.left")
                        .frame(width: 24, height: 24)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(appModel.accent.color)
                .help(localized("Previous restaurant (←)", "Edellinen ravintola (←)"))

                Spacer()

                Button {
                    appModel.selectNextRestaurant()
                } label: {
                    Image(systemName: "chevron.right")
                        .frame(width: 24, height: 24)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(appModel.accent.color)
                .help(localized("Next restaurant (→)", "Seuraava ravintola (→)"))

                Button {
                    Task {
                        await appModel.refresh()
                    }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(appModel.accent.color)
                .help(localized("Refresh", "Päivitä"))
                .disabled(
                    appModel.isLoading ||
                    !appModel.canRefreshSelectedRestaurant
                )

                Button {
                    appModel.openRestaurantPage()
                } label: {
                    Image(systemName: "arrow.up.right.square")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(appModel.accent.color)
                .help(localized("Restaurant website", "Ravintolan sivu"))
                .disabled(
                    appModel.snapshot?.restaurantURL == nil
                        && appModel.selectedRestaurant.pageURL == nil
                )

                Button {
                    panelState.isShowingSettings = true
                } label: {
                    Image(systemName: "gearshape")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(appModel.accent.color)
                .help(localized("Settings", "Asetukset"))
            }
        }
        .frame(height: 29)
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
    }

    private var settingsHeader: some View {
        ZStack {
            Text(localized("Settings", "Asetukset"))
                .font(.system(size: 14, weight: .semibold))

            HStack {
                closeButton

                Spacer()

                Button {
                    panelState.isShowingSettings = false
                } label: {
                    Image(systemName: "gearshape")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(appModel.accent.color)
                .help(localized("Back to lunch menu", "Takaisin ruokalistaan"))
            }
        }
        .frame(height: 29)
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
    }

    private var closeButton: some View {
        Button {
            panelState.dismissPanel()
        } label: {
            ZStack {
                Circle()
                    .fill(Color(nsColor: .systemRed))
                    .frame(width: 13, height: 13)

                Image(systemName: "xmark")
                    .font(.system(size: 7, weight: .bold))
                    .foregroundStyle(Color.black.opacity(0.55))
                    .opacity(isCloseButtonHovered ? 1 : 0)
            }
            .frame(width: 24, height: 24)
        }
        .buttonStyle(.plain)
        .onHover { isCloseButtonHovered = $0 }
        .help(localized("Close", "Sulje"))
        .accessibilityLabel(localized("Close", "Sulje"))
    }

    private var content: some View {
        ZStack {
            EmbeddedSettingsView()
                .opacity(panelState.isShowingSettings ? 1 : 0)
                .allowsHitTesting(panelState.isShowingSettings)
                .accessibilityHidden(!panelState.isShowingSettings)

            menuContent
                .opacity(panelState.isShowingSettings ? 0 : 1)
                .allowsHitTesting(!panelState.isShowingSettings)
                .accessibilityHidden(panelState.isShowingSettings)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private var menuContent: some View {
        if let snapshot = appModel.snapshot {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    if let closure = appModel.activeClosure {
                        ClosureNoticeView(
                            closure: closure,
                            language: appModel.language,
                            referenceYear: Int(snapshot.menu?.date.prefix(4) ?? "")
                                ?? LocalDate.today().year
                        )
                    } else {
                        menuMetadata(snapshot)

                        if let message = appModel.errorMessage {
                            StatusMessage(
                                text: message,
                                systemImage: "wifi.exclamationmark"
                            )
                        }

                        if snapshot.effectiveServiceStatus == .unknown {
                            EmptyStateView(
                                title: localized(
                                    "Menu unavailable",
                                    "Ruokalistaa ei saatavilla"
                                ),
                                description: localized(
                                    "Try refreshing the menu.",
                                    "Yritä päivittää ruokalista."
                                ),
                                systemImage: "exclamationmark.triangle"
                            )
                            .frame(maxWidth: .infinity)
                            .padding(.top, 45)
                        } else if let menu = snapshot.menu, !menu.offers.isEmpty {
                            LunchOffersView(offers: menu.offers)
                        }

                        if snapshot.effectiveServiceStatus != .unknown,
                           let menu = snapshot.menu,
                           !menu.groupsWithItems.isEmpty {
                            ForEach(menu.groupsWithItemsByDescendingPrice { group in
                                appModel.displayPrice(for: group)
                            }) { group in
                                MenuGroupView(
                                    group: group,
                                    priceText: appModel.displayPrice(for: group),
                                    showAllergens: appModel.showAllergens,
                                    layout: appModel.lunchLayout,
                                    expandedMealID: $expandedMealID
                                )
                                .background(MenuItemScrollAnchor())
                            }
                        } else if snapshot.effectiveServiceStatus != .unknown {
                            EmptyStateView(
                                title: localized("No lunch today", "Ei lounasta tänään"),
                                description: localized(
                                    "No menu was published for this restaurant.",
                                    "Ravintolalle ei ole julkaistu ruokalistaa."
                                ),
                                systemImage: "fork.knife"
                            )
                            .frame(maxWidth: .infinity)
                            .padding(.top, 45)
                        }
                    }
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 17)
                .background(MenuScrollViewMarker())
            }
        } else if appModel.isLoading {
            VStack(spacing: 12) {
                ProgressView()
                Text(localized("Loading today’s menu…", "Ladataan päivän ruokalistaa…"))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let closure = appModel.activeClosure {
            ClosureNoticeView(
                closure: closure,
                language: appModel.language,
                referenceYear: LocalDate.today().year
            )
            .padding(16)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        } else {
            EmptyStateView(
                title: localized("Menu unavailable", "Ruokalistaa ei saatavilla"),
                description: appModel.errorMessage ?? localized(
                    "Try refreshing the menu.",
                    "Yritä päivittää ruokalista."
                ),
                systemImage: "exclamationmark.triangle"
            )
        }
    }

    private func menuMetadata(_ snapshot: MenuSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            if let menu = snapshot.menu {
                TimelineView(.periodic(from: .now, by: 60)) { context in
                    metadataText(menu, at: context.date)
                        .font(.system(size: 16, weight: .semibold))
                }
            }
        }
    }

    private func localized(_ english: String, _ finnish: String) -> String {
        appModel.language == .fi ? finnish : english
    }

    private func metadataText(_ menu: LunchMenu, at date: Date) -> Text {
        let dateText = displayDate(menu.date)
        let hours = menu.lunchTime.normalizedWhitespace
        let normalColor = Color(nsColor: .secondaryLabelColor)
        var result = Text(dateText).foregroundColor(normalColor)

        guard !hours.isEmpty else { return result }
        if !dateText.isEmpty {
            result = result + Text(" · ").foregroundColor(normalColor)
        }

        let status = OpeningHoursClock.status(for: hours, at: date)
        var hoursText = Text(hours).foregroundColor(
            status == .closed ? normalColor.opacity(0.7) : normalColor
        )
        if status == .closingSoon {
            hoursText = hoursText.italic()
        }
        return result + hoursText
    }

    private func displayDate(_ date: String) -> String {
        let input = DateFormatter()
        input.calendar = Calendar(identifier: .gregorian)
        input.locale = Locale(identifier: "en_US_POSIX")
        input.dateFormat = "yyyy-MM-dd"

        guard let parsed = input.date(from: date) else {
            return date
        }

        let output = DateFormatter()
        output.locale = Locale(identifier: appModel.language == .fi ? "fi_FI" : "en_FI")
        output.dateStyle = .full
        return output.string(from: parsed)
    }
}

struct MenuScrollViewMarker: NSViewRepresentable {
    func makeNSView(context: Context) -> MenuScrollViewMarkerView {
        MenuScrollViewMarkerView()
    }

    func updateNSView(_ nsView: MenuScrollViewMarkerView, context: Context) {}
}

final class MenuScrollViewMarkerView: NSView {}

struct MenuItemScrollAnchor: NSViewRepresentable {
    func makeNSView(context: Context) -> MenuItemScrollAnchorView {
        MenuItemScrollAnchorView()
    }

    func updateNSView(_ nsView: MenuItemScrollAnchorView, context: Context) {}
}

final class MenuItemScrollAnchorView: NSView {}

private struct MenuGroupView: View {
    @EnvironmentObject private var appModel: AppModel

    let group: LunchGroup
    let priceText: String
    let showAllergens: Bool
    let layout: LunchLayout
    @Binding var expandedMealID: String?

    @ViewBuilder
    var body: some View {
        switch layout {
        case .legacy:
            legacyView
        case .standard:
            standardView
        case .compact:
            compactView
        }
    }

    private var legacyView: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !category.isEmpty || !priceText.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    if !category.isEmpty {
                        Text(category)
                            .font(.system(size: 15.5, weight: .semibold))
                    }
                    if !priceText.isEmpty {
                        Text(priceText)
                            .font(.system(size: 13.5, weight: .medium))
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                            .textSelection(.enabled)
                    }
                }
            }

            componentRows(compact: false)
        }
    }

    private var standardView: some View {
        VStack(alignment: .leading, spacing: 9) {
            priceHeader(font: .system(size: 15, weight: .semibold))
            componentRows(compact: false)
                .padding(.leading, usesPriceIndent ? 8 : 0)
            if !category.isEmpty {
                Text(category)
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(.secondary)
                    .padding(.leading, usesPriceIndent ? 8 : 0)
            }
        }
    }

    private var compactView: some View {
        VStack(alignment: .leading, spacing: 5) {
            priceHeader(font: .system(size: 14, weight: .semibold))
            componentRows(compact: true)
                .padding(.leading, usesPriceIndent ? 8 : 0)
        }
    }

    @ViewBuilder
    private func priceHeader(font: Font) -> some View {
        if !priceText.isEmpty {
            Text(priceText)
                .font(font)
                .monospacedDigit()
                .textSelection(.enabled)
        }
    }

    @ViewBuilder
    private func componentRows(compact: Bool) -> some View {
        ForEach(Array(group.components.enumerated()), id: \.offset) { index, component in
            mealRow(
                component: component,
                detail: group.detail(at: index),
                rowID: "\(group.id)-\(index)",
                compact: compact
            )
        }
    }

    private func mealRow(
        component: String,
        detail: RecipeDetail?,
        rowID: String,
        compact: Bool
    ) -> some View {
        let parts = ComponentParts(component)
        let mealHighlighted = appModel.mealIsHighlighted(parts.name)
        let ingredientHighlighted = detail.map {
            appModel.ingredientsAreHighlighted($0.ingredients)
        } ?? false
        let isExpanded = detail != nil && expandedMealID == rowID

        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 7) {
                mealText(parts: parts, compact: compact)

                Spacer(minLength: 4)

                if mealHighlighted {
                    Image(systemName: "star.fill")
                        .font(.system(size: 10))
                        .foregroundStyle(appModel.accent.color)
                        .help(localized("Highlighted meal", "Korostettu ruoka"))
                }

                if let detail, detail.hasDisplayContent {
                    Button {
                        expandedMealID = isExpanded ? nil : rowID
                    } label: {
                        Image(
                            systemName: isExpanded
                                ? "info.circle.fill"
                                : "info.circle"
                        )
                    }
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help(
                        localized(
                            isExpanded ? "Hide details" : "Show ingredients",
                            isExpanded ? "Piilota tiedot" : "Näytä ainesosat"
                        )
                    )
                }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                if mealHighlighted {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(appModel.accent.color.opacity(0.12))
                }
            }
            .overlay {
                if ingredientHighlighted {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(appModel.accent.color.opacity(0.55), lineWidth: 1)
                }
            }
            .contentShape(Rectangle())
            .contextMenu {
                Button(
                    appModel.hasExactMealHighlight(parts.name)
                        ? localized(
                            "Remove meal highlight",
                            "Poista ruoan korostus"
                        )
                        : localized("Highlight meal", "Korosta ruoka")
                ) {
                    appModel.toggleMealHighlight(parts.name)
                }
            }

            if isExpanded, let detail {
                RecipeDetailView(detail: detail)
                    .padding(.leading, 6)
            }
        }
    }

    @ViewBuilder
    private func mealText(parts: ComponentParts, compact: Bool) -> some View {
        if compact, showAllergens, !parts.diets.isEmpty {
            (
                Text(parts.name)
                + Text("  \(parts.diets)")
                    .foregroundColor(.secondary)
            )
            .font(.system(size: 14))
            .lineSpacing(1)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
        } else {
            VStack(alignment: .leading, spacing: 3) {
                Text(parts.name)
                    .font(.system(size: 15))
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                if showAllergens, !parts.diets.isEmpty {
                    Text(parts.diets)
                        .font(.system(size: 12.5))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
        }
    }

    private var category: String {
        group.name
    }

    private var usesPriceIndent: Bool {
        layout != .legacy && !priceText.isEmpty
    }

    private func localized(_ english: String, _ finnish: String) -> String {
        appModel.language == .fi ? finnish : english
    }
}

private struct LunchOffersView: View {
    @EnvironmentObject private var appModel: AppModel

    let offers: [LunchOffer]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(offers) { offer in
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        let priceText = appModel.displayPrice(for: offer)
                        if !priceText.isEmpty {
                            Text(priceText)
                                .font(.system(size: 14, weight: .semibold))
                                .monospacedDigit()
                        }
                        Text(offer.label)
                            .font(.system(size: 14.5, weight: .medium))
                    }
                    if let description = offer.description,
                       !description.normalizedWhitespace.isEmpty {
                        Text(description)
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
    }
}

private struct RecipeDetailView: View {
    @EnvironmentObject private var appModel: AppModel
    @State private var isAddingIngredientHighlight = false
    @State private var ingredientHighlight = ""

    let detail: RecipeDetail

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !detail.ingredients.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(localized("Ingredients", "Ainesosat"))
                            .font(.system(size: 11.5, weight: .semibold))
                            .foregroundStyle(.secondary)

                        Spacer()

                        Button {
                            ingredientHighlight = ""
                            isAddingIngredientHighlight = true
                        } label: {
                            Image(systemName: "plus")
                                .font(.system(size: 10, weight: .semibold))
                        }
                        .buttonStyle(.borderless)
                        .help(
                            localized(
                                "Add ingredient highlight",
                                "Lisää ainesosakorostus"
                            )
                        )
                        .popover(isPresented: $isAddingIngredientHighlight) {
                            ingredientHighlightPopover
                        }
                    }

                    highlightedIngredientText
                        .font(.system(size: 12.5))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }

            if !nutritionText.isEmpty {
                detailRow(
                    label: localized("Nutrition", "Ravintoarvot"),
                    value: nutritionText
                )
            }

            if appModel.showCarbonEmissions,
               let co2 = detail.co2KilogramsPer100Grams {
                detailRow(
                    label: "CO₂e",
                    value: String(format: "%.2f kg / 100 g", co2)
                )
            }
        }
        .padding(10)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: 7))
    }

    private var ingredientHighlightPopover: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(localized("Highlight ingredient", "Korosta ainesosa"))
                .font(.headline)

            TextField(
                localized("Ingredient or text", "Ainesosa tai teksti"),
                text: $ingredientHighlight
            )
            .textFieldStyle(.roundedBorder)
            .onSubmit(toggleIngredientHighlight)

            HStack {
                Spacer()
                Button(localized("Cancel", "Peruuta")) {
                    isAddingIngredientHighlight = false
                }
                Button(
                    ingredientHighlightIsSelected
                        ? localized("Remove", "Poista")
                        : localized("Add", "Lisää")
                ) {
                    toggleIngredientHighlight()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(ingredientHighlight.normalizedWhitespace.isEmpty)
            }
        }
        .padding(14)
        .frame(width: 270)
    }

    @ViewBuilder
    private func detailRow(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 12.5))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var nutritionText: String {
        let wanted = [
            ("EnergyKcal", "kcal"),
            ("Protein", localized("protein", "proteiinia")),
            ("Carbohydrates", localized("carbs", "hiilihydraatteja")),
            ("Fat", localized("fat", "rasvaa"))
        ]
        return wanted.compactMap { key, label in
            guard let value = detail.nutrition.first(
                where: { $0.name == key }
            ) else {
                return nil
            }
            return value.displayText(
                amountText: formatAmount(value.amount),
                label: label
            )
        }
        .joined(separator: " · ")
    }

    private var highlightedIngredientText: Text {
        let ingredients = detail.ingredients
        let ranges = TextHighlight.matchingRanges(
            in: ingredients,
            highlights: appModel.highlightedIngredients
        )
        guard !ranges.isEmpty else { return Text(ingredients) }

        var result = Text("")
        var cursor = ingredients.startIndex
        for range in ranges {
            result = result + Text(String(ingredients[cursor..<range.lowerBound]))
            result = result + Text(String(ingredients[range])).underline()
            cursor = range.upperBound
        }
        return result + Text(String(ingredients[cursor...]))
    }

    private func formatAmount(_ amount: Double) -> String {
        if amount.rounded() == amount {
            return String(Int(amount))
        }
        return String(format: "%.1f", amount)
    }

    private var ingredientHighlightIsSelected: Bool {
        appModel.hasExactIngredientHighlight(ingredientHighlight)
    }

    private func toggleIngredientHighlight() {
        let value = ingredientHighlight.normalizedWhitespace
        guard !value.isEmpty else { return }
        appModel.toggleIngredientHighlight(value)
        isAddingIngredientHighlight = false
    }

    private func localized(_ english: String, _ finnish: String) -> String {
        appModel.language == .fi ? finnish : english
    }
}

private struct StatusMessage: View {
    let text: String
    let systemImage: String

    var body: some View {
        Label(text, systemImage: systemImage)
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(9)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
    }
}

private struct ClosureNoticeView: View {
    let closure: SeasonalClosure
    let language: AppLanguage
    let referenceYear: Int

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: "calendar.badge.exclamationmark")
                .foregroundStyle(closureColor)

            Text(
                closure.noticeText(
                    language: language,
                    referenceYear: referenceYear
                )
            )
            .fontWeight(.medium)
            .foregroundStyle(.primary)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            closureColor.opacity(0.11),
            in: RoundedRectangle(cornerRadius: 9)
        )
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .stroke(closureColor.opacity(0.2), lineWidth: 1)
        }
    }

    private var closureColor: Color {
        Color(nsColor: .systemOrange)
    }
}

private struct EmptyStateView: View {
    let title: String
    let description: String
    let systemImage: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: systemImage)
                .font(.system(size: 30))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.headline)
            Text(description)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 280)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(24)
    }
}
