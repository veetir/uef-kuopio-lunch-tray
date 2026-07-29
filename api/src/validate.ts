import type { LunchSnapshot, RestaurantMenu } from "./types";

const isoDate = /^\d{4}-\d{2}-\d{2}$/;
const decimalAmount = /^\d+\.\d{2}$/;

export function validateMenu(menu: RestaurantMenu): void {
  if (
    menu.apiVersion !== "v1" ||
    menu.schemaVersion !== 1 ||
    !isoDate.test(menu.date)
  ) {
    throw new Error("Invalid menu envelope");
  }
  if (!menu.restaurant.id || !menu.restaurant.name.fi || !menu.restaurant.name.en) {
    throw new Error("Invalid restaurant identity");
  }
  if (!["serving", "closed", "noMenu", "unknown"].includes(menu.service.status)) {
    throw new Error("Invalid service status");
  }
  if (menu.closure) {
    if (
      !isoDate.test(menu.closure.startsOn) ||
      !isoDate.test(menu.closure.endsOn) ||
      menu.closure.startsOn > menu.closure.endsOn
    ) {
      throw new Error("Invalid closure period");
    }
  }
  for (const offer of menu.offers) {
    validateAmount(offer.price.amount);
    if (!offer.id || !offer.label) throw new Error("Invalid general offer");
  }
  const groupIds = new Set<string>();
  for (const group of menu.groups) {
    if (!group.id || groupIds.has(group.id) || group.items.length === 0) {
      throw new Error("Invalid menu group");
    }
    groupIds.add(group.id);
    for (const entry of group.prices) {
      validateAmount(entry.amount);
    }
    const itemIds = new Set<string>();
    for (const item of group.items) {
      if (!item.id || !item.name || itemIds.has(item.id)) {
        throw new Error("Invalid lunch item");
      }
      itemIds.add(item.id);
    }
  }
  if (!Number.isFinite(Date.parse(menu.freshness.fetchedAt))) {
    throw new Error("Invalid freshness timestamp");
  }
}

export function validateSnapshot(snapshot: LunchSnapshot): void {
  if (
    snapshot.apiVersion !== "v1" ||
    snapshot.schemaVersion !== 1 ||
    !snapshot.revision ||
    !["fi", "en"].includes(snapshot.requestedLanguage) ||
    !isoDate.test(snapshot.date)
  ) {
    throw new Error("Invalid snapshot envelope");
  }
  if (
    snapshot.restaurants.length === 0 ||
    snapshot.restaurants.length !== snapshot.menus.length
  ) {
    throw new Error("Invalid snapshot contents");
  }

  const restaurantIds = new Set<string>();
  for (const [index, restaurant] of snapshot.restaurants.entries()) {
    if (!restaurant.id || restaurantIds.has(restaurant.id)) {
      throw new Error("Invalid snapshot restaurant");
    }
    restaurantIds.add(restaurant.id);

    const menu = snapshot.menus[index];
    if (
      !menu ||
      menu.restaurant.id !== restaurant.id ||
      menu.requestedLanguage !== snapshot.requestedLanguage ||
      menu.date !== snapshot.date
    ) {
      throw new Error("Snapshot menu does not match its restaurant");
    }
    validateMenu(menu);
  }
}

function validateAmount(amount: string): void {
  if (!decimalAmount.test(amount)) {
    throw new Error(`Invalid decimal amount: ${amount}`);
  }
}
