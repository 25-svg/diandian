<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { Check, X } from "lucide-svelte";
  import MasterBuildProgress from "./MasterBuildProgress.svelte";
  import MasterPreviewPanel from "./MasterPreview.svelte";
  import { previewMasterScript, publishMasterScript, startMasterIngest, type MasterPreview } from "../../masterScript";
  export let videoId: number;
  export let videoTitle: string;
  const dispatch = createEventDispatcher();
  let sourceId = 0;
  let completed = 0;
  let total = 0;
  let failed = false;
  let error = "";
  let preview: MasterPreview | null = null;
  let busy = true;
  let scriptKey = `MS-${new Date().toISOString().slice(0,10).replaceAll("-", "")}`;

  async function build(): Promise<void> {
    busy = true; failed = false; error = "";
    try {
      const result = await startMasterIngest({ videoId, title: videoTitle });
      sourceId = result.sourceId; completed = result.status.completed; total = result.status.total;
      failed = result.status.status === "failed";
      if (failed) { error = result.status.error || "转写中断"; return; }
      preview = await previewMasterScript(sourceId, scriptKey, videoTitle);
    } catch (reason: any) { failed = true; error = reason?.message || String(reason); }
    finally { busy = false; }
  }

  async function publish(): Promise<void> {
    if (!preview?.validation.publishable || busy) return;
    busy = true; error = "";
    try { const master = await publishMasterScript(sourceId, scriptKey, preview.draft); dispatch("published", { sourceId, scriptKey, masterScriptId: master.id }); }
    catch (reason: any) { error = reason?.message || String(reason); }
    finally { busy = false; }
  }
  onMount(build);
</script>

<div class="backdrop" role="presentation">
  <section class="dialog" role="dialog" aria-modal="true" aria-labelledby="master-title">
    <header><div><h2 id="master-title">生成整场直播母稿</h2><p>{videoTitle}</p></div><button class="icon" on:click={() => dispatch("close")} aria-label="关闭"><X size={20}/></button></header>
    <div class="body">
      <label>母稿编号<input bind:value={scriptKey} disabled={Boolean(preview) || busy} /></label>
      <p class="hint">系统会自动使用公司确认的参数卡纠正产品名和型号；价格、库存和优惠仍以主播原话为准。</p>
      {#if !preview}<MasterBuildProgress {completed} {total} {failed} message={error} onRetry={failed ? build : null}/>{/if}
      {#if preview}
        {#if preview.validation.blockingIssues.length}<div class="warning">{preview.validation.blockingIssues[0].message}</div>{/if}
        <MasterPreviewPanel {preview}/>
      {/if}
      {#if error && preview}<div class="warning">{error}</div>{/if}
    </div>
    <footer><button class="secondary" on:click={() => dispatch("close")}>稍后处理</button><button class="primary" disabled={!preview?.validation.publishable || busy} on:click={publish}><Check size={16}/>发布母稿 V1.0</button></footer>
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:70;display:grid;place-items:center;padding:24px;background:rgba(15,23,42,.28);backdrop-filter:blur(12px)}.dialog{display:flex;width:min(860px,96vw);max-height:90vh;flex-direction:column;overflow:hidden;border:1px solid rgba(255,255,255,.7);border-radius:12px;background:#f7f7f9;box-shadow:0 24px 70px rgba(15,23,42,.22)}header,footer{display:flex;align-items:center;padding:16px 20px;background:rgba(255,255,255,.88)}header{border-bottom:1px solid #e5e7eb}header h2{margin:0;font-size:18px}header p{margin:3px 0 0;color:#6b7280;font-size:13px}.icon{margin-left:auto;border:0;background:none;cursor:pointer}.body{display:grid;gap:14px;overflow:auto;padding:18px 20px}label{display:grid;gap:6px;color:#374151;font-size:13px}input{height:38px;padding:0 11px;border:1px solid #d1d5db;border-radius:7px;background:#fff}.hint{margin:0;color:#6b7280;font-size:13px;line-height:1.5}.warning{padding:10px 12px;border:1px solid #f5d38a;border-radius:7px;background:#fff8e8;color:#8a5a00;font-size:13px}footer{justify-content:flex-end;gap:9px;border-top:1px solid #e5e7eb}footer button{display:inline-flex;height:38px;align-items:center;gap:6px;padding:0 15px;border-radius:7px;cursor:pointer}.secondary{border:1px solid #d1d5db;background:#fff}.primary{border:0;background:#007aff;color:#fff}.primary:disabled{opacity:.45;cursor:not-allowed}
</style>
