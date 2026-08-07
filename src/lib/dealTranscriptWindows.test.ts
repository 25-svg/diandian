import assert from "node:assert/strict";
import {
  buildDealTranscriptWindows,
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
console.log("deal transcript window tests passed");
