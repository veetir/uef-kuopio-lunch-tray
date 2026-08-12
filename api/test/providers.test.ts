import { describe, expect, it, vi } from "vitest";
import contractMenu from "./fixtures/contract-menu.json";
import huomenFixture from "./fixtures/huomen.json";
import antellRound from "./fixtures/antell-round-friday-snippet.html?raw";
import antellHours from "./fixtures/antell-hours-snippet.html?raw";
import huomenHours from "./fixtures/huomen-hours-snippet.html?raw";
import pranzeria from "./fixtures/pranzeria-snippet.html?raw";
import snellari from "./fixtures/snellari.rss?raw";
import { parseAntell } from "../src/providers/antell";
import {
  parseCompassGroups,
  parseCompassRecipe
} from "../src/providers/compass";
import { parseHuomen } from "../src/providers/huomen";
import { parseCompassRss } from "../src/providers/rss";
import { parseSorrento } from "../src/providers/sorrento";
import {
  fetchOrDefault,
  responseText
} from "../src/providers/provider";
import { validateMenu } from "../src/validate";
import type { RestaurantMenu } from "../src/types";

describe("v1 menu contract", () => {
  it("accepts offers, audience prices, recipes and price-free groups", () => {
    expect(() => validateMenu(contractMenu as RestaurantMenu)).not.toThrow();
    const menu = contractMenu as RestaurantMenu;
    expect(menu.groups[1]?.items).toHaveLength(1);
    expect(menu.groups[2]).not.toHaveProperty("title");
    expect(menu.groups[2]?.prices).toEqual([]);
  });
});

describe("provider normalization", () => {
  it("identifies every upstream request", async () => {
    const baseFetch = vi.fn(async (
      _input: RequestInfo | URL,
      _init?: RequestInit
    ) => new Response("ok"));
    const identifiedFetch = fetchOrDefault(
      baseFetch as unknown as typeof fetch
    );

    await identifiedFetch("https://example.com", {
      headers: { Accept: "application/json" }
    });

    const init = baseFetch.mock.calls[0]?.[1] as RequestInit | undefined;
    const headers = new Headers(init?.headers);
    expect(headers.get("User-Agent")).toBe(
      "UEFKuopioLunchAPI/1.0 (+https://lunch.veeti.dev)"
    );
    expect(headers.get("Accept")).toBe("application/json");
    expect(init?.signal).toBeInstanceOf(AbortSignal);
  });

  it("rejects oversized upstream responses before parsing", async () => {
    await expect(
      responseText(new Response("small", {
        headers: { "Content-Length": "2097153" }
      }))
    ).rejects.toThrow("size limit");
    await expect(
      responseText(new Response("x".repeat(2 * 1024 * 1024 + 1)))
    ).rejects.toThrow("size limit");
  });

  it("preserves Compass titleless items and maps Tietoteknia prices", () => {
    const groups = parseCompassGroups(
      [
        {
          SortOrder: 1,
          Name: "LOUNAS BUFFA",
          Price: "13,30€ / opisk.3.100EUR",
          Components: ["Härkäpapua tikka masala (G, L, M, Veg)"]
        },
        {
          SortOrder: 2,
          Name: "JÄLKKÄRI",
          Price: "Opisk. 1,80€",
          Components: ["Omena-kauramurupaistos"]
        },
        {
          SortOrder: 3,
          Name: null,
          Price: null,
          Components: ["Satokauden kasviksia"]
        },
        {
          SortOrder: 4,
          Name: "Menu",
          Price: null,
          Components: []
        }
      ],
      "tietoteknia"
    );

    expect(groups).toHaveLength(3);
    expect(groups[0]?.prices).toEqual([
      {
        amount: "13.30",
        currency: "EUR",
        audiences: ["staff", "guest"]
      },
      {
        amount: "3.10",
        currency: "EUR",
        audiences: ["student"]
      }
    ]);
    expect(groups[1]?.prices[0]?.audiences).toEqual([
      "student",
      "staff",
      "guest"
    ]);
    expect(groups[2]).not.toHaveProperty("title");
    expect(groups[2]?.items[0]?.name).toBe("Satokauden kasviksia");
  });

  it("does not turn a missing Compass CO2 value into zero", () => {
    expect(
      parseCompassRecipe(
        { recipeId: 42, kgCO2ePer100g: null },
        42
      )
    ).toBeUndefined();
    expect(
      parseCompassRecipe(
        { recipeId: 42, kgCO2ePer100g: 0 },
        42
      )
    ).toMatchObject({ co2eKilogramsPer100Grams: 0 });
  });

  it("parses Compass RSS items and their diet suffixes", () => {
    const parsed = parseCompassRss(snellari, "fi", "2026-02-23");
    expect(parsed.status).toBe("serving");
    expect(parsed.groups[0]?.items[0]).toMatchObject({
      name: "Juustoista peruna-pinaattisosekeittoa",
      tags: ["*", "A", "G", "ILM", "L"]
    });
  });

  it("parses Huomen items and infers general offers from its JSON", () => {
    const parsed = parseHuomen(
      JSON.stringify(huomenFixture),
      huomenHours,
      "fi",
      "2026-02-23"
    );
    expect(parsed.status).toBe("serving");
    expect(parsed.hours).toBe("10:30–13:00");
    expect(parsed.offers).toEqual([
      {
        id: "soup-lunch",
        label: "Keittolounas",
        price: { amount: "10.90", currency: "EUR" }
      },
      {
        id: "lunch",
        label: "Lounas",
        price: { amount: "12.90", currency: "EUR" }
      }
    ]);
    expect(parsed.groups[0]?.prices).toEqual([]);
  });

  it("parses Antell group prices into staff/guest and student audiences", () => {
    const parsed = parseAntell(
      antellRound,
      antellHours,
      "fi",
      "2026-02-20",
      "friday"
    );
    expect(parsed.status).toBe("serving");
    expect(parsed.hours).toBe("10:30–13:00");
    expect(parsed.groups[0]?.prices).toEqual([
      {
        amount: "12.50",
        currency: "EUR",
        audiences: ["staff", "guest"]
      },
      {
        amount: "3.10",
        currency: "EUR",
        audiences: ["student"]
      }
    ]);
  });

  it("parses Highway lunch hours independently of restaurant hours", () => {
    const detailHtml = antellHours.replace(
      "10.30 &#8211; 13.00",
      "10.30 &#8211; 12.30"
    );
    const parsed = parseAntell(
      antellRound,
      detailHtml,
      "fi",
      "2026-02-20",
      "friday"
    );
    expect(parsed.hours).toBe("10:30–12:30");
  });

  it("does not mistake later café hours for missing Antell lunch hours", () => {
    const parsed = parseAntell(
      antellRound,
      `<h3 class="title">Lounas</h3><p>Ei ilmoitettu</p>
       <h3 class="title">Kahvila</h3><span class="hours">8.00–14.00</span>`,
      "fi",
      "2026-02-20",
      "friday"
    );
    expect(parsed.hours).toBeUndefined();
  });

  it("parses Sorrento dishes with standalone diet tags", () => {
    const parsed = parseSorrento(
      `<h6><strong>20SALAATTILOUNAS 10.90 € (SIS. SALAATTI, KAHVI)</strong></h6>
       <h6><strong>LOUNASBUFFET 14.00 € (SIS. PIZZA, PASTA)</strong></h6>
       <h6><strong>SOPIMUSLOUNAS 13.80 €</strong></h6>${pranzeria}`,
      "fi",
      "2026-03-20"
    );
    expect(parsed.status).toBe("serving");
    expect(parsed.hours).toBe("10:30–14:00");
    expect(parsed.offers).toHaveLength(3);
    expect(parsed.offers[0]).toMatchObject({
      id: "salad-lunch",
      description: "Salaatti, kahvi"
    });
    expect(
      parsed.groups[0]?.items.find(item =>
        item.name.startsWith("Spezzatino Di Manzo")
      )?.tags
    ).toEqual(["G", "L"]);
    expect(
      parsed.groups[0]?.items.find(item =>
        item.name.startsWith("Gnocchi Burro")
      )?.tags
    ).toEqual(["V", "G"]);
  });

  it("preserves mixed-case Sorrento offer descriptions", () => {
    const parsed = parseSorrento(
      `<h6><strong>SALAATTILOUNAS 10.90 € (SIS. Salaatti, focaccia)</strong></h6>`,
      "fi",
      "2026-03-20"
    );

    expect(parsed.offers[0]?.description).toBe("Salaatti, focaccia");
  });
});
