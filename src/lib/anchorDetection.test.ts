import assert from "node:assert/strict";
import {
  anchorStatusLabel,
  canManuallyEditAnchor,
  shouldAutoDetectAnchor,
  type AnchorDetectionVideo,
} from "./anchorDetection.js";

function video(
  overrides: Partial<AnchorDetectionVideo> = {},
): AnchorDetectionVideo {
  return {
    anchor_name: "",
    anchor_source: "",
    anchor_detection_status: "pending",
    ...overrides,
  };
}

assert.equal(shouldAutoDetectAnchor(video()), true);
assert.equal(
  shouldAutoDetectAnchor(
    video({
      anchor_name: "小鱼",
      anchor_source: "manual",
      anchor_detection_status: "confirmed",
    }),
  ),
  false,
);
assert.equal(
  shouldAutoDetectAnchor(video({ anchor_detection_status: "running" })),
  false,
);
assert.equal(
  shouldAutoDetectAnchor(video({ anchor_detection_status: "failed" })),
  false,
);
assert.equal(
  shouldAutoDetectAnchor(
    video({
      anchor_detection_status: "failed",
      anchor_detection_error: "FFmpeg failed while reading playlist",
    }),
  ),
  false,
);
assert.equal(
  shouldAutoDetectAnchor(video({ anchor_detection_status: "not_requested" })),
  true,
);
assert.equal(anchorStatusLabel(video()), "等待识别");
assert.equal(
  anchorStatusLabel(
    video({
      anchor_name: "小鱼",
      anchor_source: "minimax_vision",
      anchor_detection_status: "confirmed",
    }),
  ),
  "",
);
assert.equal(
  anchorStatusLabel(
    video({
      anchor_name: "小鱼",
      anchor_source: "manual",
      anchor_detection_status: "confirmed",
    }),
  ),
  "人工确认",
);
assert.equal(
  anchorStatusLabel(video({ anchor_detection_status: "failed" })),
  "未识别",
);
assert.equal(
  anchorStatusLabel(video({ anchor_detection_status: "not_requested" })),
  "等待识别",
);
assert.equal(
  canManuallyEditAnchor(video({ anchor_detection_status: "failed" })),
  true,
);
assert.equal(
  canManuallyEditAnchor(video({ anchor_detection_status: "running" })),
  false,
);
assert.equal(
  canManuallyEditAnchor(
    video({
      anchor_name: "小鱼",
      anchor_detection_status: "confirmed",
    }),
  ),
  false,
);

console.log("anchor detection tests passed");
