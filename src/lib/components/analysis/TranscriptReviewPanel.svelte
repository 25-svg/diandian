<script lang="ts">
  import { BookOpen, CheckCircle2, ChevronLeft, ChevronRight, Loader2 } from "lucide-svelte";
  import { createEventDispatcher } from "svelte";
  import {
    correctionProgress,
    firstPendingCorrectionId,
    isCurrentAnalysisRequest,
    resolveTranscriptCorrection,
    type AnalysisRequestIdentity,
    type ReviewAction,
    type TranscriptAuditBundle,
    type TranscriptCorrection,
    type TranscriptReviewUpdateEvent,
  } from "../../transcriptReview";
  import DictionaryCandidateDrawer from "./DictionaryCandidateDrawer.svelte";
  import TranscriptCorrectionCard from "./TranscriptCorrectionCard.svelte";

  export let bundle: TranscriptAuditBundle;
  export let selectedCorrectionId = "";
  export let requestIdentity: AnalysisRequestIdentity;
  export let reviewRequestToken: number;
  export let disabled = false;
  export let disabledReason = "";

  const dispatch = createEventDispatcher<{
    select: TranscriptCorrection;
    update: TranscriptReviewUpdateEvent;
    complete: TranscriptReviewUpdateEvent;
  }>();

  let submitting = false;
  let errorMessage = "";
  let drawerOpen = false;
  let sourceKey = "";
  let activeSubmissionKey = "";
  let submissionSequence = 0;

  $: currentSourceKey = JSON.stringify(bundle.source);
  $: currentSubmissionKey = JSON.stringify([requestIdentity, reviewRequestToken]);
  $: if (currentSubmissionKey !== activeSubmissionKey) {
    activeSubmissionKey = currentSubmissionKey;
    submissionSequence += 1;
    submitting = false;
    errorMessage = "";
  }
  $: if (currentSourceKey !== sourceKey) {
    sourceKey = currentSourceKey;
    selectedCorrectionId = firstPendingCorrectionId(bundle.corrections) || bundle.corrections[0]?.id || "";
  }
  $: progress = correctionProgress(bundle.corrections);
  $: currentIndex = Math.max(0, bundle.corrections.findIndex((item) => item.id === selectedCorrectionId));
  $: currentCorrection = bundle.corrections[currentIndex] || null;

  function selectIndex(index: number): void {
    const correction = bundle.corrections[index];
    if (!correction) return;
    selectedCorrectionId = correction.id;
    errorMessage = "";
    dispatch("select", correction);
  }

  async function decide(event: CustomEvent<{
    action: ReviewAction;
    decidedText: string | null;
    addToDictionaryCandidates: boolean;
  }>): Promise<void> {
    if (!currentCorrection || submitting || disabled) return;
    const submissionIdentity = requestIdentity;
    const submissionToken = reviewRequestToken;
    const submissionId = ++submissionSequence;
    const submittedSource = bundle.source;
    const submittedCorrectionId = currentCorrection.id;
    submitting = true;
    errorMessage = "";
    try {
      const updated = await resolveTranscriptCorrection({
        source: submittedSource,
        correctionId: submittedCorrectionId,
        ...event.detail,
      });
      if (
        submissionToken !== reviewRequestToken
        || !isCurrentAnalysisRequest(
          submissionIdentity,
          requestIdentity,
          submissionId,
          submissionSequence,
        )
      ) return;
      const updateEvent = {
        bundle: updated,
        requestIdentity: submissionIdentity,
        reviewRequestToken: submissionToken,
      };
      dispatch("update", updateEvent);
      if (updated.pendingCriticalCount === 0) {
        dispatch("complete", updateEvent);
        return;
      }
      const nextId = firstPendingCorrectionId(updated.corrections);
      const nextCorrection = updated.corrections.find((item) => item.id === nextId);
      if (nextCorrection) {
        selectedCorrectionId = nextCorrection.id;
        errorMessage = "";
        dispatch("select", nextCorrection);
      }
    } catch (error: any) {
      if (
        submissionToken !== reviewRequestToken
        || !isCurrentAnalysisRequest(
          submissionIdentity,
          requestIdentity,
          submissionId,
          submissionSequence,
        )
      ) return;
      errorMessage = error?.message || String(error);
    } finally {
      if (
        submissionToken === reviewRequestToken
        && isCurrentAnalysisRequest(
          submissionIdentity,
          requestIdentity,
          submissionId,
          submissionSequence,
        )
      ) submitting = false;
    }
  }
</script>

<div class="review-panel">
  <div class="review-toolbar">
    <div>
      <strong>已处理 {progress.completed}/{progress.total}</strong>
      <span>{bundle.pendingCriticalCount > 0 ? `还需确认 ${bundle.pendingCriticalCount} 条关键信息` : "关键项已确认"}</span>
    </div>
    <div class="toolbar-actions">
      <button title="上一条" aria-label="上一条" disabled={currentIndex <= 0 || submitting} on:click={() => selectIndex(currentIndex - 1)}><ChevronLeft size={17} /></button>
      <span>{progress.total ? currentIndex + 1 : 0}/{progress.total}</span>
      <button title="下一条" aria-label="下一条" disabled={currentIndex >= progress.total - 1 || submitting} on:click={() => selectIndex(currentIndex + 1)}><ChevronRight size={17} /></button>
      <button class="dictionary-button" title="打开词库候选" on:click={() => drawerOpen = true}><BookOpen size={15} /><span>词库</span></button>
    </div>
  </div>

  <div class="panel-body">
    {#if submitting}
      <div class="submitting"><Loader2 size={16} />正在保存你的选择…</div>
    {:else if disabled && disabledReason}
      <div class="read-only-reason">{disabledReason}</div>
    {/if}
    {#if currentCorrection}
      <TranscriptCorrectionCard
        correction={currentCorrection}
        sourceKey={currentSourceKey}
        {submitting}
        {disabled}
        {disabledReason}
        {errorMessage}
        on:decide={decide}
      />
    {:else if bundle.corrections.length === 0}
      <div class="state success"><CheckCircle2 size={30} /><strong>没有需要人工确认的内容</strong><span>可以切换到“片段分析”查看结果。</span></div>
    {:else}
      <div class="state"><span>当前校稿项不存在，请选择上一条或下一条。</span></div>
    {/if}
  </div>
</div>

{#if drawerOpen}<DictionaryCandidateDrawer on:close={() => drawerOpen = false} />{/if}

<style>
  .review-panel { height: 100%; min-height: 0; display: flex; flex-direction: column; }
  .review-toolbar { min-height: 48px; flex: 0 0 auto; display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 7px 11px; border-bottom: 1px solid #eaecf0; }
  .review-toolbar > div:first-child { min-width: 0; display: grid; gap: 2px; }
  .review-toolbar strong { color: #344054; font-size: 11px; }
  .review-toolbar span { color: #667085; font-size: 9px; }
  .toolbar-actions { display: flex; align-items: center; gap: 4px; }
  .toolbar-actions button { width: 30px; height: 30px; display: inline-flex; align-items: center; justify-content: center; border: 1px solid #d8dde6; border-radius: 6px; color: #344054; background: white; cursor: pointer; }
  .toolbar-actions button:disabled { cursor: not-allowed; opacity: .4; }
  .toolbar-actions .dictionary-button { width: auto; gap: 5px; padding: 0 8px; }
  .toolbar-actions .dictionary-button span { color: inherit; font-size: 10px; }
  .toolbar-actions > span { min-width: 30px; text-align: center; font-variant-numeric: tabular-nums; }
  .panel-body { position: relative; min-height: 0; flex: 1; overflow: auto; }
  .submitting { position: sticky; top: 0; z-index: 2; display: flex; align-items: center; justify-content: center; gap: 6px; padding: 7px; color: #175cd3; background: #eff8ff; font-size: 10px; }
  .submitting :global(svg) { animation: spin 1s linear infinite; }
  .read-only-reason { position: sticky; top: 0; z-index: 2; padding: 8px 10px; color: #175cd3; background: #eff8ff; text-align: center; font-size: 10px; }
  .state { min-height: 280px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 20px; color: #667085; text-align: center; font-size: 11px; }
  .state.success :global(svg), .state.success strong { color: #087c42; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.dark) .review-toolbar { border-color: #3d4148; }
  :global(.dark) .review-toolbar strong { color: #f2f4f7; }
  :global(.dark) .toolbar-actions button { color: #f2f4f7; border-color: #4b5058; background: #303238; }
</style>
