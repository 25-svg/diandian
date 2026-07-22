<script lang="ts">
  import { Check, Download, Loader2, X, XCircle } from "lucide-svelte";
  import { createEventDispatcher } from "svelte";
  import {
    exportDictionaryCandidates,
    isLatestRequestResult,
    listDictionaryCandidates,
    setDictionaryCandidateStatus,
    type DictionaryCandidateExportFormat,
    type DictionaryCandidateStatus,
    type TranscriptDictionaryCandidate,
  } from "../../transcriptReview";

  const dispatch = createEventDispatcher<{ close: void }>();
  const filters: Array<{ value: "pending" | "approved" | "rejected"; label: string }> = [
    { value: "pending", label: "待确认" },
    { value: "approved", label: "已通过" },
    { value: "rejected", label: "已拒绝" },
  ];

  let filter: "pending" | "approved" | "rejected" = "pending";
  let candidates: TranscriptDictionaryCandidate[] = [];
  let loading = true;
  let busyId: number | null = null;
  let exporting: DictionaryCandidateExportFormat | null = null;
  let errorMessage = "";
  let loadedFilter = "";
  let loadRequestSequence = 0;

  $: if (filter !== loadedFilter) {
    loadedFilter = filter;
    void load(filter);
  }

  async function load(status = filter): Promise<void> {
    const requestedFilter = status;
    const requestId = ++loadRequestSequence;
    loading = true;
    errorMessage = "";
    try {
      const result = await listDictionaryCandidates(requestedFilter);
      if (!isLatestRequestResult(requestId, loadRequestSequence, requestedFilter, filter)) return;
      candidates = result;
    } catch (error: any) {
      if (!isLatestRequestResult(requestId, loadRequestSequence, requestedFilter, filter)) return;
      errorMessage = error?.message || String(error);
    } finally {
      if (isLatestRequestResult(requestId, loadRequestSequence, requestedFilter, filter)) {
        loading = false;
      }
    }
  }

  async function setStatus(id: number, status: DictionaryCandidateStatus): Promise<void> {
    busyId = id;
    errorMessage = "";
    try {
      await setDictionaryCandidateStatus(id, status);
      await load();
    } catch (error: any) {
      errorMessage = error?.message || String(error);
    } finally {
      busyId = null;
    }
  }

  async function downloadExport(format: DictionaryCandidateExportFormat): Promise<void> {
    exporting = format;
    errorMessage = "";
    try {
      const content = await exportDictionaryCandidates(format);
      const blob = new Blob([content], { type: format === "json" ? "application/json" : "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `transcript-dictionary-candidates.${format}`;
      link.click();
      URL.revokeObjectURL(url);
    } catch (error: any) {
      errorMessage = error?.message || String(error);
    } finally {
      exporting = null;
    }
  }
</script>

<div class="drawer-backdrop" role="presentation" on:click|self={() => dispatch("close")}>
  <aside class="drawer" aria-label="词库候选">
    <header>
      <div><h2>词库候选</h2><p>确认后可用于后续识别。</p></div>
      <div class="header-actions">
        <button title="导出 JSON" disabled={Boolean(exporting)} on:click={() => downloadExport("json")}>
          {#if exporting === "json"}<Loader2 size={15} class="spin" />{:else}<Download size={15} />{/if}<span>JSON</span>
        </button>
        <button title="导出 CSV" disabled={Boolean(exporting)} on:click={() => downloadExport("csv")}>
          {#if exporting === "csv"}<Loader2 size={15} class="spin" />{:else}<Download size={15} />{/if}<span>CSV</span>
        </button>
        <button class="icon-only" title="关闭词库候选" on:click={() => dispatch("close")}><X size={17} /></button>
      </div>
    </header>

    <nav aria-label="候选状态">
      {#each filters as item}
        <button class:active={filter === item.value} on:click={() => filter = item.value}>{item.label}</button>
      {/each}
    </nav>

    {#if errorMessage}<p class="error" role="alert">{errorMessage}<button on:click={() => load()}>重试</button></p>{/if}

    <div class="candidate-list">
      {#if loading}
        <div class="state"><Loader2 size={25} class="spin" /><span>正在读取候选词…</span></div>
      {:else if candidates.length}
        {#each candidates as candidate}
          <article>
            <div class="replacement"><span>{candidate.sourceText}</span><strong>→</strong><span>{candidate.targetText || "保留原词"}</span></div>
            <small>{new Date(candidate.updatedAt).toLocaleString("zh-CN")}</small>
            <div class="row-actions">
              <button disabled={busyId === candidate.id || candidate.status === "approved"} on:click={() => setStatus(candidate.id, "approved")}>
                <Check size={14} />通过
              </button>
              <button disabled={busyId === candidate.id || candidate.status === "rejected"} on:click={() => setStatus(candidate.id, "rejected")}>
                <XCircle size={14} />拒绝
              </button>
            </div>
          </article>
        {/each}
      {:else}
        <div class="state"><span>当前状态没有候选词。</span></div>
      {/if}
    </div>
  </aside>
</div>

<style>
  .drawer-backdrop { position: fixed; inset: 0; z-index: 80; display: flex; justify-content: flex-end; background: rgba(16,24,40,.28); }
  .drawer { width: min(430px, 92vw); height: 100%; display: flex; flex-direction: column; color: #1d2939; background: #fff; box-shadow: -18px 0 40px rgba(16,24,40,.18); }
  header { height: 68px; flex: 0 0 68px; display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 0 16px; border-bottom: 1px solid #eaecf0; }
  h2 { margin: 0; font-size: 15px; }
  header p { margin: 3px 0 0; color: #667085; font-size: 10px; }
  .header-actions, .row-actions { display: flex; align-items: center; gap: 6px; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 4px; border: 1px solid #d8dde6; border-radius: 6px; color: #344054; background: white; font: inherit; font-size: 10px; cursor: pointer; }
  button:disabled { cursor: not-allowed; opacity: .45; }
  .header-actions button { height: 30px; padding: 0 7px; }
  .header-actions .icon-only { width: 30px; padding: 0; }
  nav { display: grid; grid-template-columns: repeat(3, 1fr); gap: 3px; margin: 10px 14px 6px; padding: 3px; border-radius: 7px; background: #f0f2f5; }
  nav button { height: 29px; border: 0; background: transparent; }
  nav button.active { color: #0071e3; background: white; box-shadow: 0 1px 4px rgba(16,24,40,.09); }
  .error { margin: 6px 14px; padding: 8px; color: #b42318; background: #fef3f2; font-size: 10px; }
  .error button { margin-left: 8px; color: #b42318; border-color: #fda29b; }
  .candidate-list { min-height: 0; flex: 1; overflow: auto; padding: 6px 14px 14px; }
  article { display: grid; gap: 8px; margin-top: 8px; padding: 11px; border: 1px solid #e3e7ed; border-radius: 7px; }
  .replacement { display: grid; grid-template-columns: minmax(0,1fr) auto minmax(0,1fr); align-items: center; gap: 7px; font-size: 12px; }
  .replacement span { padding: 7px; background: #f7f8fa; overflow-wrap: anywhere; }
  .replacement strong { color: #98a2b3; }
  small { color: #98a2b3; font-size: 9px; }
  .row-actions { justify-content: flex-end; }
  .row-actions button { height: 28px; padding: 0 9px; }
  .state { min-height: 240px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; color: #98a2b3; font-size: 11px; }
  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.dark) .drawer { color: #f2f4f7; background: #24262a; }
  :global(.dark) header { border-color: #3d4148; }
  :global(.dark) article, :global(.dark) button { color: #e4e7ec; border-color: #4b5058; background: #303238; }
  :global(.dark) .replacement span { background: #292b30; }
</style>
