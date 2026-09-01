<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertCircle, Check, Layers3, Target } from "lucide-svelte";
  import {
    TRAINING_MODULES,
    type TrainingMode,
    type TrainingModuleKey,
    type TrainingRole,
  } from "../../scenarioTraining";

  export let role: TrainingRole;
  export let mode: TrainingMode = "comprehensive";
  export let selectedModule: TrainingModuleKey | null = null;
  export let busy = false;
  export let error = "";

  const dispatch = createEventDispatcher<{
    mode: TrainingMode;
    module: TrainingModuleKey;
    start: void;
  }>();

  $: canStart = role.availableCaseCount > 0 && (mode === "comprehensive" || Boolean(selectedModule));
</script>

<section class="setup" aria-labelledby="setup-title">
  <div class="role-banner mac-card">
    <div class="role-mark" aria-hidden="true">{role.displayName.slice(0, 1)}</div>
    <div>
      <span class="eyebrow">已选择训练角色</span>
      <h2 id="setup-title" data-testid="current-training-role" tabindex="-1">当前训练主播：{role.displayName}</h2>
      <p>AI只按该主播经审核的直播证据、话术特点和常见问题组织情景。</p>
    </div>
    <span class="evidence-count">{role.availableCaseCount} 条可训练案例</span>
  </div>

  {#if role.availableCaseCount === 0}
    <div class="empty-evidence" data-testid="training-empty-evidence" role="status">
      <AlertCircle size={24} aria-hidden="true" />
      <div>
        <h3>暂无经审核训练证据</h3>
        <p>该主播目前没有同时具备真实评论、对应真实回答和可播放片段的案例。系统不会调用 AI 补题，也不会串用其他主播证据。</p>
      </div>
    </div>
  {:else}
    <div class="setup-grid">
      <section class="mode-card mac-card" aria-labelledby="mode-title">
        <header><span>第二步</span><h3 id="mode-title">选择训练方式</h3></header>
        <div class="mode-options" role="group" aria-label="训练方式">
          <button
            type="button"
            class:active={mode === "comprehensive"}
            aria-pressed={mode === "comprehensive"}
            data-testid="training-mode-comprehensive"
            on:click={() => dispatch("mode", "comprehensive")}
          >
            <Layers3 size={21} aria-hidden="true" />
            <span><strong>整场综合训练</strong><small>从多个直播板块连续抽题，检验完整控场能力。</small></span>
            {#if mode === "comprehensive"}<Check size={18} class="check" aria-hidden="true" />{/if}
          </button>
          <button
            type="button"
            class:active={mode === "specialized"}
            aria-pressed={mode === "specialized"}
            data-testid="training-mode-specialized"
            on:click={() => dispatch("mode", "specialized")}
          >
            <Target size={21} aria-hidden="true" />
            <span><strong>专项训练</strong><small>只使用指定节奏板块的已审核片段与问题。</small></span>
            {#if mode === "specialized"}<Check size={18} class="check" aria-hidden="true" />{/if}
          </button>
        </div>
      </section>

      <section class="module-card mac-card" aria-labelledby="module-title" class:disabled={mode !== "specialized"}>
        <header><span>直播节奏地图</span><h3 id="module-title">选择专项板块</h3></header>
        <div class="module-grid" role="group" aria-label="专项训练板块">
          {#each TRAINING_MODULES as item, index}
            {@const available = role.availableModules.includes(item.key)}
            <button
              type="button"
              class:active={selectedModule === item.key}
              disabled={mode !== "specialized" || !available}
              aria-pressed={selectedModule === item.key}
              aria-label={`${item.label}${available ? "" : "，暂无已审核案例"}`}
              data-testid={`training-module-${item.key}`}
              on:click={() => dispatch("module", item.key)}
            >
              <span class="sequence" aria-hidden="true">{index + 1}</span>
              <span><strong>{item.label}</strong><small>{available ? item.description : "暂无合格证据"}</small></span>
            </button>
          {/each}
        </div>
      </section>
    </div>

    <div class="start-row">
      <div>
        <strong>训练规则</strong>
        <p>AI每次只抛出一个问题；你的回答提交后，才展示评分、真实回答和证据。</p>
      </div>
      <button
        type="button"
        class="mac-btn mac-btn-primary start-button"
        disabled={!canStart || busy}
        data-testid="start-training"
        on:click={() => dispatch("start")}
      >{busy ? "正在准备…" : "开始训练"}</button>
    </div>
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .setup { display: grid; gap: 18px; }
  .role-banner { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 15px; padding: 18px; }
  .role-mark { display: grid; place-items: center; width: 52px; height: 52px; border-radius: 16px; background: var(--mac-blue-soft); color: var(--mac-blue); font-size: 22px; font-weight: 800; }
  .eyebrow, header > span { display: block; color: var(--mac-blue); font-size: 10px; font-weight: 750; letter-spacing: .06em; }
  h2 { margin: 3px 0 0; color: var(--mac-label); font-size: 21px; letter-spacing: -.3px; }
  .role-banner p { margin: 5px 0 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.45; }
  .evidence-count { padding: 7px 10px; border-radius: 999px; background: var(--mac-blue-soft); color: var(--mac-blue); font-size: 11px; font-weight: 700; white-space: nowrap; }
  .setup-grid { display: grid; grid-template-columns: minmax(280px, .75fr) minmax(440px, 1.25fr); gap: 14px; }
  .mode-card, .module-card { min-width: 0; padding: 17px; }
  header h3 { margin: 4px 0 13px; color: var(--mac-label); font-size: 16px; }
  .mode-options { display: grid; gap: 9px; }
  .mode-options button {
    position: relative; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: start; gap: 11px;
    min-height: 84px; padding: 14px; border: 1px solid var(--mac-separator); border-radius: 12px;
    background: var(--mac-bg); color: var(--mac-secondary); cursor: pointer; text-align: left;
  }
  .mode-options button.active { border-color: var(--mac-blue); background: var(--mac-blue-soft); color: var(--mac-blue); box-shadow: inset 0 0 0 1px var(--mac-blue); }
  .mode-options strong, .module-grid strong { display: block; color: var(--mac-label); font-size: 13px; }
  .mode-options small, .module-grid small { display: block; margin-top: 4px; color: var(--mac-secondary); font-size: 11px; line-height: 1.4; }
  .module-card.disabled { opacity: .72; }
  .module-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .module-grid button {
    display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 8px; min-height: 70px; padding: 10px;
    border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); color: var(--mac-label);
    cursor: pointer; text-align: left;
  }
  .module-grid button.active { border-color: var(--mac-blue); background: var(--mac-blue-soft); box-shadow: inset 0 0 0 1px var(--mac-blue); }
  .module-grid button:disabled { opacity: .44; cursor: not-allowed; }
  .sequence { display: grid; place-items: center; width: 23px; height: 23px; border-radius: 50%; background: var(--mac-fill); color: var(--mac-tertiary); font-size: 10px; font-weight: 750; }
  .active .sequence { background: var(--mac-blue); color: #fff; }
  button:focus-visible { outline: 3px solid color-mix(in srgb, var(--mac-blue) 48%, transparent); outline-offset: 2px; }
  .start-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 15px 17px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-lg); background: var(--mac-bg-card); }
  .start-row strong { color: var(--mac-label); font-size: 13px; }
  .start-row p { margin: 3px 0 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.45; }
  .start-button { min-width: 116px; min-height: 40px; }
  .empty-evidence { display: flex; align-items: flex-start; gap: 13px; padding: 20px; border: 1px solid color-mix(in srgb, var(--mac-orange) 42%, var(--mac-separator)); border-radius: var(--mac-radius-lg); background: color-mix(in srgb, var(--mac-orange) 8%, var(--mac-bg-card)); color: var(--mac-orange); }
  .empty-evidence h3 { margin: 0; color: var(--mac-label); font-size: 15px; }
  .empty-evidence p { max-width: 720px; margin: 6px 0 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  .error { margin: 0; color: var(--mac-red); font-size: 12px; }
  @media (max-width: 980px) { .setup-grid { grid-template-columns: 1fr; } }
  @media (max-width: 620px) { .role-banner { grid-template-columns: auto 1fr; } .evidence-count { grid-column: 1 / -1; justify-self: start; } .module-grid { grid-template-columns: 1fr; } .start-row { align-items: stretch; flex-direction: column; } }
  @media (prefers-reduced-motion: reduce) { button { transition-duration: .001ms !important; } }
</style>
