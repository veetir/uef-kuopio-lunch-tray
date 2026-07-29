import {
  cacheRevisions,
  publicRestaurant,
  restaurantCatalog,
  restaurantConfiguration,
  restaurantConfigurations
} from "./catalog";
import {
  closureForDate,
  fetchRestaurantMenu,
  helsinkiDate,
  knownNonServingMenu,
  menuCacheKey,
  snapshotCacheKey,
  staleMenu,
  unknownMenu
} from "./menu";
import type {
  Language,
  LunchSnapshot,
  RestaurantConfiguration,
  RestaurantMenu
} from "./types";
import { validateMenu, validateSnapshot } from "./validate";

export interface Env {
  ENVIRONMENT: string;
  MENU_CACHE: KVNamespace;
  API_CLIENT_RATE_LIMITER?: RateLimit;
  API_GLOBAL_RATE_LIMITER?: RateLimit;
}

type RequestEnvironment = Partial<
  Pick<
    Env,
    "MENU_CACHE" | "API_CLIENT_RATE_LIMITER" | "API_GLOBAL_RATE_LIMITER"
  >
>;

const corsHeaders = {
  "Access-Control-Allow-Headers": "Content-Type, If-None-Match",
  "Access-Control-Allow-Methods": "GET, HEAD, OPTIONS",
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Max-Age": "86400"
};

const securityHeaders = {
  "Content-Security-Policy":
    "default-src 'none'; base-uri 'none'; frame-ancestors 'none'",
  "Permissions-Policy": "camera=(), geolocation=(), microphone=()",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "X-Robots-Tag": "noindex, nofollow, noarchive"
};

const jsonHeaders = {
  ...corsHeaders,
  ...securityHeaders,
  "Cache-Control": "public, max-age=300, stale-while-revalidate=86400",
  "Content-Type": "application/json; charset=utf-8"
};

const noMenuRetryMilliseconds = 30 * 60 * 1000;
const cacheExpirationSeconds = 7 * 24 * 60 * 60;
const refreshStartMinutes = 0;
const refreshEndMinutes = 15 * 60;
const refreshIntervalMinutes = 6;

function json(
  value: unknown,
  status = 200,
  headers: Record<string, string> = {}
): Response {
  const body = JSON.stringify(value);
  const responseHeaders: Record<string, string> = {
    ...jsonHeaders,
    ...headers
  };
  if (status >= 400 && headers["Cache-Control"] === undefined) {
    responseHeaders["Cache-Control"] = "no-store";
  }
  responseHeaders.ETag = weakEntityTag(body);
  return new Response(body, {
    status,
    headers: responseHeaders
  });
}

function weakEntityTag(body: string): string {
  const bytes = new TextEncoder().encode(body);
  let hash = 0x811c9dc5;
  for (const byte of bytes) {
    hash = Math.imul(hash ^ byte, 0x01000193);
  }
  return `W/"${bytes.byteLength.toString(16)}-${(hash >>> 0)
    .toString(16)
    .padStart(8, "0")}"`;
}

function withoutBody(response: Response): Response {
  return new Response(null, {
    status: response.status,
    headers: response.headers
  });
}

function conditionalResponse(
  request: Request,
  response: Response
): Response | undefined {
  if (response.status !== 200) return undefined;
  const requestedTags = request.headers.get("If-None-Match");
  const responseTag = response.headers.get("ETag");
  if (!requestedTags || !responseTag) return undefined;
  const matches = requestedTags
    .split(",")
    .some(tag => tag.trim() === "*" || tag.trim() === responseTag);
  if (!matches) return undefined;
  const headers = new Headers(response.headers);
  headers.delete("Content-Length");
  return new Response(null, { status: 304, headers });
}

export async function handleRequest(
  request: Request,
  environment = "development",
  env?: RequestEnvironment,
  _context?: Pick<ExecutionContext, "waitUntil">
): Promise<Response> {
  const limited = await rateLimitResponse(request, env);
  if (limited) return limited;

  const method = request.method.toUpperCase();
  if (method === "OPTIONS") {
    return new Response(null, {
      status: 204,
      headers: {
        ...corsHeaders,
        "Cache-Control": "public, max-age=86400"
      }
    });
  }
  if (method !== "GET" && method !== "HEAD") {
    return json(
      {
        apiVersion: "v1",
        error: {
          code: "method_not_allowed",
          message: "Only GET, HEAD and OPTIONS are supported."
        }
      },
      405,
      { Allow: "GET, HEAD, OPTIONS" }
    );
  }

  const url = new URL(request.url);
  if (
    url.pathname !== "/" &&
    (url.pathname.endsWith("/") || url.pathname.includes("//"))
  ) {
    return invalidRequest("Use the canonical API path without extra slashes.");
  }
  const path = url.pathname.replace(/\/+$/, "") || "/";
  let response: Response;

  if (path === "/") {
    if (url.search) return invalidRequest("This endpoint has no parameters.");
    response = new Response(
      [
        "<!doctype html>",
        '<html lang="en"><meta charset="utf-8">',
        "<title>UEF Kuopio Lunch API</title>",
        "<body><h1>UEF Kuopio Lunch API</h1>",
        '<p><a href="/v1/restaurants">Restaurant catalogue</a></p>',
        "</body></html>"
      ].join(""),
      {
        headers: {
          ...corsHeaders,
          ...securityHeaders,
          "Cache-Control": "public, max-age=300",
          "Content-Type": "text/html; charset=utf-8"
        }
      }
    );
  } else if (path === "/robots.txt") {
    if (url.search) return invalidRequest("This endpoint has no parameters.");
    response = new Response(
      ["User-agent: *", "Disallow: /"].join("\n"),
      {
        headers: {
          ...corsHeaders,
          ...securityHeaders,
          "Cache-Control": "public, max-age=86400",
          "Content-Type": "text/plain; charset=utf-8"
        }
      }
    );
  } else if (path === "/v1/status") {
    if (url.search) return invalidRequest("This endpoint has no parameters.");
    response = json(
      {
        status: "ok",
        apiVersion: "v1",
        environment
      },
      200,
      { "Cache-Control": "no-store" }
    );
  } else if (path === "/v1/restaurants") {
    if (url.search) return invalidRequest("This endpoint has no parameters.");
    response = json(restaurantCatalog(), 200, {
      "Cache-Control": "public, max-age=86400"
    });
  } else if (path === "/v1/snapshot") {
    response = await snapshotRoute(url, env);
  } else {
    response = await restaurantRoute(path, url, env);
  }

  const notModified = conditionalResponse(request, response);
  if (notModified) return notModified;
  return method === "HEAD" ? withoutBody(response) : response;
}

async function restaurantRoute(
  path: string,
  url: URL,
  env?: RequestEnvironment
): Promise<Response> {
  const match = path.match(/^\/v1\/restaurants\/([^/]+)(?:\/menu)?$/);
  if (!match?.[1]) return notFound();
  let id: string;
  try {
    id = decodeURIComponent(match[1]);
  } catch {
    return invalidRequest("The restaurant ID is not valid.");
  }
  const isMenu = path.endsWith("/menu");
  const restaurant = restaurantConfiguration(id);
  if (!restaurant) {
    return json(
      {
        apiVersion: "v1",
        error: {
          code: "restaurant_not_found",
          message: "The requested restaurant does not exist."
        }
      },
      404
    );
  }
  if (!isMenu) {
    if (url.search) return invalidRequest("This endpoint has no parameters.");
    return json(
      {
        apiVersion: "v1",
        schemaVersion: 1,
        restaurant: publicRestaurant(id)
      },
      200,
      { "Cache-Control": "public, max-age=86400" }
    );
  }

  const parameters = menuParameters(url);
  if (parameters instanceof Response) return parameters;
  const { language, date } = parameters;

  const cache = env?.MENU_CACHE;
  const cached = cache
    ? await readMenuForRequest(cache, restaurant, language, date)
    : undefined;
  if (cached) return json(cached);

  const fallback = isScheduledNonServingDay(restaurant, new Date())
    ? knownNonServingMenu(restaurant, language, date)
    : unknownMenu(restaurant, language, date);
  return json(fallback, 200, {
    "Cache-Control": fallback.service.status === "unknown"
      ? "public, max-age=60, stale-while-revalidate=300"
      : "public, max-age=300, stale-while-revalidate=86400"
  });
}

async function snapshotRoute(
  url: URL,
  env?: RequestEnvironment
): Promise<Response> {
  const parameters = menuParameters(url);
  if (parameters instanceof Response) return parameters;
  const { language, date } = parameters;
  const cache = env?.MENU_CACHE;

  const cached = cache
    ? await readCachedSnapshot(cache, snapshotCacheKey(language, date))
    : undefined;
  const snapshot =
    cached ?? await buildSnapshot(cache, language, date);
  const hasUnknown = snapshot.menus.some(
    menu => menu.service.status === "unknown"
  );
  return json(snapshot, 200, {
    "Cache-Control": hasUnknown
      ? "public, max-age=60, stale-while-revalidate=300"
      : "public, max-age=300, stale-while-revalidate=86400"
  });
}

function menuParameters(
  url: URL
): { language: Language; date: string } | Response {
  const language = languageParameter(url.searchParams.get("language"));
  const date = url.searchParams.get("date");
  if (
    !language ||
    !date ||
    url.search !== `?language=${language}&date=${date}` ||
    !/^\d{4}-\d{2}-\d{2}$/.test(date)
  ) {
    return json(
      {
        apiVersion: "v1",
        error: {
          code: "invalid_parameter",
          message:
            "Use exactly ?language=fi|en&date=YYYY-MM-DD in that order."
        }
      },
      400
    );
  }
  if (date !== helsinkiDate()) {
    return json(
      {
        apiVersion: "v1",
        error: {
          code: "date_not_available",
          message: "Only the current Helsinki date is available."
        }
      },
      400
    );
  }
  return { language, date };
}

async function refreshMenu(
  cache: KVNamespace | undefined,
  restaurant: ReturnType<typeof restaurantConfigurations>[number],
  language: Language,
  date: string
): Promise<RestaurantMenu> {
  const menu = await fetchRestaurantMenu(restaurant, language, date);
  validateMenu(menu);
  if (cache && menu.service.status !== "unknown") {
    try {
      await cache.put(
        menuCacheKey(restaurant.id, language, date),
        JSON.stringify(menu),
        { expirationTtl: cacheExpirationSeconds }
      );
    } catch (error) {
      console.error(
        "menu cache write failed",
        restaurant.id,
        language,
        date,
        error
      );
    }
  }
  return menu;
}

export function shouldRefreshCachedMenu(
  menu: RestaurantMenu,
  now = new Date(),
  restaurant?: RestaurantConfiguration
): boolean {
  if (menu.freshness.isStale) return true;
  if (menu.service.status === "serving") return false;
  if (menu.service.status === "closed") {
    if (
      restaurant &&
      menu.closure &&
      !closureForDate(
        restaurant,
        menu.requestedLanguage,
        menu.date
      )
    ) {
      return true;
    }
    return false;
  }
  if (menu.service.status === "unknown") return true;
  const fetchedAt = Date.parse(menu.freshness.fetchedAt);
  return (
    !Number.isFinite(fetchedAt) ||
    now.getTime() - fetchedAt >= noMenuRetryMilliseconds
  );
}

async function readCachedMenu(
  cache: KVNamespace,
  key: string
): Promise<RestaurantMenu | undefined> {
  const raw = await cache.get(key);
  if (!raw) return undefined;
  try {
    const menu = JSON.parse(raw) as RestaurantMenu;
    validateMenu(menu);
    return menu;
  } catch (error) {
    console.error("invalid cached menu", key, error);
    return undefined;
  }
}

function cacheLanguage(
  restaurant: RestaurantConfiguration,
  requestedLanguage: Language
): Language {
  return restaurant.languages.includes(requestedLanguage)
    ? requestedLanguage
    : (restaurant.languages[0] ?? "fi");
}

async function readMenuForRequest(
  cache: KVNamespace,
  restaurant: RestaurantConfiguration,
  requestedLanguage: Language,
  date: string
): Promise<RestaurantMenu | undefined> {
  if (closureForDate(restaurant, requestedLanguage, date)) {
    return knownNonServingMenu(restaurant, requestedLanguage, date);
  }
  const storedLanguage = cacheLanguage(restaurant, requestedLanguage);
  const revisions = cacheRevisions();
  for (const [index, revision] of revisions.entries()) {
    const cached = await readCachedMenu(
      cache,
      menuCacheKey(restaurant.id, storedLanguage, date, revision)
    );
    if (!cached) continue;
    const adapted = {
      ...cached,
      restaurant: publicRestaurant(restaurant.id) ?? cached.restaurant,
      requestedLanguage
    };
    const result = index === 0 ? adapted : staleMenu(adapted);
    validateMenu(result);
    return result;
  }
  return undefined;
}

async function buildSnapshot(
  cache: KVNamespace | undefined,
  language: Language,
  date: string,
  now = new Date()
): Promise<LunchSnapshot> {
  const catalog = restaurantCatalog();
  const configurations = restaurantConfigurations();
  const menus = await Promise.all(
    configurations.map(async restaurant =>
      cache
        ? (await readMenuForRequest(cache, restaurant, language, date)) ??
          missingMenuForSchedule(restaurant, language, date, now)
        : missingMenuForSchedule(restaurant, language, date, now)
    )
  );
  const snapshot: LunchSnapshot = {
    apiVersion: "v1",
    schemaVersion: 1,
    revision: catalog.revision,
    requestedLanguage: language,
    date,
    restaurants: catalog.restaurants,
    menus
  };
  validateSnapshot(snapshot);
  return snapshot;
}

async function readCachedSnapshot(
  cache: KVNamespace,
  key: string
): Promise<LunchSnapshot | undefined> {
  const raw = await cache.get(key);
  if (!raw) return undefined;
  try {
    const snapshot = JSON.parse(raw) as LunchSnapshot;
    validateSnapshot(snapshot);
    return snapshot;
  } catch (error) {
    console.error("invalid cached snapshot", key, error);
    return undefined;
  }
}

async function writeSnapshots(
  cache: KVNamespace,
  date: string,
  now: Date
): Promise<void> {
  await Promise.all(
    (["fi", "en"] as const).map(async language => {
      try {
        const snapshot = await buildSnapshot(cache, language, date, now);
        const key = snapshotCacheKey(language, date);
        const existing = await readCachedSnapshot(cache, key);
        if (existing && snapshotsEquivalent(existing, snapshot)) return;
        await cache.put(
          key,
          JSON.stringify(snapshot),
          { expirationTtl: cacheExpirationSeconds }
        );
      } catch (error) {
        console.error("snapshot cache write failed", language, date, error);
      }
    })
  );
}

function snapshotsEquivalent(
  left: LunchSnapshot,
  right: LunchSnapshot
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function languageParameter(value: string | null): Language | undefined {
  const normalized = value ?? "fi";
  return normalized === "fi" || normalized === "en"
    ? normalized
    : undefined;
}

function notFound(): Response {
  return json(
    {
      apiVersion: "v1",
      error: {
        code: "not_found",
        message: "The requested endpoint does not exist."
      }
    },
    404
  );
}

function invalidRequest(message: string): Response {
  return json(
    {
      apiVersion: "v1",
      error: {
        code: "invalid_request",
        message
      }
    },
    400
  );
}

async function rateLimitResponse(
  request: Request,
  env?: RequestEnvironment
): Promise<Response | undefined> {
  try {
    if (env?.API_GLOBAL_RATE_LIMITER) {
      const result = await env.API_GLOBAL_RATE_LIMITER.limit({
        key: "public-v1"
      });
      if (!result.success) return tooManyRequests();
    }

    const clientIP = request.headers.get("cf-connecting-ip");
    if (clientIP && env?.API_CLIENT_RATE_LIMITER) {
      const result = await env.API_CLIENT_RATE_LIMITER.limit({
        key: clientIP
      });
      if (!result.success) return tooManyRequests();
    }
  } catch (error) {
    console.error("rate limiter failed", error);
  }

  return undefined;
}

function tooManyRequests(): Response {
  return json(
    {
      apiVersion: "v1",
      error: {
        code: "rate_limited",
        message: "Too many requests. Try again shortly."
      }
    },
    429,
    {
      "Cache-Control": "no-store",
      "Retry-After": "60"
    }
  );
}

function helsinkiMinutes(now: Date): number {
  const parts = new Intl.DateTimeFormat("en-GB", {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    timeZone: "Europe/Helsinki"
  }).formatToParts(now);
  const hour = Number(parts.find(part => part.type === "hour")?.value);
  const minute = Number(parts.find(part => part.type === "minute")?.value);
  return hour * 60 + minute;
}

function helsinkiWeekday(now: Date): string {
  return new Intl.DateTimeFormat("en-US", {
    weekday: "short",
    timeZone: "Europe/Helsinki"
  }).format(now);
}

export async function scheduledRefresh(
  event: ScheduledController,
  env: Env
): Promise<void> {
  if (env.ENVIRONMENT !== "production") return;
  const now = new Date(event.scheduledTime);
  const selected = scheduledRestaurants(now);
  const date = helsinkiDate(now);

  await Promise.allSettled(
    selected.flatMap(restaurant =>
      [...new Set(restaurant.languages)].map(async language => {
        const cached = await readCachedMenu(
          env.MENU_CACHE,
          menuCacheKey(restaurant.id, language, date)
        );
        if (cached && !shouldRefreshCachedMenu(cached, now, restaurant)) return;
        await refreshMenu(env.MENU_CACHE, restaurant, language, date);
      })
    )
  );
  if (helsinkiMinutes(now) < refreshEndMinutes) {
    await writeSnapshots(env.MENU_CACHE, date, now);
  }
}

function isScheduledNonServingDay(
  restaurant: RestaurantConfiguration,
  now: Date
): boolean {
  const weekday = helsinkiWeekday(now);
  return weekday === "Sun" ||
    (weekday === "Sat" && restaurant.id !== "snellmania");
}

function missingMenuForSchedule(
  restaurant: RestaurantConfiguration,
  language: Language,
  date: string,
  now: Date
): RestaurantMenu {
  return isScheduledNonServingDay(restaurant, now)
    ? knownNonServingMenu(restaurant, language, date, now)
    : unknownMenu(restaurant, language, date, now);
}

export function scheduledRestaurants(
  now: Date
): ReturnType<typeof restaurantConfigurations> {
  const minutes = helsinkiMinutes(now);
  if (minutes < refreshStartMinutes || minutes >= refreshEndMinutes) return [];
  const restaurants = restaurantConfigurations();
  const slot = Math.floor(
    (minutes - refreshStartMinutes) / refreshIntervalMinutes
  );
  const restaurant = restaurants[slot % restaurants.length];
  if (restaurant && isScheduledNonServingDay(restaurant, now)) return [];
  return restaurant ? [restaurant] : [];
}

export default {
  fetch(request: Request, env: Env, context: ExecutionContext): Promise<Response> {
    return handleRequest(request, env.ENVIRONMENT, env, context);
  },
  scheduled(
    event: ScheduledController,
    env: Env,
    context: ExecutionContext
  ): void {
    context.waitUntil(scheduledRefresh(event, env));
  }
} satisfies ExportedHandler<Env>;
