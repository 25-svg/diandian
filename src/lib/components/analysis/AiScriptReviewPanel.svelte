<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import {
    Activity,
    AlertCircle,
    CheckCircle2,
    ChevronLeft,
    ChevronRight,
    Clock3,
    Gauge,
    Layers3,
    ShoppingCart,
    Sparkles,
    TimerReset,
  } from "lucide-svelte";
  import { formatWorkspaceClock, type WorkspaceTranscriptEntry } from "../../companyAnalysisWorkspace";
  import type { PaymentEvent } from "../../orderDealTimeline";
  import {
    cueOverlapsDealWindow,
    type DealTranscriptWindow,
  } from "../../dealTranscriptWindows";
  import {
    scriptIssueKindLabel,
    type ScriptIssueAnnotation,
  } from "../../scriptQuality";
  import {
    buildSessionRhythmAnalysis,
    type SessionRhythmBucket,
    type SessionRhythmReview,
  } from "../../sessionRhythm";

  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  export let events: PaymentEvent[] = [];
  export let durationSec = 0;
  /** Deal ASR windows — overlapping cues are shown with a 「成交」 badge. */
  export let dealWindows: DealTranscriptWindow[] = [];
  export let dealSentenceCount = 0;
  /** True when only deal-window cues exist and full-session ASR can be backfilled. */
  export let needsFullTranscriptBackfill = false;
  export let transcriptReady = false;
  export let analyzing = false;
  export let analyzeError = "";
  export let qualitySummary = "";
  export let annotations: ScriptIssueAnnotation[] = [];
  export let rhythmReview: SessionRhythmReview | null = null;
  export let selectedCueId: number | null = null;
  /** Absolute playback clock (seconds) for playhead → cue highlight. */
  export let playbackPositionSec = 0;

  const dispatch = createEventDispatcher<{ seek: number; analyze: void; select: number }>();

  let transcriptBoardEl: HTMLDivElement | null = null;
  let lastPlayScrollId: number | null = null;
  let lastSelectScrollId: number | null = null;
  let selectedRhythmBucketIndex = 0;
  let rhythmSourceSignature = "";

  $: annotationMap = new Map(annotations.map((item) => [item.cueId, item]));
  $: issueCount = annotations.length;
  $: rhythmAnalysis = buildSessionRhythmAnalysis(transcriptEntries, events, durationSec);
  $: nextRhythmSourceSignature = rhythmAnalysis.buckets.length
    ? `${rhythmAnalysis.metrics.durationSec}:${rhythmAnalysis.buckets.length}:${transcriptEntries.length}:${events.length}`
    : "";
  $: if (nextRhythmSourceSignature !== rhythmSourceSignature) {
    rhythmSourceSignature = nextRhythmSourceSignature;
    selectedRhythmBucketIndex = rhythmAnalysis.metrics.peakBucketIndex ?? 0;
  }
  $: selectedRhythmBucket = rhythmAnalysis.buckets.find(
    (bucket) => bucket.index === selectedRhythmBucketIndex,
  ) ?? rhythmAnalysis.buckets[0] ?? null;
  $: playingRhythmBucketIndex = rhythmAnalysis.buckets.find(
    (bucket) => bucket.startSec <= playbackPositionSec && playbackPositionSec < bucket.endSec,
  )?.index ?? null;
  $: selectedAnnotation = selectedCueId == null ? null : annotationMap.get(selectedCueId) ?? null;
  $: selectedEntry = selectedCueId == null
    ? null
    : transcriptEntries.find((entry) => entry.id === selectedCueId) ?? null;
  $: nonDealSentenceCount = Math.max(0, transcriptEntries.length - dealSentenceCount);
  $: canStartReview = transcriptEntries.length > 0 || needsFullTranscriptBackfill;
  $: primaryLabel = analyzing
    ? (needsFullTranscriptBackfill && !transcriptEntries.length ? "补转中…" : "复盘中…")
    : needsFullTranscriptBackfill && !transcriptEntries.length
      ? "补转整场文稿并复盘"
      : issueCount || rhythmReview
        ? "重新整场复盘"
        : "开始整场复盘";

  $: issueSteps = annotations
    .map((annotation) => {
      const entry = transcriptEntries.find((item) => item.id === annotation.cueId);
      return entry ? { annotation, entry } : null;
    })
    .filter((item): item is { annotation: ScriptIssueAnnotation; entry: WorkspaceTranscriptEntry } => Boolean(item))
    .sort((left, right) => left.entry.start - right.entry.start);

  $: selectedIssueIndex = selectedCueId == null
    ? -1
    : issueSteps.findIndex((item) => item.annotation.cueId === selectedCueId);

  $: playingCueId = findPlayingCueId(transcriptEntries, playbackPositionSec);

  $: if (playingCueId != null && playingCueId !== lastPlayScrollId) {
    lastPlayScrollId = playingCueId;
    void scrollCueIntoView(playingCueId);
  }

  $: if (selectedCueId != null && selectedCueId !== lastSelectScrollId) {
    lastSelectScrollId = selectedCueId;
    void scrollCueIntoView(selectedCueId);
  }

  function findPlayingCueId(
    entries: readonly WorkspaceTranscriptEntry[],
    positionSec: number,
  ): number | null {
    if (!entries.length || !Number.isFinite(positionSec)) return null;
    let fallback: number | null = null;
    for (const entry of entries) {
      if (entry.start <= positionSec && positionSec < entry.end) return entry.id;
      if (entry.start <= positionSec) fallback = entry.id;
      if (entry.start > positionSec) break;
    }
    return fallback;
  }

  async function scrollCueIntoView(cueId: number): Promise<void> {
    if (!transcriptBoardEl) return;
    await tick();
    const node = transcriptBoardEl.querySelector<HTMLElement>(`[data-cue-id="${cueId}"]`);
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    node?.scrollIntoView({ block: "nearest", behavior: reducedMotion ? "auto" : "smooth" });
  }

  function isDealCue(entry: WorkspaceTranscriptEntry): boolean {
    return cueOverlapsDealWindow(entry, dealWindows);
  }

  function handleRowClick(entry: WorkspaceTranscriptEntry): void {
    dispatch("seek", entry.start);
    if (annotationMap.has(entry.id)) {
      dispatch("select", entry.id);
    }
  }

  function goIssue(delta: number): void {
    if (!issueSteps.length) return;
    const base = selectedIssueIndex >= 0 ? selectedIssueIndex : (delta > 0 ? -1 : 0);
    const nextIndex = Math.max(0, Math.min(issueSteps.length - 1, base + delta));
    const target = issueSteps[nextIndex];
    if (!target) return;
    lastSelectScrollId = null;
    dispatch("select", target.annotation.cueId);
    dispatch("seek", target.entry.start);
  }

  function selectRhythmBucket(bucket: SessionRhythmBucket): void {
    selectedRhythmBucketIndex = bucket.index;
    dispatch("seek", bucket.startSec);
  }

  function formatPercent(value: number): string {
    return `${Math.round(Math.max(0, value) * 100)}%`;
  }

  function formatPause(value: number): string {
    const safe = Math.max(0, Math.round(value));
    return safe >= 60 ? `${Math.floor(safe / 60)}分${safe % 60}秒` : `${safe}秒`;
  }
</script>

<section class="ai-review-panel" aria-label="整场复盘">
  <div class="panel-top">
    <div class="legend" aria-label="改进类型图例">
      <span><i class="dot structure"></i>讲法顺序</span>
      <span><i class="dot unclear"></i>表达可优化</span>
      <span><i class="dot factual"></i>信息不准确</span>
      <span><i class="dot compliance"></i>承诺过重</span>
      <span><i class="badge-legend">成交</i>成交窗文稿</span>
      <span><i class="dot playing"></i>正在播放</span>
    </div>

    <p class="scope-line">
      整场文稿 · 共 {transcriptEntries.length} 句
      {#if dealSentenceCount} · 成交窗 {dealSentenceCount} 句已标注{/if}
      {#if nonDealSentenceCount && dealSentenceCount} · 非成交 {nonDealSentenceCount} 句{/if}
      {#if issueCount} · 可改进 {issueCount} 处{/if}
      · AI 复盘重点在非成交段 · 点击句子跳转视频
    </p>
  </div>

  <section class="rhythm-panel" aria-label="整场节奏与结构地图">
    <div class="rhythm-heading">
      <div>
        <span class="rhythm-eyebrow">整场节奏地图</span>
        <strong>{rhythmAnalysis.summary}</strong>
      </div>
      <span class="rhythm-window-label">
        每格 {Math.max(1, Math.round(rhythmAnalysis.bucketSec / 60))} 分钟 · 点击跳转
      </span>
    </div>

    {#if rhythmAnalysis.buckets.length && !needsFullTranscriptBackfill}
      <div class="metric-grid" aria-label="整场节奏指标">
        <div class="metric-card">
          <Activity size={16} aria-hidden="true" />
          <span>话术覆盖</span>
          <strong>{formatPercent(rhythmAnalysis.metrics.spokenRatio)}</strong>
        </div>
        <div class="metric-card">
          <Gauge size={16} aria-hidden="true" />
          <span>平均字速</span>
          <strong>{Math.round(rhythmAnalysis.metrics.charactersPerMinute)}<small> 字/分</small></strong>
        </div>
        <div class="metric-card">
          <TimerReset size={16} aria-hidden="true" />
          <span>最长低活跃</span>
          <strong>{formatPause(rhythmAnalysis.metrics.longestPauseSec)}</strong>
        </div>
        <div class="metric-card">
          <Layers3 size={16} aria-hidden="true" />
          <span>结构完整度</span>
          <strong>{rhythmAnalysis.metrics.structureCompleteness}%</strong>
        </div>
        <div class="metric-card">
          <Clock3 size={16} aria-hidden="true" />
          <span>转品提示</span>
          <strong>{rhythmAnalysis.metrics.transitionCount}<small> 次</small></strong>
        </div>
        <div class="metric-card">
          <ShoppingCart size={16} aria-hidden="true" />
          <span>成交订单</span>
          <strong>{rhythmAnalysis.metrics.orderCount}<small> 单</small></strong>
        </div>
      </div>

      <nav class="rhythm-timeline" aria-label="可跳转的整场结构时间轴">
        {#each rhythmAnalysis.buckets as bucket (bucket.index)}
          <button
            type="button"
            class="rhythm-block kind-{bucket.kind}"
            class:selected={selectedRhythmBucketIndex === bucket.index}
            class:playing={playingRhythmBucketIndex === bucket.index}
            aria-label={`${formatWorkspaceClock(bucket.startSec)}到${formatWorkspaceClock(bucket.endSec)}，${bucket.label}，话术覆盖${formatPercent(bucket.speechRatio)}，${bucket.orderCount}单`}
            aria-current={playingRhythmBucketIndex === bucket.index ? "time" : undefined}
            title={`${formatWorkspaceClock(bucket.startSec)}–${formatWorkspaceClock(bucket.endSec)} · ${bucket.label}`}
            on:click={() => selectRhythmBucket(bucket)}
          >
            <time>{formatWorkspaceClock(bucket.startSec)}</time>
            <span>{bucket.label}</span>
            {#if bucket.orderCount}<small>{bucket.orderCount}单</small>{/if}
          </button>
        {/each}
      </nav>

      {#if selectedRhythmBucket}
        <div class="rhythm-detail" aria-live="polite">
          <div class="detail-kind kind-{selectedRhythmBucket.kind}">{selectedRhythmBucket.label}</div>
          <time>{formatWorkspaceClock(selectedRhythmBucket.startSec)}–{formatWorkspaceClock(selectedRhythmBucket.endSec)}</time>
          <span>话术覆盖 {formatPercent(selectedRhythmBucket.speechRatio)}</span>
          <span>{Math.round(selectedRhythmBucket.charactersPerMinute)} 字/分</span>
          <span>最长低活跃 {formatPause(selectedRhythmBucket.longestPauseSec)}</span>
          {#if selectedRhythmBucket.orderCount}<span>{selectedRhythmBucket.orderCount} 单成交</span>{/if}
          <p>{selectedRhythmBucket.sampleText || "该时间段没有识别到有效话术。"}</p>
        </div>
      {/if}
    {:else}
      <div class="rhythm-empty">
        <Activity size={20} aria-hidden="true" />
        <span>{needsFullTranscriptBackfill ? "当前只有成交窗文稿。补转整场后再生成完整节奏图。" : "整场逐字稿完成后，这里自动生成节奏时间轴。"}</span>
      </div>
    {/if}
  </section>

  <div class="split-body">
    <div class="review-column" aria-label="复盘文稿">
      <div class="column-head">复盘文稿</div>
      <div class="transcript-board" bind:this={transcriptBoardEl}>
        {#if transcriptEntries.length}
          {#each transcriptEntries as entry (entry.id)}
            {@const issue = annotationMap.get(entry.id)}
            {@const dealCue = isDealCue(entry)}
            <button
              type="button"
              class="transcript-row"
              data-cue-id={entry.id}
              class:is-deal={dealCue}
              class:is-playing={playingCueId === entry.id}
              class:has-issue={Boolean(issue)}
              class:selected={selectedCueId === entry.id}
              class:kind-structure-gap={issue?.kind === "structure_gap"}
              class:kind-unclear-expression={issue?.kind === "unclear_expression"}
              class:kind-factual-risk={issue?.kind === "factual_risk"}
              class:kind-compliance-risk={issue?.kind === "compliance_risk"}
              on:click={() => handleRowClick(entry)}
            >
              <time>{formatWorkspaceClock(entry.start)}</time>
              <p>
                {#if dealCue}<span class="deal-tag">成交</span>{/if}
                {entry.text}
              </p>
            </button>
          {/each}
        {:else if transcriptReady && needsFullTranscriptBackfill}
          <div class="empty-box">
            <AlertCircle size={22} />
            <p>还没有可展示的整场文稿。点下方可先补转，再显示全部句子（成交窗会标注）。</p>
          </div>
        {:else if transcriptReady}
          <div class="empty-box">
            <AlertCircle size={22} />
            <p>暂无逐字稿内容。</p>
          </div>
        {:else}
          <div class="empty-box">
            <p>逐字稿生成中…</p>
          </div>
        {/if}
      </div>
    </div>

    <aside class="optimize-column" aria-label="优化建议">
      <div class="column-head optimize-head">
        <span>优化建议</span>
        {#if issueCount}
          <div class="issue-nav" aria-label="可改进点导航">
            <button
              type="button"
              class="nav-btn"
              disabled={issueSteps.length === 0 || selectedIssueIndex <= 0}
              title="上一条"
              aria-label="上一条可改进点"
              on:click={() => goIssue(-1)}
            >
              <ChevronLeft size={15} />
            </button>
            <span class="nav-count">
              {selectedIssueIndex >= 0 ? selectedIssueIndex + 1 : "—"}/{issueSteps.length}
            </span>
            <button
              type="button"
              class="nav-btn"
              disabled={issueSteps.length === 0 || selectedIssueIndex >= issueSteps.length - 1}
              title="下一条"
              aria-label="下一条可改进点"
              on:click={() => goIssue(1)}
            >
              <ChevronRight size={15} />
            </button>
          </div>
        {/if}
      </div>
      <div class="optimize-scroll">
        {#if analyzeError}
          <div class="analyze-error">{analyzeError}</div>
        {/if}

        {#if qualitySummary}
          <div class="quality-summary">
            <span class="detail-label">逐句话术概要</span>
            {qualitySummary}
          </div>
        {/if}

        {#if rhythmReview}
          <section class="rhythm-review-card" aria-label="AI整场结构判断">
            <div class="rhythm-review-title">
              <Sparkles size={15} aria-hidden="true" />
              <span>AI 整场结构判断</span>
            </div>
            {#if rhythmReview.structureVerdict}<strong>{rhythmReview.structureVerdict}</strong>{/if}
            {#if rhythmReview.summary}<p>{rhythmReview.summary}</p>{/if}
            {#if rhythmReview.goodPoints.length}
              <div class="review-list good-list">
                <span class="detail-label">做得好的地方</span>
                {#each rhythmReview.goodPoints as point}
                  <div><CheckCircle2 size={14} aria-hidden="true" /><span>{point}</span></div>
                {/each}
              </div>
            {/if}
            {#if rhythmReview.improvements.length}
              <div class="review-list improve-list">
                <span class="detail-label">下场优先调整</span>
                {#each rhythmReview.improvements as point, index}
                  <div><i>{index + 1}</i><span>{point}</span></div>
                {/each}
              </div>
            {/if}
          </section>
        {:else if rhythmAnalysis.suggestions.length && !needsFullTranscriptBackfill}
          <section class="local-rhythm-card" aria-label="本地节奏提示">
            <span class="detail-label">即时节奏提示</span>
            <p>无需等待 AI，先根据逐字稿时间戳和订单信号给出：</p>
            <ul>
              {#each rhythmAnalysis.suggestions as suggestion}
                <li>{suggestion}</li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if selectedAnnotation}
          <div class="issue-detail">
            <strong>{scriptIssueKindLabel(selectedAnnotation.kind)}</strong>
            {#if selectedEntry}
              <p class="cue-ref">
                <time>{formatWorkspaceClock(selectedEntry.start)}</time>
                {selectedEntry.text}
              </p>
            {/if}
            <p><span class="detail-label">为什么不够好</span>{selectedAnnotation.reason}</p>
            <p class="suggestion"><span class="detail-label">下次可以怎么说</span>{selectedAnnotation.suggestion}</p>
          </div>
        {:else if issueCount}
          <div class="empty-box slim">
            <p>点「下一条」或左侧有色句子，这里显示改法建议。</p>
          </div>
        {:else if analyzing}
          <div class="empty-box slim">
            <p>复盘分析中…</p>
          </div>
        {:else}
          <div class="empty-box slim">
            <p>先点下方开始整场复盘，再在左侧查看可改进句子。</p>
          </div>
        {/if}
      </div>
    </aside>
  </div>

  <footer class="panel-foot">
    <button
      type="button"
      class="primary"
      disabled={!canStartReview || analyzing}
      on:click={() => dispatch("analyze")}
    >
      <Sparkles size={15} />
      {primaryLabel}
    </button>
    <button type="button" class="secondary" disabled={!issueCount || analyzing}>
      导出复盘摘要（下一步）
    </button>
  </footer>
</section>

<style>
  .ai-review-panel {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr) auto;
    gap: 10px;
    min-height: 0;
    height: 100%;
  }
  .panel-top { display: grid; gap: 6px; min-width: 0; }
  .legend { display: flex; flex-wrap: wrap; gap: 10px; font-size: 11px; color: #475467; }
  .legend span { display: inline-flex; align-items: center; gap: 5px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
  .dot.structure { background: #f04438; }
  .dot.unclear { background: #f79009; }
  .dot.factual { background: #eab308; }
  .dot.compliance { background: #7a5af8; }
  .dot.playing { background: #12b76a; }
  .badge-legend {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 5px;
    border-radius: 4px;
    font-size: 10px;
    font-style: normal;
    font-weight: 600;
    color: #175cd3;
    background: #eff8ff;
    border: 1px solid #b2ddff;
  }
  .scope-line { margin: 0; font-size: 11px; color: #667085; }
  .rhythm-panel {
    min-width: 0;
    display: grid;
    gap: 8px;
    padding: 10px;
    border: 1px solid #dbe7f6;
    border-radius: 12px;
    background: linear-gradient(135deg, #f8fbff 0%, #ffffff 58%, #f7f9fc 100%);
  }
  .rhythm-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .rhythm-heading > div { min-width: 0; display: grid; gap: 2px; }
  .rhythm-eyebrow { color: #175cd3; font-size: 10px; font-weight: 700; letter-spacing: .04em; }
  .rhythm-heading strong { color: #1d2939; font-size: 12px; line-height: 1.45; }
  .rhythm-window-label { flex: 0 0 auto; color: #667085; font-size: 10px; }
  .metric-grid {
    display: grid;
    grid-template-columns: repeat(6, minmax(86px, 1fr));
    gap: 6px;
  }
  .metric-card {
    min-width: 0;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 1px 6px;
    padding: 7px 8px;
    border: 1px solid #e4e7ec;
    border-radius: 9px;
    background: rgba(255, 255, 255, .9);
  }
  .metric-card :global(svg) { grid-row: 1 / span 2; color: #667085; }
  .metric-card span { min-width: 0; overflow: hidden; color: #667085; font-size: 9px; white-space: nowrap; text-overflow: ellipsis; }
  .metric-card strong { color: #101828; font-size: 13px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .metric-card small { color: #667085; font-size: 9px; font-weight: 500; }
  .rhythm-timeline {
    min-width: 0;
    display: flex;
    gap: 4px;
    padding: 2px 1px 6px;
    overflow-x: auto;
    scrollbar-width: thin;
    scroll-snap-type: x proximity;
  }
  .rhythm-block {
    position: relative;
    flex: 0 0 64px;
    min-height: 46px;
    display: grid;
    align-content: center;
    gap: 1px;
    padding: 5px 6px;
    border: 1px solid transparent;
    border-radius: 8px;
    color: #344054;
    text-align: left;
    cursor: pointer;
    scroll-snap-align: start;
  }
  .rhythm-block time { font-size: 9px; font-variant-numeric: tabular-nums; opacity: .78; }
  .rhythm-block span { overflow: hidden; font-size: 10px; font-weight: 700; white-space: nowrap; text-overflow: ellipsis; }
  .rhythm-block small { font-size: 9px; font-weight: 700; }
  .rhythm-block:hover { filter: brightness(.97); }
  .rhythm-block:focus-visible { outline: 3px solid rgba(21, 112, 239, .28); outline-offset: 1px; }
  .rhythm-block.selected { border-color: #1570ef; box-shadow: 0 0 0 2px rgba(21, 112, 239, .12); }
  .rhythm-block.playing::after {
    content: "";
    position: absolute;
    right: 4px;
    top: 4px;
    width: 6px;
    height: 6px;
    border: 2px solid white;
    border-radius: 999px;
    background: #12b76a;
    box-shadow: 0 0 0 1px #027a48;
  }
  .kind-opening { background: #eaf2ff; }
  .kind-interaction { background: #e6f9fb; }
  .kind-product { background: #f1edff; }
  .kind-trust { background: #eafbf2; }
  .kind-offer { background: #fff5d6; }
  .kind-objection { background: #fff0e8; }
  .kind-conversion { background: #ffe9e7; }
  .kind-transition { background: #edf0ff; }
  .kind-low_activity { background: #f2f4f7; }
  .kind-mixed { background: #eef2f6; }
  .rhythm-detail {
    min-width: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px 10px;
    color: #667085;
    font-size: 10px;
  }
  .rhythm-detail time, .rhythm-detail span { font-variant-numeric: tabular-nums; }
  .rhythm-detail p {
    flex: 1 1 100%;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: #344054;
    font-size: 11px;
    line-height: 1.4;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .detail-kind {
    padding: 2px 6px;
    border-radius: 5px;
    color: #1d2939;
    font-weight: 700;
  }
  .rhythm-empty {
    min-height: 74px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 6px;
    color: #667085;
    font-size: 11px;
  }
  .split-body {
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1.35fr) minmax(280px, 0.9fr);
    gap: 10px;
  }
  .review-column, .optimize-column {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 6px;
  }
  .column-head {
    font-size: 12px;
    font-weight: 600;
    color: #344054;
  }
  .optimize-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .issue-nav {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .nav-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: 1px solid #d0d5dd;
    border-radius: 6px;
    background: #fff;
    color: #344054;
    cursor: pointer;
  }
  .nav-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .nav-count {
    min-width: 42px;
    text-align: center;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: #667085;
  }
  .optimize-scroll {
    min-height: 0;
    overflow: auto;
    display: grid;
    align-content: start;
    gap: 10px;
    padding: 10px;
    border: 1px solid #e4e7ec;
    border-radius: 12px;
    background: #fcfcfd;
  }
  .quality-summary, .analyze-error {
    margin: 0;
    padding: 8px 10px;
    border-radius: 10px;
    font-size: 12px;
    line-height: 1.5;
  }
  .quality-summary { background: #eff8ff; color: #175cd3; border: 1px solid #b2ddff; }
  .analyze-error { background: #fef3f2; color: #b42318; border: 1px solid #fecdca; }
  .issue-detail {
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid #e4e7ec;
    background: white;
    font-size: 12px;
    color: #344054;
  }
  .rhythm-review-card, .local-rhythm-card {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid #dbe7f6;
    background: #fff;
    color: #344054;
    font-size: 12px;
  }
  .rhythm-review-title { display: flex; align-items: center; gap: 6px; color: #175cd3; font-size: 11px; font-weight: 700; }
  .rhythm-review-card > strong { color: #101828; font-size: 13px; line-height: 1.45; }
  .rhythm-review-card > p, .local-rhythm-card > p { margin: 0; line-height: 1.55; }
  .review-list { display: grid; gap: 5px; padding-top: 4px; border-top: 1px solid #eef2f6; }
  .review-list > div { display: grid; grid-template-columns: 16px minmax(0, 1fr); gap: 6px; align-items: start; line-height: 1.45; }
  .good-list :global(svg) { margin-top: 1px; color: #12b76a; }
  .improve-list i { display: grid; place-content: center; width: 16px; height: 16px; border-radius: 50%; background: #fff3e0; color: #b54708; font-size: 9px; font-style: normal; font-weight: 700; }
  .local-rhythm-card { background: #fffcf5; border-color: #fedf89; }
  .local-rhythm-card ul { display: grid; gap: 5px; margin: 0; padding-left: 18px; line-height: 1.45; }
  .issue-detail strong { display: block; margin-bottom: 6px; color: #101828; }
  .detail-label { display: block; margin-bottom: 2px; font-size: 10px; font-weight: 600; color: #667085; }
  .issue-detail p { margin: 0 0 8px; line-height: 1.5; }
  .issue-detail .suggestion { color: #027a48; margin-bottom: 0; }
  .cue-ref {
    padding: 8px 10px;
    border-radius: 8px;
    background: #f8fafc;
    color: #475467;
    border: 1px solid #eef2f6;
  }
  .cue-ref time {
    display: inline-block;
    margin-right: 8px;
    color: #667085;
    font-variant-numeric: tabular-nums;
  }
  .transcript-board {
    min-height: 0;
    overflow: auto;
    border: 1px solid #e4e7ec;
    border-radius: 12px;
    background: white;
    padding: 4px 6px;
  }
  .transcript-row {
    width: 100%;
    display: grid;
    grid-template-columns: 54px minmax(0, 1fr);
    gap: 8px;
    padding: 7px 4px;
    border: 0;
    border-bottom: 1px solid #f2f4f7;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .transcript-row:hover { background: #f8fafc; }
  .transcript-row.selected { background: #eff8ff; }
  .transcript-row.is-deal { background: #f8fafc; }
  .transcript-row.is-deal:hover { background: #f0f4f8; }
  .transcript-row.is-playing {
    background: #ecfdf3;
    box-shadow: inset 3px 0 0 #12b76a;
  }
  .transcript-row.is-playing.selected {
    background: #e8f8ef;
  }
  .transcript-row.has-issue { border-left: 3px solid transparent; padding-left: 1px; }
  .transcript-row.kind-structure-gap { border-left-color: #f04438; background: #fff5f5; }
  .transcript-row.kind-unclear-expression { border-left-color: #f79009; background: #fffaeb; }
  .transcript-row.kind-factual-risk { border-left-color: #eab308; background: #fefce8; }
  .transcript-row.kind-compliance-risk { border-left-color: #7a5af8; background: #f4f3ff; }
  .transcript-row.is-playing.kind-structure-gap,
  .transcript-row.is-playing.kind-unclear-expression,
  .transcript-row.is-playing.kind-factual-risk,
  .transcript-row.is-playing.kind-compliance-risk {
    box-shadow: inset 3px 0 0 #12b76a;
  }
  .transcript-row time { color: #667085; font-size: 11px; font-variant-numeric: tabular-nums; }
  .transcript-row p { margin: 0; font-size: 13px; line-height: 1.55; color: #1f2937; }
  .deal-tag {
    display: inline-block;
    margin-right: 6px;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    color: #175cd3;
    background: #eff8ff;
    border: 1px solid #b2ddff;
    vertical-align: 1px;
  }
  .panel-foot { display: flex; gap: 8px; }
  .primary, .secondary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    font-size: 12px;
    cursor: pointer;
  }
  .primary { border: 1px solid #175cd3; background: #175cd3; color: white; }
  .secondary { border: 1px solid #d0d5dd; background: white; color: #344054; }
  .primary:disabled, .secondary:disabled { opacity: 0.45; cursor: not-allowed; }
  .empty-box {
    display: grid;
    justify-items: center;
    gap: 8px;
    padding: 28px;
    color: #667085;
    font-size: 13px;
    text-align: center;
  }
  .empty-box.slim { padding: 18px 12px; }
  @media (max-width: 980px) {
    .metric-grid { grid-template-columns: repeat(3, minmax(92px, 1fr)); }
    .split-body {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(220px, 1fr) minmax(180px, 0.85fr);
    }
  }
  @media (max-width: 640px) {
    .rhythm-heading { display: grid; }
    .rhythm-window-label { justify-self: start; }
    .metric-grid { grid-template-columns: repeat(2, minmax(100px, 1fr)); }
  }
  :global(.dark) .rhythm-panel {
    border-color: #344054;
    background: linear-gradient(135deg, #182230 0%, #101828 100%);
  }
  :global(.dark) .rhythm-heading strong,
  :global(.dark) .metric-card strong,
  :global(.dark) .rhythm-review-card > strong { color: #f9fafb; }
  :global(.dark) .rhythm-heading span,
  :global(.dark) .metric-card span,
  :global(.dark) .metric-card small,
  :global(.dark) .rhythm-detail,
  :global(.dark) .rhythm-empty { color: #d0d5dd; }
  :global(.dark) .metric-card,
  :global(.dark) .rhythm-review-card { border-color: #344054; background: #1d2939; color: #e4e7ec; }
  :global(.dark) .local-rhythm-card { border-color: #7a5b16; background: #332b18; color: #f5e7b8; }
  :global(.dark) .rhythm-detail p { color: #e4e7ec; }
  :global(.dark) .review-list { border-color: #344054; }
</style>
