<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { X } from "lucide-svelte";
  export let videoId: number;
  export let videoTitle: string;
  const dispatch = createEventDispatcher();
  // Keep props referenced so callers don't need to change.
  void videoId;
  onMount(() => {});
</script>

<div class="backdrop" role="presentation">
  <section class="dialog" role="dialog" aria-modal="true" aria-labelledby="master-title">
    <header><div><h2 id="master-title">整场母稿入口已关闭</h2><p>{videoTitle}</p></div><button class="icon" on:click={() => dispatch("close")} aria-label="关闭"><X size={20}/></button></header>
    <div class="body">
      <div class="warning">已废弃整场直接进 `10-企业母稿`。请走 Clip「母稿样本批次」→ 发布到「主播知识库」的 视频/成交、话术、分析建议（视频只写路径引用，不拷贝文件）。</div>
    </div>
    <footer><button class="secondary" on:click={() => dispatch("close")}>关闭</button></footer>
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:70;display:grid;place-items:center;padding:24px;background:rgba(15,23,42,.28);backdrop-filter:blur(12px)}.dialog{display:flex;width:min(860px,96vw);max-height:90vh;flex-direction:column;overflow:hidden;border:1px solid rgba(255,255,255,.7);border-radius:12px;background:#f7f7f9;box-shadow:0 24px 70px rgba(15,23,42,.22)}header,footer{display:flex;align-items:center;padding:16px 20px;background:rgba(255,255,255,.88)}header{border-bottom:1px solid #e5e7eb}header h2{margin:0;font-size:18px}header p{margin:3px 0 0;color:#6b7280;font-size:13px}.icon{margin-left:auto;border:0;background:none;cursor:pointer}.body{display:grid;gap:14px;overflow:auto;padding:18px 20px}.warning{padding:10px 12px;border:1px solid #f5d38a;border-radius:7px;background:#fff8e8;color:#8a5a00;font-size:13px;line-height:1.5}footer{justify-content:flex-end;gap:9px;border-top:1px solid #e5e7eb}footer button{display:inline-flex;height:38px;align-items:center;gap:6px;padding:0 15px;border-radius:7px;cursor:pointer}.secondary{border:1px solid #d1d5db;background:#fff}
</style>
