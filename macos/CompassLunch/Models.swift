import Foundation

enum OpeningHoursStatus: Equatable {
    case unknown
    case open
    case closingSoon
    case closed
}

enum OpeningHoursClock {
    private static let closingSoonMinutes = 15
    private static let timePattern = try! NSRegularExpression(
        pattern: #"(?<!\d)(\d{1,2}):(\d{2})(?!\d)"#
    )

    static func status(for hours: String, at date: Date) -> OpeningHoursStatus {
        let times = timeTokens(in: hours)
        guard times.count >= 2 else { return .unknown }

        let opensAt = times[0]
        let closesAt = times[1]
        guard closesAt > opensAt else { return .unknown }

        let now = helsinkiMinutes(at: date)
        guard now >= opensAt, now < closesAt else { return .closed }
        return closesAt - now <= closingSoonMinutes ? .closingSoon : .open
    }

    private static func timeTokens(in value: String) -> [Int] {
        let range = NSRange(value.startIndex..<value.endIndex, in: value)
        return timePattern.matches(in: value, range: range).compactMap { match in
            guard
                let hourRange = Range(match.range(at: 1), in: value),
                let minuteRange = Range(match.range(at: 2), in: value),
                let hour = Int(value[hourRange]),
                let minute = Int(value[minuteRange]),
                hour < 24,
                minute < 60
            else {
                return nil
            }
            return hour * 60 + minute
        }
    }

    private static func helsinkiMinutes(at date: Date) -> Int {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Helsinki")!
        let components = calendar.dateComponents([.hour, .minute], from: date)
        return (components.hour ?? 0) * 60 + (components.minute ?? 0)
    }
}

enum AppLanguage: String, CaseIterable, Identifiable, Codable {
    case fi
    case en

    var id: String { rawValue }

    var title: String {
        switch self {
        case .fi: "Suomi"
        case .en: "English"
        }
    }
}

enum LunchLayout: String, CaseIterable, Identifiable, Codable {
    case legacy
    case standard
    case compact

    var id: String { rawValue }

    var title: String {
        switch self {
        case .legacy: "Classic"
        case .standard: "Standard"
        case .compact: "Compact"
        }
    }
}

struct LocalDate: Hashable, Comparable, Codable {
    let year: Int
    let month: Int
    let day: Int

    static func < (left: LocalDate, right: LocalDate) -> Bool {
        (left.year, left.month, left.day) < (right.year, right.month, right.day)
    }

    static func today(calendar: Calendar = .current) -> LocalDate {
        let components = calendar.dateComponents([.year, .month, .day], from: Date())
        return LocalDate(
            year: components.year ?? 0,
            month: components.month ?? 0,
            day: components.day ?? 0
        )
    }
}

struct SeasonalClosure: Hashable, Codable {
    let start: LocalDate
    let end: LocalDate
    let reason: String?

    init(start: LocalDate, end: LocalDate, reason: String? = nil) {
        self.start = start
        self.end = end
        self.reason = reason
    }

    func contains(_ date: LocalDate) -> Bool {
        start <= date && date <= end
    }

    func noticeText(language: AppLanguage, referenceYear: Int) -> String {
        let includeYear = end.year != referenceYear
        let date: String
        let notice: String

        switch language {
        case .fi:
            let months = [
                "tammikuuta", "helmikuuta", "maaliskuuta", "huhtikuuta",
                "toukokuuta", "kesäkuuta", "heinäkuuta", "elokuuta",
                "syyskuuta", "lokakuuta", "marraskuuta", "joulukuuta"
            ]
            let month = months.indices.contains(end.month - 1)
                ? months[end.month - 1]
                : String(end.month)
            date = "\(end.day). \(month)" + (includeYear ? " \(end.year)" : "")
            notice = "Suljettu \(date) asti"
        case .en:
            let months = [
                "January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December"
            ]
            let month = months.indices.contains(end.month - 1)
                ? months[end.month - 1]
                : String(end.month)
            date = "\(end.day) \(month)" + (includeYear ? " \(end.year)" : "")
            notice = "Closed until \(date)"
        }

        guard let reason = reason?.normalizedWhitespace, !reason.isEmpty else {
            return notice
        }
        return "\(notice) · \(reason)"
    }
}

struct Restaurant: Identifiable, Hashable, Codable {
    let id: String
    let name: String
    let pageURL: URL?
    let languages: [AppLanguage]

    static let fallbackRestaurants: [Restaurant] = [
        Restaurant(id: "snellmania", name: "Snellmania", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "cafe-snellari", name: "Cafe Snellari", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "canthia", name: "Canthia", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "tietoteknia", name: "Tietoteknia", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "hyva-huomen-bioteknia", name: "Hyvä Huomen Bioteknia", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "antell-round", name: "Antell Round", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "antell-highway", name: "Antell Highway", pageURL: nil, languages: [.fi]),
        Restaurant(id: "mediteknia", name: "Mediteknia", pageURL: nil, languages: [.fi, .en]),
        Restaurant(id: "pranzeria-sorrento", name: "Pranzeria Sorrento", pageURL: nil, languages: [.fi]),
        Restaurant(id: "caari", name: "Caari", pageURL: nil, languages: [.fi])
    ]

    static let legacyIDs: [String: String] = [
        "0437": "snellmania",
        "snellari-rss": "cafe-snellari",
        "0436": "canthia",
        "0439": "tietoteknia",
        "huomen-bioteknia": "hyva-huomen-bioteknia",
        "043601": "mediteknia",
        "pranzeria-html": "pranzeria-sorrento",
        "3488": "caari"
    ]

    static func migratedID(_ id: String) -> String {
        legacyIDs[id] ?? id
    }
}

struct LunchMenu: Codable, Equatable {
    let date: String
    let lunchTime: String
    let offers: [LunchOffer]
    let groups: [LunchGroup]

    init(
        date: String,
        lunchTime: String,
        offers: [LunchOffer] = [],
        groups: [LunchGroup]
    ) {
        self.date = date
        self.lunchTime = lunchTime
        self.offers = offers
        self.groups = groups
    }
}

enum PriceAudience: String, Codable, Hashable {
    case student
    case staff
    case guest
}

struct LunchPrice: Codable, Equatable, Hashable {
    let amount: String
    let audiences: [PriceAudience]?

    var displayText: String {
        "\(amount.replacingOccurrences(of: ".", with: ",")) €"
    }

    func isVisible(for selection: PriceSelection) -> Bool {
        guard let audiences else { return true }
        return audiences.contains {
            switch $0 {
            case .student: selection.student
            case .staff: selection.staff
            case .guest: selection.guest
            }
        }
    }
}

struct LunchOffer: Codable, Equatable, Identifiable {
    let id: String
    let label: String
    let price: LunchPrice
    let description: String?
}

struct LunchGroup: Codable, Equatable, Identifiable {
    let id: String
    let name: String
    let price: String
    let prices: [LunchPrice]?
    let components: [String]
    let componentDetails: [RecipeDetail?]?

    init(
        id: String,
        name: String,
        price: String,
        prices: [LunchPrice]? = nil,
        components: [String],
        componentDetails: [RecipeDetail?]? = nil
    ) {
        self.id = id
        self.name = name
        self.price = price
        self.prices = prices
        self.components = components
        self.componentDetails = componentDetails
    }

    var normalizedPrice: String {
        PriceFormatter.normalize(price)
    }

    var concisePrice: String {
        PriceFormatter.removingGroupNames(from: price)
    }

    var priceValues: [Double] {
        if let prices {
            return prices.compactMap { Double($0.amount) }
        }
        return PriceFormatter.values(in: price)
    }

    func detail(at componentIndex: Int) -> RecipeDetail? {
        guard let componentDetails,
              componentDetails.indices.contains(componentIndex)
        else {
            return nil
        }
        return componentDetails[componentIndex]
    }
}

struct RecipeDetail: Codable, Equatable, Identifiable {
    let id: String
    let name: String
    let ingredients: String
    let nutrition: [NutritionValue]
    let co2KilogramsPer100Grams: Double?
    let diets: String

    var hasDisplayContent: Bool {
        !ingredients.normalizedWhitespace.isEmpty
            || !nutrition.isEmpty
            || co2KilogramsPer100Grams != nil
            || !diets.normalizedWhitespace.isEmpty
    }
}

struct NutritionValue: Codable, Equatable {
    let name: String
    let amount: Double
    let unit: String

    func displayText(amountText: String, label: String) -> String {
        let cleanUnit = unit.normalizedWhitespace
        let cleanLabel = label.normalizedWhitespace
        var parts = [amountText]
        if !cleanUnit.isEmpty {
            parts.append(cleanUnit)
        }
        if !cleanLabel.isEmpty,
           cleanLabel.caseInsensitiveCompare(cleanUnit) != .orderedSame {
            parts.append(cleanLabel)
        }
        return parts.joined(separator: " ")
    }
}

enum TextHighlight {
    static func matches(_ text: String, highlights: [String]) -> Bool {
        let haystack = normalized(text)
        return highlights.contains {
            let needle = normalized($0)
            return !needle.isEmpty && haystack.contains(needle)
        }
    }

    static func containsExact(_ text: String, in highlights: [String]) -> Bool {
        let key = normalized(text)
        return highlights.contains { normalized($0) == key }
    }

    static func matchingRanges(
        in text: String,
        highlights: [String]
    ) -> [Range<String.Index>] {
        var ranges: [Range<String.Index>] = []
        let options: String.CompareOptions = [
            .caseInsensitive,
            .diacriticInsensitive
        ]

        for highlight in highlights {
            let needle = highlight.normalizedWhitespace
            guard !needle.isEmpty else { continue }

            var searchStart = text.startIndex
            while searchStart < text.endIndex,
                  let range = text.range(
                      of: needle,
                      options: options,
                      range: searchStart..<text.endIndex,
                      locale: .current
                  ) {
                ranges.append(range)
                searchStart = range.upperBound
            }
        }

        let sorted = ranges.sorted {
            if $0.lowerBound == $1.lowerBound {
                return $0.upperBound < $1.upperBound
            }
            return $0.lowerBound < $1.lowerBound
        }
        var merged: [Range<String.Index>] = []
        for range in sorted {
            guard let last = merged.last,
                  range.lowerBound <= last.upperBound
            else {
                merged.append(range)
                continue
            }
            merged[merged.count - 1] =
                last.lowerBound..<max(last.upperBound, range.upperBound)
        }
        return merged
    }

    static func normalized(_ value: String) -> String {
        value.normalizedWhitespace.folding(
            options: [.caseInsensitive, .diacriticInsensitive],
            locale: .current
        )
    }
}

struct PriceSelection: Equatable {
    var student: Bool
    var staff: Bool
    var guest: Bool

    var hasAnySelection: Bool {
        student || staff || guest
    }
}

enum PriceFormatter {
    private static let groupLabelPattern =
        #"(?i)\b(opiskelija|opisk|op|student|henkilökunta|henkilokunta|staff|hk|vierailija|vieras|guest)\b([.:]?)\s*"#
    private static let leadingGroupLabelPattern =
        #"(?i)^(?:opiskelija|opisk|op|student|henkilökunta|henkilokunta|staff|hk|vierailija|vieras|guest)\b[ .:–-]*"#

    static func normalize(_ text: String) -> String {
        var normalized = normalizeDecimals(text.normalizedWhitespace)
        normalized = normalized.replacingOccurrences(
            of: #"(?i)EUR\b"#,
            with: "€",
            options: .regularExpression
        )
        normalized = normalized.replacingOccurrences(
            of: #"\s*€"#,
            with: " €",
            options: .regularExpression
        )
        normalized = normalized.replacingOccurrences(
            of: #"\s*/\s*"#,
            with: " / ",
            options: .regularExpression
        )
        normalized = normalized.replacingOccurrences(
            of: groupLabelPattern,
            with: "$1$2 ",
            options: .regularExpression
        )
        return normalized.normalizedWhitespace
    }

    static func removingGroupNames(from text: String) -> String {
        parseEntries(text)
            .map { removingGroupName(from: $0.text) }
            .filter { !$0.isEmpty }
            .joined(separator: " / ")
    }

    static func displayPrice(
        _ text: String,
        restaurantCode: String,
        selection: PriceSelection
    ) -> String {
        let entries = parseEntries(text)
        guard !entries.isEmpty else { return "" }

        if restaurantCode == "0439" {
            return tietotekniaDisplayPrice(entries, selection: selection)
        }

        return entries.compactMap { entry in
            includes(entry.group, in: selection)
                ? removingGroupName(from: entry.text)
                : nil
        }
        .joined(separator: " / ")
    }

    static func values(in text: String) -> [Double] {
        let normalized = normalize(text)
        let pattern = #"\d+(?:[.,]\d+)?"#
        guard let expression = try? NSRegularExpression(pattern: pattern) else { return [] }
        let range = NSRange(normalized.startIndex..<normalized.endIndex, in: normalized)
        return expression.matches(in: normalized, range: range).compactMap { match in
            guard let range = Range(match.range, in: normalized) else { return nil }
            return Double(normalized[range].replacingOccurrences(of: ",", with: "."))
        }
    }

    private static func normalizeDecimals(_ text: String) -> String {
        let characters = Array(text)
        var output = ""
        var index = 0

        while index < characters.count {
            let character = characters[index]
            output.append(character)
            index += 1

            guard character.isNumber else { continue }

            while index < characters.count, characters[index].isNumber {
                output.append(characters[index])
                index += 1
            }

            guard index < characters.count,
                  characters[index] == "," || characters[index] == ".",
                  index + 1 < characters.count,
                  characters[index + 1].isNumber
            else {
                continue
            }

            output.append(characters[index])
            index += 1

            var decimalCount = 0
            while index < characters.count, characters[index].isNumber {
                if decimalCount < 2 {
                    output.append(characters[index])
                }
                decimalCount += 1
                index += 1
            }
        }

        return output
    }

    private static func parseEntries(_ text: String) -> [PriceEntry] {
        splitSegments(normalize(text)).map { segment in
            PriceEntry(
                group: classify(segment),
                text: segment
            )
        }
    }

    private static func splitSegments(_ text: String) -> [String] {
        let slashSegments = text
            .split(separator: "/")
            .map { String($0).normalizedWhitespace }
            .filter { !$0.isEmpty }
        if slashSegments.count > 1 {
            return slashSegments
        }

        guard let expression = try? NSRegularExpression(pattern: groupLabelPattern) else {
            return slashSegments
        }
        let fullRange = NSRange(text.startIndex..<text.endIndex, in: text)
        let matches = expression.matches(in: text, range: fullRange)
        guard matches.count > 1 else {
            return slashSegments.isEmpty ? [] : slashSegments
        }

        return matches.enumerated().compactMap { index, match in
            let start = match.range.location
            let end = index + 1 < matches.count
                ? matches[index + 1].range.location
                : fullRange.length
            guard let range = Range(NSRange(location: start, length: end - start), in: text) else {
                return nil
            }
            let segment = String(text[range]).normalizedWhitespace
            return segment.isEmpty ? nil : segment
        }
    }

    private static func classify(_ segment: String) -> PriceGroup {
        if containsLabel(
            in: segment,
            pattern: #"(?i)\b(?:opiskelija|opisk|op|student)\b"#
        ) {
            return .student
        }
        if containsLabel(
            in: segment,
            pattern: #"(?i)\b(?:henkilökunta|henkilokunta|staff|hk)\b"#
        ) {
            return .staff
        }
        if containsLabel(
            in: segment,
            pattern: #"(?i)\b(?:vierailija|vieras|guest)\b"#
        ) {
            return .guest
        }
        return .unlabelled
    }

    private static func containsLabel(in text: String, pattern: String) -> Bool {
        text.range(of: pattern, options: .regularExpression) != nil
    }

    private static func removingGroupName(from segment: String) -> String {
        segment.replacingOccurrences(
            of: leadingGroupLabelPattern,
            with: "",
            options: .regularExpression
        )
        .normalizedWhitespace
    }

    private static func includes(_ group: PriceGroup, in selection: PriceSelection) -> Bool {
        switch group {
        case .student: selection.student
        case .staff: selection.staff
        case .guest: selection.guest
        case .unlabelled: selection.guest
        }
    }

    private static func tietotekniaDisplayPrice(
        _ entries: [PriceEntry],
        selection: PriceSelection
    ) -> String {
        let unlabelledIndices = entries.indices.filter {
            entries[$0].group == .unlabelled
        }
        let hasExplicitStudent = entries.contains { $0.group == .student }

        if entries.count == 1 {
            return selection.hasAnySelection
                ? removingGroupName(from: entries[0].text)
                : ""
        }

        let inferredStudentIndex = hasExplicitStudent
            ? nil
            : unlabelledIndices.last

        return entries.enumerated().compactMap { index, entry in
            let include: Bool
            switch entry.group {
            case .student:
                include = selection.student
            case .staff:
                include = selection.staff
            case .guest:
                include = selection.guest
            case .unlabelled:
                include = index == inferredStudentIndex
                    ? selection.student
                    : (selection.staff || selection.guest)
            }
            return include ? removingGroupName(from: entry.text) : nil
        }
        .joined(separator: " / ")
    }

    private enum PriceGroup {
        case student
        case staff
        case guest
        case unlabelled
    }

    private struct PriceEntry {
        let group: PriceGroup
        let text: String
    }
}

struct MenuSnapshot: Codable, Equatable {
    enum ServiceStatus: String, Codable {
        case serving
        case closed
        case noMenu
        case unknown
    }

    let restaurantCode: String
    let restaurantName: String
    let restaurantURL: URL?
    let language: AppLanguage
    let fetchedAt: Date
    let menu: LunchMenu?
    let closure: SeasonalClosure?
    let serviceStatus: ServiceStatus?
    let isStale: Bool?

    init(
        restaurantCode: String,
        restaurantName: String,
        restaurantURL: URL?,
        language: AppLanguage,
        fetchedAt: Date,
        menu: LunchMenu?,
        closure: SeasonalClosure? = nil,
        serviceStatus: ServiceStatus? = nil,
        isStale: Bool = false
    ) {
        self.restaurantCode = restaurantCode
        self.restaurantName = restaurantName
        self.restaurantURL = restaurantURL
        self.language = language
        self.fetchedAt = fetchedAt
        self.menu = menu
        self.closure = closure
        self.serviceStatus = serviceStatus
        self.isStale = isStale
    }

    var effectiveServiceStatus: ServiceStatus {
        if let serviceStatus {
            return serviceStatus
        }
        if closure != nil {
            return .closed
        }
        if let menu,
           !menu.groups.isEmpty || !menu.offers.isEmpty {
            return .serving
        }
        return .noMenu
    }
}

struct ComponentParts: Equatable {
    let name: String
    let diets: String

    init(_ component: String) {
        var main = component.normalizedWhitespace
        var tokens: [DietToken] = []

        while main.last == ")",
              let opening = Self.matchingOpeningParenthesis(in: main) {
            let insideStart = main.index(after: opening)
            let insideEnd = main.index(before: main.endIndex)
            let inside = String(main[insideStart..<insideEnd]).normalizedWhitespace
            if inside.isEmpty {
                main = String(main[..<opening]).normalizedWhitespace
                continue
            }

            let candidates = inside.contains(",")
                ? inside.split(separator: ",").map(String.init)
                : inside.split(whereSeparator: \.isWhitespace).map(String.init)
            let parsed = candidates.compactMap(Self.dietToken)
            guard parsed.count == candidates.count else { break }
            tokens.insert(contentsOf: parsed, at: 0)
            main = String(main[..<opening]).normalizedWhitespace
        }

        let inline = Self.inlineDietTokens(in: main)
        if !inline.tokens.isEmpty {
            main = inline.main
            tokens.insert(contentsOf: inline.tokens, at: 0)
        }

        var seen = Set<String>()
        let uniqueTokens = tokens.filter {
            seen.insert($0.normalized).inserted
        }
        name = main.trimmingCharacters(in: CharacterSet(charactersIn: " ,;:"))
        diets = uniqueTokens.map(\.display).joined(separator: ", ")
    }

    private static func matchingOpeningParenthesis(in value: String) -> String.Index? {
        var depth = 0
        for index in value.indices.reversed() {
            if value[index] == ")" {
                depth += 1
            } else if value[index] == "(" {
                depth -= 1
                if depth == 0 { return index }
                if depth < 0 { return nil }
            }
        }
        return nil
    }

    private static func inlineDietTokens(
        in value: String
    ) -> (main: String, tokens: [DietToken]) {
        let compact = value.trimmingCharacters(in: CharacterSet(charactersIn: " ,;:."))
        let parts = compact.split(separator: ",").map {
            String($0).normalizedWhitespace
        }

        if parts.count < 2 {
            guard let peeled = peelLastDietToken(from: compact) else {
                return (compact, [])
            }
            return (peeled.main, [peeled.token])
        }

        var mainParts = parts
        var tokens: [DietToken] = []
        while let last = mainParts.last,
              let token = dietTokenOrRequested(last) {
            tokens.insert(token, at: 0)
            mainParts.removeLast()
        }
        guard !tokens.isEmpty else { return (compact, []) }

        var main = mainParts.joined(separator: ", ").normalizedWhitespace
        while let peeled = peelLastDietToken(from: main) {
            main = peeled.main
            tokens.insert(peeled.token, at: 0)
        }
        return main.isEmpty ? (compact, []) : (main, tokens)
    }

    private static func peelLastDietToken(
        from value: String
    ) -> (main: String, token: DietToken)? {
        let clean = value.normalizedWhitespace
        guard let split = clean.lastIndex(where: \.isWhitespace) else { return nil }
        let candidateStart = clean.index(after: split)
        guard var token = dietToken(String(clean[candidateStart...])) else { return nil }

        var main = String(clean[..<split]).normalizedWhitespace
        if main.range(
            of: #"(?i)\bpyydet(?:t)?[äa]ess[äa]$"#,
            options: .regularExpression
        ) != nil {
            main = main.replacingOccurrences(
                of: #"(?i)\bpyydet(?:t)?[äa]ess[äa]$"#,
                with: "",
                options: .regularExpression
            ).normalizedWhitespace
            token = DietToken(
                normalized: token.normalized,
                display: "Pyydettäessä \(token.display)"
            )
        }
        return main.isEmpty ? nil : (main, token)
    }

    private static func dietTokenOrRequested(_ value: String) -> DietToken? {
        if let token = dietToken(value) { return token }
        guard let expression = try? NSRegularExpression(
            pattern: #"(?i)^pyydet(?:t)?[äa]ess[äa]\s+(.+)$"#
        ) else {
            return nil
        }
        let range = NSRange(value.startIndex..<value.endIndex, in: value)
        guard let match = expression.firstMatch(in: value, range: range),
              let tokenRange = Range(match.range(at: 1), in: value),
              let token = dietToken(String(value[tokenRange]))
        else {
            return nil
        }
        return DietToken(
            normalized: token.normalized,
            display: "Pyydettäessä \(token.display)"
        )
    }

    private static func dietToken(_ value: String) -> DietToken? {
        let clean = value.normalizedWhitespace.trimmingCharacters(
            in: CharacterSet(charactersIn: "(),;:.")
        )
        guard !clean.isEmpty else { return nil }
        if clean == "*" {
            return DietToken(normalized: "*", display: "*")
        }

        let upper = clean.uppercased()
        if upper.count == 1, upper.allSatisfy(\.isLetter) {
            return DietToken(normalized: upper, display: upper)
        }
        switch upper {
        case "ILM", "VS", "VL":
            return DietToken(normalized: upper, display: upper)
        case "VEG":
            return DietToken(normalized: "VEG", display: "Veg")
        default:
            return nil
        }
    }

    private struct DietToken {
        let normalized: String
        let display: String
    }
}

extension String {
    var normalizedWhitespace: String {
        split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }
}

extension LunchMenu {
    var groupsWithItems: [LunchGroup] {
        groups.filter {
            $0.components.contains { !$0.normalizedWhitespace.isEmpty }
        }
    }

    var groupsByDescendingPrice: [LunchGroup] {
        groupsByDescendingPrice { $0.normalizedPrice }
    }

    func groupsWithItemsByDescendingPrice(
        priceText: (LunchGroup) -> String
    ) -> [LunchGroup] {
        LunchMenu(
            date: date,
            lunchTime: lunchTime,
            groups: groupsWithItems
        ).groupsByDescendingPrice(priceText: priceText)
    }

    func groupsByDescendingPrice(
        priceText: (LunchGroup) -> String
    ) -> [LunchGroup] {
        groups.enumerated()
            .map { offset, group in
                let displayedPrice = priceText(group)
                return (
                    offset: offset,
                    group: group,
                    priceValues: PriceFormatter.values(
                        in: displayedPrice.isEmpty ? group.price : displayedPrice
                    )
                )
            }
            .sorted { left, right in
                let comparison = comparePriceValues(
                    left.priceValues,
                    right.priceValues
                )
                return comparison == 0 ? left.offset < right.offset : comparison > 0
            }
            .map(\.group)
    }

    private func comparePriceValues(_ left: [Double], _ right: [Double]) -> Int {
        for index in 0..<max(left.count, right.count) {
            guard left.indices.contains(index) else { return -1 }
            guard right.indices.contains(index) else { return 1 }
            if left[index] > right[index] { return 1 }
            if left[index] < right[index] { return -1 }
        }
        return 0
    }
}
