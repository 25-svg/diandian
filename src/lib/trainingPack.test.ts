import assert from "node:assert/strict";
import {
  buildTrainingPack,
  formatTrainingPackMarkdown,
  trainingPackFileName,
} from "./trainingPack.js";
import type { MasterBaseline, SupportCandidate } from "./masterScript.js";

const baseline = {
  master: { id: 1, scriptKey: "MASTER", version: "1.2.0", title: "金典拍拍企业话术" },
  sections: [{
    id: 10,
    masterScriptId: 1,
    sectionKey: "quality-objection",
    position: 1,
    sectionKind: "scenario",
    productCardId: null,
    title: "质量异议处理",
    masterText: "先确认顾虑，再说明检测证据。",
  }],
} as MasterBaseline;
const approved = {
  id: 8,
  masterSectionId: 10,
  hostText: "你担心暗病很正常，我们先把检测项给你看清楚。",
  totalScore: 89,
  sourceKey: "video:42",
  sourceStartMs: 61_000,
  sourceEndMs: 90_000,
  status: "approved",
} as SupportCandidate;

const pack = buildTrainingPack({ baseline, candidates: [approved], generatedAt: "2026-07-27" });
const markdown = formatTrainingPackMarkdown(pack);
assert.equal(pack.sections[0].goldenSegments.length, 1);
assert.match(markdown, /企业母稿范例/);
assert.match(markdown, /已审定金牌片段/);
assert.match(markdown, /video:42｜01:01—01:30/);
assert.match(markdown, /不得把 AI 建议当作标准稿/);
assert.doesNotMatch(markdown, /请照着念/);
assert.equal(trainingPackFileName(pack), "金典拍拍企业话术｜母稿培养包-V1.2.0.md");

console.log("trainingPack tests passed");
