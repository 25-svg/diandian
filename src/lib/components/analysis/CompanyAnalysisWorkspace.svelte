<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { ChevronDown, ChevronRight } from "lucide-svelte";
  import AlignSessionPanel from "./AlignSessionPanel.svelte";
  import DealSpeechPanel from "./DealSpeechPanel.svelte";
  import AiScriptReviewPanel from "./AiScriptReviewPanel.svelte";
  import LiveDataBoardPanel from "./LiveDataBoardPanel.svelte";
  import LiveHighFrequencyWordsPanel from "./LiveHighFrequencyWordsPanel.svelte";
  import LiveTopProductsPanel from "./LiveTopProductsPanel.svelte";
  import ClipReviewWorkspace from "../ClipReviewWorkspace.svelte";
  import type { PaymentEvent } from "../../orderDealTimeline";
  import type { ClipReviewRequest } from "../../clipReview";
  import type {
    CompanyAnalysisTab,
    RefinedDealSpeechRange,
    WorkspaceTranscriptEntry,
  } from "../../companyAnalysisWorkspace";
  import type { DanmuSpeakerEntry } from "../../dealWaveChatters";
  import {
    dashboardCandidateLabel,
    formatDashboardSessionLength,
    formatDashboardSessionTime,
    type LiveDataBoardCandidate,
    type LiveDataBoardOrderSummary,
    type LiveDataBoardSession,
  } from "../../liveDashboard";
  import type { ScriptIssueAnnotation } from "../../scriptQuality";
  import type { DealTranscriptWindow } from "../../dealTranscriptWindows";

  export let activeTab: CompanyAnalysisTab = "align";
  export let events: PaymentEvent[] = [];
  export let videoId: number | null = null;
  export let archiveSource = false;
  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  /** Used only to decide whether AI review still needs a full-session backfill. */
  export let nonDealTranscriptEntries: WorkspaceTranscriptEntry[] = [];
  export let dealSentenceCount = 0;
  export let dealWindows: DealTranscriptWindow[] = [];
  export let needsFullTranscriptBackfill = false;
  export let selectedOffsetSec: number | null = null;
  export let transcriptReady = false;
  export let scriptQualityAnalyzing = false;
  export let scriptQualityError = "";
  export let scriptQualitySummary = "";
  export let scriptQualityAnnotations: ScriptIssueAnnotation[] = [];
  export let selectedScriptCueId: number | null = null;
  /** Absolute playback seconds for 整场复盘 playhead sync. */
  export let playbackPositionSec = 0;
  export let canAutoClip = false;
  export let autoClipDisabledReason = "需要成交订单、逐字稿和可切片视频";
  export let autoClipping = false;
  export let autoClipProgress = "";
  export let autoClipError = "";
  export let refinedSpeechRange: RefinedDealSpeechRange | null = null;
  export let speechRefining = false;
  export let speechRefineProgress = "";
  export let speechRefineError = "";
  export let danmuEntries: DanmuSpeakerEntry[] = [];
  export let liveStartedAt: string | null = null;
  /** Deal-gap ASR is still merging into the shared transcript. */
  export let transcriptBackfilling = false;
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
  /** Latest auto-clip (or restored) review payload; enables 切片复盘 tab. */
  export let clipReviewRequest: ClipReviewRequest | null = null;

  /** Align-tab controls (orders + video sync). */
  export let alignOrderCount = 0;
  export let alignTotalPayYuan = 0;
  export let alignLoading = false;
  export let alignError = "";
  export let alignCanPullOrders = false;
  export let alignCanImportRawJson = false;
  export let alignPeakLabel = "";
  export let alignCanCalibrateCurrent = false;
  export let alignShowCalibration = false;
  export let alignCalibrationWarning = false;
  export let excelCanDownload = false;
  export let excelDownloading = false;
  export let excelDownloadHint = "";

  /** Default collapsed so analysis UI stays focused; expand when needed. */
  let dataBoardExpanded = true;
  let autoOpenedForBind = false;
  /** Only one bottom view at a time. */
  let bottomBoardTab: "data" | "words" = "data";

  $: clipReviewMode = activeTab === "clip_review";
  $: showBottomBoard = showDataBoard && !clipReviewMode;
  $: needsManualBind = showBottomBoard && showDashboardBind && !dashboardSession && dashboardCandidates.length > 0 && !dashboardLoading;
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
    closeClipReview: void;
    openClipReview: void;
    importClean: void;
    pullOrders: void;
    importRawJson: void;
    importCleanedFile: void;
    clearOrders: void;
    seekPeak: void;
    confirmLiveStart: void;
    calibrateCurrent: void;
    downloadExcel: void;
  }>();
</script>

<div
  class="company-workspace"
  class:with-data-board={showBottomBoard}
  class:data-board-open={showBottomBoard && dataBoardExpanded}
  class:clip-review-mode={clipReviewMode}
>
  {#if !clipReviewMode}
    <aside class="video-pane" aria-label="录播视频">
      <div class="video-pane-title">录播视频 · 整场</div>
      <div class="video-slot">
        <slot name="player" />
      </div>
    </aside>
  {/if}

  <section class="workspace-pane" aria-label="公司录播分析">
    <header class="workspace-head">
      <div class="tab-row" role="tablist" aria-label="分析面板">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "align"}
          class:active={activeTab === "align"}
          on:click={() => (activeTab = "align")}
        >
          对齐
        </button>
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
          aria-selected={activeTab === "clip_review"}
          class:active={activeTab === "clip_review"}
          title={clipReviewRequest ? "查看本场切片复盘" : "点击加载已生成的切片"}
          on:click={() => {
            if (clipReviewRequest) activeTab = "clip_review";
            else dispatch("openClipReview");
          }}
        >
          切片复盘
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "ai_review"}
          class:active={activeTab === "ai_review"}
          on:click={() => (activeTab = "ai_review")}
        >
          整场复盘
        </button>
      </div>
    </header>

    <div class="workspace-body">
      {#if activeTab === "align"}
        <AlignSessionPanel
          orderCount={alignOrderCount}
          totalPayYuan={alignTotalPayYuan}
          loading={alignLoading}
          error={alignError}
          canPullOrders={alignCanPullOrders}
          canImportRawJson={alignCanImportRawJson}
          peakLabel={alignPeakLabel}
          canCalibrateCurrent={alignCanCalibrateCurrent}
          showCalibration={alignShowCalibration}
          calibrationWarning={alignCalibrationWarning}
          on:importClean={() => dispatch("importClean")}
          on:pullOrders={() => dispatch("pullOrders")}
          on:importRawJson={() => dispatch("importRawJson")}
          on:importCleanedFile={() => dispatch("importCleanedFile")}
          on:clearOrders={() => dispatch("clearOrders")}
          on:seekPeak={() => dispatch("seekPeak")}
          on:confirmLiveStart={() => dispatch("confirmLiveStart")}
          on:calibrateCurrent={() => dispatch("calibrateCurrent")}
          on:bindExcel={() => dispatch("bindDashboardSession")}
          excelLabel={dashboardSession
            ? [
                dashboardSession.shopName,
                formatDashboardSessionTime(dashboardSession.startedAt),
                formatDashboardSessionLength(dashboardSession.startedAt, dashboardSession.endedAt),
              ].filter(Boolean).join(" · ")
            : ""}
          excelCandidates={dashboardCandidates.map((candidate) => ({
            id: candidate.session.id,
            label: dashboardCandidateLabel(candidate),
          }))}
          bind:selectedExcelId={selectedDashboardSessionId}
          excelLoading={dashboardLoading}
          {excelCanDownload}
          {excelDownloading}
          {excelDownloadHint}
          on:downloadExcel={() => dispatch("downloadExcel")}
        />
      {:else if activeTab === "deal_speech"}
        <DealSpeechPanel
          {events}
          {transcriptEntries}
          {selectedOffsetSec}
          {transcriptReady}
          {canAutoClip}
          {autoClipDisabledReason}
          {autoClipping}
          {autoClipProgress}
          {autoClipError}
          {refinedSpeechRange}
          {speechRefining}
          {speechRefineProgress}
          {speechRefineError}
          {danmuEntries}
          {liveStartedAt}
          on:seek={(event) => dispatch("seek", event.detail)}
          on:select={(event) => dispatch("select", event.detail)}
          on:autoClip={() => dispatch("autoClip")}
          on:retrySpeechRefine={() => dispatch("retrySpeechRefine")}
        />
      {:else if activeTab === "clip_review"}
        {#if clipReviewRequest}
          <ClipReviewWorkspace
            embedded
            request={clipReviewRequest}
            backgroundProgress={autoClipping ? autoClipProgress : ""}
            on:close={() => {
              activeTab = "ai_review";
              dispatch("closeClipReview");
            }}
          />
        {:else}
          <div class="clip-review-empty">
            <strong>还没有切片</strong>
            <span>先到「成交话术」点「AI 分析并切片」。</span>
          </div>
        {/if}
      {:else}
        <AiScriptReviewPanel
          {transcriptEntries}
          {dealWindows}
          {dealSentenceCount}
          {needsFullTranscriptBackfill}
          {transcriptReady}
          analyzing={scriptQualityAnalyzing}
          analyzeError={scriptQualityError}
          qualitySummary={scriptQualitySummary}
          annotations={scriptQualityAnnotations}
          selectedCueId={selectedScriptCueId}
          {playbackPositionSec}
          on:seek={(event) => dispatch("seek", event.detail)}
          on:select={(event) => dispatch("selectScriptCue", event.detail)}
          on:analyze={() => dispatch("analyzeScriptQuality")}
        />
      {/if}
    </div>
  </section>

  {#if showBottomBoard}
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
          <small>{dataBoardExpanded ? "收起" : "展开 · 显示数据面板/高频词"}</small>
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
      {#if dataBoardExpanded}
        <div class="data-board-expanded-grid">
          <LiveTopProductsPanel {events} {transcriptEntries} {transcriptBackfilling} />
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
        </div>
      {:else}
        <LiveTopProductsPanel {events} {transcriptEntries} {transcriptBackfilling} />
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
  .company-workspace.clip-review-mode {
    grid-template-columns: minmax(0, 1fr);
  }
  .company-workspace.with-data-board {
    grid-template-rows: minmax(0, 1fr) auto;
  }
  .company-workspace.data-board-open {
    grid-template-rows: minmax(0, 1fr) minmax(0, auto);
  }
  .clip-review-empty {
    min-height: 220px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
    color: #667085;
  }
  .clip-review-empty strong {
    color: #101828;
    font-size: 14px;
  }
  .clip-review-empty span {
    max-width: 420px;
    font-size: 12px;
    line-height: 1.55;
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
    max-height: min(460px, 48vh);
  }
  .data-board-expanded-grid {
    min-height: 0;
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: minmax(240px, 0.95fr) minmax(280px, 1.15fr);
    gap: 10px;
    overflow: hidden;
  }
  .data-board-chrome {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    border-top: 1px solid #e5e7eb;
    padding-top: 6px;
    flex: 0 0 auto;
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
    min-width: 0;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    padding: 0;
    border: 1px solid #e5efff;
    border-radius: 10px;
    background: #fff;
  }
  .data-board-body :global(.live-data-board),
  .data-board-body :global(.hf-panel) {
    flex: 1 1 auto;
    min-height: 0;
  }
  .data-board-expanded-grid :global(.top-products-panel) {
    min-height: 0;
    overflow: auto;
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
  .tab-row button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
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
      max-height: min(420px, 48vh);
    }
    .data-board-expanded-grid {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(120px, 0.9fr) minmax(140px, 1.1fr);
    }
  }
</style>
