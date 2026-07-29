import { normalizeText, stableResponseId } from "../normalize";
import type {
  GeneralOffer,
  Language,
  LunchItem
} from "../types";
import {
  fetchOrDefault,
  responseText,
  type ParsedProviderMenu,
  type ProviderRequest
} from "./provider";
import {
  extractGeneralOffers,
  normalizeOfferAmount,
  type OfferDefinition
} from "./offers";

const huomenOffers: OfferDefinition[] = [
  {
    id: "soup-lunch",
    labels: { fi: "Keittolounas", en: "Soup lunch" },
    patterns: ["Keittolounas", "Soup lunch"]
  },
  {
    id: "lunch",
    labels: { fi: "Lounas", en: "Lunch" },
    patterns: ["Lounas", "Lunch"]
  }
];

export async function fetchHuomenMenu(
  request: ProviderRequest
): Promise<ParsedProviderMenu> {
  if (request.restaurant.source.type !== "huomen") {
    throw new Error("Invalid Hyvä Huomen configuration");
  }
  const fetcher = fetchOrDefault(request.fetcher);
  const language = request.restaurant.languages.includes(request.language)
    ? request.language
    : (request.restaurant.languages[0] ?? "fi");
  const endpoint = new URL(request.restaurant.source.url);
  endpoint.searchParams.set("language", language);
  const [jsonText, pageText] = await Promise.all([
    responseText(
      await fetcher(endpoint, { headers: { Accept: "application/json" } })
    ),
    request.restaurant.websiteUrl
      ? fetcher(request.restaurant.websiteUrl, {
          headers: { Accept: "text/html" }
        })
          .then(responseText)
          .catch(() => "")
      : Promise.resolve("")
  ]);
  return parseHuomen(jsonText, pageText, language, request.date);
}

export function parseHuomen(
  jsonText: string,
  pageHtml: string,
  contentLanguage: Language,
  date: string
): ParsedProviderMenu {
  const root = JSON.parse(jsonText) as Record<string, unknown>;
  if (root.success === false) {
    throw new Error(`Hyvä Huomen: ${localized(root.message, contentLanguage) || "upstream error"}`);
  }
  const data = root.data as Record<string, unknown> | undefined;
  const week = data?.week as Record<string, unknown> | undefined;
  const days = Array.isArray(week?.days) ? week.days : [];
  const rawDay = days.find(candidate => {
    const day = candidate as Record<string, unknown>;
    return normalizeText(day.dateString) === date;
  }) as Record<string, unknown> | undefined;
  if (!rawDay) {
    return {
      contentLanguage,
      status: "noMenu",
      offers: [],
      groups: []
    };
  }
  if (rawDay.isClosed === true) {
    return {
      contentLanguage,
      status: "closed",
      offers: [],
      groups: []
    };
  }

  const lunches = Array.isArray(rawDay.lunches) ? rawDay.lunches : [];
  const items = lunches
    .map((candidate, index) =>
      huomenItem(
        candidate as Record<string, unknown>,
        contentLanguage,
        index
      )
    )
    .filter((candidate): candidate is LunchItem => candidate !== undefined);
  let offers = extractGeneralOffers(pageHtml, contentLanguage, huomenOffers);
  if (!offers.length) {
    offers = inferHuomenOffers(lunches, contentLanguage);
  }

  return {
    contentLanguage,
    status: items.length ? "serving" : "noMenu",
    offers,
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

function huomenItem(
  raw: Record<string, unknown>,
  language: Language,
  index: number
): LunchItem | undefined {
  const name = localized(raw.title, language);
  if (!name) return undefined;
  const description = localized(raw.description, language);
  const allergens = Array.isArray(raw.allergens) ? raw.allergens : [];
  const tags = unique(
    allergens
      .map(candidate =>
        localized(
          (candidate as Record<string, unknown>).abbreviation,
          language
        )
      )
      .map(normalizeDiet)
      .filter(Boolean)
  );
  const rawTags = Array.isArray(raw.tags) ? raw.tags : [];
  const notes = unique(
    rawTags
      .map(candidate =>
        localized(
          (candidate as Record<string, unknown>).description,
          language
        )
      )
      .filter(Boolean)
  );
  return {
    id: stableResponseId("item", index),
    name,
    ...(description && description !== name ? { description } : {}),
    ...(tags.length ? { tags } : {}),
    ...(notes.length ? { notes } : {})
  };
}

function inferHuomenOffers(
  lunches: unknown[],
  language: Language
): GeneralOffer[] {
  const byId = new Map<string, GeneralOffer>();
  for (const candidate of lunches) {
    const lunch = candidate as Record<string, unknown>;
    const normalPrice = lunch.normalPrice as Record<string, unknown> | undefined;
    const amount = normalizeOfferAmount(normalPrice?.price);
    if (!amount) continue;
    const title = localized(lunch.title, language).toLocaleLowerCase(language);
    const soup = title.includes("keitto") || title.includes("soup");
    const definition = huomenOffers.find(entry =>
      soup ? entry.id === "soup-lunch" : entry.id === "lunch"
    );
    if (!definition) continue;
    byId.set(definition.id, {
      id: definition.id,
      label: definition.labels[language],
      price: { amount, currency: "EUR" }
    });
  }
  return huomenOffers
    .map(definition => byId.get(definition.id))
    .filter((offer): offer is GeneralOffer => offer !== undefined);
}

function localized(value: unknown, language: Language): string {
  if (typeof value === "string" || typeof value === "number") {
    return normalizeText(String(value));
  }
  if (Array.isArray(value)) {
    for (const candidate of value) {
      const result = localized(candidate, language);
      if (result) return result;
    }
    return "";
  }
  if (value && typeof value === "object") {
    const values = value as Record<string, unknown>;
    for (const key of [language, "fi", "en"]) {
      const result = localized(values[key], language);
      if (result) return result;
    }
    for (const candidate of Object.values(values)) {
      const result = localized(candidate, language);
      if (result) return result;
    }
  }
  return "";
}

function normalizeDiet(value: string): string {
  const upper = normalizeText(value).toUpperCase();
  return upper === "VEG" ? "Veg" : upper;
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}
