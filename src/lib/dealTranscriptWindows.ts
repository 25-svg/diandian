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
