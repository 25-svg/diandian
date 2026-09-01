<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Activity, AlertTriangle, Loader2, RefreshCw, TrendingDown } from "lucide-svelte";
  import { invoke } from "../../invoker";
  import type { LiveDataBoardSession } from "../../liveDashboard";
  import LiveMetricCurveWorkbench from "./LiveMetricCurveWorkbench.svelte";
  import {
    selectCaptureForSession,
    type CompassCaptureAnalysis,
    type CompassCaptureSummary,
  } from "../../compassAnalysis";

  export let session: LiveDataBoardSession | null = null;

  const dispatch = createEventDispatcher<{ seek: number }>();

  let analysis: CompassCaptureAnalysis | null = null;
  let loading = false;
  let error = "";
  let loadedSessionKey = "";
  let activeCaptureId = "";
  let captureMessage = "";
  let captureRunning = false;

  $: sessionKey = session ? `${session.id}:${session.shopName}:${session.startedAt}` : "";
  $: if (sessionKey && sessionKey !== loadedSessionKey && !captureRunning) {
    loadedSessionKey = sessionKey;
    void loadDiagnosis();
  } else if (!sessionKey && loadedSessionKey) {
    loadedSessionKey = "";
    analysis = null;
    error = "";
  }
  onMount(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void listen<CompassCaptureSummary>("compass-capture-progress", (event) => {
      if (disposed || !activeCaptureId || event.payload.captureId !== activeCaptureId) return;
      captureMessage = event.payload.message || "正在采集本场直播数据";
      if (["completed", "partial", "failed", "cancelled"].includes(event.payload.status)) {
        captureRunning = false;
        if (event.payload.status === "failed") {
          error = captureMessage;
          return;
        }
        window.setTimeout(() => void loadDiagnosis(true), 300);
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  async function loadDiagnosis(force = false): Promise<void> {
    const currentSession = session;
    if (!currentSession || loading) return;
    loading = true;
    error = "";
    try {
      const captures = await invoke<CompassCaptureSummary[]>("list_compass_captures");
      const capture = selectCaptureForSession(captures, currentSession);
      if (!capture) {
        analysis = null;
        if (force) error = "本场还没有分钟曲线数据，请点击“获取本场数据”。";
        return;
      }
      if (!force) {
        const saved = await invoke<CompassCaptureAnalysis | null>("get_compass_capture_analysis", {
          captureId: capture.captureId,
          sessionId: currentSession.id,
        });
        if (saved?.metrics?.length) {
          if (session?.id !== currentSession.id) return;
          analysis = saved;
          return;
        }
      }
      const result = await invoke<CompassCaptureAnalysis>("analyze_compass_capture", {
        captureId: capture.captureId,
        sessionId: currentSession.id,
      });
      if (session?.id !== currentSession.id) return;
      analysis = result;
    } catch (reason) {
      analysis = null;
      error = `本场流量诊断加载失败：${String(reason)}`;
    } finally {
      loading = false;
    }
  }

  async function captureCurrentSession(): Promise<void> {
    if (!session || captureRunning) return;
    captureRunning = true;
    error = "";
    captureMessage = "正在打开罗盘；首次使用请扫码登录";
    try {
      activeCaptureId = await invoke<string>("start_compass_full_capture", {
        targetDate: session.startedAt.slice(0, 10),
        targetShopName: session.shopName,
        targetStartedAt: session.startedAt,
      });
    } catch (reason) {
      captureRunning = false;
      error = `启动本场数据采集失败：${String(reason)}`;
    }
  }

</script>

<section class="metric-diagnosis" aria-label="本场流量诊断">
  <header class="diagnosis-head">
    <div>
      <Activity size={17} />
      <div><strong>本场流量诊断</strong><span>曲线、主播原话和录播使用同一时间轴</span></div>
    </div>
    <button type="button" class="icon-btn" aria-label="刷新本场流量诊断" title="刷新" disabled={loading || captureRunning || !session} on:click={() => loadDiagnosis(true)}>
      <span class:is-spinning={loading}><RefreshCw size={15} /></span>
    </button>
  </header>

  {#if !session}
    <div class="empty-state"><TrendingDown size={22} /><strong>先绑定本场直播数据</strong><span>在“对齐”里下载或选择本场 Excel，系统会自动匹配录播时间。</span></div>
  {:else if loading}
    <div class="empty-state"><Loader2 size={22} class="is-spinning" /><strong>正在对齐本场曲线</strong><span>{session.shopName} · {session.startedAt}</span></div>
  {:else if !analysis}
    <div class="empty-state">
      <TrendingDown size={22} />
      <strong>本场还没有分钟曲线</strong>
      <span>无需离开录播页。点击后系统只采集当前店铺、当前开播时间的罗盘专业大屏数据。</span>
      <button type="button" class="primary-btn" disabled={captureRunning} on:click={captureCurrentSession}>
        {#if captureRunning}<Loader2 size={15} class="is-spinning" />{:else}<Activity size={15} />{/if}
        {captureRunning ? "正在采集…" : "获取本场数据"}
      </button>
      {#if captureMessage}<small aria-live="polite">{captureMessage}</small>{/if}
    </div>
  {:else}
    <LiveMetricCurveWorkbench {analysis} sessionStartedAt={session.startedAt} on:seek={(event) => dispatch("seek", event.detail)} />
  {/if}

  {#if error}<div class="error" role="alert"><AlertTriangle size={14} /><span>{error}</span></div>{/if}
</section>

<style>
  .metric-diagnosis { min-width: 0; min-height: 0; display: flex; flex-direction: column; gap: 10px; padding: 10px; overflow: auto; color: #344054; background: #fff; }
  .diagnosis-head, .diagnosis-head > div { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .diagnosis-head > div { justify-content: flex-start; color: #175cd3; }
  .diagnosis-head > div > div { display: grid; gap: 2px; }
  .diagnosis-head strong { color: #101828; font-size: 13px; }
  .diagnosis-head span { color: #667085; font-size: 11px; }
  .icon-btn { width: 44px; height: 44px; display: grid; place-items: center; border: 1px solid #d0d5dd; border-radius: 9px; background: #fff; color: #475467; cursor: pointer; }
  .icon-btn:disabled { opacity: .45; cursor: not-allowed; }
  .icon-btn:focus-visible, .primary-btn:focus-visible { outline: 3px solid rgba(23, 105, 210, .35); outline-offset: 2px; }
  .empty-state { min-height: 190px; display: grid; place-content: center; justify-items: center; gap: 8px; padding: 20px; color: #667085; text-align: center; }
  .empty-state strong { color: #101828; font-size: 14px; }
  .empty-state span { max-width: 520px; font-size: 12px; line-height: 1.55; }
  .empty-state small { color: #667085; font-size: 11px; }
  .primary-btn { min-height: 44px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; padding: 0 13px; border: 0; border-radius: 8px; background: #175cd3; color: #fff; font-size: 12px; font-weight: 600; cursor: pointer; }
  .primary-btn:disabled { opacity: .55; cursor: wait; }
  .error { display: flex; align-items: center; gap: 6px; padding: 8px 9px; border: 1px solid #fecdca; border-radius: 8px; background: #fff6f5; color: #b42318; font-size: 10px; }
  :global(.is-spinning) { animation: diagnosis-spin .9s linear infinite; }
  @keyframes diagnosis-spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { :global(.is-spinning) { animation: none; } }
</style>
