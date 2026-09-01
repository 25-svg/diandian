<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertTriangle, Bot, Film, ListChecks, Quote, Sparkles } from "lucide-svelte";
  import {
    SCORE_DIMENSIONS,
    getTrainingModule,
    type TrainingFeedback,
    type TrainingQuestion,
  } from "../../scenarioTraining";
  import { trainingClipUrl } from "../../trainingApi";

  export let feedback: TrainingFeedback;
  export let question: TrainingQuestion;
  export let busy = false;
  export let isLast = false;
  export let error = "";

  const dispatch = createEventDispatcher<{ next: void }>();
  let videoLoadFailed = false;
  $: clipUrl = trainingClipUrl(feedback.clipPath);
  $: moduleLabel = getTrainingModule(question.module)?.label || "待确认板块";

  function scoreFor(key: string): number | null {
    const value = (feedback.scores as Record<string, unknown>)?.[key];
    return typeof value === "number" && Number.isFinite(value) && value >= 1 && value <= 5 ? value : null;
  }
</script>

<section class="feedback" aria-labelledby="feedback-title" data-testid="training-feedback">
  <header class="feedback-header">
    <div>
      <span class="eyebrow">{moduleLabel} · 本题反馈</span>
      <h2 id="feedback-title" tabindex="-1">回答评分与真实证据</h2>
    </div>
    <button
      type="button"
      class="mac-btn mac-btn-primary"
      disabled={busy}
      data-testid={isLast ? "complete-training" : "next-question"}
      on:click={() => dispatch("next")}
    >{busy ? "正在继续…" : isLast ? "完成本轮" : "进入下一题"}</button>
  </header>

  {#if !feedback.scoresValid}
    <div class="review-warning" role="alert" data-testid="training-feedback-review">
      <AlertTriangle size={19} aria-hidden="true" />
      <span><strong>评分待复核</strong>：返回结果缺少维度或分值异常，系统未自动补造。</span>
    </div>
  {/if}

  <section class="assessment-policy mac-card" aria-labelledby="assessment-policy-title" data-testid="training-assessment-policy">
    <span class="icon" aria-hidden="true"><ListChecks size={18} /></span>
    <div>
      <h3 id="assessment-policy-title">本题判分依据</h3>
      <p>先检查已审核商品事实与禁止编造项，再按七维量表评分；缺维度、越界或无法确认时只标记待复核。</p>
    </div>
  </section>

  <section class="score-section mac-card" aria-labelledby="score-title" data-testid="training-scores">
    <h3 id="score-title">七维评分</h3>
    <div class="score-grid">
      {#each SCORE_DIMENSIONS as dimension}
        {@const score = scoreFor(dimension.key)}
        <article class:invalid={score === null} data-score-key={dimension.key}>
          <span>{dimension.label}</span>
          <strong>{score ?? "待复核"}</strong>
          {#if score !== null}<small>/ 5</small>{/if}
        </article>
      {/each}
    </div>
  </section>

  <div class="feedback-grid">
    <section class="mac-card insight priority">
      <span class="icon" aria-hidden="true"><Sparkles size={18} /></span>
      <div><h3>最需要改的一点</h3><p>{feedback.priorityImprovement || "待复核"}</p></div>
    </section>
    <section class="mac-card insight coach">
      <span class="icon" aria-hidden="true"><Bot size={18} /></span>
      <div>
        <h3>AI练习建议，非主播原话</h3>
        <p>{feedback.coachSuggestion || "待复核"}</p>
      </div>
    </section>
  </div>

  <section class="evidence mac-card" aria-labelledby="evidence-title" data-testid="training-real-evidence">
    <header>
      <span class="icon" aria-hidden="true"><Quote size={18} /></span>
      <div><span class="eyebrow">人工审核后解锁</span><h3 id="evidence-title">当时真实回答（已审核）</h3></div>
      <code>{feedback.evidenceId || "待复核"}</code>
    </header>
    <blockquote>{feedback.realAnswer || "待复核"}</blockquote>
    {#if clipUrl}
      <div class="video-wrap">
        <div class="video-label"><Film size={15} aria-hidden="true" />对应视频片段</div>
        <video
          controls
          preload="metadata"
          src={clipUrl}
          data-testid="training-evidence-video"
          aria-label={`证据 ${feedback.evidenceId} 对应视频片段`}
          on:loadedmetadata={() => (videoLoadFailed = false)}
          on:error={() => (videoLoadFailed = true)}
        >
          <track kind="captions" srclang="zh" label="中文字幕" />
          当前环境无法播放该视频，请使用证据编号复核原片。
        </video>
        {#if videoLoadFailed}
          <p class="video-error" role="alert" data-testid="training-video-error">
            片段加载失败，请按证据编号 {feedback.evidenceId} 复核原片。
          </p>
        {/if}
      </div>
    {:else}
      <p class="clip-missing">视频片段路径待复核，未展示占位证据。</p>
    {/if}
  </section>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .feedback { display: grid; gap: 14px; }
  .feedback-header { display: flex; align-items: center; justify-content: space-between; gap: 14px; }
  .eyebrow { color: var(--mac-blue); font-size: 10px; font-weight: 750; letter-spacing: .05em; }
  h2 { margin: 3px 0 0; color: var(--mac-label); font-size: 21px; letter-spacing: -.3px; }
  h3 { margin: 0; color: var(--mac-label); font-size: 13px; }
  .review-warning { display: flex; align-items: center; gap: 8px; padding: 11px 13px; border: 1px solid color-mix(in srgb, var(--mac-orange) 45%, var(--mac-separator)); border-radius: 11px; background: color-mix(in srgb, var(--mac-orange) 8%, var(--mac-bg-card)); color: var(--mac-orange); font-size: 12px; }
  .assessment-policy { display: flex; align-items: flex-start; gap: 10px; padding: 13px 15px; }
  .assessment-policy p { margin: 4px 0 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .score-section { padding: 16px; }
  .score-section > h3 { margin-bottom: 11px; }
  .score-grid { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 8px; }
  .score-grid article { min-width: 0; padding: 12px 9px; border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); text-align: center; }
  .score-grid article.invalid { border-color: color-mix(in srgb, var(--mac-orange) 40%, var(--mac-separator)); }
  .score-grid span { display: block; min-height: 32px; color: var(--mac-secondary); font-size: 10px; line-height: 1.35; }
  .score-grid strong { color: var(--mac-blue); font-size: 23px; line-height: 1; }
  .score-grid article.invalid strong { color: var(--mac-orange); font-size: 11px; }
  .score-grid small { margin-left: 2px; color: var(--mac-quaternary); font-size: 9px; }
  .feedback-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .insight { display: flex; align-items: flex-start; gap: 10px; padding: 15px; }
  .icon { flex: none; display: grid; place-items: center; width: 34px; height: 34px; border-radius: 10px; background: var(--mac-blue-soft); color: var(--mac-blue); }
  .priority .icon { background: color-mix(in srgb, var(--mac-orange) 12%, var(--mac-bg)); color: var(--mac-orange); }
  .insight p { margin: 5px 0 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  .evidence { padding: 17px; }
  .evidence > header { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 10px; }
  .evidence code { padding: 5px 8px; border-radius: 7px; background: var(--mac-fill); color: var(--mac-secondary); font-size: 10px; }
  blockquote { margin: 15px 0 0; padding: 15px; border: 0; border-left: 3px solid var(--mac-green-solid); border-radius: 0 10px 10px 0; background: color-mix(in srgb, var(--mac-green) 6%, var(--mac-bg)); color: var(--mac-label); font-size: 14px; line-height: 1.65; }
  .video-wrap { margin-top: 13px; overflow: hidden; border: 1px solid var(--mac-separator); border-radius: 12px; background: #0b0b0c; }
  .video-label { display: flex; align-items: center; gap: 6px; padding: 8px 11px; background: var(--mac-bg); color: var(--mac-secondary); font-size: 10px; }
  video { display: block; width: 100%; max-height: 420px; background: #000; }
  .video-error { margin: 0; padding: 9px 11px; background: var(--mac-bg); color: var(--mac-orange); font-size: 11px; }
  .clip-missing { margin: 12px 0 0; color: var(--mac-orange); font-size: 11px; }
  .error { margin: 0; color: var(--mac-red); font-size: 12px; }
  @media (max-width: 950px) { .score-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); } }
  @media (max-width: 660px) { .feedback-header { align-items: stretch; flex-direction: column; } .score-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } .feedback-grid { grid-template-columns: 1fr; } .evidence > header { grid-template-columns: auto 1fr; } .evidence code { grid-column: 1 / -1; justify-self: start; } }
</style>
