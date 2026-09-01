import {
  formatDealMoneyYuan,
  type DealMinuteBucket,
  type PaymentEvent,
} from "./orderDealTimeline.js";
import {
  filterTranscriptWindow,
  formatWorkspaceClock,
  type WorkspaceTranscriptEntry,
} from "./companyAnalysisWorkspace.js";

/** AI context window only — not the final clip length. */
export const DEAL_CLIP_CONTEXT_PRE_SEC = 10 * 60;
export const DEAL_CLIP_CONTEXT_POST_SEC = 2 * 60;

export const DEAL_CLIP_MIN_DURATION_SEC = 45;
export const DEAL_CLIP_MAX_DURATION_SEC = 8 * 60;
/** Orders for the same product inside this gap belong to one sales chain. */
export const DEAL_CHAIN_CLUSTER_GAP_SEC = 5 * 60;
/** Prefer speech that leads into the pay moment. */
export const DEAL_CLIP_DEFAULT_PRE_SEC = 90;
export const DEAL_CLIP_DEFAULT_POST_SEC = 30;

export type DealAutoClipAvailability = {
  analysisMode: string;
  hasVideo: boolean;
  hasArchiveSource?: boolean;
  paymentEventCount: number;
  transcriptEntryCount: number;
  isTranscribing: boolean;
};

/**
 * Frontend readiness only. Source existence/readability is checked again by
 * the backend queue preflight, so legacy video status values must not keep a
 * completed imported recording permanently disabled.
 */
export function dealAutoClipDisabledReason(input: DealAutoClipAvailability): string {
  if (input.analysisMode !== "company_deal") return "当前分析模式不支持成交自动切片";
  if (!input.hasVideo && !input.hasArchiveSource) return "尚未加载可切片的视频";
  if (input.paymentEventCount <= 0) return "请先拉取或导入成交订单";
  if (input.transcriptEntryCount <= 0) return "请先完成成交窗口转写";
  if (input.isTranscribing) return "成交窗口仍在转写，完成后即可切片";
  return "";
}

export type DealClipContext = {
  peak: DealMinuteBucket;
  payAnchorSec: number;
  payEndSec: number;
  contextStart: number;
  contextEnd: number;
  productNames: string[];
  orderCount: number;
  totalPayAmountFen: number;
  transcriptLines: Array<{ start: number; end: number; text: string }>;
};

export type DealClipAiRange = {
  start: number;
  end: number;
  title: string;
  reason: string;
};

export type DealClipRange = DealClipAiRange & {
  payAnchorSec: number;
  peakMinuteIndex: number;
  chainKey?: string;
};

export const DEAL_CLIP_SYSTEM_PROMPT = [
  "你是直播带货完整成交链路剪辑师。订单支付时间只是成交锚点，不是视频开始时间。必须向前回溯，并定位这一商品从需求或疑问、产品匹配、卖点价值、风险消除或售后承诺、价格优惠与上链接、引导下单，直到支付或成交确认的完整连续话术。",
  "每个订单簇只输出一个连续视频区间，禁止把一条成交链拆成多个零散片段；中间出现短暂互动或被打断时，仍保留在同一个连续区间内。只有明确切换到其他商品后，才结束当前链路。",
  "只输出一个 JSON 对象，不要 markdown。字段：start（秒）、end（秒）、title（商品名+完整成交话术）、reason（一句话说明链路起止证据）。",
  "start/end 必须位于给定上下文范围；start 应落在本商品最早的需求/介绍处，end 应落在最后一次下单引导、支付确认或该商品讲解结束处。目标片长约 45 秒到 8 分钟。",
].join("");

export function dealClipContextWindow(payAnchorSec: number): { start: number; end: number } {
  const anchor = Math.max(0, Math.floor(payAnchorSec || 0));
  return {
    start: Math.max(0, anchor - DEAL_CLIP_CONTEXT_PRE_SEC),
    end: anchor + DEAL_CLIP_CONTEXT_POST_SEC,
  };
}

export type DealOrderCluster = {
  productKey: string;
  events: PaymentEvent[];
};

function normalizedProductKey(event: PaymentEvent): string {
  return (event.productName || "")
    .trim()
    .toLocaleLowerCase("zh-CN")
    .replace(/[\s·•,，。/\\|【】\[\]()（）_-]+/g, "");
}

/**
 * One complete sales chain can produce several orders over adjacent minutes.
 * Group those orders by product before asking AI for one continuous boundary.
 */
export function clusterDealOrderEvents(events: readonly PaymentEvent[]): DealOrderCluster[] {
  const clusters: DealOrderCluster[] = [];
  for (const event of [...events].filter((item) => item.offsetSec >= 0).sort((a, b) => a.offsetSec - b.offsetSec)) {
    const productKey = normalizedProductKey(event);
    const maxGap = productKey ? DEAL_CHAIN_CLUSTER_GAP_SEC : 2 * 60;
    const matching = [...clusters].reverse().find((cluster) => {
      if (cluster.productKey !== productKey) return false;
      const lastEvent = cluster.events[cluster.events.length - 1];
      return Boolean(lastEvent && event.offsetSec - lastEvent.offsetSec <= maxGap);
    });
    if (matching) matching.events.push(event);
    else clusters.push({ productKey, events: [event] });
  }
  return clusters.sort((left, right) => left.events[0]!.offsetSec - right.events[0]!.offsetSec);
}

function representativeProductNames(events: readonly PaymentEvent[]): string[] {
  const names: string[] = [];
  const seen = new Set<string>();
  for (const event of events) {
    const name = (event.productName || "").trim();
    if (!name || seen.has(name)) continue;
    seen.add(name);
    names.push(name);
    if (names.length >= 5) break;
  }
  return names;
}

export function buildDealClipContexts(
  events: readonly PaymentEvent[],
  transcriptEntries: readonly WorkspaceTranscriptEntry[],
  options?: { preSec?: number; postSec?: number },
): DealClipContext[] {
  const preSec = options?.preSec ?? DEAL_CLIP_CONTEXT_PRE_SEC;
  const postSec = options?.postSec ?? DEAL_CLIP_CONTEXT_POST_SEC;
  return clusterDealOrderEvents(events).map((cluster) => {
    const clusterEvents = cluster.events;
    const payAnchorSec = clusterEvents[0]!.offsetSec;
    const payEndSec = clusterEvents[clusterEvents.length - 1]!.offsetSec;
    const window = {
      start: Math.max(0, Math.floor(payAnchorSec) - preSec),
      end: Math.floor(payEndSec) + postSec,
    };
    const peak: DealMinuteBucket = {
      minuteIndex: Math.floor(payAnchorSec / 60),
      offsetSec: payAnchorSec,
      orderCount: clusterEvents.length,
      totalPayAmountFen: clusterEvents.reduce((sum, event) => sum + (event.payAmountFen ?? 0), 0),
    };
    const transcriptLines = filterTranscriptWindow(
      [...transcriptEntries],
      window.start,
      window.end,
    ).map((entry) => ({
      start: entry.start,
      end: entry.end,
      text: entry.text,
    }));
    return {
      peak,
      payAnchorSec,
      payEndSec,
      contextStart: window.start,
      contextEnd: window.end,
      productNames: representativeProductNames(clusterEvents),
      orderCount: peak.orderCount,
      totalPayAmountFen: peak.totalPayAmountFen,
      transcriptLines,
    };
  });
}

/** Pick the single product chain represented by the order currently selected in the UI. */
export function selectDealClipContext(
  contexts: readonly DealClipContext[],
  selectedOffsetSec: number | null,
): DealClipContext | null {
  if (!contexts.length) return null;
  if (selectedOffsetSec == null || !Number.isFinite(selectedOffsetSec)) return contexts[0] ?? null;
  return [...contexts].sort((left, right) => {
    const distance = (context: DealClipContext): number => {
      if (selectedOffsetSec >= context.payAnchorSec && selectedOffsetSec <= context.payEndSec) return 0;
      return Math.min(
        Math.abs(selectedOffsetSec - context.payAnchorSec),
        Math.abs(selectedOffsetSec - context.payEndSec),
      );
    };
    return distance(left) - distance(right) || left.payAnchorSec - right.payAnchorSec;
  })[0] ?? null;
}

export function buildDealClipUserPrompt(context: DealClipContext): string {
  const products = context.productNames.length
    ? context.productNames.join("、")
    : "未命名商品";
  const amount = context.totalPayAmountFen > 0
    ? formatDealMoneyYuan(context.totalPayAmountFen)
    : "金额未知";
  const lines = context.transcriptLines.length
    ? context.transcriptLines
      .map((line) => `[${formatWorkspaceClock(line.start)}-${formatWorkspaceClock(line.end)}] ${line.text}`)
      .join("\n")
    : "（该时间窗内无逐字稿）";

  return [
    `订单簇：${formatWorkspaceClock(context.payAnchorSec)}–${formatWorkspaceClock(context.payEndSec)}（首单 ${context.payAnchorSec} 秒，末单 ${context.payEndSec} 秒）`,
    `该商品成交：${context.orderCount} 单 · ${amount}`,
    `商品：${products}`,
    `上下文范围：${context.contextStart}–${context.contextEnd} 秒。必须从逐字稿中找到一条连续完整成交链，只返回一个 start/end；最终片长约 45s–8min。`,
    "逐字稿：",
    lines,
  ].join("\n");
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function readNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function readString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

export function extractJsonObjectText(text: string): string | null {
  const cleaned = text.replace(/```json\s*/gi, "").replace(/```/g, "").trim();
  const start = cleaned.indexOf("{");
  const end = cleaned.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  return cleaned.slice(start, end + 1);
}

export function parseDealClipAiResponse(text: string): DealClipAiRange | null {
  const jsonText = extractJsonObjectText(text);
  if (!jsonText) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(jsonText);
  } catch {
    return null;
  }
  const record = asRecord(parsed);
  if (!record) return null;
  const start = readNumber(record.start ?? record.start_sec ?? record.startSec);
  const end = readNumber(record.end ?? record.end_sec ?? record.endSec);
  if (start == null || end == null || end <= start) return null;
  return {
    start,
    end,
    title: readString(record.title) || "成交话术切片",
    reason: readString(record.reason) || "",
  };
}

export function clampDealClipRange(
  range: DealClipAiRange,
  context: Pick<DealClipContext, "contextStart" | "contextEnd" | "payAnchorSec"> & { payEndSec?: number },
): DealClipAiRange | null {
  const ctxStart = context.contextStart;
  const ctxEnd = context.contextEnd;
  if (ctxEnd <= ctxStart) return null;

  let start = Math.max(ctxStart, Math.min(range.start, ctxEnd));
  let end = Math.max(ctxStart, Math.min(range.end, ctxEnd));
  if (end <= start) {
    // Fallback: speech leading into pay within the AI context window.
    start = Math.max(ctxStart, context.payAnchorSec - DEAL_CLIP_DEFAULT_PRE_SEC);
    end = Math.min(ctxEnd, (context.payEndSec ?? context.payAnchorSec) + DEAL_CLIP_DEFAULT_POST_SEC);
  }
  if (end <= start) return null;

  let duration = end - start;
  if (duration < DEAL_CLIP_MIN_DURATION_SEC) {
    const need = DEAL_CLIP_MIN_DURATION_SEC - duration;
    const expandBefore = Math.min(need, start - ctxStart);
    start -= expandBefore;
    end = Math.min(ctxEnd, end + (need - expandBefore));
    duration = end - start;
    if (duration < DEAL_CLIP_MIN_DURATION_SEC) {
      // Context too short — keep whatever fits.
      if (duration < 5) return null;
    }
  }

  if (end - start > DEAL_CLIP_MAX_DURATION_SEC) {
    // Keep the payment anchor inside the retained continuous window.
    const anchorEnd = (context.payEndSec ?? context.payAnchorSec) + DEAL_CLIP_DEFAULT_POST_SEC;
    end = Math.min(ctxEnd, Math.max(end, anchorEnd));
    start = Math.max(ctxStart, end - DEAL_CLIP_MAX_DURATION_SEC);
    start = Math.max(ctxStart, start);
    end = Math.min(ctxEnd, Math.max(end, start + 1));
  }

  if (end <= start) return null;
  return {
    start: Math.round(start * 10) / 10,
    end: Math.round(end * 10) / 10,
    title: range.title.trim() || "完整成交话术",
    reason: range.reason,
  };
}

export function mergeDealClipRanges(ranges: readonly DealClipRange[]): DealClipRange[] {
  if (!ranges.length) return [];
  const sorted = [...ranges].sort((left, right) => left.start - right.start || left.end - right.end);
  const merged: DealClipRange[] = [];
  for (const range of sorted) {
    const last = merged[merged.length - 1];
    const sameChain = !last?.chainKey || !range.chainKey || last.chainKey === range.chainKey;
    if (!last || range.start > last.end || !sameChain) {
      merged.push({ ...range });
      continue;
    }
    last.end = Math.max(last.end, range.end);
    if (range.title && range.title.length > last.title.length) {
      last.title = range.title;
    }
    if (range.reason && !last.reason) {
      last.reason = range.reason;
    }
    // Keep the earlier pay anchor / peak for identity.
    if (range.payAnchorSec < last.payAnchorSec) {
      last.payAnchorSec = range.payAnchorSec;
      last.peakMinuteIndex = range.peakMinuteIndex;
    }
  }
  return merged;
}

export function finalizeDealClipFromAi(
  context: DealClipContext,
  aiText: string,
): DealClipRange | null {
  const parsed = parseDealClipAiResponse(aiText)
    ?? {
      start: Math.max(context.contextStart, context.payAnchorSec - DEAL_CLIP_DEFAULT_PRE_SEC),
      end: Math.min(context.contextEnd, context.payEndSec + DEAL_CLIP_DEFAULT_POST_SEC),
      title: context.productNames[0]
        ? `${context.productNames[0]} 完整成交话术`
        : `完整成交话术 ${formatWorkspaceClock(context.payAnchorSec)}`,
      reason: "模型未返回有效边界，已按支付前默认窗回退",
    };
  const clamped = clampDealClipRange(parsed, context);
  if (!clamped) return null;
  return {
    ...clamped,
    payAnchorSec: context.payAnchorSec,
    peakMinuteIndex: context.peak.minuteIndex,
    chainKey: dealClipContextCacheKey(context),
  };
}

/** Stable cache key for one product-order cluster (used by speech refine + batch clip). */
export function dealClipContextCacheKey(context: Pick<DealClipContext, "payAnchorSec" | "payEndSec" | "productNames">): string {
  const products = context.productNames
    .map((name) => name.trim().toLocaleLowerCase("zh-CN"))
    .filter(Boolean)
    .join("|");
  return `${Math.floor(context.payAnchorSec)}:${Math.floor(context.payEndSec)}:${products || "_"}`;
}
