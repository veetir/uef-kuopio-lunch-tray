import Foundation

enum MenuServiceError: LocalizedError {
    case invalidResponse
    case serverStatus(Int)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            "The lunch API returned an invalid response."
        case let .serverStatus(status):
            "The lunch API returned HTTP \(status)."
        }
    }
}

struct MenuService {
    private let session: URLSession
    private let baseURL: URL

    init(
        session: URLSession = .shared,
        baseURL: URL = URL(string: "https://lunch.veeti.dev")!
    ) {
        self.session = session
        self.baseURL = baseURL
    }

    func fetch(
        restaurant: Restaurant,
        language: AppLanguage,
        now: Date = Date(),
        cachePolicy: URLRequest.CachePolicy = .useProtocolCachePolicy
    ) async throws -> MenuSnapshot {
        var components = URLComponents(
            url: endpoint(path: "v1/restaurants/\(restaurant.id)/menu"),
            resolvingAgainstBaseURL: false
        )
        components?.queryItems = [
            URLQueryItem(name: "language", value: language.rawValue),
            URLQueryItem(name: "date", value: Self.localDateString(now))
        ]
        guard let url = components?.url else {
            throw MenuServiceError.invalidResponse
        }

        let data = try await load(url, cachePolicy: cachePolicy)
        let response = try JSONDecoder().decode(APIMenuResponse.self, from: data)
        return try menuSnapshot(
            response: response,
            restaurant: restaurant,
            language: language,
            now: now
        )
    }

    func fetchDailySnapshot(
        language: AppLanguage,
        now: Date = Date(),
        cachePolicy: URLRequest.CachePolicy = .useProtocolCachePolicy
    ) async throws -> DailyMenuSnapshot {
        var components = URLComponents(
            url: endpoint(path: "v1/snapshot"),
            resolvingAgainstBaseURL: false
        )
        components?.queryItems = [
            URLQueryItem(name: "language", value: language.rawValue),
            URLQueryItem(name: "date", value: Self.localDateString(now))
        ]
        guard let url = components?.url else {
            throw MenuServiceError.invalidResponse
        }

        let data = try await load(url, cachePolicy: cachePolicy)
        let response = try JSONDecoder().decode(APISnapshotResponse.self, from: data)
        guard response.apiVersion == "v1",
              response.schemaVersion == 1,
              response.requestedLanguage == language.rawValue,
              response.date == Self.localDateString(now),
              !response.restaurants.isEmpty,
              Set(response.restaurants.map(\.id)).count == response.restaurants.count
        else {
            throw MenuServiceError.invalidResponse
        }

        let restaurants = response.restaurants
            .sorted { $0.order < $1.order }
            .map { $0.model(language: language) }
        let restaurantsByID = Dictionary(
            uniqueKeysWithValues: restaurants.map { ($0.id, $0) }
        )
        var seenMenuIDs = Set<String>()
        let menus = response.menus.elements.compactMap { apiMenu -> MenuSnapshot? in
            guard seenMenuIDs.insert(apiMenu.restaurant.id).inserted,
                  let restaurant = restaurantsByID[apiMenu.restaurant.id]
            else {
                return nil
            }
            return try? menuSnapshot(
                response: apiMenu,
                restaurant: restaurant,
                language: language,
                now: now
            )
        }
        return DailyMenuSnapshot(restaurants: restaurants, menus: menus)
    }

    private func menuSnapshot(
        response: APIMenuResponse,
        restaurant: Restaurant,
        language: AppLanguage,
        now: Date
    ) throws -> MenuSnapshot {
        guard response.apiVersion == "v1",
              response.schemaVersion == 1,
              response.restaurant.id == restaurant.id
        else {
            throw MenuServiceError.invalidResponse
        }
        let serviceStatus = MenuSnapshot.ServiceStatus(
            rawValue: response.service.status
        ) ?? .unknown

        let groups = response.groups.compactMap { group -> LunchGroup? in
            let components = group.items.map { item in
                var text = item.name.normalizedWhitespace
                if let description = item.description?.normalizedWhitespace,
                   !description.isEmpty,
                   description != text {
                    text += " – \(description)"
                }
                if let tags = item.tags?.filter({ !$0.normalizedWhitespace.isEmpty }),
                   !tags.isEmpty {
                    text += " (\(tags.joined(separator: ", ")))"
                }
                return text
            }.filter { !$0.isEmpty }
            guard !components.isEmpty else { return nil }
            let details = group.items.map { $0.recipe?.model }
            return LunchGroup(
                id: group.id,
                name: group.title?.normalizedWhitespace ?? "",
                price: "",
                prices: group.prices.map(\.model),
                components: components,
                componentDetails: details
            )
        }

        let menu = LunchMenu(
            date: response.date,
            lunchTime: response.service.hours?.normalizedWhitespace ?? "",
            offers: response.offers.map(\.model),
            groups: groups
        )
        return MenuSnapshot(
            restaurantCode: response.restaurant.id,
            restaurantName: language == .fi
                ? response.restaurant.name.fi
                : response.restaurant.name.en,
            restaurantURL: response.restaurant.websiteURL.flatMap(URL.init(string:))
                ?? restaurant.pageURL,
            language: language,
            fetchedAt: now,
            menu: menu,
            closure: response.closure?.model,
            serviceStatus: serviceStatus,
            isStale: response.freshness?.isStale ?? false
        )
    }

    private func endpoint(path: String) -> URL {
        path.split(separator: "/").reduce(baseURL) {
            $0.appendingPathComponent(String($1))
        }
    }

    private func load(
        _ url: URL,
        cachePolicy: URLRequest.CachePolicy
    ) async throws -> Data {
        var request = URLRequest(url: url)
        request.timeoutInterval = 10
        request.cachePolicy = cachePolicy
        request.setValue("LunchTray/1.0", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw MenuServiceError.invalidResponse
        }
        guard (200..<300).contains(httpResponse.statusCode) else {
            throw MenuServiceError.serverStatus(httpResponse.statusCode)
        }
        return data
    }

    private static func localDateString(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "Europe/Helsinki")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
}

struct DailyMenuSnapshot {
    let restaurants: [Restaurant]
    let menus: [MenuSnapshot]
}

private struct APIRestaurant: Decodable {
    let id: String
    let order: Int
    let name: APILocalizedText
    let websiteURL: String?
    let languages: [String]

    enum CodingKeys: String, CodingKey {
        case id
        case order
        case name
        case websiteURL = "websiteUrl"
        case languages
    }

    func model(language: AppLanguage) -> Restaurant {
        Restaurant(
            id: id,
            name: language == .fi ? name.fi : name.en,
            pageURL: websiteURL.flatMap(URL.init(string:)),
            languages: languages.compactMap(AppLanguage.init(rawValue:))
        )
    }
}

private struct APILocalizedText: Decodable {
    let fi: String
    let en: String
}

private struct APIMenuResponse: Decodable {
    let apiVersion: String
    let schemaVersion: Int
    let restaurant: APIRestaurant
    let date: String
    let service: APIService
    let closure: APIClosure?
    let offers: [APIOffer]
    let groups: [APIGroup]
    let freshness: APIFreshness?
}

private struct APISnapshotResponse: Decodable {
    let apiVersion: String
    let schemaVersion: Int
    let requestedLanguage: String
    let date: String
    let restaurants: [APIRestaurant]
    let menus: LossyDecodableArray<APIMenuResponse>
}

private struct LossyDecodableArray<Element: Decodable>: Decodable {
    let elements: [Element]

    init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        var elements: [Element] = []
        while !container.isAtEnd {
            let elementDecoder = try container.superDecoder()
            if let element = try? Element(from: elementDecoder) {
                elements.append(element)
            }
        }
        self.elements = elements
    }
}

private struct APIService: Decodable {
    let status: String
    let hours: String?
}

private struct APIFreshness: Decodable {
    let isStale: Bool
}

private struct APIClosure: Decodable {
    let startsOn: String
    let endsOn: String
    let reason: String?

    var model: SeasonalClosure? {
        guard let start = Self.localDate(startsOn),
              let end = Self.localDate(endsOn)
        else {
            return nil
        }
        return SeasonalClosure(start: start, end: end, reason: reason)
    }

    private static func localDate(_ value: String) -> LocalDate? {
        let parts = value.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        return LocalDate(year: parts[0], month: parts[1], day: parts[2])
    }
}

private struct APIPrice: Decodable {
    let amount: String
    let audiences: [String]?

    var model: LunchPrice {
        LunchPrice(
            amount: amount,
            audiences: audiences?.compactMap(PriceAudience.init(rawValue:))
        )
    }
}

private struct APIOffer: Decodable {
    let id: String
    let label: String
    let price: APIPrice
    let description: String?

    var model: LunchOffer {
        LunchOffer(
            id: id,
            label: label,
            price: price.model,
            description: description
        )
    }
}

private struct APIGroup: Decodable {
    let id: String
    let title: String?
    let prices: [APIPrice]
    let items: [APIItem]
}

private struct APIItem: Decodable {
    let name: String
    let description: String?
    let tags: [String]?
    let recipe: APIRecipe?
}

private struct APIRecipe: Decodable {
    let id: String
    let name: String?
    let ingredients: String?
    let nutritionPer100g: [APINutrition]?
    let co2eKilogramsPer100Grams: Double?
    let diets: [String]?

    var model: RecipeDetail {
        RecipeDetail(
            id: id,
            name: name ?? "",
            ingredients: ingredients ?? "",
            nutrition: nutritionPer100g?.map(\.model) ?? [],
            co2KilogramsPer100Grams: co2eKilogramsPer100Grams,
            diets: diets?.joined(separator: ", ") ?? ""
        )
    }
}

private struct APINutrition: Decodable {
    let name: String
    let amount: Double
    let unit: String

    var model: NutritionValue {
        NutritionValue(name: name, amount: amount, unit: unit)
    }
}
