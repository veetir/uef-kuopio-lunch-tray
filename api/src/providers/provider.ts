import type {
  GeneralOffer,
  Language,
  MenuGroup,
  RestaurantConfiguration,
  ServiceStatus
} from "../types";

export interface ParsedProviderMenu {
  contentLanguage: Language;
  status: ServiceStatus;
  hours?: string;
  offers: GeneralOffer[];
  groups: MenuGroup[];
}

export interface ProviderRequest {
  restaurant: RestaurantConfiguration;
  language: Language;
  date: string;
  fetcher?: typeof fetch;
}

export type ProviderAdapter = (
  request: ProviderRequest
) => Promise<ParsedProviderMenu>;

const upstreamUserAgent =
  "UEFKuopioLunchAPI/1.0 (+https://lunch.veeti.dev)";
const upstreamResponseByteLimit = 2 * 1024 * 1024;
const upstreamTimeoutMilliseconds = 10_000;

export function fetchOrDefault(fetcher?: typeof fetch): typeof fetch {
  const baseFetch = fetcher ?? fetch;
  return ((input: RequestInfo | URL, init?: RequestInit) => {
    const headers = new Headers(init?.headers);
    if (!headers.has("User-Agent")) {
      headers.set("User-Agent", upstreamUserAgent);
    }
    return baseFetch(input, {
      ...init,
      headers,
      signal: init?.signal ?? AbortSignal.timeout(upstreamTimeoutMilliseconds)
    });
  }) as typeof fetch;
}

export async function responseText(response: Response): Promise<string> {
  if (!response.ok) {
    throw new Error(`Upstream returned HTTP ${response.status}`);
  }
  const declaredLength = Number(response.headers.get("Content-Length"));
  if (
    Number.isFinite(declaredLength) &&
    declaredLength > upstreamResponseByteLimit
  ) {
    throw new Error("Upstream response exceeded the size limit");
  }
  if (!response.body) return "";

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let received = 0;
  let text = "";
  while (true) {
    const chunk = await reader.read();
    if (chunk.done) break;
    received += chunk.value.byteLength;
    if (received > upstreamResponseByteLimit) {
      await reader.cancel();
      throw new Error("Upstream response exceeded the size limit");
    }
    text += decoder.decode(chunk.value, { stream: true });
  }
  return text + decoder.decode();
}
