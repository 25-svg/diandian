import assert from "node:assert/strict";
import {
  analysisModeForVideo,
  isMissingArchiveError,
  taskNavigationTarget,
} from "./taskNavigation.js";

assert.equal(isMissingArchiveError("Database error: Entry not found"), true);
assert.equal(
  isMissingArchiveError(
    "Database error: DB error: no rows returned by a query that expected to return at least one row",
  ),
  true,
);
assert.equal(isMissingArchiveError("network timeout"), false);

assert.deepEqual(
  taskNavigationTarget({
    task_type: "generate_video_subtitle",
    metadata: JSON.stringify({ video_id: 42 }),
  }),
  { kind: "video", videoId: 42 },
);

assert.deepEqual(
  taskNavigationTarget({
    task_type: "generate_archive_subtitle",
    metadata: JSON.stringify({ platform: "douyin", room_id: "1", live_id: "2" }),
  }),
  { kind: "archive", platform: "douyin", roomId: "1", liveId: "2" },
);

assert.equal(
  taskNavigationTarget({
    task_type: "generate_video_subtitle",
    metadata: "{}",
  }),
  null,
);

assert.deepEqual(
  taskNavigationTarget({
    task_type: "generate_video_gap_fill_subtitle",
    metadata: JSON.stringify({ video_id: 7 }),
  }),
  { kind: "video", videoId: 7, preferredMode: "company_deal" },
);

assert.equal(
  analysisModeForVideo({
    note: JSON.stringify({ analysisPurpose: "enterprise_review" }),
    title: "任意标题",
  }),
  "company_deal",
);
assert.equal(
  analysisModeForVideo({
    note: JSON.stringify({ analysisPurpose: "competitor_benchmark" }),
    title: "金典拍拍也会被 note 覆盖",
  }),
  "legacy",
);
assert.equal(
  analysisModeForVideo({ note: "", title: "金典拍拍相机专场" }),
  "company_deal",
);
assert.equal(
  analysisModeForVideo({ note: "", title: "竞品镜头专场" }, "company_deal"),
  "company_deal",
);

console.log("taskNavigation tests passed");
