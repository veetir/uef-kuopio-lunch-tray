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
    await enrichCompassRecipes(
      groups,
      request.restaurant.source.costNumber,
      request.date,
      language,
      fetcher
    );
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
  costCenter: string,
  date: string,
  language: string,
  fetcher: typeof fetch
): Promise<void> {
  try {
    // The restaurant page defaults to its own date and language. Around
    // midnight it can still contain yesterday's recipes (and always Finnish
    // names on the Finnish page), even when the feed has today's menu.
    const endpoint = new URL("https://www.compass-group.fi/menuapi/day-menus");
    endpoint.searchParams.set("costCenter", costCenter);
    endpoint.searchParams.set("date", date);
    endpoint.searchParams.set("language", language);
    const dayMenu = JSON.parse(await responseText(
      await fetcher(endpoint, { headers: { Accept: "application/json" } })
    )) as Record<string, unknown>;
    if (!normalizeText(dayMenu.date).startsWith(date)) return;
    const references = compassRecipeReferences(dayMenu);
    if (!references.length) return;
    const wanted = new Map<number, RecipeReference>();
    const itemRecipes = new Map<LunchItem, number>();
    for (const group of groups) {
      for (const item of group.items) {
        const reference = references.find(
          candidate => mealKey(candidate.name) === mealKey(item.name)
        );
        if (reference) {
          wanted.set(reference.id, reference);
          itemRecipes.set(item, reference.id);
        }
      }
    }

    const details = new Map<number, RecipeDetails>();
    const pending = [...wanted.values()];
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
          if (detail) details.set(reference.id, detail);
        } catch {
          // Recipe enrichment is optional; the menu remains valid without it.
        }
      }));
    }

    for (const group of groups) {
      for (const item of group.items) {
        const id = itemRecipes.get(item);
        const detail = id === undefined ? undefined : details.get(id);
        if (detail) item.recipe = detail;
      }
    }
  } catch {
    // Recipe references can fail independently of the JSON feed.
  }
}

function compassRecipeReferences(
  dayMenu: Record<string, unknown>
): RecipeReference[] {
  const packages = Array.isArray(dayMenu.menuPackages)
    ? dayMenu.menuPackages
    : [];
  const references: RecipeReference[] = [];
  for (const menuPackage of packages) {
    const meals = Array.isArray(menuPackage?.meals) ? menuPackage.meals : [];
    for (const meal of meals) {
      const id = Number(meal?.recipeId);
      const name = normalizeText(meal?.name);
      if (Number.isInteger(id) && id > 0 && name) {
        references.push({ id, name });
      }
    }
  }
  return references;
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
