import QtQuick 2.15
import QtQuick.Controls 2.15 as QQC2
import QtCore
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid 2.0
import org.kde.kirigami 2.20 as Kirigami

import "MenuFormatter.js" as MenuFormatter

PlasmoidItem {
    id: root

    property string apiBaseUrl: "https://www.compass-group.fi/menuapi/feed/json"
    property string apiRssBaseUrl: "https://www.compass-group.fi/menuapi/feed/rss/current-day"
    property string meditekniaRestaurantCode: "043601"
    property var allRestaurantCatalog: [
        { code: "0437", fallbackName: "Snellmania", provider: "compass" },
        { code: "snellari-rss", fallbackName: "Cafe Snellari", provider: "compass-rss", rssCostNumber: "4370", rssUrlBase: "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/cafe-snellari/" },
        { code: "0436", fallbackName: "Canthia", provider: "compass" },
        { code: "043601", fallbackName: "Mediteknia", provider: "compass" },
        { code: "0439", fallbackName: "Tietoteknia", provider: "compass" },
        { code: "antell-round", fallbackName: "Antell Round", provider: "antell", antellSlug: "round", antellUrlBase: "https://antell.fi/lounas/kuopio/round/" },
        { code: "antell-highway", fallbackName: "Antell Highway", provider: "antell", antellSlug: "highway", antellUrlBase: "https://antell.fi/lounas/kuopio/highway/" },
        { code: "huomen-bioteknia", fallbackName: "Hyvä Huomen Bioteknia", provider: "huomen-json", huomenApiBase: "https://europe-west1-luncher-7cf76.cloudfunctions.net/api/v1/week/a96b7ccf-2c3d-432a-8504-971dbb6d55d3/active", huomenUrlBase: "https://hyvahuomen.fi/bioteknia/" }
    ]
    property var restaurantCatalog: filteredRestaurantCatalog(configEnabledRestaurantCodes)

    property var restaurantStates: ({})
    property var requestSerialByCode: ({})
    property var cacheStore: ({})
    property int modelVersion: 0
    property bool initialized: false
    property var supportedIconNames: ["food", "compass", "map-globe", "map-flat"]

    property string activeRestaurantCode: "0437"

    property string configEnabledRestaurantCodes: {
        var raw = String(Plasmoid.configuration.enabledRestaurantCodes || "").trim()
        if (raw.length > 0) {
            return raw
        }

        var defaults = []
        for (var i = 0; i < allRestaurantCatalog.length; i++) {
            defaults.push(String(allRestaurantCatalog[i].code))
        }
        return defaults.join(",")
    }
    property string configRestaurantCode: {
        var fallback = defaultRestaurantCode()
        var raw = String(Plasmoid.configuration.restaurantCode || Plasmoid.configuration.costNumber || fallback).trim()
        return isKnownRestaurant(raw) ? raw : fallback
    }
    property string configLanguage: {
        var raw = String(Plasmoid.configuration.language || "fi").toLowerCase()
        return raw === "en" ? "en" : "fi"
    }
    property bool configEnableWheelCycle: Plasmoid.configuration.enableWheelCycle !== false
    property int configRefreshMinutes: {
        var raw = Number(Plasmoid.configuration.refreshMinutes)
        if (!isFinite(raw)) {
            return 1440
        }
        raw = Math.floor(raw)
        if (raw < 0) {
            return 1440
        }
        return raw
    }
    property int configManualRefreshToken: Number(Plasmoid.configuration.manualRefreshToken || 0)
    property bool configShowPrices: !!Plasmoid.configuration.showPrices
    property bool configHideExpensiveStudentMeals: !!Plasmoid.configuration.hideExpensiveStudentMeals
    property bool configShowStudentPrice: Plasmoid.configuration.showStudentPrice !== false
    property bool configShowStaffPrice: Plasmoid.configuration.showStaffPrice !== false
    property bool configShowGuestPrice: Plasmoid.configuration.showGuestPrice !== false
    property bool configShowAllergens: Plasmoid.configuration.showAllergens !== false
    property bool configHighlightGlutenFree: !!Plasmoid.configuration.highlightGlutenFree
    property bool configHighlightVeg: !!Plasmoid.configuration.highlightVeg
    property bool configHighlightLactoseFree: !!Plasmoid.configuration.highlightLactoseFree
    property string configIconName: {
        var raw = String(Plasmoid.configuration.iconName || "food").trim()
        return supportedIconNames.indexOf(raw) >= 0 ? raw : "food"
    }

    Settings {
        id: cache
        property string cacheBlob: "{}"
    }

    function touchModel() {
        modelVersion += 1
    }

    function parseConfiguredRestaurantCodes(rawValue) {
        var selectedMap = {}
        var raw = String(rawValue || "")
        if (raw.length > 0) {
            var tokens = raw.split(",")
            for (var i = 0; i < tokens.length; i++) {
                var token = String(tokens[i] || "").trim()
                if (token) {
                    selectedMap[token] = true
                }
            }
        } else {
            for (var j = 0; j < allRestaurantCatalog.length; j++) {
                selectedMap[String(allRestaurantCatalog[j].code)] = true
            }
        }

        var selectedCodes = []
        for (var k = 0; k < allRestaurantCatalog.length; k++) {
            var code = String(allRestaurantCatalog[k].code)
            if (selectedMap[code]) {
                selectedCodes.push(code)
            }
        }

        if (selectedCodes.length === 0 && allRestaurantCatalog.length > 0) {
            selectedCodes.push(String(allRestaurantCatalog[0].code))
        }

        return selectedCodes
    }

    function filteredRestaurantCatalog(rawValue) {
        var selectedCodes = parseConfiguredRestaurantCodes(rawValue)
        var selectedMap = {}
        for (var i = 0; i < selectedCodes.length; i++) {
            selectedMap[selectedCodes[i]] = true
        }

        var filtered = []
        for (var j = 0; j < allRestaurantCatalog.length; j++) {
            var entry = allRestaurantCatalog[j]
            if (selectedMap[String(entry.code)]) {
                filtered.push(entry)
            }
        }
        return filtered
    }

    function writeConfiguredRestaurantCodes(codes) {
        var selectedMap = {}
        var rawCodes = Array.isArray(codes) ? codes : []
        for (var i = 0; i < rawCodes.length; i++) {
            var code = String(rawCodes[i] || "").trim()
            if (code) {
                selectedMap[code] = true
            }
        }

        var ordered = []
        for (var j = 0; j < allRestaurantCatalog.length; j++) {
            var catalogCode = String(allRestaurantCatalog[j].code)
            if (selectedMap[catalogCode]) {
                ordered.push(catalogCode)
            }
        }

        if (ordered.length === 0 && allRestaurantCatalog.length > 0) {
            ordered.push(String(allRestaurantCatalog[0].code))
        }

        Plasmoid.configuration.enabledRestaurantCodes = ordered.join(",")
    }

    function migrateEnabledRestaurantCodes() {
        var migrationLevel = Number(Plasmoid.configuration.enabledRestaurantCodesMigrationLevel || 0)
        if (migrationLevel >= 2) {
            return
        }

        var raw = String(Plasmoid.configuration.enabledRestaurantCodes || "").trim()
        var selectedCodes = parseConfiguredRestaurantCodes(raw)
        if (selectedCodes.indexOf(meditekniaRestaurantCode) < 0) {
            selectedCodes.push(meditekniaRestaurantCode)
        }
        writeConfiguredRestaurantCodes(selectedCodes)

        Plasmoid.configuration.enabledRestaurantCodesMigrationLevel = 2
    }

    function defaultRestaurantCode() {
        var codes = restaurantCodes()
        if (codes.length > 0) {
            return String(codes[0])
        }
        return allRestaurantCatalog.length > 0 ? String(allRestaurantCatalog[0].code) : "0437"
    }

    function restaurantCodes() {
        var list = []
        for (var i = 0; i < restaurantCatalog.length; i++) {
            list.push(String(restaurantCatalog[i].code))
        }
        return list
    }

    function isKnownRestaurant(code) {
        var normalized = String(code || "")
        var codes = restaurantCodes()
        return codes.indexOf(normalized) >= 0
    }

    function restaurantEntryForCode(code) {
        var normalized = String(code || "")
        for (var i = 0; i < restaurantCatalog.length; i++) {
            if (String(restaurantCatalog[i].code) === normalized) {
                return restaurantCatalog[i]
            }
        }
        return null
    }

    function restaurantLabelForCode(code) {
        var normalized = String(code || "")
        for (var i = 0; i < restaurantCatalog.length; i++) {
            if (restaurantCatalog[i].code === normalized) {
                return restaurantCatalog[i].fallbackName
            }
        }
        return "Restaurant " + normalized
    }

    function stateTemplate(code) {
        return {
            restaurantCode: code,
            status: "idle",
            errorMessage: "",
            lastUpdatedEpochMs: 0,
            payloadText: "",
            rawPayload: null,
            todayMenu: null,
            menuDateIso: "",
            providerDateValid: false,
            isTodayFresh: false,
            consecutiveFailures: 0,
            nextRetryEpochMs: 0,
            assumedNoMenuWeekend: false,
            assumedNoMenuRetryEpochMs: 0,
            restaurantName: "",
            restaurantUrl: ""
        }
    }

    function ensureStateMaps() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            if (!restaurantStates[code]) {
                restaurantStates[code] = stateTemplate(code)
            }
            if (!requestSerialByCode[code]) {
                requestSerialByCode[code] = 0
            }
        }
    }

    function resetAllStates() {
        var codes = restaurantCodes()
        var next = {}
        for (var i = 0; i < codes.length; i++) {
            next[codes[i]] = stateTemplate(codes[i])
        }
        restaurantStates = next
        touchModel()
    }

    function stateFor(code) {
        ensureStateMaps()
        var normalized = String(code || "")
        if (!restaurantStates[normalized]) {
            restaurantStates[normalized] = stateTemplate(normalized)
            touchModel()
        }
        return restaurantStates[normalized]
    }

    function formatLastUpdated(epochMs) {
        var value = Number(epochMs) || 0
        if (value <= 0) {
            return ""
        }
        return Qt.formatDateTime(new Date(value), Qt.DefaultLocaleShortDate)
    }

    function syncSettingsLastUpdatedDisplay() {
        var state = stateFor(activeRestaurantCode)
        Plasmoid.configuration.lastUpdatedDisplay = formatLastUpdated(state.lastUpdatedEpochMs)
    }

    function updateState(code, patch) {
        var current = stateFor(code)
        var next = {}
        for (var key in current) {
            next[key] = current[key]
        }
        for (var patchKey in patch) {
            next[patchKey] = patch[patchKey]
        }
        restaurantStates[String(code)] = next
        touchModel()
    }

    function localDateIso(dateObj) {
        var year = dateObj.getFullYear()
        var month = (dateObj.getMonth() + 1).toString()
        var day = dateObj.getDate().toString()

        if (month.length < 2) {
            month = "0" + month
        }
        if (day.length < 2) {
            day = "0" + day
        }

        return year + "-" + month + "-" + day
    }

    function todayIso() {
        return localDateIso(new Date())
    }

    function isStateFreshForToday(state) {
        if (!state) {
            return false
        }
        return !!state.providerDateValid && MenuFormatter.normalizeText(state.menuDateIso) === todayIso()
    }

    function retryDelayMinutes(failureCount) {
        var count = Math.max(1, Number(failureCount) || 1)
        if (count <= 1) {
            return 5
        }
        if (count === 2) {
            return 10
        }
        return 15
    }

    function isWeekendDate(dateObj) {
        var day = (dateObj || new Date()).getDay()
        return day === 0 || day === 6
    }

    function isWeekendNoMenuProvider(provider) {
        return provider === "antell" || provider === "huomen-json"
    }

    function weekendNoMenuRetryDelayMs() {
        return 6 * 60 * 60 * 1000
    }

    function weekdayToken(dateObj) {
        var names = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"]
        return names[dateObj.getDay()] || "monday"
    }

    function decodeHtmlEntities(text) {
        return String(text || "")
            .replace(/&#x([0-9a-fA-F]+);/g, function(_, hex) {
                return String.fromCharCode(parseInt(hex, 16))
            })
            .replace(/&#([0-9]+);/g, function(_, dec) {
                return String.fromCharCode(parseInt(dec, 10))
            })
            .replace(/&amp;/g, "&")
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">")
            .replace(/&quot;/g, "\"")
            .replace(/&#39;/g, "'")
            .replace(/&nbsp;/g, " ")
    }

    function stripHtmlText(rawHtml) {
        var withoutTags = String(rawHtml || "").replace(/<[^>]*>/g, " ")
        return MenuFormatter.normalizeText(decodeHtmlEntities(withoutTags))
    }

    function parseAntellSections(htmlText) {
        var sections = []
        var sectionRegex = /<section class="menu-section">([\s\S]*?)<\/section>/gi
        var sectionMatch

        while ((sectionMatch = sectionRegex.exec(String(htmlText || ""))) !== null) {
            var sectionHtml = sectionMatch[1]
            var titleMatch = sectionHtml.match(/<h2 class="menu-title">([\s\S]*?)<\/h2>/i)
            var priceMatch = sectionHtml.match(/<h2 class="menu-price">([\s\S]*?)<\/h2>/i)
            var listMatch = sectionHtml.match(/<ul class="menu-list">([\s\S]*?)<\/ul>/i)

            var title = stripHtmlText(titleMatch ? titleMatch[1] : "")
            var price = stripHtmlText(priceMatch ? priceMatch[1] : "")
            var listHtml = listMatch ? listMatch[1] : ""

            var items = []
            var liRegex = /<li[^>]*>([\s\S]*?)<\/li>/gi
            var liMatch
            while ((liMatch = liRegex.exec(listHtml)) !== null) {
                var itemText = stripHtmlText(liMatch[1])
                if (itemText) {
                    items.push(itemText)
                }
            }

            if (items.length === 0) {
                continue
            }

            sections.push({
                sortOrder: sections.length + 1,
                name: title || "Menu",
                price: price,
                components: items
            })
        }

        return sections
    }

    function parseRssTagRaw(xmlText, tagName) {
        var regex = new RegExp("<" + tagName + "(?:\\s+[^>]*)?>([\\s\\S]*?)<\\/" + tagName + ">", "i")
        var match = String(xmlText || "").match(regex)
        return match ? String(match[1] || "") : ""
    }

    function parseRssMenuDateIso(dateText) {
        var clean = MenuFormatter.normalizeText(dateText)
        if (!clean) {
            return ""
        }

        var parts = clean.match(/(\d{1,2})[-.\/](\d{1,2})[-.\/](\d{2,4})/)
        if (!parts) {
            return ""
        }

        var day = Number(parts[1])
        var month = Number(parts[2])
        var year = Number(parts[3])
        if (!isFinite(day) || !isFinite(month) || !isFinite(year)) {
            return ""
        }
        if (year < 100) {
            year += 2000
        }
        if (day < 1 || day > 31 || month < 1 || month > 12) {
            return ""
        }

        var candidate = new Date(year, month - 1, day)
        if (candidate.getFullYear() !== year || candidate.getMonth() !== month - 1 || candidate.getDate() !== day) {
            return ""
        }
        return localDateIso(candidate)
    }

    function isRssAllergenToken(token) {
        var clean = MenuFormatter.normalizeText(token).replace(/[.;:]+$/, "")
        if (!clean) {
            return false
        }
        if (clean === "*") {
            return true
        }

        if (/^[A-Z]$/.test(clean)) {
            return true
        }

        var upper = clean.toUpperCase()
        if (upper === "VEG" || upper === "VS" || upper === "ILM") {
            return true
        }

        return false
    }

    function normalizeRssAllergenToken(token) {
        var clean = MenuFormatter.normalizeText(token).replace(/[.;:]+$/, "")
        if (!clean) {
            return ""
        }
        if (clean === "*") {
            return "*"
        }

        var upper = clean.toUpperCase()
        if (upper === "VEG") {
            return "Veg"
        }
        return upper
    }

    function normalizeRssComponentLine(rawLine) {
        var line = MenuFormatter.normalizeText(rawLine)
        if (!line) {
            return ""
        }

        if (/\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\)\s*$/.test(line)) {
            return line
        }

        var compact = line.replace(/\s*[;,]\s*$/, "")
        var parts = compact.split(/\s*,\s*/)
        if (parts.length < 2) {
            return compact
        }

        var suffixTokens = []
        for (var i = parts.length - 1; i >= 0; i--) {
            var candidate = MenuFormatter.normalizeText(parts[i])
            if (!isRssAllergenToken(candidate)) {
                break
            }
            var normalizedToken = normalizeRssAllergenToken(candidate)
            if (!normalizedToken) {
                break
            }
            suffixTokens.unshift(normalizedToken)
        }

        if (suffixTokens.length === 0) {
            return compact
        }

        var mainParts = parts.slice(0, parts.length - suffixTokens.length)
        var mainText = MenuFormatter.normalizeText(mainParts.join(", "))
        if (!mainText) {
            return compact
        }

        var starMatch = mainText.match(/^(.*\S)\s*\*$/)
        if (starMatch) {
            mainText = MenuFormatter.normalizeText(starMatch[1])
            suffixTokens.unshift("*")
        }

        while (true) {
            var trailingMatch = mainText.match(/^(.*\S)\s+([A-Za-z*]{1,4})$/)
            if (!trailingMatch) {
                break
            }
            var trailingToken = normalizeRssAllergenToken(trailingMatch[2])
            if (!isRssAllergenToken(trailingMatch[2]) || !trailingToken) {
                break
            }
            mainText = MenuFormatter.normalizeText(trailingMatch[1])
            suffixTokens.unshift(trailingToken)
        }

        return mainText + " (" + suffixTokens.join(", ") + ")"
    }

    function parseRssComponents(descriptionRaw) {
        var decoded = decodeHtmlEntities(descriptionRaw)
        var components = []
        var paragraphRegex = /<p[^>]*>([\s\S]*?)<\/p>/gi
        var paragraphMatch

        while ((paragraphMatch = paragraphRegex.exec(decoded)) !== null) {
            var line = normalizeRssComponentLine(stripHtmlText(paragraphMatch[1]))
            if (line) {
                components.push(line)
            }
        }

        if (components.length === 0) {
            var fallback = normalizeRssComponentLine(stripHtmlText(decoded))
            if (fallback) {
                components.push(fallback)
            }
        }

        return components
    }

    function localizedField(value) {
        if (value === null || value === undefined) {
            return ""
        }

        var primitiveType = typeof value
        if (primitiveType === "string" || primitiveType === "number" || primitiveType === "boolean") {
            return MenuFormatter.normalizeText(value)
        }

        if (primitiveType !== "object") {
            return ""
        }

        var preferredKeys = [configLanguage, "fi", "en"]
        for (var i = 0; i < preferredKeys.length; i++) {
            var key = preferredKeys[i]
            if (!Object.prototype.hasOwnProperty.call(value, key)) {
                continue
            }
            var candidate = MenuFormatter.normalizeText(value[key])
            if (candidate) {
                return candidate
            }
        }

        for (var dynamicKey in value) {
            if (!Object.prototype.hasOwnProperty.call(value, dynamicKey)) {
                continue
            }
            var fallback = MenuFormatter.normalizeText(value[dynamicKey])
            if (fallback) {
                return fallback
            }
        }

        return ""
    }

    function normalizeHuomenAllergenToken(token) {
        var clean = MenuFormatter.normalizeText(token)
        if (!clean) {
            return ""
        }
        if (clean === "*") {
            return "*"
        }

        var upper = clean.toUpperCase()
        if (upper === "VEG") {
            return "Veg"
        }
        if (/^[A-Z]{1,8}$/.test(upper)) {
            return upper
        }

        return clean
    }

    function huomenLunchLine(lunch) {
        var title = localizedField(lunch && lunch.title)
        if (!title) {
            return ""
        }

        var description = localizedField(lunch && lunch.description)
        var line = title
        if (description && description !== title) {
            line += " - " + description
        }

        var allergens = []
        var seenAllergens = {}
        var rawAllergens = Array.isArray(lunch && lunch.allergens) ? lunch.allergens : []
        for (var i = 0; i < rawAllergens.length; i++) {
            var rawToken = localizedField(rawAllergens[i] && rawAllergens[i].abbreviation)
            var token = normalizeHuomenAllergenToken(rawToken)
            if (!token) {
                continue
            }
            var key = token.toUpperCase()
            if (seenAllergens[key]) {
                continue
            }
            seenAllergens[key] = true
            allergens.push(token)
        }

        if (allergens.length > 0) {
            line += " (" + allergens.join(", ") + ")"
        }

        return MenuFormatter.normalizeText(line)
    }

    function parseAntellMenuDateIso(menuDateText) {
        var clean = MenuFormatter.normalizeText(menuDateText)
        if (!clean) {
            return ""
        }

        var parts = clean.match(/(\d{1,2})\.(\d{1,2})(?:\.(\d{2,4}))?/)
        if (!parts) {
            return ""
        }

        var day = Number(parts[1])
        var month = Number(parts[2])
        if (!isFinite(day) || !isFinite(month) || day < 1 || day > 31 || month < 1 || month > 12) {
            return ""
        }

        function buildCandidate(yearNumber) {
            var candidate = new Date(yearNumber, month - 1, day)
            if (candidate.getFullYear() !== yearNumber || candidate.getMonth() !== month - 1 || candidate.getDate() !== day) {
                return null
            }
            return candidate
        }

        if (parts[3]) {
            var explicitYear = Number(parts[3])
            if (!isFinite(explicitYear)) {
                return ""
            }
            if (explicitYear < 100) {
                explicitYear += 2000
            }
            var datedCandidate = buildCandidate(explicitYear)
            return datedCandidate ? localDateIso(datedCandidate) : ""
        }

        var now = new Date()
        var nowMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate())
        var years = [now.getFullYear() - 1, now.getFullYear(), now.getFullYear() + 1]
        var best = null
        var bestDistance = Number.MAX_VALUE

        for (var i = 0; i < years.length; i++) {
            var candidate = buildCandidate(years[i])
            if (!candidate) {
                continue
            }
            var distance = Math.abs(candidate.getTime() - nowMidnight.getTime())
            if (distance < bestDistance) {
                bestDistance = distance
                best = candidate
            }
        }

        return best ? localDateIso(best) : ""
    }

    function normalizeCompassRssTodayMenu(rawPayload) {
        if (!rawPayload || rawPayload.provider !== "compass-rss" || !rawPayload.providerDateValid) {
            return null
        }

        var menuDate = MenuFormatter.normalizeText(rawPayload.menuDateIso)
        if (!menuDate) {
            return null
        }

        var components = Array.isArray(rawPayload.components) ? rawPayload.components.slice(0) : []
        return {
            dateIso: menuDate,
            lunchTime: "",
            menus: components.length > 0
                ? [{
                    sortOrder: 1,
                    name: configLanguage === "en" ? "Lunch" : "Lounas",
                    price: "",
                    components: components
                }]
                : []
        }
    }

    function normalizeHuomenTodayMenu(rawPayload) {
        if (!rawPayload || rawPayload.provider !== "huomen-json" || !rawPayload.providerDateValid) {
            return null
        }

        var menuDate = MenuFormatter.normalizeText(rawPayload.menuDateIso)
        if (!menuDate) {
            return null
        }

        var components = Array.isArray(rawPayload.lunchLines) ? rawPayload.lunchLines.slice(0) : []
        return {
            dateIso: menuDate,
            lunchTime: "",
            menus: components.length > 0
                ? [{
                    sortOrder: 1,
                    name: configLanguage === "en" ? "Lunch" : "Lounas",
                    price: "",
                    components: components
                }]
                : []
        }
    }

    function normalizeAntellTodayMenu(rawPayload) {
        if (!rawPayload || rawPayload.provider !== "antell" || !rawPayload.providerDateValid) {
            return null
        }

        var menuDate = MenuFormatter.normalizeText(rawPayload.menuDateIso)
        if (!menuDate) {
            return null
        }

        return {
            dateIso: menuDate,
            lunchTime: "",
            menus: parseAntellSections(rawPayload.htmlText)
        }
    }

    function parseAntellPayload(code, htmlText) {
        var entry = restaurantEntryForCode(code)
        var payloadText = String(htmlText || "")
        var locationMatch = payloadText.match(/<div class="menu-location">([\s\S]*?)<\/div>/i)
        var menuDateMatch = payloadText.match(/<div class="menu-date">([\s\S]*?)<\/div>/i)
        var location = stripHtmlText(locationMatch ? locationMatch[1] : "")
        var menuDateText = stripHtmlText(menuDateMatch ? menuDateMatch[1] : "")
        var menuDateIso = parseAntellMenuDateIso(menuDateText)
        var isDateToday = menuDateIso && menuDateIso === todayIso()
        var fallbackName = entry ? String(entry.fallbackName || "Antell") : "Antell"
        var name = location
            ? (location.toLowerCase().indexOf("antell") === 0 ? location : ("Antell " + location))
            : fallbackName
        var url = entry && entry.antellUrlBase ? String(entry.antellUrlBase) : ""
        var rawPayload = {
            provider: "antell",
            htmlText: payloadText,
            menuDateText: menuDateText,
            menuDateIso: menuDateIso,
            providerDateValid: !!isDateToday,
            restaurantName: name,
            restaurantUrl: url
        }

        return {
            rawPayload: rawPayload,
            todayMenu: normalizeAntellTodayMenu(rawPayload),
            menuDateIso: menuDateIso,
            providerDateValid: !!isDateToday,
            restaurantName: name,
            restaurantUrl: url
        }
    }

    function parseCompassRssPayload(code, xmlText) {
        var entry = restaurantEntryForCode(code)
        var payloadText = String(xmlText || "")
        var channelRaw = parseRssTagRaw(payloadText, "channel")
        var itemMatch = String(channelRaw || payloadText).match(/<item\b[^>]*>([\s\S]*?)<\/item>/i)
        var itemRaw = itemMatch ? String(itemMatch[1] || "") : ""

        var channelTitle = stripHtmlText(parseRssTagRaw(channelRaw || payloadText, "title"))
        var itemTitle = stripHtmlText(parseRssTagRaw(itemRaw, "title"))
        var itemGuid = stripHtmlText(parseRssTagRaw(itemRaw, "guid"))
        var itemLink = stripHtmlText(parseRssTagRaw(itemRaw, "link"))
        var descriptionRaw = parseRssTagRaw(itemRaw, "description")

        var menuDateIso = parseRssMenuDateIso(itemTitle) || parseRssMenuDateIso(itemGuid)
        var isDateToday = menuDateIso && menuDateIso === todayIso()
        var components = parseRssComponents(descriptionRaw)
        var fallbackName = entry ? String(entry.fallbackName || "Compass Lunch") : "Compass Lunch"
        var name = channelTitle || fallbackName
        var url = itemLink || (entry && entry.rssUrlBase ? String(entry.rssUrlBase) : "")

        var rawPayload = {
            provider: "compass-rss",
            xmlText: payloadText,
            menuDateIso: menuDateIso,
            providerDateValid: !!isDateToday,
            components: components,
            restaurantName: name,
            restaurantUrl: url
        }

        return {
            rawPayload: rawPayload,
            todayMenu: normalizeCompassRssTodayMenu(rawPayload),
            menuDateIso: menuDateIso,
            providerDateValid: !!isDateToday,
            restaurantName: name,
            restaurantUrl: url
        }
    }

    function parseHuomenPayload(code, jsonText) {
        var parsed = null
        try {
            parsed = JSON.parse(jsonText)
        } catch (e) {
            return { error: "Invalid JSON payload" }
        }

        if (!parsed || parsed.success === false || !parsed.data || !parsed.data.week || !Array.isArray(parsed.data.week.days)) {
            return { error: MenuFormatter.normalizeText(parsed && parsed.message) || "Missing week.days in Huomen payload" }
        }

        var entry = restaurantEntryForCode(code)
        var data = parsed.data
        var expectedIso = todayIso()
        var dayMatch = null
        var days = data.week.days

        for (var i = 0; i < days.length; i++) {
            var day = days[i]
            if (MenuFormatter.normalizeText(day && day.dateString) === expectedIso) {
                dayMatch = day
                break
            }
        }

        var lunchLines = []
        if (dayMatch && !dayMatch.isClosed) {
            var lunches = Array.isArray(dayMatch.lunches) ? dayMatch.lunches : []
            for (var j = 0; j < lunches.length; j++) {
                var line = huomenLunchLine(lunches[j])
                if (line) {
                    lunchLines.push(line)
                }
            }
        }

        var providerDateValid = !!dayMatch
        var menuDateIso = providerDateValid ? expectedIso : ""
        var restaurantName = localizedField(data.location && data.location.name)
            || (entry ? String(entry.fallbackName || "Huomen Lunch") : "Huomen Lunch")
        var restaurantUrl = entry && entry.huomenUrlBase ? String(entry.huomenUrlBase) : ""
        var rawPayload = {
            provider: "huomen-json",
            menuDateIso: menuDateIso,
            providerDateValid: providerDateValid,
            lunchLines: lunchLines,
            restaurantName: restaurantName,
            restaurantUrl: restaurantUrl
        }

        return {
            rawPayload: rawPayload,
            todayMenu: normalizeHuomenTodayMenu(rawPayload),
            menuDateIso: menuDateIso,
            providerDateValid: providerDateValid,
            restaurantName: restaurantName,
            restaurantUrl: restaurantUrl
        }
    }

    function normalizeMenuEntry(menuEntry) {
        var name = MenuFormatter.normalizeText(menuEntry && menuEntry.Name)
        var price = MenuFormatter.normalizeText(menuEntry && menuEntry.Price)
        var components = []

        var rawComponents = menuEntry && menuEntry.Components
        if (Array.isArray(rawComponents)) {
            for (var i = 0; i < rawComponents.length; i++) {
                var clean = MenuFormatter.normalizeText(rawComponents[i])
                if (clean) {
                    components.push(clean)
                }
            }
        }

        if (!name && components.length === 0) {
            return null
        }

        return {
            sortOrder: Number(menuEntry.SortOrder) || 0,
            name: name || "Menu",
            price: price,
            components: components
        }
    }

    function normalizeTodayMenu(payload) {
        if (!payload || !Array.isArray(payload.MenusForDays)) {
            return null
        }

        var currentDateIso = todayIso()

        for (var i = 0; i < payload.MenusForDays.length; i++) {
            var day = payload.MenusForDays[i]
            if (MenuFormatter.dayKey(day && day.Date) !== currentDateIso) {
                continue
            }

            var rawSetMenus = Array.isArray(day.SetMenus) ? day.SetMenus.slice(0) : []
            rawSetMenus.sort(function(a, b) {
                return (Number(a.SortOrder) || 0) - (Number(b.SortOrder) || 0)
            })

            var menus = []
            for (var j = 0; j < rawSetMenus.length; j++) {
                var normalized = normalizeMenuEntry(rawSetMenus[j])
                if (normalized) {
                    menus.push(normalized)
                }
            }

            return {
                todayMenu: {
                    dateIso: currentDateIso,
                    lunchTime: MenuFormatter.normalizeText(day.LunchTime),
                    menus: menus
                },
                menuDateIso: currentDateIso,
                providerDateValid: true
            }
        }

        return {
            todayMenu: null,
            menuDateIso: "",
            providerDateValid: false
        }
    }

    function cacheKey(code) {
        var entry = restaurantEntryForCode(code)
        if (entry && entry.provider === "antell") {
            return String(code) + "|antell"
        }
        return String(code) + "|" + configLanguage
    }

    function loadCacheStore() {
        try {
            var parsed = JSON.parse(cache.cacheBlob || "{}")
            if (parsed && typeof parsed === "object") {
                cacheStore = parsed
            } else {
                cacheStore = {}
            }
        } catch (e) {
            cacheStore = {}
        }
    }

    function saveCacheEntry(code, payloadText, updatedEpochMs) {
        cacheStore[cacheKey(code)] = {
            payload: payloadText,
            lastUpdatedEpochMs: Number(updatedEpochMs) || 0
        }

        try {
            cache.cacheBlob = JSON.stringify(cacheStore)
        } catch (e) {
        }
    }

    function dateMismatchMessage() {
        return "Date mismatch: expected " + todayIso()
    }

    function setErrorStateForCode(code, message) {
        var current = stateFor(code)
        if (isStateFreshForToday(current)) {
            var keepWeekendAssumed = !!current.assumedNoMenuWeekend
            updateState(code, {
                status: "ok",
                errorMessage: "",
                consecutiveFailures: 0,
                nextRetryEpochMs: 0,
                assumedNoMenuWeekend: keepWeekendAssumed,
                assumedNoMenuRetryEpochMs: keepWeekendAssumed ? (Date.now() + weekendNoMenuRetryDelayMs()) : 0
            })
            return
        }

        var failureCount = (Number(current.consecutiveFailures) || 0) + 1
        updateState(code, {
            status: current.payloadText ? "stale" : "error",
            errorMessage: message,
            isTodayFresh: false,
            consecutiveFailures: failureCount,
            nextRetryEpochMs: Date.now() + retryDelayMinutes(failureCount) * 60 * 1000,
            assumedNoMenuWeekend: false,
            assumedNoMenuRetryEpochMs: 0
        })
        retryTimer.start()
    }

    function applyPayloadForCode(code, payloadText, fromCache, cachedTimestamp) {
        var entry = restaurantEntryForCode(code)
        var provider = entry && entry.provider ? String(entry.provider) : "compass"
        var parsed = null
        var todayMenu = null
        var menuDateIso = ""
        var providerDateValid = false
        var restaurantName = ""
        var restaurantUrl = ""

        if (provider === "antell") {
            var antell = parseAntellPayload(code, payloadText)
            parsed = antell.rawPayload
            todayMenu = antell.todayMenu
            menuDateIso = antell.menuDateIso
            providerDateValid = antell.providerDateValid
            restaurantName = antell.restaurantName
            restaurantUrl = antell.restaurantUrl
        } else if (provider === "compass-rss") {
            var compassRss = parseCompassRssPayload(code, payloadText)
            parsed = compassRss.rawPayload
            todayMenu = compassRss.todayMenu
            menuDateIso = compassRss.menuDateIso
            providerDateValid = compassRss.providerDateValid
            restaurantName = compassRss.restaurantName
            restaurantUrl = compassRss.restaurantUrl
        } else if (provider === "huomen-json") {
            var huomen = parseHuomenPayload(code, payloadText)
            if (!huomen || huomen.error) {
                setErrorStateForCode(code, huomen && huomen.error ? huomen.error : "Invalid Huomen payload")
                return false
            }
            parsed = huomen.rawPayload
            todayMenu = huomen.todayMenu
            menuDateIso = huomen.menuDateIso
            providerDateValid = huomen.providerDateValid
            restaurantName = huomen.restaurantName
            restaurantUrl = huomen.restaurantUrl
        } else {
            try {
                parsed = JSON.parse(payloadText)
            } catch (e) {
                setErrorStateForCode(code, "Invalid JSON payload")
                return false
            }

            if (!parsed || !Array.isArray(parsed.MenusForDays)) {
                setErrorStateForCode(code, "Missing MenusForDays in payload")
                return false
            }

            if (parsed.ErrorText) {
                setErrorStateForCode(code, MenuFormatter.normalizeText(parsed.ErrorText))
                return false
            }

            var normalizedCompass = normalizeTodayMenu(parsed)
            if (!normalizedCompass) {
                setErrorStateForCode(code, "Invalid menu payload")
                return false
            }

            todayMenu = normalizedCompass.todayMenu
            menuDateIso = normalizedCompass.menuDateIso
            providerDateValid = normalizedCompass.providerDateValid
            restaurantName = MenuFormatter.normalizeText(parsed.RestaurantName) || "Compass Lunch"
            restaurantUrl = MenuFormatter.normalizeText(parsed.RestaurantUrl)
        }

        var updatedMs = fromCache ? (Number(cachedTimestamp) || 0) : Date.now()
        var today = todayIso()
        var freshToday = !!providerDateValid && menuDateIso === today
        var assumeWeekendNoMenu = !freshToday && isWeekendNoMenuProvider(provider) && isWeekendDate(new Date())
        if (assumeWeekendNoMenu) {
            freshToday = true
            providerDateValid = true
            menuDateIso = today
            todayMenu = {
                dateIso: today,
                lunchTime: "",
                menus: []
            }
        }
        var current = stateFor(code)
        var failureCount = Number(current.consecutiveFailures) || 0

        if (assumeWeekendNoMenu) {
            failureCount = 0
        } else if (!freshToday && !fromCache) {
            failureCount += 1
        } else if (freshToday) {
            failureCount = 0
        }

        var nextRetryEpochMs = Number(current.nextRetryEpochMs) || 0
        var assumedNoMenuRetryEpochMs = Number(current.assumedNoMenuRetryEpochMs) || 0
        if (assumeWeekendNoMenu) {
            nextRetryEpochMs = 0
            assumedNoMenuRetryEpochMs = Date.now() + weekendNoMenuRetryDelayMs()
        } else if (freshToday) {
            nextRetryEpochMs = 0
            assumedNoMenuRetryEpochMs = 0
        } else if (!fromCache) {
            nextRetryEpochMs = Date.now() + retryDelayMinutes(failureCount) * 60 * 1000
            assumedNoMenuRetryEpochMs = 0
        } else if (!isFinite(nextRetryEpochMs) || nextRetryEpochMs < 0) {
            nextRetryEpochMs = 0
            assumedNoMenuRetryEpochMs = 0
        }

        updateState(code, {
            status: freshToday ? "ok" : "stale",
            errorMessage: freshToday ? "" : dateMismatchMessage(),
            lastUpdatedEpochMs: updatedMs,
            payloadText: payloadText,
            rawPayload: parsed,
            todayMenu: todayMenu,
            menuDateIso: menuDateIso,
            providerDateValid: !!providerDateValid,
            isTodayFresh: freshToday,
            consecutiveFailures: failureCount,
            nextRetryEpochMs: nextRetryEpochMs,
            assumedNoMenuWeekend: assumeWeekendNoMenu,
            assumedNoMenuRetryEpochMs: assumedNoMenuRetryEpochMs,
            restaurantName: restaurantName,
            restaurantUrl: restaurantUrl
        })

        if ((!freshToday && !fromCache) || assumeWeekendNoMenu) {
            retryTimer.start()
        }

        if (String(code) === activeRestaurantCode) {
            syncSettingsLastUpdatedDisplay()
        }

        if (!fromCache) {
            saveCacheEntry(code, payloadText, updatedMs)
        }

        return true
    }

    function loadCachedPayloadsForCurrentLanguage() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var entry = cacheStore[cacheKey(code)]
            if (!entry || !entry.payload) {
                continue
            }
            applyPayloadForCode(code, entry.payload, true, entry.lastUpdatedEpochMs)
        }
    }

    function rederiveStateFromCachedPayload() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var state = stateFor(code)
            if (!state.payloadText) {
                continue
            }
            applyPayloadForCode(code, state.payloadText, true, state.lastUpdatedEpochMs)
        }
    }

    function buildRequestUrl(code) {
        var entry = restaurantEntryForCode(code)
        if (!entry) {
            return ""
        }

        if (entry.provider === "antell") {
            return String(entry.antellUrlBase)
                + "?print_lunch_day="
                + encodeURIComponent(weekdayToken(new Date()))
                + "&print_lunch_list_day=1"
        }

        if (entry.provider === "compass-rss") {
            var rssCost = String(entry.rssCostNumber || "").trim()
            if (!rssCost) {
                return ""
            }
            return apiRssBaseUrl
                + "?costNumber="
                + encodeURIComponent(rssCost)
                + "&language="
                + encodeURIComponent(configLanguage)
        }

        if (entry.provider === "huomen-json") {
            var huomenApi = String(entry.huomenApiBase || "").trim()
            if (!huomenApi) {
                return ""
            }
            var separator = huomenApi.indexOf("?") >= 0 ? "&" : "?"
            return huomenApi + separator + "language=" + encodeURIComponent(configLanguage)
        }

        return apiBaseUrl + "?costNumber=" + encodeURIComponent(String(code)) + "&language=" + encodeURIComponent(configLanguage)
    }

    function fetchRestaurant(code, manual) {
        if (!isKnownRestaurant(code)) {
            return
        }

        var normalized = String(code)
        var current = stateFor(normalized)
        if (!manual && current.status === "loading") {
            return
        }

        requestSerialByCode[normalized] = (requestSerialByCode[normalized] || 0) + 1
        var requestSerial = requestSerialByCode[normalized]

        if (!current.payloadText) {
            updateState(normalized, {
                status: "loading",
                errorMessage: ""
            })
        }

        var requestUrl = buildRequestUrl(normalized)
        if (!requestUrl) {
            setErrorStateForCode(normalized, "Unsupported restaurant provider")
            return
        }

        var xhr = new XMLHttpRequest()
        xhr.open("GET", requestUrl)
        xhr.timeout = manual ? 15000 : 10000

        xhr.onreadystatechange = function() {
            if (xhr.readyState !== XMLHttpRequest.DONE) {
                return
            }
            if (requestSerial !== requestSerialByCode[normalized]) {
                return
            }

            if (xhr.status >= 200 && xhr.status < 300) {
                applyPayloadForCode(normalized, xhr.responseText, false, 0)
            } else {
                setErrorStateForCode(normalized, "HTTP " + xhr.status)
            }
        }

        xhr.onerror = function() {
            if (requestSerial !== requestSerialByCode[normalized]) {
                return
            }
            setErrorStateForCode(normalized, "Network error")
        }

        xhr.ontimeout = function() {
            if (requestSerial !== requestSerialByCode[normalized]) {
                return
            }
            setErrorStateForCode(normalized, "Request timed out")
        }

        xhr.send()
    }

    function evaluateFreshnessAndRefresh(forceNetwork, manual) {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            if (forceNetwork || manual) {
                fetchRestaurant(code, !!manual)
                continue
            }

            var state = stateFor(code)
            if (!isStateFreshForToday(state)) {
                fetchRestaurant(code, false)
            }
        }
    }

    function processDueRetries() {
        var nowMs = Date.now()
        var codes = restaurantCodes()
        var hasPendingRetry = false

        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var state = stateFor(code)
            var dueMs = Number(state.nextRetryEpochMs) || 0
            var assumedDueMs = Number(state.assumedNoMenuRetryEpochMs) || 0

            if (state.assumedNoMenuWeekend && assumedDueMs > 0) {
                hasPendingRetry = true
                if (assumedDueMs <= nowMs) {
                    fetchRestaurant(code, false)
                }
                continue
            }

            if (!dueMs || isStateFreshForToday(state)) {
                continue
            }

            hasPendingRetry = true
            if (dueMs <= nowMs) {
                fetchRestaurant(code, false)
            }
        }

        if (!hasPendingRetry) {
            retryTimer.stop()
        }
    }

    function scheduleMidnightTimer() {
        var now = new Date()
        var next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 1, 0, 0)
        var msUntil = next.getTime() - now.getTime()
        midnightTimer.interval = Math.max(60000, msUntil)
        midnightTimer.restart()
    }

    function openConfigureAction() {
        var configureAction = Plasmoid.action("configure")
        if (configureAction && configureAction.enabled) {
            configureAction.trigger()
        }
    }

    function cycleRestaurant(step) {
        if (!configEnableWheelCycle) {
            return
        }

        var codes = restaurantCodes()
        if (codes.length < 2) {
            return
        }

        var idx = codes.indexOf(activeRestaurantCode)
        if (idx < 0) {
            idx = 0
        }

        var nextIdx = (idx + step + codes.length) % codes.length
        activeRestaurantCode = codes[nextIdx]

        if (!isStateFreshForToday(stateFor(activeRestaurantCode))) {
            fetchRestaurant(activeRestaurantCode, false)
        }
    }

    function tooltipMainText() {
        var state = stateFor(activeRestaurantCode)
        var title = state.restaurantName || "Compass Lunch"
        if (state.status === "stale" && !state.isTodayFresh) {
            return "[STALE] " + title
        }
        return title
    }

    function tooltipSubText() {
        var state = stateFor(activeRestaurantCode)
        var entry = restaurantEntryForCode(activeRestaurantCode)
        var isCompassProvider = !!entry && entry.provider === "compass"
        return MenuFormatter.buildTooltipSubText(
            configLanguage,
            state.status,
            state.errorMessage,
            state.lastUpdatedEpochMs,
            state.todayMenu,
            configShowPrices,
            configShowStudentPrice,
            configShowStaffPrice,
            configShowGuestPrice,
            isCompassProvider,
            configHideExpensiveStudentMeals,
            configShowAllergens,
            configHighlightGlutenFree,
            configHighlightVeg,
            configHighlightLactoseFree
        )
    }

    function tooltipSubTextRich() {
        var state = stateFor(activeRestaurantCode)
        var entry = restaurantEntryForCode(activeRestaurantCode)
        var isCompassProvider = !!entry && entry.provider === "compass"
        return MenuFormatter.buildTooltipSubTextRich(
            configLanguage,
            state.status,
            state.errorMessage,
            state.lastUpdatedEpochMs,
            state.todayMenu,
            configShowPrices,
            configShowStudentPrice,
            configShowStaffPrice,
            configShowGuestPrice,
            isCompassProvider,
            configHideExpensiveStudentMeals,
            configShowAllergens,
            configHighlightGlutenFree,
            configHighlightVeg,
            configHighlightLactoseFree
        )
    }

    function activeIconName() {
        var state = stateFor(activeRestaurantCode)
        return (state.status === "error" || state.status === "stale") ? "dialog-warning" : configIconName
    }

    function bootstrapData() {
        ensureStateMaps()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        evaluateFreshnessAndRefresh(false, false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigRestaurantCodeChanged: {
        activeRestaurantCode = configRestaurantCode
        if (!isStateFreshForToday(stateFor(activeRestaurantCode))) {
            fetchRestaurant(activeRestaurantCode, false)
        }
        syncSettingsLastUpdatedDisplay()
    }

    onActiveRestaurantCodeChanged: syncSettingsLastUpdatedDisplay()

    onConfigLanguageChanged: {
        resetAllStates()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        evaluateFreshnessAndRefresh(false, false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigEnabledRestaurantCodesChanged: {
        resetAllStates()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        evaluateFreshnessAndRefresh(false, false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigRefreshMinutesChanged: {
        refreshTimer.interval = Math.max(1, configRefreshMinutes) * 60 * 1000
        if (configRefreshMinutes > 0) {
            refreshTimer.restart()
        } else {
            refreshTimer.stop()
        }
    }
    onConfigManualRefreshTokenChanged: {
        if (!initialized) {
            return
        }
        evaluateFreshnessAndRefresh(true, true)
    }

    Component.onCompleted: {
        migrateEnabledRestaurantCodes()
        bootstrapData()
        scheduleMidnightTimer()
        initialized = true
    }

    Timer {
        id: refreshTimer
        interval: Math.max(1, root.configRefreshMinutes) * 60 * 1000
        running: root.configRefreshMinutes > 0
        repeat: true
        onTriggered: root.evaluateFreshnessAndRefresh(false, false)
    }

    Timer {
        id: retryTimer
        interval: 30000
        running: false
        repeat: true
        onTriggered: root.processDueRetries()
    }

    Timer {
        id: midnightTimer
        repeat: false
        running: false
        onTriggered: {
            root.rederiveStateFromCachedPayload()
            root.evaluateFreshnessAndRefresh(false, false)
            root.scheduleMidnightTimer()
        }
    }

    Plasmoid.icon: {
        var _ = modelVersion
        return activeIconName()
    }
    Plasmoid.status: PlasmaCore.Types.ActiveStatus
    toolTipTextFormat: Text.RichText
    toolTipMainText: {
        var _ = modelVersion
        return tooltipMainText()
    }
    toolTipSubText: {
        var _ = modelVersion
        return tooltipSubTextRich()
    }

    Plasmoid.onActivated: {
        Plasmoid.expanded = true
    }

    compactRepresentation: Item {
        id: compactRoot
        implicitWidth: PlasmaCore.Units.iconSizes.smallMedium
        implicitHeight: PlasmaCore.Units.iconSizes.smallMedium

        Kirigami.Icon {
            anchors.fill: parent
            source: Plasmoid.icon
            active: compactMouse.containsMouse
        }

        MouseArea {
            id: compactMouse
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton

            onClicked: {
                if (mouse.button === Qt.MiddleButton) {
                    var state = root.stateFor(root.activeRestaurantCode)
                    if (state.restaurantUrl) {
                        Qt.openUrlExternally(state.restaurantUrl)
                        return
                    }
                }
                Plasmoid.expanded = true
            }

            onWheel: {
                if (!root.configEnableWheelCycle) {
                    return
                }
                if (wheel.angleDelta.y > 0) {
                    root.cycleRestaurant(-1)
                } else if (wheel.angleDelta.y < 0) {
                    root.cycleRestaurant(1)
                }
                wheel.accepted = true
            }
        }
    }

    fullRepresentation: Item {
        implicitWidth: 480
        implicitHeight: 380

        Rectangle {
            anchors.fill: parent
            color: PlasmaCore.Theme.backgroundColor
            radius: Kirigami.Units.smallSpacing * 2
            border.width: 1
            border.color: PlasmaCore.Theme.highlightColor

            Flickable {
                id: flick
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing * 2
                contentWidth: width
                contentHeight: fullText.paintedHeight
                clip: true

                QQC2.Label {
                    id: fullText
                    width: flick.width
                    wrapMode: Text.Wrap
                    textFormat: Text.RichText
                    text: root.tooltipSubTextRich()
                }
            }
        }
    }
}
