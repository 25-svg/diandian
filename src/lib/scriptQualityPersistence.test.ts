import assert from "node:assert/strict";
import { parseSavedScriptQuality, scriptQualityStorageKey } from "./scriptQualityPersistence.js";

assert.equal(scriptQualityStorageKey("archive:douyin:room:live"), "bsr:script-quality:v2:archive:douyin:room:live");
assert.deepEqual(parseSavedScriptQuality(JSON.stringify({
  summary: "需要补充验货依据",
  annotations: [{ cueId: 2, startMs: 3000, endMs: 6000, kind: "unclear_expression", originalText: "这个很好", reason: "模糊", suggestion: "[建议稿] 说明成色依据" }],
  rhythmReview: {
    summary: "整体推进稳定",
    structureVerdict: "信任到价格衔接清楚",
    goodPoints: ["商品依据明确"],
    improvements: ["开场更快报主题"],
  },
})), {
  summary: "需要补充验货依据",
  annotations: [{ cueId: 2, startMs: 3000, endMs: 6000, kind: "unclear_expression", originalText: "这个很好", reason: "模糊", suggestion: "[建议稿] 说明成色依据" }],
  rhythmReview: {
    summary: "整体推进稳定",
    structureVerdict: "信任到价格衔接清楚",
    goodPoints: ["商品依据明确"],
    improvements: ["开场更快报主题"],
  },
});
assert.deepEqual(parseSavedScriptQuality(JSON.stringify({ summary: "旧版", annotations: [] })), {
  summary: "旧版",
  annotations: [],
  rhythmReview: null,
});
assert.equal(parseSavedScriptQuality("not-json"), null);

console.log("script quality persistence tests passed");
