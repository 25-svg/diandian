import assert from "node:assert/strict";
import {
  buildOptimizationPlan,
  formatOptimizationPlanMarkdown,
  optimizationPlanFileName,
} from "./optimizationPlan.js";
import type { HighlightCandidate } from "./archiveAnalysis.js";
import type { MasterComparisonResult } from "./masterScript.js";

const candidate = {
  id: "SEG-001",
  type: "异议处理",
  scene: "客户担心二手相机质量",
  product: "佳能 R7",
} as HighlightCandidate;
const comparison = {
  comparison: {
    totalScore: 88,
    suggestedInsertionPoint: "异议处理/质量担忧",
    improvements: ["补充了检测过程的自然说明"],
    verdict: "建议进入候选辅稿",
  },
} as MasterComparisonResult;
const diagnosis = {
  headline: "发现 1 段高价值候选，须人工审核后沉淀",
  highlights: ["质量异议处理：88 分"],
  issues: ["成交确认仍不完整"],
  confirmations: ["价格待确认"],
  nextActions: ["人工核对价格与链接"],
  stats: {
    totalCandidates: 1,
    reviewedCount: 1,
    pendingCount: 0,
    matchedCount: 1,
    unmatchedCount: 0,
    highScoreCount: 1,
    riskCount: 0,
    averageScore: 88,
  },
};

const plan = buildOptimizationPlan({
  sessionTitle: "7 月 27 日晚场",
  diagnosedAt: "2026-07-27 15:00",
  masterScriptVersion: "V1.1.0",
  diagnosis,
  candidates: [candidate],
  comparisons: { "SEG-001": comparison },
  reviews: { "SEG-001": { trainingChecklist: "- 练习先复述顾虑\n- 再展示检测证据" } },
});
const markdown = formatOptimizationPlanMarkdown(plan);

assert.equal(plan.masterSectionUpdates[0].score, 88);
assert.deepEqual(plan.practiceSegments[0].discussionPoints, ["练习先复述顾虑", "再展示检测证据"]);
assert.match(markdown, /# 下场优化计划｜7 月 27 日晚场/);
assert.match(markdown, /不是企业标准稿/);
assert.match(markdown, /## 待确认事实[\s\S]*价格待确认/);
assert.match(markdown, /## 建议提交母稿审核[\s\S]*SEG-001｜88 分/);
assert.equal(optimizationPlanFileName("A/B:晚场"), "A_B_晚场-下场优化计划.md");

console.log("optimizationPlan tests passed");
