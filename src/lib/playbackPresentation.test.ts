import assert from "node:assert/strict";
import {
  playbackPresentation,
  shouldAutoPreparePlayback,
} from "./playbackPresentation.js";

const pendingSource = {
  requiresPreparation: true,
  ready: false,
  preparing: false,
};

assert.equal(
  playbackPresentation({ ...pendingSource, preparing: true }),
  "player",
  "the original recording mounts before any fallback conversion",
);
assert.equal(
  playbackPresentation({ ...pendingSource, ready: true }),
  "player",
  "only a ready playback copy may mount the player",
);
assert.equal(
  playbackPresentation(pendingSource, true),
  "retry",
  "a failed raw playback needs a retry state",
);
assert.equal(
  shouldAutoPreparePlayback(pendingSource),
  true,
  "an idle TS conversion should be automatically queued",
);
assert.equal(
  shouldAutoPreparePlayback({ ...pendingSource, preparing: true }),
  false,
  "an active conversion must not be queued again",
);
