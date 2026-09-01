<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Bot, Check } from "lucide-svelte";
  import { HUMAN_MACHINE_TURN_OPTIONS, TRAINING_MODULES, type HumanMachineTurnCount, type TrainingModuleKey, type TrainingRole } from "../../scenarioTraining";
  export let role: TrainingRole;
  export let selectedModule: TrainingModuleKey | null = null;
  export let totalTurns: HumanMachineTurnCount = 3;
  export let busy = false;
  export let error = "";
  const dispatch = createEventDispatcher<{ module: TrainingModuleKey; turns: HumanMachineTurnCount; start: void }>();
</script>

<section class="setup" data-testid="human-machine-setup" aria-labelledby="hm-setup-title">
  <header class="mac-card">
    <span class="bot"><Bot size={22} aria-hidden="true" /></span>
    <div><span>人机情景训练 · 试点</span><h2 id="hm-setup-title" tabindex="-1">{role.displayName} 的真实情景</h2><p>系统从所选板块抽取一个审核通过的真实案例。没有真实案例时不会生成替代题。</p></div>
  </header>
  <section class="mac-card modules" aria-labelledby="hm-module-title">
    <div><span>直播节奏地图</span><h3 id="hm-module-title">选择本轮情景板块</h3></div>
    <div class="grid" role="group" aria-label="人机训练板块">
      {#each TRAINING_MODULES as item, index}
        {@const available = role.availableModules.includes(item.key)}
        <button type="button" class:active={selectedModule === item.key} disabled={!available} aria-pressed={selectedModule === item.key} data-testid={`human-machine-module-${item.key}`} on:click={() => dispatch("module", item.key)}>
          <span class="index">{index + 1}</span><span><b>{item.label}</b><small>{available ? item.description : "暂无真实案例"}</small></span>{#if selectedModule === item.key}<Check size={16} aria-hidden="true" />{/if}
        </button>
      {/each}
    </div>
  </section>
  <fieldset class="turn-count mac-card">
    <legend>连续追问轮数</legend>
    <p>第1轮始终来自已审核真实评论；后续每轮只根据你上一轮回答生成一个针对性追问。</p>
    <div>
      {#each HUMAN_MACHINE_TURN_OPTIONS as count}
        <button type="button" class:active={totalTurns === count} aria-pressed={totalTurns === count} data-testid={`human-machine-turns-${count}`} on:click={() => dispatch("turns", count)}>
          <strong>{count}轮</strong><small>{count === 3 ? "快速练习" : "深度追问"}</small>
        </button>
      {/each}
    </div>
  </fieldset>
  <div class="start-row">
    <p><strong>你扮演主播，AI扮演观众。</strong><br />第1轮标记“真实评论”；后续轮次标记“AI模拟追问”，不得视为主播或观众原话。</p>
    <button type="button" class="mac-btn mac-btn-primary" disabled={!selectedModule || busy} data-testid="start-human-machine" on:click={() => dispatch("start")}>{busy ? "正在准备真实情景…" : `开始${totalTurns}轮人机训练`}</button>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .setup { display: grid; gap: 15px; }
  header { display: flex; align-items: center; gap: 13px; padding: 17px; }
  .bot { display: grid; place-items: center; width: 46px; height: 46px; border-radius: 14px; background: var(--mac-blue-soft); color: var(--mac-blue); }
  header span, .modules > div:first-child > span { color: var(--mac-blue); font-size: 10px; font-weight: 750; letter-spacing: .06em; }
  h2 { margin: 3px 0 0; color: var(--mac-label); font-size: 20px; }
  h3 { margin: 3px 0 12px; color: var(--mac-label); font-size: 15px; }
  p { margin: 4px 0 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .modules { padding: 17px; }
  .turn-count { min-width: 0; margin: 0; padding: 17px; border: 1px solid var(--mac-separator); }
  .turn-count legend { padding: 0 5px; color: var(--mac-label); font-size: 13px; font-weight: 700; }
  .turn-count > div { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 8px; margin-top: 10px; }
  .turn-count button { min-height: 56px; padding: 9px 12px; border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); color: var(--mac-label); cursor: pointer; text-align: left; }
  .turn-count button.active { border-color: var(--mac-blue); background: var(--mac-blue-soft); box-shadow: inset 0 0 0 1px var(--mac-blue); }
  .turn-count strong, .turn-count small { display: block; }
  .turn-count small { margin-top: 2px; }
  .grid { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 8px; }
  .grid button { display: grid; grid-template-columns: auto minmax(0,1fr) auto; gap: 9px; min-height: 70px; padding: 10px; border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); color: var(--mac-label); cursor: pointer; text-align: left; }
  .grid button.active { border-color: var(--mac-blue); background: var(--mac-blue-soft); box-shadow: inset 0 0 0 1px var(--mac-blue); }
  .grid button:disabled { opacity: .44; cursor: not-allowed; }
  .index { display: grid; place-items: center; width: 23px; height: 23px; border-radius: 50%; background: var(--mac-fill); color: var(--mac-tertiary); font-size: 10px; font-weight: 750; }
  b { display: block; font-size: 12px; } small { display: block; margin-top: 3px; color: var(--mac-secondary); font-size: 10px; line-height: 1.35; }
  .start-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 15px 17px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-lg); background: var(--mac-bg-card); }
  .start-row button { min-height: 40px; white-space: nowrap; }
  .error { margin: 0; color: var(--mac-red); font-size: 12px; }
  button:focus-visible { outline: 3px solid color-mix(in srgb, var(--mac-blue) 48%, transparent); outline-offset: 2px; }
  @media (max-width: 700px) { .grid { grid-template-columns: 1fr; } .start-row { align-items: stretch; flex-direction: column; } }
</style>
