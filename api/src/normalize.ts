import type {
  LunchItem,
  MenuGroup,
  MenuPrice,
  PriceAudience
} from "./types";

const audienceLabels: Array<[PriceAudience, RegExp]> = [
  ["student", /\b(?:opiskelija|opisk|op|student)\b/i],
  ["staff", /\b(?:henkilökunta|henkilokunta|staff|hk)\b/i],
  ["guest", /\b(?:vierailija|vieras|guest)\b/i]
];

const tagPattern = /^(?:\*|[A-ZÅÄÖ]{1,4}|Veg|VS|ILM)$/i;

export function normalizeText(value: unknown): string {
  return typeof value === "string"
    ? value.replace(/\s+/gu, " ").trim()
    : "";
}

export function decodeHtml(value: string): string {
  const named: Record<string, string> = {
    amp: "&",
    apos: "'",
    gt: ">",
    lt: "<",
    nbsp: " ",
    quot: "\""
  };
  return value
    .replace(/<!\[CDATA\[([\s\S]*?)\]\]>/gi, "$1")
    .replace(/&#(\d+);/g, (_, digits: string) =>
      String.fromCodePoint(Number.parseInt(digits, 10))
    )
    .replace(/&#x([0-9a-f]+);/gi, (_, digits: string) =>
      String.fromCodePoint(Number.parseInt(digits, 16))
    )
    .replace(/&([a-z]+);/gi, (entity, name: string) =>
      named[name.toLowerCase()] ?? entity
    );
}

export function htmlText(value: string): string {
  return normalizeText(
    decodeHtml(
      value
        .replace(/<br\s*\/?>/gi, "\n")
        .replace(/<[^>]+>/g, " ")
    )
  );
}

export function stableResponseId(prefix: string, index: number): string {
  return `${prefix}-${index + 1}`;
}

function normalizedAmount(value: string): string | undefined {
  const match = value.match(/\d+(?:[.,]\d+)?/);
  if (!match) return undefined;
  const parsed = Number.parseFloat(match[0].replace(",", "."));
  if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1000) return undefined;
  return parsed.toFixed(2);
}

function splitPriceSegments(value: string): string[] {
  const clean = normalizeText(value)
    .replace(/\bEUR\b/gi, "€")
    .replace(/\s*\/\s*/g, " / ");
  const slashSegments = clean.split("/").map(normalizeText).filter(Boolean);
  if (slashSegments.length > 1) return slashSegments;

  const labels =
    /\b(?:opiskelija|opisk|op|student|henkilökunta|henkilokunta|staff|hk|vierailija|vieras|guest)\b/gi;
  const starts = [...clean.matchAll(labels)].map(match => match.index ?? 0);
  if (starts.length < 2) return clean ? [clean] : [];
  return starts
    .map((start, index) =>
      normalizeText(clean.slice(start, starts[index + 1] ?? clean.length))
    )
    .filter(Boolean);
}

function explicitAudience(segment: string): PriceAudience | undefined {
  return audienceLabels.find(([, pattern]) => pattern.test(segment))?.[0];
}

function price(amount: string, audiences?: PriceAudience[]): MenuPrice {
  return {
    amount,
    currency: "EUR",
    ...(audiences?.length ? { audiences } : {})
  };
}

export function parseCompassPrices(
  rawValue: unknown,
  restaurantId: string
): MenuPrice[] {
  const entries = splitPriceSegments(normalizeText(rawValue))
    .map(segment => ({
      amount: normalizedAmount(segment),
      audience: explicitAudience(segment)
    }))
    .filter(
      (entry): entry is { amount: string; audience: PriceAudience | undefined } =>
        entry.amount !== undefined
    );

  if (entries.length === 0) return [];

  if (restaurantId === "tietoteknia") {
    if (entries.length === 1) {
      return [price(entries[0]!.amount, ["student", "staff", "guest"])];
    }

    const explicitStudent = entries.some(entry => entry.audience === "student");
    const unlabelled = entries
      .map((entry, index) => ({ entry, index }))
      .filter(({ entry }) => entry.audience === undefined);
    const inferredStudentIndex = explicitStudent
      ? undefined
      : unlabelled.at(-1)?.index;

    return mergeEquivalentPrices(
      entries.map((entry, index) => {
        if (entry.audience) return price(entry.amount, [entry.audience]);
        return price(
          entry.amount,
          index === inferredStudentIndex
            ? ["student"]
            : ["staff", "guest"]
        );
      })
    );
  }

  return mergeEquivalentPrices(
    entries.map(entry =>
      price(
        entry.amount,
        entry.audience
          ? [entry.audience]
          : ["student", "staff", "guest"]
      )
    )
  );
}

export function parseGeneralPrice(rawValue: unknown): MenuPrice[] {
  const amounts = normalizeText(rawValue).match(/\d+(?:[.,]\d+)?/g) ?? [];
  return amounts
    .map(normalizedAmount)
    .filter((amount): amount is string => amount !== undefined)
    .map(amount => price(amount));
}

function mergeEquivalentPrices(prices: MenuPrice[]): MenuPrice[] {
  const merged = new Map<string, MenuPrice>();
  for (const entry of prices) {
    const existing = merged.get(entry.amount);
    if (!existing) {
      merged.set(entry.amount, {
        ...entry,
        ...(entry.audiences ? { audiences: [...entry.audiences] } : {})
      });
      continue;
    }
    if (!existing.audiences || !entry.audiences) {
      delete existing.audiences;
      continue;
    }
    for (const audience of entry.audiences) {
      if (!existing.audiences.includes(audience)) {
        existing.audiences.push(audience);
      }
    }
  }
  return [...merged.values()];
}

function normalizedTag(value: string): string {
  const upper = value.trim().replace(/[.;:]+$/g, "").toUpperCase();
  return upper === "VEG" ? "Veg" : upper;
}

function uniqueTags(values: string[]): string[] {
  const seen = new Set<string>();
  return values.filter(value => {
    const key = value.toUpperCase();
    if (!value || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function splitItemText(rawValue: unknown): {
  name: string;
  tags: string[];
} {
  let value = normalizeText(rawValue);
  const tags: string[] = [];

  while (value.endsWith(")")) {
    const open = value.lastIndexOf("(");
    if (open < 0) break;
    const candidates = value
      .slice(open + 1, -1)
      .split(/[,;/]/)
      .map(normalizeText)
      .filter(Boolean);
    if (!candidates.length || !candidates.every(token => tagPattern.test(token))) {
      break;
    }
    tags.unshift(...candidates.map(normalizedTag));
    value = normalizeText(value.slice(0, open));
  }

  const commaParts = value.split(",").map(normalizeText);
  while (
    commaParts.length > 1 &&
    tagPattern.test(commaParts.at(-1) ?? "")
  ) {
    tags.unshift(normalizedTag(commaParts.pop() ?? ""));
  }
  value = normalizeText(commaParts.join(", "));

  const trailing = value.match(/^(.*\S)\s+(\*|[A-ZÅÄÖ]{1,4}|Veg|VS|ILM)$/);
  if (trailing?.[1] && trailing[2] && tagPattern.test(trailing[2])) {
    value = normalizeText(trailing[1]);
    tags.unshift(normalizedTag(trailing[2]));
  }

  return { name: value, tags: uniqueTags(tags) };
}

export function itemFromText(
  rawValue: unknown,
  id: string
): LunchItem | undefined {
  const { name, tags } = splitItemText(rawValue);
  if (!name) return undefined;
  return {
    id,
    name,
    ...(tags.length ? { tags } : {})
  };
}

export function validGroups(groups: MenuGroup[]): MenuGroup[] {
  return groups
    .filter(group => group.items.length > 0)
    .map((group, index) => ({
      ...group,
      id: group.id || stableResponseId("group", index),
      sortOrder: Number.isFinite(group.sortOrder) ? group.sortOrder : index,
      prices: group.prices ?? []
    }));
}
