<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { CheckCircle2, ClipboardList, RotateCcw, Target } from "lucide-svelte";
  import { getTrainingModule, type TrainingSummary } from "../../scenarioTraining";

  export let summary: TrainingSummary;
  const dispatch = createEventDispatcher<{ restart: void }>();
</script>

<section class="summary" aria-labelledby="summary-title" data-testid="training-summary">
  <header class="hero mac-card">
    <span class="success" aria-hidden="true"><CheckCircle2 size={28} /></span>
    <div>
      <span class="eyebrow">本轮训练完成</span>
      <h2 id="summary-title" tabindex="-1">{summary.displayName} · 训练总结</h2>
      <p>共完成 {summary.answeredCount} 道题。以下结论仅用于训练复盘，不会自动发布到正式知识库。</p>
    </div>
    <button type="button" class="mac-btn mac-btn-primary" on:click={() => dispatch("restart")}>
      <RotateCcw size={15} aria-hidden="true" />再练一轮
    </button>
  </header>

  <section class="mac-card section" aria-labelledby="module-score-title">
    <h3 id="module-score-title"><Target size={17} aria-hidden="true" />本次板块得分</h3>
    <div class="module-scores">
      {#each summary.moduleScores as item}
        <article><span>{getTrainingModule(item.module)?.label || item.module}</span><strong>{item.score.toFixed(1)}</strong><small>/ 5</small></article>
      {:else}
        <p class="empty">板块得分待复核。</p>
      {/each}
    </div>
  </section>

  <div class="summary-grid">
    <section class="mac-card section" aria-labelledby="issues-title">
      <h3 id="issues-title"><ClipboardList size={17} aria-hidden="true" />反复出现的问题</h3>
      <ul>
        {#each summary.recurringIssues as item}<li>{item}</li>{:else}<li>本轮未发现稳定重复问题。</li>{/each}
      </ul>
    </section>
    <section class="mac-card section" aria-labelledby="suggestions-title">
      <h3 id="suggestions-title"><Target size={17} aria-hidden="true" />下次训练建议</h3>
      <ul>
        {#each summary.nextTrainingSuggestions as item}<li>{item}</li>{:else}<li>下轮可继续综合训练，观察稳定性。</li>{/each}
      </ul>
    </section>
  </div>

  <section class="mac-card section evidence-list" aria-labelledby="summary-evidence-title">
    <h3 id="summary-evidence-title">本轮证据</h3>
    <p>所有证据均在答题后解锁；主播回答与AI建议保持分开。</p>
    <div>
      {#each summary.evidenceIds as id}<code>{id}</code>{:else}<span class="empty">证据列表待复核。</span>{/each}
    </div>
  </section>
</section>

<style>
  .summary { display: grid; gap: 14px; }
  .hero { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 13px; padding: 19px; }
  .success { display: grid; place-items: center; width: 52px; height: 52px; border-radius: 16px; background: color-mix(in srgb, var(--mac-green) 11%, var(--mac-bg)); color: var(--mac-green-solid); }
  .eyebrow { color: var(--mac-green-solid); font-size: 10px; font-weight: 750; letter-spacing: .05em; }
  h2 { margin: 3px 0 0; color: var(--mac-label); font-size: 21px; }
  .hero p { margin: 5px 0 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.45; }
  .section { padding: 16px; }
  h3 { display: flex; align-items: center; gap: 7px; margin: 0 0 12px; color: var(--mac-label); font-size: 14px; }
  .module-scores { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; }
  .module-scores article { padding: 12px; border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); }
  .module-scores span { display: block; color: var(--mac-secondary); font-size: 10px; }
  .module-scores strong { color: var(--mac-blue); font-size: 21px; } .module-scores small { color: var(--mac-tertiary); font-size: 9px; }
  .summary-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  ul { display: grid; gap: 7px; margin: 0; padding-left: 19px; color: var(--mac-secondary); font-size: 12px; line-height: 1.5; }
  .evidence-list > p { margin: -5px 0 10px; color: var(--mac-tertiary); font-size: 10px; }
  .evidence-list > div { display: flex; flex-wrap: wrap; gap: 7px; }
  code { padding: 6px 8px; border-radius: 7px; background: var(--mac-fill); color: var(--mac-secondary); font-size: 10px; }
  .empty { margin: 0; color: var(--mac-tertiary); font-size: 11px; }
  @media (max-width: 760px) { .hero { grid-template-columns: auto 1fr; } .hero button { grid-column: 1 / -1; } .summary-grid { grid-template-columns: 1fr; } .module-scores { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
