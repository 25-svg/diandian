<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { RotateCcw, Search } from "lucide-svelte";

  export let dateFrom = "";
  export let dateTo = "";
  export let sessionQuery = "";
  export let productQuery = "";
  export let liveStatus: "all" | "recording" | "completed" = "all";
  export let analysisStatus: "all" | "not_started" | "in_progress" | "complete" = "all";

  const dispatch = createEventDispatcher<{ change: void; clear: void }>();

  function changed(): void {
    dispatch("change");
  }

  function clear(): void {
    dateFrom = "";
    dateTo = "";
    sessionQuery = "";
    productQuery = "";
    liveStatus = "all";
    analysisStatus = "all";
    dispatch("clear");
    dispatch("change");
  }
</script>

<section class="filter-panel" aria-label="主播直播筛选">
  <div class="filter-heading">
    <div>
      <Search size={16} aria-hidden="true" />
      <strong>筛选该主播的直播</strong>
    </div>
    <button type="button" class="clear-button" on:click={clear}>
      <RotateCcw size={14} aria-hidden="true" />
      清空筛选
    </button>
  </div>

  <div class="filter-grid">
    <label>
      <span>开始日期</span>
      <input type="date" bind:value={dateFrom} on:change={changed} aria-label="开始日期" />
    </label>
    <label>
      <span>结束日期</span>
      <input type="date" bind:value={dateTo} on:change={changed} aria-label="结束日期" />
    </label>
    <label>
      <span>场次</span>
      <input
        type="search"
        bind:value={sessionQuery}
        on:input={changed}
        aria-label="场次筛选"
        placeholder="标题、场次号、直播间"
      />
    </label>
    <label>
      <span>商品</span>
      <input
        type="search"
        bind:value={productQuery}
        on:input={changed}
        aria-label="商品筛选"
        placeholder="例如 S9、G9"
      />
    </label>
    <label>
      <span>直播状态</span>
      <select bind:value={liveStatus} on:change={changed} aria-label="直播状态筛选">
        <option value="all">全部状态</option>
        <option value="recording">正在直播</option>
        <option value="completed">已结束</option>
      </select>
    </label>
    <label>
      <span>分析状态</span>
      <select bind:value={analysisStatus} on:change={changed} aria-label="分析状态筛选">
        <option value="all">全部状态</option>
        <option value="not_started">未分析</option>
        <option value="in_progress">分析中</option>
        <option value="complete">已完成</option>
      </select>
    </label>
  </div>
</section>

<style>
  .filter-panel {
    display: grid;
    gap: 12px;
    min-width: 0;
    padding: 14px;
    border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-lg);
    background: var(--mac-bg-elevated);
  }
  .filter-heading,
  .filter-heading > div,
  .clear-button { display: flex; align-items: center; }
  .filter-heading { justify-content: space-between; gap: 12px; }
  .filter-heading > div { gap: 7px; color: var(--mac-label); }
  .filter-heading strong { font-size: 12px; }
  .filter-heading :global(svg) { color: var(--mac-blue); }
  .clear-button {
    min-height: 34px;
    gap: 6px;
    padding: 0 10px;
    border: 1px solid var(--mac-separator);
    border-radius: 9px;
    color: var(--mac-secondary);
    background: var(--mac-fill);
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .clear-button:hover { color: var(--mac-label); background: var(--mac-fill-hover); }
  .clear-button:focus-visible,
  input:focus-visible,
  select:focus-visible { outline: 3px solid var(--mac-blue); outline-offset: 2px; border-color: var(--mac-blue); }
  .filter-grid { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 9px; }
  label { display: grid; min-width: 0; gap: 5px; }
  label > span { color: var(--mac-secondary); font-size: 10px; font-weight: 650; }
  input,
  select {
    width: 100%;
    min-width: 0;
    min-height: 38px;
    padding: 0 10px;
    border: 1px solid var(--mac-separator-strong);
    border-radius: 9px;
    color: var(--mac-label);
    background: var(--mac-bg-card);
    font: inherit;
    font-size: 12px;
  }
  input::placeholder { color: var(--mac-tertiary); }
  @media (max-width: 1180px) {
    .filter-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  }
  @media (max-width: 720px) {
    .filter-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
  @media (max-width: 520px) {
    .filter-heading { align-items: stretch; flex-direction: column; }
    .clear-button { justify-content: center; width: 100%; }
    .filter-grid { grid-template-columns: 1fr; }
  }
</style>
