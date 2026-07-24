import ServiceManagement
import XCTest
@testable import CompassLunch

final class CompassParserTests: XCTestCase {
    func testLunchLayoutTitles() {
        XCTAssertEqual(LunchLayout.legacy.title, "Classic")
        XCTAssertEqual(LunchLayout.standard.title, "Standard")
        XCTAssertEqual(LunchLayout.compact.title, "Compact")
    }

    func testAccentTitles() {
        XCTAssertEqual(
            AppAccent.allCases.map(\.title),
            ["System", "Blue", "Orange", "Graphite"]
        )
    }

    func testAppearanceFollowsTheSystemForEveryAccent() {
        XCTAssertNil(AppAppearance.preferredColorScheme)
    }

    func testOnlyCustomAccentsOverrideTheSystemAccent() {
        XCTAssertFalse(AppAccent.system.overridesSystemAccent)
        XCTAssertTrue(AppAccent.blue.overridesSystemAccent)
        XCTAssertTrue(AppAccent.orange.overridesSystemAccent)
        XCTAssertTrue(AppAccent.graphite.overridesSystemAccent)
    }

    @MainActor
    func testAccentDefaultsToSystemAndPersistsSelection() {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let model = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        XCTAssertEqual(model.accent, .system)

        model.accent = .orange

        let restoredModel = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        XCTAssertEqual(restoredModel.accent, .orange)
    }

    @MainActor
    func testPanelStartsOnTheLunchMenu() {
        XCTAssertFalse(PanelState().isShowingSettings)
    }

    @MainActor
    func testSettingsBindingsWriteThroughToTheAppModel() {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let model = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        let settings = SettingsState(appModel: model)

        settings.binding(\.showAllergens).wrappedValue = false
        settings.binding(\.accent).wrappedValue = .graphite

        XCTAssertFalse(model.showAllergens)
        XCTAssertEqual(model.accent, .graphite)
    }

    @MainActor
    func testSwitchingToAFreshCachedMenuDoesNotFetchAgain() async throws {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        let cacheDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer {
            defaults.removePersistentDomain(forName: suiteName)
            try? FileManager.default.removeItem(at: cacheDirectory)
        }

        defaults.set("0437", forKey: "restaurantCode")
        defaults.set(AppLanguage.en.rawValue, forKey: "language")
        let cache = CacheStore(directory: cacheDirectory)
        cache.save(snapshot(restaurantCode: "0437"))
        cache.save(snapshot(restaurantCode: "0439"))

        RequestCountingURLProtocol.reset()
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [RequestCountingURLProtocol.self]
        let model = AppModel(
            defaults: defaults,
            cache: cache,
            service: MenuService(session: URLSession(configuration: configuration)),
            loginItemService: FakeLoginItemService()
        )

        model.selectedRestaurantCode = "0439"
        for _ in 0..<5 {
            await Task.yield()
        }

        XCTAssertEqual(model.snapshot?.restaurantCode, "0439")
        XCTAssertFalse(model.isLoading)
        XCTAssertEqual(RequestCountingURLProtocol.requestCount, 0)
    }

    @MainActor
    func testCancellingARefreshDoesNotLeaveTheLoadingIndicatorActive() async {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        let cacheDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer {
            defaults.removePersistentDomain(forName: suiteName)
            try? FileManager.default.removeItem(at: cacheDirectory)
            RequestCountingURLProtocol.reset()
        }

        defaults.set("0437", forKey: "restaurantCode")
        defaults.set(AppLanguage.en.rawValue, forKey: "language")
        let cache = CacheStore(directory: cacheDirectory)
        cache.save(snapshot(restaurantCode: "0437"))
        cache.save(snapshot(restaurantCode: "0439"))

        RequestCountingURLProtocol.reset(suspendRequests: true)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [RequestCountingURLProtocol.self]
        let session = URLSession(configuration: configuration)
        defer { session.invalidateAndCancel() }
        let model = AppModel(
            defaults: defaults,
            cache: cache,
            service: MenuService(session: session),
            loginItemService: FakeLoginItemService()
        )

        model.selectedRestaurantCode = "0438"
        for _ in 0..<100 where RequestCountingURLProtocol.requestCount == 0 {
            await Task.yield()
        }
        XCTAssertTrue(model.isLoading)
        XCTAssertGreaterThan(RequestCountingURLProtocol.requestCount, 0)

        model.selectedRestaurantCode = "0439"
        for _ in 0..<5 {
            await Task.yield()
        }

        XCTAssertEqual(model.snapshot?.restaurantCode, "0439")
        XCTAssertFalse(model.isLoading)
    }

    func testParsesAndSortsTodaysMenu() throws {
        let json = """
        {
          "RestaurantName": "Test Restaurant",
          "RestaurantUrl": "https://example.com/menu",
          "MenusForDays": [{
            "Date": "2026-07-24T00:00:00+00:00",
            "LunchTime": "10:30–14:00",
            "SetMenus": [
              {
                "SortOrder": 20,
                "Name": "Lunch",
                "Price": "Student 2,95 €",
                "Components": ["Dish two (G, L)"]
              },
              {
                "SortOrder": 10,
                "Name": "Soup",
                "Price": "Student 1,46 €",
                "Components": ["  Dish one\\n (Veg, G)  "]
              }
            ]
          }],
          "ErrorText": null
        }
        """
        let date = try XCTUnwrap(
            ISO8601DateFormatter().date(from: "2026-07-24T09:00:00+03:00")
        )

        let result = try CompassParser.parse(
            data: Data(json.utf8),
            restaurant: Restaurant.restaurants[0],
            requestedLanguage: .en,
            now: date
        )

        XCTAssertEqual(result.restaurantName, "Test Restaurant")
        XCTAssertEqual(result.menu?.lunchTime, "10:30–14:00")
        XCTAssertEqual(result.menu?.groups.map(\.name), ["Soup", "Lunch"])
        XCTAssertEqual(result.menu?.groups[0].components, ["Dish one (Veg, G)"])
    }

    func testComponentPartsSeparatesKnownDietSuffix() {
        let parts = ComponentParts("Carrot soup (A, G, L, M, Veg)")
        XCTAssertEqual(parts.name, "Carrot soup")
        XCTAssertEqual(parts.diets, "A, G, L, M, Veg")
    }

    func testComponentPartsKeepsOrdinaryParenthesesInDishName() {
        let parts = ComponentParts("Pasta (house style)")
        XCTAssertEqual(parts.name, "Pasta (house style)")
        XCTAssertEqual(parts.diets, "")
    }

    func testComponentPartsExtractsBareTrailingAllergens() {
        let glutenFree = ComponentParts("Manzo Bulgogi (Naudan Pata Korean Tapaan) G")
        XCTAssertEqual(glutenFree.name, "Manzo Bulgogi (Naudan Pata Korean Tapaan)")
        XCTAssertEqual(glutenFree.diets, "G")

        let vegetarian = ComponentParts("Pasta Al Pesto (Pastaa Basilikapestolla) V")
        XCTAssertEqual(vegetarian.name, "Pasta Al Pesto (Pastaa Basilikapestolla)")
        XCTAssertEqual(vegetarian.diets, "V")
    }

    func testMenusWithoutItemsAreNotRenderable() {
        let menu = LunchMenu(
            date: "2026-07-24",
            lunchTime: "",
            groups: [
                LunchGroup(id: "empty", name: "", price: "", components: []),
                LunchGroup(id: "whitespace", name: "Menu", price: "", components: ["  "]),
                LunchGroup(id: "lunch", name: "Lunch", price: "", components: ["Dish"])
            ]
        )

        XCTAssertEqual(menu.groupsWithItems.map(\.id), ["lunch"])
    }

    func testMenuGroupsSortByPriceDescending() {
        let menu = LunchMenu(
            date: "2026-07-24",
            lunchTime: "",
            groups: [
                group(id: "soup", price: "Opiskelija 1,46 €"),
                group(id: "lunch", price: "Opiskelija 2,95 €"),
                group(id: "dessert", price: "Opiskelija 0,66 €"),
                group(id: "vegetarian", price: "Opiskelija 1,87 €")
            ]
        )

        XCTAssertEqual(
            menu.groupsByDescendingPrice.map(\.id),
            ["lunch", "vegetarian", "soup", "dessert"]
        )
    }

    func testConcisePriceRemovesPriceGroupNames() {
        let lunch = group(
            id: "lunch",
            price: "Opiskelija 2,95 € / Henkilökunta 6,19 € / Vierailija 6,22 €"
        )

        XCTAssertEqual(lunch.concisePrice, "2,95 € / 6,19 € / 6,22 €")
    }

    func testPriceNormalizationMatchesTietotekniaRules() {
        XCTAssertEqual(
            PriceFormatter.normalize("13,30€ / opisk.3,100€"),
            "13,30 € / opisk. 3,10 €"
        )
        XCTAssertEqual(
            PriceFormatter.removingGroupNames(from: "13,30€ / opisk.3,100€"),
            "13,30 € / 3,10 €"
        )
    }

    func testPriceNormalizationUsesConsistentSeparators() {
        XCTAssertEqual(
            PriceFormatter.normalize("Opisk. 1,800€ / Henkilökunta 4,59EUR"),
            "Opisk. 1,80 € / Henkilökunta 4,59 €"
        )
        XCTAssertEqual(
            PriceFormatter.normalize("12,50/3,10€"),
            "12,50 / 3,10 €"
        )
    }

    func testPriceGroupsCanBeFiltered() {
        let price = "Opiskelija 2,95 € / Henkilökunta 6,19 € / Vierailija 6,22 €"

        XCTAssertEqual(
            PriceFormatter.displayPrice(
                price,
                restaurantCode: "0437",
                selection: PriceSelection(student: true, staff: false, guest: false)
            ),
            "2,95 €"
        )
        XCTAssertEqual(
            PriceFormatter.displayPrice(
                price,
                restaurantCode: "0437",
                selection: PriceSelection(student: false, staff: true, guest: true)
            ),
            "6,19 € / 6,22 €"
        )
    }

    func testStaffPricesForOtherCompassRestaurants() {
        let staffOnly = PriceSelection(student: false, staff: true, guest: false)
        let cases = [
            (
                "Opiskelija 5,60 € / Henkilökunta 9,05 € / Vierailija 12,50 €",
                "9,05 €"
            ),
            (
                "Student 5,60 € Staff 9,05 € Guest 12,50€",
                "9,05 €"
            )
        ]

        for restaurantCode in ["0436", "043601", "3488"] {
            for (price, expected) in cases {
                XCTAssertEqual(
                    PriceFormatter.displayPrice(
                        price,
                        restaurantCode: restaurantCode,
                        selection: staffOnly
                    ),
                    expected
                )
            }
        }
    }

    func testTietotekniaInfersStudentPriceFromUnlabelledPair() {
        let price = "13,30€ / 3,100€"

        XCTAssertEqual(
            PriceFormatter.displayPrice(
                price,
                restaurantCode: "0439",
                selection: PriceSelection(student: true, staff: false, guest: false)
            ),
            "3,10 €"
        )
        XCTAssertEqual(
            PriceFormatter.displayPrice(
                price,
                restaurantCode: "0439",
                selection: PriceSelection(student: false, staff: true, guest: false)
            ),
            "13,30 €"
        )
    }

    func testTietotekniaSharesUnlabelledPriceBetweenStaffAndGuest() {
        let price = "13,30€ / opisk.3,100€"

        XCTAssertEqual(
            PriceFormatter.displayPrice(
                price,
                restaurantCode: "0439",
                selection: PriceSelection(student: true, staff: false, guest: false)
            ),
            "3,10 €"
        )

        for selection in [
            PriceSelection(student: false, staff: true, guest: false),
            PriceSelection(student: false, staff: false, guest: true),
            PriceSelection(student: false, staff: true, guest: true)
        ] {
            XCTAssertEqual(
                PriceFormatter.displayPrice(
                    price,
                    restaurantCode: "0439",
                    selection: selection
                ),
                "13,30 €"
            )
        }
    }

    func testTietotekniaShowsSinglePricesForStaff() {
        let staffOnly = PriceSelection(student: false, staff: true, guest: false)
        for (price, expected) in [
            ("11,00€", "11,00 €"),
            ("Opisk. 1,80€", "1,80 €")
        ] {
            XCTAssertEqual(
                PriceFormatter.displayPrice(
                    price,
                    restaurantCode: "0439",
                    selection: staffOnly
                ),
                expected
            )
        }
    }

    @MainActor
    func testPriceMasterToggleTracksGroupSelection() {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let model = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )

        model.showStudentPrice = false
        model.showStaffPrice = false
        model.showGuestPrice = false
        XCTAssertFalse(model.showPrices)

        model.showPrices = true
        XCTAssertTrue(model.showStudentPrice)
        XCTAssertTrue(model.showStaffPrice)
        XCTAssertTrue(model.showGuestPrice)
    }

    @MainActor
    func testLaunchAtLoginIsEnabledOnceByDefault() {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let loginItem = FakeLoginItemService()
        let model = AppModel(
            defaults: defaults,
            loginItemService: loginItem
        )

        model.configureLaunchAtLoginIfNeeded()

        XCTAssertTrue(model.launchAtLogin)
        XCTAssertEqual(loginItem.registerCount, 1)
        XCTAssertTrue(defaults.bool(forKey: "launchAtLoginConfigured"))

        model.configureLaunchAtLoginIfNeeded()
        XCTAssertEqual(loginItem.registerCount, 1)
    }

    @MainActor
    func testDisablingLaunchAtLoginPersistsTheChoice() {
        let suiteName = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let loginItem = FakeLoginItemService(status: .enabled)
        let model = AppModel(
            defaults: defaults,
            loginItemService: loginItem
        )

        model.setLaunchAtLogin(false)

        XCTAssertFalse(model.launchAtLogin)
        XCTAssertEqual(loginItem.unregisterCount, 1)

        let nextLoginItem = FakeLoginItemService()
        let nextModel = AppModel(
            defaults: defaults,
            loginItemService: nextLoginItem
        )
        nextModel.configureLaunchAtLoginIfNeeded()
        XCTAssertEqual(nextLoginItem.registerCount, 0)
    }

    func testSeasonalClosureUsesInclusiveDates() {
        let canthia = Restaurant.restaurant(withID: "0436")

        XCTAssertNil(canthia.closure(on: LocalDate(year: 2026, month: 6, day: 17)))
        XCTAssertNotNil(canthia.closure(on: LocalDate(year: 2026, month: 6, day: 18)))
        XCTAssertNotNil(canthia.closure(on: LocalDate(year: 2026, month: 8, day: 9)))
        XCTAssertNil(canthia.closure(on: LocalDate(year: 2026, month: 8, day: 10)))
    }

    func testClosureScheduleAlreadyContainsFutureRestaurantProviders() {
        XCTAssertFalse(ClosureSchedule.periods(for: "snellari-rss").isEmpty)
        XCTAssertFalse(ClosureSchedule.periods(for: "antell-round").isEmpty)
        XCTAssertFalse(ClosureSchedule.periods(for: "antell-highway").isEmpty)
        XCTAssertFalse(ClosureSchedule.periods(for: "huomen-bioteknia").isEmpty)
    }

    func testRestaurantCatalogMatchesWindowsShortcutOrder() {
        XCTAssertEqual(
            Restaurant.restaurants.map(\.id),
            [
                "0437",
                "snellari-rss",
                "0436",
                "0439",
                "huomen-bioteknia",
                "antell-round",
                "antell-highway",
                "043601",
                "pranzeria-html",
                "3488"
            ]
        )
    }

    func testParsesSnellariRSSFixture() throws {
        let restaurant = Restaurant.restaurant(withID: "snellari-rss")
        let result = try RSSMenuParser.parse(
            data: fixture("snellari.rss"),
            restaurant: restaurant,
            requestedLanguage: .fi,
            now: localDate(year: 2026, month: 2, day: 23)
        )

        XCTAssertEqual(result.restaurantName, "Cafe Snellari")
        XCTAssertEqual(result.menu?.groups.first?.components.first, "Juustoista peruna-pinaattisosekeittoa (*, A, G, ILM, L)")
        XCTAssertEqual(result.menu?.groups.first?.components.count, 4)
        XCTAssertEqual(result.menu?.groups.first?.price, "")
    }

    func testRejectsStaleSnellariRSSFixture() throws {
        let result = try RSSMenuParser.parse(
            data: fixture("snellari.rss"),
            restaurant: Restaurant.restaurant(withID: "snellari-rss"),
            requestedLanguage: .fi,
            now: localDate(year: 2026, month: 2, day: 24)
        )

        XCTAssertNil(result.menu)
    }

    func testParsesHuomenFixture() throws {
        let result = try HuomenMenuParser.parse(
            data: fixture("huomen.json"),
            restaurant: Restaurant.restaurant(withID: "huomen-bioteknia"),
            requestedLanguage: .fi,
            now: localDate(year: 2026, month: 2, day: 23)
        )

        XCTAssertEqual(result.restaurantName, "Hyvä Huomen Bioteknia")
        XCTAssertEqual(
            result.menu?.groups.first?.components,
            [
                "Kermainen juuresosekeitto (G, L)",
                "Lihapullia, pippuri-rakuunakastiketta ja kermaperunaa (G, L)",
                "Kasvispihvejä, tsatsikia (L)"
            ]
        )
        XCTAssertEqual(
            result.menu?.groups.first?.normalizedPrice,
            "Lounas 12,90 € / Keittolounas 10,90 €"
        )
    }

    func testParsesAntellFixtures() throws {
        let round = try AntellMenuParser.parse(
            data: fixture("antell-round-friday-snippet.html"),
            restaurant: Restaurant.restaurant(withID: "antell-round"),
            requestedLanguage: .fi,
            now: localDate(year: 2026, month: 2, day: 20)
        )
        let highway = try AntellMenuParser.parse(
            data: fixture("antell-highway-friday-snippet.html"),
            restaurant: Restaurant.restaurant(withID: "antell-highway"),
            requestedLanguage: .fi,
            now: localDate(year: 2026, month: 2, day: 20)
        )

        XCTAssertEqual(round.menu?.groups.count, 3)
        XCTAssertEqual(round.menu?.groups.first?.name, "Kotiruokalounas")
        XCTAssertEqual(round.menu?.groups.first?.normalizedPrice, "12,50 / 3,10 €")
        XCTAssertEqual(
            round.menu?.groups.first?.components.first,
            "Perinteiset lihapyörykät mummonkastikkeella(G oma)"
        )
        XCTAssertEqual(highway.menu?.groups.count, 3)
        XCTAssertEqual(highway.menu?.groups.first?.name, "Pääruoaksi")
        XCTAssertEqual(highway.menu?.groups.first?.normalizedPrice, "13,90 €")
        XCTAssertEqual(
            highway.menu?.groups.first?.components.first,
            "Hoisin-kastikkeella maustettuja nyhtöpossuhodareita (A, L, M)"
        )
    }

    func testParsesPranzeriaFixture() throws {
        let result = try PranzeriaMenuParser.parse(
            data: fixture("pranzeria-snippet.html"),
            restaurant: Restaurant.restaurant(withID: "pranzeria-html"),
            requestedLanguage: .fi,
            now: localDate(year: 2026, month: 3, day: 20)
        )
        let components = try XCTUnwrap(result.menu?.groups.first?.components)

        XCTAssertEqual(components.first, "Salaatti- &AntipastoBuffet")
        XCTAssertTrue(components.contains(where: { $0.contains("Spezzatino Di Manzo") }))
        XCTAssertTrue(components.contains("Roomalainen focacciapizzabuffet"))
        XCTAssertFalse(components.contains(where: { $0.contains("Laktoositon") }))

        let bareAllergen = ComponentParts("Manzo Bulgogi (Naudan Pata Korean Tapaan) G")
        XCTAssertEqual(bareAllergen.diets, "G")
    }

    func testCompassRecipeDetailsAreMatchedToMeals() throws {
        let page = """
        <script>
        window.__INITIAL_MENU__ = {
          "dayMenu": {
            "menuPackages": [{
              "meals": [{"name": "Carrot soup", "recipeId": 42}]
            }]
          }
        };
        </script>
        """
        let references = CompassRecipeDetailParser.references(
            fromRestaurantHTML: page
        )
        XCTAssertEqual(references["carrot soup"], 42)

        let payload = """
        {
          "recipeId": 42,
          "name": "Carrot soup",
          "ingredientsCleaned": "Carrot, water, salt",
          "nutritionalValues": [
            {"name": "Protein", "amount": 2.5, "unit": "g"}
          ],
          "kgCO2ePer100g": 0.12,
          "diets": "G, L"
        }
        """
        let detail = try XCTUnwrap(
            CompassRecipeDetailParser.parse(
                data: Data(payload.utf8),
                fallbackRecipeID: 42
            )
        )
        let snapshot = MenuSnapshot(
            restaurantCode: "0437",
            restaurantName: "Test",
            restaurantURL: nil,
            language: .en,
            fetchedAt: Date(),
            menu: LunchMenu(
                date: "2026-07-24",
                lunchTime: "",
                groups: [
                    LunchGroup(
                        id: "soup",
                        name: "Soup",
                        price: "",
                        components: ["Carrot soup (G, L)"]
                    )
                ]
            )
        )
        let enriched = RecipeDetailEnrichment.applying(
            ["carrot soup": detail],
            to: snapshot
        )

        XCTAssertEqual(
            enriched.menu?.groups.first?.detail(at: 0)?.ingredients,
            "Carrot, water, salt"
        )
        XCTAssertEqual(
            enriched.menu?.groups.first?.detail(at: 0)?.nutrition.first?.amount,
            2.5
        )
    }

    func testParsesAntellRecipeDetails() throws {
        let html = """
        <section id="panel-Tuesday">
          <ul class="accordion__list">
            <li>
              <button class="accordion__button">Jauhelihatacoja</button>
              <div class="accordion__content">
                <p>Ravintoarvot (100 g): 160 kcal energiaa, 7.8 g proteiinia</p>
                <p>Hiilijalanjälki: 0.29 CO₂ e kg/100g</p>
                <div class="tooltip">
                  <div class="tooltip__body">Naudanliha, sipuli, suola</div>
                </div>
              </div>
              <div class="accordion__footer__special-diets"><p>G, L, M</p></div>
            </li>
          </ul>
        </section>
        """
        let details = AntellRecipeDetailParser.details(
            from: html,
            weekday: "tuesday"
        )
        let detail = try XCTUnwrap(details["jauhelihatacoja"])

        XCTAssertEqual(detail.ingredients, "Naudanliha, sipuli, suola")
        XCTAssertEqual(detail.co2KilogramsPer100Grams, 0.29)
        XCTAssertTrue(
            detail.nutrition.contains {
                $0.name == "Protein" && abs($0.amount - 7.8) < 0.001
            }
        )
    }

    func testRecipeDetailsRemainBackwardCompatibleWithOldCaches() throws {
        let json = """
        {
          "id": "lunch",
          "name": "Lunch",
          "price": "",
          "components": ["Dish"]
        }
        """
        let group = try JSONDecoder().decode(
            LunchGroup.self,
            from: Data(json.utf8)
        )
        XCTAssertNil(group.componentDetails)
    }

    func testMealAndIngredientHighlightsUseIndependentSubstringMatching() {
        XCTAssertTrue(
            TextHighlight.matches(
                "Kermainen sipulikeitto",
                highlights: ["SIPULI"]
            )
        )
        XCTAssertTrue(
            TextHighlight.matches(
                "Crème fraîche, valkosipuli",
                highlights: ["creme"]
            )
        )
        XCTAssertFalse(
            TextHighlight.matches(
                "Carrot soup",
                highlights: ["chicken"]
            )
        )
    }

    func testIngredientHighlightRangesFindAndMergeVisibleMatches() {
        let ingredients = "Crème fraîche, onion, ONION"
        let ranges = TextHighlight.matchingRanges(
            in: ingredients,
            highlights: ["creme", "onion", "ion"]
        )

        XCTAssertEqual(
            ranges.map { String(ingredients[$0]) },
            ["Crème", "onion", "ONION"]
        )
    }

    func testNutritionDisplayDoesNotDuplicateUnits() {
        XCTAssertEqual(
            NutritionValue(
                name: "EnergyKcal",
                amount: 93,
                unit: "kcal"
            ).displayText(amountText: "93", label: "kcal"),
            "93 kcal"
        )
        XCTAssertEqual(
            NutritionValue(
                name: "EnergyKcal",
                amount: 93,
                unit: ""
            ).displayText(amountText: "93", label: "kcal"),
            "93 kcal"
        )
        XCTAssertEqual(
            NutritionValue(
                name: "Protein",
                amount: 3.9,
                unit: "g"
            ).displayText(amountText: "3.9", label: "protein"),
            "3.9 g protein"
        )
    }

    private func group(id: String, price: String) -> LunchGroup {
        LunchGroup(
            id: id,
            name: id,
            price: price,
            components: ["Dish"]
        )
    }

    private func snapshot(restaurantCode: String) -> MenuSnapshot {
        MenuSnapshot(
            restaurantCode: restaurantCode,
            restaurantName: "Test",
            restaurantURL: nil,
            language: .en,
            fetchedAt: Date(),
            menu: LunchMenu(
                date: "2026-07-24",
                lunchTime: "10:30–14:00",
                groups: [group(id: "lunch", price: "3,10 €")]
            ),
            detailEnrichmentAttempted: true
        )
    }

    private func fixture(_ name: String) throws -> Data {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return try Data(
            contentsOf: repository
                .appendingPathComponent("plasma6/tests/fixtures")
                .appendingPathComponent(name)
        )
    }

    private func localDate(year: Int, month: Int, day: Int) -> Date {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .current
        return calendar.date(
            from: DateComponents(
                year: year,
                month: month,
                day: day,
                hour: 12
            )
        )!
    }
}

private final class FakeLoginItemService: LoginItemService {
    var status: SMAppService.Status
    private(set) var registerCount = 0
    private(set) var unregisterCount = 0

    init(status: SMAppService.Status = .notRegistered) {
        self.status = status
    }

    func register() throws {
        registerCount += 1
        status = .enabled
    }

    func unregister() throws {
        unregisterCount += 1
        status = .notRegistered
    }

    func openSystemSettings() {}
}

private final class RequestCountingURLProtocol: URLProtocol {
    private static let lock = NSLock()
    private static var count = 0
    private static var suspendsRequests = false

    static var requestCount: Int {
        lock.withLock { count }
    }

    static func reset(suspendRequests: Bool = false) {
        lock.withLock {
            count = 0
            suspendsRequests = suspendRequests
        }
    }

    override class func canInit(with request: URLRequest) -> Bool {
        lock.withLock { count += 1 }
        return true
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        let shouldSuspend = Self.lock.withLock { Self.suspendsRequests }
        guard !shouldSuspend else { return }
        client?.urlProtocol(
            self,
            didFailWithError: URLError(.cannotConnectToHost)
        )
    }

    override func stopLoading() {}
}
