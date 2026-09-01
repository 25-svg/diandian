import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  buildImportedArchiveLiveId,
  getImportedVideoId,
  importedArchiveKindFromVideo,
  isImportedArchive,
  videoToImportedArchive,
} from "./importedArchive.js";
import type { VideoItem } from "./interface.js";

function sampleVideo(overrides: Partial<VideoItem> = {}): VideoItem {
  return {
    id: 42,
    room_id: "bsr:import",
    cover: "cover.jpg",
    file: "imported.mp4",
    length: 3600,
    size: 1024,
    status: 1,
    bvid: "",
    title: "7/28 金典拍拍相机专场",
    note: JSON.stringify({ analysisPurpose: "enterprise_review" }),
    desc: "",
    tags: "",
    area: 0,
    created_at: "2026-07-30T08:00:00+08:00",
    platform: "imported",
    anchor_name: "",
    anchor_source: "",
    anchor_confidence: "",
    anchor_detection_status: "pending",
    anchor_detection_error: "",
    anchor_detected_at: "",
    ...overrides,
  };
}

describe("importedArchive", () => {
  it("maps imported videos into archive rows", () => {
    const archive = videoToImportedArchive(sampleVideo());
    assert.equal(archive.live_id, buildImportedArchiveLiveId(42));
    assert.equal(archive.platform, "imported");
    assert.equal(archive.archive_kind, "company");
    assert.equal(archive.imported_video_id, 42);
    assert.equal(isImportedArchive(archive), true);
    assert.equal(getImportedVideoId(archive), 42);
  });

  it("classifies competitor imports by note purpose", () => {
    assert.equal(
      importedArchiveKindFromVideo(
        JSON.stringify({ analysisPurpose: "competitor_benchmark" }),
      ),
      "competitor",
    );
  });
});
