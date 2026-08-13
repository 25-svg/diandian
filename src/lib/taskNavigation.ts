export type TaskAnalysisMode = "legacy" | "company_deal";

export type TaskNavigationTarget =
  | {
      kind: "archive";
      platform: string;
      roomId: string;
      liveId: string;
      /** Prefer company workspace when the archive is company-owned. */
      preferredMode?: TaskAnalysisMode;
    }
  | {
      kind: "video";
      videoId: number;
      /** Prefer company workspace for enterprise / 金典 company recordings. */
      preferredMode?: TaskAnalysisMode;
    };

type NavigableTask = {
  task_type: string;
  metadata: string;
};

const COMPANY_VIDEO_TASK_TYPES = new Set([
  "generate_video_deal_window_subtitle",
  "generate_video_gap_fill_subtitle",
  "deal_auto_clip_batch",
]);

function readPositiveInt(value: unknown): number | null {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

export function taskNavigationTarget(
  task: NavigableTask,
): TaskNavigationTarget | null {
  let metadata: Record<string, unknown>;
  try {
    metadata = JSON.parse(task.metadata || "{}");
  } catch {
    return null;
  }

  if (task.task_type === "generate_archive_subtitle") {
    const platform = String(metadata.platform || "").trim();
    const roomId = String(metadata.room_id || "").trim();
    const liveId = String(metadata.live_id || "").trim();
    return platform && roomId && liveId
      ? { kind: "archive", platform, roomId, liveId }
      : null;
  }

  if (
    task.task_type === "generate_video_subtitle"
    || task.task_type === "prepare_video_playback"
  ) {
    const videoId = readPositiveInt(metadata.video_id);
    return videoId != null ? { kind: "video", videoId } : null;
  }

  if (COMPANY_VIDEO_TASK_TYPES.has(task.task_type)) {
    const videoId = readPositiveInt(metadata.video_id)
      ?? readPositiveInt(metadata.parent_video_id);
    return videoId != null
      ? { kind: "video", videoId, preferredMode: "company_deal" }
      : null;
  }

  return null;
}

/** Decide analysis shell when opening a full-session video from tasks. */
export function analysisModeForVideo(
  video: {
    note?: string | null;
    title?: string | null;
    anchor_name?: string | null;
  },
  preferredMode?: TaskAnalysisMode,
): TaskAnalysisMode {
  if (preferredMode === "company_deal") return "company_deal";
  if (preferredMode === "legacy") return "legacy";
  try {
    const parsed = JSON.parse(video.note || "{}") as { analysisPurpose?: string };
    if (parsed.analysisPurpose === "competitor_benchmark") return "legacy";
    if (parsed.analysisPurpose === "enterprise_review") return "company_deal";
  } catch {
    // fall through to title/anchor heuristics
  }
  const text = `${video.title || ""} ${video.anchor_name || ""}`;
  return text.includes("金典拍拍") ? "company_deal" : "legacy";
}

export function isMissingArchiveError(error: unknown): boolean {
  const message = String(error);
  return (
    message.includes("Entry not found") ||
    message.includes("no rows returned by a query that expected to return at least one row")
  );
}
