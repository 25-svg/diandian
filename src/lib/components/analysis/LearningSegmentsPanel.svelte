<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { BookmarkPlus, Download, Play, Trash2 } from "lucide-svelte";
  import {
    addLearningSegment,
    createLearningSegment,
    formatLearningSegmentTime,
    removeLearningSegment,
    type LearningSegment,
    type LearningSpeechSource,
  } from "../../learningSegments";

  export let sourceKey = "";
  export let speech: LearningSpeechSource | null = null;
  export let segments: LearningSegment[] = [];
  export let disabled = false;
  export let exportingSegmentId = "";
  export let preBufferSec: number | undefined = undefined;
  export let postBufferSec: number | undefined = undefined;

  const dispatch = createEventDispatcher<{
    change: LearningSegment[];
    seek: number;
    export: LearningSegment;
  }>();

  $: pendingSegment = speech && sourceKey
    ? createLearningSegment(speech, { sourceKey, preBufferSec, postBufferSec })
    : null;
  $: alreadyAdded = pendingSegment ? segments.some((segment) => segment.id === pendingSegment.id) : false;

  function add(): void {
    if (!pendingSegment || disabled) return;
    dispatch("change", addLearningSegment(segments, pendingSegment));
  }

  function remove(segmentId: string): void {
    if (disabled) return;
    dispatch("change", removeLearningSegment(segments, segmentId));
  }
</script>

<section class="learning-segments" aria-label="学习片段">
  <header>
    <div>
      <strong>学习片段</strong>
      <p>基于 AI 精炼后的成交链路；导出 MP4 时再使用这些范围。</p>
    </div>
    <span class="count">{segments.length} 段</span>
  </header>

  {#if pendingSegment}
    <div class="add-row">
      <div>
        <span>{formatLearningSegmentTime(pendingSegment.startSec)} — {formatLearningSegmentTime(pendingSegment.endSec)}</span>
        <p>{pendingSegment.text}</p>
      </div>
      <button type="button" class="add" disabled={disabled || alreadyAdded} on:click={add}>
        <BookmarkPlus size={15} />{alreadyAdded ? "已加入" : "加入学习片段"}
      </button>
    </div>
  {:else}
    <p class="hint">选择一条成交话术后，可加入学习片段。</p>
  {/if}

  {#if segments.length}
    <ul>
      {#each segments as segment (segment.id)}
        <li>
          <button type="button" class="segment-copy" on:click={() => dispatch("seek", segment.startSec)}>
            <span>{formatLearningSegmentTime(segment.startSec)} — {formatLearningSegmentTime(segment.endSec)}</span>
            <strong>{segment.productName || "成交话术"}</strong>
            <p>{segment.text}</p>
          </button>
          <div class="actions">
            <button type="button" title="Export MP4" disabled={disabled || exportingSegmentId === segment.id} on:click={() => dispatch("export", segment)}><Download size={15} /></button>
            <button type="button" title="预览此片段" on:click={() => dispatch("seek", segment.startSec)}><Play size={15} /></button>
            <button type="button" class="delete" title="移除学习片段" disabled={disabled} on:click={() => remove(segment.id)}><Trash2 size={15} /></button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .learning-segments { display: grid; gap: 10px; border: 1px solid #dbe7ff; border-radius: 10px; padding: 12px; background: #f8fbff; }
  header { display: flex; justify-content: space-between; gap: 12px; }
  header strong { color: #175cd3; font-size: 13px; } header p, .hint { margin: 4px 0 0; color: #667085; font-size: 11px; line-height: 1.45; }
  .count { align-self: start; padding: 3px 7px; border-radius: 999px; background: #d1e9ff; color: #175cd3; font-size: 11px; }
  .add-row { display: flex; justify-content: space-between; align-items: flex-start; gap: 10px; padding: 9px; border: 1px solid #b2ddff; border-radius: 8px; background: white; min-width: 0; }
  .add-row > div { min-width: 0; flex: 1 1 auto; }
  .add-row span, .segment-copy span { color: #175cd3; font-size: 11px; font-variant-numeric: tabular-nums; }
  /* Preview only — full text lives in the explanation window above. */
  .add-row p, .segment-copy p {
    margin: 4px 0 0;
    color: #344054;
    font-size: 12px;
    line-height: 1.45;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    overflow: hidden;
  }
  button { font: inherit; cursor: pointer; } button:disabled { cursor: not-allowed; opacity: .5; }
  .add { flex: none; display: inline-flex; align-items: center; gap: 5px; border: 1px solid #175cd3; border-radius: 7px; padding: 7px 9px; background: #175cd3; color: white; font-size: 11px; font-weight: 650; }
  ul { display: grid; gap: 7px; margin: 0; padding: 0; list-style: none; } li { display: flex; gap: 6px; border: 1px solid #e4e7ec; border-radius: 8px; background: white; overflow: hidden; }
  .segment-copy { flex: 1; min-width: 0; border: 0; padding: 9px; background: transparent; text-align: left; } .segment-copy:hover { background: #eff8ff; } .segment-copy strong { display: block; margin-top: 3px; color: #1d2939; font-size: 12px; }
  .actions { display: flex; align-items: center; gap: 2px; padding-right: 6px; } .actions button { display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 6px; color: #475467; background: transparent; } .actions button:hover { background: #eff8ff; color: #175cd3; } .actions .delete:hover { background: #fef3f2; color: #b42318; }
</style>
