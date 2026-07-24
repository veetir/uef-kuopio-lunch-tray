import Foundation

enum MenuServiceError: LocalizedError {
    case invalidResponse
    case serverStatus(Int)
    case provider(String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            "The menu service returned an invalid response."
        case let .serverStatus(status):
            "The menu service returned HTTP \(status)."
        case let .provider(message):
            message
        }
    }
}

struct MenuService {
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func fetch(restaurant: Restaurant, language: AppLanguage, now: Date = Date()) async throws -> MenuSnapshot {
        switch restaurant.provider {
        case .compass:
            return try await fetchCompass(restaurant: restaurant, language: language, now: now)
        case let .compassRSS(costNumber):
            var components = URLComponents(string: "https://www.compass-group.fi/menuapi/feed/rss/current-day")
            components?.queryItems = [
                URLQueryItem(name: "costNumber", value: costNumber),
                URLQueryItem(name: "language", value: language.rawValue)
            ]
            guard let url = components?.url else {
                throw MenuServiceError.invalidResponse
            }
            let data = try await load(url)
            return try RSSMenuParser.parse(
                data: data,
                restaurant: restaurant,
                requestedLanguage: language,
                now: now
            )
        case let .huomen(apiURL):
            var components = URLComponents(url: apiURL, resolvingAgainstBaseURL: false)
            var queryItems = components?.queryItems ?? []
            queryItems.append(URLQueryItem(name: "language", value: language.rawValue))
            components?.queryItems = queryItems
            guard let url = components?.url else {
                throw MenuServiceError.invalidResponse
            }
            let data = try await load(url)
            return try HuomenMenuParser.parse(
                data: data,
                restaurant: restaurant,
                requestedLanguage: language,
                now: now
            )
        case let .antell(slug):
            let weekday = ProviderParsing.weekdayToken(now)
            let urlString: String
            if language == .en, restaurant.id == "antell-round" {
                urlString = "https://antell.fi/en/lunch/kuopio/\(slug)/?print_lunch_list_day=1&print_lunch_day=panel-\(weekday)"
            } else {
                urlString = "https://antell.fi/lounas/kuopio/\(slug)/?print_lunch_day=\(weekday)&print_lunch_list_day=1"
            }
            guard let url = URL(string: urlString) else {
                throw MenuServiceError.invalidResponse
            }
            let data = try await load(url)
            let snapshot = try AntellMenuParser.parse(
                data: data,
                restaurant: restaurant,
                requestedLanguage: language,
                now: now
            )
            let detailURLString = language == .en && restaurant.id == "antell-round"
                ? "https://antell.fi/en/lunch/kuopio/\(slug)/"
                : "https://antell.fi/lounas/kuopio/\(slug)/"
            guard let detailURL = URL(string: detailURLString),
                  let detailData = try? await load(detailURL),
                  let detailHTML = String(data: detailData, encoding: .utf8)
            else {
                return snapshot.markingDetailEnrichmentAttempted()
            }
            return RecipeDetailEnrichment.applying(
                AntellRecipeDetailParser.details(
                    from: detailHTML,
                    weekday: weekday
                ),
                to: snapshot
            )
        case .pranzeria:
            guard let url = restaurant.pageURL else {
                throw MenuServiceError.invalidResponse
            }
            let data = try await load(url)
            return try PranzeriaMenuParser.parse(
                data: data,
                restaurant: restaurant,
                requestedLanguage: language,
                now: now
            )
        }
    }

    private func fetchCompass(
        restaurant: Restaurant,
        language: AppLanguage,
        now: Date
    ) async throws -> MenuSnapshot {
        let fetchLanguage = restaurant.englishMenuAvailable
            ? language.rawValue
            : AppLanguage.fi.rawValue
        var components = URLComponents(string: "https://www.compass-group.fi/menuapi/feed/json")
        components?.queryItems = [
            URLQueryItem(name: "costNumber", value: restaurant.id),
            URLQueryItem(name: "language", value: fetchLanguage)
        ]
        guard let url = components?.url else {
            throw MenuServiceError.invalidResponse
        }

        let data = try await load(url)
        let snapshot = try CompassParser.parse(
            data: data,
            restaurant: restaurant,
            requestedLanguage: language,
            now: now
        )
        guard let pageURL = snapshot.restaurantURL ?? restaurant.pageURL,
              let pageData = try? await load(pageURL),
              let pageHTML = String(data: pageData, encoding: .utf8)
        else {
            return snapshot.markingDetailEnrichmentAttempted()
        }

        let references = CompassRecipeDetailParser.references(
            fromRestaurantHTML: pageHTML
        )
        guard !references.isEmpty else {
            return snapshot.markingDetailEnrichmentAttempted()
        }
        let detailsByID = await compassRecipeDetails(
            recipeIDs: Set(references.values),
            language: fetchLanguage
        )
        let detailsByMealName = references.reduce(
            into: [String: RecipeDetail]()
        ) { result, entry in
            if let detail = detailsByID[entry.value] {
                result[entry.key] = detail
            }
        }
        return RecipeDetailEnrichment.applying(detailsByMealName, to: snapshot)
    }

    private func compassRecipeDetails(
        recipeIDs: Set<Int>,
        language: String
    ) async -> [Int: RecipeDetail] {
        await withTaskGroup(of: (Int, RecipeDetail?).self) { group in
            for recipeID in recipeIDs {
                group.addTask {
                    guard var components = URLComponents(
                        string: "https://www.compass-group.fi/menuapi/recipes/\(recipeID)"
                    ) else {
                        return (recipeID, nil)
                    }
                    components.queryItems = [
                        URLQueryItem(name: "language", value: language)
                    ]
                    guard let url = components.url,
                          let data = try? await self.load(url)
                    else {
                        return (recipeID, nil)
                    }
                    return (
                        recipeID,
                        CompassRecipeDetailParser.parse(
                            data: data,
                            fallbackRecipeID: recipeID
                        )
                    )
                }
            }

            var details: [Int: RecipeDetail] = [:]
            for await (recipeID, detail) in group {
                if let detail {
                    details[recipeID] = detail
                }
            }
            return details
        }
    }

    private func load(_ url: URL) async throws -> Data {
        var request = URLRequest(url: url)
        request.timeoutInterval = 10
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("LunchTray/1.0", forHTTPHeaderField: "User-Agent")

        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw MenuServiceError.invalidResponse
        }
        guard (200..<300).contains(httpResponse.statusCode) else {
            throw MenuServiceError.serverStatus(httpResponse.statusCode)
        }
        return data
    }
}

enum CompassParser {
    static func parse(
        data: Data,
        restaurant: Restaurant,
        requestedLanguage: AppLanguage,
        now: Date
    ) throws -> MenuSnapshot {
        let response = try JSONDecoder().decode(APIResponse.self, from: data)
        if let error = response.errorText?.normalizedWhitespace, !error.isEmpty {
            throw MenuServiceError.provider(error)
        }

        let today = localDateString(now)
        let days = response.menusForDays ?? []
        let day = days.first { apiDay in
            guard let date = apiDay.date else { return false }
            return String(date.prefix(10)) == today
        }

        let indexedGroups = Array((day?.setMenus ?? []).enumerated())
        let sortedGroups = indexedGroups.sorted { left, right in
            let leftOrder = left.element.sortOrder ?? left.offset
            let rightOrder = right.element.sortOrder ?? right.offset
            return leftOrder < rightOrder
        }
        let groups: [LunchGroup] = sortedGroups.compactMap { item in
            let index = item.offset
            let group = item.element
            let components = (group.components ?? [])
                .map { $0.normalizedWhitespace }
                .filter { !$0.isEmpty }
            guard !components.isEmpty else { return nil }
            return LunchGroup(
                id: "\(group.sortOrder ?? index)-\(index)",
                name: group.name?.normalizedWhitespace ?? "",
                price: group.price?.normalizedWhitespace ?? "",
                components: components
            )
        }

        let menu = day.map {
            LunchMenu(
                date: today,
                lunchTime: $0.lunchTime?.normalizedWhitespace ?? "",
                groups: groups
            )
        }

        return MenuSnapshot(
            restaurantCode: restaurant.id,
            restaurantName: response.restaurantName?.normalizedWhitespace.nonEmpty ?? restaurant.name,
            restaurantURL: URL(string: response.restaurantURL ?? "") ?? restaurant.pageURL,
            language: requestedLanguage,
            fetchedAt: now,
            menu: menu
        )
    }

    private static func localDateString(_ date: Date) -> String {
        ProviderParsing.localDateString(date)
    }
}

private struct APIResponse: Decodable {
    let restaurantName: String?
    let restaurantURL: String?
    let menusForDays: [APIMenuDay]?
    let errorText: String?

    enum CodingKeys: String, CodingKey {
        case restaurantName = "RestaurantName"
        case restaurantURL = "RestaurantUrl"
        case menusForDays = "MenusForDays"
        case errorText = "ErrorText"
    }
}

private struct APIMenuDay: Decodable {
    let date: String?
    let lunchTime: String?
    let setMenus: [APISetMenu]?

    enum CodingKeys: String, CodingKey {
        case date = "Date"
        case lunchTime = "LunchTime"
        case setMenus = "SetMenus"
    }
}

private struct APISetMenu: Decodable {
    let sortOrder: Int?
    let name: String?
    let price: String?
    let components: [String]?

    enum CodingKeys: String, CodingKey {
        case sortOrder = "SortOrder"
        case name = "Name"
        case price = "Price"
        case components = "Components"
    }
}

private extension String {
    var nonEmpty: String? {
        isEmpty ? nil : self
    }
}
