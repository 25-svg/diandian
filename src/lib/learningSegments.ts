
export type LearningSpeechSource = {
  id: string | number;
  startSec: number;
  endSec: number;
  text: string;
  productName?: string;
  orderAnchorSec?: number;
};

export type LearningSegment = {
  id: string;
  sourceKey: string;
  sourceSpeechId: string;
  startSec: number;
  endSec: number;
  preBufferSec: number;
  postBufferSec: number;
  text: string;
  productName: string;
  orderAnchorSec: number | null;
};

export type LearningSegmentOptions = {
  sourceKey: string;
  preBufferSec?: number;
  postBufferSec?: number;
  maxDurationSec?: number;
};

const DEFAULT_PRE_BUFFER_SEC = 12;
const DEFAULT_POST_BUFFER_SEC = 18;
const DEFAULT_MAX_DURATION_SEC = 5 * 60;

export function createLearningSegment(
  speech: LearningSpeechSource,
  options: LearningSegmentOptions,
): LearningSegment | null {
  const text = speech.text.trim();
  if (!options.sourceKey.trim() || !text || !Number.isFinite(speech.startSec) || !Number.isFinite(speech.endSec) || speech.endSec <= speech.startSec) return null;
  const preBufferSec = Math.max(0, options.preBufferSec ?? DEFAULT_PRE_BUFFER_SEC);
  const postBufferSec = Math.max(0, options.postBufferSec ?? DEFAULT_POST_BUFFER_SEC);
  const maxDurationSec = Math.max(1, options.maxDurationSec ?? DEFAULT_MAX_DURATION_SEC);
  let startSec = Math.max(0, speech.startSec - preBufferSec);
  let endSec = speech.endSec + postBufferSec;
  if (endSec - startSec > maxDurationSec) {
    endSec = startSec + maxDurationSec;
    if (endSec < speech.endSec) {
      endSec = speech.endSec;
      startSec = Math.max(0, endSec - maxDurationSec);
    }
  }
  return {
    id: `${options.sourceKey}:${String(speech.id)}`,
    sourceKey: options.sourceKey,
    sourceSpeechId: String(speech.id),
    startSec,
    endSec,
    preBufferSec,
    postBufferSec,
    text,
    productName: speech.productName?.trim() || "",
    orderAnchorSec: Number.isFinite(speech.orderAnchorSec) ? speech.orderAnchorSec! : null,
  };
}

export function addLearningSegment(
  segments: readonly LearningSegment[],
  segment: LearningSegment,
): LearningSegment[] {
  return segments.some((item) => item.id === segment.id) ? [...segments] : [...segments, segment];
}

export function removeLearningSegment(
  segments: readonly LearningSegment[],
  segmentId: string,
): LearningSegment[] {
  return segments.filter((segment) => segment.id !== segmentId);
}

export function formatLearningSegmentTime(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const secs = whole % 60;
  return hours > 0
    ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

export interface LearningSegmentExport {
  output_path: string;
  start_time: number;
  end_time: number;
  /** True only when fast stream-copy remux was not possible. */
  re_encoded: boolean;
}

/** Export one selected source-video range to an MP4 learning clip. */
export function exportLearningSegment(
  videoId: number,
  startTime: number,
  endTime: number,
): Promise<LearningSegmentExport> {
  return import("./invoker.js").then(({ invoke }) =>
    invoke<LearningSegmentExport>("export_learning_segment", { videoId, startTime, endTime }),
  );
}
