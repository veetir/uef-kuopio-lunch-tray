import {
  itemFromText,
  normalizeText,
  parseCompassPrices,
  splitItemText,
  stableResponseId,
  validGroups
} from "../normalize";
import type {
  LunchItem,
  MenuGroup,
  RecipeDetails
} from "../types";
import {
  fetchOrDefault,
  responseText,
  type ParsedProviderMenu,
  type ProviderRequest
} from "./provider";

interface CompassRoot {
  RestaurantUrl?: unknown;
  MenusForDays?: CompassDay[] | null;
  ErrorText?: unknown;
}

interface CompassDay {
  Date?: unknown;
  LunchTime?: unknown;
  SetMenus?: CompassSetMenu[] | null;
}

interface CompassSetMenu {
  SortOrder?: unknown;
  Name?: unknown;
  Price?: unknown;
  Components?: unknown[] | null;
}

interface RecipeReference {
  id: number;
  name: string;
}

export async function fetchCompassMenu(
  request: ProviderRequest
): Promise<ParsedProviderMenu> {
  if (request.restaurant.source.type !== "compass") {
    throw new Error("Invalid Compass configuration");
  }
  const fetcher = fetchOrDefault(request.fetcher);
  const language = request.restaurant.languages.includes(request.language)
    ? request.language
    : (request.restaurant.languages[0] ?? "fi");
  const endpoint = new URL("https://www.compass-group.fi/menuapi/feed/json");
  endpoint.searchParams.set(
    "costNumber",
    request.restaurant.source.costNumber
  );
  endpoint.searchParams.set("language", language);
  const raw = await responseText(
    await fetcher(endpoint, {
      headers: { Accept: "application/json" }
    })
  );
  const payload = JSON.parse(raw) as CompassRoot;
  const errorText = normalizeText(payload.ErrorText);
  if (errorText) throw new Error(`Compass: ${errorText}`);

  const day = (payload.MenusForDays ?? []).find(candidate =>
    normalizeText(candidate.Date).startsWith(request.date)
  );
  if (!day) {
    return {
      contentLanguage: language,
      status: "noMenu",
      offers: [],
      groups: []
    };
  }

  const groups = parseCompassGroups(
    day.SetMenus ?? [],
    request.restaurant.id
  );
  if (groups.length > 0) {
    const pageUrl =
      normalizeText(payload.RestaurantUrl) ||
      request.restaurant.websiteUrl ||
      "";
    if (pageUrl) {
      await enrichCompassRecipes(groups, pageUrl, language, fetcher);
    }
  }

  return {
    contentLanguage: language,
    status: groups.length ? "serving" : "noMenu",
    ...(normalizeText(day.LunchTime)
      ? { hours: normalizeText(day.LunchTime) }
      : {}),
    offers: [],
    groups
  };
}

export function parseCompassGroups(
  setMenus: CompassSetMenu[],
  restaurantId: string
): MenuGroup[] {
  return validGroups(
    setMenus
      .map((rawGroup, sourceIndex) => {
        const components = Array.isArray(rawGroup.Components)
          ? rawGroup.Components
          : [];
        const items = components
          .map((component, itemIndex) =>
            itemFromText(
              component,
              stableResponseId(`group-${sourceIndex + 1}-item`, itemIndex)
            )
          )
          .filter((item): item is LunchItem => item !== undefined);
        const title = normalizedCompassTitle(
          normalizeText(rawGroup.Name),
          restaurantId
        );
        const numericSort = Number(rawGroup.SortOrder);
        return {
          id: stableResponseId("group", sourceIndex),
          ...(title ? { title } : {}),
          prices: parseCompassPrices(rawGroup.Price, restaurantId),
          items,
          sortOrder: Number.isFinite(numericSort) ? numericSort : sourceIndex
        };
      })
      .sort(
        (left, right) =>
          left.sortOrder - right.sortOrder ||
          left.id.localeCompare(right.id)
      )
  );
}

function normalizedCompassTitle(
  value: string,
  restaurantId: string
): string {
  if (!value || value.toLowerCase() === "menu") return "";
  if (restaurantId !== "tietoteknia") return value;
  const titles: Record<string, string> = {
    "LUNCH BUFFEE": "Main course",
    "PÄIVÄN SOPPA": "Keitto",
    "LOUNAS BUFFA": "Pääruoka",
    "JÄLKKÄRI": "Jälkiruoka"
  };
  return titles[value] ?? value;
}

async function enrichCompassRecipes(
  groups: MenuGroup[],
  pageUrl: string,
  language: string,
  fetcher: typeof fetch
): Promise<void> {
  try {
    const html = await responseText(
      await fetcher(pageUrl, { headers: { Accept: "text/html" } })
    );
    const references = compassRecipeReferences(html);
    if (!references.length) return;
    const wanted = new Map<string, RecipeReference>();
    for (const group of groups) {
      for (const item of group.items) {
        const reference = references.find(
          candidate => mealKey(candidate.name) === mealKey(item.name)
        );
        if (reference) wanted.set(String(reference.id), reference);
      }
    }

    const details = new Map<string, RecipeDetails>();
    const pending = [...wanted.values()].slice(0, 16);
    for (let index = 0; index < pending.length; index += 4) {
      await Promise.all(pending.slice(index, index + 4).map(async reference => {
        try {
          const endpoint =
            `https://www.compass-group.fi/menuapi/recipes/${reference.id}` +
            `?language=${encodeURIComponent(language)}`;
          const payload = JSON.parse(
            await responseText(
              await fetcher(endpoint, {
                headers: { Accept: "application/json" }
              })
            )
          ) as Record<string, unknown>;
          const detail = parseCompassRecipe(payload, reference.id);
          if (detail) details.set(mealKey(reference.name), detail);
        } catch {
          // Recipe enrichment is optional; the menu remains valid without it.
        }
      }));
    }

    for (const group of groups) {
      for (const item of group.items) {
        const detail = details.get(mealKey(item.name));
        if (detail) item.recipe = detail;
      }
    }
  } catch {
    // Restaurant pages occasionally fail independently of the JSON menu.
  }
}

export function compassRecipeReferences(html: string): RecipeReference[] {
  const marker = html.indexOf("window.__INITIAL_MENU__");
  if (marker < 0) return [];
  const objectStart = html.indexOf("{", marker);
  if (objectStart < 0) return [];
  const json = balancedJsonObject(html, objectStart);
  if (!json) return [];
  try {
    const root = JSON.parse(json) as Record<string, unknown>;
    const dayMenu = root.dayMenu as Record<string, unknown> | undefined;
    const packages = Array.isArray(dayMenu?.menuPackages)
      ? dayMenu.menuPackages
      : [];
    const references: RecipeReference[] = [];
    for (const rawPackage of packages) {
      const menuPackage = rawPackage as Record<string, unknown>;
      const meals = Array.isArray(menuPackage.meals)
        ? menuPackage.meals
        : [];
      for (const rawMeal of meals) {
        const meal = rawMeal as Record<string, unknown>;
        const id = Number(meal.recipeId);
        const name = normalizeText(meal.name);
        if (Number.isInteger(id) && id > 0 && name) {
          references.push({ id, name });
        }
      }
    }
    return references;
  } catch {
    return [];
  }
}

function balancedJsonObject(value: string, start: number): string | undefined {
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let index = start; index < value.length; index += 1) {
    const character = value[index];
    if (inString) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === "\"") inString = false;
      continue;
    }
    if (character === "\"") inString = true;
    else if (character === "{") depth += 1;
    else if (character === "}") {
      depth -= 1;
      if (depth === 0) return value.slice(start, index + 1);
    }
  }
  return undefined;
}

export function parseCompassRecipe(
  payload: Record<string, unknown>,
  fallbackId: number
): RecipeDetails | undefined {
  const nutrition = Array.isArray(payload.nutritionalValues)
    ? payload.nutritionalValues
        .map(raw => {
          const entry = raw as Record<string, unknown>;
          const name = normalizeText(entry.name);
          const amount = Number(entry.amount);
          const unit = normalizeText(entry.unit);
          return name && Number.isFinite(amount) && unit
            ? { name, amount, unit }
            : undefined;
        })
        .filter(
          (
            value
          ): value is { name: string; amount: number; unit: string } =>
            value !== undefined
        )
    : [];
  const ingredients = normalizeText(payload.ingredientsCleaned);
  const name = normalizeText(payload.name);
  const diets = normalizeText(payload.diets)
    .split(/[,;/]/)
    .map(normalizeText)
    .filter(Boolean);
  const rawCo2 = payload.kgCO2ePer100g;
  const co2 = rawCo2 === null ||
      rawCo2 === undefined ||
      (typeof rawCo2 === "string" && normalizeText(rawCo2) === "")
    ? undefined
    : Number(rawCo2);
  const hasCo2 = co2 !== undefined && Number.isFinite(co2) && co2 >= 0;
  if (!ingredients && !nutrition.length && !diets.length && !hasCo2) {
    return undefined;
  }
  const id = Number(payload.recipeId);
  return {
    id: `compass-${Number.isInteger(id) && id > 0 ? id : fallbackId}`,
    ...(name ? { name } : {}),
    ...(ingredients ? { ingredients } : {}),
    ...(nutrition.length ? { nutritionPer100g: nutrition } : {}),
    ...(hasCo2
      ? { co2eKilogramsPer100Grams: co2 }
      : {}),
    ...(diets.length ? { diets } : {})
  };
}

function mealKey(value: string): string {
  const { name } = splitItemText(value);
  return name
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLocaleLowerCase("en");
}
