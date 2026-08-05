import assert from "node:assert/strict";
import {
  DEAL_SPEECH_CONTEXT_POST_SEC,
  DEAL_SPEECH_CONTEXT_PRE_SEC,
  DEAL_SPEECH_FALLBACK_POST_SEC,
  DEAL_SPEECH_FALLBACK_PRE_SEC,
  dealSpeechContextWindow,
  dealSpeechWindow,
  filterTranscriptWindow,
  formatWorkspaceClock,
  learningSpeechFromWindow,
  resolveDealSpeechWindow,
  type WorkspaceTranscriptEntry,
} from "./companyAnalysisWorkspace.js";
import { createLearningSegment } from "./learningSegments.js";

assert.equal(formatWorkspaceClock(3661), "01:01:01");

const window = dealSpeechWindow(600);
assert.equal(window.start, 600 - DEAL_SPEECH_FALLBACK_PRE_SEC);
assert.equal(window.end, 600 + DEAL_SPEECH_FALLBACK_POST_SEC);
assert.equal(window.anchorOffsetSec, 600);

const contextWindow = dealSpeechContextWindow(600);
assert.equal(contextWindow.start, 600 - DEAL_SPEECH_CONTEXT_PRE_SEC);
assert.equal(contextWindow.end, 600 + DEAL_SPEECH_CONTEXT_POST_SEC);

const entries: WorkspaceTranscriptEntry[] = [
  { id: 1, start: 10, end: 20, text: "a" },
  { id: 2, start: 590, end: 610, text: "b" },
  { id: 3, start: 800, end: 810, text: "c" },
];
assert.deepEqual(
  filterTranscriptWindow(entries, window.start, window.end).map((entry) => entry.id),
  [2],
);

const refined = resolveDealSpeechWindow({
  anchorOffsetSec: 600,
  refined: { start: 540, end: 630, title: "小白兔" },
});
assert.equal(refined.mode, "refined");
assert.equal(refined.start, 540);
assert.equal(refined.end, 630);

const fallback = resolveDealSpeechWindow({
  anchorOffsetSec: 600,
  refined: null,
});
assert.equal(fallback.mode, "fallback");
assert.equal(fallback.start, window.start);
assert.equal(fallback.end, window.end);

const expanded = resolveDealSpeechWindow({
  anchorOffsetSec: 600,
  refined: { start: 540, end: 630 },
  expandContext: true,
});
assert.equal(expanded.mode, "context");
assert.equal(expanded.start, contextWindow.start);
assert.equal(expanded.end, contextWindow.end);

const learningSource = learningSpeechFromWindow({
  id: "deal-600",
  window: refined,
  entries: filterTranscriptWindow(entries, refined.start, refined.end),
  productName: "小白兔",
  orderAnchorSec: 600,
});
assert.ok(learningSource);
assert.equal(learningSource!.startSec, 540);
assert.equal(learningSource!.endSec, 630);
assert.match(learningSource!.text, /b/);

const learning = createLearningSegment(learningSource!, { sourceKey: "video-1", preBufferSec: 0, postBufferSec: 0 });
assert.ok(learning);
assert.equal(learning!.startSec, 540);
assert.equal(learning!.endSec, 630);
assert.ok(learning!.endSec - learning!.startSec < DEAL_SPEECH_FALLBACK_PRE_SEC + DEAL_SPEECH_FALLBACK_POST_SEC);

const coarseLearning = learningSpeechFromWindow({
  id: "deal-coarse",
  window: fallback,
  entries: filterTranscriptWindow(entries, fallback.start, fallback.end),
  orderAnchorSec: 600,
});
assert.ok(coarseLearning);
assert.ok(
  (learning!.endSec - learning!.startSec) < (coarseLearning!.endSec - coarseLearning!.startSec),
  "refined learning span must be shorter than fallback browse window",
);

console.log("company analysis workspace passed");
