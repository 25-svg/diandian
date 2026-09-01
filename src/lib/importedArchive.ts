import type { RecordItem } from "./db";
import type { VideoItem } from "./interface";

export const IMPORTED_ARCHIVE_LIVE_ID_PREFIX = "import:";

export function buildImportedArchiveLiveId(videoId: number): string {
  return `${IMPORTED_ARCHIVE_LIVE_ID_PREFIX}${videoId}`;
}

export function isImportedArchive(archive: Pick<RecordItem, "platform" | "imported_video_id" | "live_id">): boolean {
  if (archive.platform === "imported" || archive.imported_video_id != null) {
    return true;
  }
  return archive.live_id.startsWith(IMPORTED_ARCHIVE_LIVE_ID_PREFIX);
}

export function getImportedVideoId(archive: Pick<RecordItem, "imported_video_id" | "live_id">): number | null {
  if (archive.imported_video_id != null) {
    return archive.imported_video_id;
  }
  if (!archive.live_id.startsWith(IMPORTED_ARCHIVE_LIVE_ID_PREFIX)) {
    return null;
  }
  const parsed = Number.parseInt(archive.live_id.slice(IMPORTED_ARCHIVE_LIVE_ID_PREFIX.length), 10);
  return Number.isFinite(parsed) ? parsed : null;
}

export function parseImportedVideoNote(note: string): {
  analysisPurpose?: string;
  competitorName?: string;
  masterScriptKey?: string;
} {
  try {
    return JSON.parse(note || "{}");
  } catch {
    return {};
  }
}

export function importedArchiveKindFromVideo(note: string): "company" | "competitor" {
  const parsed = parseImportedVideoNote(note);
  return parsed.analysisPurpose === "competitor_benchmark" ? "competitor" : "company";
}

export function buildImportedVideoNote(
  archiveKind: "company" | "competitor",
  existingNote: string,
): string {
  const parsed = parseImportedVideoNote(existingNote);
  return JSON.stringify({
    ...parsed,
    analysisPurpose: archiveKind === "company" ? "enterprise_review" : "competitor_benchmark",
  });
}

export function videoToImportedArchive(video: VideoItem): RecordItem {
  return {
    platform: "imported",
    title: video.title,
    parent_id: "",
    live_id: buildImportedArchiveLiveId(video.id),
    room_id: video.room_id || "bsr:import",
    length: video.length,
    size: video.size,
    created_at: video.created_at,
    cover: video.cover,
    anchor_name: video.anchor_name,
    anchor_source: video.anchor_source,
    anchor_confidence: video.anchor_confidence,
    anchor_detection_status: video.anchor_detection_status,
    anchor_detection_error: video.anchor_detection_error,
    anchor_detected_at: video.anchor_detected_at,
    archive_kind: importedArchiveKindFromVideo(video.note),
    classification_source: "manual",
    source_path: "",
    imported_video_id: video.id,
  };
}

export function mergeVideoIntoImportedArchive(
  archive: RecordItem,
  video: VideoItem,
): RecordItem {
  return {
    ...archive,
    title: video.title,
    length: video.length,
    size: video.size,
    created_at: video.created_at,
    cover: video.cover || archive.cover,
    anchor_name: video.anchor_name,
    anchor_source: video.anchor_source,
    anchor_confidence: video.anchor_confidence,
    anchor_detection_status: video.anchor_detection_status,
    anchor_detection_error: video.anchor_detection_error,
    anchor_detected_at: video.anchor_detected_at,
    archive_kind: importedArchiveKindFromVideo(video.note),
  };
}
