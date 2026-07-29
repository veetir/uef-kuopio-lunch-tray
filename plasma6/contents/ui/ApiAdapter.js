.pragma library

function normalizeText(value) {
    return String(value === undefined || value === null ? "" : value)
        .replace(/\s+/g, " ")
        .trim();
}

function helsinkiDateIso(dateObj) {
    var timestamp = dateObj.getTime();
    var year = dateObj.getUTCFullYear();
    var marchLast = new Date(Date.UTC(year, 2, 31));
    var octoberLast = new Date(Date.UTC(year, 9, 31));
    var daylightSavingStart = Date.UTC(
        year,
        2,
        31 - marchLast.getUTCDay(),
        1
    );
    var daylightSavingEnd = Date.UTC(
        year,
        9,
        31 - octoberLast.getUTCDay(),
        1
    );
    var offsetHours = timestamp >= daylightSavingStart
        && timestamp < daylightSavingEnd
        ? 3
        : 2;
    var helsinki = new Date(timestamp + offsetHours * 60 * 60 * 1000);
    var month = String(helsinki.getUTCMonth() + 1);
    var day = String(helsinki.getUTCDate());
    if (month.length < 2) {
        month = "0" + month;
    }
    if (day.length < 2) {
        day = "0" + day;
    }
    return helsinki.getUTCFullYear() + "-" + month + "-" + day;
}

function retrySchedule(previousCount, previousDate, currentDate, nowMs) {
    var count = previousDate === currentDate
        ? Math.max(0, Number(previousCount) || 0) + 1
        : 1;
    var delayMinutes = count === 1 ? 5 : (count === 2 ? 15 : 60);
    return {
        failureCount: count,
        retryDateIso: currentDate,
        nextRetryEpochMs: count < 4
            ? Number(nowMs) + delayMinutes * 60 * 1000
            : 0
    };
}

function automaticRetryDue(
    failureCount,
    retryDate,
    currentDate,
    nextRetryEpochMs,
    nowMs
) {
    if (retryDate !== currentDate) {
        return true;
    }
    var count = Math.max(0, Number(failureCount) || 0);
    var dueMs = Number(nextRetryEpochMs) || 0;
    if (count >= 4 && !dueMs) {
        return false;
    }
    return !dueMs || dueMs <= Number(nowMs);
}

function automaticRefreshDue(
    forceNetwork,
    failureCount,
    retryDate,
    currentDate,
    nextRetryEpochMs,
    nowMs
) {
    return !!forceNetwork || automaticRetryDue(
        failureCount,
        retryDate,
        currentDate,
        nextRetryEpochMs,
        nowMs
    );
}

function euroText(price) {
    var amount = normalizeText(price && price.amount);
    if (!amount) {
        return "";
    }
    return amount.replace(".", ",") + " €";
}

function audienceLabel(audience, language) {
    var labels = language === "en"
        ? { student: "Student", staff: "Staff", guest: "Guest" }
        : { student: "Opiskelija", staff: "Henkilökunta", guest: "Vierailija" };
    return labels[audience] || "";
}

function groupPriceText(prices, language) {
    var source = Array.isArray(prices) ? prices : [];
    var segments = [];

    for (var i = 0; i < source.length; i++) {
        var priceText = euroText(source[i]);
        if (!priceText) {
            continue;
        }

        var audiences = Array.isArray(source[i].audiences)
            ? source[i].audiences
            : [];
        if (audiences.length === 0) {
            segments.push(priceText);
            continue;
        }

        for (var j = 0; j < audiences.length; j++) {
            var label = audienceLabel(audiences[j], language);
            if (label) {
                segments.push(label + " " + priceText);
            }
        }
    }

    return segments.join(" / ");
}

function itemText(item) {
    var name = normalizeText(item && item.name);
    if (!name) {
        return "";
    }

    var description = normalizeText(item && item.description);
    if (description) {
        name += " – " + description;
    }

    var tags = Array.isArray(item && item.tags) ? item.tags : [];
    var cleanTags = [];
    for (var i = 0; i < tags.length; i++) {
        var tag = normalizeText(tags[i]);
        if (tag) {
            cleanTags.push(tag);
        }
    }
    return name + (cleanTags.length > 0 ? " (" + cleanTags.join(", ") + ")" : "");
}

function normalizedMenus(payload, language) {
    var menus = [];
    var offers = Array.isArray(payload && payload.offers) ? payload.offers : [];
    var groups = Array.isArray(payload && payload.groups) ? payload.groups : [];

    for (var i = 0; i < offers.length; i++) {
        var offer = offers[i] || {};
        var offerName = normalizeText(offer.label);
        var offerDescription = normalizeText(offer.description);
        if (!offerName && !offerDescription) {
            continue;
        }
        menus.push({
            sortOrder: -1000 + i,
            name: offerName,
            price: euroText(offer.price),
            components: offerDescription ? [offerDescription] : [],
            audiencePrices: false
        });
    }

    for (var j = 0; j < groups.length; j++) {
        var group = groups[j] || {};
        var components = [];
        var items = Array.isArray(group.items) ? group.items : [];
        for (var k = 0; k < items.length; k++) {
            var component = itemText(items[k]);
            if (component) {
                components.push(component);
            }
        }
        if (components.length === 0) {
            continue;
        }

        var prices = Array.isArray(group.prices) ? group.prices : [];
        var audiencePrices = false;
        for (var p = 0; p < prices.length; p++) {
            if (Array.isArray(prices[p] && prices[p].audiences)
                    && prices[p].audiences.length > 0) {
                audiencePrices = true;
                break;
            }
        }

        menus.push({
            sortOrder: Number(group.sortOrder) || 0,
            name: normalizeText(group.title),
            price: groupPriceText(prices, language),
            components: components,
            audiencePrices: audiencePrices
        });
    }

    return menus;
}

function localizedRestaurantName(restaurant, language) {
    var names = restaurant && restaurant.name ? restaurant.name : {};
    return normalizeText(names[language])
        || normalizeText(names.fi)
        || normalizeText(names.en);
}

function dateParts(isoDate) {
    var match = normalizeText(isoDate).match(/^(\d{4})-(\d{2})-(\d{2})$/);
    if (!match) {
        return null;
    }
    return {
        year: Number(match[1]),
        month: Number(match[2]),
        day: Number(match[3])
    };
}

function closureMessage(closure, targetDate, language) {
    var end = dateParts(closure && closure.endsOn);
    if (!end) {
        return language === "en" ? "Closed." : "Suljettu.";
    }

    var reference = dateParts(targetDate);
    var includeYear = !reference || reference.year !== end.year;
    var date;
    if (language === "en") {
        var months = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December"
        ];
        date = end.day + " " + months[end.month - 1]
            + (includeYear ? " " + end.year : "");
    } else {
        var finnishMonths = [
            "tammikuuta", "helmikuuta", "maaliskuuta", "huhtikuuta",
            "toukokuuta", "kesäkuuta", "heinäkuuta", "elokuuta",
            "syyskuuta", "lokakuuta", "marraskuuta", "joulukuuta"
        ];
        date = end.day + ". " + finnishMonths[end.month - 1]
            + (includeYear ? " " + end.year : "");
    }

    var message = language === "en"
        ? "Closed until " + date + "."
        : "Suljettu " + date + " asti.";
    var reason = normalizeText(closure && closure.reason);
    return reason ? message + " " + reason : message;
}

function normalizePayload(payload, expectedRestaurantId, targetDate, language) {
    if (!payload || payload.apiVersion !== "v1" || Number(payload.schemaVersion) !== 1) {
        return { error: "Unsupported API response" };
    }
    if (!payload.restaurant
            || normalizeText(payload.restaurant.id) !== normalizeText(expectedRestaurantId)) {
        return { error: "Restaurant mismatch" };
    }
    if (normalizeText(payload.date) !== normalizeText(targetDate)) {
        return { error: "Date mismatch" };
    }
    if (!payload.service || !Array.isArray(payload.offers) || !Array.isArray(payload.groups)) {
        return { error: "Incomplete API response" };
    }

    var status = normalizeText(payload.service.status);
    if (["serving", "closed", "noMenu", "unknown"].indexOf(status) < 0) {
        status = "unknown";
    }

    var serviceState = "";
    var serviceMessage = "";
    var todayMenu = {
        dateIso: targetDate,
        lunchTime: normalizeText(payload.service.hours),
        menus: normalizedMenus(payload, language)
    };

    if (status === "closed") {
        serviceState = "closed";
        serviceMessage = closureMessage(payload.closure, targetDate, language);
        todayMenu = null;
    } else if (status === "unknown") {
        return {
            error: language === "en" ? "Menu unavailable" : "Ruokalistaa ei saatavilla",
            payload: payload
        };
    }

    var fetchedAt = Date.parse(payload.freshness && payload.freshness.fetchedAt);
    return {
        payload: payload,
        todayMenu: todayMenu,
        menuDateIso: targetDate,
        providerDateValid: true,
        serviceState: serviceState,
        serviceMessage: serviceMessage,
        isStale: !!(payload.freshness && payload.freshness.isStale),
        fetchedAtEpochMs: isFinite(fetchedAt) ? fetchedAt : 0,
        restaurantName: localizedRestaurantName(payload.restaurant, language),
        restaurantUrl: normalizeText(payload.restaurant.websiteUrl)
    };
}
