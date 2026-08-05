import { describe, expect, it } from "vitest";
import {
  buildImportedArchiveLiveId,
  getImportedVideoId,
  importedArchiveKindFromVideo,
  isImportedArchive,
  videoToImportedArchive,
} from "./importedArchive";
import type { VideoItem } from "./interface";

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
    expect(archive.live_id).toBe(buildImportedArchiveLiveId(42));
    expect(archive.platform).toBe("imported");
    expect(archive.archive_kind).toBe("company");
    expect(archive.imported_video_id).toBe(42);
    expect(isImportedArchive(archive)).toBe(true);
    expect(getImportedVideoId(archive)).toBe(42);
  });

  it("classifies competitor imports by note purpose", () => {
    expect(
      importedArchiveKindFromVideo(
        JSON.stringify({ analysisPurpose: "competitor_benchmark" }),
      ),
    ).toBe("competitor");
  });
});
