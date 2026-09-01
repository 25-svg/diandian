import assert from "node:assert/strict";
import {
  buildSessionRhythmAnalysis,
  buildSessionRhythmUserMessage,
  chooseSessionRhythmBucketSec,
  parseSessionRhythmReview,
  sessionStructureLabel,
} from "./sessionRhythm.js";

assert.equal(chooseSessionRhythmBucketSec(3_000), 60);
assert.equal(chooseSessionRhythmBucketSec(5_000), 120);
assert.equal(chooseSessionRhythmBucketSec(10_000), 180);
assert.equal(sessionStructureLabel("conversion"), "促单成交");

const analysis = buildSessionRhythmAnalysis(
  [
    { id: 1, start: 0, end: 20, text: "欢迎新进来的朋友，今天给大家讲佳能镜头" },
    { id: 2, start: 60, end: 100, text: "这个镜头成色很好，原装配件都在" },
    { id: 3, start: 130, end: 170, text: "支持验货和质保，售后大家放心" },
    { id: 4, start: 190, end: 230, text: "到手价格三千元，优惠券在小黄车" },
    { id: 5, start: 250, end: 285, text: "需要的现在拍下下单，我给你备注" },
  ],
  [{ offsetSec: 270, payAmountFen: 300_000, productName: "佳能镜头" }],
  300,
);

assert.equal(analysis.buckets.length, 5);
assert.equal(analysis.buckets[0].kind, "opening");
assert.equal(analysis.buckets[2].kind, "trust");
assert.equal(analysis.buckets[4].kind, "conversion");
assert.equal(analysis.metrics.orderCount, 1);
assert.ok(analysis.metrics.structureCompleteness >= 60);
assert.ok(buildSessionRhythmUserMessage(analysis).includes("[bucket:4]"));

assert.deepEqual(parseSessionRhythmReview(`\n\`\`\`json\n{
  "summary": "结构完整",
  "structureVerdict": "价格到促单衔接自然",
  "goodPoints": ["信任依据清楚"],
  "improvements": ["开场更快报主题"],
}\n\`\`\``), {
  summary: "结构完整",
  structureVerdict: "价格到促单衔接自然",
  goodPoints: ["信任依据清楚"],
  improvements: ["开场更快报主题"],
});

assert.equal(parseSessionRhythmReview(`{"summary":"第一句
第二句","structureVerdict":"可执行","goodPoints":[],"improvements":[]}`).summary, "第一句\n第二句");

const empty = buildSessionRhythmAnalysis([], [], 600);
assert.equal(empty.buckets.length, 0);
assert.equal(empty.metrics.structureCompleteness, 0);

console.log("session rhythm tests passed");
