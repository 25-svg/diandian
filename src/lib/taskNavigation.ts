export type TaskNavigationTarget =
  | {
      kind: "archive";
      platform: string;
      roomId: string;
      liveId: string;
    }
  | { kind: "video"; videoId: number };

type NavigableTask = {
  task_type: string;
  metadata: string;
};

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
    task.task_type === "generate_video_subtitle" ||
    task.task_type === "prepare_video_playback"
  ) {
    const videoId = Number(metadata.video_id);
    return Number.isInteger(videoId) && videoId > 0
      ? { kind: "video", videoId }
      : null;
  }

  return null;
}

export function isMissingArchiveError(error: unknown): boolean {
  const message = String(error);
  return (
    message.includes("Entry not found") ||
    message.includes("no rows returned by a query that expected to return at least one row")
  );
}
