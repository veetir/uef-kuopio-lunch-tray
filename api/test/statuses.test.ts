import { afterEach, describe, expect, it, vi } from "vitest";
import contractMenu from "./fixtures/contract-menu.json";
import {
  cacheRevisions,
  restaurantCatalog,
  restaurantConfiguration,
  restaurantConfigurations
} from "../src/catalog";
import {
  handleRequest,
  scheduledRefresh,
  scheduledRestaurants,
  shouldRefreshCachedMenu
} from "../src/index";
import {
  closureForDate,
  configuredOffers,
  fetchRestaurantMenu,
  menuCacheKey,
  snapshotCacheKey,
  unknownMenu
} from "../src/menu";
import { validateMenu, validateSnapshot } from "../src/validate";
import type { LunchSnapshot, RestaurantMenu } from "../src/types";

class MemoryKv {
  readonly values = new Map<string, string>();
  readonly putOptions: Array<{ expirationTtl?: number }> = [];
  gets = 0;
  puts = 0;

  async get(key: string): Promise<string | null> {
    this.gets += 1;
    return this.values.get(key) ?? null;
  }

  async put(
    key: string,
    value: string,
    options?: { expirationTtl?: number }
  ): Promise<void> {
    this.puts += 1;
    this.putOptions.push(options ?? {});
    this.values.set(key, value);
  }
}

class FailingPutKv extends MemoryKv {
  override async put(): Promise<void> {
    throw new Error("KV write limit reached");
  }
}

class FakeRateLimit {
  calls: string[] = [];

  constructor(private readonly success: boolean) {}

  async limit({ key }: RateLimitOptions): Promise<RateLimitOutcome> {
    this.calls.push(key);
    return { success: this.success };
  }
}

describe("service states", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("keeps every v1 service state valid", () => {
    const serving = contractMenu as RestaurantMenu;
    const restaurant = restaurantConfiguration("tietoteknia");
    expect(restaurant).toBeDefined();
    if (!restaurant) return;

    const variants: RestaurantMenu[] = [
      serving,
      {
        ...serving,
        service: { status: "noMenu" },
        offers: [],
        groups: []
      },
      {
        ...serving,
        service: { status: "closed" },
        closure: {
          kind: "seasonal",
          startsOn: "2026-07-01",
          endsOn: "2026-07-31"
        },
        offers: [],
        groups: []
      },
      unknownMenu(restaurant, "fi", "2026-07-24")
    ];

    for (const menu of variants) {
      expect(() => validateMenu(menu)).not.toThrow();
    }
  });

  it("serves all restaurants and menus in one cache-only snapshot", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    cache.values.set(
      menuCacheKey("tietoteknia", "fi", "2026-07-24"),
      JSON.stringify(contractMenu)
    );
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/snapshot?language=fi&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    const snapshot = await response.json() as LunchSnapshot;

    expect(response.status).toBe(200);
    expect(snapshot.restaurants).toHaveLength(10);
    expect(snapshot.menus).toHaveLength(10);
    expect(snapshot.menus.map(menu => menu.restaurant.id)).toEqual(
      snapshot.restaurants.map(restaurant => restaurant.id)
    );
    expect(
      snapshot.menus.find(menu => menu.restaurant.id === "tietoteknia")
    ).toMatchObject({ service: { status: "serving" } });
    expect(
      snapshot.menus.find(menu => menu.restaurant.id === "caari")
    ).toMatchObject({ service: { status: "closed" } });
    expect(response.headers.get("Cache-Control")).toContain("max-age=60");
    expect(() => validateSnapshot(snapshot)).not.toThrow();
    expect(cache.puts).toBe(0);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("serves a preassembled snapshot with one KV read", async () => {
    vi.useFakeTimers();
    const now = new Date("2026-07-24T09:00:00.000Z");
    vi.setSystemTime(now);
    const cache = new MemoryKv();
    const catalog = restaurantCatalog();
    const snapshot: LunchSnapshot = {
      apiVersion: "v1",
      schemaVersion: 1,
      revision: catalog.revision,
      requestedLanguage: "en",
      date: "2026-07-24",
      restaurants: catalog.restaurants,
      menus: restaurantConfigurations().map(restaurant =>
        unknownMenu(restaurant, "en", "2026-07-24", now)
      )
    };
    cache.values.set(
      snapshotCacheKey("en", "2026-07-24"),
      JSON.stringify(snapshot)
    );

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/snapshot?language=en&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(snapshot);
    expect(cache.gets).toBe(1);
    expect(cache.puts).toBe(0);
  });

  it("serves an older cache generation as stale while the new one warms", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    const revisions = cacheRevisions();
    cache.values.set(
      menuCacheKey(
        "tietoteknia",
        "fi",
        "2026-07-24",
        revisions[revisions.length - 1]
      ),
      JSON.stringify(contractMenu)
    );
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu"
          + "?language=fi&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    const menu = await response.json() as RestaurantMenu;

    expect(response.status).toBe(200);
    expect(menu.service.status).toBe("serving");
    expect(menu.freshness.isStale).toBe(true);
    expect(shouldRefreshCachedMenu(menu)).toBe(true);
    expect(cache.gets).toBe(revisions.length);
    expect(cache.puts).toBe(0);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("falls back to a restaurant's available content language", async () => {
    vi.useFakeTimers();
    const now = new Date("2026-07-24T09:00:00.000Z");
    vi.setSystemTime(now);
    const cache = new MemoryKv();
    const restaurant = restaurantConfiguration("pranzeria-sorrento");
    expect(restaurant).toBeDefined();
    if (!restaurant) return;
    const cached = unknownMenu(restaurant, "fi", "2026-07-24", now);
    cache.values.set(
      menuCacheKey(restaurant.id, "fi", "2026-07-24"),
      JSON.stringify(cached)
    );

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/pranzeria-sorrento/menu?language=en&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );

    expect(await response.json()).toMatchObject({
      requestedLanguage: "en",
      contentLanguage: "fi",
      restaurant: { id: "pranzeria-sorrento" }
    });
    expect(cache.gets).toBe(1);
  });

  it("populates configured closures through the scheduler", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    await scheduledRefresh(
      {
        scheduledTime: Date.parse("2026-07-23T21:06:00.000Z")
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/cafe-snellari/menu?language=en&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      restaurant: { id: "cafe-snellari" },
      requestedLanguage: "en",
      service: { status: "closed" },
      closure: {
        startsOn: "2026-05-08",
        endsOn: "2026-08-30"
      }
    });
    expect(cache.values.size).toBe(4);
    expect(cache.values.has(snapshotCacheKey("fi", "2026-07-24"))).toBe(true);
    expect(cache.values.has(snapshotCacheKey("en", "2026-07-24"))).toBe(true);
    expect(cache.putOptions).toEqual([
      { expirationTtl: 604800 },
      { expirationTtl: 604800 },
      { expirationTtl: 604800 },
      { expirationTtl: 604800 }
    ]);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("returns a cacheable unknown menu without fetching or writing on a miss", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new FailingPutKv();
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);
    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?language=en&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );

    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      restaurant: { id: "tietoteknia" },
      service: { status: "unknown" }
    });
    expect(response.headers.get("Cache-Control")).toContain("max-age=60");
    expect(fetcher).not.toHaveBeenCalled();
    expect(cache.puts).toBe(0);
  });

  it("keeps the full closure interval and treats endsOn as inclusive", () => {
    const restaurant = restaurantConfiguration("cafe-snellari");
    expect(restaurant).toBeDefined();
    if (!restaurant) return;

    expect(closureForDate(restaurant, "en", "2026-08-30")).toMatchObject({
      startsOn: "2026-05-08",
      endsOn: "2026-08-30"
    });
    expect(closureForDate(restaurant, "en", "2026-08-31")).toBeUndefined();
  });

  it("serves cached unpublished menus without a public provider refresh", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T12:00:00.000Z"));
    const cache = new MemoryKv();
    const cached = {
      ...(contractMenu as RestaurantMenu),
      service: { status: "noMenu" as const },
      offers: [],
      groups: [],
      freshness: {
        fetchedAt: "2026-07-24T08:00:00.000Z",
        isStale: false
      }
    };
    const key = menuCacheKey("tietoteknia", "fi", "2026-07-24");
    cache.values.set(key, JSON.stringify(cached));
    const fetcher = vi.fn().mockRejectedValue(new Error("offline"));
    vi.stubGlobal("fetch", fetcher);

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?language=fi&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      restaurant: { id: "tietoteknia" },
      service: { status: "noMenu" },
      freshness: { isStale: false }
    });
    expect(cache.values.get(key)).toBe(JSON.stringify(cached));
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("does not refetch a successful current-day menu", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T18:00:00.000Z"));
    const cache = new MemoryKv();
    const cached = contractMenu as RestaurantMenu;
    const key = menuCacheKey("tietoteknia", "fi", "2026-07-24");
    cache.values.set(key, JSON.stringify(cached));
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?language=fi&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );

    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      service: { status: "serving" },
      freshness: { isStale: false }
    });
    expect(fetcher).not.toHaveBeenCalled();
    expect(cache.puts).toBe(0);
  });

  it("retries an unpublished menu after thirty minutes", () => {
    const menu = {
      ...(contractMenu as RestaurantMenu),
      service: { status: "noMenu" as const },
      offers: [],
      groups: [],
      freshness: {
        fetchedAt: "2026-07-24T08:00:00.000Z",
        isStale: false
      }
    };

    expect(
      shouldRefreshCachedMenu(menu, new Date("2026-07-24T08:29:59.999Z"))
    ).toBe(false);
    expect(
      shouldRefreshCachedMenu(menu, new Date("2026-07-24T08:30:00.000Z"))
    ).toBe(true);
  });

  it("refreshes a cached configured closure after it is removed", () => {
    const restaurant = restaurantConfiguration("tietoteknia");
    expect(restaurant).toBeDefined();
    if (!restaurant) return;
    const closure = {
      kind: "seasonal" as const,
      startsOn: "2026-07-20",
      endsOn: "2026-07-24"
    };
    const menu = {
      ...(contractMenu as RestaurantMenu),
      service: { status: "closed" as const },
      closure,
      offers: [],
      groups: []
    };

    expect(
      shouldRefreshCachedMenu(menu, new Date(), {
        ...restaurant,
        closures: [closure]
      })
    ).toBe(false);
    expect(
      shouldRefreshCachedMenu(menu, new Date(), {
        ...restaurant,
        closures: []
      })
    ).toBe(true);
  });

  it("leaves unpublished-menu retries to the scheduler", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T12:00:00.000Z"));
    const cache = new MemoryKv();
    const cached = {
      ...(contractMenu as RestaurantMenu),
      service: { status: "noMenu" as const },
      offers: [],
      groups: [],
      freshness: {
        fetchedAt: "2026-07-24T08:00:00.000Z",
        isStale: false
      }
    };
    cache.values.set(
      menuCacheKey("tietoteknia", "en", "2026-07-24"),
      JSON.stringify(cached)
    );
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);
    const waitUntil = vi.fn();

    const response = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?language=en&date=2026-07-24"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace },
      { waitUntil }
    );

    expect(await response.json()).toMatchObject({
      service: { status: "noMenu" },
      freshness: { isStale: false }
    });
    expect(fetcher).not.toHaveBeenCalled();
    expect(waitUntil).not.toHaveBeenCalled();
  });

  it("rejects non-current dates and non-canonical queries before KV access", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    for (const url of [
      "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?language=en&date=2026-07-23",
      "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?date=2026-07-24&language=en",
      "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu?language=en&date=2026-07-24&cacheBust=1",
      "https://lunch.veeti.dev/v1/restaurants/tietoteknia/menu/"
    ]) {
      const response = await handleRequest(
        new Request(url),
        "test",
        { MENU_CACHE: cache as unknown as KVNamespace }
      );
      expect(response.status).toBe(400);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
    }

    expect(cache.gets).toBe(0);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("rejects invalid snapshot queries before KV access", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();

    for (const url of [
      "https://lunch.veeti.dev/v1/snapshot",
      "https://lunch.veeti.dev/v1/snapshot?date=2026-07-24&language=en",
      "https://lunch.veeti.dev/v1/snapshot?language=en&date=2026-07-23",
      "https://lunch.veeti.dev/v1/snapshot?language=en&date=2026-07-24&cacheBust=1"
    ]) {
      const response = await handleRequest(
        new Request(url),
        "test",
        { MENU_CACHE: cache as unknown as KVNamespace }
      );
      expect(response.status).toBe(400);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
    }

    expect(cache.gets).toBe(0);
    expect(cache.puts).toBe(0);
  });

  it("does not cache any API error response", async () => {
    const requests = [
      new Request("https://lunch.veeti.dev/v1/not-found"),
      new Request("https://lunch.veeti.dev/v1/restaurants/not-found"),
      new Request("https://lunch.veeti.dev/v1/restaurants", {
        method: "POST"
      })
    ];

    for (const request of requests) {
      const response = await handleRequest(request, "test");
      expect(response.status).toBeGreaterThanOrEqual(400);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
    }
  });

  it("rate limits abusive clients before KV access", async () => {
    const cache = new MemoryKv();
    const globalLimiter = new FakeRateLimit(true);
    const clientLimiter = new FakeRateLimit(false);
    const response = await handleRequest(
      new Request("https://lunch.veeti.dev/v1/restaurants", {
        headers: { "CF-Connecting-IP": "192.0.2.1" }
      }),
      "test",
      {
        MENU_CACHE: cache as unknown as KVNamespace,
        API_GLOBAL_RATE_LIMITER:
          globalLimiter as unknown as RateLimit,
        API_CLIENT_RATE_LIMITER:
          clientLimiter as unknown as RateLimit
      }
    );

    expect(response.status).toBe(429);
    expect(response.headers.get("Retry-After")).toBe("60");
    expect(globalLimiter.calls).toEqual(["public-v1"]);
    expect(clientLimiter.calls).toEqual(["192.0.2.1"]);
    expect(cache.gets).toBe(0);
  });

  it("does not run scheduled provider refreshes in staging", async () => {
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);
    await scheduledRefresh(
      {
        scheduledTime: Date.parse("2026-07-24T04:00:00.000Z")
      } as ScheduledController,
      {
        ENVIRONMENT: "staging",
        MENU_CACHE: new MemoryKv() as unknown as KVNamespace
      }
    );
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("cycles through every restaurant once per hour from midnight", () => {
    const midnightUtc = Date.parse("2026-07-27T21:00:00.000Z");
    const firstHour = Array.from({ length: 10 }, (_, index) =>
      scheduledRestaurants(new Date(midnightUtc + index * 6 * 60 * 1000))
        .map(restaurant => restaurant.id)
    ).flat();

    expect(firstHour).toEqual([
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
    expect(
      scheduledRestaurants(new Date("2026-07-27T22:00:00.000Z"))
        .map(restaurant => restaurant.id)
    ).toEqual(["snellmania"]);
  });

  it("stops the daily refresh cycle at 15:00 Helsinki time", () => {
    const midnightUtc = Date.parse("2026-07-27T21:00:00.000Z");
    const attempts = new Map<string, number>();
    for (let slot = 0; slot < 15 * 10; slot += 1) {
      for (const restaurant of scheduledRestaurants(
        new Date(midnightUtc + slot * 6 * 60 * 1000)
      )) {
        attempts.set(restaurant.id, (attempts.get(restaurant.id) ?? 0) + 1);
      }
    }

    expect([...attempts.values()]).toEqual(Array(10).fill(15));
    expect(
      scheduledRestaurants(new Date("2026-07-28T12:00:00.000Z"))
    ).toEqual([]);
  });

  it("checks only Snellmania on Saturdays and nothing on Sundays", () => {
    const saturdayMidnightUtc = Date.parse("2026-07-31T21:00:00.000Z");
    const saturdayFirstHour = Array.from({ length: 10 }, (_, index) =>
      scheduledRestaurants(
        new Date(saturdayMidnightUtc + index * 6 * 60 * 1000)
      ).map(restaurant => restaurant.id)
    ).flat();
    expect(saturdayFirstHour).toEqual(["snellmania"]);
    expect(
      scheduledRestaurants(new Date("2026-07-31T22:00:00.000Z"))
        .map(restaurant => restaurant.id)
    ).toEqual(["snellmania"]);

    const sundayMidnightUtc = Date.parse("2026-08-01T21:00:00.000Z");
    expect(
      scheduledRestaurants(new Date(sundayMidnightUtc))
    ).toEqual([]);
    expect(
      scheduledRestaurants(new Date("2026-08-02T09:00:00.000Z"))
    ).toEqual([]);
  });

  it("publishes a complete non-unknown snapshot on Sundays", async () => {
    vi.useFakeTimers();
    const sundayMidnight = new Date("2026-08-01T21:00:00.000Z");
    vi.setSystemTime(sundayMidnight);
    const cache = new MemoryKv();
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    await scheduledRefresh(
      {
        scheduledTime: sundayMidnight.getTime()
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );

    expect(cache.puts).toBe(2);
    expect(fetcher).not.toHaveBeenCalled();
    for (const language of ["fi", "en"] as const) {
      const raw = cache.values.get(
        snapshotCacheKey(language, "2026-08-02")
      );
      expect(raw).toBeDefined();
      const snapshot = JSON.parse(raw ?? "{}") as LunchSnapshot;
      expect(snapshot.menus).toHaveLength(10);
      expect(
        snapshot.menus.some(menu => menu.service.status === "unknown")
      ).toBe(false);
      expect(() => validateSnapshot(snapshot)).not.toThrow();
    }
    const menuResponse = await handleRequest(
      new Request(
        "https://lunch.veeti.dev/v1/restaurants/snellmania/menu?language=en&date=2026-08-02"
      ),
      "test",
      { MENU_CACHE: cache as unknown as KVNamespace }
    );
    expect(await menuResponse.json()).toMatchObject({
      restaurant: { id: "snellmania" },
      service: { status: "noMenu" }
    });
    expect(menuResponse.headers.get("Cache-Control")).toContain("max-age=300");
    expect(cache.puts).toBe(2);

    await scheduledRefresh(
      {
        scheduledTime: sundayMidnight.getTime() + 6 * 60 * 1000
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );
    expect(cache.puts).toBe(2);
  });

  it("writes the first weekday snapshot even when its provider fetch fails", async () => {
    vi.useFakeTimers();
    const mondayMidnight = new Date("2026-07-26T21:00:00.000Z");
    vi.setSystemTime(mondayMidnight);
    const cache = new MemoryKv();
    const fetcher = vi.fn().mockRejectedValue(new Error("provider offline"));
    vi.stubGlobal("fetch", fetcher);

    await scheduledRefresh(
      {
        scheduledTime: mondayMidnight.getTime()
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );

    expect(cache.values.has(snapshotCacheKey("fi", "2026-07-27"))).toBe(true);
    expect(cache.values.has(snapshotCacheKey("en", "2026-07-27"))).toBe(true);
    expect(cache.puts).toBe(2);
  });

  it("rewrites snapshots when upstream freshness changes", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-24T09:00:00.000Z"));
    const cache = new MemoryKv();
    const key = menuCacheKey("snellmania", "fi", "2026-07-24");
    const publicValue = restaurantCatalog().restaurants.find(
      restaurant => restaurant.id === "snellmania"
    );
    expect(publicValue).toBeDefined();
    if (!publicValue) return;
    const menu = {
      ...(contractMenu as RestaurantMenu),
      restaurant: publicValue,
      requestedLanguage: "fi" as const,
      contentLanguage: "fi" as const
    };
    cache.values.set(key, JSON.stringify(menu));
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    await scheduledRefresh(
      {
        scheduledTime: Date.parse("2026-07-24T03:00:00.000Z")
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );
    const writesAfterInitialSnapshot = cache.puts;
    cache.values.set(
      key,
      JSON.stringify({
        ...menu,
        freshness: {
          ...menu.freshness,
          fetchedAt: "2026-07-24T09:30:00.000Z"
        }
      })
    );

    await scheduledRefresh(
      {
        scheduledTime: Date.parse("2026-07-24T03:06:00.000Z")
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );

    expect(cache.puts).toBeGreaterThan(writesAfterInitialSnapshot);
    const snapshot = JSON.parse(
      cache.values.get(snapshotCacheKey("fi", "2026-07-24")) ?? "{}"
    ) as LunchSnapshot;
    expect(
      snapshot.menus.find(entry => entry.restaurant.id === "snellmania")
        ?.freshness.fetchedAt
    ).toBe("2026-07-24T09:30:00.000Z");
  });

  it("repairs snapshots without refetching settled daily menus", async () => {
    const cache = new MemoryKv();
    const date = "2026-07-24";
    for (const restaurant of ["snellmania", "cafe-snellari"]) {
      const publicValue = restaurantCatalog().restaurants.find(
        candidate => candidate.id === restaurant
      );
      expect(publicValue).toBeDefined();
      if (!publicValue) continue;
      for (const language of ["fi", "en"] as const) {
        cache.values.set(
          menuCacheKey(restaurant, language, date),
          JSON.stringify({
            ...contractMenu,
            restaurant: publicValue,
            requestedLanguage: language,
            contentLanguage: language
          })
        );
      }
    }
    const fetcher = vi.fn().mockRejectedValue(new Error("must not fetch"));
    vi.stubGlobal("fetch", fetcher);

    await scheduledRefresh(
      {
        scheduledTime: Date.parse("2026-07-24T03:00:00.000Z")
      } as ScheduledController,
      {
        ENVIRONMENT: "production",
        MENU_CACHE: cache as unknown as KVNamespace
      }
    );

    expect(fetcher).not.toHaveBeenCalled();
    expect(cache.puts).toBe(2);
    expect(cache.values.has(snapshotCacheKey("fi", date))).toBe(true);
    expect(cache.values.has(snapshotCacheKey("en", date))).toBe(true);
  });

  it("logs snapshot write failures without failing the scheduled run", async () => {
    const cache = new FailingPutKv();
    const fetcher = vi.fn().mockRejectedValue(new Error("provider offline"));
    const errorLog = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.stubGlobal("fetch", fetcher);

    await expect(
      scheduledRefresh(
        {
          scheduledTime: Date.parse("2026-07-24T03:00:00.000Z")
        } as ScheduledController,
        {
          ENVIRONMENT: "production",
          MENU_CACHE: cache as unknown as KVNamespace
        }
      )
    ).resolves.toBeUndefined();

    expect(errorLog).toHaveBeenCalledWith(
      "snapshot cache write failed",
      "fi",
      "2026-07-24",
      expect.any(Error)
    );
    expect(errorLog).toHaveBeenCalledWith(
      "snapshot cache write failed",
      "en",
      "2026-07-24",
      expect.any(Error)
    );
  });

  it("supports dated manual offer overrides without a client update", () => {
    const restaurant = restaurantConfiguration("pranzeria-sorrento");
    expect(restaurant).toBeDefined();
    if (!restaurant) return;
    const configured = {
      ...restaurant,
      offerOverrides: [
        {
          startsOn: "2026-08-01",
          endsOn: "2026-12-31",
          offers: [
            {
              id: "lunch-buffet",
              label: { fi: "Lounasbuffet", en: "Lunch buffet" },
              amount: "14.50"
            }
          ]
        }
      ]
    };
    expect(
      configuredOffers(
        configured,
        [
          {
            id: "old",
            label: "Old",
            price: { amount: "1.00", currency: "EUR" }
          }
        ],
        "en",
        "2026-09-01"
      )
    ).toEqual([
      {
        id: "lunch-buffet",
        label: "Lunch buffet",
        price: { amount: "14.50", currency: "EUR" }
      }
    ]);
  });
});
