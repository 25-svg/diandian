<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Package, Scissors, Loader2, RefreshCw } from "lucide-svelte";
  import type { PaymentEvent } from "../../orderDealTimeline";
  import { buildDealWaves, formatDealMoneyYuan } from "../../orderDealTimeline";
  import {
    summarizeWaveChattersMap,
    type DanmuSpeakerEntry,
  } from "../../dealWaveChatters";
  import {
    filterTranscriptWindow,
    formatWorkspaceClock,
    resolveDealSpeechWindow,
    type RefinedDealSpeechRange,
    type WorkspaceTranscriptEntry,
  } from "../../companyAnalysisWorkspace";

  export let events: PaymentEvent[] = [];
  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  export let selectedOffsetSec: number | null = null;
  export let transcriptReady = false;
  export let canAutoClip = false;
  export let autoClipDisabledReason = "需要成交订单、逐字稿和可切片视频";
  export let autoClipping = false;
  export let autoClipProgress = "";
  export let autoClipError = "";
  export let refinedSpeechRange: RefinedDealSpeechRange | null = null;
  export let speechRefining = false;
  export let speechRefineProgress = "";
  export let speechRefineError = "";
  /** Absolute-ms danmu rows for weak pre-pay speaker对照. */
  export let danmuEntries: DanmuSpeakerEntry[] = [];
  export let liveStartedAt: string | null = null;

  let expandContext = false;
  let lastRefinedKey = "";

  const dispatch = createEventDispatcher<{
    seek: number;
    select: number;
    autoClip: void;
    retrySpeechRefine: void;
  }>();

  $: sortedEvents = [...events].sort((left, right) => left.offsetSec - right.offsetSec);
  $: dealWaves = [...buildDealWaves(sortedEvents)].sort(
    (left, right) => left.startOffsetSec - right.startOffsetSec,
  );
  $: waveChatters = summarizeWaveChattersMap(dealWaves, danmuEntries, liveStartedAt);
  $: activeOffset = selectedOffsetSec ?? sortedEvents[0]?.offsetSec ?? null;
  $: activeWave = activeOffset == null
    ? null
    : dealWaves.find((wave) => wave.events.some((event) => event.offsetSec === activeOffset)) ?? null;
  $: activeChatters = activeWave ? waveChatters.get(activeWave.id) ?? null : null;
  $: activeEvent = activeOffset == null
    ? null
    : sortedEvents.find((event) => event.offsetSec === activeOffset) ?? null;
  $: refinedKey = refinedSpeechRange
    ? `${refinedSpeechRange.start}:${refinedSpeechRange.end}`
    : "";
  $: if (refinedKey !== lastRefinedKey) {
    lastRefinedKey = refinedKey;
    expandContext = false;
  }
  $: activeWindow = activeOffset == null
    ? null
    : resolveDealSpeechWindow({
      anchorOffsetSec: activeOffset,
      refined: refinedSpeechRange,
      expandContext,
    });
  $: windowEntries = activeWindow
    ? filterTranscriptWindow(transcriptEntries, activeWindow.start, activeWindow.end)
    : [];
  $: windowModeLabel = activeWindow?.mode === "refined"
    ? "AI 精炼链路"
    : activeWindow?.mode === "context"
      ? "展开上下文"
      : speechRefining
        ? "短窗兜底（定位中）"
        : "短窗兜底";

  function minuteLabel(offsetSec: number): string {
    return formatWorkspaceClock(offsetSec);
  }

  function handleSelect(offsetSec: number): void {
    expandContext = false;
    dispatch("select", offsetSec);
  }
</script>

<section class="deal-speech-panel" aria-label="成交话术">
  <header class="panel-head">
    <div>
      <strong>成交话术</strong>
    </div>
    <div class="head-actions">
      <button
        type="button"
        class="auto-clip-btn"
        disabled={!canAutoClip || autoClipping}
        title={!canAutoClip ? autoClipDisabledReason : "批量导出当前选中商品簇的完整成交链路 MP4"}
        on:click={() => dispatch("autoClip")}
      >
        {#if autoClipping}
          <Loader2 size={14} class="is-spinning" />
        {:else}
          <Scissors size={14} />
        {/if}
        AI 分析并切片
      </button>
    </div>
  </header>
  {#if autoClipProgress || autoClipError || speechRefineError || speechRefining || (!canAutoClip && autoClipDisabledReason)}
    <div
      class="auto-clip-status"
      class:error={Boolean(autoClipError || speechRefineError)}
      role="status"
    >
      {#if autoClipError}
        <span>{autoClipError}</span>
      {:else if autoClipProgress}
        <span>{autoClipProgress}</span>
      {:else if speechRefineError}
        <span>{speechRefineError}</span>
        <button type="button" class="retry-link" on:click={() => dispatch("retrySpeechRefine")}>
          <RefreshCw size={12} />
          重试定位
        </button>
      {:else if speechRefining}
        <span class="inline-busy">
          <Loader2 size={14} class="is-spinning" />
          {speechRefineProgress || "AI 正在定位成交链路…"}
        </span>
      {:else}
        <span>{autoClipDisabledReason}</span>
      {/if}
    </div>
  {/if}

  {#if !events.length}
    <div class="empty-box">
      <p>尚未导入成交订单。请在上方「一键拉取成交」或导入 payment-events JSON。</p>
      <p class="muted">无订单时不会标记为已确认成交。</p>
    </div>
  {:else}
    <div class="panel-grid">
      <aside class="order-column" aria-label="订单列表">
        <h3>成交波次</h3>
        <div class="order-list">
          {#each dealWaves as wave (wave.id)}
            <button
              type="button"
              class="order-card"
              class:selected={wave.events.some((event) => event.offsetSec === activeOffset)}
              on:click={() => handleSelect(wave.anchorOffsetSec)}
            >
              <time>
                {minuteLabel(wave.startOffsetSec)}
                {#if wave.endOffsetSec > wave.startOffsetSec}—{minuteLabel(wave.endOffsetSec)}{/if}
              </time>
              <div class="product-cell">
                <span class="product" title={wave.productName}>
                  <Package size={12} />
                  {wave.productName}
                </span>
                {#if wave.buyerLabel}
                  <span class="buyer" title={wave.buyerLabel}>收货：{wave.buyerLabel}</span>
                {/if}
                {#if waveChatters.get(wave.id)?.label}
                  <span
                    class="chatters"
                    title={waveChatters.get(wave.id)?.hint || "付款前互动人（弱对照）"}
                  >
                    互动：{waveChatters.get(wave.id)?.label}
                  </span>
                {/if}
              </div>
              <strong>{wave.eventCount} 次 · {formatDealMoneyYuan(wave.totalPayAmountFen)}</strong>
            </button>
          {/each}
        </div>
      </aside>

      <div class="speech-column">
        {#if activeWindow && activeEvent}
          <div class="speech-meta">
            <span>讲解窗：{minuteLabel(activeWindow.start)} — {minuteLabel(activeWindow.end)}</span>
            <span>锚点下单：{minuteLabel(activeEvent.offsetSec)}</span>
            {#if activeChatters?.label}
              <span title={activeChatters.hint}>付款前互动：{activeChatters.label}</span>
            {:else if activeChatters?.hint}
              <span class="muted">{activeChatters.hint}</span>
            {/if}
            <span class="tier" class:pending={activeWindow.mode !== "refined"} class:ready={activeWindow.mode === "refined"}>
              {windowModeLabel}
            </span>
            {#if refinedSpeechRange && activeWindow.mode === "refined"}
              <button type="button" class="meta-link" on:click={() => { expandContext = true; }}>
                展开上下文
              </button>
            {:else if expandContext}
              <button
                type="button"
                class="meta-link"
                on:click={() => { expandContext = false; }}
                disabled={!refinedSpeechRange}
              >
                回到精炼链路
              </button>
            {/if}
            {#if speechRefineError}
              <button type="button" class="meta-link" on:click={() => dispatch("retrySpeechRefine")}>
                重试 AI 定位
              </button>
            {/if}
          </div>
          <div class="speech-transcript" aria-label="讲解窗逐字稿">
            {#if windowEntries.length}
              {#each windowEntries as entry (entry.id)}
                <button
                  type="button"
                  class="line"
                  on:click={() => dispatch("seek", entry.start)}
                  title="跳到该句"
                >
                  <time>{formatWorkspaceClock(entry.start)}</time>
                  <p>{entry.text}</p>
                </button>
              {/each}
            {:else if transcriptReady}
              <p class="muted">该时间窗内暂无逐字稿，请检查 ASR 或展开上下文。</p>
            {:else}
              <p class="muted">逐字稿生成中…</p>
            {/if}
          </div>
          <footer class="speech-actions">切片完成后自动打开：左侧视频 · 中间文稿 · 右侧 AI 分析</footer>
        {:else}
          <div class="empty-box inner">
            <p>请选择左侧订单或成交峰，查看对应时段话术。</p>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .deal-speech-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }
  .panel-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; flex: 0 0 auto; }
  .panel-head strong { font-size: 15px; color: #111827; }
  .panel-head p { margin: 4px 0 0; font-size: 12px; line-height: 1.5; color: #667085; max-width: 520px; }
  .head-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 8px; }
  .auto-clip-btn {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 7px 12px; border: 1px solid #2e90fa; border-radius: 999px;
    background: #eff8ff; color: #175cd3; font-size: 12px; font-weight: 600; cursor: pointer;
  }
  .auto-clip-btn:disabled { opacity: 0.55; cursor: not-allowed; }
  .auto-clip-status {
    flex: 0 0 auto;
    display: flex; flex-wrap: wrap; align-items: center; gap: 8px;
    padding: 8px 10px; border-radius: 8px; background: #f0f9ff; color: #175cd3;
    font-size: 12px; line-height: 1.45; border: 1px solid #b2ddff;
  }
  .auto-clip-status.error { background: #fef3f2; color: #b42318; border-color: #fecdca; }
  .inline-busy { display: inline-flex; align-items: center; gap: 6px; }
  .retry-link, .meta-link {
    display: inline-flex; align-items: center; gap: 4px;
    border: 0; background: transparent; color: inherit; font: inherit;
    text-decoration: underline; cursor: pointer; padding: 0;
  }
  .meta-link:disabled { opacity: 0.45; cursor: not-allowed; text-decoration: none; }
  :global(.is-spinning) { animation: deal-spin 0.9s linear infinite; }
  @keyframes deal-spin { to { transform: rotate(360deg); } }
  .panel-grid {
    min-height: 0;
    min-width: 0;
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: minmax(180px, 280px) minmax(0, 1fr);
    gap: 12px;
    overflow: hidden;
  }
  .order-column, .speech-column { min-height: 0; min-width: 0; border: 1px solid #e4e7ec; border-radius: 12px; background: #fff; overflow: hidden; display: flex; flex-direction: column; }
  .order-column h3 { margin: 0; padding: 10px 12px 6px; font-size: 12px; color: #475467; }
  .order-list { padding: 0 8px 8px; display: grid; gap: 6px; flex: 1 1 auto; min-height: 0; overflow: auto; }
  .order-card { display: grid; grid-template-columns: 94px minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 8px; border: 1px solid #eef2f6; border-radius: 8px; background: #fafafa; text-align: left; cursor: pointer; font-size: 12px; color: #344054; }
  .order-card.selected { border-color: #84caFF; background: #eff8ff; box-shadow: inset 0 0 0 1px #2e90fa; }
  .order-card time { color: #175cd3; font-variant-numeric: tabular-nums; }
  .product-cell { min-width: 0; display: grid; gap: 2px; }
  .product { display: inline-flex; align-items: center; gap: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .buyer, .chatters { color: #667085; font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chatters { color: #475467; }
  .muted-inline { color: #98a2b3; }
  .speech-column { padding: 0; }
  .speech-meta { flex: 0 0 auto; display: flex; flex-wrap: wrap; gap: 8px; align-items: center; padding: 10px 12px; border-bottom: 1px solid #eef2f6; font-size: 11px; color: #475467; background: #f9fafb; }
  .tier { padding: 2px 8px; border-radius: 999px; }
  .tier.pending { background: #fffaeb; color: #b54708; }
  .tier.ready { background: #ecfdf3; color: #027a48; }
  /* Keep the explanation window as the main pane (like the reference "分钟段落"). */
  .speech-transcript {
    flex: 1 1 auto;
    min-height: 160px;
    overflow: auto;
    padding: 10px 12px;
    background: #fff;
  }
  .line {
    width: 100%;
    display: grid;
    grid-template-columns: 54px minmax(0, 1fr);
    gap: 8px;
    padding: 6px 0;
    border: 0;
    border-bottom: 1px solid #f2f4f7;
    background: transparent;
    text-align: left;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .line:hover { background: #f8fafc; }
  .line time { color: #2e90fa; font-size: 11px; font-variant-numeric: tabular-nums; }
  .line p { margin: 0; font-size: 13px; line-height: 1.55; color: #1f2937; }
  .speech-actions { flex: 0 0 auto; padding: 9px 12px; border-top: 1px solid #eef2f6; background: #fcfcfd; color: #667085; font-size: 11px; }
  .empty-box { padding: 24px; border: 1px dashed #d0d5dd; border-radius: 12px; background: #fcfcfd; color: #475467; font-size: 13px; line-height: 1.6; }
  .empty-box.inner { margin: 12px; }
  .muted { color: #98a2b3; font-size: 12px; }

  @media (max-width: 720px) {
    .panel-grid {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(140px, 36%) minmax(0, 1fr);
    }
    .order-list { max-height: none; }
    .order-column { overflow: auto; }
  }
</style>
