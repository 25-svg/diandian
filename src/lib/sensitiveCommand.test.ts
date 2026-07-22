import assert from "node:assert/strict";
import { buildSensitiveCommandArgs } from "./sensitiveCommand.js";

const desktopArgs = buildSensitiveCommandArgs(
  "delete_video",
  { id: 42 },
  true,
  "delete-video-42-test",
  "trace-delete-42"
);

assert.deepEqual(desktopArgs, {
  id: 42,
  idempotencyKey: "delete-video-42-test",
  confirmationToken: "confirm:delete_video",
  traceId: "trace-delete-42",
});

const httpArgs = buildSensitiveCommandArgs(
  "delete_video",
  { id: 42 },
  false,
  "delete-video-42-test",
  "trace-delete-42"
);

assert.deepEqual(httpArgs, {
  id: 42,
  idempotency_key: "delete-video-42-test",
  confirmation_token: "confirm:delete_video",
  trace_id: "trace-delete-42",
});

console.log("sensitive command argument tests passed");
