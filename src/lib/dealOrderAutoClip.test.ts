import assert from "node:assert/strict";
import type { PaymentEvent } from "./orderDealTimeline.js";
import type { WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";
import {
  buildDealClipContexts,
  buildDealClipUserPrompt,
  clampDealClipRange,
  clusterDealOrderEvents,
  dealClipContextCacheKey,
  DEAL_CLIP_CONTEXT_POST_SEC,
  DEAL_CLIP_CONTEXT_PRE_SEC,
  DEAL_CLIP_MAX_DURATION_SEC,
  DEAL_CLIP_MIN_DURATION_SEC,
  finalizeDealClipFromAi,
  mergeDealClipRanges,
  parseDealClipAiResponse,
  selectDealClipContext,
  type DealClipRange,
} from "./dealOrderAutoClip.js";

const events: PaymentEvent[] = [
  { offsetSec: 45, payAmountFen: 10000, productName: "佳能 R7" },
  { offsetSec: 50, payAmountFen: 20000, productName: "佳能 R7" },
  { offsetSec: 58, payAmountFen: 30000, productName: "尼康 Z5" },
  { offsetSec: 620, payAmountFen: 40000, productName: "索尼 A7" },
];

const transcript: WorkspaceTranscriptEntry[] = [
  { id: 1, start: 10, end: 20, text: "开场闲聊" },
  { id: 2, start: 30, end: 55, text: "现在上链接，限时优惠" },
  { id: 3, start: 56, end: 80, text: "拍下就送配件" },
  { id: 4, start: 600, end: 630, text: "这款索尼也给大家报个价" },
];

const contexts = buildDealClipContexts(events, transcript);
assert.equal(contexts.length, 3, "different products in the same minute must remain separate chains");
assert.equal(contexts[0]?.orderCount, 2);
assert.equal(contexts[0]?.payAnchorSec, 45);
assert.equal(contexts[0]?.payEndSec, 50);
assert.equal(contexts[0]?.contextStart, 0);
assert.equal(contexts[0]?.contextEnd, 50 + DEAL_CLIP_CONTEXT_POST_SEC);
assert.ok(contexts[0]?.transcriptLines.some((line) => line.text.includes("上链接")));
assert.equal(contexts[1]?.productNames[0], "尼康 Z5");
assert.equal(contexts[2]?.payAnchorSec, 620);
assert.equal(contexts[2]?.contextStart, 620 - DEAL_CLIP_CONTEXT_PRE_SEC);
assert.equal(selectDealClipContext(contexts, 49)?.productNames[0], "佳能 R7");
assert.equal(selectDealClipContext(contexts, 59)?.productNames[0], "尼康 Z5");
assert.equal(selectDealClipContext(contexts, 618)?.productNames[0], "索尼 A7");

const adjacentMinuteOrders = clusterDealOrderEvents([
  { offsetSec: 50, payAmountFen: 10000, productName: "佳能 小白兔" },
  { offsetSec: 190, payAmountFen: 10000, productName: "佳能 小白兔" },
  { offsetSec: 510, payAmountFen: 10000, productName: "佳能 小白兔" },
]);
assert.equal(adjacentMinuteOrders.length, 2, "same-product orders inside five minutes should share one chain");
assert.equal(adjacentMinuteOrders[0]?.events.length, 2);

const prompt = buildDealClipUserPrompt(contexts[0]!);
assert.match(prompt, /订单簇/);
assert.match(prompt, /上链接/);

assert.deepEqual(
  parseDealClipAiResponse('```json\n{"start":30,"end":90,"title":"促单","reason":"上链接"}\n```'),
  { start: 30, end: 90, title: "促单", reason: "上链接" },
);
assert.equal(parseDealClipAiResponse("not json"), null);
assert.equal(parseDealClipAiResponse('{"start":90,"end":30,"title":"x"}'), null);

const clampedShort = clampDealClipRange(
  { start: 40, end: 50, title: "短", reason: "" },
  { contextStart: 0, contextEnd: 200, payAnchorSec: 45 },
);
assert.ok(clampedShort);
assert.ok((clampedShort!.end - clampedShort!.start) >= DEAL_CLIP_MIN_DURATION_SEC);

const clampedLong = clampDealClipRange(
  { start: 0, end: 800, title: "长", reason: "" },
  { contextStart: 0, contextEnd: 800, payAnchorSec: 600 },
);
assert.ok(clampedLong);
assert.ok((clampedLong!.end - clampedLong!.start) <= DEAL_CLIP_MAX_DURATION_SEC);
assert.ok(clampedLong!.start <= 600 && clampedLong!.end >= 600, "trimmed long ranges must retain the pay anchor");

const outside = clampDealClipRange(
  { start: -10, end: 5, title: "边", reason: "" },
  { contextStart: 100, contextEnd: 120, payAnchorSec: 110 },
);
assert.ok(outside);
assert.ok(outside!.start >= 100);
assert.ok(outside!.end <= 120);

const ranges: DealClipRange[] = [
  { start: 10, end: 80, title: "A", reason: "a", payAnchorSec: 50, peakMinuteIndex: 0 },
  { start: 70, end: 120, title: "BB", reason: "b", payAnchorSec: 75, peakMinuteIndex: 1 },
  { start: 200, end: 260, title: "C", reason: "c", payAnchorSec: 220, peakMinuteIndex: 3 },
];
const merged = mergeDealClipRanges(ranges);
assert.equal(merged.length, 2);
assert.equal(merged[0]?.start, 10);
assert.equal(merged[0]?.end, 120);
assert.equal(merged[0]?.title, "BB");
assert.equal(merged[1]?.start, 200);

const distinctChains = mergeDealClipRanges([
  { start: 10, end: 80, title: "佳能", reason: "", payAnchorSec: 50, peakMinuteIndex: 0, chainKey: "佳能" },
  { start: 70, end: 120, title: "尼康", reason: "", payAnchorSec: 75, peakMinuteIndex: 1, chainKey: "尼康" },
]);
assert.equal(distinctChains.length, 2, "overlapping time ranges for different products must stay separate");

const finalized = finalizeDealClipFromAi(contexts[0]!, '{"start":30,"end":100,"title":"报价上链接","reason":"促单"}');
assert.ok(finalized);
assert.equal(finalized!.title, "报价上链接");
assert.equal(finalized!.peakMinuteIndex, 0);
assert.equal(finalized!.chainKey, dealClipContextCacheKey(contexts[0]!));
assert.equal(
  dealClipContextCacheKey(contexts[0]!),
  dealClipContextCacheKey(contexts[0]!),
  "cache key must be stable for the same cluster",
);
assert.notEqual(
  dealClipContextCacheKey(contexts[0]!),
  dealClipContextCacheKey(contexts[1]!),
  "different product clusters must not share a refine cache key",
);

const fallback = finalizeDealClipFromAi(contexts[0]!, "抱歉无法解析");
assert.ok(fallback);
assert.match(fallback!.reason, /回退/);

console.log("dealOrderAutoClip tests passed");
