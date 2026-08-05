<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { WorkspaceTranscriptEntry } from "../../companyAnalysisWorkspace";
  import {
    extractLiveHighFrequencyHits,
    filterHighFrequencyHits,
    formatHighFrequencySampleTime,
    paginateHighFrequencyHits,
    summarizeHighFrequencyCategories,
    type HighFrequencyCategory,
    type HighFrequencyHit,
  } from "../../liveHighFrequencyWords";

  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  export let transcriptReady = false;
  export let compactHeader = false;

  const PAGE_SIZE = 8;
  const dispatch = createEventDispatcher<{ seek: number }>();

  let activeCategory: HighFrequencyCategory | "全部" = "全部";
  let page = 1;
  let lastHitSignature = "";

  $: hits = extractLiveHighFrequencyHits(transcriptEntries);
  $: hitSignature = hits.map((hit) => hit.id).join("|");
  $: if (hitSignature !== lastHitSignature) {
    lastHitSignature = hitSignature;
    page = 1;
  }
  $: summaries = summarizeHighFrequencyCategories(hits).filter(
    (item) => item.category === "全部" || item.count > 0,
  );
  $: filtered = filterHighFrequencyHits(hits, activeCategory);
  $: paged = paginateHighFrequencyHits(filtered, page, PAGE_SIZE);
  $: maxHits = Math.max(1, ...filtered.map((hit) => hit.hitCount));

  function selectCategory(category: HighFrequencyCategory | "全部"): void {
    activeCategory = category;
    page = 1;
  }

  function viewSentence(hit: HighFrequencyHit): void {
    dispatch("seek", hit.sampleStartSec);
  }

  function goToPage(next: number): void {
    page = Math.min(Math.max(1, next), paged.pageCount);
  }

  function barWidth(count: number): number {
    return Math.max(4, Math.round((count / maxHits) * 100));
  }
</script>

<section class="hf-panel" aria-label="直播高频词">
  {#if !compactHeader}
    <header class="hf-head">
      <strong>高频词</strong>
      <span class="meta">{hits.length} 个</span>
    </header>
  {/if}

  {#if summaries.length > 1}
    <div class="hf-filters" aria-label="分类筛选">
      {#each summaries as summary (summary.category)}
        <button
          type="button"
          class="filter"
          class:active={activeCategory === summary.category}
          on:click={() => selectCategory(summary.category)}
        >
          {summary.category === "全部" ? "全部" : summary.category}
          <em>{summary.count}</em>
        </button>
      {/each}
    </div>
  {/if}

  {#if !transcriptReady}
    <p class="empty">逐字稿生成中…</p>
  {:else if !hits.length}
    <p class="empty">暂无命中词</p>
  {:else}
    <ul class="word-list">
      {#each paged.pageItems as hit (hit.id)}
        <li>
          <button type="button" class="row" on:click={() => viewSentence(hit)} title={hit.sampleText}>
            <div class="row-main">
              <span class="word">{hit.word}</span>
              <span class="cat">{hit.category}</span>
              <span class="time">{formatHighFrequencySampleTime(hit.sampleStartSec)}</span>
              <span class="num">{hit.hitCount}</span>
            </div>
            <div class="bar-track" aria-hidden="true">
              <div class="bar-fill" style={`width:${barWidth(hit.hitCount)}%`}></div>
            </div>
          </button>
        </li>
      {/each}
    </ul>

    {#if paged.pageCount > 1}
      <footer class="pager">
        <span>{paged.page}/{paged.pageCount}</span>
        <button type="button" disabled={paged.page <= 1} on:click={() => goToPage(paged.page - 1)}>上一页</button>
        <button type="button" disabled={paged.page >= paged.pageCount} on:click={() => goToPage(paged.page + 1)}>下一页</button>
      </footer>
    {/if}
  {/if}
</section>

<style>
  .hf-panel {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow: hidden;
  }
  .hf-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .hf-head strong {
    font-size: 13px;
    font-weight: 600;
    color: #111827;
  }
  .meta { font-size: 12px; color: #9ca3af; }
  .hf-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .filter {
    border: 1px solid #e5e7eb;
    border-radius: 999px;
    background: #fff;
    color: #6b7280;
    font-size: 12px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .filter em {
    font-style: normal;
    margin-left: 4px;
    color: #9ca3af;
  }
  .filter.active {
    border-color: #111827;
    background: #111827;
    color: #fff;
  }
  .filter.active em { color: #d1d5db; }
  .word-list {
    list-style: none;
    margin: 0;
    padding: 0;
    min-height: 0;
    overflow: auto;
  }
  .row {
    width: 100%;
    display: grid;
    gap: 5px;
    padding: 8px 0;
    border: 0;
    border-bottom: 1px solid #f3f4f6;
    background: transparent;
    text-align: left;
    cursor: pointer;
    font: inherit;
    color: #374151;
  }
  .row:hover .word { color: #000; }
  .row-main {
    display: grid;
    grid-template-columns: minmax(72px, 1.1fr) minmax(64px, 1fr) 52px 40px;
    gap: 8px;
    align-items: center;
  }
  .word { font-weight: 650; color: #111827; }
  .cat, .time { color: #9ca3af; font-size: 12px; }
  .num {
    font-variant-numeric: tabular-nums;
    color: #111827;
    font-size: 12px;
    font-weight: 600;
    text-align: right;
  }
  .bar-track {
    height: 4px;
    border-radius: 999px;
    background: #f3f4f6;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    border-radius: 999px;
    background: #111827;
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 10px;
    color: #9ca3af;
    font-size: 12px;
  }
  .pager button {
    border: 0;
    background: transparent;
    color: #6b7280;
    font-size: 12px;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .pager button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
    text-decoration: none;
  }
  .empty {
    margin: 0;
    padding: 10px 0;
    color: #9ca3af;
    font-size: 12px;
  }
</style>
