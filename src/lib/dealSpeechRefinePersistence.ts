import type { DealClipRange } from "./dealOrderAutoClip.js";

export function dealSpeechRefineStorageKey(sourceKey: string): string {
  return `bsr:deal-speech-refine:v1:${sourceKey}`;
}

function isDealClipRange(value: unknown): value is DealClipRange {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const row = value as Partial<DealClipRange>;
  return Number.isFinite(row.start)
    && Number.isFinite(row.end)
    && Number(row.end) > Number(row.start)
    && typeof row.title === "string"
    && typeof row.reason === "string"
    && Number.isFinite(row.payAnchorSec)
    && Number.isFinite(row.peakMinuteIndex);
}

export function parseSavedDealSpeechRefine(raw: string | null): Record<string, DealClipRange> {
  if (!raw) return {};
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const next: Record<string, DealClipRange> = {};
    for (const [key, range] of Object.entries(value)) {
      if (key && isDealClipRange(range)) next[key] = range;
    }
    return next;
  } catch {
    return {};
  }
}
