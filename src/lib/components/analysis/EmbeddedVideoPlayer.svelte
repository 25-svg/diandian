<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import {
    Loader2,
    Maximize2,
    Minimize2,
    Pause,
    Play,
    Volume2,
    VolumeX,
  } from "lucide-svelte";

  /** Media URL for native <video src>. Leave empty when a host attaches HLS/Shaka to `videoEl`. */
  export let src = "";
  /** External loading overlay (e.g. preparing playable copy / HLS playlist). */
  export let loading = false;
  /** External error message shown above controls. */
  export let error = "";
  /** When changed to a finite number, seek the element to that time (seconds). */
  export let seekTo: number | null = null;
  /** Optional poster frame. */
  export let poster = "";
  /** Disable interaction while a host job owns the element. */
  export let disabled = false;
  /**
   * When Shaka/HLS attaches without a `src` prop, set this false so the idle
   * placeholder does not cover the element. Defaults to hiding idle whenever
   * `src` is set or `loading` is true.
   */
  export let showPlaceholder: boolean | null = null;

  /** Bound out so Codex/Shaka can attach without replacing this shell. */
  export let videoEl: HTMLVideoElement | null = null;

  const dispatch = createEventDispatcher<{
    play: void;
    pause: void;
    ended: void;
    timeupdate: { currentTime: number; duration: number };
    seek: number;
    volumechange: { volume: number; muted: boolean };
    error: string;
    loadedmetadata: { duration: number };
    fullscreenchange: boolean;
    videobind: HTMLVideoElement;
  }>();

  let shellEl: HTMLDivElement | null = null;
  let paused = true;
  let currentTime = 0;
  let duration = 0;
  let volume = 1;
  let muted = false;
  let isFullscreen = false;
  let localError = "";
  let isBuffering = false;
  let lastSeekToken: number | null = null;
  let dragging = false;

  $: displayError = error || localError;
  $: showLoading = loading || (Boolean(src) && isBuffering && !displayError);
  $: showIdle = showPlaceholder ?? (!src && !loading && !displayError);

  $: if (src) localError = "";

  $: if (videoEl && seekTo != null && Number.isFinite(seekTo) && seekTo !== lastSeekToken) {
    const target = Math.max(0, seekTo);
    lastSeekToken = seekTo;
    try {
      videoEl.currentTime = target;
      currentTime = videoEl.currentTime;
      dispatch("seek", currentTime);
    } catch {
      /* ignore seek until metadata is ready */
    }
  }

  function formatClock(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds < 0) return "00:00";
    const total = Math.floor(seconds);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    if (h > 0) {
      return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
    }
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  function syncFromVideo(): void {
    if (!videoEl) return;
    paused = videoEl.paused;
    currentTime = videoEl.currentTime || 0;
    duration = Number.isFinite(videoEl.duration) ? videoEl.duration : duration;
    volume = videoEl.volume;
    muted = videoEl.muted;
  }

  async function togglePlay(): Promise<void> {
    if (!videoEl || disabled) return;
    try {
      if (videoEl.paused) {
        await videoEl.play();
      } else {
        videoEl.pause();
      }
    } catch (err: any) {
      localError = err?.message || String(err);
      dispatch("error", localError);
    }
  }

  function onProgressInput(event: Event): void {
    if (!videoEl || disabled) return;
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (!Number.isFinite(value)) return;
    dragging = true;
    currentTime = value;
  }

  function onProgressChange(event: Event): void {
    if (!videoEl || disabled) return;
    const value = Number((event.currentTarget as HTMLInputElement).value);
    dragging = false;
    if (!Number.isFinite(value)) return;
    videoEl.currentTime = value;
    currentTime = videoEl.currentTime;
    dispatch("seek", currentTime);
  }

  function onVolumeInput(event: Event): void {
    if (!videoEl || disabled) return;
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (!Number.isFinite(value)) return;
    videoEl.volume = Math.min(1, Math.max(0, value));
    videoEl.muted = videoEl.volume === 0;
    volume = videoEl.volume;
    muted = videoEl.muted;
    dispatch("volumechange", { volume, muted });
  }

  function toggleMute(): void {
    if (!videoEl || disabled) return;
    videoEl.muted = !videoEl.muted;
    muted = videoEl.muted;
    dispatch("volumechange", { volume: videoEl.volume, muted });
  }

  function fullscreenElement(): Element | null {
    const doc = document as Document & {
      webkitFullscreenElement?: Element | null;
      msFullscreenElement?: Element | null;
    };
    return document.fullscreenElement || doc.webkitFullscreenElement || doc.msFullscreenElement || null;
  }

  async function toggleFullscreen(): Promise<void> {
    if (!shellEl || disabled) return;
    try {
      if (fullscreenElement()) {
        const doc = document as Document & {
          webkitExitFullscreen?: () => Promise<void> | void;
          msExitFullscreen?: () => Promise<void> | void;
        };
        if (document.exitFullscreen) await document.exitFullscreen();
        else if (doc.webkitExitFullscreen) await doc.webkitExitFullscreen();
        else if (doc.msExitFullscreen) await doc.msExitFullscreen();
      } else {
        const el = shellEl as HTMLElement & {
          webkitRequestFullscreen?: () => Promise<void> | void;
          msRequestFullscreen?: () => Promise<void> | void;
        };
        if (el.requestFullscreen) await el.requestFullscreen();
        else if (el.webkitRequestFullscreen) await el.webkitRequestFullscreen();
        else if (el.msRequestFullscreen) await el.msRequestFullscreen();
      }
    } catch (err: any) {
      localError = err?.message || String(err);
      dispatch("error", localError);
    }
  }

  function onFullscreenChange(): void {
    isFullscreen = fullscreenElement() === shellEl;
    dispatch("fullscreenchange", isFullscreen);
  }

  function handleVideoError(): void {
    localError = "视频无法播放。请检查源文件或生成可播放版本。";
    isBuffering = false;
    dispatch("error", localError);
  }

  function bindVideo(node: HTMLVideoElement): { destroy: () => void } {
    videoEl = node;
    dispatch("videobind", node);
    const onPlay = () => {
      paused = false;
      localError = "";
      dispatch("play");
    };
    const onPause = () => {
      paused = true;
      dispatch("pause");
    };
    const onEnded = () => {
      paused = true;
      dispatch("ended");
    };
    const onTime = () => {
      if (!dragging) currentTime = node.currentTime || 0;
      duration = Number.isFinite(node.duration) ? node.duration : duration;
      dispatch("timeupdate", { currentTime: node.currentTime || 0, duration });
    };
    const onMeta = () => {
      syncFromVideo();
      dispatch("loadedmetadata", { duration: node.duration || 0 });
    };
    const onWaiting = () => {
      isBuffering = true;
    };
    const onPlaying = () => {
      isBuffering = false;
      localError = "";
    };
    const onCanPlay = () => {
      isBuffering = false;
    };
    node.addEventListener("play", onPlay);
    node.addEventListener("pause", onPause);
    node.addEventListener("ended", onEnded);
    node.addEventListener("timeupdate", onTime);
    node.addEventListener("loadedmetadata", onMeta);
    node.addEventListener("waiting", onWaiting);
    node.addEventListener("playing", onPlaying);
    node.addEventListener("canplay", onCanPlay);
    node.addEventListener("error", handleVideoError);
    syncFromVideo();
    return {
      destroy() {
        node.removeEventListener("play", onPlay);
        node.removeEventListener("pause", onPause);
        node.removeEventListener("ended", onEnded);
        node.removeEventListener("timeupdate", onTime);
        node.removeEventListener("loadedmetadata", onMeta);
        node.removeEventListener("waiting", onWaiting);
        node.removeEventListener("playing", onPlaying);
        node.removeEventListener("canplay", onCanPlay);
        node.removeEventListener("error", handleVideoError);
        if (videoEl === node) videoEl = null;
      },
    };
  }

  onMount(() => {
    document.addEventListener("fullscreenchange", onFullscreenChange);
    document.addEventListener("webkitfullscreenchange", onFullscreenChange as EventListener);
    return () => {
      document.removeEventListener("fullscreenchange", onFullscreenChange);
      document.removeEventListener("webkitfullscreenchange", onFullscreenChange as EventListener);
    };
  });

  onDestroy(() => {
    if (fullscreenElement() === shellEl) {
      void document.exitFullscreen?.().catch(() => undefined);
    }
  });
</script>

<div
  class="embedded-player"
  class:is-fullscreen={isFullscreen}
  class:has-error={Boolean(displayError)}
  bind:this={shellEl}
  aria-label="录播播放器"
>
  <div class="media-stage">
    <!-- svelte-ignore a11y-media-has-caption -->
    <video
      use:bindVideo
      src={src || undefined}
      {poster}
      playsinline
      preload="metadata"
      controls={false}
    ></video>

    {#if showLoading}
      <div class="state-layer" role="status">
        <Loader2 size={28} class="is-spinning" />
        <span>正在加载视频…</span>
      </div>
    {:else if displayError}
      <div class="state-layer error" role="alert">
        <strong>播放失败</strong>
        <span>{displayError}</span>
      </div>
    {:else if showIdle}
      <div class="state-layer idle">
        <Play size={28} />
        <span>等待视频源</span>
      </div>
    {/if}
  </div>

  <div class="controls" aria-label="播放控制">
    <button
      type="button"
      class="icon-btn"
      title={paused ? "播放" : "暂停"}
      aria-label={paused ? "播放" : "暂停"}
      disabled={disabled || Boolean(displayError)}
      on:click={() => void togglePlay()}
    >
      {#if paused}
        <Play size={16} fill="currentColor" />
      {:else}
        <Pause size={16} fill="currentColor" />
      {/if}
    </button>

    <input
      class="progress"
      type="range"
      min="0"
      max={Math.max(duration || 0, 0.1)}
      step="0.1"
      value={currentTime}
      aria-label="播放进度"
      disabled={disabled || !duration}
      on:input={onProgressInput}
      on:change={onProgressChange}
    />

    <span class="time" aria-live="off">
      {formatClock(currentTime)} <i>/</i> {duration ? formatClock(duration) : "--:--"}
    </span>

    <button
      type="button"
      class="icon-btn"
      title={muted || volume === 0 ? "取消静音" : "静音"}
      aria-label={muted || volume === 0 ? "取消静音" : "静音"}
      disabled={disabled}
      on:click={toggleMute}
    >
      {#if muted || volume === 0}
        <VolumeX size={16} />
      {:else}
        <Volume2 size={16} />
      {/if}
    </button>

    <input
      class="volume"
      type="range"
      min="0"
      max="1"
      step="0.05"
      value={muted ? 0 : volume}
      aria-label="音量"
      disabled={disabled}
      on:input={onVolumeInput}
    />

    <button
      type="button"
      class="icon-btn"
      title={isFullscreen ? "退出全屏" : "全屏"}
      aria-label={isFullscreen ? "退出全屏" : "全屏"}
      disabled={disabled}
      on:click={() => void toggleFullscreen()}
    >
      {#if isFullscreen}
        <Minimize2 size={16} />
      {:else}
        <Maximize2 size={16} />
      {/if}
    </button>
  </div>
</div>

<style>
  .embedded-player {
    --ep-bg: #0b1220;
    --ep-panel: rgba(15, 23, 42, 0.92);
    --ep-text: #e2e8f0;
    --ep-muted: #94a3b8;
    --ep-accent: #2e90fa;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    min-height: 0;
    min-width: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    border-radius: 10px;
    overflow: hidden;
    background: var(--ep-bg);
    color: var(--ep-text);
  }
  .embedded-player.is-fullscreen {
    border-radius: 0;
    width: 100%;
    height: 100%;
  }
  .media-stage {
    position: relative;
    min-height: 0;
    min-width: 0;
    display: grid;
    place-items: center;
    background: #020617;
  }
  video {
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    background: #000;
  }
  .state-layer {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    padding: 16px;
    text-align: center;
    background: rgba(2, 6, 23, 0.55);
    color: var(--ep-text);
    font-size: 12px;
    pointer-events: none;
  }
  .state-layer.error {
    background: rgba(69, 10, 10, 0.72);
    color: #fecaca;
  }
  .state-layer strong { font-size: 14px; }
  .state-layer.idle { color: var(--ep-muted); }
  .controls {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto minmax(56px, 88px) auto;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--ep-panel);
    border-top: 1px solid rgba(148, 163, 184, 0.18);
  }
  .icon-btn {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ep-text);
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) { background: rgba(148, 163, 184, 0.16); }
  .icon-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .progress, .volume {
    width: 100%;
    accent-color: var(--ep-accent);
    cursor: pointer;
  }
  .progress:disabled, .volume:disabled { cursor: not-allowed; opacity: 0.5; }
  .time {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--ep-muted);
    white-space: nowrap;
  }
  .time i { font-style: normal; opacity: 0.7; margin: 0 2px; }
  :global(.embedded-player .is-spinning) { animation: ep-spin 0.9s linear infinite; }
  @keyframes ep-spin { to { transform: rotate(360deg); } }

  @media (max-width: 720px) {
    .controls {
      grid-template-columns: auto minmax(0, 1fr) auto auto;
      grid-template-areas:
        "play progress progress fs"
        "mute volume time time";
      row-gap: 6px;
    }
    .icon-btn:first-child { grid-area: play; }
    .progress { grid-area: progress; }
    .time { grid-area: time; justify-self: end; }
    .icon-btn:nth-child(4) { grid-area: mute; }
    .volume { grid-area: volume; }
    .icon-btn:last-child { grid-area: fs; }
  }
</style>
