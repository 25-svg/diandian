<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Loader2 } from "lucide-svelte";
  import {
    dashboardVisualGroups,
    dashboardCandidateLabel,
    formatDashboardSessionTime,
    type LiveDataBoardCandidate,
    type LiveDataBoardOrderSummary,
    type LiveDataBoardSession,
  } from "../../liveDashboard";

  export let loading = false;
  export let error = "";
  export let session: LiveDataBoardSession | null = null;
  export let matchMethod: string | null = null;
  export let candidates: LiveDataBoardCandidate[] = [];
  export let selectedSessionId = "";
  export let orderSummary: LiveDataBoardOrderSummary | null = null;
  export let showDashboardBind = true;
  /** When embedded under a parent tab header, hide the local title row. */
  export let compactHeader = false;

  const dispatch = createEventDispatcher<{
    openDashboard: void;
    bindSession: void;
    rebindSession: void;
  }>();

  $: visual = session ? dashboardVisualGroups(session) : null;
  $: matchLabel = matchMethod === "manual" ? "人工绑定" : matchMethod ? "自动匹配" : "";
</script>

<section class="live-data-board" aria-label="数据面板">
  {#if !compactHeader}
    <header class="board-head">
      <div class="board-title">
        <strong>数据</strong>
        {#if session}
          <span class="meta">
            {session.shopName || "已绑定"}
            · {formatDashboardSessionTime(session.startedAt)}
            {#if matchLabel} · {matchLabel}{/if}
          </span>
        {:else}
          <span class="meta">整场 KPI · 手动绑定直播大屏</span>
        {/if}
      </div>
      <div class="head-actions">
        {#if session}
          <button type="button" class="text-btn" on:click={() => dispatch("openDashboard")}>打开大屏</button>
          <button type="button" class="text-btn" on:click={() => dispatch("rebindSession")}>换绑</button>
        {/if}
      </div>
    </header>
  {:else if session}
    <div class="compact-meta">
      <span>
        {session.shopName || "已绑定"}
        · {formatDashboardSessionTime(session.startedAt)}
        {#if matchLabel} · {matchLabel}{/if}
      </span>
      <div class="head-actions">
        <button type="button" class="text-btn" on:click={() => dispatch("openDashboard")}>打开大屏</button>
        <button type="button" class="text-btn" on:click={() => dispatch("rebindSession")}>换绑</button>
      </div>
    </div>
  {/if}

  {#if orderSummary}
    <p class="order-line">
      订单 {orderSummary.eventCount.toLocaleString("zh-CN")} 笔
      · ¥{orderSummary.totalPayAmountYuan.toLocaleString("zh-CN")}
      {#if orderSummary.peakMinuteLabel}
        · 高峰 {orderSummary.peakMinuteLabel}
      {/if}
    </p>
  {/if}

  {#if showDashboardBind && candidates.length}
    <div class="bind-block bind-block-top">
      {#if session}
        <p class="hint">当前匹配不对时，在这里选择正确场次并覆盖原绑定。</p>
      {:else}
        <p class="hint">系统无法唯一判断，请选择与视频开播时间对应的场次。</p>
      {/if}
      <div class="bind-row">
        <select bind:value={selectedSessionId} aria-label="选择直播数据场次">
          <option value="">选择正确场次</option>
          {#each candidates as candidate (candidate.session.id)}
            <option value={String(candidate.session.id)}>
              {dashboardCandidateLabel(candidate)}
            </option>
          {/each}
        </select>
        <button
          type="button"
          class="bind-btn"
          disabled={!selectedSessionId}
          on:click={() => dispatch("bindSession")}
        >
          {session ? "确认换绑" : "绑定"}
        </button>
      </div>
    </div>
  {/if}

  {#if loading}
    <p class="status">
      <Loader2 size={13} class="is-spinning" />
      加载场次中…
    </p>
  {:else}
    {#if visual}
      <div class="hero-grid">
        {#each visual.heroes as hero (hero.key)}
          <article class="hero-card" data-key={hero.key}>
            <span>{hero.label}</span>
            <strong>{hero.value}</strong>
            <small>{hero.hint}</small>
          </article>
        {/each}
      </div>

      <div class="rate-panel">
        {#each visual.rates as rate (rate.key)}
          <div class="rate-row">
            <div class="rate-label">
              <span>{rate.label}</span>
              <strong>{rate.value}</strong>
            </div>
            <div class="rate-track" aria-hidden="true">
              {#if rate.percent == null}
                <div class="rate-fill missing"></div>
              {:else}
                <div class="rate-fill" style={`width:${Math.max(rate.percent, rate.percent > 0 ? 2 : 0)}%`}></div>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <div class="secondary-row">
        {#each visual.secondary as item (item.label)}
          <div class="secondary">
            <span>{item.label}</span>
            <strong>{item.value}</strong>
          </div>
        {/each}
      </div>
    {/if}

    {#if !session && showDashboardBind && !candidates.length}
      <p class="status muted">未找到可绑定场次。请先在「直播数据大屏」导入官方 XLSX。</p>
    {:else if !session && !orderSummary}
      <p class="status muted">暂无数据</p>
    {/if}
  {/if}

  {#if error}
    <small class="board-error">{error}</small>
  {/if}
</section>

<style>
  .live-data-board {
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    color: #374151;
    overflow: auto;
  }
  .board-head, .compact-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .board-title {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12px;
  }
  .board-title strong {
    color: #111827;
    font-size: 13px;
    font-weight: 600;
  }
  .meta, .compact-meta > span {
    color: #9ca3af;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head-actions {
    display: inline-flex;
    gap: 10px;
    flex: 0 0 auto;
  }
  .text-btn {
    border: 0;
    background: transparent;
    color: #4b5563;
    font-size: 12px;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .text-btn:disabled, .bind-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .order-line, .hint {
    margin: 0;
    font-size: 12px;
    color: #6b7280;
    line-height: 1.45;
  }
  .hero-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  .hero-card {
    min-width: 0;
    padding: 10px 12px;
    border: 1px solid #e5e7eb;
    border-radius: 10px;
    background: #fafafa;
    display: grid;
    gap: 4px;
  }
  .hero-card[data-key="gmv"] {
    background: #111827;
    border-color: #111827;
    color: #f9fafb;
  }
  .hero-card[data-key="gmv"] span,
  .hero-card[data-key="gmv"] small { color: #9ca3af; }
  .hero-card[data-key="gmv"] strong { color: #fff; }
  .hero-card span {
    font-size: 11px;
    color: #6b7280;
  }
  .hero-card strong {
    font-size: 18px;
    line-height: 1.15;
    font-weight: 700;
    color: #111827;
    font-variant-numeric: tabular-nums;
  }
  .hero-card small {
    font-size: 11px;
    color: #9ca3af;
  }
  .rate-panel {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid #e5e7eb;
    border-radius: 10px;
    background: #fff;
  }
  .rate-row { display: grid; gap: 4px; }
  .rate-label {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
  }
  .rate-label span { color: #6b7280; }
  .rate-label strong {
    color: #111827;
    font-variant-numeric: tabular-nums;
  }
  .rate-track {
    height: 6px;
    border-radius: 999px;
    background: #f3f4f6;
    overflow: hidden;
  }
  .rate-fill {
    height: 100%;
    border-radius: 999px;
    background: #111827;
  }
  .rate-fill.missing {
    width: 100%;
    background: repeating-linear-gradient(
      -45deg,
      #e5e7eb,
      #e5e7eb 4px,
      #f3f4f6 4px,
      #f3f4f6 8px
    );
  }
  .secondary-row {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }
  .secondary {
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 8px;
    background: #f9fafb;
  }
  .secondary span { font-size: 11px; color: #9ca3af; }
  .secondary strong {
    font-size: 13px;
    color: #111827;
    font-variant-numeric: tabular-nums;
  }
  .bind-block { min-width: 0; }
  .bind-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .bind-row select {
    min-width: min(320px, 100%);
    height: 32px;
    padding: 0 8px;
    border: 1px solid #e5e7eb;
    border-radius: 6px;
    color: #374151;
    background: #fff;
    font: inherit;
  }
  .bind-btn {
    height: 32px;
    padding: 0 12px;
    border: 1px solid #111827;
    border-radius: 6px;
    background: #111827;
    color: #fff;
    font-size: 12px;
    cursor: pointer;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 12px;
    color: #6b7280;
  }
  .status.muted { color: #9ca3af; }
  .board-error { color: #b91c1c; font-size: 11px; }
  :global(.is-spinning) { animation: board-spin 0.9s linear infinite; }
  @keyframes board-spin { to { transform: rotate(360deg); } }

  @media (max-width: 900px) {
    .hero-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .secondary-row { grid-template-columns: 1fr; }
  }
</style>
