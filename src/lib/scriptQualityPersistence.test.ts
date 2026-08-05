import assert from "node:assert/strict";
import { parseSavedScriptQuality, scriptQualityStorageKey } from "./scriptQualityPersistence.js";

assert.equal(scriptQualityStorageKey("archive:douyin:room:live"), "bsr:script-quality:v1:archive:douyin:room:live");
assert.deepEqual(parseSavedScriptQuality(JSON.stringify({
  summary: "需要补充验货依据",
  annotations: [{ cueId: 2, startMs: 3000, endMs: 6000, kind: "unclear_expression", originalText: "这个很好", reason: "模糊", suggestion: "[建议稿] 说明成色依据" }],
})), {
  summary: "需要补充验货依据",
  annotations: [{ cueId: 2, startMs: 3000, endMs: 6000, kind: "unclear_expression", originalText: "这个很好", reason: "模糊", suggestion: "[建议稿] 说明成色依据" }],
});
assert.equal(parseSavedScriptQuality("not-json"), null);

console.log("script quality persistence tests passed");
