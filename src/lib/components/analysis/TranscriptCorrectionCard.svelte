<script lang="ts">
  import { Check, RotateCcw } from "lucide-svelte";
  import { createEventDispatcher } from "svelte";
  import {
    correctionBusinessReason,
    correctionCategoryLabel,
    correctionEditIdentity,
    isUnresolvedTranscriptPlaceholder,
    reviewStatusLabel,
    type ReviewAction,
    type TranscriptCorrection,
  } from "../../transcriptReview";

  export let correction: TranscriptCorrection;
  export let sourceKey: string;
  export let submitting = false;
  export let disabled = false;
  export let disabledReason = "";
  export let errorMessage = "";

  const dispatch = createEventDispatcher<{
    decide: {
      action: ReviewAction;
      decidedText: string | null;
      addToDictionaryCandidates: boolean;
    };
  }>();

  let loadedEditIdentity = "";
  let editedText = "";
  let addToDictionary = false;

  $: editIdentity = correctionEditIdentity(sourceKey, correction);
  $: unresolvedPlaceholder = isUnresolvedTranscriptPlaceholder(editedText);
  $: if (editIdentity !== loadedEditIdentity) {
    loadedEditIdentity = editIdentity;
    editedText = correction.decidedText || correction.proposed;
    addToDictionary = false;
  }

  function formatTime(milliseconds: number): string {
    const seconds = Math.max(0, Math.floor(milliseconds / 1000));
    const minutes = Math.floor(seconds / 60);
    const remainder = seconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`;
  }

  function decide(action: ReviewAction): void {
    if (submitting || disabled) return;
    dispatch("decide", {
      action,
      decidedText: action === "approve" ? editedText.trim() : null,
      addToDictionaryCandidates: action === "approve" && addToDictionary,
    });
  }
</script>

<article class="correction-sheet" aria-label="当前校稿项">
  <div class="correction-meta">
    <span>{formatTime(correction.startMs)}—{formatTime(correction.endMs)}</span>
    <span>{correctionCategoryLabel(correction.category)}</span>
    <strong class:pending={correction.decision === "pending"}>{reviewStatusLabel(correction.decision)}</strong>
  </div>

  <section>
    <span class="field-label">听到的内容</span>
    <p class="original-text">{correction.original}</p>
  </section>

  <section>
    <label for={`correction-${correction.id}`}>
      {correction.decision === "pending"
        ? "建议改成"
        : correction.decision === "approved"
          ? "已采用的最终文本"
          : "已保留的原文"}
    </label>
    {#if correction.decision === "pending"}
      <textarea
        id={`correction-${correction.id}`}
        bind:value={editedText}
        disabled={submitting || disabled}
        rows="3"
      />
    {:else}
      <p class="resolved-text">{correction.decidedText || correction.original}</p>
    {/if}
  </section>

  <p class="business-reason">{correctionBusinessReason(correction.category)}</p>

  {#if correction.decision === "pending" && unresolvedPlaceholder}
    <p class="placeholder-guidance">这条内容还没有确认。请回听视频后填写真实内容，或选择“保留原文”。</p>
  {/if}

  {#if correction.decision === "pending"}
    {#if disabled && disabledReason}<p class="busy-reason">{disabledReason}</p>{/if}
    <label class="dictionary-option">
      <input type="checkbox" bind:checked={addToDictionary} disabled={submitting || disabled} />
      <span>加入词库候选</span>
    </label>

    <div class="decision-actions">
      <button class="secondary" disabled={submitting || disabled} on:click={() => decide("keep_original")}>
        <RotateCcw size={15} />保留原文
      </button>
      <button class="primary" disabled={submitting || disabled || !editedText.trim() || unresolvedPlaceholder} on:click={() => decide("approve")}>
        <Check size={15} />采用修改
      </button>
    </div>
  {:else}
    <div class="resolved-line">
      <Check size={15} />
      <span>这条已经处理并锁定，不会重复提交修改。</span>
    </div>
    <p class="resolved-guidance">如需改动，请重新打开校对并重新分析关联片段，避免逐字稿与片段结论不一致。</p>
  {/if}

  {#if errorMessage}<p class="inline-error" role="alert">{errorMessage}</p>{/if}

  <details>
    <summary>查看识别依据</summary>
    {#if correction.evidence.length}
      <ul>{#each correction.evidence as item}<li>{item}</li>{/each}</ul>
    {:else}
      <p>当前没有额外识别依据，请以视频原声为准。</p>
    {/if}
  </details>
</article>

<style>
  .correction-sheet { display: grid; gap: 13px; padding: 15px; color: #253044; }
  .correction-meta { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; color: #667085; font-size: 10px; }
  .correction-meta span { padding: 3px 6px; border-radius: 5px; background: #f0f3f7; }
  .correction-meta strong { margin-left: auto; color: #087c42; font-size: 10px; }
  .correction-meta strong.pending { color: #a34b00; }
  section { display: grid; gap: 6px; }
  section label, .field-label { color: #475467; font-size: 11px; font-weight: 650; }
  .original-text { margin: 0; padding: 10px 11px; border-left: 3px solid #98a2b3; background: #f7f8fa; font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; }
  textarea { width: 100%; box-sizing: border-box; resize: vertical; min-height: 76px; padding: 9px 10px; border: 1px solid #cfd6e1; border-radius: 7px; color: #1d2939; background: white; font: inherit; font-size: 13px; line-height: 1.55; outline: none; }
  textarea:focus { border-color: #1687f8; box-shadow: 0 0 0 3px rgba(22,135,248,.12); }
  .resolved-text { min-height: 42px; margin: 0; padding: 10px 11px; border: 1px solid #b7dfc5; border-radius: 7px; color: #17663a; background: #f0faf3; font-size: 13px; line-height: 1.55; overflow-wrap: anywhere; }
  .business-reason { margin: 0; padding: 9px 10px; border-left: 3px solid #f79009; color: #5d4328; background: #fff8eb; font-size: 11px; line-height: 1.55; }
  .placeholder-guidance { margin: 0; padding: 9px 10px; border-left: 3px solid #d92d20; color: #912018; background: #fef3f2; font-size: 11px; line-height: 1.55; }
  .busy-reason { margin: 0; padding: 9px 10px; color: #175cd3; background: #eff8ff; font-size: 11px; line-height: 1.5; }
  .dictionary-option { display: inline-flex; align-items: center; gap: 7px; color: #475467; font-size: 11px; cursor: pointer; }
  .dictionary-option input { width: 15px; height: 15px; accent-color: #0071e3; }
  .decision-actions { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  button { height: 36px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; border-radius: 7px; font: inherit; font-size: 11px; font-weight: 650; cursor: pointer; }
  button:disabled { cursor: not-allowed; opacity: .48; }
  .secondary { border: 1px solid #d5dae3; color: #344054; background: white; }
  .primary { border: 1px solid #0071e3; color: white; background: #0071e3; }
  .resolved-line { display: flex; align-items: center; gap: 7px; padding: 9px 10px; color: #087c42; background: #edf9f2; font-size: 11px; }
  .resolved-guidance { margin: 0; color: #667085; font-size: 10px; line-height: 1.5; }
  .inline-error { margin: 0; padding: 8px 10px; color: #b42318; background: #fef3f2; font-size: 11px; line-height: 1.5; }
  details { padding-top: 10px; border-top: 1px solid #eaecf0; color: #667085; font-size: 10px; }
  summary { cursor: pointer; font-weight: 650; }
  details ul { display: grid; gap: 5px; margin: 9px 0 0; padding-left: 18px; line-height: 1.5; }
  details p { margin: 9px 0 0; line-height: 1.5; }
  :global(.dark) .correction-sheet { color: #f2f4f7; }
  :global(.dark) .original-text, :global(.dark) .correction-meta span { background: #303238; }
  :global(.dark) textarea, :global(.dark) .secondary { color: #f2f4f7; border-color: #4b5058; background: #292b30; }
  :global(.dark) .resolved-text { border-color: #316b47; color: #c7f9d4; background: #173a25; }
</style>
