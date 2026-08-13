import type { WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";
import type { VideoItem } from "./interface.js";

export type ClipReviewRange = {
  start: number;
  end: number;
  title: string;
  reason: string;
};

export type GeneratedDealClip = {
  index: number;
  videoId: number;
  title: string;
  start: number;
  end: number;
};

export type ClipReviewItem = {
  video: VideoItem;
  sourceStartSec: number;
  sourceEndSec: number;
  reason: string;
  transcriptEntries: WorkspaceTranscriptEntry[];
};

export type ClipReviewRequest = {
  taskId: string;
  parentVideoId: number;
  items: ClipReviewItem[];
};

function srtTimeToSeconds(value: string): number {
  const parts = value.replace(",", ".").split(":").map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part))) return 0;
  return parts[0] * 3600 + parts[1] * 60 + parts[2];
}

export function parseClipTranscript(value: string): WorkspaceTranscriptEntry[] {
  return value
    .trim()
    .split(/\r?\n\r?\n+/)
    .map((block, index) => {
      const lines = block.split(/\r?\n/);
      const timingIndex = lines.findIndex((line) => line.includes("-->"));
      if (timingIndex < 0) return null;
      const [from, to] = lines[timingIndex].split("-->").map((item) => item.trim());
      const text = lines.slice(timingIndex + 1).join(" ").trim();
      if (!text) return null;
      const start = srtTimeToSeconds(from);
      const end = srtTimeToSeconds(to);
      if (end <= start) return null;
      return { id: index, start, end, text };
    })
    .filter(Boolean) as WorkspaceTranscriptEntry[];
}

export function buildExistingClipReviewRequest(
  video: VideoItem,
  subtitle: string,
): ClipReviewRequest {
  const transcriptEntries = parseClipTranscript(subtitle);
  const transcriptEnd = transcriptEntries.reduce((end, entry) => Math.max(end, entry.end), 0);
  const duration = Math.max(0, Number(video.length) || 0, transcriptEnd);
  return {
    taskId: `existing-clip-${video.id}`,
    parentVideoId: video.id,
    items: [{
      video,
      sourceStartSec: 0,
      sourceEndSec: duration,
      reason: video.note?.trim() || "已生成切片",
      transcriptEntries,
    }],
  };
}

function readFiniteNumber(value: unknown): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function parseGeneratedDealClips(metadata: string): GeneratedDealClip[] {
  let payload: unknown;
  try {
    payload = JSON.parse(metadata || "{}");
  } catch {
    return [];
  }
  const record = payload && typeof payload === "object" && !Array.isArray(payload)
    ? payload as Record<string, unknown>
    : null;
  const clips = Array.isArray(record?.generated_clips) ? record.generated_clips : [];
  return clips.flatMap((value) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return [];
    const row = value as Record<string, unknown>;
    const index = readFiniteNumber(row.index);
    const videoId = readFiniteNumber(row.video_id ?? row.videoId);
    const start = readFiniteNumber(row.start);
    const end = readFiniteNumber(row.end);
    if (index == null || videoId == null || start == null || end == null || end <= start) return [];
    return [{
      index: Math.trunc(index),
      videoId: Math.trunc(videoId),
      title: String(row.title || "").trim(),
      start,
      end,
    }];
  });
}

export function clipLocalTranscript(
  entries: readonly WorkspaceTranscriptEntry[],
  start: number,
  end: number,
): WorkspaceTranscriptEntry[] {
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return [];
  return entries
    .filter((entry) => entry.end >= start && entry.start <= end && entry.text.trim())
    .map((entry) => ({
      id: entry.id,
      start: Math.max(0, entry.start - start),
      end: Math.min(end - start, Math.max(0, entry.end - start)),
      text: entry.text,
    }))
    .filter((entry) => entry.end > entry.start);
}

function srtClock(seconds: number): string {
  const totalMs = Math.max(0, Math.round(seconds * 1000));
  const hours = Math.floor(totalMs / 3_600_000);
  const minutes = Math.floor((totalMs % 3_600_000) / 60_000);
  const secs = Math.floor((totalMs % 60_000) / 1000);
  const millis = totalMs % 1000;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")},${String(millis).padStart(3, "0")}`;
}

export function clipTranscriptToSrt(entries: readonly WorkspaceTranscriptEntry[]): string {
  return [...entries]
    .filter((entry) => entry.end > entry.start && entry.text.trim())
    .sort((left, right) => left.start - right.start || left.id - right.id)
    .map((entry, index) => `${index + 1}\n${srtClock(entry.start)} --> ${srtClock(entry.end)}\n${entry.text.trim()}\n`)
    .join("\n");
}

export function buildClipReviewRequest(options: {
  taskId: string;
  parentVideoId: number;
  ranges?: readonly ClipReviewRange[];
  transcriptEntries: readonly WorkspaceTranscriptEntry[];
  videos: readonly VideoItem[];
  taskMetadata: string;
}): ClipReviewRequest | null {
  const videoById = new Map(options.videos.map((video) => [video.id, video]));
  const items = parseGeneratedDealClips(options.taskMetadata).flatMap((generated) => {
    const video = videoById.get(generated.videoId);
    if (!video) return [];
    const range = options.ranges?.[generated.index];
    return [{
      video,
      sourceStartSec: generated.start,
      sourceEndSec: generated.end,
      reason: range?.reason || generated.title || "",
      transcriptEntries: clipLocalTranscript(
        options.transcriptEntries,
        generated.start,
        generated.end,
      ),
    }];
  });
  return items.length
    ? {
        taskId: options.taskId,
        parentVideoId: options.parentVideoId,
        items: sortClipReviewItems(items),
      }
    : null;
}

function sortClipReviewItems(items: ClipReviewItem[]): ClipReviewItem[] {
  return [...items].sort((left, right) =>
    left.sourceStartSec - right.sourceStartSec || left.sourceEndSec - right.sourceEndSec
  );
}

export function mergeClipReviewRequest(
  current: ClipReviewRequest | null,
  incoming: ClipReviewRequest,
): ClipReviewRequest {
  const byId = new Map((current?.items ?? []).map((item) => [item.video.id, item]));
  for (const item of incoming.items) {
    if (!byId.has(item.video.id)) byId.set(item.video.id, item);
  }
  return {
    taskId: incoming.taskId,
    parentVideoId: incoming.parentVideoId,
    items: sortClipReviewItems([...byId.values()]),
  };
}

export type SavedClipReviewItem = {
  videoId: number;
  sourceStartSec: number;
  sourceEndSec: number;
  reason: string;
};

export type SavedClipReviewRequest = {
  taskId: string;
  parentVideoId: number;
  items: SavedClipReviewItem[];
};

export function clipReviewRequestStorageKey(sourceKey: string): string {
  return `bsr:clip-review-request:v1:${sourceKey}`;
}

export function serializeClipReviewRequest(request: ClipReviewRequest): SavedClipReviewRequest {
  return {
    taskId: request.taskId,
    parentVideoId: request.parentVideoId,
    items: request.items.map((item) => ({
      videoId: item.video.id,
      sourceStartSec: item.sourceStartSec,
      sourceEndSec: item.sourceEndSec,
      reason: item.reason,
    })),
  };
}

export function parseSavedClipReviewRequest(raw: string | null): SavedClipReviewRequest | null {
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as Partial<SavedClipReviewRequest>;
    if (!value || typeof value.taskId !== "string" || !Number.isFinite(value.parentVideoId) || !Array.isArray(value.items)) {
      return null;
    }
    const items = value.items.flatMap((item) => {
      if (!item || !Number.isFinite(item.videoId) || !Number.isFinite(item.sourceStartSec) || !Number.isFinite(item.sourceEndSec)) {
        return [];
      }
      return [{
        videoId: Math.trunc(item.videoId),
        sourceStartSec: item.sourceStartSec,
        sourceEndSec: item.sourceEndSec,
        reason: typeof item.reason === "string" ? item.reason : "",
      }];
    });
    return items.length
      ? { taskId: value.taskId, parentVideoId: Math.trunc(value.parentVideoId), items }
      : null;
  } catch {
    return null;
  }
}

export function rebuildClipReviewRequest(options: {
  taskId: string;
  parentVideoId: number;
  items: readonly SavedClipReviewItem[];
  videos: readonly VideoItem[];
  transcriptEntries: readonly WorkspaceTranscriptEntry[];
}): ClipReviewRequest | null {
  const videoById = new Map(options.videos.map((video) => [video.id, video]));
  const items = options.items.flatMap((saved) => {
    const video = videoById.get(saved.videoId);
    if (!video || saved.sourceEndSec <= saved.sourceStartSec) return [];
    return [{
      video,
      sourceStartSec: saved.sourceStartSec,
      sourceEndSec: saved.sourceEndSec,
      reason: saved.reason,
      transcriptEntries: clipLocalTranscript(
        options.transcriptEntries,
        saved.sourceStartSec,
        saved.sourceEndSec,
      ),
    }];
  });
  return items.length
    ? {
        taskId: options.taskId,
        parentVideoId: options.parentVideoId,
        items: sortClipReviewItems(items),
      }
    : null;
}
