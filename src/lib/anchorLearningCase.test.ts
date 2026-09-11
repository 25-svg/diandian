import assert from "node:assert/strict";
import {
  activeLearningCaseLine,
  buildPublicLearningCaseSearch,
  readLearningCaseDetail,
} from "./anchorLearningCase.js";

const validPrivateDetail = {
  caseId: "c1", assetId: "a1", anchorId: "anchor-a", anchorName: "主播A",
  title: "价格异议处理", stage: "objection", skill: "价格异议处理",
  productCategory: "相机", difficulty: "beginner", evidenceLevel: "D",
  evidenceSummary: "仅为AI初步拆解，等待人工审核", sceneContext: "观众认为价格偏高",
  audienceTrigger: "为什么比别家贵", trainingGoal: "先确认需求再提供证据",
  expressionReason: "短句承接", logicReason: "先需求后证据", trustReason: "只说已核验事实",
  actionReason: "引导继续看实物", reusableOutline: "承接→追问→证据→行动",
  forbiddenCopy: "价格、库存、赠品、成色、售后和链接不得照搬",
  traineeReference: "先确认您的[用途]，再看[已核验事实]", factSlots: ["用途", "已核验事实"],
  applicableScope: "二手相机价格异议", expiryConditions: "价格、库存或售后规则变化时复审",
  reviewDueAt: "2026-12-31", durationMs: 3000,
  internalUseConfirmed: true, videoPath: "C:/fixtures/case.mp4", videoStartMs: 1000, videoEndMs: 4000,
  reviewStatus: "candidate", retiredAt: null, publishedAt: null,
  lines: [{ lineId: "l1", startMs: 1000, endMs: 2500, originalText: "先问用途", function: "需求确认", timingReason: "承接评论", technique: "追问", trustMechanism: "", actionCue: "停顿", riskNote: "", reusablePattern: "先问[用途]" }],
  sources: [], tags: ["价格异议"],
} as const;

assert.deepEqual(buildPublicLearningCaseSearch({ query: "嫌贵", evidenceLevels: ["A", "D"] }), {
  query: "嫌贵",
  anchorId: null,
  stage: null,
  skill: null,
  difficulty: null,
  evidenceLevels: ["A"],
});
assert.equal(activeLearningCaseLine([
  { lineId: "l1", startMs: 1000, endMs: 2500, originalText: "先问用途", function: "需求确认", timingReason: "承接评论", technique: "追问", trustMechanism: "", actionCue: "停顿", riskNote: "", reusablePattern: "先问[用途]" },
], 1800)?.lineId, "l1");
assert.equal(readLearningCaseDetail({ caseId: "x", evidenceLevel: "D" }), null);
assert.equal(readLearningCaseDetail(validPrivateDetail, { scope: "private" })?.evidenceLevel, "D");

const validPublicDetail = { ...validPrivateDetail, evidenceLevel: "A", reviewStatus: "published", publishedAt: "2026-09-01" };

assert.deepEqual(buildPublicLearningCaseSearch({ evidenceLevels: ["C", "A", "C", "D", "B", "B"] }), {
  query: "",
  anchorId: null,
  stage: null,
  skill: null,
  difficulty: null,
  evidenceLevels: ["C", "A", "B"],
});
assert.equal(readLearningCaseDetail(validPublicDetail)?.caseId, "c1");
assert.equal(readLearningCaseDetail({ ...validPublicDetail, stage: "unknown" }), null);
assert.equal(readLearningCaseDetail({ ...validPublicDetail, difficulty: "expert" }), null);
assert.equal(readLearningCaseDetail({ ...validPublicDetail, evidenceLevel: "E" }), null);
assert.equal(readLearningCaseDetail({ ...validPublicDetail, reviewStatus: "draft" }), null);
assert.equal(readLearningCaseDetail({ ...validPublicDetail, assetId: "" }), null);
assert.equal(readLearningCaseDetail({ ...validPublicDetail, videoStartMs: 4000, videoEndMs: 1000 }), null);
assert.equal(readLearningCaseDetail({ ...validPublicDetail, lines: [{ ...validPublicDetail.lines[0], originalText: "" }] }), null);
assert.equal(readLearningCaseDetail({
  ...validPublicDetail,
  lines: [
    validPublicDetail.lines[0],
    { ...validPublicDetail.lines[0], lineId: "l2", startMs: 2000, endMs: 3500 },
  ],
}), null);
assert.equal(readLearningCaseDetail({
  ...validPublicDetail,
  lines: [
    validPublicDetail.lines[0],
    { ...validPublicDetail.lines[0], lineId: "l2", startMs: 500, endMs: 900 },
  ],
}), null);
assert.equal(activeLearningCaseLine(validPublicDetail.lines, 2500), null);

console.log("anchor learning case tests passed");
