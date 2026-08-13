import assert from "node:assert/strict";
import { dealSpeechRefineStorageKey, parseSavedDealSpeechRefine } from "./dealSpeechRefinePersistence.js";

assert.equal(
  dealSpeechRefineStorageKey("archive:douyin:room:live"),
  "bsr:deal-speech-refine:v1:archive:douyin:room:live",
);
assert.deepEqual(parseSavedDealSpeechRefine(JSON.stringify({
  "120:180:99新c": {
    start: 90,
    end: 200,
    title: "99新 C 完整成交话术",
    reason: "从成色讲到下单",
    payAnchorSec: 1720,
    peakMinuteIndex: 28,
  },
})), {
  "120:180:99新c": {
    start: 90,
    end: 200,
    title: "99新 C 完整成交话术",
    reason: "从成色讲到下单",
    payAnchorSec: 1720,
    peakMinuteIndex: 28,
  },
});
assert.deepEqual(parseSavedDealSpeechRefine("not-json"), {});
assert.deepEqual(parseSavedDealSpeechRefine(JSON.stringify({ bad: { start: 1 } })), {});

console.log("deal speech refine persistence tests passed");
