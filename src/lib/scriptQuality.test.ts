import assert from "node:assert/strict";
import {
  mergeScriptQualityAnnotations,
  parseScriptQualityBundle,
  parseScriptQualityResponse,
  splitTranscriptForScriptQuality,
} from "./scriptQuality.js";

const entries = [
  { id: 0, start: 0, end: 3, text: "欢迎来到直播间" },
  { id: 1, start: 10, end: 15, text: "这只小白兔 99 新" },
];

const chunks = splitTranscriptForScriptQuality(entries, 480, 30);
assert.equal(chunks.length, 1);
assert.match(chunks[0].text, /\[cue:1\]/);

const parsed = parseScriptQualityResponse(
  JSON.stringify({
    summary: "测试",
    annotations: [{
      cueId: 1,
      kind: "factual_risk",
      reason: "未报成色依据",
      suggestion: "[建议稿] 先说明验货结论再报价",
    }],
  }),
  new Map(entries.map((entry) => [entry.id, entry])),
);
assert.equal(parsed.length, 1);
assert.equal(parsed[0].kind, "factual_risk");

const merged = mergeScriptQualityAnnotations([
  parsed,
  [{
    cueId: 1,
    startMs: 10000,
    endMs: 15000,
    kind: "compliance_risk",
    originalText: "永久保修",
    reason: "过度承诺",
    suggestion: "[建议稿] 改为官方质保口径",
  }],
]);
assert.equal(merged.length, 1);
assert.equal(merged[0].kind, "compliance_risk");

const brokenNewline = parseScriptQualityResponse(
  `{"summary":"摘要","annotations":[{"cueId":1,"kind":"unclear_expression","reason":"第一句
第二句未转义换行","suggestion":"[建议稿] 合并表达"}]}`,
  new Map(entries.map((entry) => [entry.id, entry])),
);
assert.equal(brokenNewline.length, 1);
assert.match(brokenNewline[0].reason, /第一句/);

console.log("script quality passed");
