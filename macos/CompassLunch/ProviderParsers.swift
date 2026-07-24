import Foundation

enum ProviderParsing {
    static func localDateString(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = .current
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }

    static func weekdayToken(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = .current
        formatter.dateFormat = "EEEE"
        return formatter.string(from: date).lowercased()
    }

    static func string(data: Data) throws -> String {
        guard let value = String(data: data, encoding: .utf8) else {
            throw MenuServiceError.invalidResponse
        }
        return value
    }

    static func firstCapture(
        _ pattern: String,
        in text: String,
        group: Int = 1,
        options: NSRegularExpression.Options = [.caseInsensitive, .dotMatchesLineSeparators]
    ) -> String? {
        guard let expression = try? NSRegularExpression(pattern: pattern, options: options) else {
            return nil
        }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = expression.firstMatch(in: text, range: range),
              group < match.numberOfRanges,
              let captureRange = Range(match.range(at: group), in: text)
        else {
            return nil
        }
        return String(text[captureRange])
    }

    static func captures(
        _ pattern: String,
        in text: String,
        group: Int = 1,
        options: NSRegularExpression.Options = [.caseInsensitive, .dotMatchesLineSeparators]
    ) -> [String] {
        guard let expression = try? NSRegularExpression(pattern: pattern, options: options) else {
            return []
        }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        return expression.matches(in: text, range: range).compactMap { match in
            guard group < match.numberOfRanges,
                  let captureRange = Range(match.range(at: group), in: text)
            else {
                return nil
            }
            return String(text[captureRange])
        }
    }

    static func htmlText(_ value: String) -> String {
        var text = value.replacingOccurrences(
            of: #"<br\s*/?>"#,
            with: "\n",
            options: [.regularExpression, .caseInsensitive]
        )
        text = text.replacingOccurrences(
            of: #"<[^>]+>"#,
            with: " ",
            options: .regularExpression
        )
        return decodeHTMLEntities(text).normalizedWhitespace
    }

    static func htmlLines(_ value: String) -> [String] {
        let withBreaks = value.replacingOccurrences(
            of: #"<br\s*/?>"#,
            with: "\n",
            options: [.regularExpression, .caseInsensitive]
        )
        return withBreaks.components(separatedBy: .newlines)
            .map(htmlText)
            .filter { !$0.isEmpty }
    }

    static func decodeHTMLEntities(_ value: String) -> String {
        var result = value
        let named = [
            "&nbsp;": " ", "&#160;": " ", "&amp;": "&", "&lt;": "<", "&gt;": ">",
            "&quot;": "\"", "&#39;": "'", "&apos;": "'", "&auml;": "ä", "&Auml;": "Ä",
            "&ouml;": "ö", "&Ouml;": "Ö", "&aring;": "å", "&Aring;": "Å",
            "&ndash;": "–", "&mdash;": "—", "&euro;": "€"
        ]
        for _ in 0..<2 {
            for (entity, replacement) in named {
                result = result.replacingOccurrences(of: entity, with: replacement)
            }
            result = decodeNumericEntities(result)
        }
        return result
    }

    static func inferredDate(
        day: Int,
        month: Int,
        year: Int?,
        near now: Date,
        maximumDistanceDays: Int? = nil
    ) -> Date? {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .current

        if let year {
            return calendar.date(from: DateComponents(year: year, month: month, day: day))
        }

        let currentYear = calendar.component(.year, from: now)
        let candidates = [currentYear - 1, currentYear, currentYear + 1].compactMap {
            calendar.date(from: DateComponents(year: $0, month: month, day: day))
        }
        let closest = candidates.min {
            abs($0.timeIntervalSince(now)) < abs($1.timeIntervalSince(now))
        }
        guard let closest else { return nil }
        if let maximumDistanceDays {
            let distance = abs(calendar.dateComponents([.day], from: closest, to: now).day ?? .max)
            guard distance <= maximumDistanceDays else { return nil }
        }
        return closest
    }

    private static func decodeNumericEntities(_ value: String) -> String {
        guard let expression = try? NSRegularExpression(pattern: #"&#(x?[0-9A-Fa-f]+);"#) else {
            return value
        }
        var result = value
        let matches = expression.matches(
            in: value,
            range: NSRange(value.startIndex..<value.endIndex, in: value)
        )
        for match in matches.reversed() {
            guard let fullRange = Range(match.range(at: 0), in: result),
                  let numberRange = Range(match.range(at: 1), in: result)
            else {
                continue
            }
            let raw = String(result[numberRange])
            let radix = raw.hasPrefix("x") ? 16 : 10
            let digits = raw.hasPrefix("x") ? String(raw.dropFirst()) : raw
            guard let scalarValue = UInt32(digits, radix: radix),
                  let scalar = UnicodeScalar(scalarValue)
            else {
                continue
            }
            result.replaceSubrange(fullRange, with: String(Character(scalar)))
        }
        return result
    }
}

enum RSSMenuParser {
    static func parse(
        data: Data,
        restaurant: Restaurant,
        requestedLanguage: AppLanguage,
        now: Date
    ) throws -> MenuSnapshot {
        let xml = try ProviderParsing.string(data: data)
        let channel = ProviderParsing.firstCapture(
            #"<channel(?:\s+[^>]*)?>(.*?)</channel>"#,
            in: xml
        ) ?? xml
        let item = ProviderParsing.firstCapture(#"<item\b[^>]*>(.*?)</item>"#, in: channel) ?? ""
        let channelTitle = tag("title", in: channel)
        let itemTitle = tag("title", in: item)
        let guid = tag("guid", in: item)
        let link = tag("link", in: item)
        let description = rawTag("description", in: item)
        let menuDate = parseDate(itemTitle) ?? parseDate(guid)
        let today = ProviderParsing.localDateString(now)
        let components = parseComponents(description)

        let menu = menuDate == today
            ? LunchMenu(
                date: today,
                lunchTime: "",
                groups: [
                    LunchGroup(
                        id: "rss-lunch",
                        name: requestedLanguage == .fi ? "Lounas" : "Lunch",
                        price: "",
                        components: components
                    )
                ]
            )
            : nil

        return MenuSnapshot(
            restaurantCode: restaurant.id,
            restaurantName: channelTitle.isEmpty ? restaurant.name : channelTitle,
            restaurantURL: URL(string: link) ?? restaurant.pageURL,
            language: requestedLanguage,
            fetchedAt: now,
            menu: menu
        )
    }

    private static func rawTag(_ name: String, in text: String) -> String {
        ProviderParsing.firstCapture(
            "<\(NSRegularExpression.escapedPattern(for: name))(?:\\s+[^>]*)?>(.*?)</\(NSRegularExpression.escapedPattern(for: name))>",
            in: text
        ) ?? ""
    }

    private static func tag(_ name: String, in text: String) -> String {
        ProviderParsing.htmlText(rawTag(name, in: text))
    }

    private static func parseDate(_ value: String) -> String? {
        guard let expression = try? NSRegularExpression(
            pattern: #"(\d{1,2})[-./](\d{1,2})[-./](\d{2,4})"#
        ) else {
            return nil
        }
        let range = NSRange(value.startIndex..<value.endIndex, in: value)
        guard let match = expression.firstMatch(in: value, range: range),
              let dayRange = Range(match.range(at: 1), in: value),
              let monthRange = Range(match.range(at: 2), in: value),
              let yearRange = Range(match.range(at: 3), in: value),
              let day = Int(value[dayRange]),
              let month = Int(value[monthRange]),
              var year = Int(value[yearRange])
        else {
            return nil
        }
        if year < 100 { year += 2000 }
        return String(format: "%04d-%02d-%02d", year, month, day)
    }

    private static func parseComponents(_ rawDescription: String) -> [String] {
        let decoded = ProviderParsing.decodeHTMLEntities(rawDescription)
        let paragraphs = ProviderParsing.captures(#"<p\b[^>]*>(.*?)</p>"#, in: decoded)
        let source = paragraphs.isEmpty ? [decoded] : paragraphs
        return source
            .map { normalizeComponent(ProviderParsing.htmlText($0)) }
            .filter { !$0.isEmpty }
    }

    private static func normalizeComponent(_ value: String) -> String {
        var line = value.trimmingCharacters(in: CharacterSet(charactersIn: " \t\n\r,;"))
        guard !line.isEmpty else { return "" }
        if line.range(
            of: #"\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\)\s*$"#,
            options: .regularExpression
        ) != nil {
            return line
        }

        var parts = line.split(separator: ",").map {
            String($0).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        var diets: [String] = []
        while let last = parts.last, isDiet(last) {
            diets.insert(normalizedDiet(last), at: 0)
            parts.removeLast()
        }
        guard !diets.isEmpty else { return line }

        line = parts.joined(separator: ", ").normalizedWhitespace
        if line.hasSuffix("*") {
            line = String(line.dropLast()).normalizedWhitespace
            diets.insert("*", at: 0)
        }
        while let token = line.split(separator: " ").last.map(String.init), isDiet(token) {
            diets.insert(normalizedDiet(token), at: 0)
            line = String(line.dropLast(token.count)).normalizedWhitespace
        }
        return line.isEmpty ? value : "\(line) (\(diets.joined(separator: ", ")))"
    }

    private static func isDiet(_ value: String) -> Bool {
        let clean = value.trimmingCharacters(in: CharacterSet(charactersIn: " .;:"))
        if clean == "*" { return true }
        let upper = clean.uppercased()
        return (upper.count == 1 && upper.allSatisfy(\.isLetter))
            || ["VEG", "VS", "ILM"].contains(upper)
    }

    private static func normalizedDiet(_ value: String) -> String {
        let upper = value.trimmingCharacters(in: CharacterSet(charactersIn: " .;:")).uppercased()
        return upper == "VEG" ? "Veg" : upper
    }
}

enum HuomenMenuParser {
    static func parse(
        data: Data,
        restaurant: Restaurant,
        requestedLanguage: AppLanguage,
        now: Date
    ) throws -> MenuSnapshot {
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw MenuServiceError.invalidResponse
        }
        if let success = root["success"] as? Bool, !success {
            let message = localized(root["message"], language: requestedLanguage)
            throw MenuServiceError.provider(
                message.isEmpty ? "The menu service returned an error." : message
            )
        }
        guard let dataObject = root["data"] as? [String: Any],
              let week = dataObject["week"] as? [String: Any],
              let days = week["days"] as? [[String: Any]]
        else {
            throw MenuServiceError.invalidResponse
        }

        let today = ProviderParsing.localDateString(now)
        let day = days.first { ($0["dateString"] as? String) == today }
        let isClosed = day?["isClosed"] as? Bool ?? false
        let lunches = isClosed ? [] : (day?["lunches"] as? [[String: Any]] ?? [])
        let components = lunches.compactMap { lunchLine($0, language: requestedLanguage) }
        let name = localized(
            (dataObject["location"] as? [String: Any])?["name"],
            language: requestedLanguage
        )
        let menu = day.map { _ in
            LunchMenu(
                date: today,
                lunchTime: "",
                groups: [
                    LunchGroup(
                        id: "huomen-lunch",
                        name: requestedLanguage == .fi ? "Lounas" : "Lunch",
                        price: requestedLanguage == .fi
                            ? "Lounas 12,90 € / Keittolounas 10,90 €"
                            : "Lunch 12,90 € / Soup lunch 10,90 €",
                        components: components
                    )
                ]
            )
        }

        return MenuSnapshot(
            restaurantCode: restaurant.id,
            restaurantName: name.isEmpty ? restaurant.name : name,
            restaurantURL: restaurant.pageURL,
            language: requestedLanguage,
            fetchedAt: now,
            menu: menu
        )
    }

    private static func lunchLine(
        _ lunch: [String: Any],
        language: AppLanguage
    ) -> String? {
        let title = localized(lunch["title"], language: language)
        guard !title.isEmpty else { return nil }
        let description = localized(lunch["description"], language: language)
        var line = title
        if !description.isEmpty, description != title {
            line += " - \(description)"
        }

        var seen = Set<String>()
        let allergens = (lunch["allergens"] as? [[String: Any]] ?? []).compactMap { allergen -> String? in
            let raw = localized(allergen["abbreviation"], language: language)
            guard !raw.isEmpty else { return nil }
            let token = raw.uppercased() == "VEG" ? "Veg" : raw.uppercased()
            guard seen.insert(token.uppercased()).inserted else { return nil }
            return token
        }
        if !allergens.isEmpty {
            line += " (\(allergens.joined(separator: ", ")))"
        }
        return line.normalizedWhitespace
    }

    private static func localized(_ value: Any?, language: AppLanguage) -> String {
        switch value {
        case let text as String:
            return text.normalizedWhitespace
        case let number as NSNumber:
            return number.stringValue
        case let dictionary as [String: Any]:
            for key in [language.rawValue, "fi", "en"] {
                let result = localized(dictionary[key], language: language)
                if !result.isEmpty { return result }
            }
            for candidate in dictionary.values {
                let result = localized(candidate, language: language)
                if !result.isEmpty { return result }
            }
            return ""
        case let values as [Any]:
            for candidate in values {
                let result = localized(candidate, language: language)
                if !result.isEmpty { return result }
            }
            return ""
        default:
            return ""
        }
    }
}

enum AntellMenuParser {
    static func parse(
        data: Data,
        restaurant: Restaurant,
        requestedLanguage: AppLanguage,
        now: Date
    ) throws -> MenuSnapshot {
        let html = try ProviderParsing.string(data: data)
        let today = ProviderParsing.localDateString(now)
        let menuDateText = ProviderParsing.firstCapture(
            #"<div\b[^>]*class=["'][^"']*\bmenu-date\b[^"']*["'][^>]*>(.*?)</div>"#,
            in: html
        ).map(ProviderParsing.htmlText) ?? ""
        let menuDate = parseMenuDate(menuDateText, now: now)
        let groups = parseGroups(html)

        return MenuSnapshot(
            restaurantCode: restaurant.id,
            restaurantName: restaurant.name,
            restaurantURL: restaurant.pageURL,
            language: requestedLanguage,
            fetchedAt: now,
            menu: menuDate == today
                ? LunchMenu(date: today, lunchTime: "", groups: groups)
                : nil
        )
    }

    private static func parseGroups(_ html: String) -> [LunchGroup] {
        ProviderParsing.captures(
            #"<section\b[^>]*class=["'][^"']*\bmenu-section\b[^"']*["'][^>]*>(.*?)</section>"#,
            in: html
        ).enumerated().compactMap { index, section in
            let items = ProviderParsing.captures(#"<li\b[^>]*>(.*?)</li>"#, in: section)
                .map(ProviderParsing.htmlText)
                .filter { !$0.isEmpty }
            guard !items.isEmpty else { return nil }
            let title = ProviderParsing.firstCapture(
                #"<h2\b[^>]*class=["'][^"']*\bmenu-title\b[^"']*["'][^>]*>(.*?)</h2>"#,
                in: section
            ).map(ProviderParsing.htmlText) ?? "Menu"
            let price = ProviderParsing.firstCapture(
                #"<h2\b[^>]*class=["'][^"']*\bmenu-price\b[^"']*["'][^>]*>(.*?)</h2>"#,
                in: section
            ).map(ProviderParsing.htmlText) ?? ""
            return LunchGroup(
                id: "antell-\(index)",
                name: title,
                price: price,
                components: items
            )
        }
    }

    private static func parseMenuDate(_ value: String, now: Date) -> String? {
        guard let expression = try? NSRegularExpression(pattern: #"(\d{1,2})[.](\d{1,2})(?:[.](\d{2,4}))?"#),
              let match = expression.firstMatch(
                in: value,
                range: NSRange(value.startIndex..<value.endIndex, in: value)
              ),
              let dayRange = Range(match.range(at: 1), in: value),
              let monthRange = Range(match.range(at: 2), in: value),
              let day = Int(value[dayRange]),
              let month = Int(value[monthRange])
        else {
            return nil
        }
        var year: Int?
        if match.range(at: 3).location != NSNotFound,
           let yearRange = Range(match.range(at: 3), in: value),
           var parsedYear = Int(value[yearRange]) {
            if parsedYear < 100 { parsedYear += 2000 }
            year = parsedYear
        }
        guard let date = ProviderParsing.inferredDate(day: day, month: month, year: year, near: now) else {
            return nil
        }
        return ProviderParsing.localDateString(date)
    }
}

enum PranzeriaMenuParser {
    static func parse(
        data: Data,
        restaurant: Restaurant,
        requestedLanguage: AppLanguage,
        now: Date
    ) throws -> MenuSnapshot {
        let html = try ProviderParsing.string(data: data)
        let today = ProviderParsing.localDateString(now)
        var linesByDate: [String: [String]] = [:]
        var currentDate: String?
        var prices: [String] = []

        for block in ProviderParsing.captures(
            #"<(?:p|h[1-6]|li)\b[^>]*>(.*?)</(?:p|h[1-6]|li)>"#,
            in: html
        ) {
            for line in ProviderParsing.htmlLines(block) {
                let foundPrices = priceParts(line)
                if !foundPrices.isEmpty {
                    for price in foundPrices where !prices.contains(price) {
                        prices.append(price)
                    }
                    continue
                }
                if let header = dayHeader(line, now: now) {
                    currentDate = header.date
                    linesByDate[header.date, default: []]
                        .append(contentsOf: header.trailing.isEmpty ? [] : [header.trailing])
                    continue
                }
                if isLegend(line) {
                    currentDate = nil
                    continue
                }
                guard let activeDate = currentDate else { continue }
                linesByDate[activeDate, default: []].append(normalizeMenuLine(line))
            }
        }

        let components = (linesByDate[today] ?? [])
            .map(\.normalizedWhitespace)
            .filter { !$0.isEmpty }
            .reduce(into: [String]()) { result, line in
                if result.last != line { result.append(line) }
            }
        let menu = linesByDate[today].map { _ in
            LunchMenu(
                date: today,
                lunchTime: "",
                groups: components.isEmpty ? [] : [
                    LunchGroup(
                        id: "pranzeria-lunch",
                        name: requestedLanguage == .fi ? "Lounas" : "Lunch",
                        price: orderedPrices(prices).joined(separator: " / "),
                        components: components
                    )
                ]
            )
        }

        return MenuSnapshot(
            restaurantCode: restaurant.id,
            restaurantName: restaurant.name,
            restaurantURL: restaurant.pageURL,
            language: requestedLanguage,
            fetchedAt: now,
            menu: menu
        )
    }

    private static func dayHeader(_ value: String, now: Date) -> (date: String, trailing: String)? {
        let weekdays = "maanantai|tiistai|keskiviikko|torstai|perjantai|lauantai|sunnuntai|monday|tuesday|wednesday|thursday|friday|saturday|sunday"
        let datePattern = #"(?:\d{4}[-/]\d{1,2}[-/]\d{1,2}|\d{1,2}[./-]\d{1,2}(?:[./-]\d{2,4})?\.?)"#
        let patterns = [
            #"^(?:"# + weekdays + #")\s+("# + datePattern + #")(.*)$"#,
            #"^("# + datePattern + #")(.*)$"#
        ]
        for pattern in patterns {
            guard let expression = try? NSRegularExpression(
                pattern: pattern,
                options: .caseInsensitive
            ) else {
                continue
            }
            let range = NSRange(value.startIndex..<value.endIndex, in: value)
            guard let match = expression.firstMatch(in: value, range: range),
                  let dateRange = Range(match.range(at: 1), in: value),
                  let restRange = Range(match.range(at: 2), in: value)
            else {
                continue
            }
            let dateText = String(value[dateRange])
            let rest = String(value[restRange]).normalizedWhitespace
            if dateText.range(of: #"[./]"#, options: .regularExpression) != nil,
               rest.range(of: #"^-\s*\d{1,2}[.:]\d{2}"#, options: .regularExpression) != nil {
                return nil
            }
            guard let parsedDate = parseDate(dateText, now: now) else { continue }
            let trailing = rest.trimmingCharacters(
                in: CharacterSet(charactersIn: " \t\n\r:,–-|/")
            ).normalizedWhitespace
            return (parsedDate, trailing)
        }
        return nil
    }

    private static func parseDate(_ value: String, now: Date) -> String? {
        if let expression = try? NSRegularExpression(pattern: #"^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$"#),
           let match = expression.firstMatch(
               in: value,
               range: NSRange(value.startIndex..<value.endIndex, in: value)
           ),
           let yearRange = Range(match.range(at: 1), in: value),
           let monthRange = Range(match.range(at: 2), in: value),
           let dayRange = Range(match.range(at: 3), in: value),
           let year = Int(value[yearRange]),
           let month = Int(value[monthRange]),
           let day = Int(value[dayRange]),
           let date = ProviderParsing.inferredDate(day: day, month: month, year: year, near: now) {
            return ProviderParsing.localDateString(date)
        }

        guard let expression = try? NSRegularExpression(
            pattern: #"^(\d{1,2})[./-](\d{1,2})(?:[./-](\d{2,4}))?\.?$"#
        ),
        let match = expression.firstMatch(
            in: value,
            range: NSRange(value.startIndex..<value.endIndex, in: value)
        ),
        let dayRange = Range(match.range(at: 1), in: value),
        let monthRange = Range(match.range(at: 2), in: value),
        let day = Int(value[dayRange]),
        let month = Int(value[monthRange])
        else {
            return nil
        }
        var year: Int?
        if match.range(at: 3).location != NSNotFound,
           let yearRange = Range(match.range(at: 3), in: value),
           var parsedYear = Int(value[yearRange]) {
            if parsedYear < 100 { parsedYear += 2000 }
            year = parsedYear
        }
        guard let date = ProviderParsing.inferredDate(
            day: day,
            month: month,
            year: year,
            near: now,
            maximumDistanceDays: year == nil ? 14 : nil
        ) else {
            return nil
        }
        return ProviderParsing.localDateString(date)
    }

    private static func priceParts(_ line: String) -> [String] {
        let pattern = #"(SALAATTILOUNAS|LOUNASBUFFET|SOPIMUSLOUNAS)\b\s+(\d{1,2}[,.]\d{2})\s*€"#
        guard let expression = try? NSRegularExpression(
            pattern: pattern,
            options: .caseInsensitive
        ) else {
            return []
        }
        let range = NSRange(line.startIndex..<line.endIndex, in: line)
        return expression.matches(in: line, range: range).compactMap { match in
            guard let labelRange = Range(match.range(at: 1), in: line),
                  let priceRange = Range(match.range(at: 2), in: line)
            else {
                return nil
            }
            let label: String
            switch line[labelRange].uppercased() {
            case "SALAATTILOUNAS": label = "Salaattilounas"
            case "LOUNASBUFFET": label = "Lounasbuffet"
            case "SOPIMUSLOUNAS": label = "Sopimuslounas"
            default: label = "Lounas"
            }
            return "\(label) \(line[priceRange].replacingOccurrences(of: ".", with: ",")) €"
        }
    }

    private static func orderedPrices(_ prices: [String]) -> [String] {
        let labels = ["Salaattilounas", "Lounasbuffet", "Sopimuslounas"]
        return prices.sorted { leftPrice, rightPrice in
            let left = labels.firstIndex(where: leftPrice.hasPrefix) ?? labels.count
            let right = labels.firstIndex(where: rightPrice.hasPrefix) ?? labels.count
            return left < right
        }
    }

    private static func isLegend(_ line: String) -> Bool {
        line.range(of: #"^(?:L|G|M|V|VG)\s*="#, options: .regularExpression) != nil
            || ["Laktoositon", "Gluteeniton", "Maidoton", "Kasvis", "Vegaani"]
                .contains(where: line.contains)
    }

    private static func normalizeMenuLine(_ value: String) -> String {
        var line = value.normalizedWhitespace
        line = line.replacingOccurrences(
            of: #"(?i)pyydet(?:t|)äessä\s+G"#,
            with: "G",
            options: .regularExpression
        )
        if line.hasSuffix(")") { return line }

        var parts = line.split(separator: ",").map {
            String($0).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        var diets: [String] = []
        while let last = parts.last {
            let tokens = last.split(separator: " ").map(String.init)
            guard !tokens.isEmpty,
                  tokens.allSatisfy({ ["G", "L", "M", "V", "VG"].contains($0.uppercased()) })
            else {
                break
            }
            diets.insert(contentsOf: tokens.map { $0.uppercased() }, at: 0)
            parts.removeLast()
        }
        guard !diets.isEmpty, !parts.isEmpty else { return line }
        return "\(parts.joined(separator: ", ")) (\(diets.joined(separator: ", ")))"
    }
}
