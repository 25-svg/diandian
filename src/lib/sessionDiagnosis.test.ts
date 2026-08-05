import assert from "node:assert/strict";
import { buildSessionDiagnosis } from "./sessionDiagnosis.js";
import type { HighlightCandidate, SessionReviewSummary } from "./archiveAnalysis.js";

const candidate = (input: Partial<HighlightCandidate> & Pick<HighlightCandidate, "id" | "type">): HighlightCandidate => ({
  id: input.id,
  start: 10,
  end: 80,
  type: input.type,
  confidence: "高",
  tier: "候选高光",
  score: 0,
  product: "佳能 R7",
  evidence: "00:20 原话",
  reason: "有训练价值",
  verify: "",
  signals: [],
  chainStages: [],
  hook: "",
  takeaway: "",
  verificationStatus: "partial",
  scene: "",
  customerNeed: "",
  originalText: "",
  keySentence: "",
  outcome: "unconfirmed",
  interrupted: false,
  whySelected: "",
  evidenceItems: [],
  ...input,
});

const stats: SessionReviewSummary = {
  totalCandidates: 3,
  reviewedCount: 2,
  pendingCount: 1,
  matchedCount: 2,
  unmatchedCount: 0,
  highScoreCount: 1,
  riskCount: 1,
  averageScore: 78,
};

const diagnosis = buildSessionDiagnosis({
  candidates: [
    candidate({ id: "SEG-001", type: "完整成交链路", scene: "质量异议处理" }),
    candidate({
      id: "SEG-002",
      type: "高质量金句",
      scene: "留人承接",
      keySentence: "先告诉我预算，我帮你把选择范围缩小。",
      interrupted: true,
      verify: "价格待确认",
    }),
    candidate({ id: "SEG-003", type: "产品讲解" }),
  ],
  masterComparisons: {
    "SEG-001": {
      comparison: {
        totalScore: 88,
        masterSectionId: 9,
        admission: "candidate_queue",
        risks: [],
      },
    },
    "SEG-002": {
      comparison: {
        totalScore: 68,
        masterSectionId: 10,
        admission: "review_only",
        risks: ["库存待确认"],
      },
    },
  },
  auditBundle: { pendingCriticalCount: 2 },
  sessionReviewSummary: stats,
});

assert.equal(diagnosis.headline, "发现 1 段高价值候选，须人工审核后沉淀");
assert.match(diagnosis.highlights[0], /质量异议处理.*88 分/);
assert.ok(diagnosis.issues.some((item) => item.includes("1 段未完成")));
assert.ok(diagnosis.issues.some((item) => item.includes("受到场内沟通打断")));
assert.ok(diagnosis.confirmations.some((item) => item.includes("2 条关键内容")));
assert.ok(diagnosis.confirmations.some((item) => item.includes("价格待确认")));
assert.ok(diagnosis.nextActions.some((item) => item.includes("85 分及以上")));
assert.ok(diagnosis.headline.length <= 45);
assert.ok(diagnosis.highlights.length <= 3);
assert.ok(diagnosis.issues.length <= 3);
assert.ok(diagnosis.confirmations.length <= 3);
assert.ok(diagnosis.nextActions.length <= 3);

const empty = buildSessionDiagnosis({
  candidates: [],
  masterComparisons: {},
  auditBundle: null,
  sessionReviewSummary: { ...stats, totalCandidates: 0, reviewedCount: 0, pendingCount: 0 },
});
assert.match(empty.headline, /暂未发现/);

console.log("sessionDiagnosis tests passed");
