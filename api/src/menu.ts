import {
  cacheRevisions,
  publicRestaurant
} from "./catalog";
import type {
  ActiveClosure,
  Language,
  RestaurantConfiguration,
  RestaurantMenu
} from "./types";
import { fetchAntellMenu } from "./providers/antell";
import { fetchCompassMenu } from "./providers/compass";
import { fetchHuomenMenu } from "./providers/huomen";
import type {
  ParsedProviderMenu,
  ProviderAdapter
} from "./providers/provider";
import { fetchCompassRssMenu } from "./providers/rss";
import { fetchSorrentoMenu } from "./providers/sorrento";
import { validateMenu } from "./validate";

const providers: Record<RestaurantConfiguration["source"]["type"], ProviderAdapter> = {
  antell: fetchAntellMenu,
  compass: fetchCompassMenu,
  compassRss: fetchCompassRssMenu,
  huomen: fetchHuomenMenu,
  pranzeria: fetchSorrentoMenu
};

const providerFetchTimeoutMilliseconds = 15_000;

const timedProviderFetch: typeof fetch = async (input, init) => {
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort("Upstream request timed out"),
    providerFetchTimeoutMilliseconds
  );
  try {
    return await fetch(input, {
      ...init,
      signal: controller.signal
    });
  } finally {
    clearTimeout(timeout);
  }
};

export async function fetchRestaurantMenu(
  restaurant: RestaurantConfiguration,
  language: Language,
  date: string,
  options: { fetcher?: typeof fetch; now?: Date } = {}
): Promise<RestaurantMenu> {
  const now = options.now ?? new Date();
  const activeClosure = closureForDate(restaurant, language, date);
  let parsed: ParsedProviderMenu;
  if (activeClosure) {
    parsed = {
      contentLanguage: restaurant.languages.includes(language)
        ? language
        : (restaurant.languages[0] ?? "fi"),
      status: "closed",
      offers: [],
      groups: []
    };
  } else {
    parsed = await providers[restaurant.source.type]({
      restaurant,
      language,
      date,
      fetcher: options.fetcher ?? timedProviderFetch
    });
    parsed.offers = configuredOffers(
      restaurant,
      parsed.offers,
      language,
      date
    );
  }

  const publicValue = publicRestaurant(restaurant.id);
  if (!publicValue) throw new Error("Restaurant is missing from catalogue");
  const menu: RestaurantMenu = {
    apiVersion: "v1",
    schemaVersion: 1,
    restaurant: publicValue,
    requestedLanguage: language,
    contentLanguage: parsed.contentLanguage,
    date,
    service: {
      status: parsed.status,
      ...(parsed.hours ? { hours: parsed.hours } : {})
    },
    ...(activeClosure ? { closure: activeClosure } : {}),
    offers: parsed.offers,
    groups: parsed.groups,
    freshness: {
      fetchedAt: now.toISOString(),
      isStale: false
    }
  };
  validateMenu(menu);
  return menu;
}

export function configuredOffers(
  restaurant: RestaurantConfiguration,
  providerOffers: ParsedProviderMenu["offers"],
  language: Language,
  date: string
): ParsedProviderMenu["offers"] {
  const override = restaurant.offerOverrides?.find(
    candidate =>
      candidate.startsOn <= date &&
      (candidate.endsOn === undefined || candidate.endsOn >= date)
  );
  if (!override) return providerOffers;
  return override.offers.map(offer => ({
    id: offer.id,
    label: offer.label[language] || offer.label.fi || offer.label.en,
    price: {
      amount: offer.amount,
      currency: "EUR"
    },
    ...(offer.description?.[language] ||
    offer.description?.fi ||
    offer.description?.en
      ? {
          description:
            offer.description?.[language] ??
            offer.description?.fi ??
            offer.description?.en
        }
      : {})
  }));
}

export function unknownMenu(
  restaurant: RestaurantConfiguration,
  language: Language,
  date: string,
  _now = new Date()
): RestaurantMenu {
  const publicValue = publicRestaurant(restaurant.id);
  if (!publicValue) throw new Error("Restaurant is missing from catalogue");
  return {
    apiVersion: "v1",
    schemaVersion: 1,
    restaurant: publicValue,
    requestedLanguage: language,
    contentLanguage: restaurant.languages.includes(language)
      ? language
      : (restaurant.languages[0] ?? "fi"),
    date,
    service: { status: "unknown" },
    offers: [],
    groups: [],
    freshness: {
      fetchedAt: `${date}T00:00:00.000Z`,
      isStale: false
    }
  };
}

export function knownNonServingMenu(
  restaurant: RestaurantConfiguration,
  language: Language,
  date: string,
  _now = new Date()
): RestaurantMenu {
  const activeClosure = closureForDate(restaurant, language, date);
  const publicValue = publicRestaurant(restaurant.id);
  if (!publicValue) throw new Error("Restaurant is missing from catalogue");
  return {
    apiVersion: "v1",
    schemaVersion: 1,
    restaurant: publicValue,
    requestedLanguage: language,
    contentLanguage: restaurant.languages.includes(language)
      ? language
      : (restaurant.languages[0] ?? "fi"),
    date,
    service: { status: activeClosure ? "closed" : "noMenu" },
    ...(activeClosure ? { closure: activeClosure } : {}),
    offers: [],
    groups: [],
    freshness: {
      fetchedAt: `${date}T00:00:00.000Z`,
      isStale: false
    }
  };
}

export function staleMenu(menu: RestaurantMenu): RestaurantMenu {
  return {
    ...menu,
    freshness: {
      ...menu.freshness,
      isStale: true
    }
  };
}

export function closureForDate(
  restaurant: RestaurantConfiguration,
  language: Language,
  date: string
): ActiveClosure | undefined {
  const closure = restaurant.closures.find(
    candidate => candidate.startsOn <= date && candidate.endsOn >= date
  );
  if (!closure) return undefined;
  const reason =
    closure.reason?.[language] ??
    closure.reason?.fi ??
    closure.reason?.en;
  return {
    kind: closure.kind,
    startsOn: closure.startsOn,
    endsOn: closure.endsOn,
    ...(reason ? { reason } : {})
  };
}

export function menuCacheKey(
  restaurantId: string,
  language: Language,
  date: string,
  cacheRevision = cacheRevisions()[0]
): string {
  return `v1:menu:${cacheRevision}:${restaurantId}:${date}:${language}`;
}

export function snapshotCacheKey(
  language: Language,
  date: string
): string {
  return `v1:snapshot:${cacheRevisions()[0]}:${date}:${language}`;
}

export function helsinkiDate(now = new Date()): string {
  return new Intl.DateTimeFormat("en-CA", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    timeZone: "Europe/Helsinki"
  }).format(now);
}
