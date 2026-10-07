import { describe, expect, it, vi } from "vitest";
import { restaurantConfiguration } from "../src/catalog";
import { fetchCompassMenu } from "../src/providers/compass";

const restaurant = restaurantConfiguration("tietoteknia")!;
const date = "2026-10-07";

function upstream(options: {
  language?: "fi" | "en";
  referenceDate?: string;
  referenceFailure?: boolean;
  failedRecipe?: number;
  count?: number;
} = {}) {
  const names = options.language === "en"
    ? ["Onion soup, croutons and cheese", "Rice"]
    : ["Sipulikeittoa, krutonkeja ja juustoa", "Tummaa riisiä"];
  const meals = Array.from({ length: options.count ?? 2 }, (_, i) => ({
    name: names[i] ?? `Dish ${i}`,
    recipeId: i + 1
  }));
  // A recipe may be repeated under a different display name.
  meals.push({ name: "Alternative name", recipeId: 1 });
  const fetcher = vi.fn(async (input: RequestInfo | URL) => {
    const url = new URL(String(input));
    if (url.pathname === "/menuapi/feed/json") {
      return Response.json({
        RestaurantUrl: "https://example.com/finnish-page-with-yesterdays-menu",
        MenusForDays: [{ Date: date, SetMenus: [{
          Components: meals.map(meal => `${meal.name} (L)`)
        }] }]
      });
    }
    if (url.pathname === "/menuapi/day-menus") {
      expect(url.searchParams.get("costCenter")).toBe("0439");
      expect(url.searchParams.get("date")).toBe(date);
      expect(url.searchParams.get("language")).toBe(options.language ?? "fi");
      if (options.referenceFailure) return new Response("Unavailable", { status: 503 });
      return Response.json({
        date: `${options.referenceDate ?? date}T00:00:00`,
        menuPackages: [{ meals: meals.map(meal => ({ ...meal, name: `${meal.name}\n` })) }]
      });
    }
    const match = url.pathname.match(/^\/menuapi\/recipes\/(\d+)$/);
    if (!match) throw new Error(`Unexpected upstream: ${url}`);
    const id = Number(match[1]);
    expect(url.searchParams.get("language")).toBe(options.language ?? "fi");
    if (options.failedRecipe === id) return new Response("Unavailable", { status: 503 });
    return Response.json({ recipeId: id, ingredientsCleaned: `Ingredients ${id}` });
  });
  return fetcher;
}

describe("Compass recipe enrichment", () => {
  it.each(["fi", "en"] as const)("uses the requested date and %s language", async language => {
    const fetcher = upstream({ language });
    const menu = await fetchCompassMenu({ restaurant, date, language, fetcher });
    expect(menu.groups[0]?.items.map(item => item.recipe?.ingredients)).toEqual([
      "Ingredients 1", "Ingredients 2", "Ingredients 1"
    ]);
    expect(fetcher).toHaveBeenCalledTimes(4);
  });

  it("fetches all recipes when there are more than sixteen", async () => {
    const fetcher = upstream({ count: 18 });
    const menu = await fetchCompassMenu({ restaurant, date, language: "fi", fetcher });
    expect(menu.groups[0]?.items).toHaveLength(19);
    expect(menu.groups[0]?.items.every(item => item.recipe?.ingredients)).toBe(true);
    expect(fetcher).toHaveBeenCalledTimes(20);
  });

  it("preserves other recipes and the menu when one recipe fails", async () => {
    const menu = await fetchCompassMenu({
      restaurant, date, language: "fi", fetcher: upstream({ failedRecipe: 2 })
    });
    expect(menu.status).toBe("serving");
    expect(menu.groups[0]?.items.map(item => item.recipe?.ingredients)).toEqual([
      "Ingredients 1", undefined, "Ingredients 1"
    ]);
  });

  it.each([
    { referenceDate: "2026-10-06" },
    { referenceFailure: true }
  ])("keeps the menu without attaching wrong-day or unavailable recipes: %j", async options => {
    const fetcher = upstream(options);
    const menu = await fetchCompassMenu({ restaurant, date, language: "fi", fetcher });
    expect(menu.status).toBe("serving");
    expect(menu.groups[0]?.items).toHaveLength(3);
    expect(menu.groups[0]?.items.every(item => !item.recipe)).toBe(true);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });
});
