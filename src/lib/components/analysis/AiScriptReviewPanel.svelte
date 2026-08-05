<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Sparkles, AlertCircle } from "lucide-svelte";
  import { formatWorkspaceClock, type WorkspaceTranscriptEntry } from "../../companyAnalysisWorkspace";
  import {
    scriptIssueKindLabel,
    type ScriptIssueAnnotation,
    type ScriptIssueKind,
  } from "../../scriptQuality";

  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  export let transcriptReady = false;
  export let analyzing = false;
  export let analyzeError = "";
  export let qualitySummary = "";
  export let annotations: ScriptIssueAnnotation[] = [];
  export let selectedCueId: number | null = null;

  const dispatch = createEventDispatcher<{ seek: number; analyze: void; select: number }>();

  $: annotationMap = new Map(annotations.map((item) => [item.cueId, item]));
  $: issueCount = annotations.length;
  $: selectedAnnotation = selectedCueId == null ? null : annotationMap.get(selectedCueId) ?? null;

  function kindClass(kind: ScriptIssueKind): string {
    return `kind-${kind.replace(/_/g, "-")}`;
  }

  function handleRowClick(entry: WorkspaceTranscriptEntry): void {
    dispatch("seek", entry.start);
    if (annotationMap.has(entry.id)) {
      dispatch("select", entry.id);
    }
  }
</script>

<section class="ai-review-panel" aria-label="话术复盘">
  <div class="legend" aria-label="改进类型图例">
    <span><i class="dot structure"></i>讲法顺序</span>
    <span><i class="dot unclear"></i>表达可优化</span>
    <span><i class="dot factual"></i>信息不准确</span>
    <span><i class="dot compliance"></i>承诺过重</span>
  </div>

  <p class="scope-line">
    整场逐字稿 · 共 {transcriptEntries.length} 句
    {#if issueCount} · 可改进 {issueCount} 处{/if}
    · 点击句子查看建议并跳转视频
  </p>

  {#if qualitySummary}
    <div class="quality-summary">{qualitySummary}</div>
  {/if}

  {#if analyzeError}
    <div class="analyze-error">{analyzeError}</div>
  {/if}

  {#if selectedAnnotation}
    <aside class="issue-detail" aria-label="改法建议">
      <strong>{scriptIssueKindLabel(selectedAnnotation.kind)}</strong>
      <p><span class="detail-label">为什么不够好</span>{selectedAnnotation.reason}</p>
      <p class="suggestion"><span class="detail-label">下次可以怎么说</span>{selectedAnnotation.suggestion}</p>
    </aside>
  {/if}

  <div class="transcript-board">
    {#if transcriptEntries.length}
      {#each transcriptEntries as entry (entry.id)}
        {@const issue = annotationMap.get(entry.id)}
        <button
          type="button"
          class="transcript-row"
          class:has-issue={Boolean(issue)}
          class:selected={selectedCueId === entry.id}
          class:kind-structure-gap={issue?.kind === "structure_gap"}
          class:kind-unclear-expression={issue?.kind === "unclear_expression"}
          class:kind-factual-risk={issue?.kind === "factual_risk"}
          class:kind-compliance-risk={issue?.kind === "compliance_risk"}
          on:click={() => handleRowClick(entry)}
        >
          <time>{formatWorkspaceClock(entry.start)}</time>
          <p>{entry.text}</p>
        </button>
      {/each}
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

  <footer class="panel-foot">
    <button
      type="button"
      class="primary"
      disabled={!transcriptEntries.length || analyzing}
      on:click={() => dispatch("analyze")}
    >
      <Sparkles size={15} />
      {analyzing ? "复盘中…" : issueCount ? "重新话术复盘" : "开始话术复盘"}
    </button>
    <button type="button" class="secondary" disabled={!issueCount || analyzing}>
      导出复盘摘要（下一步）
    </button>
  </footer>
</section>

<style>
  .ai-review-panel { display: grid; grid-template-rows: auto auto auto auto minmax(0, 1fr) auto; gap: 10px; min-height: 0; height: 100%; }
  .legend { display: flex; flex-wrap: wrap; gap: 10px; font-size: 11px; color: #475467; }
  .legend span { display: inline-flex; align-items: center; gap: 5px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
  .dot.structure { background: #f04438; }
  .dot.unclear { background: #f79009; }
  .dot.factual { background: #eab308; }
  .dot.compliance { background: #7a5af8; }
  .scope-line { margin: 0; font-size: 11px; color: #667085; }
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
    background: #fcfcfd;
    font-size: 12px;
    color: #344054;
  }
  .issue-detail strong { display: block; margin-bottom: 6px; color: #101828; }
  .issue-detail .detail-label { display: block; margin-bottom: 2px; font-size: 10px; font-weight: 600; color: #667085; }
  .issue-detail p { margin: 0 0 8px; line-height: 1.5; }
  .issue-detail .suggestion { color: #027a48; }
  .issue-detail small { color: #667085; }
  .transcript-board { min-height: 0; overflow: auto; border: 1px solid #e4e7ec; border-radius: 12px; background: white; padding: 4px 6px; }
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
  .transcript-row.has-issue { border-left: 3px solid transparent; padding-left: 1px; }
  .transcript-row.kind-structure-gap { border-left-color: #f04438; background: #fff5f5; }
  .transcript-row.kind-unclear-expression { border-left-color: #f79009; background: #fffaeb; }
  .transcript-row.kind-factual-risk { border-left-color: #eab308; background: #fefce8; }
  .transcript-row.kind-compliance-risk { border-left-color: #7a5af8; background: #f4f3ff; }
  .transcript-row time { color: #667085; font-size: 11px; font-variant-numeric: tabular-nums; }
  .transcript-row p { margin: 0; font-size: 13px; line-height: 1.55; color: #1f2937; }
  .panel-foot { display: flex; gap: 8px; }
  .primary, .secondary { display: inline-flex; align-items: center; gap: 6px; height: 34px; padding: 0 12px; border-radius: 8px; font-size: 12px; cursor: pointer; }
  .primary { border: 1px solid #175cd3; background: #175cd3; color: white; }
  .secondary { border: 1px solid #d0d5dd; background: white; color: #344054; }
  .primary:disabled, .secondary:disabled { opacity: 0.45; cursor: not-allowed; }
  .empty-box { display: grid; justify-items: center; gap: 8px; padding: 28px; color: #667085; font-size: 13px; text-align: center; }
</style>
