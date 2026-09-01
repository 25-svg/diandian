<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertTriangle, Bot, CheckCircle2, Film, MessageCircle, ShieldCheck, UserRound, XCircle } from "lucide-svelte";
  import { SCORE_DIMENSIONS, getTrainingModule, type HumanMachineScenario } from "../../scenarioTraining";
  import { trainingClipUrl } from "../../trainingApi";
  export let scenario: HumanMachineScenario;
  export let answer = "";
  export let busy = false;
  export let error = "";
  const dispatch = createEventDispatcher<{ submit: void; restart: void }>();
  let videoLoadFailed = false;
  $: moduleLabel = getTrainingModule(scenario.module)?.label || "待确认板块";
  $: current = scenario.turns.find((item) => item.turnIndex === scenario.currentTurn);
  $: clipUrl = trainingClipUrl(scenario.clipPath);
  $: answerWasSaved = error.includes("本轮回答已保存");
  function factEntries(): [string, string][] {
    return Object.entries(scenario.factConstraints || {}).map(([key, value]) => [key, typeof value === "string" ? value : JSON.stringify(value)]);
  }
  function scoreFor(key: string): number | null {
    const value = (scenario.scores as Record<string, unknown> | null)?.[key];
    return typeof value === "number" && value >= 1 && value <= 5 ? value : null;
  }
  function focusLabel(value?: string): string {
    return ({
      adaptive_follow_up: "承接回答",
      needs_confirmation: "需求确认",
      fact_clarification: "事实核实",
      trust_building: "信任建立",
      objection_resolution: "异议处理",
      deal_advancement: "成交推进",
      risk_compliance: "风险合规",
      live_pacing: "直播节奏",
    } as Record<string, string>)[value || ""] || "承接回答";
  }
</script>

<section class="training" data-testid={scenario.status === "completed" ? "human-machine-result" : "human-machine-training"} aria-labelledby="hm-title">
  <header class="top mac-card">
    <div><span>人机情景训练 · {moduleLabel}</span><h2 id="hm-title" tabindex="-1">{scenario.displayName} · {scenario.status === "completed" ? "本轮结果" : `第 ${scenario.currentTurn} / ${scenario.totalTurns} 轮`}</h2></div>
    <div class="progress" aria-label={`训练进度 ${scenario.currentTurn} / ${scenario.totalTurns}`}><i style={`--progress:${scenario.status === "completed" ? 100 : scenario.currentTurn / scenario.totalTurns * 100}%`}></i></div>
  </header>

  <div class="workspace">
    <section class="dialogue mac-card" aria-label={`${scenario.totalTurns}轮情景对话`}>
      {#each scenario.turns as turn}
        <article class="viewer">
          <span class="avatar"><MessageCircle size={17} aria-hidden="true" /></span>
          <div><div class="speaker"><strong>直播间观众</strong><em class:real={turn.viewerSource === "approved_comment"}>{turn.viewerSource === "approved_comment" ? "真实评论 · 已审核" : "AI模拟追问 · 非真实原话"}</em>{#if turn.viewerSource === "ai_simulated_follow_up"}<em class="focus">追问方向：{focusLabel(turn.followUpFocus)}</em>{/if}</div><p>{turn.viewerMessage}</p></div>
        </article>
        {#if turn.traineeAnswer}
          <article class="trainee"><span class="avatar"><UserRound size={17} aria-hidden="true" /></span><div><div class="speaker"><strong>你 · 主播</strong><em>训练回答</em></div><p>{turn.traineeAnswer}</p></div></article>
        {/if}
      {/each}
      {#if scenario.status === "active" && current && !current.traineeAnswer}
        <div class="composer">
          <label for="human-machine-answer">你会怎么现场回应？</label>
          <textarea id="human-machine-answer" data-testid="human-machine-answer" bind:value={answer} maxlength="4000" rows="5" aria-describedby={error ? "hm-submit-error" : undefined} placeholder="先承接问题，再确认需求；没有依据的商品事实明确说需要核实。"></textarea>
          <div><span>{answer.trim().length} / 4000</span><button type="button" class="mac-btn mac-btn-primary" disabled={!answer.trim() || busy} data-testid="submit-human-machine-turn" on:click={() => dispatch("submit")}>{busy ? "AI观众正在回应…" : answerWasSaved ? "原样重试生成追问" : scenario.currentTurn === scenario.totalTurns ? "提交并查看结果" : "提交本轮回答"}</button></div>
          {#if error}<div id="hm-submit-error" class="retry-error" role="alert" data-testid="human-machine-retry-error"><AlertTriangle size={16} aria-hidden="true" /><span>{error}{#if answerWasSaved}<small>你的回答不会被覆盖。检查 MiniMax 连接后，点击“原样重试生成追问”。</small>{/if}</span></div>{/if}
        </div>
      {/if}
    </section>

    <aside class="guardrail mac-card" aria-labelledby="guardrail-title">
      <div class="aside-title"><ShieldCheck size={18} aria-hidden="true" /><div><span>只可引用</span><h3 id="guardrail-title">已审核事实边界</h3></div></div>
      {#if factEntries().length}
        <dl>{#each factEntries() as [key, value]}<div><dt>{key}</dt><dd>{value}</dd></div>{/each}</dl>
      {:else}<p>未提供商品事实。价格、库存、赠品、售后、链接和成色都必须标记待确认。</p>{/if}
      <div class="forbidden"><AlertTriangle size={15} aria-hidden="true" /><span>禁止补造：价格、库存、赠品、售后、链接、成色及任何确定性承诺。</span></div>
    </aside>
  </div>

  {#if scenario.status === "completed"}
    <section class="result" aria-label="训练结果">
      <div class="checks mac-card">
        <h3>硬规则检查</h3>
        <div>{#each scenario.hardChecks as check}<article class:passed={check.passed}>{#if check.passed}<CheckCircle2 size={18} />{:else}<XCircle size={18} />{/if}<span><strong>{check.label}</strong><small>{check.detail}</small></span></article>{/each}</div>
      </div>
      <div class="scores mac-card">
        <h3>七维评分 {#if !scenario.scoresValid}<em>待复核</em>{/if}</h3>
        <div>{#each SCORE_DIMENSIONS as dimension}<article data-score-key={dimension.key}><span>{dimension.label}</span><strong>{scoreFor(dimension.key) ?? "待复核"}</strong>{#if scoreFor(dimension.key) !== null}<small>/ 5</small>{/if}</article>{/each}</div>
      </div>
      <div class="insights">
        <article class="mac-card"><Bot size={18} /><div><h3>最需要改的一点</h3><p>{scenario.priorityImprovement || "AI评分待人工复核"}</p></div></article>
        <article class="mac-card"><Bot size={18} /><div><h3>AI练习建议 · 非主播原话</h3><p>{scenario.coachSuggestion || "AI练习建议待复核，未生成建议。"}</p></div></article>
      </div>
      <section class="evidence mac-card">
        <header><div><span>审核证据</span><h3>当时真实回答</h3></div><code>{scenario.evidenceId}</code></header>
        <blockquote>{scenario.realAnswer}</blockquote>
        {#if clipUrl}<div class="video"><div><Film size={15} />对应原片</div><video controls preload="metadata" src={clipUrl} data-testid="human-machine-evidence-video" on:loadedmetadata={() => videoLoadFailed = false} on:error={() => videoLoadFailed = true}><track kind="captions" srclang="zh" label="中文字幕" />当前环境无法播放该视频。</video>{#if videoLoadFailed}<p role="alert">片段加载失败，请按证据编号复核原片。</p>{/if}</div>{/if}
      </section>
      <div class="preview mac-card"><strong>技能树写回预览</strong><span>本试点只展示结果，不写入主播长期技能树。待真实场景库稳定后再启用。</span><button type="button" class="mac-btn" data-testid="restart-human-machine" on:click={() => dispatch("restart")}>再练一个情景</button></div>
    </section>
  {/if}
  {#if error && scenario.status === "completed"}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .training { display: grid; gap: 14px; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 15px 17px; }
  .top span, .aside-title span, .evidence header span { color: var(--mac-blue); font-size: 10px; font-weight: 750; letter-spacing: .05em; }
  h2 { margin: 3px 0 0; color: var(--mac-label); font-size: 20px; } h3 { margin: 0; color: var(--mac-label); font-size: 13px; }
  .progress { width: 150px; height: 7px; overflow: hidden; border-radius: 999px; background: var(--mac-fill); } .progress i { display: block; width: var(--progress); height: 100%; background: var(--mac-blue); }
  .workspace { display: grid; grid-template-columns: minmax(0,1.7fr) minmax(260px,.7fr); gap: 14px; align-items: start; }
  .dialogue { display: grid; gap: 14px; padding: 17px; }
  .viewer, .trainee { display: grid; grid-template-columns: auto minmax(0,1fr); gap: 10px; max-width: 88%; }
  .trainee { justify-self: end; width: 84%; } .avatar { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 11px; background: var(--mac-fill); color: var(--mac-secondary); } .trainee .avatar { background: var(--mac-blue-soft); color: var(--mac-blue); }
  .speaker { display: flex; align-items: center; gap: 7px; } .speaker strong { font-size: 11px; } .speaker em { padding: 3px 6px; border-radius: 999px; background: color-mix(in srgb, var(--mac-orange) 10%, var(--mac-bg)); color: var(--mac-orange); font-size: 9px; font-style: normal; } .speaker em.real { background: color-mix(in srgb, var(--mac-green) 10%, var(--mac-bg)); color: var(--mac-green-solid); }
  .speaker { flex-wrap: wrap; } .speaker em.focus { background: var(--mac-blue-soft); color: var(--mac-blue); }
  .viewer p, .trainee p { margin: 5px 0 0; padding: 11px 13px; border-radius: 4px 13px 13px 13px; background: var(--mac-fill); color: var(--mac-label); font-size: 13px; line-height: 1.6; } .trainee p { background: var(--mac-blue-soft); border-radius: 13px 4px 13px 13px; }
  .composer { display: grid; gap: 7px; padding-top: 14px; border-top: 1px solid var(--mac-separator); } .composer label { color: var(--mac-label); font-size: 12px; font-weight: 700; } textarea { width: 100%; resize: vertical; box-sizing: border-box; padding: 11px; border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); color: var(--mac-label); font: inherit; line-height: 1.55; } textarea:focus { outline: 3px solid color-mix(in srgb, var(--mac-blue) 30%, transparent); border-color: var(--mac-blue); } .composer > div { display: flex; align-items: center; justify-content: space-between; } .composer > div span { color: var(--mac-tertiary); font-size: 10px; }
  .guardrail { position: sticky; top: 12px; padding: 15px; } .aside-title { display: flex; align-items: center; gap: 8px; color: var(--mac-blue); } dl { display: grid; gap: 8px; margin: 13px 0; } dl div { padding: 8px; border-radius: 8px; background: var(--mac-bg); } dt { color: var(--mac-tertiary); font-size: 9px; overflow-wrap: anywhere; } dd { margin: 3px 0 0; color: var(--mac-label); font-size: 10px; line-height: 1.45; overflow-wrap: anywhere; } .guardrail > p { color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .forbidden { display: flex; gap: 6px; padding: 9px; border-radius: 8px; background: color-mix(in srgb, var(--mac-orange) 8%, var(--mac-bg)); color: var(--mac-orange); font-size: 10px; line-height: 1.45; }
  .retry-error { display: flex; gap: 7px; padding: 10px; border: 1px solid color-mix(in srgb, var(--mac-orange) 32%, var(--mac-separator)); border-radius: 9px; background: color-mix(in srgb, var(--mac-orange) 7%, var(--mac-bg)); color: var(--mac-orange); font-size: 11px; line-height: 1.45; }
  .retry-error span, .retry-error small { display: block; } .retry-error small { margin-top: 3px; color: var(--mac-secondary); }
  .result { display: grid; gap: 14px; } .checks, .scores, .evidence { padding: 16px; } .checks > div { display: grid; grid-template-columns: repeat(3,1fr); gap: 8px; margin-top: 10px; } .checks article { display: flex; gap: 8px; padding: 10px; border-radius: 9px; background: color-mix(in srgb, var(--mac-red) 7%, var(--mac-bg)); color: var(--mac-red); } .checks article.passed { background: color-mix(in srgb, var(--mac-green) 7%, var(--mac-bg)); color: var(--mac-green-solid); } .checks strong, .checks small { display: block; } .checks strong { font-size: 11px; } .checks small { margin-top: 3px; color: var(--mac-secondary); font-size: 9px; line-height: 1.4; }
  .scores > h3 em { margin-left: 5px; color: var(--mac-orange); font-size: 10px; font-style: normal; } .scores > div { display: grid; grid-template-columns: repeat(7,1fr); gap: 7px; margin-top: 10px; } .scores article { padding: 10px 6px; border: 1px solid var(--mac-separator); border-radius: 9px; text-align: center; } .scores article span { display: block; min-height: 28px; color: var(--mac-secondary); font-size: 9px; } .scores article strong { color: var(--mac-blue); font-size: 21px; } .scores article small { color: var(--mac-tertiary); font-size: 8px; }
  .insights { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; } .insights article { display: flex; gap: 9px; padding: 14px; color: var(--mac-blue); } .insights p { margin: 4px 0 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .evidence header { display: flex; justify-content: space-between; gap: 10px; } code { padding: 5px 8px; border-radius: 7px; background: var(--mac-fill); color: var(--mac-secondary); font-size: 9px; } blockquote { margin: 13px 0 0; padding: 13px; border: 0; border-left: 3px solid var(--mac-green-solid); background: color-mix(in srgb, var(--mac-green) 6%, var(--mac-bg)); color: var(--mac-label); font-size: 13px; line-height: 1.6; } .video { margin-top: 12px; overflow: hidden; border: 1px solid var(--mac-separator); border-radius: 10px; background: #000; } .video > div { display: flex; align-items: center; gap: 5px; padding: 7px 9px; background: var(--mac-bg); color: var(--mac-secondary); font-size: 10px; } video { display: block; width: 100%; max-height: 380px; } .video p { margin: 0; padding: 8px; background: var(--mac-bg); color: var(--mac-orange); font-size: 10px; }
  .preview { display: grid; grid-template-columns: auto minmax(0,1fr) auto; align-items: center; gap: 10px; padding: 13px 15px; } .preview strong { color: var(--mac-label); font-size: 11px; } .preview span { color: var(--mac-secondary); font-size: 10px; } .error { margin: 0; color: var(--mac-red); font-size: 12px; }
  @media (max-width: 900px) { .workspace { grid-template-columns: 1fr; } .guardrail { position: static; } .scores > div { grid-template-columns: repeat(4,1fr); } }
  @media (max-width: 650px) { .top { align-items: stretch; flex-direction: column; } .progress { width: 100%; } .checks > div, .insights { grid-template-columns: 1fr; } .scores > div { grid-template-columns: repeat(2,1fr); } .preview { grid-template-columns: 1fr; } }
</style>
