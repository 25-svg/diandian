<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertTriangle, ArrowRight, CheckCircle2, Circle, Loader2, RotateCcw } from "lucide-svelte";
  import type { RecordItem } from "../db";
  import type { ReviewPipelineSnapshot, ReviewPipelineStageStatus } from "../reviewPipeline";

  export let record: RecordItem;
  export let pipeline: ReviewPipelineSnapshot;

  export let retrying = false;

  const dispatch = createEventDispatcher<{ open: RecordItem; retry: RecordItem }>();

  function statusText(status: ReviewPipelineStageStatus): string {
    if (status === "done") return "已完成";
    if (status === "active") return "处理中";
    if (status === "attention") return "需要处理";
    return "等待中";
  }

  $: totalStages = pipeline.stages.length;
</script>

<article class:attention={pipeline.needsAttention} class="pipeline-card">
  <header>
    <div class="title-block">
      <div class="title-row">
        <h3>{record.title || `直播 ${record.live_id}`}</h3>
        <span class:attention={pipeline.needsAttention} class="current-state">
          {pipeline.needsAttention ? "需要处理" : `当前：${pipeline.currentLabel}`}
        </span>
      </div>
      <p>{record.anchor_name || record.room_id} · {new Date(record.created_at).toLocaleString("zh-CN")}</p>
    </div>
    <div class="header-actions">
      {#if pipeline.needsAttention}
        <button class="retry-button" type="button" disabled={retrying} on:click={() => dispatch("retry", record)}>
          {#if retrying}<Loader2 aria-hidden="true" />{:else}<RotateCcw aria-hidden="true" />{/if}
          {retrying ? "正在重试" : "重试流程"}
        </button>
      {/if}
      <button type="button" on:click={() => dispatch("open", record)}>
        进入复盘
        <ArrowRight aria-hidden="true" />
      </button>
    </div>
  </header>

  <div class="progress-summary" aria-label={`复盘进度 ${pipeline.completed}/${totalStages}`}>
    <span>{pipeline.completed}/{totalStages} 已完成</span>
    <div class="progress-track" role="progressbar" aria-valuemin="0" aria-valuemax={totalStages} aria-valuenow={pipeline.completed}>
      <div class="progress-value" style={`width:${(pipeline.completed / Math.max(totalStages, 1)) * 100}%`}></div>
    </div>
  </div>

  <ol class="stage-list">
    {#each pipeline.stages as stage}
      <li class:done={stage.status === "done"} class:active={stage.status === "active"} class:attention={stage.status === "attention"}>
        <div class="stage-icon" aria-hidden="true">
          {#if stage.status === "done"}
            <CheckCircle2 />
          {:else if stage.status === "active"}
            <Loader2 />
          {:else if stage.status === "attention"}
            <AlertTriangle />
          {:else}
            <Circle />
          {/if}
        </div>
        <div class="stage-copy">
          <strong>{stage.label}</strong>
          <small>{stage.detail}</small>
        </div>
        <span class="stage-status">{statusText(stage.status)}</span>
      </li>
    {/each}
  </ol>
</article>

<style>
  .pipeline-card{padding:18px;border:1px solid var(--mac-separator);border-radius:16px;background:var(--mac-bg-card);box-shadow:0 8px 24px rgba(15,23,42,.04)}
  .pipeline-card.attention{border-color:#f5b7b1;background:linear-gradient(135deg,var(--mac-bg-card),rgba(254,242,242,.72))}
  header{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.title-block{min-width:0}.title-row{display:flex;align-items:center;flex-wrap:wrap;gap:8px}h3{max-width:520px;margin:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--mac-text-primary);font-size:15px;font-weight:750}.title-block p{margin:5px 0 0;color:var(--mac-text-secondary);font-size:12px}.current-state{padding:4px 8px;border-radius:999px;background:#eaf3ff;color:#175cd3;font-size:11px;font-weight:700}.current-state.attention{background:#fee4e2;color:#b42318}.header-actions{display:flex;gap:8px;flex:0 0 auto}header button{min-height:40px;display:inline-flex;align-items:center;gap:6px;flex:0 0 auto;padding:0 13px;border:1px solid #b9d4ff;border-radius:10px;background:#f5f9ff;color:#175cd3;font-size:12px;font-weight:750;cursor:pointer}header button:hover{background:#eaf3ff}header button:disabled{opacity:.55;cursor:not-allowed}header button.retry-button{border-color:#f5b7b1;background:#fff5f5;color:#b42318}header button.retry-button:disabled :global(svg){animation:spin .9s linear infinite}header button:focus-visible{outline:3px solid rgba(22,119,255,.24);outline-offset:2px}header button :global(svg){width:15px;height:15px}
  .progress-summary{display:grid;grid-template-columns:auto minmax(80px,1fr);align-items:center;gap:10px;margin:15px 0 12px;color:var(--mac-text-secondary);font-size:11px}.progress-track{height:6px;overflow:hidden;border-radius:999px;background:#e5e7eb}.progress-value{height:100%;border-radius:inherit;background:linear-gradient(90deg,#1677ff,#22c55e);transition:width .3s ease}
  .stage-list{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:8px;margin:0;padding:0;list-style:none}.stage-list li{min-width:0;display:grid;grid-template-columns:20px minmax(0,1fr);grid-template-rows:auto auto;gap:1px 8px;padding:10px;border:1px solid #e5e7eb;border-radius:11px;background:rgba(248,250,252,.86)}.stage-list li.done{border-color:#b7ebc6;background:#f0fdf4}.stage-list li.active{border-color:#b9d4ff;background:#eff6ff}.stage-list li.attention{border-color:#f5b7b1;background:#fff1f0}.stage-icon{grid-row:1/3;color:#98a2b3}.stage-icon :global(svg){width:18px;height:18px}.done .stage-icon{color:#16a34a}.active .stage-icon{color:#1677ff}.active .stage-icon :global(svg){animation:spin .9s linear infinite}.attention .stage-icon{color:#d92d20}.stage-copy{min-width:0}.stage-copy strong,.stage-copy small{display:block}.stage-copy strong{color:var(--mac-text-primary);font-size:12px}.stage-copy small{margin-top:3px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--mac-text-secondary);font-size:10px}.stage-status{grid-column:2;color:#667085;font-size:10px}.done .stage-status{color:#15803d}.active .stage-status{color:#175cd3}.attention .stage-status{color:#b42318}@keyframes spin{to{transform:rotate(360deg)}}
  :global(.dark) .pipeline-card.attention{border-color:#7f1d1d;background:linear-gradient(135deg,var(--mac-bg-card),rgba(69,10,10,.28))}:global(.dark) .current-state{background:#172554;color:#bfdbfe}:global(.dark) .current-state.attention{background:#450a0a;color:#fecaca}:global(.dark) header button{border-color:#1e40af;background:#172554;color:#bfdbfe}:global(.dark) .progress-track{background:#344054}:global(.dark) .stage-list li{border-color:#344054;background:rgba(15,23,42,.45)}:global(.dark) .stage-list li.done{border-color:#236b43;background:#10261c}:global(.dark) .stage-list li.active{border-color:#1d4ed8;background:#111f44}:global(.dark) .stage-list li.attention{border-color:#7f1d1d;background:#2b1010}
  @media(max-width:980px){.stage-list{grid-template-columns:repeat(2,minmax(0,1fr))}}@media(max-width:640px){header{flex-direction:column}.header-actions{width:100%;flex-wrap:wrap}.header-actions button{flex:1 1 140px;justify-content:center}.stage-list{grid-template-columns:1fr}h3{max-width:100%}}
  @media(prefers-reduced-motion:reduce){.progress-value{transition:none}.active .stage-icon :global(svg),header button.retry-button:disabled :global(svg){animation:none}}
</style>
