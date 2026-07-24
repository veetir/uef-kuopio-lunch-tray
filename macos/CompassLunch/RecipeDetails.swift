import Foundation

enum RecipeDetailEnrichment {
    static func applying(
        _ detailsByMealName: [String: RecipeDetail],
        to snapshot: MenuSnapshot
    ) -> MenuSnapshot {
        guard let menu = snapshot.menu, !detailsByMealName.isEmpty else {
            return snapshot.markingDetailEnrichmentAttempted()
        }

        let groups = menu.groups.map { group in
            let details = group.components.map {
                detailsByMealName[mealKey($0)]
            }
            return LunchGroup(
                id: group.id,
                name: group.name,
                price: group.price,
                components: group.components,
                componentDetails: details
            )
        }
        return MenuSnapshot(
            restaurantCode: snapshot.restaurantCode,
            restaurantName: snapshot.restaurantName,
            restaurantURL: snapshot.restaurantURL,
            language: snapshot.language,
            fetchedAt: snapshot.fetchedAt,
            menu: LunchMenu(
                date: menu.date,
                lunchTime: menu.lunchTime,
                groups: groups
            ),
            detailEnrichmentAttempted: true
        )
    }

    static func mealKey(_ value: String) -> String {
        ComponentParts(value).name
            .normalizedWhitespace
            .folding(
                options: [.caseInsensitive, .diacriticInsensitive],
                locale: Locale(identifier: "en_US_POSIX")
            )
    }
}

enum CompassRecipeDetailParser {
    static func references(fromRestaurantHTML html: String) -> [String: Int] {
        guard let json = initialMenuJSON(in: html),
              let root = try? JSONSerialization.jsonObject(with: Data(json.utf8))
                as? [String: Any],
              let dayMenu = root["dayMenu"] as? [String: Any],
              let packages = dayMenu["menuPackages"] as? [[String: Any]]
        else {
            return [:]
        }

        var references: [String: Int] = [:]
        for package in packages {
            for meal in package["meals"] as? [[String: Any]] ?? [] {
                guard let name = meal["name"] as? String,
                      let recipeID = (meal["recipeId"] as? NSNumber)?.intValue,
                      recipeID > 0
                else {
                    continue
                }
                let key = RecipeDetailEnrichment.mealKey(name)
                if !key.isEmpty {
                    references[key] = recipeID
                }
            }
        }
        return references
    }

    static func parse(data: Data, fallbackRecipeID: Int) -> RecipeDetail? {
        guard let payload = try? JSONDecoder().decode(
            CompassRecipePayload.self,
            from: data
        ) else {
            return nil
        }
        let recipeID = payload.recipeID ?? fallbackRecipeID
        let detail = RecipeDetail(
            id: "compass-\(recipeID)",
            name: payload.name?.normalizedWhitespace ?? "",
            ingredients: payload.ingredientsCleaned?.normalizedWhitespace ?? "",
            nutrition: (payload.nutritionalValues ?? []).compactMap { value in
                guard let name = value.name?.normalizedWhitespace,
                      !name.isEmpty,
                      let amount = value.amount
                else {
                    return nil
                }
                return NutritionValue(
                    name: name,
                    amount: amount,
                    unit: value.unit?.normalizedWhitespace ?? ""
                )
            },
            co2KilogramsPer100Grams: payload.co2KilogramsPer100Grams,
            diets: payload.diets?.normalizedWhitespace ?? ""
        )
        return detail.hasDisplayContent ? detail : nil
    }

    private static func initialMenuJSON(in html: String) -> String? {
        guard let marker = html.range(of: "window.__INITIAL_MENU__"),
              let equals = html[marker.upperBound...].firstIndex(of: "="),
              let start = html[html.index(after: equals)...].firstIndex(of: "{")
        else {
            return nil
        }

        var depth = 0
        var inString = false
        var escaped = false
        var index = start
        while index < html.endIndex {
            let character = html[index]
            if inString {
                if escaped {
                    escaped = false
                } else if character == "\\" {
                    escaped = true
                } else if character == "\"" {
                    inString = false
                }
            } else if character == "\"" {
                inString = true
            } else if character == "{" {
                depth += 1
            } else if character == "}" {
                depth -= 1
                if depth == 0 {
                    return String(html[start...index])
                }
            }
            index = html.index(after: index)
        }
        return nil
    }
}

enum AntellRecipeDetailParser {
    static func details(from html: String, weekday: String) -> [String: RecipeDetail] {
        let panelID = "panel-\(weekday.prefix(1).uppercased())\(weekday.dropFirst())"
        let escapedPanelID = NSRegularExpression.escapedPattern(for: panelID)
        let panel = ProviderParsing.firstCapture(
            #"<section\b[^>]*id=["']"# + escapedPanelID + #"["'][^>]*>(.*?)</section>"#,
            in: html
        ) ?? html

        var details: [String: RecipeDetail] = [:]
        for item in ProviderParsing.captures(#"<li\b[^>]*>(.*?)</li>"#, in: panel) {
            guard let rawName = ProviderParsing.firstCapture(
                #"<button\b[^>]*class=["'][^"']*\baccordion__button\b[^"']*["'][^>]*>(.*?)</button>"#,
                in: item
            ) else {
                continue
            }
            let name = ProviderParsing.htmlText(rawName)
            guard !name.isEmpty else { continue }

            let content = ProviderParsing.firstCapture(
                #"<div\b[^>]*class=["'][^"']*\baccordion__content\b[^"']*["'][^>]*>(.*?)</div>"#,
                in: item
            ) ?? item
            let tooltipIngredients = ProviderParsing.firstCapture(
                #"<div\b[^>]*class=["'][^"']*\btooltip__body\b[^"']*["'][^>]*>(.*?)</div>"#,
                in: item
            ).map(ProviderParsing.htmlText) ?? ""
            let ingredients = tooltipIngredients.isEmpty
                ? labeledParagraph(
                    in: content,
                    labels: ["Ainesosat", "Ingredients"]
                )
                : tooltipIngredients
            guard !ingredients.isEmpty else { continue }

            let nutritionLine = labeledParagraph(
                in: content,
                labels: [
                    "Ravintoarvot (100 g)",
                    "Nutritional values (100 g)"
                ]
            )
            let co2Line = labeledParagraph(
                in: content,
                labels: ["Hiilijalanjälki", "Carbon footprint"]
            )
            let diets = ProviderParsing.firstCapture(
                #"<div\b[^>]*class=["'][^"']*\baccordion__footer__special-diets\b[^"']*["'][^>]*>(.*?)</div>"#,
                in: item
            ).map(ProviderParsing.htmlText) ?? ""
            let key = RecipeDetailEnrichment.mealKey(name)
            details[key] = RecipeDetail(
                id: "antell-\(key)",
                name: name,
                ingredients: ingredients,
                nutrition: nutritionValues(from: nutritionLine),
                co2KilogramsPer100Grams: firstNumber(in: co2Line),
                diets: diets
            )
        }
        return details
    }

    private static func labeledParagraph(
        in html: String,
        labels: [String]
    ) -> String {
        for rawParagraph in ProviderParsing.captures(
            #"<p\b[^>]*>(.*?)</p>"#,
            in: html
        ) {
            let text = ProviderParsing.htmlText(rawParagraph)
            for label in labels {
                guard let range = text.range(
                    of: label,
                    options: [.caseInsensitive, .diacriticInsensitive]
                ) else {
                    continue
                }
                return String(text[range.upperBound...])
                    .trimmingCharacters(in: CharacterSet(charactersIn: " :"))
                    .normalizedWhitespace
            }
        }
        return ""
    }

    private static func nutritionValues(from line: String) -> [NutritionValue] {
        var values: [NutritionValue] = []
        for part in line.split(separator: ",").map(String.init) {
            let lower = part.lowercased()
            guard let amount = firstNumber(in: part) else { continue }
            let name: String
            if lower.contains("kcal")
                || lower.contains("energia")
                || lower.contains("energy") {
                name = "EnergyKcal"
            } else if lower.contains("hiilihydra")
                        || lower.contains("carbohydrate")
                        || lower.contains("carbs") {
                name = "Carbohydrates"
            } else if lower.contains("proteiin") || lower.contains("protein") {
                name = "Protein"
            } else if (lower.contains("rasva") || lower.contains("fat"))
                        && !lower.contains("tyydytt")
                        && !lower.contains("saturated") {
                name = "Fat"
            } else {
                continue
            }
            guard !values.contains(where: { $0.name == name }) else { continue }
            values.append(
                NutritionValue(
                    name: name,
                    amount: amount,
                    unit: lower.contains("kcal") ? "kcal" : "g"
                )
            )
        }
        return values
    }

    private static func firstNumber(in value: String) -> Double? {
        guard let expression = try? NSRegularExpression(
            pattern: #"\d+(?:[.,]\d+)?"#
        ),
        let match = expression.firstMatch(
            in: value,
            range: NSRange(value.startIndex..<value.endIndex, in: value)
        ),
        let range = Range(match.range, in: value)
        else {
            return nil
        }
        return Double(value[range].replacingOccurrences(of: ",", with: "."))
    }
}

private struct CompassRecipePayload: Decodable {
    let recipeID: Int?
    let name: String?
    let ingredientsCleaned: String?
    let nutritionalValues: [CompassNutritionPayload]?
    let co2KilogramsPer100Grams: Double?
    let diets: String?

    enum CodingKeys: String, CodingKey {
        case recipeID = "recipeId"
        case name
        case ingredientsCleaned
        case nutritionalValues
        case co2KilogramsPer100Grams = "kgCO2ePer100g"
        case diets
    }
}

private struct CompassNutritionPayload: Decodable {
    let name: String?
    let amount: Double?
    let unit: String?
}
