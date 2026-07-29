import {
  htmlText,
  itemFromText,
  normalizeText,
  stableResponseId
} from "../normalize";
import type { Language, LunchItem } from "../types";
import { parseDateNear } from "./dates";
import {
  extractGeneralOffers,
  type OfferDefinition
} from "./offers";
import {
  fetchOrDefault,
  responseText,
  type ParsedProviderMenu,
  type ProviderRequest
} from "./provider";

const weekdays =
  "maanantai|tiistai|keskiviikko|torstai|perjantai|lauantai|sunnuntai|" +
  "monday|tuesday|wednesday|thursday|friday|saturday|sunday";
const datePattern =
  "(?:\\d{4}[-/]\\d{1,2}[-/]\\d{1,2}|\\d{1,2}[./-]\\d{1,2}(?:[./-]\\d{2,4})?\\.?)";

const sorrentoOffers: OfferDefinition[] = [
  {
    id: "salad-lunch",
    labels: { fi: "Salaattilounas", en: "Salad lunch" },
    patterns: ["Salaattilounas"]
  },
  {
    id: "lunch-buffet",
    labels: { fi: "Lounasbuffet", en: "Lunch buffet" },
    patterns: ["Lounasbuffet"]
  },
  {
    id: "contract-lunch",
    labels: { fi: "Sopimuslounas", en: "Contract lunch" },
    patterns: ["Sopimuslounas"]
  }
];

export async function fetchSorrentoMenu(
  request: ProviderRequest
): Promise<ParsedProviderMenu> {
  if (request.restaurant.source.type !== "pranzeria") {
    throw new Error("Invalid Sorrento configuration");
  }
  const language = request.restaurant.languages.includes(request.language)
    ? request.language
    : (request.restaurant.languages[0] ?? "fi");
  const html = await responseText(
    await fetchOrDefault(request.fetcher)(request.restaurant.source.url, {
      headers: { Accept: "text/html" }
    })
  );
  return parseSorrento(html, language, request.date);
}

export function parseSorrento(
  html: string,
  contentLanguage: Language,
  date: string
): ParsedProviderMenu {
  const hours = extractSorrentoHours(html);
  const linesByDate = new Map<string, string[]>();
  let activeDate: string | undefined;
  const blockExpression =
    /<(?:p|h[1-6]|li)\b[^>]*>([\s\S]*?)<\/(?:p|h[1-6]|li)>/gi;

  for (const block of html.matchAll(blockExpression)) {
    const lines = (block[1] ?? "")
      .split(/<br\s*\/?>/gi)
      .map(htmlText)
      .filter(Boolean);
    for (const line of lines) {
      if (containsOffer(line)) continue;
      const header = parseDayHeader(line, date);
      if (header) {
        activeDate = header.date;
        if (!linesByDate.has(header.date)) linesByDate.set(header.date, []);
        if (header.trailing) linesByDate.get(header.date)?.push(header.trailing);
        continue;
      }
      if (isLegend(line)) {
        activeDate = undefined;
        continue;
      }
      if (activeDate) linesByDate.get(activeDate)?.push(line);
    }
  }

  const rawLines = normalizeSorrentoLines(linesByDate.get(date) ?? []);
  const items = rawLines
    .map((line, index) =>
      itemFromText(line, stableResponseId("item", index))
    )
    .filter((candidate): candidate is LunchItem => candidate !== undefined);

  return {
    contentLanguage,
    status: linesByDate.has(date)
      ? items.length
        ? "serving"
        : "noMenu"
      : "noMenu",
    ...(hours ? { hours } : {}),
    offers: extractGeneralOffers(html, contentLanguage, sorrentoOffers),
    groups: items.length
      ? [
          {
            id: "group-1",
            prices: [],
            items,
            sortOrder: 0
          }
        ]
      : []
  };
}

function extractSorrentoHours(html: string): string | undefined {
  const match = htmlText(html).match(
    /\bma\s*[-–—]\s*pe(?:\s+klo)?\s*(\d{1,2})[.:](\d{2})\s*[-–—]\s*(\d{1,2})[.:](\d{2})\b/i
  );
  if (!match) return undefined;

  const startHour = Number(match[1]);
  const startMinute = Number(match[2]);
  const endHour = Number(match[3]);
  const endMinute = Number(match[4]);
  if (
    startHour > 23 ||
    endHour > 23 ||
    startMinute > 59 ||
    endMinute > 59 ||
    startHour * 60 + startMinute >= endHour * 60 + endMinute
  ) {
    return undefined;
  }

  const time = (hour: number, minute: number) =>
    `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  return `${time(startHour, startMinute)}–${time(endHour, endMinute)}`;
}

function parseDayHeader(
  value: string,
  referenceIso: string
): { date: string; trailing: string } | undefined {
  const clean = normalizeText(value);
  const expressions = [
    new RegExp(`^(?:${weekdays})\\s+(${datePattern})(.*)$`, "i"),
    new RegExp(`^(${datePattern})(.*)$`, "i")
  ];
  for (const expression of expressions) {
    const match = clean.match(expression);
    if (!match?.[1]) continue;
    const rest = normalizeText(match[2]).replace(/^[\s:,\-–|/]+|[\s:,\-–|/]+$/g, "");
    if (
      !match[1].includes("/") &&
      (match[1].match(/\./g)?.length ?? 0) <= 1 &&
      /^-\s*\d{1,2}[.:]\d{2}/.test(normalizeText(match[2]))
    ) {
      continue;
    }
    const parsed = parseDateNear(
      match[1],
      referenceIso,
      match[1].match(/\d{4}/) ? undefined : 14
    );
    if (parsed) return { date: parsed, trailing: rest };
  }
  return undefined;
}

function containsOffer(value: string): boolean {
  return /\b(?:SALAATTILOUNAS|LOUNASBUFFET|SOPIMUSLOUNAS)\b[^0-9]{0,30}\d{1,3}[,.]\d{1,3}\s*(?:€|EUR)/i.test(
    value
  );
}

function isLegend(value: string): boolean {
  return (
    /^(?:L|G|M|V|VG)\s*=/.test(value) ||
    ["Laktoositon", "Gluteeniton", "Maidoton", "Kasvis", "Vegaani"].some(
      token => value.includes(token)
    )
  );
}

function normalizeSorrentoLines(values: string[]): string[] {
  const result: string[] = [];
  for (const raw of values) {
    const clean = normalizeText(raw).replace(
      /pyydet(?:t|)äessä\s+G/gi,
      "G"
    );
    for (const line of splitFusedItems(clean)) {
      if (line && result[result.length - 1] !== line) result.push(line);
    }
  }
  return result;
}

function splitFusedItems(value: string): string[] {
  const starts =
    /\b(?:Pasta|Pollo|Manzo|Maiale|Salmone|Gnocchi|Cotoletta|Porco|Spezzatino|Lasagne|Risotto|Ravioli|Tagliatelle|Spaghetti|Fusilli|Penne|Rigatoni)\b/g;
  const matches = [...value.matchAll(starts)];
  if (matches.length < 2) return value ? [value] : [];
  const result: string[] = [];
  let start = 0;
  for (const match of matches.slice(1)) {
    const index = match.index ?? 0;
    const before = value.slice(start, index).trimEnd();
    if (/(?:\b(?:G|L|M|V|VG)\b|pyydettäessä\s+G)$/i.test(before)) {
      result.push(normalizeText(before));
      start = index;
    }
  }
  result.push(normalizeText(value.slice(start)));
  return result.filter(Boolean);
}
