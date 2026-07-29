import AppKit
import Foundation
import ServiceManagement

protocol LoginItemService: AnyObject {
    var status: SMAppService.Status { get }
    func register() throws
    func unregister() throws
    func openSystemSettings()
}

final class SystemLoginItemService: LoginItemService {
    private let service = SMAppService.mainApp

    var status: SMAppService.Status {
        service.status
    }

    func register() throws {
        try service.register()
    }

    func unregister() throws {
        try service.unregister()
    }

    func openSystemSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }
}

struct BackgroundPreloadAttempt: Codable, Equatable {
    var count: Int
    var lastAttempt: Date
}

enum MenuPreloadPolicy {
    static let maximumAttempts = 3
    static let retryInterval: TimeInterval = 60 * 60
    static let cutoffHour = 15

    static func permits(
        restaurantID: String,
        now: Date,
        calendar: Calendar = helsinkiCalendar
    ) -> Bool {
        let components = calendar.dateComponents([.weekday, .hour], from: now)
        guard (components.hour ?? cutoffHour) < cutoffHour else { return false }
        switch components.weekday {
        case 1:
            return false
        case 7:
            return restaurantID == "snellmania"
        default:
            return true
        }
    }

    static func shouldAttempt(
        snapshot: MenuSnapshot?,
        attempt: BackgroundPreloadAttempt?,
        now: Date,
        calendar: Calendar = helsinkiCalendar
    ) -> Bool {
        if let snapshot,
           calendar.isDate(snapshot.fetchedAt, inSameDayAs: now),
           snapshot.isStale != true {
            switch snapshot.effectiveServiceStatus {
            case .serving, .closed:
                return false
            case .noMenu, .unknown:
                break
            }
        }
        guard (attempt?.count ?? 0) < maximumAttempts else { return false }
        let currentSnapshotDate = snapshot.flatMap {
            calendar.isDate($0.fetchedAt, inSameDayAs: now)
                ? $0.fetchedAt
                : nil
        }
        let mostRecent = [attempt?.lastAttempt, currentSnapshotDate]
            .compactMap { $0 }
            .max()
        return mostRecent.map { now.timeIntervalSince($0) >= retryInterval } ?? true
    }

    static var helsinkiCalendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Helsinki")!
        return calendar
    }
}

enum ManualRefreshPolicy {
    static let cooldown: TimeInterval = 15 * 60

    static func permits(lastRefresh: Date?, now: Date) -> Bool {
        guard let lastRefresh else { return true }
        return now.timeIntervalSince(lastRefresh) >= cooldown
    }
}

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
            preloadGeneration &+= 1
            selectionDidChange()
            Task {
                await prepareMenusInBackground()
            }
        }
    }

    @Published var showPrices: Bool {
        didSet {
            if showPrices && !hasSelectedPriceGroup {
                showStudentPrice = true
                showStaffPrice = true
                showGuestPrice = true
            }
            defaults.set(showPrices, forKey: Keys.showPrices)
        }
    }

    @Published var showStudentPrice: Bool {
        didSet {
            defaults.set(showStudentPrice, forKey: Keys.showStudentPrice)
            disablePricesIfNoGroupIsSelected()
        }
    }

    @Published var showStaffPrice: Bool {
        didSet {
            defaults.set(showStaffPrice, forKey: Keys.showStaffPrice)
            disablePricesIfNoGroupIsSelected()
        }
    }

    @Published var showGuestPrice: Bool {
        didSet {
            defaults.set(showGuestPrice, forKey: Keys.showGuestPrice)
            disablePricesIfNoGroupIsSelected()
        }
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

    @Published var accent: AppAccent {
        didSet { defaults.set(accent.rawValue, forKey: Keys.accent) }
    }

    @Published private(set) var launchAtLogin = false
    @Published private(set) var launchAtLoginRequiresApproval = false

    @Published private(set) var highlightedMeals: [String] {
        didSet { defaults.set(highlightedMeals, forKey: Keys.highlightedMeals) }
    }

    @Published private(set) var highlightedIngredients: [String] {
        didSet {
            defaults.set(highlightedIngredients, forKey: Keys.highlightedIngredients)
        }
    }

    @Published private(set) var restaurants: [Restaurant]

    private let defaults: UserDefaults
    private let cache: CacheStore
    private let service: MenuService
    private let loginItemService: LoginItemService
    private let nowProvider: () -> Date
    private var refreshTask: Task<Void, Never>?
    private var refreshGeneration = 0
    private var isPreloading = false
    private var preloadGeneration = 0
    private var preloadAttempts: [String: BackgroundPreloadAttempt] = [:]
    private var lastManualRefresh: Date?
    private var inFlightFetches: [String: InFlightFetch] = [:]
    @Published private var cooldownRevision = 0

    private struct InFlightFetch {
        let id: UUID
        let task: Task<MenuSnapshot, Error>
    }

    var selectedRestaurant: Restaurant {
        restaurants.first(where: { $0.id == selectedRestaurantCode })
            ?? restaurants[0]
    }

    var selectedRestaurantIndex: Int {
        restaurants.firstIndex(where: { $0.id == selectedRestaurantCode }) ?? 0
    }

    var activeClosure: SeasonalClosure? {
        snapshot?.closure
    }

    init(
        defaults: UserDefaults = .standard,
        cache: CacheStore = CacheStore(),
        service: MenuService = MenuService(),
        loginItemService: LoginItemService = SystemLoginItemService(),
        nowProvider: @escaping () -> Date = Date.init
    ) {
        self.defaults = defaults
        self.cache = cache
        self.service = service
        self.loginItemService = loginItemService
        self.nowProvider = nowProvider
        restaurants = Restaurant.fallbackRestaurants

        let savedRestaurant = Restaurant.migratedID(
            defaults.string(forKey: Keys.restaurant) ?? "snellmania"
        )
        selectedRestaurantCode = Restaurant.fallbackRestaurants.contains(
            where: { $0.id == savedRestaurant }
        ) ? savedRestaurant : "snellmania"
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
        accent = AppAccent(
            rawValue: defaults.string(forKey: Keys.accent) ?? ""
        ) ?? .system
        highlightedMeals = defaults.stringArray(forKey: Keys.highlightedMeals) ?? []
        highlightedIngredients =
            defaults.stringArray(forKey: Keys.highlightedIngredients) ?? []
        defaults.set(selectedRestaurantCode, forKey: Keys.restaurant)
        preloadAttempts = Self.decode(
            [String: BackgroundPreloadAttempt].self,
            from: defaults.data(forKey: Keys.preloadAttempts)
        ) ?? [:]
        lastManualRefresh = Self.decode(
            Date.self,
            from: defaults.data(forKey: Keys.lastManualRefresh)
        ) ?? Self.decode(
            [String: Date].self,
            from: defaults.data(forKey: Keys.legacyManualRefreshDates)
        )?.values.max()

        if showPrices && !hasSelectedPriceGroup {
            showPrices = false
            defaults.set(false, forKey: Keys.showPrices)
        }

        refreshLaunchAtLoginStatus()

        snapshot = cache.load(
            restaurantCode: selectedRestaurantCode,
            language: language
        )
        scheduleManualRefreshCooldownUpdateIfNeeded()
    }

    func refreshIfNeeded() async {
        await refreshIfNeeded(
            loadCachedSnapshot: true,
            allowUnpublishedRetry: true
        )
    }

    private func refreshIfNeeded(
        loadCachedSnapshot: Bool,
        allowUnpublishedRetry: Bool
    ) async {
        if let snapshot,
           snapshot.restaurantCode == selectedRestaurantCode,
           snapshot.language == language,
           Calendar.current.isDate(
               snapshot.fetchedAt,
               inSameDayAs: nowProvider()
           ) {
            if snapshot.isStale == true {
                if !allowUnpublishedRetry ||
                    nowProvider().timeIntervalSince(snapshot.fetchedAt) <
                    noMenuForegroundRetryInterval {
                    return
                }
            } else {
                switch snapshot.effectiveServiceStatus {
                case .serving, .closed:
                    return
                case .noMenu, .unknown:
                    if !allowUnpublishedRetry ||
                        nowProvider().timeIntervalSince(snapshot.fetchedAt) <
                        noMenuForegroundRetryInterval {
                        return
                    }
                }
            }
        }
        await performRefresh(
            loadCachedSnapshot: loadCachedSnapshot,
            cachePolicy: .useProtocolCachePolicy
        )
    }

    var canRefreshSelectedRestaurant: Bool {
        _ = cooldownRevision
        return ManualRefreshPolicy.permits(
            lastRefresh: lastManualRefresh,
            now: nowProvider()
        )
    }

    func refresh() async {
        guard canRefreshSelectedRestaurant else { return }
        beginManualRefreshCooldown()
        await performRefresh(
            loadCachedSnapshot: true,
            cachePolicy: .reloadRevalidatingCacheData
        )
    }

    private func performRefresh(
        loadCachedSnapshot: Bool,
        cachePolicy: URLRequest.CachePolicy
    ) async {
        refreshTask?.cancel()
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let restaurant = selectedRestaurant
        let requestedLanguage = language

        isLoading = true
        errorMessage = nil
        if loadCachedSnapshot {
            snapshot = cache.load(
                restaurantCode: restaurant.id,
                language: requestedLanguage
            )
        } else if snapshot?.restaurantCode != restaurant.id
                    || snapshot?.language != requestedLanguage {
            snapshot = nil
        }

        let task = Task { [weak self] in
            guard let self else { return }
            defer {
                if generation == refreshGeneration {
                    isLoading = false
                }
            }

            do {
                let result = try await fetchSnapshot(
                    restaurant: restaurant,
                    language: requestedLanguage,
                    cachePolicy: cachePolicy
                )
                guard !Task.isCancelled else { return }
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
        }
        refreshTask = task
        await task.value
    }

    func prepareMenusInBackground() async {
        guard !isPreloading else { return }
        isPreloading = true
        defer { isPreloading = false }

        let generation = preloadGeneration
        let requestedLanguage = language
        let now = nowProvider()
        prunePreloadAttempts(now: now)

        let candidates = restaurants.filter { restaurant in
            guard MenuPreloadPolicy.permits(
                restaurantID: restaurant.id,
                now: now
            ) else {
                return false
            }
            let cached = cache.load(
                restaurantCode: restaurant.id,
                language: requestedLanguage
            )
            let key = preloadKey(
                restaurantID: restaurant.id,
                language: requestedLanguage,
                now: now
            )
            if preloadAttempts[key] == nil,
               let cached,
               cached.effectiveServiceStatus == .noMenu,
               MenuPreloadPolicy.helsinkiCalendar.isDate(
                   cached.fetchedAt,
                   inSameDayAs: now
               ) {
                preloadAttempts[key] = BackgroundPreloadAttempt(
                    count: 1,
                    lastAttempt: cached.fetchedAt
                )
            }
            return MenuPreloadPolicy.shouldAttempt(
                snapshot: cached,
                attempt: preloadAttempts[key],
                now: now
            )
        }
        guard !candidates.isEmpty else { return }

        for restaurant in candidates {
            let key = preloadKey(
                restaurantID: restaurant.id,
                language: requestedLanguage,
                now: now
            )
            let previous = preloadAttempts[key]
            preloadAttempts[key] = BackgroundPreloadAttempt(
                count: (previous?.count ?? 0) + 1,
                lastAttempt: now
            )
        }
        persistPreloadAttempts()

        guard let daily = try? await service.fetchDailySnapshot(
            language: requestedLanguage,
            now: now
        ), generation == preloadGeneration,
           requestedLanguage == language
        else {
            return
        }

        restaurants = daily.restaurants
        for menu in daily.menus {
            cache.save(menu)
        }
        if let selected = daily.menus.first(where: {
            $0.restaurantCode == selectedRestaurantCode
        }) {
            snapshot = selected
            errorMessage = nil
        }
    }

    private func fetchSnapshot(
        restaurant: Restaurant,
        language: AppLanguage,
        cachePolicy: URLRequest.CachePolicy
    ) async throws -> MenuSnapshot {
        let key = [
            restaurant.id,
            language.rawValue,
            localDateKey(nowProvider()),
            String(cachePolicy.rawValue)
        ].joined(separator: "|")
        if let inFlight = inFlightFetches[key] {
            let result = try await inFlight.task.value
            cache.save(result)
            return result
        }

        let id = UUID()
        let task = Task { [service] in
            try await service.fetch(
                restaurant: restaurant,
                language: language,
                now: nowProvider(),
                cachePolicy: cachePolicy
            )
        }
        inFlightFetches[key] = InFlightFetch(id: id, task: task)
        defer {
            if inFlightFetches[key]?.id == id {
                inFlightFetches.removeValue(forKey: key)
            }
        }
        let result = try await task.value
        cache.save(result)
        return result
    }

    func openRestaurantPage() {
        guard let url = snapshot?.restaurantURL ?? selectedRestaurant.pageURL else { return }
        NSWorkspace.shared.open(url)
    }

    func configureLaunchAtLoginIfNeeded() {
        guard !defaults.bool(forKey: Keys.launchAtLoginConfigured) else {
            refreshLaunchAtLoginStatus()
            return
        }

        do {
            switch loginItemService.status {
            case .enabled, .requiresApproval:
                break
            case .notRegistered, .notFound:
                try loginItemService.register()
            @unknown default:
                try loginItemService.register()
            }
            defaults.set(true, forKey: Keys.launchAtLoginConfigured)
        } catch {
            NSLog("Could not enable launch at login: %@", error.localizedDescription)
        }
        refreshLaunchAtLoginStatus()
    }

    func setLaunchAtLogin(_ enabled: Bool) {
        do {
            if enabled {
                switch loginItemService.status {
                case .enabled:
                    break
                case .requiresApproval:
                    loginItemService.openSystemSettings()
                case .notRegistered, .notFound:
                    try loginItemService.register()
                @unknown default:
                    try loginItemService.register()
                }
            } else if loginItemService.status != .notRegistered {
                try loginItemService.unregister()
            }
            defaults.set(true, forKey: Keys.launchAtLoginConfigured)
        } catch {
            NSLog("Could not update launch at login: %@", error.localizedDescription)
        }
        refreshLaunchAtLoginStatus()
    }

    func openLoginItemsSettings() {
        loginItemService.openSystemSettings()
    }

    func refreshLaunchAtLoginStatus() {
        switch loginItemService.status {
        case .enabled:
            launchAtLogin = true
            launchAtLoginRequiresApproval = false
        case .requiresApproval:
            launchAtLogin = true
            launchAtLoginRequiresApproval = true
        case .notRegistered, .notFound:
            launchAtLogin = false
            launchAtLoginRequiresApproval = false
        @unknown default:
            launchAtLogin = false
            launchAtLoginRequiresApproval = false
        }
    }

    func displayPrice(for group: LunchGroup) -> String {
        guard showPrices else { return "" }
        let selection = PriceSelection(
            student: showStudentPrice,
            staff: showStaffPrice,
            guest: showGuestPrice
        )
        if let prices = group.prices {
            var seen = Set<String>()
            return prices
                .filter { $0.isVisible(for: selection) }
                .map(\.displayText)
                .filter { seen.insert($0).inserted }
                .joined(separator: " / ")
        }
        return PriceFormatter.displayPrice(
            group.price,
            restaurantCode: selectedRestaurantCode,
            selection: selection
        )
    }

    func displayPrice(for offer: LunchOffer) -> String {
        showPrices ? offer.price.displayText : ""
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
        refreshGeneration &+= 1
        isLoading = false
        errorMessage = nil
        snapshot = cache.load(
            restaurantCode: selectedRestaurantCode,
            language: language
        )
        scheduleManualRefreshCooldownUpdateIfNeeded()
        Task {
            await refreshIfNeeded(
                loadCachedSnapshot: false,
                allowUnpublishedRetry: true
            )
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

    private var hasSelectedPriceGroup: Bool {
        showStudentPrice || showStaffPrice || showGuestPrice
    }

    private func disablePricesIfNoGroupIsSelected() {
        if showPrices && !hasSelectedPriceGroup {
            showPrices = false
        }
    }

    private var noMenuForegroundRetryInterval: TimeInterval {
        30 * 60
    }

    private func beginManualRefreshCooldown() {
        let now = nowProvider()
        lastManualRefresh = now
        defaults.set(
            Self.encode(now),
            forKey: Keys.lastManualRefresh
        )
        defaults.removeObject(forKey: Keys.legacyManualRefreshDates)
        cooldownRevision &+= 1
        scheduleManualRefreshCooldownUpdateIfNeeded(now: now)
    }

    private func scheduleManualRefreshCooldownUpdateIfNeeded(
        now: Date? = nil
    ) {
        let now = now ?? nowProvider()
        guard let lastRefresh = lastManualRefresh else {
            return
        }
        let remaining = ManualRefreshPolicy.cooldown -
            now.timeIntervalSince(lastRefresh)
        guard remaining > 0 else { return }

        Task { [weak self] in
            try? await Task.sleep(
                nanoseconds: UInt64(remaining * 1_000_000_000)
            )
            guard !Task.isCancelled else { return }
            self?.cooldownRevision &+= 1
        }
    }

    private func preloadKey(
        restaurantID: String,
        language: AppLanguage,
        now: Date
    ) -> String {
        "\(localDateKey(now))|\(restaurantID)|\(language.rawValue)"
    }

    private func prunePreloadAttempts(now: Date) {
        let prefix = "\(localDateKey(now))|"
        let retained = preloadAttempts.filter { $0.key.hasPrefix(prefix) }
        guard retained != preloadAttempts else { return }
        preloadAttempts = retained
        persistPreloadAttempts()
    }

    private func persistPreloadAttempts() {
        defaults.set(
            Self.encode(preloadAttempts),
            forKey: Keys.preloadAttempts
        )
    }

    private func localDateKey(_ date: Date) -> String {
        let components = MenuPreloadPolicy.helsinkiCalendar.dateComponents(
            [.year, .month, .day],
            from: date
        )
        return String(
            format: "%04d-%02d-%02d",
            components.year ?? 0,
            components.month ?? 0,
            components.day ?? 0
        )
    }

    private static func encode<Value: Encodable>(_ value: Value) -> Data? {
        try? JSONEncoder().encode(value)
    }

    private static func decode<Value: Decodable>(
        _ type: Value.Type,
        from data: Data?
    ) -> Value? {
        guard let data else { return nil }
        return try? JSONDecoder().decode(type, from: data)
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
        static let accent = "accent"
        static let launchAtLoginConfigured = "launchAtLoginConfigured"
        static let highlightedMeals = "highlightedMeals"
        static let highlightedIngredients = "highlightedIngredients"
        static let preloadAttempts = "backgroundPreloadAttempts"
        static let lastManualRefresh = "lastManualRefresh"
        static let legacyManualRefreshDates = "manualRefreshDates"
    }
}
