#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

function normalizeText(value) {
  if (value === null || value === undefined) {
    return "";
  }
  return String(value).replace(/\s*\n+\s*/g, " ").replace(/\s+/g, " ").trim();
}

function dayKey(dateString) {
  const clean = normalizeText(dateString);
  if (!clean) {
    return "";
  }
  return clean.split("T")[0] || "";
}

function localDateIso(dateObj) {
  const year = dateObj.getFullYear();
  const month = String(dateObj.getMonth() + 1).padStart(2, "0");
  const day = String(dateObj.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function normalizeMenusForDay(day) {
  const rawMenus = Array.isArray(day.SetMenus) ? [...day.SetMenus] : [];
  rawMenus.sort((a, b) => (Number(a.SortOrder) || 0) - (Number(b.SortOrder) || 0));

  return rawMenus
    .map((entry) => {
      const name = normalizeText(entry.Name) || "Menu";
      const price = normalizeText(entry.Price);
      const components = Array.isArray(entry.Components)
        ? entry.Components.map((item) => normalizeText(item)).filter(Boolean)
        : [];

      if (!name && components.length === 0) {
        return null;
      }

      return {
        sortOrder: Number(entry.SortOrder) || 0,
        name,
        price,
        components,
      };
    })
    .filter(Boolean);
}

function normalizeCompassToday(payload, targetDate) {
  if (!payload || !Array.isArray(payload.MenusForDays)) {
    return null;
  }

  const match = payload.MenusForDays.find((day) => dayKey(day.Date) === targetDate);
  if (!match) {
    return {
      todayMenu: null,
      menuDateIso: "",
      providerDateValid: false,
    };
  }

  return {
    todayMenu: {
      dateIso: targetDate,
      lunchTime: normalizeText(match.LunchTime),
      menus: normalizeMenusForDay(match),
    },
    menuDateIso: targetDate,
    providerDateValid: true,
  };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function readFixture(name) {
  const fixturePath = path.join(__dirname, "fixtures", name);
  const raw = fs.readFileSync(fixturePath, "utf8");
  return JSON.parse(raw);
}

function readTextFixture(name) {
  const fixturePath = path.join(__dirname, "fixtures", name);
  return fs.readFileSync(fixturePath, "utf8");
}

function decodeHtmlEntities(value) {
  return String(value)
    .replace(/&#x([0-9a-fA-F]+);/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
    .replace(/&#([0-9]+);/g, (_, dec) => String.fromCharCode(parseInt(dec, 10)))
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ");
}

function stripHtmlText(value) {
  return normalizeText(decodeHtmlEntities(String(value).replace(/<[^>]*>/g, " ")));
}

function parseAntellSections(htmlText) {
  const sections = [];
  const sectionRegex = /<section class="menu-section">([\s\S]*?)<\/section>/gi;
  let sectionMatch;

  while ((sectionMatch = sectionRegex.exec(String(htmlText))) !== null) {
    const sectionHtml = sectionMatch[1];
    const titleMatch = sectionHtml.match(/<h2 class="menu-title">([\s\S]*?)<\/h2>/i);
    const priceMatch = sectionHtml.match(/<h2 class="menu-price">([\s\S]*?)<\/h2>/i);
    const listMatch = sectionHtml.match(/<ul class="menu-list">([\s\S]*?)<\/ul>/i);

    const title = stripHtmlText(titleMatch ? titleMatch[1] : "");
    const price = stripHtmlText(priceMatch ? priceMatch[1] : "");
    const listHtml = listMatch ? listMatch[1] : "";

    const items = [];
    const itemRegex = /<li[^>]*>([\s\S]*?)<\/li>/gi;
    let itemMatch;
    while ((itemMatch = itemRegex.exec(listHtml)) !== null) {
      const item = stripHtmlText(itemMatch[1]);
      if (item) {
        items.push(item);
      }
    }

    if (items.length === 0) {
      continue;
    }

    sections.push({
      title: title || "Menu",
      price,
      items,
    });
  }

  return sections;
}

function parseAntellMenuDateIso(menuDateText, nowDate) {
  const clean = normalizeText(menuDateText);
  if (!clean) {
    return "";
  }

  const parts = clean.match(/(\d{1,2})\.(\d{1,2})(?:\.(\d{2,4}))?/);
  if (!parts) {
    return "";
  }

  const day = Number(parts[1]);
  const month = Number(parts[2]);
  if (!Number.isFinite(day) || !Number.isFinite(month) || day < 1 || day > 31 || month < 1 || month > 12) {
    return "";
  }

  function buildCandidate(yearNumber) {
    const candidate = new Date(yearNumber, month - 1, day);
    if (
      candidate.getFullYear() !== yearNumber ||
      candidate.getMonth() !== month - 1 ||
      candidate.getDate() !== day
    ) {
      return null;
    }
    return candidate;
  }

  if (parts[3]) {
    let explicitYear = Number(parts[3]);
    if (!Number.isFinite(explicitYear)) {
      return "";
    }
    if (explicitYear < 100) {
      explicitYear += 2000;
    }
    const explicit = buildCandidate(explicitYear);
    return explicit ? localDateIso(explicit) : "";
  }

  const now = nowDate instanceof Date ? nowDate : new Date();
  const nowMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const years = [now.getFullYear() - 1, now.getFullYear(), now.getFullYear() + 1];
  let best = null;
  let bestDistance = Number.MAX_VALUE;

  for (const year of years) {
    const candidate = buildCandidate(year);
    if (!candidate) {
      continue;
    }
    const distance = Math.abs(candidate.getTime() - nowMidnight.getTime());
    if (distance < bestDistance) {
      bestDistance = distance;
      best = candidate;
    }
  }

  return best ? localDateIso(best) : "";
}

function parseAntellMeta(htmlText, nowDate) {
  const raw = String(htmlText || "");
  const dateMatch = raw.match(/<div class="menu-date">([\s\S]*?)<\/div>/i);
  const menuDateText = stripHtmlText(dateMatch ? dateMatch[1] : "");
  const menuDateIso = parseAntellMenuDateIso(menuDateText, nowDate);
  const expectedIso = localDateIso(nowDate instanceof Date ? nowDate : new Date());
  return {
    menuDateText,
    menuDateIso,
    providerDateValid: !!menuDateIso && menuDateIso === expectedIso,
  };
}

function parseRssTagRaw(xmlText, tagName) {
  const regex = new RegExp(`<${tagName}(?:\\s+[^>]*)?>([\\s\\S]*?)<\\/${tagName}>`, "i");
  const match = String(xmlText || "").match(regex);
  return match ? String(match[1] || "") : "";
}

function parseRssDateIso(dateText) {
  const clean = normalizeText(dateText);
  if (!clean) {
    return "";
  }

  const parts = clean.match(/(\d{1,2})[-./](\d{1,2})[-./](\d{2,4})/);
  if (!parts) {
    return "";
  }

  const day = Number(parts[1]);
  const month = Number(parts[2]);
  let year = Number(parts[3]);
  if (!Number.isFinite(day) || !Number.isFinite(month) || !Number.isFinite(year)) {
    return "";
  }
  if (year < 100) {
    year += 2000;
  }
  if (day < 1 || day > 31 || month < 1 || month > 12) {
    return "";
  }

  const candidate = new Date(year, month - 1, day);
  if (candidate.getFullYear() !== year || candidate.getMonth() !== month - 1 || candidate.getDate() !== day) {
    return "";
  }
  return localDateIso(candidate);
}

function isRssAllergenToken(token) {
  const clean = normalizeText(token).replace(/[.;:]+$/, "");
  if (!clean) {
    return false;
  }
  if (clean === "*") {
    return true;
  }
  if (/^[A-Z]$/.test(clean)) {
    return true;
  }
  const upper = clean.toUpperCase();
  if (upper === "VEG" || upper === "VS" || upper === "ILM") {
    return true;
  }
  return false;
}

function normalizeRssAllergenToken(token) {
  const clean = normalizeText(token).replace(/[.;:]+$/, "");
  if (!clean) {
    return "";
  }
  if (clean === "*") {
    return "*";
  }
  const upper = clean.toUpperCase();
  if (upper === "VEG") {
    return "Veg";
  }
  return upper;
}

function normalizeRssComponentLine(rawLine) {
  const line = normalizeText(rawLine);
  if (!line) {
    return "";
  }

  if (/\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\)\s*$/.test(line)) {
    return line;
  }

  const compact = line.replace(/\s*[;,]\s*$/, "");
  const parts = compact.split(/\s*,\s*/);
  if (parts.length < 2) {
    return compact;
  }

  const suffixTokens = [];
  for (let i = parts.length - 1; i >= 0; i -= 1) {
    const candidate = normalizeText(parts[i]);
    if (!isRssAllergenToken(candidate)) {
      break;
    }
    const normalized = normalizeRssAllergenToken(candidate);
    if (!normalized) {
      break;
    }
    suffixTokens.unshift(normalized);
  }

  if (suffixTokens.length === 0) {
    return compact;
  }

  const mainParts = parts.slice(0, parts.length - suffixTokens.length);
  let mainText = normalizeText(mainParts.join(", "));
  if (!mainText) {
    return compact;
  }

  const starMatch = mainText.match(/^(.*\S)\s*\*$/);
  if (starMatch) {
    mainText = normalizeText(starMatch[1]);
    suffixTokens.unshift("*");
  }

  while (true) {
    const trailingMatch = mainText.match(/^(.*\S)\s+([A-Za-z*]{1,4})$/);
    if (!trailingMatch) {
      break;
    }
    const trailingToken = normalizeRssAllergenToken(trailingMatch[2]);
    if (!isRssAllergenToken(trailingMatch[2]) || !trailingToken) {
      break;
    }
    mainText = normalizeText(trailingMatch[1]);
    suffixTokens.unshift(trailingToken);
  }

  return `${mainText} (${suffixTokens.join(", ")})`;
}

function parseRssComponents(descriptionRaw) {
  const decoded = decodeHtmlEntities(String(descriptionRaw || ""));
  const components = [];
  const paragraphRegex = /<p[^>]*>([\s\S]*?)<\/p>/gi;
  let paragraphMatch;

  while ((paragraphMatch = paragraphRegex.exec(decoded)) !== null) {
    const line = normalizeRssComponentLine(stripHtmlText(paragraphMatch[1]));
    if (line) {
      components.push(line);
    }
  }

  if (components.length === 0) {
    const fallback = normalizeRssComponentLine(stripHtmlText(decoded));
    if (fallback) {
      components.push(fallback);
    }
  }

  return components;
}

function parseRssMeta(rssText, nowDate) {
  const raw = String(rssText || "");
  const channelRaw = parseRssTagRaw(raw, "channel");
  const itemMatch = String(channelRaw || raw).match(/<item\b[^>]*>([\s\S]*?)<\/item>/i);
  const itemRaw = itemMatch ? String(itemMatch[1] || "") : "";

  const itemTitle = stripHtmlText(parseRssTagRaw(itemRaw, "title"));
  const itemGuid = stripHtmlText(parseRssTagRaw(itemRaw, "guid"));
  const itemLink = stripHtmlText(parseRssTagRaw(itemRaw, "link"));
  const descriptionRaw = parseRssTagRaw(itemRaw, "description");
  const menuDateIso = parseRssDateIso(itemTitle) || parseRssDateIso(itemGuid);
  const expectedIso = localDateIso(nowDate instanceof Date ? nowDate : new Date());

  return {
    itemTitle,
    itemGuid,
    itemLink,
    menuDateIso,
    providerDateValid: !!menuDateIso && menuDateIso === expectedIso,
    components: parseRssComponents(descriptionRaw),
  };
}

function localizedField(value, language = "fi") {
  if (value === null || value === undefined) {
    return "";
  }

  const primitiveType = typeof value;
  if (primitiveType === "string" || primitiveType === "number" || primitiveType === "boolean") {
    return normalizeText(value);
  }

  if (primitiveType !== "object") {
    return "";
  }

  const preferredKeys = [language, "fi", "en"];
  for (const key of preferredKeys) {
    if (!Object.prototype.hasOwnProperty.call(value, key)) {
      continue;
    }
    const candidate = normalizeText(value[key]);
    if (candidate) {
      return candidate;
    }
  }

  for (const dynamicKey of Object.keys(value)) {
    const fallback = normalizeText(value[dynamicKey]);
    if (fallback) {
      return fallback;
    }
  }

  return "";
}

function normalizeHuomenAllergenToken(token) {
  const clean = normalizeText(token);
  if (!clean) {
    return "";
  }
  if (clean === "*") {
    return "*";
  }

  const upper = clean.toUpperCase();
  if (upper === "VEG") {
    return "Veg";
  }
  if (/^[A-Z]{1,8}$/.test(upper)) {
    return upper;
  }

  return clean;
}

function huomenLunchLine(lunch, language = "fi") {
  const title = localizedField(lunch && lunch.title, language);
  if (!title) {
    return "";
  }

  const description = localizedField(lunch && lunch.description, language);
  let line = title;
  if (description && description !== title) {
    line += ` - ${description}`;
  }

  const allergens = [];
  const seen = new Set();
  const rawAllergens = Array.isArray(lunch && lunch.allergens) ? lunch.allergens : [];
  for (const rawAllergen of rawAllergens) {
    const token = normalizeHuomenAllergenToken(localizedField(rawAllergen && rawAllergen.abbreviation, language));
    if (!token) {
      continue;
    }
    const key = token.toUpperCase();
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    allergens.push(token);
  }

  if (allergens.length > 0) {
    line += ` (${allergens.join(", ")})`;
  }

  return normalizeText(line);
}

function parseHuomenToday(payload, targetDate, language = "fi") {
  if (!payload || payload.success === false || !payload.data || !payload.data.week || !Array.isArray(payload.data.week.days)) {
    return null;
  }

  const days = payload.data.week.days;
  const dayMatch = days.find((day) => normalizeText(day && day.dateString) === targetDate) || null;
  const providerDateValid = !!dayMatch;
  const menuDateIso = providerDateValid ? targetDate : "";
  const lines = [];

  if (dayMatch && !dayMatch.isClosed) {
    const lunches = Array.isArray(dayMatch.lunches) ? dayMatch.lunches : [];
    for (const lunch of lunches) {
      const line = huomenLunchLine(lunch, language);
      if (line) {
        lines.push(line);
      }
    }
  }

  return {
    providerDateValid,
    menuDateIso,
    lines,
    restaurantName: localizedField(payload.data.location && payload.data.location.name, language),
  };
}

function retryDelayMinutes(failureCount) {
  const count = Math.max(1, Number(failureCount) || 1);
  if (count <= 1) {
    return 5;
  }
  if (count === 2) {
    return 10;
  }
  return 15;
}

function shouldAssumeWeekendNoMenu(provider, nowDate) {
  const date = nowDate instanceof Date ? nowDate : new Date();
  const day = date.getDay();
  const isWeekend = day === 0 || day === 6;
  return isWeekend && (provider === "antell" || provider === "huomen-json");
}

function isHardWeekendClosedProvider(provider) {
  return provider === "pranzeria";
}

function parsePranzeriaDayHeader(lineText) {
  const clean = normalizeText(lineText);
  const match = clean.match(
    /^(Maanantai|Tiistai|Keskiviikko|Torstai|Perjantai|Lauantai|Sunnuntai)\s+(\d{1,2}\.\d{1,2}\.\d{2,4})(?:\s+(.+))?$/i
  );
  if (!match) {
    return null;
  }

  const dateIso = parseAntellMenuDateIso(match[2], new Date(2026, 2, 20));
  if (!dateIso) {
    return null;
  }

  return {
    dateIso,
    trailing: normalizeText(match[3]),
  };
}

function isPranzeriaLegendLine(lineText) {
  const clean = normalizeText(lineText);
  if (!clean) {
    return false;
  }

  if (/^(?:L|G|M|V|VG)\s*=/.test(clean)) {
    return true;
  }

  return (
    clean.includes("Laktoositon") ||
    clean.includes("Gluteeniton") ||
    clean.includes("Maidoton") ||
    clean.includes("Kasvis") ||
    clean.includes("Vegaani")
  );
}

function parsePranzeriaDayLines(htmlText, targetDateIso) {
  const html = String(htmlText || "");
  const paragraphRegex = /<p\b[^>]*>([\s\S]*?)<\/p>/gi;
  const linesByDate = {};
  let currentDateIso = "";
  let match;

  while ((match = paragraphRegex.exec(html)) !== null) {
    const line = stripHtmlText(match[1]);
    if (!line) {
      continue;
    }

    const header = parsePranzeriaDayHeader(line);
    if (header) {
      currentDateIso = header.dateIso;
      if (!Object.prototype.hasOwnProperty.call(linesByDate, currentDateIso)) {
        linesByDate[currentDateIso] = [];
      }
      if (header.trailing) {
        linesByDate[currentDateIso].push(header.trailing);
      }
      continue;
    }

    if (!currentDateIso) {
      continue;
    }

    if (isPranzeriaLegendLine(line)) {
      break;
    }

    linesByDate[currentDateIso].push(line);
  }

  const providerDateValid = Object.prototype.hasOwnProperty.call(linesByDate, targetDateIso);
  const rawLines = providerDateValid ? linesByDate[targetDateIso] : [];
  const lines = [];
  for (const raw of rawLines) {
    const clean = normalizeText(raw);
    if (!clean) {
      continue;
    }
    if (lines.length > 0 && lines[lines.length - 1] === clean) {
      continue;
    }
    lines.push(clean);
  }

  return {
    providerDateValid,
    menuDateIso: providerDateValid ? targetDateIso : "",
    lines,
  };
}

function checkCompassFixture(name, expectedMenuName) {
  const payload = readFixture(name);

  assert(normalizeText(payload.RestaurantName).length > 0, `${name}: missing RestaurantName`);
  assert(Array.isArray(payload.MenusForDays), `${name}: MenusForDays is not an array`);
  assert(payload.MenusForDays.length > 0, `${name}: MenusForDays is empty`);

  const fresh = normalizeCompassToday(payload, "2026-02-19");
  assert(fresh && fresh.providerDateValid, `${name}: expected providerDateValid on 2026-02-19`);
  assert(fresh.menuDateIso === "2026-02-19", `${name}: unexpected menuDateIso: ${fresh.menuDateIso}`);
  assert(fresh.todayMenu, `${name}: expected todayMenu on 2026-02-19`);
  assert(fresh.todayMenu.lunchTime === "10:30–14:30", `${name}: unexpected lunch time: ${fresh.todayMenu.lunchTime}`);
  assert(fresh.todayMenu.menus.length > 0, `${name}: no menus on 2026-02-19`);
  assert(fresh.todayMenu.menus[0].name === expectedMenuName, `${name}: first menu mismatch: ${fresh.todayMenu.menus[0].name}`);

  for (const menu of fresh.todayMenu.menus) {
    for (const component of menu.components) {
      assert(!component.includes("\n"), `${name}: newline remained in component: ${component}`);
    }
  }

  const closedDay = normalizeCompassToday(payload, "2026-02-22");
  assert(closedDay && closedDay.providerDateValid, `${name}: 2026-02-22 should still be a valid day`);
  assert(closedDay.todayMenu, `${name}: expected closed-day todayMenu object`);
  assert(closedDay.todayMenu.menus.length === 0, `${name}: expected no menus on 2026-02-22`);
  assert(closedDay.todayMenu.lunchTime === "", `${name}: expected empty lunchTime on 2026-02-22`);

  const staleDay = normalizeCompassToday(payload, "2026-02-23");
  assert(staleDay && !staleDay.providerDateValid, `${name}: expected stale when day is missing`);
  assert(staleDay.todayMenu === null, `${name}: expected null todayMenu for missing day`);
  assert(staleDay.menuDateIso === "", `${name}: expected empty menuDateIso when day missing`);
}

function checkAntellFixture(name, expectedFirstTitle, expectedFirstItem, expectedSections) {
  const html = readTextFixture(name);
  const sections = parseAntellSections(html);

  assert(sections.length === expectedSections, `${name}: expected ${expectedSections} parsed sections, got ${sections.length}`);
  assert(sections[0].title === expectedFirstTitle, `${name}: unexpected first title: ${sections[0].title}`);
  assert(sections[0].items[0] === expectedFirstItem, `${name}: unexpected first item: ${sections[0].items[0]}`);

  for (const section of sections) {
    for (const item of section.items) {
      assert(item.length > 0, `${name}: empty parsed item`);
    }
  }

  const matchingDate = new Date(2026, 1, 20);
  const validMeta = parseAntellMeta(html, matchingDate);
  assert(validMeta.menuDateText.length > 0, `${name}: missing parsed menu-date text`);
  assert(validMeta.menuDateIso === "2026-02-20", `${name}: expected parsed menu date 2026-02-20`);
  assert(validMeta.providerDateValid, `${name}: expected providerDateValid on matching local date`);

  const mismatchMeta = parseAntellMeta(html, new Date(2026, 1, 21));
  assert(!mismatchMeta.providerDateValid, `${name}: expected mismatch on non-matching date`);

  const missingDateHtml = html.replace(/<div class="menu-date">[\s\S]*?<\/div>/i, "");
  const missingMeta = parseAntellMeta(missingDateHtml, matchingDate);
  assert(missingMeta.menuDateIso === "", `${name}: missing menu-date should produce empty ISO`);
  assert(!missingMeta.providerDateValid, `${name}: missing menu-date should be invalid`);
}

function checkRssFixture(name) {
  const rss = readTextFixture(name);
  const todayMeta = parseRssMeta(rss, new Date(2026, 1, 23));
  assert(todayMeta.providerDateValid, `${name}: expected valid date on 2026-02-23`);
  assert(todayMeta.menuDateIso === "2026-02-23", `${name}: unexpected date: ${todayMeta.menuDateIso}`);
  assert(todayMeta.itemLink.includes("cafe-snellari"), `${name}: missing restaurant link`);
  assert(todayMeta.components.length >= 4, `${name}: expected at least 4 menu lines`);
  assert(
    todayMeta.components[0] === "Juustoista peruna-pinaattisosekeittoa (*, A, G, ILM, L)",
    `${name}: unexpected first line: ${todayMeta.components[0]}`
  );
  assert(
    todayMeta.components[1] === "Basilikalla ja hunajalla maustettua broileria (G, L, M)",
    `${name}: unexpected second line: ${todayMeta.components[1]}`
  );
  assert(
    todayMeta.components.some((line) => line.includes("katkarapuja")),
    `${name}: expected katkarapuja line in components`
  );

  const staleMeta = parseRssMeta(rss, new Date(2026, 1, 24));
  assert(!staleMeta.providerDateValid, `${name}: expected stale date on 2026-02-24`);

  const noDateRss = rss
    .replace(/#23-02-2026/i, "#no-date")
    .replace(/Maanantai,\s*23-02-2026/i, "Maanantai");
  const missingDateMeta = parseRssMeta(noDateRss, new Date(2026, 1, 23));
  assert(missingDateMeta.menuDateIso === "", `${name}: expected empty date when RSS has no date`);
  assert(!missingDateMeta.providerDateValid, `${name}: expected invalid providerDateValid when date is missing`);
}

function checkHuomenFixture(name) {
  const payload = readFixture(name);
  const today = parseHuomenToday(payload, "2026-02-23", "fi");
  assert(today, `${name}: parseHuomenToday returned null`);
  assert(today.providerDateValid, `${name}: expected providerDateValid on 2026-02-23`);
  assert(today.menuDateIso === "2026-02-23", `${name}: unexpected menuDateIso: ${today.menuDateIso}`);
  assert(today.restaurantName === "Hyvä Huomen Bioteknia", `${name}: unexpected location name: ${today.restaurantName}`);
  assert(today.lines.length === 3, `${name}: expected 3 lunches for 2026-02-23, got ${today.lines.length}`);
  assert(
    today.lines[0] === "Kermainen juuresosekeitto (G, L)",
    `${name}: unexpected first lunch line: ${today.lines[0]}`
  );
  assert(
    today.lines[1].includes("(G, L)"),
    `${name}: expected allergens in second lunch line: ${today.lines[1]}`
  );
  assert(
    today.lines[2] === "Kasvispihvejä, tsatsikia (L)",
    `${name}: unexpected third lunch line: ${today.lines[2]}`
  );

  const stale = parseHuomenToday(payload, "2026-03-03", "fi");
  assert(stale && !stale.providerDateValid, `${name}: expected stale for missing date`);
  assert(stale.menuDateIso === "", `${name}: expected empty menuDateIso for missing date`);
  assert(stale.lines.length === 0, `${name}: expected no lines for missing date`);
}

function checkPranzeriaFixture(name) {
  const html = readTextFixture(name);
  const friday = parsePranzeriaDayLines(html, "2026-03-20");
  assert(friday.providerDateValid, `${name}: expected providerDateValid on 2026-03-20`);
  assert(friday.menuDateIso === "2026-03-20", `${name}: unexpected menuDateIso: ${friday.menuDateIso}`);
  assert(friday.lines.length >= 5, `${name}: expected at least 5 lines on Friday, got ${friday.lines.length}`);
  assert(
    friday.lines[0] === "Salaatti- &AntipastoBuffet",
    `${name}: expected trailing day-header line as first entry`
  );
  assert(
    friday.lines.some((line) => line.includes("Spezzatino Di Manzo")),
    `${name}: expected Spezzatino line for Friday`
  );
  assert(
    friday.lines.some((line) => line.includes("Roomalainen focacciapizzabuffet")),
    `${name}: expected focacciapizzabuffet line for Friday`
  );
  assert(!friday.lines.some((line) => line.includes("Laktoositon")), `${name}: legend lines should be excluded`);

  const stale = parsePranzeriaDayLines(html, "2026-03-22");
  assert(!stale.providerDateValid, `${name}: expected stale on missing Sunday date`);
  assert(stale.lines.length === 0, `${name}: expected no Sunday lines`);
}

function checkRetryDelays() {
  assert(retryDelayMinutes(1) === 5, "retry delay for first failure should be 5");
  assert(retryDelayMinutes(2) === 10, "retry delay for second failure should be 10");
  assert(retryDelayMinutes(3) === 15, "retry delay for third failure should be 15");
  assert(retryDelayMinutes(8) === 15, "retry delay should stay at 15 after third failure");
}

function checkWeekendNoMenuAssumption() {
  const saturday = new Date(2026, 2, 14);
  const monday = new Date(2026, 2, 16);

  assert(shouldAssumeWeekendNoMenu("antell", saturday), "antell should assume no-menu on weekend mismatch");
  assert(shouldAssumeWeekendNoMenu("huomen-json", saturday), "huomen-json should assume no-menu on weekend mismatch");
  assert(!shouldAssumeWeekendNoMenu("antell", monday), "antell should use normal retry flow on weekdays");
  assert(!shouldAssumeWeekendNoMenu("huomen-json", monday), "huomen-json should use normal retry flow on weekdays");
  assert(!shouldAssumeWeekendNoMenu("pranzeria", saturday), "pranzeria should not use retry-based weekend assumption");
  assert(isHardWeekendClosedProvider("pranzeria"), "pranzeria should be hard weekend-closed");
  assert(!shouldAssumeWeekendNoMenu("compass", saturday), "compass should not use weekend no-menu assumption");
  assert(!shouldAssumeWeekendNoMenu("compass-rss", saturday), "compass-rss should not use weekend no-menu assumption");
}

function main() {
  checkCompassFixture("output-en.json", "Lunch");
  checkCompassFixture("output-fi.json", "Annosruoka");
  checkAntellFixture(
    "antell-highway-friday-snippet.html",
    "Pääruoaksi",
    "Hoisin-kastikkeella maustettuja nyhtöpossuhodareita (A, L, M)",
    3
  );
  checkAntellFixture(
    "antell-round-friday-snippet.html",
    "Kotiruokalounas",
    "Perinteiset lihapyörykät mummonkastikkeella(G oma)",
    3
  );
  checkRssFixture("snellari.rss");
  checkHuomenFixture("huomen.json");
  checkPranzeriaFixture("pranzeria-snippet.html");
  checkWeekendNoMenuAssumption();
  checkRetryDelays();
  process.stdout.write("Parser checks passed for Compass, Antell, RSS, Huomen and Pranzeria freshness rules\n");
}

main();
