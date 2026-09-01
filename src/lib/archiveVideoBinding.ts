import type { RecordItem } from "./db.js";
import type { VideoItem } from "./interface.js";
import { getImportedVideoId } from "./importedArchive.js";

function comparableText(value: string): string {
  return String(value || "")
    .toLocaleLowerCase()
    .replace(/[\s\-_()[\]{}（）【】.,，。:：/\\\\]/g, "");
}

function durationLooksEquivalent(archiveLength: number, videoLength: number): boolean {
  if (!archiveLength || !videoLength) return false;
  return Math.abs(archiveLength - videoLength) <= Math.max(120, archiveLength * 0.08);
}

function isGeneratedClip(video: Pick<VideoItem, "platform" | "file">): boolean {
  if ((video.platform || "").toLowerCase() === "clip") return true;
  const file = (video.file || "").replace(/\\/g, "/");
  return file.startsWith("clips/") || file.includes("/clips/");
}

/**
 * A finished recorder archive and the exported full-session VideoRow are
 * stored separately. Prefer an explicit imported-video id, then only accept a
 * strong deterministic match. This prevents a similarly named clip from being
 * accidentally used as the source for deal auto-clipping.
 */
export function findArchiveSourceVideo(
  archive: RecordItem,
  videos: readonly VideoItem[],
): VideoItem | null {
  const importedVideoId = getImportedVideoId(archive);
  if (importedVideoId != null) {
    return videos.find((video) => video.id === importedVideoId && !isGeneratedClip(video)) || null;
  }

  const liveId = String(archive.live_id || "").trim();
  const roomId = String(archive.room_id || "").trim();
  const archiveTitle = comparableText(archive.title || "");
  const candidates = videos
    .filter((video) => !isGeneratedClip(video))
    .map((video) => {
      const source = `${video.file || ""} ${video.note || ""}`.toLocaleLowerCase();
      const title = comparableText(video.title || "");
      const sameRoom = Boolean(roomId && video.room_id === roomId);
      const sameDuration = durationLooksEquivalent(Number(archive.length), Number(video.length));
      const titleMatches = archiveTitle.length >= 12
        && (title.includes(archiveTitle) || archiveTitle.includes(title));
      const hasLiveId = Boolean(liveId && source.includes(liveId.toLocaleLowerCase()));
      const score = hasLiveId && sameRoom
        ? 100
        : hasLiveId
          ? 90
          : sameRoom && sameDuration
            ? 80
            : titleMatches && sameDuration
              ? 70
              : 0;
      return { video, score };
    })
    .filter((candidate) => candidate.score >= 70)
    .sort((left, right) => right.score - left.score || right.video.id - left.video.id);

  return candidates[0]?.video || null;
}
