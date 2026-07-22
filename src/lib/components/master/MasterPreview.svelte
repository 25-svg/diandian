<script lang="ts">
  import type { MasterPreview as Preview } from "../../masterScript";
  export let preview: Preview;
  const kindLabels = { opening: "开场", product: "商品讲解", transition: "转品", scenario: "随机应对", closing: "收尾" };
  function time(ms: number): string { const seconds = Math.floor(ms / 1000); return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`; }
</script>

<div class="preview-list">
  {#each preview.draft.sections as section}
    <article>
      <header><span>{section.position}</span><strong>{kindLabels[section.kind]}</strong><small>{time(section.sourceStartMs)} - {time(section.sourceEndMs)}</small></header>
      <p>{section.masterText}</p>
      {#if section.productCardId}<footer>参数卡：{section.productCardId}</footer>{/if}
    </article>
  {/each}
</div>

<style>
  .preview-list{display:grid;gap:10px}article{padding:14px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}header{display:flex;align-items:center;gap:8px}header span{display:grid;width:22px;height:22px;place-items:center;border-radius:50%;background:#e8f2ff;color:#06c;font-size:12px}header small{margin-left:auto;color:#6b7280}p{margin:10px 0 0;white-space:pre-wrap;line-height:1.65;color:#374151}footer{margin-top:9px;color:#6b7280;font-size:12px}
</style>
