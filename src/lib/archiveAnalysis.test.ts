import assert from "node:assert/strict";
import {
  analysisSourceKey,
  normalizeBeginnerReview,
  normalizeCandidate,
  normalizeCandidates,
  selectArchiveTranscriptAction,
  friendlyArchiveTranscriptError,
  transcriptRefreshFailureState,
  masterComparisonPresentation,
  isCurrentMasterComparison,
  autoMatchMasterSection,
  type CandidateInput,
} from "./archiveAnalysis.js";

const masterSections = [
  { id: 11, sectionKind: "product", productCardId: "CANON-R7", title: "佳能 R7 主推讲解" },
  { id: 12, sectionKind: "product", productCardId: "INSTA360-A4PRO2", title: "影石 Insta360 A4 Pro 2" },
  { id: 13, sectionKind: "scenario", productCardId: null, title: "售后异议处理" },
] as const;

assert.deepEqual(autoMatchMasterSection("INSTA360-A4PRO2", masterSections), {
  sectionId: 12,
  status: "matched",
  strategy: "product-card",
});
assert.deepEqual(autoMatchMasterSection("影石 A4 PRO 2（99新）", masterSections), {
  sectionId: 12,
  status: "matched",
  strategy: "title",
});
assert.deepEqual(autoMatchMasterSection("商品待确认", [masterSections[0]]), {
  sectionId: 11,
  status: "matched",
  strategy: "single-product",
});
assert.deepEqual(autoMatchMasterSection("A4 Pro 2", [
  masterSections[1],
  { ...masterSections[1], id: 14, title: "影石 A4 Pro 2 套装" },
]), {
  sectionId: null,
  status: "ambiguous",
  strategy: null,
});
assert.deepEqual(autoMatchMasterSection("完全未知商品", masterSections), {
  sectionId: null,
  status: "unmatched",
  strategy: null,
});

assert.deepEqual(masterComparisonPresentation({ admission: "review_only", totalScore: 84, reasons: [] }), {
  tone: "neutral", label: "仅保留复盘", detail: "本地复核分 84，未达到 85 分",
});
assert.deepEqual(masterComparisonPresentation({ admission: "candidate_queue", totalScore: 85, reasons: [] }), {
  tone: "success", label: "已进入候选辅稿", detail: "本地复核分 85，等待人工确认",
});
assert.deepEqual(masterComparisonPresentation({ admission: "blocked", totalScore: 100, reasons: ["价格仍待确认"] }), {
  tone: "warning", label: "暂不能进入候选辅稿", detail: "价格仍待确认",
});
assert.deepEqual(masterComparisonPresentation({ admission: null, totalScore: null, reasons: [] }), {
  tone: "warning", label: "还没有匹配到母稿章节", detail: "请选择母稿位置后重新评分",
});
assert.equal(isCurrentMasterComparison("C01", 3, "C01", 3), true);
assert.equal(isCurrentMasterComparison("C01", 3, "C02", 3), false);
assert.equal(isCurrentMasterComparison("C01", 3, "C01", 4), false);

assert.equal(
  analysisSourceKey({ kind: "archive", platform: "douyin", roomId: "100", liveId: "200" }),
  "archive:douyin:100:200",
);
assert.equal(analysisSourceKey({ kind: "video", videoId: 42 }), "video:42");

assert.equal(
  selectArchiveTranscriptAction(false, "1\n00:00:00,000 --> 00:00:01,000\n已有逐字稿"),
  "use-existing",
);
assert.equal(selectArchiveTranscriptAction(false, ""), "refresh");
assert.equal(selectArchiveTranscriptAction(true, "已有逐字稿"), "refresh");

assert.equal(
  friendlyArchiveTranscriptError("IO error: Playlist file not found"),
  "程序找不到这条录播的源文件，暂时无法重新识别。录播记录仍在，请检查缓存目录或重新导入原视频。",
);
assert.equal(friendlyArchiveTranscriptError("network unavailable"), "network unavailable");
assert.deepEqual(transcriptRefreshFailureState(true), {
  notice: "刷新失败 · 继续使用旧稿",
  stage: "逐字稿刷新失败，已保留原有分析",
});
assert.deepEqual(transcriptRefreshFailureState(false), {
  notice: "源文件不可用",
  stage: "找不到可分析的录播源文件",
});

const beginnerReview = normalizeBeginnerReview({
  verdict: "建议保留",
  summary: "主播准确回应需求，但购买动作还需要核验。",
  good_points: "先确认型号，再承诺找好成色",
  improvements: ["报价后补一句购买指令", "减少与同事沟通的空档"],
  checks: null,
});
assert.equal(beginnerReview.verdict, "建议保留");
assert.equal(beginnerReview.summary, "主播准确回应需求，但购买动作还需要核验。");
assert.deepEqual(beginnerReview.goodPoints, ["先确认型号，再承诺找好成色"]);
assert.deepEqual(beginnerReview.improvements, ["报价后补一句购买指令", "减少与同事沟通的空档"]);
assert.deepEqual(beginnerReview.checks, ["暂无，按逐字稿结论使用"]);

const fallbackReview = normalizeBeginnerReview({}, "这是一段旧版专业复盘。\n后续内容不应出现在摘要中。");
assert.equal(fallbackReview.verdict, "建议人工判断");
assert.equal(fallbackReview.summary, "这是一段旧版专业复盘。");

const baseCandidate: CandidateInput = {
  start: 12,
  end: 48,
  type: "成交片段",
  confidence: "高",
  product: "R7 相机",
  evidence: "用户确认下单，主播完成备注",
  reason: "出现明确成交确认",
  verify: "核验订单时间",
};

const core = normalizeCandidate(baseCandidate, 0);
assert.ok(core);
assert.equal(core.tier, "核心高光");
assert.equal(core.score, 88);
assert.deepEqual(core.signals, ["明确成交", "高置信度"]);

const explicitCandidate = normalizeCandidate({
  ...baseCandidate,
  tier: "候选高光",
  score: 72,
  signals: ["链接动作", "待核验订单"],
}, 1);
assert.ok(explicitCandidate);
assert.equal(explicitCandidate.tier, "候选高光");
assert.equal(explicitCandidate.score, 72);
assert.deepEqual(explicitCandidate.signals, ["链接动作", "待核验订单"]);

const legacyCandidate = normalizeCandidate({
  ...baseCandidate,
  type: "转品/上链接片段",
  confidence: "中",
}, 2);
assert.ok(legacyCandidate);
assert.equal(legacyCandidate.tier, "候选高光");
assert.equal(legacyCandidate.score, 62);

assert.equal(normalizeCandidate({ ...baseCandidate, end: 12 }, 3), null);

const normalized = normalizeCandidates([
  { ...baseCandidate, start: 80, end: 110, confidence: "中" },
  { ...baseCandidate, start: 20, end: 50, tier: "核心高光", score: 95 },
  { ...baseCandidate, start: 120, end: 119 },
]);
assert.equal(normalized.length, 2);
assert.equal(normalized[0].id, "C01-20-50");
assert.equal(normalized[0].tier, "核心高光");
assert.equal(normalized[1].id, "C02-80-110");

console.log("archiveAnalysis domain tests passed");
