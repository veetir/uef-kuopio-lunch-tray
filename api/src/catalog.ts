import rawConfiguration from "../config/restaurants.json";
import type {
  ApiConfiguration,
  PublicRestaurant,
  RestaurantCatalog,
  RestaurantConfiguration
} from "./types";

const isoDate = /^\d{4}-\d{2}-\d{2}$/;
const configuration = rawConfiguration as ApiConfiguration;
assertValidClosures(configuration.restaurants);

export function assertValidClosures(
  restaurants: Array<Pick<RestaurantConfiguration, "id" | "closures">>
): void {
  for (const restaurant of restaurants) {
    const closures = [...restaurant.closures].sort((left, right) =>
      left.startsOn.localeCompare(right.startsOn)
    );
    for (const [index, closure] of closures.entries()) {
      if (
        !isoDate.test(closure.startsOn) ||
        !isoDate.test(closure.endsOn) ||
        closure.startsOn > closure.endsOn
      ) {
        throw new Error(`Invalid closure for ${restaurant.id}`);
      }
      const previous = closures[index - 1];
      if (previous && closure.startsOn <= previous.endsOn) {
        throw new Error(`Overlapping closures for ${restaurant.id}`);
      }
    }
  }
}

function toPublicRestaurant(
  restaurant: RestaurantConfiguration
): PublicRestaurant {
  const {
    id,
    order,
    name,
    websiteUrl,
    languages,
    closures
  } = restaurant;
  return { id, order, name, websiteUrl, languages, closures };
}

export function restaurantConfigurations(): RestaurantConfiguration[] {
  return [...configuration.restaurants].sort(
    (left, right) => left.order - right.order
  );
}

export function restaurantCatalog(): RestaurantCatalog {
  return {
    apiVersion: "v1",
    schemaVersion: 1,
    revision: configuration.revision,
    restaurants: restaurantConfigurations().map(toPublicRestaurant)
  };
}

export function cacheRevisions(): string[] {
  return [
    configuration.cacheRevision,
    ...(configuration.cacheFallbackRevisions ?? [])
  ];
}

export function publicRestaurant(id: string): PublicRestaurant | undefined {
  const restaurant = configuration.restaurants.find(
    candidate => candidate.id === id
  );
  return restaurant ? toPublicRestaurant(restaurant) : undefined;
}

export function restaurantConfiguration(
  id: string
): RestaurantConfiguration | undefined {
  return configuration.restaurants.find(candidate => candidate.id === id);
}
