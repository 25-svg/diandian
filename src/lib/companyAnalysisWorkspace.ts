export type WorkspaceTranscriptEntry = {
  id: number;
  start: number;
  end: number;
  text: string;
};

export type CompanyAnalysisTab = "align" | "deal_speech" | "clip_review" | "ai_review";

export type DealSpeechWindow = {
  start: number;
  end: number;
  anchorOffsetSec: number;
};

export type DealSpeechWindowMode = "refined" | "fallback" | "context";

export type ResolvedDealSpeechWindow = DealSpeechWindow & {
  mode: DealSpeechWindowMode;
};

export type RefinedDealSpeechRange = {
  start: number;
  end: number;
  title?: string;
  reason?: string;
};

/** Short browse window while AI refine is pending or failed. */
export const DEAL_SPEECH_FALLBACK_PRE_SEC = 3 * 60;
export const DEAL_SPEECH_FALLBACK_POST_SEC = 60;
/** Coarse expand-context window — keep in sync with DEAL_CLIP_CONTEXT_* in dealOrderAutoClip. */
export const DEAL_SPEECH_CONTEXT_PRE_SEC = 10 * 60;
export const DEAL_SPEECH_CONTEXT_POST_SEC = 2 * 60;

export function formatWorkspaceClock(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds || 0));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;
  return hours > 0
    ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** Default / fallback speech window around the pay anchor (not the learning product). */
export function dealSpeechWindow(offsetSec: number): DealSpeechWindow {
  const anchor = Math.max(0, Math.floor(offsetSec || 0));
  return {
    anchorOffsetSec: anchor,
    start: Math.max(0, anchor - DEAL_SPEECH_FALLBACK_PRE_SEC),
    end: anchor + DEAL_SPEECH_FALLBACK_POST_SEC,
  };
}

/** Coarse context window used for expand-context browse (matches AI input span). */
export function dealSpeechContextWindow(offsetSec: number): DealSpeechWindow {
  const anchor = Math.max(0, Math.floor(offsetSec || 0));
  return {
    anchorOffsetSec: anchor,
    start: Math.max(0, anchor - DEAL_SPEECH_CONTEXT_PRE_SEC),
    end: anchor + DEAL_SPEECH_CONTEXT_POST_SEC,
  };
}

/**
 * Prefer AI-refined chain bounds; otherwise short fallback, or coarse context when expanded.
 */
export function resolveDealSpeechWindow(options: {
  anchorOffsetSec: number;
  refined: RefinedDealSpeechRange | null | undefined;
  expandContext?: boolean;
}): ResolvedDealSpeechWindow {
  const anchor = Math.max(0, Math.floor(options.anchorOffsetSec || 0));
  const refined = options.refined;
  if (
    refined
    && Number.isFinite(refined.start)
    && Number.isFinite(refined.end)
    && refined.end > refined.start
    && !options.expandContext
  ) {
    return {
      anchorOffsetSec: anchor,
      start: Math.max(0, refined.start),
      end: refined.end,
      mode: "refined",
    };
  }
  if (options.expandContext) {
    return { ...dealSpeechContextWindow(anchor), mode: "context" };
  }
  return { ...dealSpeechWindow(anchor), mode: "fallback" };
}

export function filterTranscriptWindow(
  entries: WorkspaceTranscriptEntry[],
  startSec: number,
  endSec: number,
): WorkspaceTranscriptEntry[] {
  return entries.filter((entry) => entry.end >= startSec && entry.start <= endSec);
}

/** Build learning-segment source from the active speech window (prefer window bounds over first/last cue). */
export function learningSpeechFromWindow(options: {
  id: string | number;
  window: DealSpeechWindow;
  entries: readonly WorkspaceTranscriptEntry[];
  productName?: string;
  orderAnchorSec?: number;
}): {
  id: string | number;
  startSec: number;
  endSec: number;
  text: string;
  productName?: string;
  orderAnchorSec?: number;
} | null {
  const text = options.entries.map((entry) => entry.text.trim()).filter(Boolean).join("\n").trim();
  if (!text || options.window.end <= options.window.start) return null;
  return {
    id: options.id,
    startSec: options.window.start,
    endSec: options.window.end,
    text,
    productName: options.productName,
    orderAnchorSec: options.orderAnchorSec,
  };
}
