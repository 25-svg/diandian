export type PlaybackSourceState = {
  requiresPreparation: boolean;
  ready: boolean;
  preparing: boolean;
};

export type PlaybackPresentation = "player" | "preparing" | "retry";

/** Original TS/HLS is mounted first; a playable MP4 is only a decode fallback. */
export function playbackPresentation(
  source: PlaybackSourceState | null,
  rawPlaybackFailed = false,
): PlaybackPresentation {
  if (!rawPlaybackFailed) return "player";
  if (source?.requiresPreparation && !source.ready) {
    return source.preparing ? "preparing" : "retry";
  }
  return "player";
}

export function shouldAutoPreparePlayback(source: PlaybackSourceState): boolean {
  return source.requiresPreparation && !source.ready && !source.preparing;
}
