import assert from "node:assert/strict";
import {
  chunkArchiveDeleteGroups,
  groupArchivesForDeletion,
} from "./archiveDelete.js";

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

const chunked = chunkArchiveDeleteGroups(
  [{ platform: "douyin", roomId: "1", liveIds: Array.from({ length: 45 }, (_, i) => `id-${i}`) }],
  20
);
assert.equal(chunked.length, 3);
assert.equal(chunked[0]?.liveIds.length, 20);
assert.equal(chunked[1]?.liveIds.length, 20);
assert.equal(chunked[2]?.liveIds.length, 5);

console.log("archive delete grouping tests passed");
