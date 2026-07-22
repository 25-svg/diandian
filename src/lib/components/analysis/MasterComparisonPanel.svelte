<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { CheckCircle2, Loader2, Scale, ShieldAlert } from "lucide-svelte";
  import { masterComparisonPresentation } from "../../archiveAnalysis";
  import type { MasterBaseline, MasterComparisonResult } from "../../masterScript";
  export let baseline: MasterBaseline | null = null;
  export let selectedSectionId = 0;
  export let result: MasterComparisonResult | null = null;
  export let loading = false;
  export let error = "";
  const dispatch = createEventDispatcher();
  $: sections = baseline?.sections.filter((item) => item.sectionKind === "product" || item.sectionKind === "scenario") || [];
  $: view = result ? masterComparisonPresentation({
    admission: result.comparison.admission,
    totalScore: result.comparison.totalScore,
    reasons: result.comparison.gates?.reasons || [],
  }) : null;
</script>

<section class="master-compare">
  <header><Scale size={17}/><div><strong>与母稿相比</strong><small>{baseline ? `${baseline.master.scriptKey} · V${baseline.master.version}` : "尚未选择企业母稿"}</small></div></header>
  {#if baseline}
    <label>对应母稿位置
      <select bind:value={selectedSectionId} on:change={() => dispatch("compare", { sectionId: Number(selectedSectionId) })}>
        <option value={0}>请选择商品或场景章节</option>
        {#each sections as section}<option value={section.id}>{section.position}. {section.title}{section.productCardId ? ` · ${section.productCardId}` : ""}</option>{/each}
      </select>
    </label>
    {#if loading}<div class="state"><Loader2 class="spin" size={20}/>正在按母稿重新评分</div>
    {:else if error}<div class="state warning"><ShieldAlert size={20}/>{error}</div>
    {:else if view}
      <article class:view-success={view.tone === "success"} class:view-warning={view.tone === "warning"}>
        <div class="score">{result?.comparison.totalScore ?? "-"}<small>本地复核分</small></div>
        <div><h3>{view.label}</h3><p>{view.detail}</p></div>
      </article>
      {#if result?.comparison.improvements.length}<div class="points"><strong>比母稿更好的地方</strong>{#each result.comparison.improvements as item}<p><CheckCircle2 size={14}/>{item}</p>{/each}</div>{/if}
      {#if result?.comparison.suggestedInsertionPoint}<div class="insert"><strong>建议放到母稿哪里</strong><p>{result.comparison.suggestedInsertionPoint}</p></div>{/if}
    {:else}<div class="state">选择对应章节后开始评分</div>{/if}
  {:else}<div class="state">先在“助手”中导入并发布整场直播母稿</div>{/if}
</section>

<style>
  .master-compare{display:grid;gap:10px;margin:12px 0;padding:13px;border:1px solid rgba(0,0,0,.07);border-radius:8px;background:#f8f9fb}.master-compare header{display:flex;align-items:center;gap:8px}.master-compare header div{display:grid;gap:2px}.master-compare header small{color:#86868b;font-size:10px}.master-compare label{display:grid;gap:5px;color:#6e6e73;font-size:10px}.master-compare select{height:34px;padding:0 9px;border:1px solid #d8d9dd;border-radius:7px;background:white;color:#1d1d1f}.state{display:flex;align-items:center;gap:8px;padding:12px;color:#6e6e73;font-size:12px}.warning{color:#9a6700}article{display:flex;align-items:center;gap:12px;padding:12px;border-left:3px solid #8e8e93;background:white}.view-success{border-left-color:#34c759}.view-warning{border-left-color:#ff9f0a}.score{display:grid;min-width:52px;font-size:24px;font-weight:700}.score small{font-size:9px;font-weight:500;color:#86868b}h3,p{margin:0}h3{font-size:13px}article p,.points p,.insert p{margin-top:4px;color:#6e6e73;font-size:11px;line-height:1.5}.points,.insert{padding:10px;background:white}.points>strong,.insert>strong{font-size:11px}.points p{display:flex;gap:6px}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
</style>
