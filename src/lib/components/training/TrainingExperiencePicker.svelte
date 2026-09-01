<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Bot, CheckCircle2, MessagesSquare } from "lucide-svelte";
  import type { TrainingExperience, TrainingRole } from "../../scenarioTraining";

  export let role: TrainingRole;
  const dispatch = createEventDispatcher<{ select: TrainingExperience }>();
</script>

<section class="experience" aria-labelledby="experience-title" data-testid="training-experience-picker">
  <div class="role-banner mac-card">
    <div class="role-mark" aria-hidden="true">{role.displayName.slice(0, 1)}</div>
    <div>
      <span>已选择训练角色</span>
      <h2 id="experience-title" data-testid="current-training-role" tabindex="-1">当前训练主播：{role.displayName}</h2>
      <p>先选择本轮训练形式。两种形式都只使用该主播经审核的真实案例。</p>
    </div>
    <strong>{role.availableCaseCount} 条可训练案例</strong>
  </div>

  <div class="cards" role="group" aria-label="训练形式">
    <button type="button" data-testid="experience-evidence" on:click={() => dispatch("select", "evidence_quiz")}>
      <span class="icon"><CheckCircle2 size={25} aria-hidden="true" /></span>
      <span><b>证据答题训练</b><small>逐题回答，随后查看七维评分、主播真实回答和对应原片。</small></span>
      <em>原有训练</em>
    </button>
    <button type="button" data-testid="experience-human-machine" on:click={() => dispatch("select", "human_machine")}>
      <span class="icon human"><MessagesSquare size={25} aria-hidden="true" /></span>
      <span><b>人机情景训练 · 试点</b><small>你当主播，AI当直播间观众。1轮真实评论 + 2轮受事实边界约束的模拟追问。</small></span>
      <em class="pilot"><Bot size={13} aria-hidden="true" />3轮对话</em>
    </button>
  </div>
</section>

<style>
  .experience { display: grid; gap: 16px; }
  .role-banner { display: grid; grid-template-columns: auto minmax(0,1fr) auto; align-items: center; gap: 15px; padding: 18px; }
  .role-mark { display: grid; place-items: center; width: 52px; height: 52px; border-radius: 16px; background: var(--mac-blue-soft); color: var(--mac-blue); font-size: 22px; font-weight: 800; }
  .role-banner span { color: var(--mac-blue); font-size: 10px; font-weight: 750; letter-spacing: .06em; }
  h2 { margin: 3px 0 0; color: var(--mac-label); font-size: 21px; }
  p { margin: 5px 0 0; color: var(--mac-secondary); font-size: 12px; }
  .role-banner > strong { padding: 7px 10px; border-radius: 999px; background: var(--mac-blue-soft); color: var(--mac-blue); font-size: 11px; white-space: nowrap; }
  .cards { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  button { display: grid; grid-template-columns: auto minmax(0,1fr) auto; align-items: start; gap: 13px; min-height: 132px; padding: 20px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-lg); background: var(--mac-bg-card); color: var(--mac-label); cursor: pointer; text-align: left; }
  button:hover { border-color: var(--mac-blue); box-shadow: 0 8px 24px color-mix(in srgb, var(--mac-blue) 10%, transparent); transform: translateY(-1px); }
  button:focus-visible { outline: 3px solid color-mix(in srgb, var(--mac-blue) 48%, transparent); outline-offset: 2px; }
  .icon { display: grid; place-items: center; width: 46px; height: 46px; border-radius: 14px; background: color-mix(in srgb, var(--mac-green) 10%, var(--mac-bg)); color: var(--mac-green-solid); }
  .icon.human { background: var(--mac-blue-soft); color: var(--mac-blue); }
  b { display: block; margin-top: 2px; font-size: 16px; }
  small { display: block; margin-top: 8px; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  em { padding: 5px 8px; border-radius: 999px; background: var(--mac-fill); color: var(--mac-secondary); font-size: 10px; font-style: normal; white-space: nowrap; }
  em.pilot { display: flex; align-items: center; gap: 4px; background: var(--mac-blue-soft); color: var(--mac-blue); }
  @media (max-width: 760px) { .cards { grid-template-columns: 1fr; } .role-banner { grid-template-columns: auto 1fr; } .role-banner > strong { grid-column: 1 / -1; justify-self: start; } }
  @media (prefers-reduced-motion: reduce) { button { transition: none; } }
</style>
