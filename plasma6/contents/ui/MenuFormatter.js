.pragma library

var MAX_MENU_HEADING_CHARS = 140;
var MAX_MENU_PRICE_CHARS = 96;
var MAX_COMPONENT_MAIN_CHARS = 220;

function normalizeText(value) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value).replace(/\s*\n+\s*/g, " ").replace(/\s+/g, " ").trim();
}

function truncateDisplayText(value, maxChars) {
    var clean = normalizeText(value);
    var limit = Number(maxChars) || 0;
    if (limit <= 0 || clean.length <= limit) {
        return clean;
    }
    if (limit <= 3) {
        return clean.slice(0, limit);
    }
    return clean.slice(0, limit - 3) + "...";
}

function escapeHtml(value) {
    var text = normalizeText(value);
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\"/g, "&quot;")
        .replace(/'/g, "&#39;");
}

function dayKey(dateString) {
    var clean = normalizeText(dateString);
    if (!clean) {
        return "";
    }
    var parts = clean.split("T");
    return parts[0] || "";
}

function formatDisplayDate(dateIso, language) {
    var iso = normalizeText(dateIso);
    var match = iso.match(/^(\d{4})-(\d{2})-(\d{2})$/);
    if (!match) {
        return iso;
    }

    var year = match[1];
    var month = parseInt(match[2], 10);
    var day = parseInt(match[3], 10);

    if (language === "fi") {
        return day + "." + month + "." + year;
    }

    return month + "/" + day + "/" + year;
}

function dateAndTimeLine(todayMenu, language) {
    if (!todayMenu) {
        return "";
    }

    var datePart = formatDisplayDate(todayMenu.dateIso, language);
    var timePart = normalizeText(todayMenu.lunchTime);

    if (datePart && timePart) {
        return datePart + " " + timePart;
    }
    if (datePart) {
        return datePart;
    }
    return timePart;
}

function textFor(language, key) {
    var fi = {
        loading: "Ladataan ruokalistaa...",
        noMenu: "Tälle päivälle ei ole lounaslistaa.",
        stale: "Ruokalistan päivä ei vastaa tämän päivän päivämäärää",
        fetchError: "Päivitysvirhe"
    };

    var en = {
        loading: "Loading menu...",
        noMenu: "No lunch menu available for today.",
        stale: "Menu date does not match today. Waiting for a valid refresh",
        fetchError: "Fetch error"
    };

    var dict = language === "fi" ? fi : en;
    return dict[key] || key;
}

function parseCompassPriceSegments(priceText) {
    var raw = normalizeText(priceText);
    if (!raw) {
        return [];
    }

    var defs = [
        { key: "student", pattern: "(?:Opiskelija|Student|Opisk\\.?|Op\\.?)" },
        { key: "staff", pattern: "(?:Henkilökunta|Staff|Henk\\.?|Hk\\.?)" },
        { key: "guest", pattern: "(?:Vierailija|Guest|Vieras|Vier\\.?)" }
    ];
    var segments = [];
    var matchedRanges = [];

    for (var i = 0; i < defs.length; i++) {
        var regex = new RegExp("(" + defs[i].pattern + ")\\s*[:\\-]?\\s*([0-9]+(?:[\\.,][0-9]+)?\\s*€?)", "gi");
        var match;
        while ((match = regex.exec(raw)) !== null) {
            var label = normalizeText(match[1]);
            var value = normalizeText(match[2]);
            if (!value) {
                continue;
            }
            segments.push({
                key: defs[i].key,
                label: label,
                value: value,
                start: match.index
            });
            matchedRanges.push({
                start: match.index,
                end: regex.lastIndex
            });
        }
    }

    if (matchedRanges.length > 0) {
        matchedRanges.sort(function(a, b) {
            return a.start - b.start;
        });

        var cursor = 0;
        var leftovers = "";
        for (var j = 0; j < matchedRanges.length; j++) {
            var range = matchedRanges[j];
            if (range.start > cursor) {
                leftovers += raw.slice(cursor, range.start) + " ";
            }
            cursor = Math.max(cursor, range.end);
        }
        if (cursor < raw.length) {
            leftovers += raw.slice(cursor);
        }

        var maskedChars = raw.split("");
        for (var k = 0; k < matchedRanges.length; k++) {
            var range = matchedRanges[k];
            for (var p = range.start; p < range.end && p < maskedChars.length; p++) {
                maskedChars[p] = " ";
            }
        }
        var masked = maskedChars.join("");
        var baseRegex = /[0-9]+(?:[.,][0-9]+)?\s*€?/g;
        var baseMatch;
        while ((baseMatch = baseRegex.exec(masked)) !== null) {
            var baseValue = normalizeText(baseMatch[0]);
            if (baseValue) {
                segments.push({
                    key: "base",
                    label: "",
                    value: baseValue,
                    start: baseMatch.index
                });
            }
        }
    } else {
        var standaloneRegex = /[0-9]+(?:[.,][0-9]+)?\s*€?/g;
        var standaloneMatch;
        while ((standaloneMatch = standaloneRegex.exec(raw)) !== null) {
            var standalone = normalizeText(standaloneMatch[0]);
            if (standalone) {
                segments.push({
                    key: "base",
                    label: "",
                    value: standalone,
                    start: standaloneMatch.index
                });
            }
        }
    }

    segments.sort(function(a, b) {
        return (Number(a.start) || 0) - (Number(b.start) || 0);
    });

    return segments;
}

function shouldShowPriceSegment(segmentKey, showStudentPrice, showStaffPrice, showGuestPrice) {
    if (segmentKey === "student") {
        return showStudentPrice !== false;
    }
    if (segmentKey === "staff") {
        return showStaffPrice !== false;
    }
    if (segmentKey === "guest") {
        return showGuestPrice !== false;
    }
    if (segmentKey === "base") {
        return showStaffPrice !== false || showGuestPrice !== false;
    }
    return true;
}

function formatCompassPrice(priceText, showStudentPrice, showStaffPrice, showGuestPrice) {
    var segments = parseCompassPriceSegments(priceText);
    if (segments.length === 0) {
        return normalizeText(priceText);
    }

    var selected = [];
    for (var i = 0; i < segments.length; i++) {
        var segment = segments[i];
        if (shouldShowPriceSegment(segment.key, showStudentPrice, showStaffPrice, showGuestPrice)) {
            selected.push(segment.label ? (segment.label + " " + segment.value) : segment.value);
        }
    }

    return selected.join(" / ");
}

function parseEuroNumber(valueText) {
    var clean = normalizeText(valueText).replace(",", ".");
    var match = clean.match(/([0-9]+(?:\.[0-9]+)?)/);
    if (!match) {
        return null;
    }
    var parsed = Number(match[1]);
    return isFinite(parsed) ? parsed : null;
}

function compassStudentPriceValue(priceText) {
    var segments = parseCompassPriceSegments(priceText);
    if (segments.length === 0) {
        return null;
    }

    var student = null;
    var fallbackBase = null;
    for (var i = 0; i < segments.length; i++) {
        var segment = segments[i];
        var numeric = parseEuroNumber(segment.value);
        if (numeric === null) {
            continue;
        }
        if (segment.key === "student") {
            if (student === null || numeric < student) {
                student = numeric;
            }
        } else if (segment.key === "base") {
            if (fallbackBase === null || numeric < fallbackBase) {
                fallbackBase = numeric;
            }
        }
    }

    return student !== null ? student : fallbackBase;
}

function shouldHideMenuByStudentPrice(menu, hideExpensiveStudentMeals, isCompassProvider) {
    if (!hideExpensiveStudentMeals || !isCompassProvider) {
        return false;
    }

    var studentPrice = compassStudentPriceValue(menu && menu.price);
    return studentPrice !== null && studentPrice > 4.0;
}

function menuHeading(menu, showPrices, showStudentPrice, showStaffPrice, showGuestPrice, isCompassProvider) {
    var heading = truncateDisplayText(menu && menu.name, MAX_MENU_HEADING_CHARS);
    if (!heading) {
        heading = "Menu";
    }

    var price = truncateDisplayText(menu && menu.price, MAX_MENU_PRICE_CHARS);
    if (showPrices && price) {
        if (isCompassProvider) {
            price = formatCompassPrice(price, showStudentPrice, showStaffPrice, showGuestPrice);
            if (!price) {
                return heading;
            }
        }
        return heading + " - " + price;
    }

    return heading;
}

function splitComponentSuffix(component) {
    var text = normalizeText(component);
    var match = text.match(/^(.*\S)\s+(\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\))$/);
    if (!match) {
        return {
            main: text,
            suffix: ""
        };
    }
    return {
        main: normalizeText(match[1]),
        suffix: normalizeText(match[2])
    };
}

function shouldShowAllergens(showAllergens) {
    return showAllergens !== false;
}

function shouldHighlightTag(tag, highlightGlutenFree, highlightVeg, highlightLactoseFree) {
    var normalized = normalizeText(tag).toUpperCase();
    if (!normalized) {
        return false;
    }
    if (highlightGlutenFree && normalized === "G") {
        return true;
    }
    if (highlightVeg && normalized === "VEG") {
        return true;
    }
    if (highlightLactoseFree && normalized === "L") {
        return true;
    }
    return false;
}

function plainComponentLine(component, showAllergens) {
    var parts = splitComponentSuffix(component);
    var main = truncateDisplayText(parts.main, MAX_COMPONENT_MAIN_CHARS);
    if (shouldShowAllergens(showAllergens) || !parts.suffix) {
        return main + (parts.suffix ? " " + parts.suffix : "");
    }
    return main;
}

function highlightSuffixRich(suffix, highlightGlutenFree, highlightVeg, highlightLactoseFree) {
    var clean = normalizeText(suffix);
    if (!clean || clean.charAt(0) !== "(" || clean.charAt(clean.length - 1) !== ")") {
        return escapeHtml(clean);
    }

    var inner = clean.slice(1, -1);
    var tags = inner ? inner.split(/\s*,\s*/) : [];
    var styledTags = [];

    for (var i = 0; i < tags.length; i++) {
        var tag = normalizeText(tags[i]);
        if (!tag) {
            continue;
        }
        var escapedTag = escapeHtml(tag);
        if (shouldHighlightTag(tag, highlightGlutenFree, highlightVeg, highlightLactoseFree)) {
            styledTags.push("<b>" + escapedTag + "</b>");
        } else {
            styledTags.push(escapedTag);
        }
    }

    return "(" + styledTags.join(", ") + ")";
}

function buildTooltipSubText(language, fetchState, errorMessage, lastUpdatedEpochMs, todayMenu, showPrices, showStudentPrice, showStaffPrice, showGuestPrice, isCompassProvider, hideExpensiveStudentMeals, showAllergens, highlightGlutenFree, highlightVeg, highlightLactoseFree) {
    var lines = [];

    if (fetchState === "stale") {
        lines.push("[STALE]");
    }

    if (!todayMenu && fetchState === "loading") {
        lines.push(textFor(language, "loading"));
    }

    var dateLine = dateAndTimeLine(todayMenu, language);
    if (dateLine) {
        lines.push(dateLine);
    }

    if (todayMenu && todayMenu.menus && todayMenu.menus.length > 0) {
        var hasVisibleMenu = false;
        for (var i = 0; i < todayMenu.menus.length; i++) {
            var menu = todayMenu.menus[i];
            if (shouldHideMenuByStudentPrice(menu, hideExpensiveStudentMeals, isCompassProvider)) {
                continue;
            }
            hasVisibleMenu = true;
            lines.push(menuHeading(menu, showPrices, showStudentPrice, showStaffPrice, showGuestPrice, isCompassProvider));
            var components = menu.components || [];
            for (var j = 0; j < components.length; j++) {
                var component = plainComponentLine(components[j], showAllergens);
                if (component) {
                    lines.push("  - " + component);
                }
            }
        }
        if (!hasVisibleMenu && fetchState !== "loading") {
            lines.push(textFor(language, "noMenu"));
        }
    } else if (fetchState !== "loading") {
        lines.push(textFor(language, "noMenu"));
    }

    if (fetchState === "stale") {
        lines.push("");
        lines.push(textFor(language, "stale"));
    }

    var cleanError = normalizeText(errorMessage);
    if (cleanError && fetchState !== "ok") {
        lines.push(textFor(language, "fetchError") + ": " + cleanError);
    }

    return lines.join("\n");
}

function buildTooltipSubTextRich(language, fetchState, errorMessage, lastUpdatedEpochMs, todayMenu, showPrices, showStudentPrice, showStaffPrice, showGuestPrice, isCompassProvider, hideExpensiveStudentMeals, showAllergens, highlightGlutenFree, highlightVeg, highlightLactoseFree) {
    var lines = [];

    if (fetchState === "stale") {
        lines.push("<b>[STALE]</b>");
    }

    if (!todayMenu && fetchState === "loading") {
        lines.push(escapeHtml(textFor(language, "loading")));
    }

    var dateLine = dateAndTimeLine(todayMenu, language);
    if (dateLine) {
        lines.push("<b>" + escapeHtml(dateLine) + "</b>");
    }

    if (todayMenu && todayMenu.menus && todayMenu.menus.length > 0) {
        var hasVisibleMenu = false;
        for (var i = 0; i < todayMenu.menus.length; i++) {
            var menu = todayMenu.menus[i];
            if (shouldHideMenuByStudentPrice(menu, hideExpensiveStudentMeals, isCompassProvider)) {
                continue;
            }
            hasVisibleMenu = true;
            lines.push("<b>" + escapeHtml(menuHeading(menu, showPrices, showStudentPrice, showStaffPrice, showGuestPrice, isCompassProvider)) + "</b>");

            var components = menu.components || [];
            for (var j = 0; j < components.length; j++) {
                var component = normalizeText(components[j]);
                if (component) {
                    var parts = splitComponentSuffix(component);
                    var componentLine = "&nbsp;&nbsp;&nbsp;▸ " + escapeHtml(truncateDisplayText(parts.main, MAX_COMPONENT_MAIN_CHARS));
                    if (parts.suffix && shouldShowAllergens(showAllergens)) {
                        componentLine += " <small><font color=\"#808080\">" + highlightSuffixRich(parts.suffix, highlightGlutenFree, highlightVeg, highlightLactoseFree) + "</font></small>";
                    }
                    lines.push(componentLine);
                }
            }
        }
        if (!hasVisibleMenu && fetchState !== "loading") {
            lines.push(escapeHtml(textFor(language, "noMenu")));
        }
    } else if (fetchState !== "loading") {
        lines.push(escapeHtml(textFor(language, "noMenu")));
    }

    if (fetchState === "stale") {
        lines.push("&nbsp;");
        lines.push(escapeHtml(textFor(language, "stale")));
    }

    var cleanError = normalizeText(errorMessage);
    if (cleanError && fetchState !== "ok") {
        lines.push(escapeHtml(textFor(language, "fetchError") + ": " + cleanError));
    }

    return lines.join("<br/>");
}
