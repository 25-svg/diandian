<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Loader2, Upload } from "lucide-svelte";

  export let orderCount = 0;
  export let totalPayYuan = 0;
  export let loading = false;
  export let error = "";
  export let canPullOrders = false;
  export let canImportRawJson = false;
  export let peakLabel = "";
  export let canCalibrateCurrent = false;
  export let showCalibration = false;
  export let calibrationWarning = false;
  export let excelLabel = "";
  export let excelCandidates: Array<{ id: number; label: string }> = [];
  export let selectedExcelId = "";
  export let excelLoading = false;
  export let excelCanDownload = false;
  export let excelDownloading = false;
  export let excelDownloadHint = "";

  const dispatch = createEventDispatcher<{
    importClean: void;
    pullOrders: void;
    importRawJson: void;
    importCleanedFile: void;
    clearOrders: void;
    seekPeak: void;
    confirmLiveStart: void;
    calibrateCurrent: void;
    bindExcel: void;
    downloadExcel: void;
  }>();
</script>

<section class="align-panel" aria-label="对齐本场">
  <div class="block">
    <div class="block-head">
      <strong>订单</strong>
      {#if orderCount > 0}
        <span>已拉取 {orderCount} 笔 · ¥{totalPayYuan.toLocaleString("zh-CN")}</span>
      {:else}
        <span>未拉取</span>
      {/if}
    </div>
    <div class="actions">
      <button type="button" class="primary" disabled={loading || !canPullOrders} on:click={() => dispatch("pullOrders")}>
        {#if loading}
          <Loader2 size={14} class="is-spinning" />
        {/if}
        自动拉取订单
      </button>
      <button type="button" class="ghost" disabled={loading} on:click={() => dispatch("importClean")}>
        <Upload size={14} />
        手动导入（备用）
      </button>
      <button type="button" class="ghost" disabled={loading || !canImportRawJson} on:click={() => dispatch("importRawJson")}>
        更新 JSON
      </button>
      <button type="button" class="ghost" disabled={loading} on:click={() => dispatch("importCleanedFile")}>
        已清洗文件
      </button>
      {#if orderCount > 0}
        <button type="button" class="ghost" on:click={() => dispatch("clearOrders")}>清除</button>
      {/if}
    </div>
    {#if peakLabel}
      <button type="button" class="peak" on:click={() => dispatch("seekPeak")}>成交高峰：{peakLabel}</button>
    {/if}
  </div>

  {#if showCalibration}
    <div class="block" class:warning={calibrationWarning}>
      <div class="block-head">
        <strong>视频对齐</strong>
      </div>
      <div class="actions">
        <button type="button" class="secondary" on:click={() => dispatch("confirmLiveStart")}>
          视频从开播开始
        </button>
        <button
          type="button"
          class="secondary"
          disabled={!canCalibrateCurrent}
          on:click={() => dispatch("calibrateCurrent")}
        >
          当前位置是成交点
        </button>
      </div>
    </div>
  {/if}

  <div class="block">
    <div class="block-head">
      <strong>整场 Excel</strong>
      {#if excelLabel}
        <span>{excelLabel}</span>
      {:else}
        <span>未匹配</span>
      {/if}
    </div>
    {#if excelLabel}
      <p class="hint">已按本场视频的开播时间和时长自动对上。不对时请改选或重新下载当天 Excel。</p>
    {:else}
      <p class="hint">用视频时长和日期匹配已导入场次；本地没有时，点下方下载当天罗盘「整场数据」（首次需扫码登录）。</p>
    {/if}
    {#if excelCandidates.length}
      <div class="excel-row">
        <select bind:value={selectedExcelId} disabled={excelLoading || excelDownloading} aria-label="按视频匹配的 Excel 场次">
          <option value="">选择匹配场次</option>
          {#each excelCandidates as candidate (candidate.id)}
            <option value={String(candidate.id)}>{candidate.label}</option>
          {/each}
        </select>
        <button
          type="button"
          class="secondary"
          disabled={excelLoading || excelDownloading || !selectedExcelId}
          on:click={() => dispatch("bindExcel")}
        >
          使用这场
        </button>
      </div>
    {/if}
    {#if excelCanDownload && !excelLabel}
      <div class="actions">
        <button
          type="button"
          class="secondary"
          disabled={excelDownloading || excelLoading}
          on:click={() => dispatch("downloadExcel")}
        >
          {#if excelDownloading}
            <Loader2 size={14} class="is-spinning" />
            正在下载整场数据…
          {:else}
            下载本场 Excel
          {/if}
        </button>
        {#if excelDownloadHint}
          <span class="hint-inline">{excelDownloadHint}</span>
        {/if}
      </div>
    {/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</section>

<style>
  .align-panel {
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    overflow: auto;
    box-sizing: border-box;
  }
  .block {
    display: grid;
    gap: 10px;
    padding: 12px;
    border: 1px solid #e4e7ec;
    border-radius: 12px;
    background: #fff;
  }
  .block.warning {
    border-color: #f7b27a;
    background: #fffaf5;
  }
  .block-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .block-head strong {
    color: #101828;
    font-size: 13px;
  }
  .block-head span {
    color: #667085;
    font-size: 12px;
  }
  .hint {
    margin: 0;
    color: #667085;
    font-size: 12px;
    line-height: 1.5;
  }
  .hint-inline {
    color: #667085;
    font-size: 12px;
  }
  .excel-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .excel-row select {
    flex: 1;
    min-width: 0;
    border: 1px solid #d0d5dd;
    border-radius: 8px;
    padding: 7px 8px;
    font-size: 12px;
  }
  .excel-row button {
    flex-shrink: 0;
    border-radius: 8px;
    padding: 7px 11px;
    font-size: 12px;
    cursor: pointer;
  }
  .excel-row button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-radius: 8px;
    padding: 7px 11px;
    font-size: 12px;
    cursor: pointer;
  }
  .actions button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .primary {
    border: 1px solid #2e90fa;
    background: #eff8ff;
    color: #175cd3;
    font-weight: 600;
  }
  .secondary {
    border: 1px solid #d0d5dd;
    background: #fff;
    color: #344054;
  }
  .ghost {
    border: 0;
    background: transparent;
    color: #667085;
    padding-left: 4px;
    padding-right: 4px;
  }
  .peak {
    align-self: start;
    border: 0;
    background: transparent;
    color: #027a48;
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    padding: 0;
  }
  .error {
    margin: 0;
    color: #b42318;
    font-size: 12px;
  }
  :global(.is-spinning) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
