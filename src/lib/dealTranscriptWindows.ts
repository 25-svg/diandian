import type { PaymentEvent } from "./orderDealTimeline.js";

export type DealTranscriptWindow = {
  startSec: number;
  endSec: number;
};

export const DEAL_TRANSCRIPT_PRE_SEC = 180;
export const DEAL_TRANSCRIPT_POST_SEC = 60;

/**
 * Build sparse ASR windows around paid-order anchors. Nearby orders share one
 * window so the same sales speech is never transcribed repeatedly.
 */
export function buildDealTranscriptWindows(
  events: PaymentEvent[],
  durationSec: number,
  preSec = DEAL_TRANSCRIPT_PRE_SEC,
  postSec = DEAL_TRANSCRIPT_POST_SEC,
): DealTranscriptWindow[] {
  const duration = Number.isFinite(durationSec) && durationSec > 0
    ? durationSec
    : Number.POSITIVE_INFINITY;
  const ranges = events
    .map((event) => Number(event.offsetSec))
    .filter((offset) => Number.isFinite(offset) && offset >= 0 && offset <= duration)
    .sort((left, right) => left - right)
    .map((offset) => ({
      startSec: Math.max(0, offset - preSec),
      endSec: Math.min(duration, offset + postSec),
    }))
    .filter((range) => range.endSec > range.startSec);

  const merged: DealTranscriptWindow[] = [];
  for (const range of ranges) {
    const previous = merged[merged.length - 1];
    if (previous && range.startSec <= previous.endSec) {
      previous.endSec = Math.max(previous.endSec, range.endSec);
    } else {
      merged.push({ ...range });
    }
  }
  return merged;
}

export function totalDealTranscriptDuration(windows: DealTranscriptWindow[]): number {
  return windows.reduce((total, range) => total + Math.max(0, range.endSec - range.startSec), 0);
}

export const DEFAULT_TRANSCRIPT_GAP_MIN_SEC = 20;

/**
 * Invert deal ASR windows into uncovered timeline segments that still need
 * transcription. Tiny leftovers are skipped so we do not burn ASR on noise gaps.
 */
export function buildTranscriptGapWindows(
  covered: readonly DealTranscriptWindow[],
  durationSec: number,
  minGapSec = DEFAULT_TRANSCRIPT_GAP_MIN_SEC,
): DealTranscriptWindow[] {
  const duration = Number.isFinite(durationSec) && durationSec > 0 ? durationSec : 0;
  if (duration <= 0) return [];
  const merged = [...covered]
    .filter((window) =>
      Number.isFinite(window.startSec)
      && Number.isFinite(window.endSec)
      && window.endSec > window.startSec)
    .map((window) => ({
      startSec: Math.max(0, Math.min(duration, window.startSec)),
      endSec: Math.max(0, Math.min(duration, window.endSec)),
    }))
    .filter((window) => window.endSec > window.startSec)
    .sort((left, right) => left.startSec - right.startSec);

  const compacted: DealTranscriptWindow[] = [];
  for (const window of merged) {
    const previous = compacted[compacted.length - 1];
    if (previous && window.startSec <= previous.endSec) {
      previous.endSec = Math.max(previous.endSec, window.endSec);
    } else {
      compacted.push({ ...window });
    }
  }

  const gaps: DealTranscriptWindow[] = [];
  let cursor = 0;
  for (const window of compacted) {
    if (window.startSec - cursor >= minGapSec) {
      gaps.push({ startSec: cursor, endSec: window.startSec });
    }
    cursor = Math.max(cursor, window.endSec);
  }
  if (duration - cursor >= minGapSec) {
    gaps.push({ startSec: cursor, endSec: duration });
  }
  return gaps;
}

/** Merge cue lists onto one timeline (sorted, re-id). Later list wins on exact overlaps only by both being kept; callers should pass non-overlapping ranges. */
export function mergeTimedTranscriptCues<T extends TimedTranscriptCue>(
  left: readonly T[],
  right: readonly T[],
): TimedTranscriptCue[] {
  return [...left, ...right]
    .filter((cue) =>
      Number.isFinite(cue.start)
      && Number.isFinite(cue.end)
      && cue.end > cue.start
      && String(cue.text || "").trim())
    .sort((a, b) => a.start - b.start || a.end - b.end)
    .map((cue, index) => ({
      id: index + 1,
      start: cue.start,
      end: cue.end,
      text: String(cue.text).trim(),
    }));
}

export type TimedTranscriptCue = {
  id: number;
  start: number;
  end: number;
  text: string;
};

/** True when a cue overlaps any deal ASR window (half-open on the end). */
export function cueOverlapsDealWindow(
  cue: Pick<TimedTranscriptCue, "start" | "end">,
  windows: readonly DealTranscriptWindow[],
): boolean {
  if (!windows.length) return false;
  const start = Number(cue.start);
  const end = Number(cue.end);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return false;
  return windows.some((window) => start < window.endSec && end > window.startSec);
}

/**
 * Full-session AI review should not re-coach deal windows — those belong in
 * the 「成交话术」 tab. Keep cues whose time range sits entirely outside deals.
 */
export function excludeDealWindowCues<T extends TimedTranscriptCue>(
  entries: readonly T[],
  windows: readonly DealTranscriptWindow[],
): T[] {
  if (!windows.length) return [...entries];
  return entries.filter((entry) => !cueOverlapsDealWindow(entry, windows));
}
