import QtQuick 2.15
import QtQuick.Controls 2.15 as QQC2
import QtCore
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid 2.0
import org.kde.kirigami 2.20 as Kirigami

import "MenuFormatter.js" as MenuFormatter
import "ApiAdapter.js" as ApiAdapter

PlasmoidItem {
    id: root

    property string apiBaseUrl: "https://lunch.veeti.dev/v1"
    property var allRestaurantCatalog: [
        { code: "snellmania", fallbackName: "Snellmania" },
        { code: "cafe-snellari", fallbackName: "Cafe Snellari" },
        { code: "canthia", fallbackName: "Canthia" },
        { code: "tietoteknia", fallbackName: "Tietoteknia" },
        { code: "hyva-huomen-bioteknia", fallbackName: "Hyvä Huomen Bioteknia" },
        { code: "antell-round", fallbackName: "Antell Round" },
        { code: "antell-highway", fallbackName: "Antell Highway" },
        { code: "mediteknia", fallbackName: "Mediteknia" },
        { code: "pranzeria-sorrento", fallbackName: "Pranzeria Sorrento" },
        { code: "caari", fallbackName: "Caari" }
    ]
    property var legacyRestaurantCodes: ({
        "0437": "snellmania",
        "snellari-rss": "cafe-snellari",
        "0436": "canthia",
        "0439": "tietoteknia",
        "huomen-bioteknia": "hyva-huomen-bioteknia",
        "043601": "mediteknia",
        "pranzeria-html": "pranzeria-sorrento",
        "3488": "caari"
    })
    property var restaurantCatalog: {
        var filtered = filteredRestaurantCatalog(configEnabledRestaurantCodes)
        return Array.isArray(filtered) ? filtered : []
    }

    property var restaurantStates: ({})
    property var cacheStore: ({})
    property int snapshotRequestSerial: 0
    property bool snapshotRequestInFlight: false
    property double lastSnapshotRequestEpochMs: 0
    property int minimumSnapshotRequestIntervalMs: 60000
    property int modelVersion: 0
    property int clockRevision: 0
    property bool initialized: false
    property string lastObservedDateIso: ""
    property int refreshProbeIntervalMs: 60000
    property var supportedIconNames: ["food", "compass", "map-globe", "map-flat"]
    property int maxPayloadChars: 524288
    property int maxCacheBlobChars: 1048576
    property var allowedExternalHostSuffixes: ["compass-group.fi", "antell.fi", "hyvahuomen.fi", "sorrento.fi", "github.com"]

    property string activeRestaurantCode: "snellmania"

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
        var migrated = migratedRestaurantCode(raw)
        return isKnownRestaurant(migrated) ? migrated : fallback
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

    function migratedRestaurantCode(code) {
        var normalized = String(code || "").trim()
        return legacyRestaurantCodes[normalized] || normalized
    }

    function parseConfiguredRestaurantCodes(rawValue) {
        var selectedMap = {}
        var raw = String(rawValue || "")
        if (raw.length > 0) {
            var tokens = raw.split(",")
            for (var i = 0; i < tokens.length; i++) {
                var token = migratedRestaurantCode(tokens[i])
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
        if (migrationLevel >= 4) {
            return
        }

        var raw = String(Plasmoid.configuration.enabledRestaurantCodes || "").trim()
        var selectedCodes = parseConfiguredRestaurantCodes(raw)
        if (selectedCodes.indexOf("caari") < 0) {
            selectedCodes.push("caari")
        }
        writeConfiguredRestaurantCodes(selectedCodes)

        var selectedRestaurant = migratedRestaurantCode(
            Plasmoid.configuration.restaurantCode
                || Plasmoid.configuration.costNumber
                || defaultRestaurantCode()
        )
        Plasmoid.configuration.restaurantCode = isKnownRestaurant(selectedRestaurant)
            ? selectedRestaurant
            : defaultRestaurantCode()
        Plasmoid.configuration.enabledRestaurantCodesMigrationLevel = 4
    }

    function defaultRestaurantCode() {
        var codes = restaurantCodes()
        if (codes.length > 0) {
            return String(codes[0])
        }
        return allRestaurantCatalog.length > 0 ? String(allRestaurantCatalog[0].code) : "snellmania"
    }

    function restaurantCodes() {
        var catalog = Array.isArray(restaurantCatalog) ? restaurantCatalog : []
        var list = []
        for (var i = 0; i < catalog.length; i++) {
            list.push(String(catalog[i].code))
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
        var catalog = Array.isArray(restaurantCatalog) ? restaurantCatalog : []
        for (var i = 0; i < catalog.length; i++) {
            if (String(catalog[i].code) === normalized) {
                return catalog[i]
            }
        }
        return null
    }

    function restaurantLabelForCode(code) {
        var normalized = String(code || "")
        var catalog = Array.isArray(restaurantCatalog) ? restaurantCatalog : []
        for (var i = 0; i < catalog.length; i++) {
            if (catalog[i].code === normalized) {
                return catalog[i].fallbackName
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
            serviceState: "",
            consecutiveFailures: 0,
            retryDateIso: "",
            nextRetryEpochMs: 0,
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

    function todayIso() {
        return ApiAdapter.helsinkiDateIso(new Date())
    }

    function isStateFreshForToday(state) {
        if (!state) {
            return false
        }
        return ApiAdapter.menuStateFreshForDate(
            state.status,
            state.providerDateValid,
            state.menuDateIso,
            todayIso()
        )
    }

    function isAllowedExternalUrl(rawUrl) {
        var value = MenuFormatter.normalizeText(rawUrl)
        if (!value || !/^https:\/\//i.test(value)) {
            return false
        }

        var match = value.match(/^https:\/\/([^\/?#:]+)(?::\d+)?(?:[\/?#]|$)/i)
        if (!match) {
            return false
        }

        var host = String(match[1] || "").toLowerCase()
        if (!host) {
            return false
        }

        for (var i = 0; i < allowedExternalHostSuffixes.length; i++) {
            var suffix = String(allowedExternalHostSuffixes[i] || "").toLowerCase()
            if (!suffix) {
                continue
            }
            if (host === suffix || host.slice(-(suffix.length + 1)) === ("." + suffix)) {
                return true
            }
        }

        return false
    }

    function sanitizeExternalUrl(rawUrl, fallbackUrl) {
        var primary = MenuFormatter.normalizeText(rawUrl)
        if (isAllowedExternalUrl(primary)) {
            return primary
        }

        var fallback = MenuFormatter.normalizeText(fallbackUrl)
        if (isAllowedExternalUrl(fallback)) {
            return fallback
        }

        return ""
    }

    function cacheKey(code) {
        return String(code) + "|" + configLanguage
    }

    function loadCacheStore() {
        var blob = String(cache.cacheBlob || "{}")
        if (blob.length > maxCacheBlobChars) {
            cacheStore = {}
            try {
                cache.cacheBlob = "{}"
            } catch (e1) {
            }
            return
        }

        try {
            var parsed = JSON.parse(blob)
            if (parsed && typeof parsed === "object") {
                cacheStore = parsed
            } else {
                cacheStore = {}
            }
        } catch (e) {
            cacheStore = {}
            try {
                cache.cacheBlob = "{}"
            } catch (e2) {
            }
        }
    }

    function saveCacheEntry(code, payloadText, updatedEpochMs) {
        var payload = String(payloadText || "")
        if (payload.length > maxPayloadChars) {
            return
        }

        var key = cacheKey(code)
        var previous = Object.prototype.hasOwnProperty.call(cacheStore, key) ? cacheStore[key] : undefined
        cacheStore[key] = {
            payload: payload,
            lastUpdatedEpochMs: Number(updatedEpochMs) || 0
        }

        try {
            var serialized = JSON.stringify(cacheStore)
            if (serialized.length <= maxCacheBlobChars) {
                cache.cacheBlob = serialized
                return
            }
        } catch (e) {
        }

        if (previous !== undefined) {
            cacheStore[key] = previous
        } else {
            delete cacheStore[key]
        }
    }

    function setErrorStateForCode(code, message, fromCache) {
        var current = stateFor(code)
        if (current.status === "ok" && isStateFreshForToday(current)) {
            return
        }

        var retry = ApiAdapter.retryStateAfterFailure(
            !!fromCache,
            current.consecutiveFailures,
            current.retryDateIso,
            todayIso(),
            Date.now()
        )
        updateState(code, {
            status: current.payloadText ? "stale" : "error",
            errorMessage: message,
            isTodayFresh: false,
            serviceState: "",
            consecutiveFailures: retry.failureCount,
            retryDateIso: retry.retryDateIso,
            nextRetryEpochMs: retry.nextRetryEpochMs
        })
        if (retry.nextRetryEpochMs) {
            retryTimer.start()
        }
    }

    function applyPayloadForCode(code, payloadText, fromCache, cachedTimestamp) {
        var apiPayload
        try {
            apiPayload = JSON.parse(payloadText)
        } catch (apiError) {
            setErrorStateForCode(code, "Invalid JSON payload", fromCache)
            return false
        }

        var apiMenu = ApiAdapter.normalizePayload(
            apiPayload,
            String(code),
            todayIso(),
            configLanguage
        )
        if (!apiMenu || apiMenu.error) {
            setErrorStateForCode(
                code,
                apiMenu && apiMenu.error ? apiMenu.error : "Invalid API response",
                fromCache
            )
            return false
        }

        var updatedMs = Number(apiMenu.fetchedAtEpochMs)
            || (fromCache ? (Number(cachedTimestamp) || 0) : Date.now())
        var current = stateFor(code)
        var stale = !!apiMenu.isStale
        var retry = stale
            ? ApiAdapter.retryStateAfterFailure(
                !!fromCache,
                current.consecutiveFailures,
                current.retryDateIso,
                todayIso(),
                Date.now()
            )
            : {
                failureCount: 0,
                retryDateIso: todayIso(),
                nextRetryEpochMs: 0
            }
        updateState(code, {
            status: stale ? "stale" : "ok",
            errorMessage: apiMenu.serviceMessage || "",
            lastUpdatedEpochMs: updatedMs,
            payloadText: payloadText,
            rawPayload: apiPayload,
            todayMenu: apiMenu.todayMenu,
            menuDateIso: apiMenu.menuDateIso,
            providerDateValid: true,
            isTodayFresh: true,
            serviceState: apiMenu.serviceState || "",
            consecutiveFailures: retry.failureCount,
            retryDateIso: retry.retryDateIso,
            nextRetryEpochMs: retry.nextRetryEpochMs,
            restaurantName: apiMenu.restaurantName || restaurantLabelForCode(code),
            restaurantUrl: sanitizeExternalUrl(apiMenu.restaurantUrl, "")
        })

        if (retry.nextRetryEpochMs) {
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

    function refreshIfDateChangedOrStale() {
        var currentDateIso = todayIso()
        var previousDateIso = MenuFormatter.normalizeText(lastObservedDateIso)

        if (!previousDateIso) {
            lastObservedDateIso = currentDateIso
        } else if (previousDateIso !== currentDateIso) {
            lastObservedDateIso = currentDateIso
            rederiveStateFromCachedPayload()
            evaluateFreshnessAndRefresh(false, false)
            scheduleMidnightTimer()
            return
        }

        var activeState = stateFor(activeRestaurantCode)
        if (activeState.status === "stale" || !isStateFreshForToday(activeState)) {
            evaluateFreshnessAndRefresh(false, false)
        }
    }

    function buildSnapshotRequestUrl() {
        return apiBaseUrl
            + "/snapshot?language="
            + encodeURIComponent(configLanguage)
            + "&date="
            + encodeURIComponent(todayIso())
    }

    function fetchDailySnapshot(manual) {
        if (snapshotRequestInFlight) {
            return
        }
        var requestStartedMs = Date.now()
        if (!manual
                && requestStartedMs - lastSnapshotRequestEpochMs
                    < minimumSnapshotRequestIntervalMs) {
            return
        }
        lastSnapshotRequestEpochMs = requestStartedMs

        snapshotRequestSerial += 1
        var requestSerial = snapshotRequestSerial
        snapshotRequestInFlight = true
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var current = stateFor(codes[i])
            if (!current.payloadText) {
                updateState(codes[i], {
                    status: "loading",
                    errorMessage: "",
                    serviceState: ""
                })
            }
        }

        var xhr = new XMLHttpRequest()
        xhr.open("GET", buildSnapshotRequestUrl())
        xhr.timeout = manual ? 15000 : 10000

        xhr.onreadystatechange = function() {
            if (xhr.readyState !== XMLHttpRequest.DONE) {
                return
            }
            if (requestSerial !== snapshotRequestSerial) {
                return
            }
            snapshotRequestInFlight = false

            if (xhr.status >= 200 && xhr.status < 300) {
                var responseText = String(xhr.responseText || "")
                if (responseText.length > maxPayloadChars) {
                    setSnapshotError("Payload too large")
                    return
                }
                var payload
                try {
                    payload = JSON.parse(responseText)
                } catch (parseError) {
                    setSnapshotError("Invalid JSON payload")
                    return
                }
                if (!payload
                        || payload.apiVersion !== "v1"
                        || Number(payload.schemaVersion) !== 1
                        || String(payload.requestedLanguage || "") !== configLanguage
                        || String(payload.date || "") !== todayIso()
                        || !Array.isArray(payload.menus)) {
                    setSnapshotError("Invalid API response")
                    return
                }

                var applied = {}
                for (var j = 0; j < payload.menus.length; j++) {
                    var menu = payload.menus[j]
                    var code = String(menu && menu.restaurant
                        ? menu.restaurant.id || ""
                        : "")
                    if (!isKnownRestaurant(code)) {
                        continue
                    }
                    var menuText = JSON.stringify(menu)
                    if (menuText.length > maxPayloadChars) {
                        setErrorStateForCode(code, "Payload too large")
                        continue
                    }
                    if (applyPayloadForCode(code, menuText, false, 0)) {
                        applied[code] = true
                    }
                }
                for (var k = 0; k < codes.length; k++) {
                    if (!applied[codes[k]]) {
                        setErrorStateForCode(codes[k], "Incomplete API response")
                    }
                }
            } else {
                setSnapshotError("HTTP " + xhr.status)
            }
        }

        xhr.onerror = function() {
            if (requestSerial !== snapshotRequestSerial) {
                return
            }
            snapshotRequestInFlight = false
            setSnapshotError("Network error")
        }

        xhr.ontimeout = function() {
            if (requestSerial !== snapshotRequestSerial) {
                return
            }
            snapshotRequestInFlight = false
            setSnapshotError("Request timed out")
        }

        xhr.send()
    }

    function setSnapshotError(message) {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            setErrorStateForCode(codes[i], message)
        }
    }

    function evaluateFreshnessAndRefresh(forceNetwork, manual) {
        var codes = restaurantCodes()
        if (manual) {
            fetchDailySnapshot(true)
            return
        }
        var nowMs = Date.now()
        var needsRefresh = false
        for (var i = 0; i < codes.length; i++) {
            var state = stateFor(codes[i])
            var needsRestaurant = !!forceNetwork
                || !isStateFreshForToday(state)
            if (needsRestaurant && ApiAdapter.automaticRefreshDue(
                    forceNetwork,
                    state.consecutiveFailures,
                    state.retryDateIso,
                    todayIso(),
                    state.nextRetryEpochMs,
                    nowMs)) {
                needsRefresh = true
                break
            }
        }
        if (needsRefresh) {
            fetchDailySnapshot(!!manual)
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

            if (!dueMs || (state.status === "ok" && isStateFreshForToday(state))) {
                continue
            }

            hasPendingRetry = true
            if (dueMs <= nowMs) {
                fetchDailySnapshot(false)
                return
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
            evaluateFreshnessAndRefresh(true, false)
        }
    }

    function tooltipMainText() {
        var state = stateFor(activeRestaurantCode)
        var title = MenuFormatter.truncateDisplayText(state.restaurantName || "Lunch", 160)
        var safeTitle = MenuFormatter.escapeHtml(title)
        if (state.status === "stale" && !state.isTodayFresh) {
            return "[STALE] " + safeTitle
        }
        return safeTitle
    }

    function tooltipSubText() {
        var state = stateFor(activeRestaurantCode)
        var isCompassProvider = false
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
            configHighlightLactoseFree,
            state.serviceState,
            state.errorMessage
        )
    }

    function tooltipSubTextRich() {
        var _clock = clockRevision
        var state = stateFor(activeRestaurantCode)
        var isCompassProvider = false
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
            configHighlightLactoseFree,
            state.serviceState,
            state.errorMessage,
            new Date(),
            String(PlasmaCore.Theme.textColor),
            String(PlasmaCore.Theme.backgroundColor)
        )
    }

    function activeIconName() {
        var state = stateFor(activeRestaurantCode)
        return (state.status === "error" || state.status === "stale") ? "dialog-warning" : configIconName
    }

    function bootstrapData() {
        ensureStateMaps()
        activeRestaurantCode = configRestaurantCode
        lastObservedDateIso = todayIso()
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        evaluateFreshnessAndRefresh(false, false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigRestaurantCodeChanged: {
        activeRestaurantCode = configRestaurantCode
        if (!isStateFreshForToday(stateFor(activeRestaurantCode))) {
            evaluateFreshnessAndRefresh(true, false)
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
        onTriggered: root.evaluateFreshnessAndRefresh(true, false)
    }

    Timer {
        id: retryTimer
        interval: 30000
        running: false
        repeat: true
        onTriggered: root.processDueRetries()
    }

    Timer {
        id: freshnessProbeTimer
        interval: root.refreshProbeIntervalMs
        running: true
        repeat: true
        onTriggered: {
            root.clockRevision += 1
            root.refreshIfDateChangedOrStale()
        }
    }

    Timer {
        id: midnightTimer
        repeat: false
        running: false
        onTriggered: {
            root.lastObservedDateIso = root.todayIso()
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
        implicitWidth: Kirigami.Units.iconSizes.smallMedium
        implicitHeight: Kirigami.Units.iconSizes.smallMedium

        Kirigami.Icon {
            anchors.fill: parent
            source: {
                var _ = modelVersion
                return activeIconName()
            }
            active: compactMouse.containsMouse
        }

        MouseArea {
            id: compactMouse
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton

            onEntered: root.refreshIfDateChangedOrStale()

            onClicked: {
                root.refreshIfDateChangedOrStale()
                if (mouse.button === Qt.MiddleButton) {
                    var state = root.stateFor(root.activeRestaurantCode)
                    var safeUrl = root.sanitizeExternalUrl(state.restaurantUrl, "")
                    if (safeUrl) {
                        Qt.openUrlExternally(safeUrl)
                        return
                    }
                }
                Plasmoid.expanded = true
            }

            onWheel: {
                if (!root.configEnableWheelCycle) {
                    return
                }
                root.refreshIfDateChangedOrStale()
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
