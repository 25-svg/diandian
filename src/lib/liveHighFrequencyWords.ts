import { formatWorkspaceClock, type WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";

export type HighFrequencyCategory =
  | "互动力"
  | "关注力"
  | "促单/线索"
  | "塑品"
  | "售后"
  | "其他";

export type HighFrequencyLexiconEntry = {
  word: string;
  category: HighFrequencyCategory;
  subcategory: string;
  scene: string;
  industry: string;
};

export type HighFrequencyHit = {
  id: string;
  type: "运营关键词";
  category: HighFrequencyCategory;
  subcategory: string;
  word: string;
  hitCount: number;
  fullTextCount: number;
  source: "逐字稿";
  scene: string;
  industry: string;
  sampleStartSec: number;
  sampleText: string;
};

export type HighFrequencyCategorySummary = {
  category: HighFrequencyCategory | "全部";
  count: number;
  ratioLabel: string;
};

/** Short operational lexicon for live commerce — not a full industry dictionary. */
export const LIVE_HIGH_FREQUENCY_LEXICON: readonly HighFrequencyLexiconEntry[] = [
  { word: "怎么", category: "互动力", subcategory: "互动力-公屏互动", scene: "知识和问题互动", industry: "通用词库" },
  { word: "干什么", category: "互动力", subcategory: "互动力-公屏互动", scene: "知识和问题互动", industry: "通用词库" },
  { word: "选哪个", category: "互动力", subcategory: "互动力-公屏互动", scene: "对比选型互动", industry: "通用词库" },
  { word: "告诉主播", category: "互动力", subcategory: "互动力-公屏互动", scene: "引导观众发言", industry: "通用词库" },
  { word: "扣1", category: "互动力", subcategory: "互动力-公屏互动", scene: "引导公屏互动", industry: "通用词库" },
  { word: "打在公屏", category: "互动力", subcategory: "互动力-公屏互动", scene: "引导公屏互动", industry: "通用词库" },
  { word: "有没有想问", category: "互动力", subcategory: "互动力-公屏互动", scene: "知识和问题互动", industry: "通用词库" },
  { word: "听得懂", category: "互动力", subcategory: "互动力-公屏互动", scene: "确认理解", industry: "通用词库" },

  { word: "点个关注", category: "关注力", subcategory: "关注力-涨粉引导", scene: "引导关注", industry: "通用词库" },
  { word: "关注一下", category: "关注力", subcategory: "关注力-涨粉引导", scene: "引导关注", industry: "通用词库" },
  { word: "没关注的", category: "关注力", subcategory: "关注力-涨粉引导", scene: "引导关注", industry: "通用词库" },
  { word: "点亮小铃铛", category: "关注力", subcategory: "关注力-涨粉引导", scene: "引导开播提醒", industry: "通用词库" },
  { word: "记得关注", category: "关注力", subcategory: "关注力-涨粉引导", scene: "引导关注", industry: "通用词库" },

  { word: "上链接", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "上链接促单", industry: "通用词库" },
  { word: "小黄车", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "上链接促单", industry: "通用词库" },
  { word: "几号链接", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "上链接促单", industry: "通用词库" },
  { word: "几号车", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "上链接促单", industry: "通用词库" },
  { word: "置顶", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "上链接促单", industry: "通用词库" },
  { word: "拍下", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "引导下单", industry: "通用词库" },
  { word: "下单", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "引导下单", industry: "通用词库" },
  { word: "上车", category: "促单/线索", subcategory: "促单/线索-引导下单", scene: "引导下单", industry: "通用词库" },
  { word: "优惠完", category: "促单/线索", subcategory: "促单/线索-价格优惠", scene: "价格促单", industry: "通用词库" },
  { word: "限时", category: "促单/线索", subcategory: "促单/线索-价格优惠", scene: "紧迫感促单", industry: "通用词库" },
  { word: "库存", category: "促单/线索", subcategory: "促单/线索-价格优惠", scene: "稀缺促单", industry: "通用词库" },
  { word: "给你备注", category: "促单/线索", subcategory: "促单/线索-成交确认", scene: "成交确认", industry: "通用词库" },
  { word: "恭喜", category: "促单/线索", subcategory: "促单/线索-成交确认", scene: "成交确认", industry: "通用词库" },

  { word: "成色", category: "塑品", subcategory: "塑品-成色说明", scene: "二手成色讲解", industry: "相机数码" },
  { word: "九成新", category: "塑品", subcategory: "塑品-成色说明", scene: "二手成色讲解", industry: "相机数码" },
  { word: "99新", category: "塑品", subcategory: "塑品-成色说明", scene: "二手成色讲解", industry: "相机数码" },
  { word: "在仓", category: "塑品", subcategory: "塑品-现货信任", scene: "现货可信度", industry: "相机数码" },
  { word: "现货", category: "塑品", subcategory: "塑品-现货信任", scene: "现货可信度", industry: "通用词库" },
  { word: "原装", category: "塑品", subcategory: "塑品-卖点说明", scene: "产品卖点", industry: "相机数码" },
  { word: "镜头", category: "塑品", subcategory: "塑品-卖点说明", scene: "产品卖点", industry: "相机数码" },
  { word: "机身", category: "塑品", subcategory: "塑品-卖点说明", scene: "产品卖点", industry: "相机数码" },
  { word: "遮光罩", category: "塑品", subcategory: "塑品-卖点说明", scene: "配件说明", industry: "相机数码" },
  { word: "脚架环", category: "塑品", subcategory: "塑品-卖点说明", scene: "配件说明", industry: "相机数码" },
  { word: "多少钱", category: "塑品", subcategory: "塑品-价格问答", scene: "询价应对", industry: "通用词库" },
  { word: "什么价", category: "塑品", subcategory: "塑品-价格问答", scene: "询价应对", industry: "通用词库" },

  { word: "包邮", category: "售后", subcategory: "售后-物流承诺", scene: "物流售后", industry: "通用词库" },
  { word: "发货", category: "售后", subcategory: "售后-物流承诺", scene: "物流售后", industry: "通用词库" },
  { word: "今天发", category: "售后", subcategory: "售后-物流承诺", scene: "物流售后", industry: "通用词库" },
  { word: "质保", category: "售后", subcategory: "售后-保障承诺", scene: "售后保障", industry: "通用词库" },
  { word: "保修", category: "售后", subcategory: "售后-保障承诺", scene: "售后保障", industry: "通用词库" },
  { word: "七天", category: "售后", subcategory: "售后-保障承诺", scene: "售后保障", industry: "通用词库" },
  { word: "退换", category: "售后", subcategory: "售后-保障承诺", scene: "售后保障", industry: "通用词库" },
  { word: "验货", category: "售后", subcategory: "售后-保障承诺", scene: "售后保障", industry: "相机数码" },
];

const CATEGORY_ORDER: HighFrequencyCategory[] = [
  "互动力",
  "关注力",
  "促单/线索",
  "塑品",
  "售后",
  "其他",
];

function countOccurrences(haystack: string, needle: string): number {
  if (!needle) return 0;
  let count = 0;
  let from = 0;
  while (from <= haystack.length - needle.length) {
    const index = haystack.indexOf(needle, from);
    if (index < 0) break;
    count += 1;
    from = index + needle.length;
  }
  return count;
}

function firstSample(
  entries: readonly WorkspaceTranscriptEntry[],
  word: string,
): { startSec: number; text: string } | null {
  for (const entry of entries) {
    if (!entry.text.includes(word)) continue;
    return { startSec: entry.start, text: entry.text.trim() };
  }
  return null;
}

export function extractLiveHighFrequencyHits(
  entries: readonly WorkspaceTranscriptEntry[],
  lexicon: readonly HighFrequencyLexiconEntry[] = LIVE_HIGH_FREQUENCY_LEXICON,
): HighFrequencyHit[] {
  if (!entries.length) return [];
  const fullText = entries.map((entry) => entry.text).join("\n");
  const hits: HighFrequencyHit[] = [];
  for (const item of lexicon) {
    const fullTextCount = countOccurrences(fullText, item.word);
    if (fullTextCount <= 0) continue;
    const sample = firstSample(entries, item.word);
    if (!sample) continue;
    hits.push({
      id: `${item.category}:${item.word}`,
      type: "运营关键词",
      category: item.category,
      subcategory: item.subcategory,
      word: item.word,
      hitCount: fullTextCount,
      fullTextCount,
      source: "逐字稿",
      scene: item.scene,
      industry: item.industry,
      sampleStartSec: sample.startSec,
      sampleText: sample.text,
    });
  }
  return hits.sort(
    (left, right) =>
      right.hitCount - left.hitCount
      || left.category.localeCompare(right.category, "zh-CN")
      || left.word.localeCompare(right.word, "zh-CN"),
  );
}

export function summarizeHighFrequencyCategories(
  hits: readonly HighFrequencyHit[],
): HighFrequencyCategorySummary[] {
  const byCategory = new Map<HighFrequencyCategory, number>();
  for (const category of CATEGORY_ORDER) byCategory.set(category, 0);
  for (const hit of hits) {
    byCategory.set(hit.category, (byCategory.get(hit.category) ?? 0) + 1);
  }
  const operationalTotal = hits.length;
  const summaries: HighFrequencyCategorySummary[] = [
    {
      category: "全部",
      count: operationalTotal,
      ratioLabel: "",
    },
  ];
  for (const category of CATEGORY_ORDER) {
    const count = byCategory.get(category) ?? 0;
    if (!count && category === "其他") continue;
    const ratio = operationalTotal > 0 ? ((count / operationalTotal) * 100).toFixed(1) : "0.0";
    summaries.push({
      category,
      count,
      ratioLabel: `${ratio}%`,
    });
  }
  return summaries;
}

export function filterHighFrequencyHits(
  hits: readonly HighFrequencyHit[],
  category: HighFrequencyCategory | "全部",
): HighFrequencyHit[] {
  if (category === "全部") return [...hits];
  return hits.filter((hit) => hit.category === category);
}

export function paginateHighFrequencyHits<T>(
  items: readonly T[],
  page: number,
  pageSize: number,
): { pageItems: T[]; total: number; pageCount: number; page: number; pageSize: number } {
  const safeSize = Math.max(1, Math.floor(pageSize || 1));
  const total = items.length;
  const pageCount = Math.max(1, Math.ceil(total / safeSize));
  const safePage = Math.min(Math.max(1, Math.floor(page || 1)), pageCount);
  const start = (safePage - 1) * safeSize;
  return {
    pageItems: items.slice(start, start + safeSize) as T[],
    total,
    pageCount,
    page: safePage,
    pageSize: safeSize,
  };
}

export function formatHighFrequencySampleTime(startSec: number): string {
  return formatWorkspaceClock(startSec);
}
