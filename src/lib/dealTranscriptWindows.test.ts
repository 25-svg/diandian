import assert from "node:assert/strict";
import {
  buildDealTranscriptWindows,
  buildTranscriptGapWindows,
  cueOverlapsDealWindow,
  excludeDealWindowCues,
  mergeTimedTranscriptCues,
  totalDealTranscriptDuration,
} from "./dealTranscriptWindows.js";

const merged = buildDealTranscriptWindows([
  { offsetSec: 200, payAmountFen: 100 },
  { offsetSec: 300, payAmountFen: 200 },
  { offsetSec: 1_000, payAmountFen: 300 },
], 1_200);
assert.deepEqual(merged, [
  { startSec: 20, endSec: 360 },
  { startSec: 820, endSec: 1_060 },
]);
assert.equal(totalDealTranscriptDuration(merged), 580);

const clamped = buildDealTranscriptWindows([
  { offsetSec: 30, payAmountFen: null },
  { offsetSec: 1_190, payAmountFen: null },
], 1_200);
assert.deepEqual(clamped, [
  { startSec: 0, endSec: 90 },
  { startSec: 1_010, endSec: 1_200 },
]);

assert.deepEqual(buildDealTranscriptWindows([], 1_200), []);

const windows = buildDealTranscriptWindows([{ offsetSec: 200, payAmountFen: 100 }], 1_200);
assert.deepEqual(windows, [{ startSec: 20, endSec: 260 }]);

const cues = [
  { id: 1, start: 0, end: 5, text: "开场" },
  { id: 2, start: 100, end: 110, text: "成交窗内" },
  { id: 3, start: 400, end: 410, text: "成交窗外" },
];
assert.equal(cueOverlapsDealWindow(cues[1], windows), true);
assert.equal(cueOverlapsDealWindow(cues[0], windows), false);
assert.deepEqual(
  excludeDealWindowCues(cues, windows).map((item) => item.id),
  [1, 3],
);
assert.deepEqual(excludeDealWindowCues(cues, []).map((item) => item.id), [1, 2, 3]);

assert.deepEqual(
  buildTranscriptGapWindows([{ startSec: 20, endSec: 260 }], 1_200, 20),
  [
    { startSec: 0, endSec: 20 },
    { startSec: 260, endSec: 1_200 },
  ],
);
assert.deepEqual(buildTranscriptGapWindows([{ startSec: 0, endSec: 1_200 }], 1_200), []);
assert.deepEqual(
  buildTranscriptGapWindows([{ startSec: 100, endSec: 110 }], 120, 20),
  [{ startSec: 0, endSec: 100 }],
);

const mergedCues = mergeTimedTranscriptCues(
  [{ id: 1, start: 10, end: 20, text: "成交" }],
  [{ id: 9, start: 0, end: 5, text: "开场" }],
);
assert.deepEqual(mergedCues.map((cue) => cue.text), ["开场", "成交"]);
assert.equal(mergedCues[0]?.id, 1);

console.log("deal transcript window tests passed");
