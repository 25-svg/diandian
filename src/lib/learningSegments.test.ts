import assert from "node:assert/strict";
import {
  addLearningSegment,
  createLearningSegment,
  formatLearningSegmentTime,
  removeLearningSegment,
} from "./learningSegments.js";

const segment = createLearningSegment({
  id: 42,
  startSec: 100,
  endSec: 115,
  text: "现在下单，链接就在左下角。",
  productName: "Sony A7M4",
  orderAnchorSec: 112,
}, { sourceKey: "archive-20260801" });

assert.ok(segment);
assert.equal(segment!.startSec, 88);
assert.equal(segment!.endSec, 133);
assert.equal(segment!.preBufferSec, 12);
assert.equal(segment!.postBufferSec, 18);
assert.equal(segment!.sourceSpeechId, "42");

const capped = createLearningSegment({ id: "a", startSec: 20, endSec: 35, text: "成交收口" }, {
  sourceKey: "archive-a", preBufferSec: 20, postBufferSec: 20, maxDurationSec: 25,
});
assert.ok(capped);
assert.equal(capped!.endSec - capped!.startSec, 25);

assert.equal(createLearningSegment({ id: 1, startSec: 8, endSec: 8, text: "无效" }, { sourceKey: "x" }), null);
assert.equal(createLearningSegment({ id: 1, startSec: 1, endSec: 2, text: "" }, { sourceKey: "x" }), null);

const list = addLearningSegment([], segment!);
assert.equal(list.length, 1);
assert.equal(addLearningSegment(list, segment!).length, 1, "same transcript row must not be added twice");
assert.equal(removeLearningSegment(list, segment!.id).length, 0);
assert.equal(formatLearningSegmentTime(3661), "01:01:01");

console.log("learningSegments tests passed");
