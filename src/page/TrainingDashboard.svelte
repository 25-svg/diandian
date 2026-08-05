<script lang="ts">
  import { onMount } from "svelte";
  import { RefreshCw } from "lucide-svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import {
    buildTrainingDashboardRows,
    type StoredSessionAnalysis,
    type TrainingDashboardRow,
  } from "../lib/trainingDashboard";

  let rows: TrainingDashboardRow[] = [];
  let trainingPackCount = 0;

  function load(): void {
    const records: StoredSessionAnalysis[] = [];
    for (let index = 0; index < localStorage.length; index += 1) {
      const key = localStorage.key(index);
      if (!key?.startsWith("bsr:content-analysis:v3:")) continue;
      try {
        const value = JSON.parse(localStorage.getItem(key) || "{}");
        records.push({
          ...value,
          sourceKey: key.slice("bsr:content-analysis:v3:".length),
        });
      } catch {
        // A damaged local cache entry should not block the management view.
      }
    }
    rows = buildTrainingDashboardRows(records);
    trainingPackCount = Number(localStorage.getItem("bsr:training-pack-exports") || 0);
  }

  onMount(load);
</script>

<PageShell title="主播培养看板" subtitle="汇总本机已保存的场次诊断、待确认事项和高价值候选。">
  <div slot="actions">
    <button type="button" class="mac-btn" title="刷新" on:click={load}>
      <RefreshCw size={16} />刷新
    </button>
  </div>

  <div class="summary-strip">
    <div><span>已保存场次</span><strong>{rows.length}</strong></div>
    <div><span>高价值候选</span><strong>{rows.reduce((total, row) => total + row.highScoreCount, 0)}</strong></div>
    <div><span>待复盘片段</span><strong>{rows.reduce((total, row) => total + row.pendingCount, 0)}</strong></div>
    <div><span>已导出培养包</span><strong>{trainingPackCount}</strong></div>
  </div>

  <div class="mac-card table-wrap">
    <table class="mac-table">
      <thead><tr><th>场次</th><th>一句话诊断</th><th>平均分</th><th>高价值</th><th>待复盘</th><th>待确认</th><th>更新时间</th></tr></thead>
      <tbody>
        {#each rows as row}
          <tr>
            <td><strong>{row.title}</strong><small>{row.sourceKey}</small></td>
            <td>{row.headline}</td>
            <td>{row.averageScore ?? "-"}</td>
            <td>{row.highScoreCount}</td>
            <td>{row.pendingCount}</td>
            <td>{row.confirmationCount}</td>
            <td>{new Date(row.updatedAt).toLocaleString("zh-CN")}</td>
          </tr>
        {:else}
          <tr><td class="empty" colspan="7">尚无已保存场次。完成一次片段发现后，这里会自动汇总。</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</PageShell>

<style>
  .summary-strip {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-lg);
    background: var(--mac-bg-card);
    overflow: hidden;
  }
  .summary-strip div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-right: 1px solid var(--mac-separator);
  }
  .summary-strip div:last-child { border-right: 0; }
  .summary-strip span { color: var(--mac-tertiary); font-size: 12px; }
  .summary-strip strong { font-size: 22px; color: var(--mac-blue); letter-spacing: -0.4px; }
  .table-wrap { overflow: auto; }
  .table-wrap :global(td strong),
  .table-wrap :global(td small) { display: block; }
  .table-wrap :global(td small) { margin-top: 3px; color: var(--mac-quaternary); font-size: 9px; }
  .empty { text-align: center; color: var(--mac-quaternary); padding: 48px !important; }
  @media (max-width: 1000px) {
    .summary-strip { grid-template-columns: 1fr 1fr; }
    .summary-strip div:nth-child(2) { border-right: 0; }
    .summary-strip div:nth-child(1),
    .summary-strip div:nth-child(2) { border-bottom: 1px solid var(--mac-separator); }
  }
</style>
