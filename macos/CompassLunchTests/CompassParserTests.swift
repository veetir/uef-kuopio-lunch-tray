import XCTest
@testable import CompassLunch

final class CompassParserTests: XCTestCase {
    func testLunchLayoutTitles() {
        XCTAssertEqual(LunchLayout.legacy.title, "Classic")
        XCTAssertEqual(LunchLayout.standard.title, "Standard")
        XCTAssertEqual(LunchLayout.compact.title, "Compact")
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
        XCTAssertEqual(
            round.menu?.groups.first?.components.first,
            "Perinteiset lihapyörykät mummonkastikkeella(G oma)"
        )
        XCTAssertEqual(highway.menu?.groups.count, 3)
        XCTAssertEqual(highway.menu?.groups.first?.name, "Pääruoaksi")
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

    private func group(id: String, price: String) -> LunchGroup {
        LunchGroup(
            id: id,
            name: id,
            price: price,
            components: ["Dish"]
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
