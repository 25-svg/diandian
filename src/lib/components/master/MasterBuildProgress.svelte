<script lang="ts">
  import { CheckCircle2, Loader2, RotateCcw } from "lucide-svelte";
  import { chunkProgress } from "../../masterScript";
  export let completed = 0;
  export let total = 0;
  export let failed = false;
  export let message = "";
  export let onRetry: (() => void) | null = null;
  $: progress = chunkProgress(completed, total);
</script>

<div class="progress" aria-live="polite">
  <div class="progress-heading">
    {#if failed}<RotateCcw size={18} />{:else if progress === 100}<CheckCircle2 size={18} />{:else}<Loader2 class="spin" size={18} />{/if}
    <strong>{failed ? "母稿转写已暂停" : progress === 100 ? "逐字稿已生成" : "正在生成整场逐字稿"}</strong>
    <span>{progress}%</span>
  </div>
  <div class="track"><span style={`width:${progress}%`}></span></div>
  <p>{message || `已完成 ${completed}/${total} 个十分钟分段`}</p>
  {#if failed && onRetry}<button type="button" on:click={onRetry}><RotateCcw size={15} />继续未完成部分</button>{/if}
</div>

<style>
  .progress{padding:16px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}.progress-heading{display:flex;align-items:center;gap:8px;color:#1f2937}.progress-heading span{margin-left:auto;font-variant-numeric:tabular-nums}.track{height:6px;margin-top:12px;overflow:hidden;border-radius:3px;background:#eef0f3}.track span{display:block;height:100%;background:#007aff;transition:width .2s}.progress p{margin:9px 0 0;color:#6b7280;font-size:13px}.progress button{display:inline-flex;align-items:center;gap:6px;margin-top:10px;border:0;background:none;color:#007aff;cursor:pointer}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
</style>
