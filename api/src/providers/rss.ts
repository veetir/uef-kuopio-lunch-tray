import {
  decodeHtml,
  htmlText,
  itemFromText,
  normalizeText,
  stableResponseId
} from "../normalize";
import type { LunchItem } from "../types";
import {
  fetchOrDefault,
  responseText,
  type ParsedProviderMenu,
  type ProviderRequest
} from "./provider";

export async function fetchCompassRssMenu(
  request: ProviderRequest
): Promise<ParsedProviderMenu> {
  if (request.restaurant.source.type !== "compassRss") {
    throw new Error("Invalid Compass RSS configuration");
  }
  const language = request.restaurant.languages.includes(request.language)
    ? request.language
    : (request.restaurant.languages[0] ?? "fi");
  const endpoint = new URL(
    "https://www.compass-group.fi/menuapi/feed/rss/current-day"
  );
  endpoint.searchParams.set(
    "costNumber",
    request.restaurant.source.costNumber
  );
  endpoint.searchParams.set("language", language);
  const xml = await responseText(
    await fetchOrDefault(request.fetcher)(endpoint, {
      headers: { Accept: "application/rss+xml, application/xml" }
    })
  );
  return parseCompassRss(xml, language, request.date);
}

export function parseCompassRss(
  xml: string,
  contentLanguage: "fi" | "en",
  date: string
): ParsedProviderMenu {
  const item = capture(xml, /<item\b[^>]*>([\s\S]*?)<\/item>/i) ?? "";
  const title = htmlText(capture(item, /<title\b[^>]*>([\s\S]*?)<\/title>/i) ?? "");
  const guid = htmlText(capture(item, /<guid\b[^>]*>([\s\S]*?)<\/guid>/i) ?? "");
  const payloadDate = parseDate(title) ?? parseDate(guid);
  if (payloadDate !== date) {
    return {
      contentLanguage,
      status: "noMenu",
      offers: [],
      groups: []
    };
  }

  const description =
    capture(item, /<description\b[^>]*>([\s\S]*?)<\/description>/i) ?? "";
  const decoded = decodeHtml(description);
  const paragraphs = [...decoded.matchAll(/<p\b[^>]*>([\s\S]*?)<\/p>/gi)]
    .map(match => htmlText(match[1] ?? ""))
    .filter(Boolean);
  const rawItems = paragraphs.length ? paragraphs : [htmlText(decoded)];
  const items = rawItems
    .map((line, index) =>
      itemFromText(normalizeRssLine(line), stableResponseId("item", index))
    )
    .filter((candidate): candidate is LunchItem => candidate !== undefined);

  return {
    contentLanguage,
    status: items.length ? "serving" : "noMenu",
    offers: [],
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

function capture(value: string, expression: RegExp): string | undefined {
  return expression.exec(value)?.[1];
}

function parseDate(value: string): string | undefined {
  const match = value.match(/(\d{1,2})[-./](\d{1,2})[-./](\d{2,4})/);
  if (!match) return undefined;
  const year = Number(match[3]) < 100 ? 2000 + Number(match[3]) : Number(match[3]);
  const month = Number(match[2]);
  const day = Number(match[1]);
  if (!validDate(year, month, day)) return undefined;
  return `${String(year).padStart(4, "0")}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

function validDate(year: number, month: number, day: number): boolean {
  const value = new Date(Date.UTC(year, month - 1, day));
  return (
    value.getUTCFullYear() === year &&
    value.getUTCMonth() === month - 1 &&
    value.getUTCDate() === day
  );
}

function normalizeRssLine(rawValue: string): string {
  let value = normalizeText(rawValue).replace(/\s*[;,]\s*$/, "");
  if (!value || /\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\)$/.test(value)) {
    return value;
  }
  const parts = value.split(",").map(normalizeText).filter(Boolean);
  const tags: string[] = [];
  while (
    parts.length > 1 &&
    /^(?:\*|[A-ZÅÄÖ]{1,4}|Veg|VS|ILM)$/i.test(parts[parts.length - 1] ?? "")
  ) {
    tags.unshift(parts.pop() ?? "");
  }
  if (tags.length) {
    value = `${parts.join(", ")} (${tags.join(", ")})`;
  }
  return value;
}
