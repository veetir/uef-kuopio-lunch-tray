export type Language = "fi" | "en";

export interface LocalizedText {
  fi: string;
  en: string;
}

export interface ClosurePeriod {
  kind: "seasonal" | "exceptional";
  startsOn: string;
  endsOn: string;
  reason?: Partial<LocalizedText>;
}

export interface PublicRestaurant {
  id: string;
  order: number;
  name: LocalizedText;
  websiteUrl: string | null;
  languages: Language[];
  closures: ClosurePeriod[];
}

export interface RestaurantCatalog {
  apiVersion: "v1";
  schemaVersion: 1;
  revision: string;
  restaurants: PublicRestaurant[];
}

export type SourceConfiguration =
  | { type: "compass"; costNumber: string }
  | { type: "compassRss"; costNumber: string }
  | { type: "huomen"; url: string }
  | { type: "antell"; slug: string }
  | { type: "pranzeria"; url: string };

export interface RestaurantConfiguration extends PublicRestaurant {
  source: SourceConfiguration;
  offerOverrides?: OfferOverride[];
}

export interface ApiConfiguration {
  revision: string;
  cacheRevision: string;
  cacheFallbackRevisions?: string[];
  restaurants: RestaurantConfiguration[];
}

export type ServiceStatus = "serving" | "closed" | "noMenu" | "unknown";
export type PriceAudience = "student" | "staff" | "guest";

export interface MenuPrice {
  amount: string;
  currency: "EUR";
  audiences?: PriceAudience[];
}

export interface GeneralOffer {
  id: string;
  label: string;
  price: MenuPrice;
  description?: string;
}

export interface OfferOverride {
  startsOn: string;
  endsOn?: string;
  offers: Array<{
    id: string;
    label: LocalizedText;
    amount: string;
    description?: Partial<LocalizedText>;
  }>;
}

export interface NutritionValue {
  name: string;
  amount: number;
  unit: string;
}

export interface RecipeDetails {
  id: string;
  name?: string;
  ingredients?: string;
  nutritionPer100g?: NutritionValue[];
  co2eKilogramsPer100Grams?: number;
  diets?: string[];
}

export interface LunchItem {
  id: string;
  name: string;
  description?: string;
  tags?: string[];
  notes?: string[];
  recipe?: RecipeDetails;
}

export interface MenuGroup {
  id: string;
  title?: string;
  prices: MenuPrice[];
  items: LunchItem[];
  sortOrder: number;
}

export interface ActiveClosure {
  kind: ClosurePeriod["kind"];
  startsOn: string;
  endsOn: string;
  reason?: string;
}

export interface MenuFreshness {
  fetchedAt: string;
  isStale: boolean;
}

export interface RestaurantMenu {
  apiVersion: "v1";
  schemaVersion: 1;
  restaurant: PublicRestaurant;
  requestedLanguage: Language;
  contentLanguage: Language;
  date: string;
  service: {
    status: ServiceStatus;
    hours?: string;
  };
  closure?: ActiveClosure;
  offers: GeneralOffer[];
  groups: MenuGroup[];
  freshness: MenuFreshness;
}

export interface LunchSnapshot {
  apiVersion: "v1";
  schemaVersion: 1;
  revision: string;
  requestedLanguage: Language;
  date: string;
  restaurants: PublicRestaurant[];
  menus: RestaurantMenu[];
}
