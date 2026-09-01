import type { WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";
import type { PaymentEvent } from "./orderDealTimeline.js";

export type LiveTopProduct = {
  id: string;
  name: string;
  mentionCount: number;
  orderCount: number;
  /** Which metric drives the bar: transcript mentions, or order-count fallback. */
  metric: "mention" | "order";
};

export type LiveProductCatalogItem = {
  id: string;
  name: string;
  aliases: string[];
};

export type LiveProductMentionScanItem = {
  id: string;
  name: string;
  mentionCount: number;
};

export type LiveProductMentionScanSnapshot = {
  version: number;
  videoId: number;
  catalogSignature: string;
  status: "idle" | "processing" | "completed" | "failed";
  processedDurationSec: number;
  totalDurationSec: number;
  products: LiveProductMentionScanItem[];
  error?: string | null;
};

const CONDITION_PREFIX = /^(?:\s*(?:99|98|95|9|8|7)\s*新|\s*准新|\s*全新)\s*/i;
const SHOP_BRACKET_PREFIX = /^(?:\s*[【\[][^】\]]*[】\]]\s*)+/;
const GENERIC_MODEL_TOKENS = new Set([
  "99", "98", "95", "9", "8", "7", "f1", "f2", "ii", "iii", "iv",
  "is", "usm", "stm", "oss", "vr", "gm", "rf", "fe", "ef", "xf",
]);
const BRAND_ZH: Record<string, string> = {
  sony: "索尼",
  canon: "佳能",
  nikon: "尼康",
  fujifilm: "富士",
  fuji: "富士",
  panasonic: "松下",
  olympus: "奥林巴斯",
  leica: "徕卡",
  sigma: "适马",
  tamron: "腾龙",
};

function normalize(value: string): string {
  return value
    .toLocaleLowerCase("zh-CN")
    .replace(/[\s\-_—–/\\·.,，。:：;；()（）\[\]【】+]/g, "");
}

export function cleanLiveProductName(value: string): string {
  return value
    .replace(SHOP_BRACKET_PREFIX, "")
    .replace(CONDITION_PREFIX, "")
    .replace(/[|｜].*$/, "")
    .replace(/\s+/g, " ")
    .trim();
}

function modelTokens(value: string): string[] {
  const compact = value
    .toLocaleLowerCase("zh-CN")
    .replace(/(\d)\s*[-–—]\s*(\d)/g, "$1-$2");
  const matches = compact.match(/[a-z]*\d+(?:[-.]\d+)*(?:mm)?[a-z]*/g) ?? [];
  return [...new Set(matches
    .map(normalize)
    .filter((token) => token.length >= 2 && !GENERIC_MODEL_TOKENS.has(token)))];
}

function brandAliases(value: string): string[] {
  const brands = new Set<string>();
  const slashZh = value.match(/[A-Za-z][A-Za-z0-9.+-]{1,24}\s*[/／]\s*([\u4e00-\u9fff]{2,8})/g) ?? [];
  for (const chunk of slashZh) {
    const zh = chunk.split(/[/／]/).pop()?.trim();
    if (zh && zh.length >= 2) brands.add(normalize(zh));
  }
  const latin = value.toLocaleLowerCase("zh-CN").match(/\b[a-z]{3,12}\b/g) ?? [];
  for (const token of latin) {
    const zh = BRAND_ZH[token];
    if (zh) brands.add(normalize(zh));
  }
  return [...brands];
}

function productAliases(name: string): string[] {
  const cleaned = cleanLiveProductName(name);
  const normalizedName = normalize(cleaned);
  const tokens = modelTokens(cleaned);
  const brands = brandAliases(cleaned);
  const aliases = new Set<string>();
  if (normalizedName.length >= 4) aliases.add(normalizedName);
  for (const token of tokens) {
    aliases.add(token);
    if (token.endsWith("mm") && token.length > 3) aliases.add(token.slice(0, -2));
    for (const brand of brands) {
      aliases.add(`${brand}${token}`);
      if (token.endsWith("mm") && token.length > 3) aliases.add(`${brand}${token.slice(0, -2)}`);
    }
  }

  // Adjacent model tokens are more precise than a brand-only or mount-only word.
  for (let index = 0; index < tokens.length - 1; index += 1) {
    aliases.add(`${tokens[index]}${tokens[index + 1]}`);
  }
  return [...aliases].sort((left, right) => right.length - left.length);
}

export function buildLiveProductCatalog(events: readonly PaymentEvent[]): LiveProductCatalogItem[] {
  const grouped = new Map<string, { id: string; name: string; aliases: Set<string> }>();
  for (const event of events) {
    const rawName = event.productName?.trim();
    if (!rawName) continue;
    const name = cleanLiveProductName(rawName);
    const aliases = productAliases(rawName);
    if (!name || !aliases.length) continue;
    const id = normalize(name);
    const existing = grouped.get(id);
    if (existing) {
      aliases.forEach((alias) => existing.aliases.add(alias));
    } else {
      grouped.set(id, { id, name, aliases: new Set(aliases) });
    }
  }

  const aliasOwners = new Map<string, number>();
  for (const product of grouped.values()) {
    for (const alias of product.aliases) {
      aliasOwners.set(alias, (aliasOwners.get(alias) ?? 0) + 1);
    }
  }
  return [...grouped.values()]
    .map((product) => ({
      id: product.id,
      name: product.name,
      aliases: [...product.aliases]
        .filter((alias) => aliasOwners.get(alias) === 1)
        .sort((left, right) => right.length - left.length),
    }))
    .filter((product) => product.aliases.length > 0)
    .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
}

export function liveProductCatalogSignature(catalog: readonly LiveProductCatalogItem[]): string {
  return catalog.map((product) => product.id).sort().join("|");
}

function countOccurrences(text: string, needle: string): number {
  if (!needle) return 0;
  let count = 0;
  let cursor = 0;
  while (cursor <= text.length - needle.length) {
    const index = text.indexOf(needle, cursor);
    if (index < 0) break;
    count += 1;
    cursor = index + needle.length;
  }
  return count;
}

function mentionCount(entries: readonly WorkspaceTranscriptEntry[], aliases: readonly string[]): number {
  if (!aliases.length) return 0;
  return entries.reduce((total, entry) => {
    const text = normalize(entry.text);
    const strongestCount = aliases.reduce(
      (max, alias) => Math.max(max, countOccurrences(text, alias)),
      0,
    );
    return total + strongestCount;
  }, 0);
}

/**
 * Rank products mentioned in the currently available transcript.
 * Payment events supply the product dictionary; they do not increase mention counts.
 * When the transcript has no alias hits, fall back to TOP by order count.
 */
export function extractLiveTopProducts(
  entries: readonly WorkspaceTranscriptEntry[],
  events: readonly PaymentEvent[],
  limit = 5,
): LiveTopProduct[] {
  if (!events.length || limit <= 0) return [];

  const products = new Map<string, { id: string; name: string; orderCount: number; aliases: Set<string> }>();
  for (const event of events) {
    const rawName = event.productName?.trim();
    if (!rawName) continue;
    const name = cleanLiveProductName(rawName);
    const aliases = productAliases(rawName);
    if (!name || !aliases.length) continue;
    const id = normalize(name);
    const existing = products.get(id);
    if (existing) {
      existing.orderCount += 1;
      aliases.forEach((alias) => existing.aliases.add(alias));
    } else {
      products.set(id, { id, name, orderCount: 1, aliases: new Set(aliases) });
    }
  }
  if (!products.size) return [];

  const aliasOwners = new Map<string, number>();
  for (const product of products.values()) {
    for (const alias of product.aliases) {
      aliasOwners.set(alias, (aliasOwners.get(alias) ?? 0) + 1);
    }
  }

  const ranked = [...products.values()].map((product) => {
    const uniqueAliases = [...product.aliases].filter((alias) => aliasOwners.get(alias) === 1);
    return {
      id: product.id,
      name: product.name,
      mentionCount: entries.length ? mentionCount(entries, uniqueAliases) : 0,
      orderCount: product.orderCount,
      metric: "mention" as const,
    };
  });

  const byMention = ranked
    .filter((product) => product.mentionCount > 0)
    .sort((left, right) =>
      right.mentionCount - left.mentionCount
      || right.orderCount - left.orderCount
      || left.name.localeCompare(right.name, "zh-CN"));
  if (byMention.length) return byMention.slice(0, Math.floor(limit));

  return ranked
    .map((product) => ({ ...product, metric: "order" as const }))
    .sort((left, right) =>
      right.orderCount - left.orderCount
      || left.name.localeCompare(right.name, "zh-CN"))
    .slice(0, Math.floor(limit));
}
