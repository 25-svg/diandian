<script lang="ts">
  import type { PaymentEvent } from "../../orderDealTimeline";
  import {
    buildLiveProductCatalog,
    extractLiveTopProducts,
  } from "../../liveTopProducts";
  import type { WorkspaceTranscriptEntry } from "../../companyAnalysisWorkspace";

  export let events: PaymentEvent[] = [];
  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];
  /** True while deal gaps are still being transcribed into the shared SRT. */
  export let transcriptBackfilling = false;

  $: catalog = buildLiveProductCatalog(events);
  $: visibleProducts = extractLiveTopProducts(transcriptEntries, events).slice(0, 5);
  $: hasTranscript = transcriptEntries.length > 0;
  $: rankByOrders = visibleProducts.length > 0 && visibleProducts.every((product) => product.metric === "order");
  $: maxScore = Math.max(
    1,
    ...visibleProducts.map((product) => (product.metric === "order" ? product.orderCount : product.mentionCount)),
  );
</script>

<section class="top-products-panel" aria-label="整场高频商品 TOP5">
  <header>
    <div>
      <strong>整场高频商品 TOP5</strong>
      <span>{rankByOrders ? "按成交件数统计（文稿未匹配到型号）" : "按逐字稿提及次数统计"}</span>
    </div>
    {#if transcriptBackfilling}
      <small>文稿补转中</small>
    {:else if hasTranscript && visibleProducts.length && !rankByOrders}
      <small>统计完成</small>
    {:else if visibleProducts.length && rankByOrders}
      <small>按成交件数</small>
    {/if}
  </header>

  {#if !catalog.length}
    <p class="empty">本场没有可用于识别的订单商品</p>
  {:else if visibleProducts.length}
    <ol class="chart" aria-label={rankByOrders ? "成交件数条形图" : "提及次数条形图"}>
      {#each visibleProducts as product, index (product.id)}
        {@const score = product.metric === "order" ? product.orderCount : product.mentionCount}
        {@const unit = product.metric === "order" ? "笔" : "次"}
        <li>
          <span class="rank">{index + 1}</span>
          <div class="row">
            <div class="meta">
              <span class="name" title={product.name}>{product.name}</span>
              <strong>{score} {unit}</strong>
            </div>
            <div
              class="bar-track"
              role="img"
              aria-label={`${product.name} ${score} ${unit}`}
            >
              <div
                class="bar-fill"
                class:top={index === 0}
                style={`width: ${(score / maxScore) * 100}%`}
              ></div>
            </div>
          </div>
        </li>
      {/each}
    </ol>
    {#if transcriptBackfilling}
      <p class="hint">成交窗外文稿补转中，TOP5 会随文稿更新</p>
    {:else if rankByOrders && hasTranscript}
      <p class="hint">文稿未匹配到订单型号别名，已按本场成交件数展示</p>
    {/if}
  {:else if !hasTranscript && transcriptBackfilling}
    <p class="empty">等待成交窗/补转文稿…</p>
  {:else if !hasTranscript}
    <p class="empty">等待逐字稿生成…</p>
  {:else}
    <p class="empty">当前文稿暂未识别到商品提及{transcriptBackfilling ? "（补转中）" : ""}</p>
  {/if}
</section>

<style>
  .top-products-panel {
    min-width: 0;
    padding: 9px 12px;
    border: 1px solid #dbeafe;
    border-radius: 10px;
    background: #f8fbff;
  }
  header, header > div {
    display: flex;
    align-items: center;
  }
  header { justify-content: space-between; gap: 12px; }
  header > div { gap: 8px; min-width: 0; }
  header strong { color: #1d4ed8; font-size: 13px; white-space: nowrap; }
  header span, header small { color: #94a3b8; font-size: 11px; white-space: nowrap; }
  .chart {
    list-style: none;
    display: grid;
    gap: 7px;
    margin: 8px 0 0;
    padding: 0;
  }
  .chart li {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr);
    gap: 8px;
    align-items: center;
    min-width: 0;
  }
  .rank {
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    background: #dbeafe;
    color: #1d4ed8;
    font-size: 11px;
    font-weight: 700;
  }
  .row { min-width: 0; display: grid; gap: 4px; }
  .meta {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    min-width: 0;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #334155;
    font-size: 12px;
  }
  .meta strong {
    flex: 0 0 auto;
    color: #0f172a;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .bar-track {
    height: 8px;
    border-radius: 999px;
    background: #e8eef7;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #93c5fd 0%, #3b82f6 100%);
    min-width: 4px;
    transition: width 240ms ease;
  }
  .bar-fill.top {
    background: linear-gradient(90deg, #60a5fa 0%, #1d4ed8 100%);
  }
  .empty, .hint { margin: 6px 0 0; color: #94a3b8; font-size: 12px; }
  .hint { color: #64748b; }
  @media (max-width: 900px) {
    header span { display: none; }
  }
</style>
