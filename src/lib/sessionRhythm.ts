import type { WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";
import type { PaymentEvent } from "./orderDealTimeline.js";

export type SessionStructureKind =
  | "opening"
  | "interaction"
  | "product"
  | "trust"
  | "offer"
  | "objection"
  | "conversion"
  | "transition"
  | "low_activity"
  | "mixed";

export type SessionRhythmBucket = {
  index: number;
  startSec: number;
  endSec: number;
  kind: SessionStructureKind;
  label: string;
  characterCount: number;
  speechSec: number;
  speechRatio: number;
  charactersPerMinute: number;
  longestPauseSec: number;
  orderCount: number;
  orderAmountFen: number;
  transitionCount: number;
  promotionCount: number;
  activityScore: number;
  sampleText: string;
};

export type SessionRhythmMetrics = {
  durationSec: number;
  spokenRatio: number;
  charactersPerMinute: number;
  longestPauseSec: number;
  transitionCount: number;
  promotionCount: number;
  orderCount: number;
  structureCompleteness: number;
  peakBucketIndex: number | null;
  orderPeakBucketIndex: number | null;
};

export type SessionRhythmAnalysis = {
  bucketSec: number;
  buckets: SessionRhythmBucket[];
  metrics: SessionRhythmMetrics;
  summary: string;
  suggestions: string[];
};

export type SessionRhythmReview = {
  summary: string;
  structureVerdict: string;
  goodPoints: string[];
  improvements: string[];
};

type TermGroup = {
  kind: SessionStructureKind;
  terms: readonly string[];
  weight: number;
};

const TERM_GROUPS: readonly TermGroup[] = [
  {
    kind: "opening",
    terms: ["欢迎", "新进来的", "直播间", "今天给大家", "刚进来", "点个关注"],
    weight: 1.2,
  },
  {
    kind: "interaction",
    terms: ["公屏", "扣1", "评论区", "告诉主播", "有没有", "想看", "听得懂", "家人们"],
    weight: 1.1,
  },
  {
    kind: "product",
    terms: ["成色", "镜头", "机身", "快门", "型号", "焦段", "画质", "功能", "参数", "配件", "原装"],
    weight: 1,
  },
  {
    kind: "trust",
    terms: ["验货", "检测", "质保", "保修", "包邮", "发货", "退换", "售后", "保障", "承诺"],
    weight: 1.15,
  },
  {
    kind: "offer",
    terms: ["价格", "到手", "优惠", "券", "多少钱", "小黄车", "链接", "库存", "元钱", "直降"],
    weight: 1.2,
  },
  {
    kind: "objection",
    terms: ["担心", "怕", "问题", "但是", "值不值", "划算", "区别", "适合", "预算", "为什么"],
    weight: 1.05,
  },
  {
    kind: "conversion",
    terms: ["下单", "拍下", "上车", "付款", "锁单", "成交", "赶紧", "最后", "仅剩", "给你备注"],
    weight: 1.35,
  },
  {
    kind: "transition",
    terms: ["下一个", "下一款", "换一个", "再看一款", "转下一款", "再看看"],
    weight: 1.45,
  },
];

const KIND_PRIORITY: readonly SessionStructureKind[] = [
  "conversion",
  "transition",
  "offer",
  "objection",
  "trust",
  "product",
  "interaction",
  "opening",
];

const KIND_LABELS: Record<SessionStructureKind, string> = {
  opening: "开场承接",
  interaction: "互动暖场",
  product: "商品讲解",
  trust: "信任塑造",
  offer: "报价优惠",
  objection: "异议处理",
  conversion: "促单成交",
  transition: "转品过渡",
  low_activity: "低活跃段",
  mixed: "综合讲解",
};

export function sessionStructureLabel(kind: SessionStructureKind): string {
  return KIND_LABELS[kind];
}

export function chooseSessionRhythmBucketSec(durationSec: number): number {
  if (durationSec <= 60 * 60) return 60;
  if (durationSec <= 2 * 60 * 60) return 120;
  if (durationSec <= 4 * 60 * 60) return 180;
  return 300;
}

function cleanCharacterCount(text: string): number {
  return text.replace(/[\s\p{P}\p{S}]/gu, "").length;
}

function countOccurrences(text: string, term: string): number {
  if (!term) return 0;
  let count = 0;
  let offset = 0;
  while (offset <= text.length - term.length) {
    const index = text.indexOf(term, offset);
    if (index < 0) break;
    count += 1;
    offset = index + term.length;
  }
  return count;
}

function countTerms(text: string, terms: readonly string[]): number {
  return terms.reduce((total, term) => total + countOccurrences(text, term), 0);
}

function clippedIntervals(
  entries: readonly WorkspaceTranscriptEntry[],
  startSec: number,
  endSec: number,
): Array<[number, number]> {
  return entries
    .filter((entry) => entry.end > startSec && entry.start < endSec)
    .map((entry) => [Math.max(startSec, entry.start), Math.min(endSec, entry.end)] as [number, number])
    .filter(([start, end]) => end > start)
    .sort((left, right) => left[0] - right[0]);
}

function mergeIntervals(intervals: readonly [number, number][]): Array<[number, number]> {
  const merged: Array<[number, number]> = [];
  for (const interval of intervals) {
    const last = merged[merged.length - 1];
    if (!last || interval[0] > last[1]) {
      merged.push([...interval]);
    } else {
      last[1] = Math.max(last[1], interval[1]);
    }
  }
  return merged;
}

function intervalSpeechSeconds(intervals: readonly [number, number][]): number {
  return mergeIntervals(intervals).reduce((total, [start, end]) => total + end - start, 0);
}

function longestPauseInRange(
  intervals: readonly [number, number][],
  startSec: number,
  endSec: number,
): number {
  const merged = mergeIntervals(intervals);
  if (!merged.length) return Math.max(0, endSec - startSec);
  let longest = Math.max(0, merged[0][0] - startSec);
  for (let index = 1; index < merged.length; index += 1) {
    longest = Math.max(longest, merged[index][0] - merged[index - 1][1]);
  }
  return Math.max(longest, endSec - merged[merged.length - 1][1]);
}

function dominantKind(
  text: string,
  startSec: number,
  speechRatio: number,
  orderCount: number,
): { kind: SessionStructureKind; transitionCount: number; promotionCount: number } {
  const scores = new Map<SessionStructureKind, number>();
  for (const group of TERM_GROUPS) {
    scores.set(group.kind, countTerms(text, group.terms) * group.weight);
  }
  if (startSec < 10 * 60) scores.set("opening", (scores.get("opening") ?? 0) + 1.2);
  if (orderCount > 0) scores.set("conversion", (scores.get("conversion") ?? 0) + orderCount * 3);
  const transitionCount = countTerms(
    text,
    TERM_GROUPS.find((group) => group.kind === "transition")?.terms ?? [],
  );
  const promotionCount = countTerms(
    text,
    TERM_GROUPS.find((group) => group.kind === "conversion")?.terms ?? [],
  );
  if (cleanCharacterCount(text) < 8 || speechRatio < 0.12) {
    return { kind: "low_activity", transitionCount, promotionCount };
  }
  let winner: SessionStructureKind = "mixed";
  let winnerScore = 0;
  for (const kind of KIND_PRIORITY) {
    const score = scores.get(kind) ?? 0;
    if (score > winnerScore) {
      winner = kind;
      winnerScore = score;
    }
  }
  return { kind: winnerScore > 0 ? winner : "mixed", transitionCount, promotionCount };
}

function sampleText(entries: readonly WorkspaceTranscriptEntry[]): string {
  return entries
    .map((entry) => entry.text.trim())
    .filter(Boolean)
    .join(" ")
    .slice(0, 160);
}

function buildSuggestions(
  buckets: readonly SessionRhythmBucket[],
  metrics: SessionRhythmMetrics,
): string[] {
  const present = new Set(buckets.map((bucket) => bucket.kind));
  const suggestions: string[] = [];
  if (!present.has("opening")) suggestions.push("开场承接不明显：前10分钟先说清今天主讲商品、适合人群和观看收益。");
  if (!present.has("trust")) suggestions.push("信任塑造偏少：报价前补充验货、成色依据、售后和发货承诺。");
  if (!present.has("offer")) suggestions.push("价格承接不完整：固定讲清原价、优惠、到手价和链接位置。");
  if (!present.has("conversion")) suggestions.push("促单收口不明显：商品价值讲清后增加明确、不过度承诺的下单动作。");
  if (metrics.longestPauseSec >= 90) suggestions.push(`最长低活跃约 ${Math.round(metrics.longestPauseSec)} 秒：准备转品过渡句，避免观众不知道下一步看什么。`);
  if (metrics.charactersPerMinute > 260) suggestions.push("整体信息密度偏高：参数、成色、价格分三步讲，每讲完一步留出互动确认。 ");
  if (metrics.charactersPerMinute > 0 && metrics.charactersPerMinute < 70) suggestions.push("整体话术密度偏低：减少无信息停顿，用“商品结论→依据→适合谁”保持推进。");
  if (metrics.transitionCount > 0 && metrics.durationSec / metrics.transitionCount < 240) {
    suggestions.push("转品较频繁：每款商品至少完成卖点、信任、价格、收口后再切换。");
  }
  if (!metrics.orderCount) suggestions.push("本场未加载订单时间轴：当前只判断话术节奏，导入订单后可补充成交高峰对照。");
  if (!suggestions.length) suggestions.push("整场主要结构已覆盖；下一步重点复盘订单高峰前后的完整成交链路。 ");
  return suggestions.slice(0, 5).map((item) => item.trim());
}

export function buildSessionRhythmAnalysis(
  entries: readonly WorkspaceTranscriptEntry[],
  events: readonly PaymentEvent[] = [],
  durationHintSec = 0,
): SessionRhythmAnalysis {
  const transcriptEnd = entries.reduce((end, entry) => Math.max(end, entry.end), 0);
  const orderEnd = events.reduce((end, event) => Math.max(end, event.offsetSec), 0);
  const durationSec = Math.max(0, durationHintSec, transcriptEnd, orderEnd);
  const bucketSec = chooseSessionRhythmBucketSec(durationSec);
  if (!entries.length || durationSec <= 0) {
    const metrics: SessionRhythmMetrics = {
      durationSec,
      spokenRatio: 0,
      charactersPerMinute: 0,
      longestPauseSec: durationSec,
      transitionCount: 0,
      promotionCount: 0,
      orderCount: events.length,
      structureCompleteness: 0,
      peakBucketIndex: null,
      orderPeakBucketIndex: null,
    };
    return { bucketSec, buckets: [], metrics, summary: "整场逐字稿尚未就绪。", suggestions: [] };
  }

  const bucketCount = Math.max(1, Math.ceil(durationSec / bucketSec));
  const buckets: SessionRhythmBucket[] = [];
  for (let index = 0; index < bucketCount; index += 1) {
    const startSec = index * bucketSec;
    const endSec = Math.min(durationSec, startSec + bucketSec);
    const bucketEntries = entries.filter((entry) => entry.end > startSec && entry.start < endSec);
    const bucketEvents = events.filter((event) => event.offsetSec >= startSec && event.offsetSec < endSec);
    const text = bucketEntries.map((entry) => entry.text).join(" ");
    const intervals = clippedIntervals(bucketEntries, startSec, endSec);
    const speechSec = intervalSpeechSeconds(intervals);
    const spanSec = Math.max(1, endSec - startSec);
    const speechRatio = Math.min(1, speechSec / spanSec);
    const characterCount = cleanCharacterCount(text);
    const orderCount = bucketEvents.length;
    const classification = dominantKind(text, startSec, speechRatio, orderCount);
    const charactersPerMinute = (characterCount / spanSec) * 60;
    const activityScore = charactersPerMinute
      + speechRatio * 100
      + orderCount * 45
      + classification.promotionCount * 8;
    buckets.push({
      index,
      startSec,
      endSec,
      kind: classification.kind,
      label: sessionStructureLabel(classification.kind),
      characterCount,
      speechSec,
      speechRatio,
      charactersPerMinute,
      longestPauseSec: longestPauseInRange(intervals, startSec, endSec),
      orderCount,
      orderAmountFen: bucketEvents.reduce((total, event) => total + (event.payAmountFen ?? 0), 0),
      transitionCount: classification.transitionCount,
      promotionCount: classification.promotionCount,
      activityScore,
      sampleText: sampleText(bucketEntries),
    });
  }

  const allIntervals = clippedIntervals(entries, 0, durationSec);
  const totalCharacters = entries.reduce((total, entry) => total + cleanCharacterCount(entry.text), 0);
  const present = new Set(buckets.map((bucket) => bucket.kind));
  const coreKinds: SessionStructureKind[] = ["opening", "interaction", "product", "trust", "offer", "conversion"];
  const structureCompleteness = Math.round(
    (coreKinds.filter((kind) => present.has(kind)).length / coreKinds.length) * 100,
  );
  const peakBucket = [...buckets].sort((left, right) => right.activityScore - left.activityScore)[0] ?? null;
  const orderPeakBucket = [...buckets]
    .filter((bucket) => bucket.orderCount > 0)
    .sort((left, right) => right.orderCount - left.orderCount || right.orderAmountFen - left.orderAmountFen)[0] ?? null;
  const metrics: SessionRhythmMetrics = {
    durationSec,
    spokenRatio: Math.min(1, intervalSpeechSeconds(allIntervals) / Math.max(1, durationSec)),
    charactersPerMinute: (totalCharacters / Math.max(1, durationSec)) * 60,
    longestPauseSec: longestPauseInRange(allIntervals, 0, durationSec),
    transitionCount: buckets.reduce((total, bucket) => total + bucket.transitionCount, 0),
    promotionCount: buckets.reduce((total, bucket) => total + bucket.promotionCount, 0),
    orderCount: events.length,
    structureCompleteness,
    peakBucketIndex: peakBucket?.index ?? null,
    orderPeakBucketIndex: orderPeakBucket?.index ?? null,
  };
  const suggestions = buildSuggestions(buckets, metrics);
  const summary = `整场覆盖 ${Math.round(metrics.durationSec / 60)} 分钟，话术覆盖率 ${Math.round(metrics.spokenRatio * 100)}%，结构完整度 ${metrics.structureCompleteness}%。`;
  return { bucketSec, buckets, metrics, summary, suggestions };
}

function clock(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;
  return hours > 0
    ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export function sessionRhythmSystemPrompt(): string {
  return [
    "你是二手相机直播的整场节奏与结构复盘教练。",
    "输入是系统按时间窗整理的客观指标和逐字稿摘录，不是完整原文。",
    "只根据输入判断，不补写不存在的订单、价格、商品或主播动作。",
    "重点判断：开场承接、互动暖场、商品讲解、信任塑造、报价优惠、异议处理、促单成交、转品过渡是否衔接顺畅。",
    "建议必须让新手主播能立即执行，避免空泛评价。",
    "只返回合法 JSON 对象，不要 Markdown：",
    '{"summary":"3-5句整场判断","structureVerdict":"一句话结构结论","goodPoints":["最多4条"],"improvements":["最多5条，指出时间段和改法"]}',
  ].join("\n");
}

export function buildSessionRhythmUserMessage(analysis: SessionRhythmAnalysis): string {
  const rows = analysis.buckets.map((bucket) => [
    `[bucket:${bucket.index}] ${clock(bucket.startSec)}-${clock(bucket.endSec)}`,
    `本地信号=${bucket.label}`,
    `话术覆盖=${Math.round(bucket.speechRatio * 100)}%`,
    `字/分钟=${Math.round(bucket.charactersPerMinute)}`,
    `最长停顿=${Math.round(bucket.longestPauseSec)}秒`,
    `订单=${bucket.orderCount}`,
    `摘录=${bucket.sampleText || "无"}`,
  ].join(" | "));
  return [
    "请复盘以下整场节奏窗口。颜色标签只是本地关键词初判，可以纠正结论，但不能编造事实。",
    analysis.summary,
    ...rows,
  ].join("\n");
}

function asStringArray(value: unknown, limit: number): string[] {
  return Array.isArray(value)
    ? value.map((item) => String(item || "").trim()).filter(Boolean).slice(0, limit)
    : [];
}

function escapeJsonStringControlCharacters(input: string): string {
  let output = "";
  let inString = false;
  let escaped = false;
  for (const character of input) {
    if (inString) {
      if (escaped) {
        output += character;
        escaped = false;
        continue;
      }
      if (character === "\\") {
        output += character;
        escaped = true;
        continue;
      }
      if (character === '"') {
        inString = false;
        output += character;
        continue;
      }
      if (character === "\n") {
        output += "\\n";
        continue;
      }
      if (character === "\r") continue;
      if (character === "\t") {
        output += "\\t";
        continue;
      }
      output += character;
      continue;
    }
    if (character === '"') inString = true;
    output += character;
  }
  return output;
}

export function parseSessionRhythmReview(raw: string): SessionRhythmReview {
  const source = String(raw || "").trim().replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/i, "");
  const start = source.indexOf("{");
  const end = source.lastIndexOf("}");
  if (start < 0 || end <= start) throw new Error("MiniMax 未返回整场结构 JSON");
  const candidate = source
    .slice(start, end + 1)
    .replace(/[\u201c\u201d]/g, '"')
    .replace(/[\u2018\u2019]/g, "'")
    .replace(/,\s*([}\]])/g, "$1");
  let value: Record<string, unknown> | null = null;
  for (const variant of [candidate, escapeJsonStringControlCharacters(candidate)]) {
    try {
      value = JSON.parse(variant) as Record<string, unknown>;
      break;
    } catch {
      // Try the repaired variant.
    }
  }
  if (!value) throw new Error("MiniMax 整场结构 JSON 格式错误");
  const review: SessionRhythmReview = {
    summary: String(value.summary || "").trim(),
    structureVerdict: String(value.structureVerdict ?? value.structure_verdict ?? "").trim(),
    goodPoints: asStringArray(value.goodPoints ?? value.good_points, 4),
    improvements: asStringArray(value.improvements, 5),
  };
  if (!review.summary && !review.structureVerdict && !review.goodPoints.length && !review.improvements.length) {
    throw new Error("MiniMax 整场结构结果为空");
  }
  return review;
}

export function isSessionRhythmReview(value: unknown): value is SessionRhythmReview {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return typeof row.summary === "string"
    && typeof row.structureVerdict === "string"
    && Array.isArray(row.goodPoints)
    && Array.isArray(row.improvements);
}
