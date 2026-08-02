import { describe, expect, it } from "vitest";
import {
  assertValidClosures,
  cacheRevisions,
  publicRestaurant,
  restaurantCatalog,
  restaurantConfigurations
} from "../src/catalog";
import { handleRequest } from "../src/index";

describe("restaurant catalogue", () => {
  it("keeps stable IDs in application shortcut order", () => {
    expect(restaurantCatalog().restaurants.map(({ id }) => id)).toEqual([
      "snellmania",
      "cafe-snellari",
      "canthia",
      "tietoteknia",
      "hyva-huomen-bioteknia",
      "antell-round",
      "antell-highway",
      "mediteknia",
      "pranzeria-sorrento",
      "caari"
    ]);
  });

  it("does not expose provider configuration", () => {
    const restaurant = publicRestaurant("snellmania");
    expect(restaurant).toBeDefined();
    expect(restaurant).not.toHaveProperty("source");
  });

  it("has unique IDs and orders", () => {
    const restaurants = restaurantConfigurations();
    expect(new Set(restaurants.map(({ id }) => id)).size).toBe(
      restaurants.length
    );
    expect(new Set(restaurants.map(({ order }) => order)).size).toBe(
      restaurants.length
    );
  });

  it("publishes restaurant website links", () => {
    expect(publicRestaurant("canthia")?.websiteUrl).toBe(
      "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopistocanthia/"
    );
    expect(publicRestaurant("tietoteknia")?.websiteUrl).toBe(
      "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/tietoteknia/"
    );
  });

  it("keeps cache generations ordered and unique", () => {
    const revisions = cacheRevisions();
    expect(revisions[0]).toBeTruthy();
    expect(new Set(revisions).size).toBe(revisions.length);
  });

  it("contains valid inclusive closure periods", () => {
    for (const restaurant of restaurantConfigurations()) {
      for (const closure of restaurant.closures) {
        expect(closure.startsOn).toMatch(/^\d{4}-\d{2}-\d{2}$/);
        expect(closure.endsOn).toMatch(/^\d{4}-\d{2}-\d{2}$/);
        expect(closure.startsOn <= closure.endsOn).toBe(true);
      }
    }
  });

  it("rejects inverted and overlapping closure configuration", () => {
    expect(() => assertValidClosures([{
      id: "inverted",
      closures: [{
        kind: "seasonal",
        startsOn: "2026-08-02",
        endsOn: "2026-08-01"
      }]
    }])).toThrow("Invalid closure");
    expect(() => assertValidClosures([{
      id: "overlapping",
      closures: [
        {
          kind: "seasonal",
          startsOn: "2026-07-01",
          endsOn: "2026-07-10"
        },
        {
          kind: "exceptional",
          startsOn: "2026-07-10",
          endsOn: "2026-07-12"
        }
      ]
    }])).toThrow("Overlapping closures");
  });
});

describe("HTTP API", () => {
  it("serves the versioned catalogue with CORS", async () => {
    const response = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/restaurants"),
      "test"
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Access-Control-Allow-Origin")).toBe("*");
    expect(response.headers.get("X-Robots-Tag")).toContain("noindex");
    expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff");
    expect(response.headers.get("ETag")).toMatch(/^W\/"[0-9a-f]+-[0-9a-f]{8}"$/);
    const body = await response.json();
    expect(body).toMatchObject({
      apiVersion: "v1",
      schemaVersion: 1
    });
  });

  it("supports conditional requests for cacheable JSON", async () => {
    const url = "https://lunch.veeti.dev/v1/restaurants";
    const initial = await handleRequest(new Request(url), "test");
    const entityTag = initial.headers.get("ETag");
    expect(entityTag).toBeTruthy();

    const unchanged = await handleRequest(
      new Request(url, {
        headers: { "If-None-Match": entityTag ?? "" }
      }),
      "test"
    );
    expect(unchanged.status).toBe(304);
    expect(unchanged.headers.get("ETag")).toBe(entityTag);
    expect(await unchanged.text()).toBe("");
  });

  it("discourages indexing of the API hostname", async () => {
    const response = await handleRequest(
      new Request("https://lunch.veeti.dev/robots.txt")
    );
    expect(response.status).toBe(200);
    expect(await response.text()).toContain("Disallow: /");
    expect(response.headers.get("Cache-Control")).toContain("max-age=86400");
  });

  it("serves a non-cached status response", async () => {
    const response = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/status"),
      "test"
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(await response.json()).toEqual({
      status: "ok",
      apiVersion: "v1",
      environment: "test"
    });
  });

  it("serves one restaurant and handles unknown IDs", async () => {
    const known = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/restaurants/tietoteknia")
    );
    expect(known.status).toBe(200);
    expect(await known.json()).toMatchObject({
      restaurant: { id: "tietoteknia", name: { fi: "Tietoteknia" } }
    });

    const unknown = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/restaurants/missing")
    );
    expect(unknown.status).toBe(404);
    expect(await unknown.json()).toMatchObject({
      error: { code: "restaurant_not_found" }
    });
  });

  it("supports HEAD without returning a body", async () => {
    const response = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/restaurants", {
        method: "HEAD"
      })
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("ETag")).toBeTruthy();
    expect(await response.text()).toBe("");
  });
});
