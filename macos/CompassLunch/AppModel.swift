import AppKit
import Foundation

@MainActor
final class AppModel: ObservableObject {
    static let shared = AppModel()

    @Published private(set) var snapshot: MenuSnapshot?
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?

    @Published var selectedRestaurantCode: String {
        didSet {
            defaults.set(selectedRestaurantCode, forKey: Keys.restaurant)
            selectionDidChange()
        }
    }

    @Published var language: AppLanguage {
        didSet {
            defaults.set(language.rawValue, forKey: Keys.language)
            selectionDidChange()
        }
    }

    @Published var showPrices: Bool {
        didSet { defaults.set(showPrices, forKey: Keys.showPrices) }
    }

    @Published var showStudentPrice: Bool {
        didSet { defaults.set(showStudentPrice, forKey: Keys.showStudentPrice) }
    }

    @Published var showStaffPrice: Bool {
        didSet { defaults.set(showStaffPrice, forKey: Keys.showStaffPrice) }
    }

    @Published var showGuestPrice: Bool {
        didSet { defaults.set(showGuestPrice, forKey: Keys.showGuestPrice) }
    }

    @Published var showAllergens: Bool {
        didSet { defaults.set(showAllergens, forKey: Keys.showAllergens) }
    }

    @Published var showCarbonEmissions: Bool {
        didSet {
            defaults.set(showCarbonEmissions, forKey: Keys.showCarbonEmissions)
        }
    }

    @Published var lunchLayout: LunchLayout {
        didSet { defaults.set(lunchLayout.rawValue, forKey: Keys.lunchLayout) }
    }

    @Published private(set) var highlightedMeals: [String] {
        didSet { defaults.set(highlightedMeals, forKey: Keys.highlightedMeals) }
    }

    @Published private(set) var highlightedIngredients: [String] {
        didSet {
            defaults.set(highlightedIngredients, forKey: Keys.highlightedIngredients)
        }
    }

    let restaurants = Restaurant.restaurants

    private let defaults: UserDefaults
    private let cache: CacheStore
    private let service: MenuService
    private var refreshTask: Task<Void, Never>?
    private let refreshInterval: TimeInterval = 4 * 60 * 60

    var selectedRestaurant: Restaurant {
        Restaurant.restaurant(withID: selectedRestaurantCode)
    }

    var selectedRestaurantIndex: Int {
        restaurants.firstIndex(where: { $0.id == selectedRestaurantCode }) ?? 0
    }

    var activeClosure: SeasonalClosure? {
        selectedRestaurant.closure()
    }

    private init(
        defaults: UserDefaults = .standard,
        cache: CacheStore = CacheStore(),
        service: MenuService = MenuService()
    ) {
        self.defaults = defaults
        self.cache = cache
        self.service = service

        let savedRestaurant = defaults.string(forKey: Keys.restaurant) ?? "0437"
        selectedRestaurantCode = Restaurant.restaurants.contains(where: { $0.id == savedRestaurant })
            ? savedRestaurant
            : "0437"
        language = AppLanguage(rawValue: defaults.string(forKey: Keys.language) ?? "") ?? .fi
        showPrices = defaults.object(forKey: Keys.showPrices) as? Bool ?? true
        showStudentPrice = defaults.object(forKey: Keys.showStudentPrice) as? Bool ?? true
        showStaffPrice = defaults.object(forKey: Keys.showStaffPrice) as? Bool ?? true
        showGuestPrice = defaults.object(forKey: Keys.showGuestPrice) as? Bool ?? true
        showAllergens = defaults.object(forKey: Keys.showAllergens) as? Bool ?? true
        showCarbonEmissions =
            defaults.object(forKey: Keys.showCarbonEmissions) as? Bool ?? true
        lunchLayout = LunchLayout(
            rawValue: defaults.string(forKey: Keys.lunchLayout) ?? ""
        ) ?? .standard
        highlightedMeals = defaults.stringArray(forKey: Keys.highlightedMeals) ?? []
        highlightedIngredients =
            defaults.stringArray(forKey: Keys.highlightedIngredients) ?? []

        snapshot = cache.load(
            restaurantCode: selectedRestaurantCode,
            language: language
        )
    }

    func refreshIfNeeded() async {
        if let snapshot,
           snapshot.restaurantCode == selectedRestaurantCode,
           snapshot.language == language,
           (!selectedRestaurant.supportsRecipeDetails
               || snapshot.detailEnrichmentAttempted == true),
           Calendar.current.isDate(snapshot.fetchedAt, inSameDayAs: Date()),
           Date().timeIntervalSince(snapshot.fetchedAt) < refreshInterval {
            return
        }
        await refresh()
    }

    func refresh() async {
        refreshTask?.cancel()
        let restaurant = selectedRestaurant
        let requestedLanguage = language

        isLoading = true
        errorMessage = nil
        if let cached = cache.load(restaurantCode: restaurant.id, language: requestedLanguage) {
            snapshot = cached
        } else {
            snapshot = nil
        }

        let task = Task { [service, cache] in
            do {
                let result = try await service.fetch(
                    restaurant: restaurant,
                    language: requestedLanguage
                )
                guard !Task.isCancelled else { return }
                cache.save(result)
                guard restaurant.id == selectedRestaurantCode,
                      requestedLanguage == language
                else { return }
                snapshot = result
            } catch is CancellationError {
                return
            } catch {
                guard restaurant.id == selectedRestaurantCode,
                      requestedLanguage == language
                else { return }
                errorMessage = localizedFetchError(error)
            }

            if restaurant.id == selectedRestaurantCode,
               requestedLanguage == language {
                isLoading = false
            }
        }
        refreshTask = task
        await task.value
    }

    func openRestaurantPage() {
        guard let url = snapshot?.restaurantURL ?? selectedRestaurant.pageURL else { return }
        NSWorkspace.shared.open(url)
    }

    func displayPrice(for group: LunchGroup) -> String {
        guard showPrices else { return "" }
        guard selectedRestaurant.provider == .compass else {
            return group.normalizedPrice
        }
        return PriceFormatter.displayPrice(
            group.price,
            restaurantCode: selectedRestaurantCode,
            selection: PriceSelection(
                student: showStudentPrice,
                staff: showStaffPrice,
                guest: showGuestPrice
            )
        )
    }

    func mealIsHighlighted(_ meal: String) -> Bool {
        TextHighlight.matches(meal, highlights: highlightedMeals)
    }

    func ingredientsAreHighlighted(_ ingredients: String) -> Bool {
        TextHighlight.matches(ingredients, highlights: highlightedIngredients)
    }

    func hasExactMealHighlight(_ meal: String) -> Bool {
        TextHighlight.containsExact(meal, in: highlightedMeals)
    }

    func toggleMealHighlight(_ meal: String) {
        let value = meal.normalizedWhitespace
        guard !value.isEmpty else { return }
        if let index = highlightedMeals.firstIndex(
            where: {
                TextHighlight.normalized($0) == TextHighlight.normalized(value)
            }
        ) {
            highlightedMeals.remove(at: index)
        } else {
            highlightedMeals.append(value)
        }
    }

    func addMealHighlight(_ value: String) {
        highlightedMeals = addingHighlight(value, to: highlightedMeals)
    }

    func removeMealHighlight(_ value: String) {
        highlightedMeals = removingHighlight(value, from: highlightedMeals)
    }

    func addIngredientHighlight(_ value: String) {
        highlightedIngredients = addingHighlight(value, to: highlightedIngredients)
    }

    func removeIngredientHighlight(_ value: String) {
        highlightedIngredients = removingHighlight(
            value,
            from: highlightedIngredients
        )
    }

    func selectPreviousRestaurant() {
        let previousIndex = (selectedRestaurantIndex - 1 + restaurants.count) % restaurants.count
        selectedRestaurantCode = restaurants[previousIndex].id
    }

    func selectNextRestaurant() {
        let nextIndex = (selectedRestaurantIndex + 1) % restaurants.count
        selectedRestaurantCode = restaurants[nextIndex].id
    }

    @discardableResult
    func selectRestaurant(shortcutNumber: Int) -> Bool {
        let index = shortcutNumber == 0 ? 9 : shortcutNumber - 1
        guard restaurants.indices.contains(index) else { return false }
        selectedRestaurantCode = restaurants[index].id
        return true
    }

    private func selectionDidChange() {
        refreshTask?.cancel()
        errorMessage = nil
        snapshot = cache.load(
            restaurantCode: selectedRestaurantCode,
            language: language
        )
        Task {
            await refresh()
        }
    }

    private func localizedFetchError(_ error: Error) -> String {
        if language == .fi {
            return snapshot == nil
                ? "Ruokalistan päivittäminen epäonnistui: \(error.localizedDescription)"
                : "Päivitys epäonnistui. Näytetään tallennettu ruokalista."
        }
        return snapshot == nil
            ? "Couldn’t update the menu: \(error.localizedDescription)"
            : "Update failed. Showing the cached menu."
    }

    private func addingHighlight(_ value: String, to highlights: [String]) -> [String] {
        let clean = value.normalizedWhitespace
        guard !clean.isEmpty,
              !highlights.contains(where: {
                  TextHighlight.normalized($0) == TextHighlight.normalized(clean)
              })
        else {
            return highlights
        }
        return highlights + [clean]
    }

    private func removingHighlight(_ value: String, from highlights: [String]) -> [String] {
        let key = TextHighlight.normalized(value)
        return highlights.filter { TextHighlight.normalized($0) != key }
    }

    private enum Keys {
        static let restaurant = "restaurantCode"
        static let language = "language"
        static let showPrices = "showPrices"
        static let showStudentPrice = "showStudentPrice"
        static let showStaffPrice = "showStaffPrice"
        static let showGuestPrice = "showGuestPrice"
        static let showAllergens = "showAllergens"
        static let showCarbonEmissions = "showCarbonEmissions"
        static let lunchLayout = "lunchLayout"
        static let highlightedMeals = "highlightedMeals"
        static let highlightedIngredients = "highlightedIngredients"
    }
}
