import assert from "node:assert/strict";
import {
  buildClipReviewRequest,
  buildExistingClipReviewRequest,
  clipLocalTranscript,
  clipTranscriptToSrt,
  parseClipTranscript,
  parseGeneratedDealClips,
} from "./clipReview.js";

const metadata = JSON.stringify({
  generated_clips: [{ index: 0, video_id: 88, title: "成交链路", start: 37, end: 72 }],
});
assert.deepEqual(parseGeneratedDealClips(metadata), [{
  index: 0,
  videoId: 88,
  title: "成交链路",
  start: 37,
  end: 72,
}]);
assert.deepEqual(parseGeneratedDealClips("not-json"), []);

const transcript = clipLocalTranscript([
  { id: 1, start: 35, end: 38, text: "开场" },
  { id: 2, start: 40, end: 45, text: "介绍成色" },
  { id: 3, start: 73, end: 74, text: "下一款" },
], 37, 72);
assert.deepEqual(transcript, [
  { id: 1, start: 0, end: 1, text: "开场" },
  { id: 2, start: 3, end: 8, text: "介绍成色" },
]);
assert.equal(
  clipTranscriptToSrt(transcript),
  "1\n00:00:00,000 --> 00:00:01,000\n开场\n\n2\n00:00:03,000 --> 00:00:08,000\n介绍成色\n",
);

const request = buildClipReviewRequest({
  taskId: "task-1",
  parentVideoId: 59,
  ranges: [{ start: 37, end: 72, title: "成交链路", reason: "覆盖完整成交路径" }],
  transcriptEntries: [
    { id: 2, start: 40, end: 45, text: "介绍成色" },
  ],
  videos: [{
    id: 88,
    room_id: "room",
    cover: "",
    file: "clips/clip.mp4",
    length: 35,
    size: 1,
    status: 1,
    bvid: "",
    title: "成交链路",
    note: "",
    desc: "",
    tags: "",
    area: 0,
    created_at: "2026-08-02T00:00:00Z",
    platform: "clip",
    anchor_name: "",
    anchor_source: "",
    anchor_confidence: "",
    anchor_detection_status: "",
    anchor_detection_error: "",
    anchor_detected_at: "",
  }],
  taskMetadata: metadata,
});
assert.equal(request?.items[0]?.video.id, 88);
assert.equal(request?.items[0]?.transcriptEntries[0]?.start, 3);

const parsedClipTranscript = parseClipTranscript(
  "1\n00:00:01,000 --> 00:00:03,500\n开场话术\n\n2\n00:00:04,000 --> 00:00:07,000\n成交收口\n",
);
assert.deepEqual(parsedClipTranscript, [
  { id: 0, start: 1, end: 3.5, text: "开场话术" },
  { id: 1, start: 4, end: 7, text: "成交收口" },
]);

const existingRequest = buildExistingClipReviewRequest(
  { ...request!.items[0].video, length: 45, note: "完整成交链路" },
  "1\n00:00:01,000 --> 00:00:03,500\n开场话术\n",
);
assert.equal(existingRequest.taskId, "existing-clip-88");
assert.equal(existingRequest.items[0].sourceEndSec, 45);
assert.equal(existingRequest.items[0].reason, "完整成交链路");
assert.equal(existingRequest.items[0].transcriptEntries.length, 1);

console.log("clipReview tests passed");
