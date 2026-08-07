<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { ChevronDown, ChevronRight } from "lucide-svelte";
  import DealSpeechPanel from "./DealSpeechPanel.svelte";
  import AiScriptReviewPanel from "./AiScriptReviewPanel.svelte";
  import LiveDataBoardPanel from "./LiveDataBoardPanel.svelte";
  import LiveHighFrequencyWordsPanel from "./LiveHighFrequencyWordsPanel.svelte";
  import LiveTopProductsPanel from "./LiveTopProductsPanel.svelte";
  import type { PaymentEvent } from "../../orderDealTimeline";
  import type {
    CompanyAnalysisTab,
    RefinedDealSpeechRange,
    WorkspaceTranscriptEntry,
  } from "../../companyAnalysisWorkspace";
  import type {
    LiveDataBoardCandidate,
    LiveDataBoardOrderSummary,
    LiveDataBoardSession,
  } from "../../liveDashboard";
  import type { ScriptIssueAnnotation } from "../../scriptQuality";

  export let activeTab: CompanyAnalysisTab = "deal_speech";
  export let events: PaymentEvent[] = [];
  export let videoId: number | null = null;
  export let archiveSource = false;
  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  export let selectedOffsetSec: number | null = null;
  export let transcriptReady = false;
  export let scriptQualityAnalyzing = false;
  export let scriptQualityError = "";
  export let scriptQualitySummary = "";
  export let scriptQualityAnnotations: ScriptIssueAnnotation[] = [];
  export let selectedScriptCueId: number | null = null;
  export let canAutoClip = false;
  export let autoClipping = false;
  export let autoClipProgress = "";
  export let autoClipError = "";
  export let refinedSpeechRange: RefinedDealSpeechRange | null = null;
  export let speechRefining = false;
  export let speechRefineProgress = "";
  export let speechRefineError = "";
  export let dashboardLoading = false;
  export let dashboardError = "";
  export let dashboardSession: LiveDataBoardSession | null = null;
  export let dashboardMatchMethod: string | null = null;
  export let dashboardCandidates: LiveDataBoardCandidate[] = [];
  export let selectedDashboardSessionId = "";
  export let orderSummary: LiveDataBoardOrderSummary | null = null;
  export let showDashboardBind = true;
  /** Bottom KPI board is for full live sessions only — never for clipped videos. */
  export let showDataBoard = true;

  /** Default collapsed so analysis UI stays focused; expand when needed. */
  let dataBoardExpanded = false;
  let autoOpenedForBind = false;
  /** Only one bottom view at a time. */
  let bottomBoardTab: "data" | "words" = "data";

  $: needsManualBind = showDataBoard && showDashboardBind && !dashboardSession && dashboardCandidates.length > 0 && !dashboardLoading;
  $: if (needsManualBind && !autoOpenedForBind) {
    dataBoardExpanded = true;
    bottomBoardTab = "data";
    autoOpenedForBind = true;
  }

  const dispatch = createEventDispatcher<{
    seek: number;
    select: number;
    analyzeScriptQuality: void;
    selectScriptCue: number;
    autoClip: void;
    retrySpeechRefine: void;
    openDashboard: void;
    bindDashboardSession: void;
    rebindDashboardSession: void;
  }>();
</script>

<div class="company-workspace" class:with-data-board={showDataBoard} class:data-board-open={showDataBoard && dataBoardExpanded}>
  <aside class="video-pane" aria-label="录播视频">
    <div class="video-pane-title">录播视频 · 整场</div>
    <div class="video-slot">
      <slot name="player" />
    </div>
  </aside>

  <section class="workspace-pane" aria-label="公司录播分析">
    <header class="workspace-head">
      <div class="tab-row" role="tablist" aria-label="分析面板">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "deal_speech"}
          class:active={activeTab === "deal_speech"}
          on:click={() => (activeTab = "deal_speech")}
        >
          成交话术
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "ai_review"}
          class:active={activeTab === "ai_review"}
          on:click={() => (activeTab = "ai_review")}
        >
          AI 分析
        </button>
      </div>
      {#if activeTab === "deal_speech"}
        <p>按下单时间锚定商品，AI 精炼完整成交链路并生成连续切片。</p>
      {:else}
        <p>复盘整场话术：标出可改进的句子，AI 给出「下次可以怎么说」的建议。</p>
      {/if}
    </header>

    <div class="workspace-body">
      {#if activeTab === "deal_speech"}
        <DealSpeechPanel
          {events}
          {transcriptEntries}
          {selectedOffsetSec}
          {transcriptReady}
          {canAutoClip}
          {autoClipping}
          {autoClipProgress}
          {autoClipError}
          {refinedSpeechRange}
          {speechRefining}
          {speechRefineProgress}
          {speechRefineError}
          on:seek={(event) => dispatch("seek", event.detail)}
          on:select={(event) => dispatch("select", event.detail)}
          on:autoClip={() => dispatch("autoClip")}
          on:retrySpeechRefine={() => dispatch("retrySpeechRefine")}
        />
      {:else}
        <AiScriptReviewPanel
          {transcriptEntries}
          {transcriptReady}
          analyzing={scriptQualityAnalyzing}
          analyzeError={scriptQualityError}
          qualitySummary={scriptQualitySummary}
          annotations={scriptQualityAnnotations}
          selectedCueId={selectedScriptCueId}
          on:seek={(event) => dispatch("seek", event.detail)}
          on:select={(event) => dispatch("selectScriptCue", event.detail)}
          on:analyze={() => dispatch("analyzeScriptQuality")}
        />
      {/if}
    </div>
  </section>

  {#if showDataBoard}
    <div class="data-board-slot" class:expanded={dataBoardExpanded}>
      <div class="data-board-chrome">
        <button
          type="button"
          class="data-board-toggle"
          aria-expanded={dataBoardExpanded}
          aria-controls="company-data-board-body"
          on:click={() => { dataBoardExpanded = !dataBoardExpanded; }}
        >
          {#if dataBoardExpanded}
            <ChevronDown size={14} />
          {:else}
            <ChevronRight size={14} />
          {/if}
          <span>数据看板</span>
          <small>{dataBoardExpanded ? "收起" : "展开"}</small>
        </button>
        {#if dataBoardExpanded}
          <div class="bottom-tabs" role="tablist" aria-label="数据看板视图">
            <button
              type="button"
              role="tab"
              class:active={bottomBoardTab === "data"}
              aria-selected={bottomBoardTab === "data"}
              on:click={() => { bottomBoardTab = "data"; }}
            >
              数据面板
            </button>
            <button
              type="button"
              role="tab"
              class:active={bottomBoardTab === "words"}
              aria-selected={bottomBoardTab === "words"}
              on:click={() => { bottomBoardTab = "words"; }}
            >
              高频词
            </button>
          </div>
        {/if}
      </div>
      <LiveTopProductsPanel {videoId} {archiveSource} {events} {transcriptEntries} />
      {#if dataBoardExpanded}
        <div id="company-data-board-body" class="data-board-body">
          {#if bottomBoardTab === "data"}
            <LiveDataBoardPanel
              loading={dashboardLoading}
              error={dashboardError}
              session={dashboardSession}
              matchMethod={dashboardMatchMethod}
              candidates={dashboardCandidates}
              bind:selectedSessionId={selectedDashboardSessionId}
              {orderSummary}
              {showDashboardBind}
              compactHeader={true}
              on:openDashboard={() => dispatch("openDashboard")}
              on:bindSession={() => dispatch("bindDashboardSession")}
              on:rebindSession={() => dispatch("rebindDashboardSession")}
            />
          {:else}
            <LiveHighFrequencyWordsPanel
              {transcriptEntries}
              {transcriptReady}
              compactHeader={true}
              on:seek={(event) => dispatch("seek", event.detail)}
            />
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .company-workspace {
    min-height: 0;
    height: 100%;
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: minmax(280px, 42%) minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    gap: 14px;
    padding: 14px;
    overflow: hidden;
    box-sizing: border-box;
  }
  .company-workspace.with-data-board {
    grid-template-rows: minmax(0, 1fr) auto;
  }
  .company-workspace.data-board-open {
    grid-template-rows: minmax(0, 1fr) minmax(0, auto);
  }
  .video-pane {
    grid-column: 1;
    grid-row: 1;
    min-height: 0;
    min-width: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 8px;
    border: 1px solid #e4e7ec;
    border-radius: 14px;
    background: #fff;
    padding: 10px;
    overflow: hidden;
  }
  .video-pane-title {
    font-size: 12px;
    font-weight: 600;
    color: #475467;
    flex: 0 0 auto;
  }
  /*
   * Player host: fills remaining pane height and shrinks with the window.
   * Children (EmbeddedVideoPlayer / legacy shells) must use height 100% + min-height 0.
   */
  .video-slot {
    position: relative;
    min-height: 0;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: stretch;
    justify-content: stretch;
    border-radius: 10px;
    overflow: hidden;
    background: #0b1220;
  }
  .video-slot :global(> *) {
    flex: 1 1 auto;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }
  .video-slot :global(video),
  .video-slot :global(iframe) {
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    border: 0;
    border-radius: 10px;
    background: #0f172a;
    object-fit: contain;
  }
  .video-slot :global(.empty-player),
  .video-slot :global(.embedded-player),
  .video-slot :global(.company-player-shell) {
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .workspace-pane {
    grid-column: 2;
    grid-row: 1;
    min-height: 0;
    min-width: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
    border: 1px solid #e4e7ec;
    border-radius: 14px;
    background: linear-gradient(180deg, #ffffff 0%, #f8fafc 100%);
    padding: 12px;
    overflow: hidden;
  }
  .data-board-slot {
    grid-column: 1 / -1;
    grid-row: 2;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 0 2px;
    overflow: hidden;
  }
  .data-board-slot.expanded {
    max-height: min(320px, 40vh);
  }
  .data-board-chrome {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    border-top: 1px solid #e5e7eb;
    padding-top: 6px;
  }
  .data-board-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
    border: 0;
    background: transparent;
    color: #374151;
    font-size: 13px;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }
  .data-board-toggle :global(svg) {
    color: #6b7280;
    flex: 0 0 auto;
  }
  .data-board-toggle small {
    margin-left: 4px;
    font-size: 12px;
    font-weight: 400;
    color: #9ca3af;
  }
  .bottom-tabs {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: #f3f4f6;
  }
  .bottom-tabs button {
    border: 0;
    border-radius: 6px;
    padding: 5px 12px;
    background: transparent;
    color: #6b7280;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .bottom-tabs button.active {
    background: #111827;
    color: #fff;
  }
  .data-board-body {
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding-top: 8px;
  }
  .data-board-body :global(.live-data-board),
  .data-board-body :global(.hf-panel) {
    flex: 1 1 auto;
    min-height: 0;
  }
  .workspace-head p {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: #667085;
    max-width: 560px;
  }
  .tab-row {
    display: inline-flex;
    gap: 4px;
    padding: 3px;
    border-radius: 10px;
    background: #f2f4f7;
  }
  .tab-row button {
    border: 0;
    border-radius: 8px;
    padding: 7px 14px;
    font-size: 13px;
    font-weight: 600;
    color: #475467;
    background: transparent;
    cursor: pointer;
  }
  .tab-row button.active {
    background: #fff;
    color: #175cd3;
    box-shadow: 0 1px 2px rgba(16, 24, 40, 0.08);
  }
  .workspace-body {
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .workspace-body :global(> *) {
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
  }

  @media (max-width: 1100px) {
    .company-workspace {
      grid-template-columns: minmax(240px, 38%) minmax(0, 1fr);
      gap: 10px;
      padding: 10px;
    }
  }

  @media (max-width: 860px) {
    .company-workspace {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(200px, 32%) minmax(0, 1fr);
    }
    .company-workspace.with-data-board {
      grid-template-rows: minmax(200px, 32%) minmax(0, 1fr) auto;
    }
    .video-pane {
      grid-column: 1;
      grid-row: 1;
      max-height: none;
    }
    .workspace-pane {
      grid-column: 1;
      grid-row: 2;
    }
    .data-board-slot {
      grid-column: 1;
      grid-row: 3;
    }
    .data-board-slot.expanded {
      max-height: min(280px, 36vh);
    }
  }
</style>
