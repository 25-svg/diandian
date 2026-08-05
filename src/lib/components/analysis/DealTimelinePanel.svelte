<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { buildDealMinuteBuckets, formatDealMoneyYuan, type PaymentEvent } from "../../orderDealTimeline";
  export let events: PaymentEvent[] = [];
  const dispatch = createEventDispatcher<{ seek: number }>();
  $: buckets = buildDealMinuteBuckets(events).filter((bucket) => bucket.orderCount > 0);
  function label(offsetSec: number): string { const minute = Math.floor(offsetSec / 60); return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`; }
</script>

<section class="deal-timeline" aria-label="成交时间轴">
  <header><strong>成交时间轴</strong><span>{events.length ? `${events.length} 笔订单` : "未导入订单"}</span></header>
  {#if buckets.length}
    <div class="timeline-list">{#each buckets as bucket}<button type="button" on:click={() => dispatch("seek", bucket.offsetSec)} title="跳到该成交点前两分钟"><time>{label(bucket.offsetSec)}</time><span>{bucket.orderCount} 单</span><strong>{formatDealMoneyYuan(bucket.totalPayAmountFen)}</strong></button>{/each}</div>
  {:else}
    <p>未导入成交订单。可在上方一键拉取或导入成交 JSON；无订单时不标记为已确认成交。</p>
  {/if}
</section>

<style>
  .deal-timeline {
    margin: 12px 0;
    border: 1px solid #dbe7df;
    border-radius: 10px;
    background: #f7fbf8;
    padding: 10px;
    min-width: 0;
    max-height: min(220px, 28vh);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-sizing: border-box;
  }
  header {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    color: #52606d;
    flex: 0 0 auto;
  }
  header strong, time { color: #166534; }
  .timeline-list {
    display: grid;
    gap: 6px;
    margin-top: 8px;
    min-height: 0;
    flex: 1 1 auto;
    overflow: auto;
  }
  button {
    display: grid;
    grid-template-columns: 48px 42px minmax(0, 1fr);
    align-items: center;
    gap: 7px;
    border: 0;
    border-radius: 7px;
    padding: 7px;
    text-align: left;
    background: white;
    cursor: pointer;
    color: #344054;
    font-size: 12px;
    min-width: 0;
  }
  button:hover { background: #e8f5ed; }
  button strong { text-align: right; color: #111827; }
  p { margin: 8px 0 0; color: #7a5d15; font-size: 12px; line-height: 1.5; }
</style>
