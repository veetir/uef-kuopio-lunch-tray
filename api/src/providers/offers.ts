import { htmlText, normalizeText } from "../normalize";
import type { GeneralOffer, Language } from "../types";

export interface OfferDefinition {
  id: string;
  labels: Record<Language, string>;
  patterns: string[];
}

export function extractGeneralOffers(
  html: string,
  language: Language,
  definitions: OfferDefinition[]
): GeneralOffer[] {
  const text = htmlText(
    html
      .replace(/<\/(?:p|div|li|h[1-6])>/gi, "\n")
      .replace(/<br\s*\/?>/gi, "\n")
  );
  const offers: GeneralOffer[] = [];
  for (const definition of definitions) {
    const labelPattern = definition.patterns
      .map(escapeRegularExpression)
      .join("|");
    const expression = new RegExp(
      `(?:^|[^A-Za-zÅÄÖåäö])(?:${labelPattern})\\b[^\\d]{0,30}(\\d{1,3}[,.]\\d{1,3})\\s*(?:€|EUR)(?:\\s*\\(([^)]{1,300})\\))?`,
      "i"
    );
    const match = expression.exec(text);
    if (!match?.[1]) continue;
    const amount = normalizeOfferAmount(match[1]);
    if (!amount) continue;
    const description = normalizeText(match[2]).replace(/^SIS\.\s*/i, "");
    offers.push({
      id: definition.id,
      label: definition.labels[language],
      price: { amount, currency: "EUR" },
      ...(description ? { description } : {})
    });
  }
  return offers;
}

export function normalizeOfferAmount(value: unknown): string | undefined {
  const clean = normalizeText(value);
  const match = clean.match(/\d+(?:[.,]\d+)?/);
  if (!match) return undefined;
  const amount = Number.parseFloat(match[0].replace(",", "."));
  if (!Number.isFinite(amount) || amount < 0 || amount > 1000) return undefined;
  return amount.toFixed(2);
}

function escapeRegularExpression(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
