import assert from "node:assert/strict";
import { groupArchivesForDeletion } from "./archiveDelete.js";

const groups = groupArchivesForDeletion(
  [
    { platform: "bilibili", room_id: "100", live_id: "a" },
    { platform: "bilibili", room_id: "100", live_id: "b" },
    { platform: "douyin", room_id: "200", live_id: "c" },
  ],
  new Set(["a", "b", "c", "missing"])
);

assert.deepEqual(groups, [
  { platform: "bilibili", roomId: "100", liveIds: ["a", "b"] },
  { platform: "douyin", roomId: "200", liveIds: ["c"] },
]);

console.log("archive delete grouping tests passed");
