import assert from "node:assert/strict";
import { COMMERCE_REVIEW_PROMPT } from "./agent/prompts.js";
import { operationalScriptLabel, operationalScriptTag } from "./scriptTaxonomy.js";
import {
  analysisSourceKey,
  normalizeBeginnerReview,
  normalizeCandidate,
  normalizeCandidates,
  applyCandidateContextVerification,
  mergeSalesCandidates,
  selectDiscoveryCandidates,
  isCompleteSalesChain,
  isMasterScoreEligible,
  isStructuredDiscoveryCandidate,
  candidateNeedsLegacyContextVerification,
  missingSalesChainStages,
  COMPLETE_SALES_CHAIN_STAGES,
  sessionCandidateWorkPlan,
  summarizeSessionReview,
  selectArchiveTranscriptAction,
  friendlyArchiveTranscriptError,
  findActiveArchiveSubtitleTask,
  findActiveVideoSubtitleTask,
  transcriptRefreshFailureState,
  masterComparisonPresentation,
  isCurrentMasterComparison,
  autoMatchMasterSection,
  interruptedChainContextWindow,
  buildCandidateDiscoveryPrompt,
  candidateOutcomeLabel,
  parseCandidateDiscoveryResponse,
  parseDiscoverySegments,
  parseAnalysisProfile,
  competitorDiscoveryOutcome,
  type CandidateInput,
} from "./archiveAnalysis.js";

assert.deepEqual(parseAnalysisProfile(""), {
  analysisPurpose: "enterprise_review",
  competitorName: "",
  masterScriptKey: "",
});
assert.deepEqual(parseAnalysisProfile('{"analysisPurpose":"competitor_benchmark","competitorName":"示例相机店","masterScriptKey":"MS-BATCH-4"}'), {
  analysisPurpose: "competitor_benchmark",
  competitorName: "示例相机店",
  masterScriptKey: "MS-BATCH-4",
});
assert.equal(competitorDiscoveryOutcome("confirmed_conversion"), "conversion_signal");

assert.deepEqual(interruptedChainContextWindow(600, 660), { start: 300, end: 960 });
assert.deepEqual(interruptedChainContextWindow(120, 180), { start: 0, end: 480 });
const forbiddenDirectReadTerms = new RegExp([
  ["可直接", "口播"].join(""),
  ["直接照", "着念"].join(""),
  "口播优化",
  "融合口播",
].join("|"));
assert.doesNotMatch(COMMERCE_REVIEW_PROMPT, forbiddenDirectReadTerms);
assert.match(COMMERCE_REVIEW_PROMPT, /话术发展建议（不可直接照念，须培训负责人审定）/);
assert.equal(operationalScriptLabel("产品讲解"), "塑品讲解 · 产品讲解");
assert.equal(operationalScriptTag("产品推荐"), "选品推荐");
assert.equal(operationalScriptTag("留人钩子"), "迎新留人");
assert.equal(operationalScriptTag("信任建立"), "信任保障");
assert.equal(operationalScriptTag("售后与风险消除"), "信任保障");
assert.equal(operationalScriptTag("价格、优惠或链接承接"), "报价链接");
assert.equal(operationalScriptTag("催单与成交确认"), "逼单成交");
assert.equal(operationalScriptTag("完整成交链路"), "逼单成交");
assert.equal(operationalScriptTag("高质量金句"), "金句示范");
assert.equal(operationalScriptTag("需要改进的反面案例"), "反面案例");

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
  tone: "neutral", label: "仅保留复盘", detail: "母稿匹配与可复用分 84，未达到 85 分",
});
assert.deepEqual(masterComparisonPresentation({ admission: "candidate_queue", totalScore: 85, reasons: [] }), {
  tone: "success", label: "已进入候选辅稿", detail: "母稿匹配与可复用分 85，等待人工审核",
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

assert.deepEqual(
  findActiveVideoSubtitleTask([
    { id: "finished", task_type: "generate_video_subtitle", status: "success", metadata: '{"video_id":39}' },
    { id: "other-video", task_type: "generate_video_subtitle", status: "processing", metadata: '{"video_id":40}' },
    { id: "active", task_type: "generate_video_subtitle", status: "processing", metadata: '{"video_id":39}', message: "火山 ASR 识别第 3 段" },
  ], 39),
  { id: "active", message: "火山 ASR 识别第 3 段" },
);

assert.deepEqual(
  findActiveArchiveSubtitleTask([
    { id: "finished", task_type: "generate_archive_subtitle", status: "success", metadata: '{"platform":"douyin","room_id":"100","live_id":"200"}' },
    { id: "other-live", task_type: "generate_archive_subtitle", status: "processing", metadata: '{"platform":"douyin","room_id":"100","live_id":"201"}' },
    { id: "active", task_type: "generate_archive_subtitle", status: "processing", metadata: '{"platform":"douyin","room_id":"100","live_id":"200"}', message: "正在生成整场逐字稿" },
  ], "douyin", "100", "200"),
  { id: "active", message: "正在生成整场逐字稿" },
);

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
assert.match(
  friendlyArchiveTranscriptError("该录播已有逐字稿任务正在进行，请等待当前任务完成后再试"),
  /正在转写逐字稿/,
);
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

const malformedReviewEnvelope = normalizeBeginnerReview({}, `{"verdict":"证据不足","summary":"没有下单或成交确认，只能作为线索片段。","good_points":["主播说明了型号和成色"],"improvements":["补上价格和购买指令"],"checks":["确认是否有真实成交"],"professional_detail":"第一行
第二行"}`);
assert.equal(malformedReviewEnvelope.verdict, "证据不足");
assert.equal(malformedReviewEnvelope.summary, "没有下单或成交确认，只能作为线索片段。");
assert.deepEqual(malformedReviewEnvelope.goodPoints, ["主播说明了型号和成色"]);
assert.deepEqual(malformedReviewEnvelope.improvements, ["补上价格和购买指令"]);
assert.deepEqual(malformedReviewEnvelope.checks, ["确认是否有真实成交"]);

const cachedMalformedReview = normalizeBeginnerReview({
  summary: `{"verdict":"证据不足","summary":"没有下单或成交确认，只能作为线索片段。","good_points":["主播说明了型号和成色"],"improvements":["补上价格和购买指令"],"checks":["确认是否有真实成交"]}`,
});
assert.equal(cachedMalformedReview.verdict, "证据不足");
assert.equal(cachedMalformedReview.summary, "没有下单或成交确认，只能作为线索片段。");

const baseCandidate: CandidateInput = {
  start: 12,
  end: 90,
  type: "完整成交链路",
  confidence: "高",
  product: "R7 相机",
  evidence: "客户说明用途，主播匹配 R7，讲卖点和售后，报价后引导下单并确认成交",
  reason: "覆盖完整成交链路",
  verify: "核验订单时间",
  chainStages: COMPLETE_SALES_CHAIN_STAGES,
};

const core = normalizeCandidate({ ...baseCandidate, verificationStatus: "verified_complete" }, 0);
assert.ok(core);
assert.equal(core.tier, "核心高光");
assert.equal(core.score, 88);
assert.deepEqual(core.signals, ["完整成交链路", "高置信度"]);
assert.equal(isCompleteSalesChain(core), true);
assert.equal(isMasterScoreEligible(core), true);

const closingOnly = normalizeCandidate({
  ...baseCandidate,
  start: 39,
  end: 51,
  chainStages: ["确认成交"],
}, 1);
assert.ok(closingOnly);
assert.equal(closingOnly.type, "成交收口");
assert.equal(isCompleteSalesChain(closingOnly), false);
assert.equal(isMasterScoreEligible(closingOnly), false);

const localSalesTalk = normalizeCandidate({
  ...baseCandidate,
  start: 120,
  end: 180,
  chainStages: ["客户需求/疑问", "产品匹配"],
}, 2);
assert.ok(localSalesTalk);
assert.equal(localSalesTalk.type, "关键话术片段");
assert.equal(localSalesTalk.verificationStatus, "unverified");
assert.deepEqual(missingSalesChainStages(localSalesTalk.chainStages), [
  "卖点或价值说明",
  "风险消除/售后承诺",
  "价格/链接/优惠",
  "引导下单",
  "确认成交",
]);
assert.equal(isMasterScoreEligible(localSalesTalk), false);

const verifiedFullTalk = applyCandidateContextVerification(localSalesTalk, {
  start: 90,
  end: 210,
  type: "完整成交链路",
  chainStages: COMPLETE_SALES_CHAIN_STAGES,
  product: "R7 相机",
});
assert.equal(verifiedFullTalk.verificationStatus, "verified_complete");
assert.equal(verifiedFullTalk.type, "完整成交链路");
assert.equal(isMasterScoreEligible(verifiedFullTalk), true);

const verifiedPartialTalk = applyCandidateContextVerification(core, {
  start: 120,
  end: 180,
  type: "完整成交链路",
  chainStages: ["客户需求/疑问", "产品匹配"],
});
assert.equal(verifiedPartialTalk.verificationStatus, "partial");
assert.equal(verifiedPartialTalk.type, "关键话术片段");
assert.equal(isMasterScoreEligible(verifiedPartialTalk), false);

const malformedSavedComplete = normalizeCandidate({
  ...baseCandidate,
  chainStages: ["客户需求/疑问", "产品匹配"],
  verificationStatus: "verified_complete",
}, 5);
assert.ok(malformedSavedComplete);
assert.equal(malformedSavedComplete.type, "关键话术片段");
assert.equal(malformedSavedComplete.verificationStatus, "partial");

const explicitCandidate = normalizeCandidate({
  ...baseCandidate,
  tier: "候选高光",
  score: 72,
  signals: ["链接动作", "待核验订单"],
}, 2);
assert.ok(explicitCandidate);
assert.equal(explicitCandidate.tier, "候选高光");
assert.equal(explicitCandidate.score, 72);
assert.deepEqual(explicitCandidate.signals, ["链接动作", "待核验订单"]);

const legacyCandidate = normalizeCandidate({
  ...baseCandidate,
  type: "转品/上链接片段",
  confidence: "中",
}, 3);
assert.ok(legacyCandidate);
assert.equal(legacyCandidate.tier, "候选高光");
assert.equal(legacyCandidate.score, 62);

assert.equal(normalizeCandidate({ ...baseCandidate, end: 12 }, 4), null);

const mergedSalesCandidates = mergeSalesCandidates([
  { ...core, id: "C01", start: 100, end: 145, score: 72, evidence: "quote and link" },
  { ...core, id: "C02", start: 130, end: 175, score: 86, evidence: "order confirmed" },
]);
assert.equal(mergedSalesCandidates.length, 1);
assert.equal(mergedSalesCandidates[0].start, 100);
assert.equal(mergedSalesCandidates[0].end, 175);
assert.equal(mergedSalesCandidates[0].score, 86);
assert.match(mergedSalesCandidates[0].evidence, /quote and link/);
assert.match(mergedSalesCandidates[0].evidence, /order confirmed/);

assert.equal(mergeSalesCandidates([
  { ...core, id: "C01", start: 100, end: 130, product: "R7" },
  { ...core, id: "C02", start: 125, end: 150, product: "R50" },
]).length, 2);

assert.equal(mergeSalesCandidates([
  { ...core, id: "C01", start: 450, end: 480, score: 70 },
  { ...core, id: "C02", start: 490, end: 520, score: 80 },
]).length, 1);

const allIndependentDiscoveries = selectDiscoveryCandidates([
  { ...closingOnly, id: "C01", start: 100, end: 130 },
  { ...closingOnly, id: "C02", start: 260, end: 290 },
  { ...closingOnly, id: "C03", start: 420, end: 450 },
  { ...closingOnly, id: "C04", start: 580, end: 610 },
  { ...closingOnly, id: "C05", start: 740, end: 770 },
]);
assert.equal(allIndependentDiscoveries.length, 5);

assert.deepEqual(sessionCandidateWorkPlan(false, false), [
  "novice_review",
  "master_comparison",
]);
assert.deepEqual(sessionCandidateWorkPlan(false, true), ["novice_review"]);
assert.deepEqual(sessionCandidateWorkPlan(true, false), ["master_comparison"]);
assert.deepEqual(sessionCandidateWorkPlan(true, true), []);

const sessionCandidates = normalizeCandidates([
  { ...baseCandidate, start: 600, end: 640 },
  { ...baseCandidate, start: 700, end: 740 },
  { ...baseCandidate, start: 800, end: 840 },
]);
assert.deepEqual(summarizeSessionReview(sessionCandidates, {
  [sessionCandidates[0].id]: {
    comparison: { totalScore: 88, masterSectionId: 1, admission: "candidate_queue", risks: [] },
  },
  [sessionCandidates[1].id]: {
    comparison: { totalScore: 72, masterSectionId: null, admission: "review_only", risks: ["needs fact check"] },
  },
  stale: {
    comparison: { totalScore: 100, masterSectionId: 9, admission: "candidate_queue", risks: [] },
  },
}, {
  [sessionCandidates[0].id]: { verdict: "证据不足" },
  [sessionCandidates[1].id]: { verdict: "修改后保留" },
  stale: { verdict: "建议保留" },
}), {
  totalCandidates: 3,
  reviewedCount: 2,
  pendingCount: 1,
  matchedCount: 1,
  unmatchedCount: 1,
  highScoreCount: 1,
  riskCount: 1,
  averageScore: 80,
});

const normalized = normalizeCandidates([
  { ...baseCandidate, start: 80, end: 110, confidence: "中" },
  { ...baseCandidate, start: 20, end: 50, tier: "核心高光", score: 95 },
  { ...baseCandidate, start: 120, end: 119 },
]);
assert.equal(normalized.length, 2);
assert.equal(normalized[0].id, "C01-20-50");
assert.equal(normalized[0].tier, "核心高光");
assert.equal(normalized[1].id, "C02-80-110");

const discoveredQuote = parseDiscoverySegments({
  segments: [{
    segment_id: "SEG-001",
    start_time: "00:10:00",
    end_time: "00:10:42",
    type: "高质量金句",
    scene: "客户担心相机太重",
    customer_need: "客户需要旅行便携设备",
    original_text: "老师，带得出去比参数堆得高更重要。",
    key_sentence: "带得出去比参数堆得高更重要。",
    outcome: "unconfirmed",
    interrupted: true,
    why_selected: "一句话把便携价值讲清楚",
    evidence: [{ time: "00:10:21", quote: "带得出去比参数堆得高更重要。" }],
  }],
});
assert.equal(discoveredQuote.length, 1);
assert.equal(discoveredQuote[0].id, "SEG-001");
assert.equal(discoveredQuote[0].start, 600);
assert.equal(discoveredQuote[0].end, 642);
assert.equal(discoveredQuote[0].type, "高质量金句");
assert.equal(discoveredQuote[0].scene, "客户担心相机太重");
assert.equal(discoveredQuote[0].customerNeed, "客户需要旅行便携设备");
assert.equal(discoveredQuote[0].originalText, "老师，带得出去比参数堆得高更重要。");
assert.equal(discoveredQuote[0].keySentence, "带得出去比参数堆得高更重要。");
assert.equal(discoveredQuote[0].outcome, "unconfirmed");
assert.equal(discoveredQuote[0].interrupted, true);
assert.equal(discoveredQuote[0].whySelected, "一句话把便携价值讲清楚");
assert.deepEqual(discoveredQuote[0].evidenceItems, [
  { time: "00:10:21", quote: "带得出去比参数堆得高更重要。" },
]);
assert.equal(discoveredQuote[0].hook, "带得出去比参数堆得高更重要。");
assert.equal(discoveredQuote[0].reason, "一句话把便携价值讲清楚");
assert.equal(discoveredQuote[0].verificationStatus, "unverified");
assert.equal(parseCandidateDiscoveryResponse(`\`\`\`json
${JSON.stringify({ segments: [{
  segment_id: "SEG-FENCED",
  start_time: "01:20",
  end_time: "02:10",
  type: "需求判断",
  scene: "客户说明预算",
  customer_need: "预算内选择相机",
  original_text: "老师，您的预算是多少，主要拍什么？",
  key_sentence: "您的预算是多少，主要拍什么？",
  outcome: "unconfirmed",
  interrupted: false,
  why_selected: "先问需求再推荐",
  evidence: [{ time: "01:30", quote: "您的预算是多少，主要拍什么？" }],
}] })}
\`\`\``)[0].id, "SEG-FENCED");

const allDiscoveryTypes = [
  "完整成交链路",
  "高质量金句",
  "需求判断",
  "产品推荐",
  "产品讲解",
  "异议处理",
  "留人钩子",
  "信任建立",
  "售后与风险消除",
  "价格、优惠或链接承接",
  "催单与成交确认",
  "需要改进的反面案例",
] as const;
const discoveredTypes = parseDiscoverySegments({
  segments: allDiscoveryTypes.map((type, index) => ({
    segment_id: `SEG-${String(index + 1).padStart(3, "0")}`,
    start_time: index * 200,
    end_time: index * 200 + 60,
    type,
    scene: `场景${index + 1}`,
    customer_need: "客户需要帮助",
    original_text: `第${index + 1}段完整原话`,
    key_sentence: `第${index + 1}段关键句`,
    outcome: index === 0 ? "confirmed_conversion" : index === 1 ? "conversion_signal" : index === 11 ? "no_conversion" : "unconfirmed",
    interrupted: false,
    why_selected: "具有训练价值",
    evidence: [{ time: index * 200 + 10, quote: `第${index + 1}段证据` }],
  })),
});
assert.deepEqual(discoveredTypes.map((item) => item.type), [...allDiscoveryTypes]);
assert.deepEqual(selectDiscoveryCandidates(discoveredTypes).map((item) => item.type), [...allDiscoveryTypes]);

assert.equal(parseDiscoverySegments({ segments: [{
  segment_id: "BAD-TYPE",
  start_time: "00:00:00",
  end_time: "00:01:00",
  type: "普通聊天",
  original_text: "没有训练价值",
  key_sentence: "",
  outcome: "unconfirmed",
  interrupted: false,
  why_selected: "错误类型",
  evidence: [{ time: "00:00:10", quote: "没有训练价值" }],
}] }).length, 0);
assert.equal(parseDiscoverySegments({ segments: [{
  segment_id: "BAD-OUTCOME",
  start_time: "00:00:00",
  end_time: "00:01:00",
  type: "产品讲解",
  original_text: "有效原话",
  key_sentence: "有效原话",
  outcome: "sold",
  interrupted: false,
  why_selected: "结果枚举错误",
  evidence: [{ time: "00:00:10", quote: "有效原话" }],
}] }).length, 0);
assert.equal(parseDiscoverySegments({ segments: [{
  segment_id: "NO-EVIDENCE",
  start_time: "00:00:00",
  end_time: "00:01:00",
  type: "产品讲解",
  original_text: "有效原话",
  key_sentence: "有效原话",
  outcome: "unconfirmed",
  interrupted: false,
  why_selected: "缺少证据",
  evidence: [],
}] }).length, 0);

assert.equal(candidateOutcomeLabel("confirmed_conversion"), "已发现成交确认，等待复核");
assert.equal(candidateOutcomeLabel("conversion_signal"), "出现成交信号");
assert.equal(candidateOutcomeLabel("unconfirmed"), "尚未确认结果");
assert.equal(candidateOutcomeLabel("no_conversion"), "未形成成交");

const discoveryPrompt = buildCandidateDiscoveryPrompt("S9：松下全画幅相机");
for (const type of allDiscoveryTypes) assert.match(discoveryPrompt, new RegExp(type));
for (const outcome of ["confirmed_conversion", "conversion_signal", "unconfirmed", "no_conversion"]) {
  assert.match(discoveryPrompt, new RegExp(outcome));
}
assert.match(discoveryPrompt, /30秒至3分钟/);
assert.match(discoveryPrompt, /受到打断/);
assert.match(discoveryPrompt, /不得.*补写/);
assert.match(discoveryPrompt, /S9：松下全画幅相机/);

const discoveryCandidate = parseDiscoverySegments({ segments: [{
  segment_id: "SEG-PRESERVE",
  start_time: "00:03:00",
  end_time: "00:03:45",
  type: "高质量金句",
  scene: "客户担心二手机器质量",
  customer_need: "希望降低购买风险",
  original_text: "我们每台机器都会经过检测，有问题也有售后保障。",
  key_sentence: "先把风险讲明白，再让客户做决定。",
  outcome: "conversion_signal",
  interrupted: true,
  why_selected: "一句话同时完成信任建立和风险消除。",
  evidence: [{ time: "00:03:12", quote: "我们每台机器都会经过检测" }],
}] })[0];
const verifiedDiscoveryCandidate = applyCandidateContextVerification(discoveryCandidate, {
  start: 175,
  end: 235,
  type: "关键话术片段",
  product: "二手相机",
  evidence: "前后字幕确认是同一轮质量异议处理",
  reason: "链路不完整，但话术可训练",
  verify: "未发现明确下单",
  signals: ["风险消除"],
  chainStages: ["风险消除/售后承诺"],
  hook: "先把风险讲明白",
  takeaway: "先回应风险，再推荐商品",
});
assert.equal(verifiedDiscoveryCandidate.type, "高质量金句");
assert.equal(verifiedDiscoveryCandidate.verificationStatus, "partial");
assert.equal(verifiedDiscoveryCandidate.scene, discoveryCandidate.scene);
assert.equal(verifiedDiscoveryCandidate.originalText, discoveryCandidate.originalText);
assert.equal(verifiedDiscoveryCandidate.keySentence, discoveryCandidate.keySentence);
assert.equal(verifiedDiscoveryCandidate.outcome, discoveryCandidate.outcome);
assert.equal(verifiedDiscoveryCandidate.interrupted, true);
assert.deepEqual(verifiedDiscoveryCandidate.evidenceItems, discoveryCandidate.evidenceItems);
assert.equal(isStructuredDiscoveryCandidate(verifiedDiscoveryCandidate), true);
assert.equal(isStructuredDiscoveryCandidate(localSalesTalk), false);
assert.equal(candidateNeedsLegacyContextVerification(discoveryCandidate), false);
assert.equal(candidateNeedsLegacyContextVerification({
  ...discoveryCandidate,
  evidenceItems: [],
}), true);
assert.equal(candidateNeedsLegacyContextVerification(localSalesTalk), true);

const discoveredCompleteChain = parseDiscoverySegments({ segments: [{
  segment_id: "SEG-COMPLETE",
  start_time: "00:04:00",
  end_time: "00:06:00",
  type: "完整成交链路",
  scene: "完整商品推荐",
  customer_need: "客户需要合适的相机",
  original_text: "从需求确认到下单确认的完整原话",
  key_sentence: "我先按您的用途和预算帮您选。",
  outcome: "confirmed_conversion",
  interrupted: false,
  why_selected: "模型发现为完整链路，仍需证据核验后才能确认七步",
  evidence: [{ time: "00:05:30", quote: "已经拍下了。" }],
}] })[0];
assert.deepEqual(discoveredCompleteChain.chainStages, []);
assert.equal(isCompleteSalesChain(discoveredCompleteChain), false);

const overlappingDiscoveryCandidates = parseDiscoverySegments({ segments: [
  {
    segment_id: "SEG-OVERLAP-A",
    start_time: "00:07:40",
    end_time: "00:08:20",
    type: "异议处理",
    product: "R7 相机",
    original_text: "前半段原话",
    key_sentence: "先回应客户担心。",
    outcome: "unconfirmed",
    interrupted: false,
    why_selected: "回应质量担忧。",
    evidence: [{ time: "00:07:50", quote: "前半段证据" }],
  },
  {
    segment_id: "SEG-OVERLAP-B",
    start_time: "00:08:05",
    end_time: "00:08:45",
    type: "异议处理",
    product: "R7 相机",
    original_text: "后半段原话",
    key_sentence: "再说明检测和售后。",
    outcome: "conversion_signal",
    interrupted: true,
    why_selected: "继续消除购买风险。",
    evidence: [{ time: "00:08:25", quote: "后半段证据" }],
  },
] });
const mergedDiscoveryCandidate = mergeSalesCandidates(overlappingDiscoveryCandidates)[0];
assert.equal(mergedDiscoveryCandidate.start, 460);
assert.equal(mergedDiscoveryCandidate.end, 525);
assert.match(mergedDiscoveryCandidate.originalText, /前半段原话/);
assert.match(mergedDiscoveryCandidate.originalText, /后半段原话/);
assert.equal(mergedDiscoveryCandidate.outcome, "conversion_signal");
assert.equal(mergedDiscoveryCandidate.interrupted, true);
assert.equal(mergedDiscoveryCandidate.evidenceItems.length, 2);

console.log("archiveAnalysis domain tests passed");
