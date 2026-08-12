import {
  htmlText,
  itemFromText,
  normalizeText,
  stableResponseId,
  validGroups
} from "../normalize";
import type {
  Language,
  LunchItem,
  MenuGroup,
  MenuPrice,
  RecipeDetails
} from "../types";
import { parseDateNear } from "./dates";
import { normalizeHoursRange } from "./hours";
import {
  fetchOrDefault,
  responseText,
  type ParsedProviderMenu,
  type ProviderRequest
} from "./provider";

export async function fetchAntellMenu(
  request: ProviderRequest
): Promise<ParsedProviderMenu> {
  if (request.restaurant.source.type !== "antell") {
    throw new Error("Invalid Antell configuration");
  }
  const fetcher = fetchOrDefault(request.fetcher);
  const language = request.restaurant.languages.includes(request.language)
    ? request.language
    : (request.restaurant.languages[0] ?? "fi");
  const weekday = weekdayToken(request.date);
  const base =
    language === "en" && request.restaurant.id === "antell-round"
      ? `https://antell.fi/en/lunch/kuopio/${request.restaurant.source.slug}/`
      : `https://antell.fi/lounas/kuopio/${request.restaurant.source.slug}/`;
  const print = new URL(base);
  print.searchParams.set("print_lunch_list_day", "1");
  print.searchParams.set(
    "print_lunch_day",
    language === "en" ? `panel-${weekday}` : weekday
  );
  const [printHtml, detailHtml] = await Promise.all([
    responseText(
      await fetcher(print, { headers: { Accept: "text/html" } })
    ),
    fetcher(base, { headers: { Accept: "text/html" } })
      .then(responseText)
      .catch(() => "")
  ]);
  return parseAntell(printHtml, detailHtml, language, request.date, weekday);
}

export function parseAntell(
  printHtml: string,
  detailHtml: string,
  contentLanguage: Language,
  date: string,
  weekday = weekdayToken(date)
): ParsedProviderMenu {
  const menuDateText = htmlText(
    capture(
      printHtml,
      /<div\b[^>]*class=["'][^"']*\bmenu-date\b[^"']*["'][^>]*>([\s\S]*?)<\/div>/i
    ) ?? ""
  );
  const parsedDate = parseDateNear(menuDateText.match(/\d{1,2}\.\d{1,2}(?:\.\d{2,4})?/)?.[0] ?? "", date);
  if (parsedDate !== date) {
    return {
      contentLanguage,
      status: "noMenu",
      offers: [],
      groups: []
    };
  }

  const groups = parseAntellGroups(printHtml);
  const details = parseAntellDetails(detailHtml, weekday);
  const hours = extractAntellLunchHours(detailHtml);
  for (const group of groups) {
    for (const item of group.items) {
      const detail = details.get(mealKey(item.name));
      if (detail) item.recipe = detail;
    }
  }
  return {
    contentLanguage,
    status: groups.length ? "serving" : "noMenu",
    ...(hours ? { hours } : {}),
    offers: [],
    groups
  };
}

function extractAntellLunchHours(html: string): string | undefined {
  for (const heading of html.matchAll(
    /<h[1-6]\b[^>]*>([\s\S]*?)<\/h[1-6]>/gi
  )) {
    const label = htmlText(heading[1] ?? "").toLocaleLowerCase("fi");
    if (label !== "lounas" && label !== "lunch") continue;
    const following = html.slice((heading.index ?? 0) + heading[0].length);
    const nextHeading = following.search(/<h[1-6]\b/i);
    const section = following.slice(
      0,
      nextHeading >= 0 ? nextHeading : 2_000
    );
    const rawHours = htmlText(
      capture(
        section,
        /<span\b[^>]*class=["'][^"']*\bhours\b[^"']*["'][^>]*>([\s\S]*?)<\/span>/i
      ) ?? ""
    );
    return normalizeHoursRange(rawHours);
  }
  return undefined;
}

export function parseAntellGroups(html: string): MenuGroup[] {
  const sections = [
    ...html.matchAll(
      /<section\b[^>]*class=["'][^"']*\bmenu-section\b[^"']*["'][^>]*>([\s\S]*?)<\/section>/gi
    )
  ];
  return validGroups(
    sections.map((sectionMatch, groupIndex) => {
      const section = sectionMatch[1] ?? "";
      const items = [
        ...section.matchAll(/<li\b[^>]*>([\s\S]*?)<\/li>/gi)
      ]
        .map((match, itemIndex) =>
          itemFromText(
            htmlText(match[1] ?? ""),
            stableResponseId(`group-${groupIndex + 1}-item`, itemIndex)
          )
        )
        .filter((candidate): candidate is LunchItem => candidate !== undefined);
      const title = htmlText(
        capture(
          section,
          /<h2\b[^>]*class=["'][^"']*\bmenu-title\b[^"']*["'][^>]*>([\s\S]*?)<\/h2>/i
        ) ?? ""
      );
      const rawPrice = htmlText(
        capture(
          section,
          /<h2\b[^>]*class=["'][^"']*\bmenu-price\b[^"']*["'][^>]*>([\s\S]*?)<\/h2>/i
        ) ?? ""
      );
      return {
        id: stableResponseId("group", groupIndex),
        ...(title && title.toLowerCase() !== "menu" ? { title } : {}),
        prices: parseAntellPrices(rawPrice),
        items,
        sortOrder: groupIndex
      };
    })
  );
}

function parseAntellPrices(value: string): MenuPrice[] {
  const amounts = value.match(/\d+(?:[.,]\d+)?/g) ?? [];
  const normalized = amounts
    .map(amount => Number.parseFloat(amount.replace(",", ".")))
    .filter(amount => Number.isFinite(amount) && amount >= 0 && amount < 1000)
    .map(amount => amount.toFixed(2));
  if (normalized.length === 1 && normalized[0]) {
    return [
      {
        amount: normalized[0],
        currency: "EUR",
        audiences: ["student", "staff", "guest"]
      }
    ];
  }
  if (normalized.length >= 2 && normalized[0] && normalized[1]) {
    const result: MenuPrice[] = [
      {
        amount: normalized[0],
        currency: "EUR",
        audiences: ["staff", "guest"]
      },
      {
        amount: normalized[1],
        currency: "EUR",
        audiences: ["student"]
      }
    ];
    return result;
  }
  return [];
}

function parseAntellDetails(
  html: string,
  weekday: string
): Map<string, RecipeDetails> {
  const panel =
    capture(
      html,
      new RegExp(
        `<section\\b[^>]*id=["']panel-${escapeRegularExpression(
          titleCase(weekday)
        )}["'][^>]*>([\\s\\S]*?)<\\/section>`,
        "i"
      )
    ) ?? html;
  const result = new Map<string, RecipeDetails>();
  for (const match of panel.matchAll(
    /<li\b[^>]*>([\s\S]*?class=["'][^"']*\baccordion__button\b[\s\S]*?)<\/li>/gi
  )) {
    const item = match[1] ?? "";
    const name = htmlText(
      capture(
        item,
        /<button\b[^>]*class=["'][^"']*\baccordion__button\b[^"']*["'][^>]*>([\s\S]*?)<\/button>/i
      ) ?? ""
    );
    if (!name) continue;
    const ingredients =
      htmlText(
        capture(
          item,
          /<div\b[^>]*class=["'][^"']*\btooltip__body\b[^"']*["'][^>]*>([\s\S]*?)<\/div>/i
        ) ?? ""
      ) || labeledParagraph(item, ["Ainesosat", "Ingredients"]);
    if (!ingredients) continue;
    const nutritionLine = labeledParagraph(item, [
      "Ravintoarvot (100 g)",
      "Nutritional values (100 g)"
    ]);
    const co2Line = labeledParagraph(item, [
      "Hiilijalanjälki",
      "Carbon footprint"
    ]);
    const diets = htmlText(
      capture(
        item,
        /<div\b[^>]*class=["'][^"']*\baccordion__footer__special-diets\b[^"']*["'][^>]*>([\s\S]*?)<\/div>/i
      ) ?? ""
    )
      .split(/[,;/]/)
      .map(normalizeText)
      .filter(Boolean);
    const co2 = firstNumber(co2Line);
    result.set(mealKey(name), {
      id: `antell-${hashString(mealKey(name))}`,
      name,
      ingredients,
      ...(parseNutrition(nutritionLine).length
        ? { nutritionPer100g: parseNutrition(nutritionLine) }
        : {}),
      ...(co2 !== undefined ? { co2eKilogramsPer100Grams: co2 } : {}),
      ...(diets.length ? { diets } : {})
    });
  }
  return result;
}

function labeledParagraph(html: string, labels: string[]): string {
  for (const match of html.matchAll(/<p\b[^>]*>([\s\S]*?)<\/p>/gi)) {
    const text = htmlText(match[1] ?? "");
    for (const label of labels) {
      const index = text.toLocaleLowerCase().indexOf(label.toLocaleLowerCase());
      if (index >= 0) {
        return normalizeText(
          text.slice(index + label.length).replace(/^[\s:]+/, "")
        );
      }
    }
  }
  return "";
}

function parseNutrition(
  value: string
): Array<{ name: string; amount: number; unit: string }> {
  const result: Array<{ name: string; amount: number; unit: string }> = [];
  for (const part of value.split(",").map(normalizeText)) {
    const lower = part.toLocaleLowerCase();
    const amount = firstNumber(part);
    if (amount === undefined) continue;
    let name = "";
    if (lower.includes("kcal") || lower.includes("energia") || lower.includes("energy")) {
      name = "EnergyKcal";
    } else if (
      lower.includes("hiilihydra") ||
      lower.includes("carbohydrate") ||
      lower.includes("carbs")
    ) {
      name = "Carbohydrates";
    } else if (lower.includes("proteiin") || lower.includes("protein")) {
      name = "Protein";
    } else if (
      (lower.includes("rasva") || lower.includes("fat")) &&
      !lower.includes("tyydytt") &&
      !lower.includes("saturated")
    ) {
      name = "Fat";
    }
    if (name && !result.some(entry => entry.name === name)) {
      result.push({ name, amount, unit: lower.includes("kcal") ? "kcal" : "g" });
    }
  }
  return result;
}

function firstNumber(value: string): number | undefined {
  const match = value.match(/\d+(?:[.,]\d+)?/);
  if (!match) return undefined;
  const parsed = Number.parseFloat(match[0].replace(",", "."));
  return Number.isFinite(parsed) ? parsed : undefined;
}

function weekdayToken(date: string): string {
  return new Intl.DateTimeFormat("en-US", {
    weekday: "long",
    timeZone: "Europe/Helsinki"
  })
    .format(new Date(`${date}T12:00:00Z`))
    .toLocaleLowerCase("en-US");
}

function capture(value: string, expression: RegExp): string | undefined {
  return expression.exec(value)?.[1];
}

function mealKey(value: string): string {
  return normalizeText(value)
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLocaleLowerCase("en");
}

function titleCase(value: string): string {
  return value ? value[0]?.toUpperCase() + value.slice(1) : value;
}

function escapeRegularExpression(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function hashString(value: string): string {
  let hash = 0x811c9dc5;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193);
  }
  return String(hash >>> 0);
}
