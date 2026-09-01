import assert from "node:assert/strict";
import { findArchiveSourceVideo } from "./archiveVideoBinding.js";

const archive: any = {
  platform: "douyin",
  room_id: "room-1",
  live_id: "live-1",
  title: "金典拍拍相机专卖店 2026-08-01 直播",
  length: 3600,
};
const source: any = {
  id: 7,
  room_id: "room-1",
  title: "",
  file: "[full][douyin][room-1][live-1]recording.mp4",
  length: 3600,
  note: "",
};

assert.equal(findArchiveSourceVideo(archive, [source])?.id, 7);
assert.equal(findArchiveSourceVideo(archive, [{ ...source, id: 8, file: "clips/short.mp4", length: 40 }]), null);
assert.equal(
  findArchiveSourceVideo({ ...archive, platform: "imported", live_id: "import:7" }, [source])?.id,
  7,
);
