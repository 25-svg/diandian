<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { MessageCircleQuestion, ShieldCheck, UserRound } from "lucide-svelte";
  import { getTrainingModule, normalizeTrainingViewerRole, type TrainingQuestion, type TrainingSession } from "../../scenarioTraining";

  export let session: TrainingSession;
  export let question: TrainingQuestion;
  export let answer = "";
  export let busy = false;
  export let error = "";

  const dispatch = createEventDispatcher<{ submit: void }>();
  $: moduleLabel = getTrainingModule(question.module)?.label || "待确认板块";
  $: viewerRole = normalizeTrainingViewerRole(question.viewerRole);

  function handleKeydown(event: KeyboardEvent): void {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter" && answer.trim() && !busy) {
      event.preventDefault();
      dispatch("submit");
    }
  }
</script>

<section class="question-layout" aria-labelledby="question-title">
  <aside class="context-card mac-card">
    <span class="eyebrow">当前训练主播</span>
    <strong>{session.displayName}</strong>
    <dl>
      <div><dt>训练方式</dt><dd>{session.mode === "specialized" ? "专项训练" : "整场综合训练"}</dd></div>
      <div><dt>当前板块</dt><dd data-testid="training-module-label">{moduleLabel}</dd></div>
      <div><dt>进度</dt><dd>{Math.min(session.currentIndex + 1, session.totalQuestions)} / {session.totalQuestions}</dd></div>
    </dl>
    <div class="guardrail"><ShieldCheck size={17} aria-hidden="true" /><span>商品事实不足时，只能追问或标记待确认。</span></div>
  </aside>

  <div class="question-card mac-card" data-testid="training-question">
    <header>
      <span class="icon" aria-hidden="true"><MessageCircleQuestion size={21} /></span>
      <div><span class="eyebrow">模拟观众提问 · 源自已审核真实评论</span><h2 id="question-title" tabindex="-1">现场问题</h2></div>
    </header>
    <div class="question-meta" aria-label="训练题目来源">
      <span><UserRound size={14} aria-hidden="true" />虚拟人物：{viewerRole}</span>
      <span><ShieldCheck size={14} aria-hidden="true" />题目来源：已审核评论题库</span>
    </div>
    <blockquote>{question.prompt}</blockquote>
    <p class="disclaimer">这是训练情景。题面用于虚拟人物提问，不代表主播说过这句话；提交前不会展示参考回答。</p>

    <form on:submit|preventDefault={() => dispatch("submit")}>
      <label for="training-answer">你的现场回答</label>
      <textarea
        id="training-answer"
        class="mac-field"
        bind:value={answer}
        data-testid="training-answer"
        rows="6"
        maxlength="1800"
        placeholder="像正在直播一样直接回答观众…"
        disabled={busy}
        on:keydown={handleKeydown}
      ></textarea>
      <div class="answer-footer">
        <span>{answer.length} / 1800 · Ctrl/⌘ + Enter 提交</span>
        <button
          type="submit"
          class="mac-btn mac-btn-primary submit-button"
          disabled={!answer.trim() || busy}
          data-testid="submit-answer"
        >{busy ? "正在评分…" : "提交回答"}</button>
      </div>
    </form>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</section>

<style>
  .question-layout { display: grid; grid-template-columns: 240px minmax(0, 1fr); align-items: start; gap: 14px; }
  .context-card { position: sticky; top: 0; padding: 17px; }
  .eyebrow { color: var(--mac-blue); font-size: 10px; font-weight: 750; letter-spacing: .05em; }
  .context-card > strong { display: block; margin-top: 4px; color: var(--mac-label); font-size: 20px; }
  dl { display: grid; gap: 9px; margin: 17px 0; }
  dl div { display: flex; justify-content: space-between; gap: 8px; padding-top: 9px; border-top: 1px solid var(--mac-separator); }
  dt { color: var(--mac-tertiary); font-size: 11px; } dd { margin: 0; color: var(--mac-label); font-size: 11px; font-weight: 650; text-align: right; }
  .guardrail { display: flex; align-items: flex-start; gap: 7px; padding: 10px; border-radius: 10px; background: color-mix(in srgb, var(--mac-green) 8%, var(--mac-bg)); color: var(--mac-green-solid); }
  .guardrail span { color: var(--mac-secondary); font-size: 10px; line-height: 1.45; }
  .question-card { padding: 20px; }
  .question-card header { display: flex; align-items: center; gap: 10px; }
  .icon { display: grid; place-items: center; width: 40px; height: 40px; border-radius: 12px; background: var(--mac-blue-soft); color: var(--mac-blue); }
  h2 { margin: 2px 0 0; color: var(--mac-label); font-size: 18px; }
  blockquote { margin: 20px 0 8px; padding: 18px; border: 0; border-left: 4px solid var(--mac-blue); border-radius: 0 12px 12px 0; background: var(--mac-bg); color: var(--mac-label); font-size: clamp(17px, 2vw, 22px); font-weight: 650; line-height: 1.55; }
  .question-meta { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 14px; }
  .question-meta span { display: inline-flex; align-items: center; gap: 5px; min-height: 30px; padding: 0 9px; border: 1px solid var(--mac-separator); border-radius: 999px; background: var(--mac-fill); color: var(--mac-secondary); font-size: 10px; font-weight: 650; }
  .disclaimer { margin: 0 0 20px; color: var(--mac-tertiary); font-size: 10px; line-height: 1.45; }
  form { display: grid; gap: 7px; }
  label { color: var(--mac-label); font-size: 12px; font-weight: 700; }
  textarea.mac-field { width: 100%; min-height: 150px; box-sizing: border-box; font-size: 14px; line-height: 1.6; }
  .answer-footer { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .answer-footer > span { color: var(--mac-tertiary); font-size: 10px; }
  .submit-button { min-height: 40px; min-width: 116px; }
  .error { margin: 10px 0 0; color: var(--mac-red); font-size: 12px; }
  @media (max-width: 800px) { .question-layout { grid-template-columns: 1fr; } .context-card { position: static; } }
  @media (max-width: 520px) { .answer-footer { align-items: stretch; flex-direction: column; } }
</style>
