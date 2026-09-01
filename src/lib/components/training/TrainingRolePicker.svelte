<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertCircle, CheckCircle2, UserRound } from "lucide-svelte";
  import type { TrainingRole } from "../../scenarioTraining";

  export let roles: TrainingRole[] = [];
  export let selectedRoleId: string | null = null;
  export let loading = false;

  const dispatch = createEventDispatcher<{ select: TrainingRole; retry: void }>();
</script>

<section class="role-picker" data-testid="training-role-selection" aria-labelledby="role-picker-title">
  <header class="intro">
    <span class="eyebrow">第一步 · 选择训练角色</span>
    <h2 id="role-picker-title">你想按照谁的情景训练？</h2>
    <p>每道题只取自该主播经人工审核的真实评论、对应回答和可播放片段。</p>
  </header>

  {#if loading}
    <div class="status-card" role="status" aria-live="polite">正在读取已审核训练角色…</div>
  {:else if roles.length === 0}
    <div class="status-card empty" role="status" data-testid="training-empty-evidence">
      <AlertCircle size={22} aria-hidden="true" />
      <div>
        <strong>暂无可展示的训练角色</strong>
        <p>未发现经审核的评论、回答和视频片段证据。</p>
      </div>
      <button type="button" class="mac-btn" on:click={() => dispatch("retry")}>重新读取</button>
    </div>
  {:else}
    <div class="role-grid" aria-label="可选择的训练主播">
      {#each roles as role}
        {@const selected = selectedRoleId === role.id}
        {@const dimmed = Boolean(selectedRoleId && !selected)}
        <button
          type="button"
          class:selected
          class:dimmed
          class="role-card"
          aria-pressed={selected}
          aria-label={`按照${role.displayName}的情景训练，${role.availableCaseCount > 0 ? `${role.availableCaseCount}条已审核案例` : "暂无经审核训练证据"}`}
          data-testid={`role-card-${role.id}`}
          data-role-id={role.id}
          data-selected={selected ? "true" : "false"}
          data-dimmed={dimmed ? "true" : "false"}
          on:click={() => dispatch("select", role)}
        >
          <span
            class="role-visual"
            data-testid={`role-visual-${role.id}`}
            aria-hidden="true"
            style={role.avatarUrl ? `background-image:url('${role.avatarUrl.replace(/'/g, "%27")}')` : ""}
          >
            {#if !role.avatarUrl}
              <UserRound size={38} strokeWidth={1.7} />
            {/if}
            <span class="evidence-pill" class:empty={role.availableCaseCount === 0}>
              {#if role.availableCaseCount > 0}
                <CheckCircle2 size={13} />{role.availableCaseCount} 条
              {:else}
                <AlertCircle size={13} />待补证据
              {/if}
            </span>
          </span>

          <span class="role-copy">
            <span class="role-name" data-testid={`role-name-${role.id}`}>{role.displayName}</span>
            {#if role.sourceLabel}<span class="source-label">{role.sourceLabel}</span>{/if}
            <span class="description">{role.description || "使用该主播经审核案例开展现场应答训练。"}</span>
          </span>
          <span class="select-hint">{selected ? "正在进入训练主页" : "选择该主播"}</span>
        </button>
      {/each}
    </div>
  {/if}
</section>

<style>
  .role-picker { display: grid; gap: 22px; }
  .intro { max-width: 720px; }
  .eyebrow {
    display: block; margin-bottom: 7px; color: var(--mac-blue); font-size: 11px;
    font-weight: 750; letter-spacing: .08em; text-transform: uppercase;
  }
  h2 { margin: 0; color: var(--mac-label); font-size: clamp(22px, 3vw, 32px); line-height: 1.16; letter-spacing: -.55px; }
  .intro p { margin: 9px 0 0; color: var(--mac-secondary); font-size: 14px; line-height: 1.55; }
  .role-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 14px; }
  .role-card {
    min-width: 0; min-height: 322px; padding: 0; overflow: hidden; border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-xl); background: var(--mac-bg-card); box-shadow: var(--mac-shadow-sm);
    color: var(--mac-label); cursor: pointer; text-align: left;
    transition: transform 220ms cubic-bezier(.22,.8,.3,1), border-color 180ms ease, box-shadow 220ms ease, background 180ms ease;
  }
  .role-card:hover { transform: translateY(-3px); border-color: color-mix(in srgb, var(--mac-blue) 36%, var(--mac-separator)); box-shadow: var(--mac-shadow-md); }
  .role-card.selected { z-index: 1; transform: translateY(-7px) scale(1.015); border-color: var(--mac-blue); box-shadow: 0 18px 42px color-mix(in srgb, var(--mac-blue) 20%, transparent); }
  .role-card.dimmed { border-color: transparent; background: color-mix(in srgb, var(--mac-bg-card) 82%, var(--mac-bg)); }
  .role-card:focus-visible { outline: 3px solid color-mix(in srgb, var(--mac-blue) 52%, transparent); outline-offset: 3px; }
  .role-visual {
    position: relative; display: grid; place-items: center; height: 160px; overflow: hidden;
    background-color: var(--mac-blue-soft); background-position: center; background-size: cover; color: var(--mac-blue);
    transition: filter 220ms ease, opacity 220ms ease, transform 220ms cubic-bezier(.22,.8,.3,1);
  }
  .role-card:nth-child(2n) .role-visual { background-color: color-mix(in srgb, var(--mac-purple) 13%, var(--mac-bg-card)); color: var(--mac-purple); }
  .role-card:nth-child(3n) .role-visual { background-color: color-mix(in srgb, var(--mac-orange) 13%, var(--mac-bg-card)); color: var(--mac-orange); }
  .role-card.dimmed .role-visual { filter: blur(1.6px) brightness(.68) saturate(.55); opacity: .62; transform: scale(.96); }
  .role-card.selected .role-visual { transform: scale(1.035); }
  .evidence-pill {
    position: absolute; right: 10px; bottom: 10px; display: inline-flex; align-items: center; gap: 4px;
    min-height: 26px; padding: 0 9px; border: 1px solid color-mix(in srgb, var(--mac-green) 40%, transparent);
    border-radius: 999px; background: color-mix(in srgb, var(--mac-bg-card) 90%, transparent);
    color: var(--mac-green-solid); font-size: 11px; font-weight: 700; backdrop-filter: blur(10px);
  }
  .evidence-pill.empty { border-color: color-mix(in srgb, var(--mac-orange) 45%, transparent); color: var(--mac-orange); }
  .role-copy { display: grid; gap: 5px; min-height: 112px; padding: 16px 16px 10px; }
  .role-name { color: var(--mac-label); font-size: 18px; font-weight: 750; letter-spacing: -.25px; line-height: 1.25; }
  .source-label { color: var(--mac-orange); font-size: 10px; font-weight: 650; }
  .description { color: var(--mac-secondary); font-size: 12px; line-height: 1.5; }
  .select-hint { display: block; padding: 0 16px 16px; color: var(--mac-blue); font-size: 12px; font-weight: 650; }
  .status-card {
    display: flex; align-items: center; gap: 12px; min-height: 96px; padding: 18px;
    border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-lg); background: var(--mac-bg-card);
    color: var(--mac-secondary);
  }
  .status-card.empty { color: var(--mac-orange); }
  .status-card div { flex: 1; }
  .status-card strong { color: var(--mac-label); }
  .status-card p { margin: 4px 0 0; color: var(--mac-secondary); font-size: 12px; }
  @media (max-width: 1120px) { .role-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 620px) { .role-grid { grid-template-columns: 1fr; } .role-card { min-height: 280px; } }
  @media (prefers-reduced-motion: reduce) {
    .role-card, .role-visual { transition-duration: .001ms !important; }
    .role-card:hover, .role-card.selected, .role-card.selected .role-visual { transform: none; }
  }
</style>
