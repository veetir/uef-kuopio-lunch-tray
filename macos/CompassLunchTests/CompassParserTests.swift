import AppKit
import ServiceManagement
import XCTest
@testable import CompassLunch

final class CompassParserTests: XCTestCase {
    @MainActor
    func testPanelStateRunsConfiguredDismissAction() {
        let panelState = PanelState()
        var didDismiss = false
        panelState.onDismissPanel = {
            didDismiss = true
        }

        panelState.dismissPanel()

        XCTAssertTrue(didDismiss)
    }

    func testLunchPanelFindsMenuScrollViewAfterSettingsScrollView() {
        let root = NSView()

        let settingsScrollView = NSScrollView()
        settingsScrollView.documentView = NSView()
        root.addSubview(settingsScrollView)

        let menuDocumentView = NSView()
        menuDocumentView.addSubview(MenuScrollViewMarkerView())
        let menuScrollView = NSScrollView()
        menuScrollView.documentView = menuDocumentView
        root.addSubview(menuScrollView)

        XCTAssertTrue(MenuScrollViewFinder.find(in: root) === menuScrollView)
    }

    func testMenuItemScrollingMovesDownToTailAndBackWithoutSkipping() {
        let itemOffsets: [CGFloat] = [80, 240, 400, 560, 760]
        let maximumOffset: CGFloat = 640
        let tailSnapThreshold: CGFloat = 96

        var offset: CGFloat = 0
        let expectedDown: [CGFloat] = [80, 240, 400, 640]
        for expected in expectedDown {
            offset = MenuItemScrollNavigator.targetOffset(
                currentOffset: offset,
                direction: 1,
                itemTopOffsets: itemOffsets,
                maximumOffset: maximumOffset,
                tailSnapThreshold: tailSnapThreshold
            )
            XCTAssertEqual(offset, expected)
        }

        let expectedUp: [CGFloat] = [560, 400, 240, 80, 0]
        for expected in expectedUp {
            offset = MenuItemScrollNavigator.targetOffset(
                currentOffset: offset,
                direction: -1,
                itemTopOffsets: itemOffsets,
                maximumOffset: maximumOffset,
                tailSnapThreshold: tailSnapThreshold
            )
            XCTAssertEqual(offset, expected)
        }
    }

    func testMenuItemScrollingKeepsDistinctLargeTailStep() {
        XCTAssertEqual(
            MenuItemScrollNavigator.targetOffset(
                currentOffset: 400,
                direction: 1,
                itemTopOffsets: [80, 240, 400, 500, 900],
                maximumOffset: 700,
                tailSnapThreshold: 96
            ),
            500
        )
    }

    func testLunchLayoutTitles() {
        XCTAssertEqual(LunchLayout.legacy.title, "Classic")
        XCTAssertEqual(LunchLayout.standard.title, "Standard")
        XCTAssertEqual(LunchLayout.compact.title, "Compact")
    }

    func testClosureNoticeUsesOnlyTheInclusiveEndDate() {
        let closure = SeasonalClosure(
            start: LocalDate(year: 2026, month: 6, day: 18),
            end: LocalDate(year: 2026, month: 8, day: 9)
        )

        XCTAssertEqual(
            closure.noticeText(language: .en, referenceYear: 2026),
            "Closed until 9 August"
        )
        XCTAssertEqual(
            closure.noticeText(language: .fi, referenceYear: 2026),
            "Suljettu 9. elokuuta asti"
        )
        XCTAssertEqual(
            closure.noticeText(language: .en, referenceYear: 2025),
            "Closed until 9 August 2026"
        )
    }

    func testAppearanceFollowsTheSystemForEveryAccent() {
        XCTAssertNil(AppAppearance.preferredColorScheme)
        XCTAssertFalse(AppAccent.system.overridesSystemAccent)
        XCTAssertTrue(AppAccent.orange.overridesSystemAccent)
    }

    func testMacUpdaterSelectsLatestMacReleaseOnly() throws {
        let data = Data(
            """
            [
              {
                "tag_name": "windows-v9.0.0",
                "html_url": "https://example.test/windows",
                "draft": false,
                "prerelease": false
              },
              {
                "tag_name": "macos-v0.3.0",
                "html_url": "https://example.test/macos-0.3.0",
                "draft": false,
                "prerelease": false
              },
              {
                "tag_name": "macos-v0.4.0",
                "html_url": "https://example.test/macos-prerelease",
                "draft": false,
                "prerelease": true
              },
              {
                "tag_name": "macos-v1.0.0",
                "html_url": "https://example.test/draft",
                "draft": true,
                "prerelease": false
              },
              {
                "tag_name": "macos-v0.2.0",
                "html_url": "https://example.test/macos-0.2.0",
                "draft": false,
                "prerelease": true
              }
            ]
            """.utf8
        )

        XCTAssertEqual(
            try MacUpdateChecker.result(
                currentVersion: "0.2.0",
                releasesData: data
            ),
            .updateAvailable(
                currentVersion: "0.2.0",
                latestVersion: "0.3.0",
                releaseURL: URL(string: "https://example.test/macos-0.3.0")!
            )
        )
    }

    func testMacUpdaterClassifiesCurrentAndNewerVersions() throws {
        let data = Data(
            """
            [{
              "tag_name": "macos-v0.2.0",
              "html_url": "https://example.test/macos-0.2.0",
              "draft": false,
              "prerelease": false
            }]
            """.utf8
        )

        XCTAssertEqual(
            try MacUpdateChecker.result(
                currentVersion: "0.2.0",
                releasesData: data
            ),
            .latestPublished(
                currentVersion: "0.2.0",
                releaseURL: URL(string: "https://example.test/macos-0.2.0")!
            )
        )
        XCTAssertEqual(
            try MacUpdateChecker.result(
                currentVersion: "0.3.0",
                releasesData: data
            ),
            .newerThanLatestPublished(
                currentVersion: "0.3.0",
                latestVersion: "0.2.0"
            )
        )
    }

    @MainActor
    func testAccentDefaultsToSystemAndPersistsSelection() {
        let defaults = testDefaults()
        defer { defaults.removePersistentDomain(forName: defaultsSuite(defaults)) }
        let model = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        XCTAssertEqual(model.accent, .system)
        model.accent = .orange
        XCTAssertEqual(
            AppModel(
                defaults: defaults,
                loginItemService: FakeLoginItemService()
            ).accent,
            .orange
        )
    }

    func testBundledCatalogUsesPermanentProviderNeutralIDs() {
        XCTAssertEqual(
            Restaurant.fallbackRestaurants.map(\.id),
            [
                "snellmania",
                "cafe-snellari",
                "canthia",
                "tietoteknia",
                "hyva-huomen-bioteknia",
                "antell-round",
                "antell-highway",
                "mediteknia",
                "pranzeria-sorrento",
                "caari"
            ]
        )
        XCTAssertEqual(Restaurant.migratedID("0439"), "tietoteknia")
        XCTAssertEqual(
            Restaurant.migratedID("pranzeria-html"),
            "pranzeria-sorrento"
        )
    }

    func testMenuServiceDecodesNormalizedMenuContract() async throws {
        StubURLProtocol.handler = { request in
            XCTAssertEqual(request.url?.path, "/v1/restaurants/tietoteknia/menu")
            XCTAssertTrue(request.url?.query?.contains("language=en") == true)
            return Self.response(
                request,
                json: """
                {
                  "apiVersion": "v1",
                  "schemaVersion": 1,
                  "restaurant": {
                    "id": "tietoteknia",
                    "order": 4,
                    "name": {"fi": "Tietoteknia", "en": "Tietoteknia"},
                    "websiteUrl": "https://example.com/tietoteknia",
                    "languages": ["fi", "en"],
                    "closures": []
                  },
                  "requestedLanguage": "en",
                  "contentLanguage": "en",
                  "date": "2026-07-24",
                  "service": {"status": "serving", "hours": "10:30–14:00"},
                  "offers": [{
                    "id": "lunch",
                    "label": "Lunch",
                    "price": {"amount": "12.90", "currency": "EUR"}
                  }],
                  "groups": [{
                    "id": "group-1",
                    "prices": [
                      {
                        "amount": "13.30",
                        "currency": "EUR",
                        "audiences": ["staff", "guest", "future-audience"]
                      },
                      {
                        "amount": "3.10",
                        "currency": "EUR",
                        "audiences": ["student"]
                      }
                    ],
                    "items": [{
                      "id": "item-1",
                      "name": "Broad bean tikka masala",
                      "tags": ["G", "L", "Veg"],
                      "recipe": {
                        "id": "compass-1",
                        "ingredients": "broad bean, tomato",
                        "nutritionPer100g": [
                          {"name": "Protein", "amount": 7.8, "unit": "g"}
                        ],
                        "co2eKilogramsPer100Grams": 0.2,
                        "diets": ["G", "L", "Veg"]
                      }
                    }],
                    "sortOrder": 1
                  }],
                  "freshness": {
                    "fetchedAt": "2026-07-24T08:00:00Z",
                    "isStale": false
                  }
                }
                """
            )
        }
        defer { StubURLProtocol.reset() }

        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let date = try XCTUnwrap(
            ISO8601DateFormatter().date(from: "2026-07-24T09:00:00+03:00")
        )
        let snapshot = try await service.fetch(
            restaurant: Restaurant.fallbackRestaurants[3],
            language: .en,
            now: date
        )

        XCTAssertEqual(snapshot.menu?.lunchTime, "10:30–14:00")
        XCTAssertEqual(snapshot.serviceStatus, .serving)
        XCTAssertEqual(snapshot.menu?.offers.first?.price.amount, "12.90")
        XCTAssertEqual(snapshot.menu?.groups.first?.prices?.first?.audiences, [
            .staff, .guest
        ])
        XCTAssertEqual(
            ComponentParts(snapshot.menu?.groups.first?.components.first ?? "").diets,
            "G, L, Veg"
        )
        XCTAssertEqual(
            snapshot.menu?.groups.first?.detail(at: 0)?.ingredients,
            "broad bean, tomato"
        )
    }

    func testMenuServiceDecodesClosureFromAPI() async throws {
        StubURLProtocol.handler = { request in
            Self.response(
                request,
                json: """
                {
                  "apiVersion": "v1",
                  "schemaVersion": 1,
                  "restaurant": {
                    "id": "cafe-snellari",
                    "order": 2,
                    "name": {"fi": "Cafe Snellari", "en": "Cafe Snellari"},
                    "websiteUrl": null,
                    "languages": ["fi", "en"],
                    "closures": []
                  },
                  "requestedLanguage": "en",
                  "contentLanguage": "en",
                  "date": "2026-07-24",
                  "service": {"status": "closed"},
                  "closure": {
                    "kind": "seasonal",
                    "startsOn": "2026-05-08",
                    "endsOn": "2026-08-30"
                  },
                  "offers": [],
                  "groups": [],
                  "freshness": {
                    "fetchedAt": "2026-07-24T08:00:00Z",
                    "isStale": false
                  }
                }
                """
            )
        }
        defer { StubURLProtocol.reset() }
        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let snapshot = try await service.fetch(
            restaurant: Restaurant.fallbackRestaurants[1],
            language: .en
        )
        XCTAssertEqual(snapshot.closure?.start, LocalDate(year: 2026, month: 5, day: 8))
        XCTAssertEqual(snapshot.closure?.end, LocalDate(year: 2026, month: 8, day: 30))
        XCTAssertEqual(snapshot.serviceStatus, .closed)
    }

    func testMenuServiceMapsFutureServiceStatusToUnknown() async throws {
        let fixture = includeFixture("contract-menu.json")
            .replacingOccurrences(
                of: #""status": "serving""#,
                with: #""status": "future-status""#
            )
        StubURLProtocol.handler = { request in
            XCTAssertTrue(request.url?.query?.contains("date=2026-07-24") == true)
            return Self.response(request, json: fixture)
        }
        defer { StubURLProtocol.reset() }

        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let date = try XCTUnwrap(
            ISO8601DateFormatter().date(from: "2026-07-23T21:30:00Z")
        )
        let snapshot = try await service.fetch(
            restaurant: Restaurant.fallbackRestaurants[3],
            language: .fi,
            now: date
        )

        XCTAssertEqual(snapshot.serviceStatus, .unknown)
    }

    func testMenuServicePreservesStaleGenerationMarker() async throws {
        let fixture = includeFixture("contract-menu.json")
            .replacingOccurrences(
                of: #""isStale": false"#,
                with: #""isStale": true"#
            )
        StubURLProtocol.handler = { request in
            Self.response(request, json: fixture)
        }
        defer { StubURLProtocol.reset() }

        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let date = try XCTUnwrap(
            ISO8601DateFormatter().date(from: "2026-07-24T09:00:00+03:00")
        )
        let snapshot = try await service.fetch(
            restaurant: Restaurant.fallbackRestaurants[3],
            language: .fi,
            now: date
        )

        XCTAssertEqual(snapshot.isStale, true)
    }

    func testSnapshotKeepsValidMenusWhenOneEntryIsMalformed() async throws {
        let menuData = Data(includeFixture("contract-menu.json").utf8)
        let menu = try XCTUnwrap(
            JSONSerialization.jsonObject(with: menuData) as? [String: Any]
        )
        var futureStatusMenu = menu
        var futureRestaurant = try XCTUnwrap(
            futureStatusMenu["restaurant"] as? [String: Any]
        )
        futureRestaurant["id"] = "snellmania"
        futureRestaurant["order"] = 1
        futureStatusMenu["restaurant"] = futureRestaurant
        futureStatusMenu["service"] = ["status": "future-status"]

        let restaurants = [
            futureRestaurant,
            try XCTUnwrap(menu["restaurant"] as? [String: Any]),
            [
                "id": "cafe-snellari",
                "order": 2,
                "name": ["fi": "Cafe Snellari", "en": "Cafe Snellari"],
                "websiteUrl": NSNull(),
                "languages": ["fi", "en"],
                "closures": []
            ] as [String: Any]
        ]
        let response: [String: Any] = [
            "apiVersion": "v1",
            "schemaVersion": 1,
            "revision": "test",
            "requestedLanguage": "fi",
            "date": "2026-07-24",
            "restaurants": restaurants,
            "menus": [
                futureStatusMenu,
                menu,
                ["restaurant": ["id": "cafe-snellari"]]
            ]
        ]
        let responseData = try JSONSerialization.data(withJSONObject: response)
        StubURLProtocol.handler = { request in
            (
                HTTPURLResponse(
                    url: request.url!,
                    statusCode: 200,
                    httpVersion: nil,
                    headerFields: ["Content-Type": "application/json"]
                )!,
                responseData
            )
        }
        defer { StubURLProtocol.reset() }

        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let date = try XCTUnwrap(
            ISO8601DateFormatter().date(from: "2026-07-24T09:00:00+03:00")
        )
        let snapshot = try await service.fetchDailySnapshot(
            language: .fi,
            now: date
        )

        XCTAssertEqual(snapshot.restaurants.count, 3)
        XCTAssertEqual(snapshot.menus.count, 2)
        XCTAssertEqual(
            snapshot.menus.first(where: {
                $0.restaurantCode == "snellmania"
            })?.serviceStatus,
            .unknown
        )
        XCTAssertEqual(
            snapshot.menus.first(where: {
                $0.restaurantCode == "tietoteknia"
            })?.serviceStatus,
            .serving
        )
    }

    func testSnapshotCatalogUsesSelectedLanguage() async throws {
        StubURLProtocol.handler = { request in
            XCTAssertEqual(request.url?.path, "/v1/snapshot")
            XCTAssertEqual(request.cachePolicy, .useProtocolCachePolicy)
            return Self.response(
                request,
                json: """
                {
                  "apiVersion": "v1",
                  "schemaVersion": 1,
                  "revision": "test",
                  "requestedLanguage": "en",
                  "date": "2026-07-28",
                  "restaurants": [{
                    "id": "new-restaurant",
                    "order": 1,
                    "name": {"fi": "Uusi", "en": "New"},
                    "websiteUrl": "https://example.com",
                    "languages": ["fi"],
                    "closures": []
                  }],
                  "menus": [{
                    "apiVersion": "v1",
                    "schemaVersion": 1,
                    "restaurant": {
                      "id": "new-restaurant",
                      "order": 1,
                      "name": {"fi": "Uusi", "en": "New"},
                      "websiteUrl": "https://example.com",
                      "languages": ["fi"],
                      "closures": []
                    },
                    "requestedLanguage": "en",
                    "contentLanguage": "fi",
                    "date": "2026-07-28",
                    "service": {"status": "noMenu"},
                    "offers": [],
                    "groups": [],
                    "freshness": {
                      "fetchedAt": "2026-07-28T07:00:00Z",
                      "isStale": false
                    }
                  }]
                }
                """
            )
        }
        defer { StubURLProtocol.reset() }
        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let now = ISO8601DateFormatter().date(from: "2026-07-28T10:00:00+03:00")!
        let snapshot = try await service.fetchDailySnapshot(language: .en, now: now)
        XCTAssertEqual(snapshot.restaurants.first?.id, "new-restaurant")
        XCTAssertEqual(snapshot.restaurants.first?.name, "New")
    }

    func testMenuServiceSupportsExplicitCacheRevalidation() async throws {
        StubURLProtocol.handler = { request in
            XCTAssertEqual(request.cachePolicy, .reloadRevalidatingCacheData)
            return Self.response(
                request,
                json: self.includeFixture("contract-menu.json")
            )
        }
        defer { StubURLProtocol.reset() }

        let service = MenuService(
            session: stubSession(),
            baseURL: URL(string: "https://example.test")!
        )
        let now = ISO8601DateFormatter().date(
            from: "2026-07-24T10:00:00+03:00"
        )!
        _ = try await service.fetch(
            restaurant: Restaurant.fallbackRestaurants[3],
            language: .fi,
            now: now,
            cachePolicy: .reloadRevalidatingCacheData
        )
    }

    @MainActor
    func testAPIAudiencePricesFilterWithoutRestaurantRules() {
        let defaults = testDefaults()
        defer { defaults.removePersistentDomain(forName: defaultsSuite(defaults)) }
        let model = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        let group = LunchGroup(
            id: "lunch",
            name: "Lunch",
            price: "",
            prices: [
                LunchPrice(amount: "13.30", audiences: [.staff, .guest]),
                LunchPrice(amount: "3.10", audiences: [.student])
            ],
            components: ["Dish"]
        )
        model.showStudentPrice = false
        model.showStaffPrice = true
        model.showGuestPrice = false
        XCTAssertEqual(model.displayPrice(for: group), "13,30 €")
    }

    @MainActor
    func testSwitchingToFreshCachedMenuDoesNotFetchAgain() async {
        let defaults = testDefaults()
        let cacheDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer {
            defaults.removePersistentDomain(forName: defaultsSuite(defaults))
            try? FileManager.default.removeItem(at: cacheDirectory)
            StubURLProtocol.reset()
        }
        defaults.set("snellmania", forKey: "restaurantCode")
        defaults.set(AppLanguage.en.rawValue, forKey: "language")
        let cache = CacheStore(directory: cacheDirectory)
        cache.save(snapshot(restaurantCode: "snellmania"))
        cache.save(snapshot(restaurantCode: "tietoteknia"))
        let model = AppModel(
            defaults: defaults,
            cache: cache,
            service: MenuService(
                session: stubSession(),
                baseURL: URL(string: "https://example.test")!
            ),
            loginItemService: FakeLoginItemService()
        )

        model.selectedRestaurantCode = "tietoteknia"
        for _ in 0..<10 { await Task.yield() }
        XCTAssertEqual(model.snapshot?.restaurantCode, "tietoteknia")
        XCTAssertFalse(model.isLoading)
        XCTAssertEqual(StubURLProtocol.requestCount, 0)
    }

    @MainActor
    func testBackgroundPreparationCachesEveryRestaurantWithoutChangingSelection() async {
        let defaults = testDefaults()
        let cacheDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let now = ISO8601DateFormatter().date(
            from: "2026-07-28T10:00:00+03:00"
        )!
        defer {
            defaults.removePersistentDomain(forName: defaultsSuite(defaults))
            try? FileManager.default.removeItem(at: cacheDirectory)
            StubURLProtocol.reset()
        }
        defaults.set("tietoteknia", forKey: "restaurantCode")
        defaults.set(AppLanguage.en.rawValue, forKey: "language")
        StubURLProtocol.handler = { request in
            XCTAssertEqual(request.url?.path, "/v1/snapshot")
            let restaurants = Restaurant.fallbackRestaurants.enumerated().map {
                index, restaurant in
                """
                {
                  "id": "\(restaurant.id)",
                  "order": \(index + 1),
                  "name": {"fi": "\(restaurant.id)", "en": "\(restaurant.id)"},
                  "websiteUrl": null,
                  "languages": ["fi", "en"],
                  "closures": []
                }
                """
            }.joined(separator: ",")
            let menus = Restaurant.fallbackRestaurants.enumerated().map {
                index, restaurant in
                """
                {
                  "apiVersion": "v1",
                  "schemaVersion": 1,
                  "restaurant": {
                    "id": "\(restaurant.id)",
                    "order": \(index + 1),
                    "name": {"fi": "\(restaurant.id)", "en": "\(restaurant.id)"},
                    "websiteUrl": null,
                    "languages": ["fi", "en"],
                    "closures": []
                  },
                  "requestedLanguage": "en",
                  "contentLanguage": "en",
                  "date": "2026-07-28",
                  "service": {"status": "serving"},
                  "offers": [],
                  "groups": [{
                    "id": "group-1",
                    "prices": [],
                    "items": [{"id": "item-1", "name": "Dish"}],
                    "sortOrder": 1
                  }],
                  "freshness": {
                    "fetchedAt": "2026-07-28T07:00:00Z",
                    "isStale": false
                  }
                }
                """
            }.joined(separator: ",")
            return Self.response(
                request,
                json: """
                {
                  "apiVersion": "v1",
                  "schemaVersion": 1,
                  "requestedLanguage": "en",
                  "date": "2026-07-28",
                  "restaurants": [\(restaurants)],
                  "menus": [\(menus)]
                }
                """
            )
        }

        let cache = CacheStore(directory: cacheDirectory)
        let model = AppModel(
            defaults: defaults,
            cache: cache,
            service: MenuService(
                session: stubSession(),
                baseURL: URL(string: "https://example.test")!
            ),
            loginItemService: FakeLoginItemService(),
            nowProvider: { now }
        )

        await model.prepareMenusInBackground()

        XCTAssertEqual(model.selectedRestaurantCode, "tietoteknia")
        XCTAssertFalse(model.isLoading)
        XCTAssertEqual(StubURLProtocol.requestCount, 1)
        for restaurant in Restaurant.fallbackRestaurants {
            XCTAssertEqual(
                cache.load(
                    restaurantCode: restaurant.id,
                    language: .en
                )?.serviceStatus,
                .serving
            )
        }
    }

    func testComponentPartsAndHighlightsRemainIndependent() {
        let parts = ComponentParts("Carrot soup (A, G, L, M, Veg)")
        XCTAssertEqual(parts.name, "Carrot soup")
        XCTAssertEqual(parts.diets, "A, G, L, M, Veg")
        XCTAssertTrue(TextHighlight.matches("Crème fraîche", highlights: ["creme"]))
        XCTAssertFalse(TextHighlight.matches("Carrot soup", highlights: ["chicken"]))
    }

    func testBackgroundPreloadPolicyStopsAfterSuccessOrThreeAttempts() {
        let now = ISO8601DateFormatter().date(
            from: "2026-07-28T10:00:00+03:00"
        )!
        let old = now.addingTimeInterval(-MenuPreloadPolicy.retryInterval)
        let terminal = MenuSnapshot(
            restaurantCode: "snellmania",
            restaurantName: "Snellmania",
            restaurantURL: nil,
            language: .en,
            fetchedAt: old,
            menu: LunchMenu(
                date: "2026-07-28",
                lunchTime: "",
                groups: [
                    LunchGroup(
                        id: "lunch",
                        name: "",
                        price: "",
                        components: ["Dish"]
                    )
                ]
            ),
            serviceStatus: .serving
        )
        let staleTerminal = MenuSnapshot(
            restaurantCode: "snellmania",
            restaurantName: "Snellmania",
            restaurantURL: nil,
            language: .en,
            fetchedAt: old,
            menu: terminal.menu,
            serviceStatus: .serving,
            isStale: true
        )
        let unpublished = MenuSnapshot(
            restaurantCode: "snellmania",
            restaurantName: "Snellmania",
            restaurantURL: nil,
            language: .en,
            fetchedAt: old,
            menu: LunchMenu(date: "2026-07-28", lunchTime: "", groups: []),
            serviceStatus: .noMenu
        )
        let unavailable = MenuSnapshot(
            restaurantCode: "snellmania",
            restaurantName: "Snellmania",
            restaurantURL: nil,
            language: .en,
            fetchedAt: old,
            menu: LunchMenu(date: "2026-07-28", lunchTime: "", groups: []),
            serviceStatus: .unknown
        )

        XCTAssertFalse(
            MenuPreloadPolicy.shouldAttempt(
                snapshot: terminal,
                attempt: nil,
                now: now
            )
        )
        XCTAssertTrue(
            MenuPreloadPolicy.shouldAttempt(
                snapshot: staleTerminal,
                attempt: BackgroundPreloadAttempt(count: 1, lastAttempt: old),
                now: now
            )
        )
        XCTAssertTrue(
            MenuPreloadPolicy.shouldAttempt(
                snapshot: unpublished,
                attempt: BackgroundPreloadAttempt(count: 1, lastAttempt: old),
                now: now
            )
        )
        XCTAssertTrue(
            MenuPreloadPolicy.shouldAttempt(
                snapshot: unavailable,
                attempt: BackgroundPreloadAttempt(count: 1, lastAttempt: old),
                now: now
            )
        )
        XCTAssertFalse(
            MenuPreloadPolicy.shouldAttempt(
                snapshot: unpublished,
                attempt: BackgroundPreloadAttempt(count: 3, lastAttempt: old),
                now: now
            )
        )
    }

    func testBackgroundPreloadPolicyMatchesWeekendRulesAndCutoff() {
        let formatter = ISO8601DateFormatter()
        let saturday = formatter.date(from: "2026-08-01T09:00:00+03:00")!
        let sunday = formatter.date(from: "2026-08-02T09:00:00+03:00")!
        let afterCutoff = formatter.date(from: "2026-07-28T15:00:00+03:00")!

        XCTAssertTrue(
            MenuPreloadPolicy.permits(
                restaurantID: "snellmania",
                now: saturday
            )
        )
        XCTAssertFalse(
            MenuPreloadPolicy.permits(
                restaurantID: "tietoteknia",
                now: saturday
            )
        )
        XCTAssertFalse(
            MenuPreloadPolicy.permits(
                restaurantID: "snellmania",
                now: sunday
            )
        )
        XCTAssertFalse(
            MenuPreloadPolicy.permits(
                restaurantID: "snellmania",
                now: afterCutoff
            )
        )
    }

    func testManualRefreshPolicyUsesFifteenMinuteCooldown() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        XCTAssertTrue(
            ManualRefreshPolicy.permits(lastRefresh: nil, now: now)
        )
        XCTAssertFalse(
            ManualRefreshPolicy.permits(
                lastRefresh: now.addingTimeInterval(-14 * 60 - 59),
                now: now
            )
        )
        XCTAssertTrue(
            ManualRefreshPolicy.permits(
                lastRefresh: now.addingTimeInterval(-15 * 60),
                now: now
            )
        )
    }

    @MainActor
    func testManualRefreshCooldownAppliesAcrossRestaurants() throws {
        let defaults = testDefaults()
        let cacheDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let now = Date(timeIntervalSince1970: 1_000_000)
        defer {
            defaults.removePersistentDomain(forName: defaultsSuite(defaults))
            try? FileManager.default.removeItem(at: cacheDirectory)
        }
        defaults.set(
            try JSONEncoder().encode(now),
            forKey: "lastManualRefresh"
        )
        let model = AppModel(
            defaults: defaults,
            cache: CacheStore(directory: cacheDirectory),
            loginItemService: FakeLoginItemService(),
            nowProvider: { now }
        )

        XCTAssertFalse(model.canRefreshSelectedRestaurant)
        model.selectedRestaurantCode = "tietoteknia"
        XCTAssertFalse(model.canRefreshSelectedRestaurant)
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
    }

    @MainActor
    func testPriceMasterToggleTracksGroupSelection() {
        let defaults = testDefaults()
        defer { defaults.removePersistentDomain(forName: defaultsSuite(defaults)) }
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
    func testPriceMasterToggleAlsoControlsGeneralOffers() {
        let defaults = testDefaults()
        defer { defaults.removePersistentDomain(forName: defaultsSuite(defaults)) }
        let model = AppModel(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        let offer = LunchOffer(
            id: "buffet",
            label: "Lunch buffet",
            price: LunchPrice(amount: "14.00", audiences: nil),
            description: nil
        )

        XCTAssertEqual(model.displayPrice(for: offer), "14,00 €")
        model.showPrices = false
        XCTAssertEqual(model.displayPrice(for: offer), "")
    }

    @MainActor
    func testLaunchAtLoginIsEnabledOnceByDefault() {
        let defaults = testDefaults()
        defer { defaults.removePersistentDomain(forName: defaultsSuite(defaults)) }
        let loginItem = FakeLoginItemService()
        let model = AppModel(
            defaults: defaults,
            loginItemService: loginItem
        )
        model.configureLaunchAtLoginIfNeeded()
        model.configureLaunchAtLoginIfNeeded()
        XCTAssertTrue(model.launchAtLogin)
        XCTAssertEqual(loginItem.registerCount, 1)
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
                groups: [
                    LunchGroup(
                        id: "lunch",
                        name: "Lunch",
                        price: "",
                        prices: [LunchPrice(amount: "3.10", audiences: [.student])],
                        components: ["Dish"]
                    )
                ]
            )
        )
    }

    private func stubSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [StubURLProtocol.self]
        return URLSession(configuration: configuration)
    }

    private func includeFixture(_ name: String) -> String {
        let testsURL = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        let fixtureURL = testsURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("api/test/fixtures")
            .appendingPathComponent(name)
        return try! String(contentsOf: fixtureURL, encoding: .utf8)
    }

    private func testDefaults() -> UserDefaults {
        let name = "CompassLunchTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.set(name, forKey: "__suite")
        return defaults
    }

    private func defaultsSuite(_ defaults: UserDefaults) -> String {
        defaults.string(forKey: "__suite")!
    }

    private static func response(
        _ request: URLRequest,
        json: String
    ) -> (HTTPURLResponse, Data) {
        (
            HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!,
            Data(json.utf8)
        )
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

private final class StubURLProtocol: URLProtocol {
    typealias Handler = (URLRequest) throws -> (HTTPURLResponse, Data)

    private static let lock = NSLock()
    static var handler: Handler?
    private static var count = 0

    static var requestCount: Int {
        lock.withLock { count }
    }

    static func reset() {
        lock.withLock {
            handler = nil
            count = 0
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
        do {
            guard let handler = Self.lock.withLock({ Self.handler }) else {
                throw URLError(.cannotConnectToHost)
            }
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}
