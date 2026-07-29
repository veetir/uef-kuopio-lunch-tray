import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { afterEach, describe, expect, it, vi } from "vitest";
import menuSchema from "../schema/v1/menu.schema.json";
import restaurantsSchema from "../schema/v1/restaurants.schema.json";
import snapshotSchema from "../schema/v1/snapshot.schema.json";
import { restaurantCatalog } from "../src/catalog";
import { handleRequest } from "../src/index";
import { menuCacheKey } from "../src/menu";
import contractMenu from "./fixtures/contract-menu.json";

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
ajv.addSchema(restaurantsSchema);
ajv.addSchema(menuSchema);
ajv.addSchema(snapshotSchema);

class MemoryKv {
  readonly values = new Map<string, string>();

  async get(key: string): Promise<string | null> {
    return this.values.get(key) ?? null;
  }

  async put(key: string, value: string): Promise<void> {
    this.values.set(key, value);
  }
}

describe("published v1 schemas", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("validates the contract menu fixture", () => {
    const validate = ajv.getSchema(menuSchema.$id);
    expect(validate).toBeDefined();
    expect(validate?.(contractMenu), JSON.stringify(validate?.errors)).toBe(true);
  });

  it("validates the generated catalogue", () => {
    const validate = ajv.getSchema(restaurantsSchema.$id);
    expect(validate).toBeDefined();
    expect(
      validate?.(restaurantCatalog()),
      JSON.stringify(validate?.errors)
    ).toBe(true);
  });

  it("validates a contract snapshot", () => {
    const validate = ajv.getSchema(snapshotSchema.$id);
    const snapshot = {
      apiVersion: "v1",
      schemaVersion: 1,
      revision: "test",
      requestedLanguage: "fi",
      date: contractMenu.date,
      restaurants: [contractMenu.restaurant],
      menus: [contractMenu]
    };
    expect(validate).toBeDefined();
    expect(validate?.(snapshot), JSON.stringify(validate?.errors)).toBe(true);
  });

  it("allows additive fields on every response object", () => {
    for (const schema of [menuSchema, restaurantsSchema, snapshotSchema]) {
      expect(findClosedObjects(schema)).toEqual([]);
    }
  });

  it("validates generated catalogue and snapshot route responses", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    const catalogueResponse = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/restaurants"),
      "test"
    );
    const snapshotResponse = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/snapshot?language=fi&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    const catalogue = await catalogueResponse.json();
    const snapshot = await snapshotResponse.json();
    const validateCatalogue = ajv.getSchema(restaurantsSchema.$id);
    const validateSnapshot = ajv.getSchema(snapshotSchema.$id);

    expect(
      validateCatalogue?.(catalogue),
      JSON.stringify(validateCatalogue?.errors)
    ).toBe(true);
    expect(
      validateSnapshot?.(snapshot),
      JSON.stringify(validateSnapshot?.errors)
    ).toBe(true);
  });

  it("validates a stale-generation menu route response", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    cache.values.set(
      menuCacheKey("tietoteknia", "fi", "2026-07-24", "2026-07-27"),
      JSON.stringify(contractMenu)
    );
    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu"
          + "?language=fi&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    const menu = await response.json();
    const validate = ajv.getSchema(menuSchema.$id);

    expect(menu).toMatchObject({ freshness: { isStale: true } });
    expect(validate?.(menu), JSON.stringify(validate?.errors)).toBe(true);
  });
});

function findClosedObjects(
  value: unknown,
  path = "$"
): string[] {
  if (!value || typeof value !== "object") return [];
  if (Array.isArray(value)) {
    return value.flatMap((item, index) =>
      findClosedObjects(item, `${path}[${index}]`)
    );
  }
  const object = value as Record<string, unknown>;
  const own = object.additionalProperties === false ? [path] : [];
  return own.concat(
    Object.entries(object).flatMap(([key, child]) =>
      findClosedObjects(child, `${path}.${key}`)
    )
  );
}
