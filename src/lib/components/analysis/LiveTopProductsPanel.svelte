<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "../../invoker";
  import type { PaymentEvent } from "../../orderDealTimeline";
  import {
    buildLiveProductCatalog,
    extractLiveTopProducts,
    liveProductCatalogSignature,
    type LiveProductMentionScanSnapshot,
  } from "../../liveTopProducts";
  import type { WorkspaceTranscriptEntry } from "../../companyAnalysisWorkspace";

  export let videoId: number | null = null;
  export let archiveSource = false;
  export let events: PaymentEvent[] = [];
  export let transcriptEntries: WorkspaceTranscriptEntry[] = [];

  let snapshot: LiveProductMentionScanSnapshot | null = null;
  let loading = false;
  let error = "";
  let activeKey = "";
  let runToken = 0;

  $: catalog = buildLiveProductCatalog(events);
  $: catalogSignature = liveProductCatalogSignature(catalog);
  $: scanKey = videoId != null && catalogSignature ? `${videoId}:${catalogSignature}` : "";
  $: if (scanKey && scanKey !== activeKey) {
    activeKey = scanKey;
    void loadOrStartScan(videoId as number, catalogSignature, catalog, ++runToken);
  }
  $: if (!scanKey && activeKey) {
    activeKey = "";
    snapshot = null;
    error = "";
    loading = false;
    runToken += 1;
  }
  $: archiveProducts = archiveSource
    ? extractLiveTopProducts(transcriptEntries, events).slice(0, 5)
    : [];
  $: visibleProducts = videoId == null
    ? archiveProducts
    : (snapshot?.products ?? []).filter((product) => product.mentionCount > 0).slice(0, 5);
  $: progress = snapshot && snapshot.totalDurationSec > 0
    ? Math.min(100, Math.max(0, snapshot.processedDurationSec / snapshot.totalDurationSec * 100))
    : 0;

  onDestroy(() => {
    runToken += 1;
  });

  function wait(milliseconds: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
  }

  async function hasActiveScanTask(targetVideoId: number): Promise<boolean> {
    try {
      const tasks = await invoke<Array<{ task_type?: string; taskType?: string; status: string; metadata?: string }>>("get_tasks");
      return tasks.some((task) => {
        if ((task.task_type || task.taskType) !== "generate_video_product_mention_scan") return false;
        if (!['pending', 'processing'].includes(task.status)) return false;
        try {
          const metadata = JSON.parse(task.metadata || "{}") as { video_id?: number };
          return Number(metadata.video_id) === targetVideoId;
        } catch {
          return false;
        }
      });
    } catch {
      return false;
    }
  }

  async function refreshSnapshot(
    targetVideoId: number,
    signature: string,
    token: number,
  ): Promise<LiveProductMentionScanSnapshot | null> {
    const next = await invoke<LiveProductMentionScanSnapshot>("get_video_product_mention_scan", {
      id: targetVideoId,
      catalogSignature: signature,
    });
    if (token !== runToken) return null;
    snapshot = next;
    return next;
  }

  async function loadOrStartScan(
    targetVideoId: number,
    signature: string,
    products: ReturnType<typeof buildLiveProductCatalog>,
    token: number,
  ): Promise<void> {
    loading = true;
    error = "";
    try {
      let current = await refreshSnapshot(targetVideoId, signature, token);
      if (!current || token !== runToken) return;
      if (current.status === "completed") return;

      // The first chunk may still be running before a partial cache exists.
      // Task state prevents reopening the panel from starting a duplicate scan.
      const taskActive = await hasActiveScanTask(targetVideoId);
      let generationSettled = taskActive;
      let generationError: unknown = null;
      if (!taskActive) {
        generationSettled = false;
        void invoke<LiveProductMentionScanSnapshot>("generate_video_product_mention_scan", {
          eventId: `full_product_scan_${targetVideoId}_${Date.now()}`,
          id: targetVideoId,
          catalogSignature: signature,
          products,
          force: false,
        }).then((result) => {
          if (token === runToken) snapshot = result;
          generationSettled = true;
        }).catch((reason) => {
          generationError = reason;
          generationSettled = true;
        });
      }

      while (token === runToken) {
        await wait(3000);
        current = await refreshSnapshot(targetVideoId, signature, token);
        if (!current || current.status === "completed") break;
        if (current.status === "failed" && generationSettled) break;
        if (taskActive && !(await hasActiveScanTask(targetVideoId))) break;
        if (!taskActive && generationSettled) break;
      }
      if (generationError && token === runToken) throw generationError;
    } catch (reason: any) {
      if (token === runToken) {
        error = String(reason?.message || reason || "整场商品扫描失败");
      }
    } finally {
      if (token === runToken) loading = false;
    }
  }

  function retry(): void {
    if (videoId == null || !catalogSignature) return;
    void loadOrStartScan(videoId, catalogSignature, catalog, ++runToken);
  }
</script>

<section class="top-products-panel" aria-label="整场高频商品 TOP5">
  <header>
    <div>
      <strong>整场高频商品 TOP5</strong>
      <span>按主播逐句提及次数统计</span>
    </div>
    {#if snapshot?.status === "processing" || loading}
      <small>扫描中 {progress.toFixed(0)}%</small>
    {:else if snapshot?.status === "completed" || (archiveSource && transcriptEntries.length)}
      <small>统计完成</small>
    {/if}
  </header>

  {#if videoId == null && !archiveSource}
    <p class="empty">请选择一场录播</p>
  {:else if !catalog.length}
    <p class="empty">本场没有可用于识别的订单商品</p>
  {:else if videoId != null && (error || snapshot?.status === "failed")}
    <div class="error-row">
      <span>{error || snapshot?.error || "整场商品扫描失败"}</span>
      <button type="button" on:click={retry}>继续扫描</button>
    </div>
  {:else if visibleProducts.length}
    <ol>
      {#each visibleProducts as product, index (product.id)}
        <li>
          <span class="rank">{index + 1}</span>
          <span class="name" title={product.name}>{product.name}</span>
          <strong>{product.mentionCount} 次</strong>
        </li>
      {/each}
    </ol>
  {:else if archiveSource && !transcriptEntries.length}
    <p class="empty">等待整场逐字稿生成…</p>
  {:else if snapshot?.status === "completed" || archiveSource}
    <p class="empty">整场直播暂未识别到商品提及</p>
  {:else}
    <p class="empty">正在扫描整场直播商品…</p>
  {/if}

  {#if snapshot?.status === "processing" || loading}
    <div class="progress" aria-label={`整场商品扫描进度 ${progress.toFixed(0)}%`}>
      <span style={`width:${progress}%`}></span>
    </div>
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
  header, header > div, ol, li, .error-row {
    display: flex;
    align-items: center;
  }
  header { justify-content: space-between; gap: 12px; }
  header > div { gap: 8px; min-width: 0; }
  header strong { color: #1d4ed8; font-size: 13px; white-space: nowrap; }
  header span, header small { color: #94a3b8; font-size: 11px; white-space: nowrap; }
  ol { list-style: none; gap: 8px; margin: 7px 0 0; padding: 0; overflow: hidden; }
  li {
    flex: 1 1 0;
    min-width: 0;
    gap: 6px;
    padding: 5px 7px;
    border: 1px solid #e5efff;
    border-radius: 8px;
    background: #fff;
    font-size: 12px;
  }
  .rank {
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    border-radius: 5px;
    background: #dbeafe;
    color: #1d4ed8;
    font-weight: 700;
  }
  .name { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #334155; }
  li strong { flex: 0 0 auto; color: #0f172a; font-size: 12px; }
  .empty { margin: 6px 0 0; color: #94a3b8; font-size: 12px; }
  .error-row { justify-content: space-between; gap: 12px; margin-top: 6px; color: #b42318; font-size: 12px; }
  .error-row span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .error-row button { border: 0; background: transparent; color: #175cd3; cursor: pointer; white-space: nowrap; }
  .progress { height: 3px; margin-top: 7px; overflow: hidden; border-radius: 99px; background: #dbeafe; }
  .progress span { display: block; height: 100%; border-radius: inherit; background: #2563eb; transition: width .25s ease; }
  @media (max-width: 900px) {
    ol { flex-wrap: wrap; }
    li { flex-basis: calc(50% - 4px); }
    header span { display: none; }
  }
</style>
