import assert from "node:assert/strict";
import { analyzeTranscriptSignals } from "./transcriptSignals.js";

const entries = [
  { start: 10, end: 15, text: "老师这个多少钱，预算五千。" },
  { start: 16, end: 20, text: "给你上三号链接，点小黄车。" },
  { start: 21, end: 25, text: "下单了告诉我，我给你备注。" },
  { start: 90, end: 95, text: "下一个商品。" },
];

const result = analyzeTranscriptSignals(entries, 9, 30);
assert.equal(result.inquiryCount, 2);
assert.equal(result.linkCount, 2);
assert.equal(result.conversionConfirmationCount, 2);
assert.equal(result.transitionCount, 0);
assert.match(result.label, /问价×2/);
assert.match(result.label, /成交确认×2/);

const noConversion = analyzeTranscriptSignals(entries, 9, 20);
assert.match(noConversion.label, /无成交确认/);

console.log("transcriptSignals tests passed");
