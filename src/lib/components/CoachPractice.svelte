<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { knowledgeQuestions, sensitiveQuestions, popularCameraQuestions, popularCameraQuestionsFor, companyFoundationQuestions, beginnerCameraLessons, evaluateHotspot, parseFeedback, practicePrompt, practiceKey, objectivePrompt, parseObjectiveQuestion, applyObjectiveFeedback, type QuestionKind, type PracticeQuestion, type PracticeFeedback } from '../coachPractice';
  export let hostId: string;
  export let hostName: string;
  export let title = '';
  let mode = '';
  let questionKind: QuestionKind | 'mixed' = 'open';
  let questions: PracticeQuestion[] = [];
  let index = 0;
  let answer = '';
  let feedback: PracticeFeedback | null = null;
  let busy = false;
  let error = '';
  let finished = false;
  let generation = 0;
  let results: Array<{question: PracticeQuestion; answer: string; feedback: PracticeFeedback}> = [];
  let history: Array<{mode: string; date: string; results: typeof results}> = [];
  let lessonIndex = 0;
  let beginnerStep: 'learn' | 'locate' | 'result' = 'learn';
  let hotspotCorrect = false;
  let imageReady = false;
  $: question = questions[index];
  $: lesson = beginnerCameraLessons[lessonIndex];
  $: resetHost(hostId);
  function resetHost(id: string) {
    generation++; mode = ''; questions = []; results = []; feedback = null; answer = ''; busy = false; error = ''; finished = false; lessonIndex = 0; beginnerStep = 'learn'; hotspotCorrect = false; imageReady = false;
    try { const value = JSON.parse(localStorage.getItem(practiceKey(id)) || '[]'); history = Array.isArray(value) ? value.filter(v => typeof v.mode === 'string' && typeof v.date === 'string' && Array.isArray(v.results)) : []; } catch { history = []; }
  }
  async function start(nextMode: string) {
    const token = ++generation;
    mode = nextMode; busy = true; error = ''; finished = false; results = []; questions = []; feedback = null; index = 0; answer = '';
    if (nextMode === '零基础认机') { busy = false; beginnerStep = 'learn'; lessonIndex = 0; hotspotCorrect = false; imageReady = false; return; }
    try {
      let loaded = nextMode === '直播敏感词训练'
        ? sensitiveQuestions
        : nextMode === '公司基础课'
          ? companyFoundationQuestions
        : nextMode === '评论情景训练'
          ? popularCameraQuestions.filter(question => question.id.startsWith('comment-'))
        : nextMode === '常卖机型训练'
          ? popularCameraQuestionsFor(questionKind)
          : knowledgeQuestions(await invoke('get_coach_knowledge_cards'));
      if (token !== generation) return;
      const selectedKind = questionKind;
      if (nextMode === '基础知识问答' && selectedKind !== 'open') {
        const generated: PracticeQuestion[] = [];
        for (const [i, base] of loaded.slice(0,6).entries()) {
          const kind = selectedKind === 'mixed' ? (['choice','boolean','open'] as const)[i % 3] : selectedKind;
          if (kind === 'open') generated.push({...base, kind});
          else {
            const raw = await invoke<string>('minimax_training_json', {systemPrompt:objectivePrompt(base,kind),messages:[{role:'user',content:'请生成一道题。'}],schema:'question'});
            if (token !== generation) return;
            try { generated.push(parseObjectiveQuestion(raw,base,kind)); } catch { /* Skip unsupported or malformed questions. */ }
          }
        }
        loaded = generated;
      }
      if (token !== generation) return;
      questions = loaded;
      if (!loaded.length) error = '知识库暂无可用知识卡。请在设置中连接并同步知识库，确认卡片审核状态后重试。';
    } catch { if (token === generation) error = '题目加载失败，请检查知识库与模型连接后重试。'; }
    finally { if (token === generation) busy = false; }
  }
  async function submit() {
    if (busy || feedback || !answer.trim() || !question) return;
    const token = generation;
    busy = true; error = '';
    try {
      const raw = await invoke<string>('minimax_training_json', {systemPrompt:practicePrompt(question),messages:[{role:'user',content:JSON.stringify({question:question.question,answer:answer.trim()})}],schema:'feedback'});
      if (token !== generation) return;
      feedback = applyObjectiveFeedback(question, answer, parseFeedback(raw, question));
      results = [...results, {question, answer: question.options ? `${answer} · ${question.options[answer.charCodeAt(0)-65]}` : answer.trim(), feedback}];
    } catch { if (token === generation) error = '点评失败，可保留答案重试；本次未计分。'; }
    finally { if (token === generation) busy = false; }
  }
  function finish() {
    if (finished || !results.length) return;
    generation++; busy = false; finished = true;
    history = [{mode, date:new Date().toISOString(), results}, ...history].slice(0,20);
    try { localStorage.setItem(practiceKey(hostId), JSON.stringify(history)); } catch { error = '本次总结已显示，但本机记录保存失败。'; }
  }
  function next() { if (index + 1 >= questions.length) finish(); else { index++; answer = ''; feedback = null; error = ''; } }
  function startLocate() { beginnerStep = 'locate'; hotspotCorrect = false; }
  function revealHotspot() { beginnerStep = 'result'; hotspotCorrect = false; }
  function checkHotspot(event: MouseEvent) {
    if (event.detail === 0) { revealHotspot(); return; }
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    hotspotCorrect = evaluateHotspot({x:(event.clientX-rect.left)/rect.width*100,y:(event.clientY-rect.top)/rect.height*100},lesson.hotspot);
    beginnerStep = 'result';
  }
  function nextLesson() { lessonIndex = (lessonIndex + 1) % beginnerCameraLessons.length; beginnerStep = 'learn'; hotspotCorrect = false; imageReady = false; }
  function averageResult(items: typeof results): string {
    const scores = items.map(item => item.feedback.score).filter((score): score is number => typeof score === 'number');
    return scores.length ? `${Math.round(scores.reduce((total,score)=>total+score,0)/scores.length)} 分（${scores.length} 题计分）` : '本轮没有可计分题目';
  }
</script>

<section class="practice mac-card" aria-label="主播基础跟练">
  <h3>{title || `基础跟练 · ${hostName}`}</h3>
  <div class="actions">
    <label>基础题型 <select aria-label="基础题型" bind:value={questionKind} disabled={busy}><option value="open">问答题</option><option value="choice">选择题</option><option value="boolean">判断题</option><option value="mixed">混合练习</option></select></label>
    <button class="mac-btn" on:click={() => start('零基础认机')}>零基础认机</button>
    <button class="mac-btn" on:click={() => start('公司基础课')}>公司基础课</button>
    <button class="mac-btn" on:click={() => start('常卖机型训练')}>常卖机型训练</button>
    <button class="mac-btn" on:click={() => start('评论情景训练')}>评论情景训练</button>
    <button class="mac-btn" on:click={() => start('基础知识问答')}>基础知识问答</button>
    <button class="mac-btn" on:click={() => start('直播敏感词训练')}>直播敏感词训练</button>
  </div>
  <p>零基础认机覆盖 12 款常卖机型，使用内部参数卡原图并以品牌官方参数校准；先学知识点、顾客痛点、可讲卖点和使用边界，再做看图定位。其他训练每次一题，答完再看参考。</p>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if busy}<p role="status">{question ? '教练正在点评…' : '正在读取题目…'}</p>{/if}
  {#if mode === '零基础认机' && lesson}
    <div class="beginner-head">
      <div><span class="eyebrow">看图认识 · {lessonIndex+1}/{beginnerCameraLessons.length}</span><h4>{lesson.model}</h4><p class="aliases">直播常用叫法：{lesson.aliases}</p></div>
      <button class="mac-btn" on:click={nextLesson}>下一款相机</button>
    </div>
    <div class="beginner-grid" aria-live="polite">
      <div>
        <button class="camera-image" type="button" aria-label={beginnerStep === 'locate' ? '在相机图片上选择快门按钮' : `${lesson.model} 学习图片`} disabled={beginnerStep === 'learn'} on:click={checkHotspot}>
          <img src={lesson.image} alt={lesson.imageAlt} width="1500" height="1500" on:load={() => imageReady = true} />
          {#if beginnerStep !== 'locate'}
            <span class:incorrect={beginnerStep === 'result' && !hotspotCorrect} class:correct={beginnerStep === 'result' && hotspotCorrect} class="hotspot" style={`left:${lesson.hotspot.x}%;top:${lesson.hotspot.y}%`}><span>{lesson.hotspot.label}</span></span>
          {/if}
        </button>
        <p class="source-note">图片来源：{lesson.cardSource}</p>
      </div>
      <div class="lesson-copy">
        {#if beginnerStep === 'learn'}
          <h4>主播先记住</h4>
          <p>先看参数结论，再理解它能解决顾客什么问题。商品成色、配件和库存仍以当前场次资料为准。</p>
          <dl>{#each lesson.facts as fact}<div><dt>{fact.label}</dt><dd>{fact.value}</dd></div>{/each}</dl>
          <div class="knowledge-list" aria-label="小白知识点">
            {#each lesson.knowledgePoints as point}
              <article><h5>{point.title}</h5><p>{point.plainMeaning}</p></article>
            {/each}
          </div>
          <h4>顾客常见痛点</h4>
          <div class="pain-list">
            {#each lesson.painPoints as point}
              <article>
                <h5>{point.customerNeed}</h5>
                <p><strong>顾客担心：</strong>{point.concern}</p>
                <p><strong>回答重点：</strong>{point.responseFocus}</p>
              </article>
            {/each}
          </div>
          <h4>卖点怎么理解</h4>
          <div class="selling-list">
            {#each lesson.sellingPoints as point}
              <article>
                <h5>{point.title}</h5>
                <p><strong>参数依据：</strong>{point.parameterBasis}</p>
                <p><strong>顾客得到：</strong>{point.customerBenefit}</p>
              </article>
            {/each}
          </div>
          <h4>可以这样讲</h4>
          <blockquote class="live-pitch">{lesson.livePitch}</blockquote>
          <h4>使用边界</h4>
          <ul class="boundary-list">{#each lesson.boundaries as boundary}<li>{boundary}</li>{/each}</ul>
          <div class="source-links">
            <a class="official-link" href={lesson.officialSourceUrl} target="_blank" rel="noreferrer">查看品牌官方参数</a>
          </div>
          <button class="mac-btn mac-btn-primary" on:click={startLocate} disabled={!imageReady}>{imageReady ? '开始找快门按钮' : '图片加载中…'}</button>
        {:else if beginnerStep === 'locate'}
          <h4>请在相机图片上点出快门按钮</h4>
          <p>鼠标或触屏直接点图片。键盘用户可使用下面的查看按钮。</p>
          <button class="mac-btn" on:click={revealHotspot}>显示快门按钮位置</button>
        {:else}
          <div class="result" role="status">
            <h4>{hotspotCorrect ? '找对了' : '快门按钮位置已标出'}</h4>
            <p>{hotspotCorrect ? '这是拍照时最常用的按键。' : '看图片中的圆圈和标签，再记一遍位置。'}</p>
          </div>
          <button class="mac-btn" on:click={startLocate}>再找一次</button>
          <button class="mac-btn mac-btn-primary" on:click={nextLesson}>学习下一款</button>
        {/if}
        {#if lesson.partsDiagramUrl}
          <details class="official-parts"><summary>展开品牌官方部件图</summary><img src={lesson.partsDiagramUrl} alt={`${lesson.model} 品牌官方部件图`} loading="lazy" /></details>
        {/if}
        {#if lesson.partsSourceUrl}<a class="official-link" href={lesson.partsSourceUrl} target="_blank" rel="noreferrer">查看佳能官方部件图</a>{/if}
      </div>
    </div>
  {:else if finished}
    <h4>本次训练总结</h4>
    <p>{mode} · 已完成 {results.length} 题 · 平均分 {averageResult(results)}</p>
    <h4>错题与复习重点（低于 80 分）</h4>
    {#each results.filter(r => typeof r.feedback.score === 'number' && r.feedback.score < 80) as result}<details><summary>{result.question.question}</summary><p>{result.feedback.keyImprovement || result.feedback.feedback}</p><p>{result.feedback.reference}</p><p>依据：{result.question.source}</p><blockquote>{result.feedback.evidenceQuote || result.question.evidence}</blockquote></details>{:else}<p>本轮没有低于 80 分的题目，可继续练习其他知识卡。</p>{/each}
    <p>下次建议：先重答低分题，再对照来源检查遗漏点。评分为 AI 训练反馈。</p>
  {:else if question}
    <h4>{mode} · 第 {index+1}/{questions.length} 题</h4>
    {#if question.media}<figure class="question-media"><img src={question.media.image} alt={question.media.alt} loading="eager" /><figcaption>{question.media.caption}</figcaption></figure>{/if}
    <p>{question.question}</p>
    <form on:submit|preventDefault={submit}>
      {#if question.options}
      <fieldset disabled={busy || !!feedback}><legend>请选择答案</legend>{#each question.options as option, i}<label class="option"><input type="radio" name="practice-option" value={String.fromCharCode(65+i)} bind:group={answer} /> {String.fromCharCode(65+i)}. {option}</label>{/each}</fieldset>
      {:else}
      <label for="practice-answer">你的现场回答</label>
      <p class="voice-hint">可以说话作答：点下方输入框，按 Win＋H 启用 Windows 语音输入（需麦克风、网络及系统权限，无需音响）。核对型号和文字后再点“提交回答”。本程序不自动录音；无法使用时直接打字。</p>
      <textarea id="practice-answer" class="mac-field" bind:value={answer} disabled={busy || !!feedback} maxlength="6000" rows="4"></textarea>
      {/if}
      <button class="mac-btn mac-btn-primary" type="submit" disabled={busy || !!feedback || !answer.trim()}>提交回答</button>
    </form>
    {#if feedback}<section class="analysis-card" aria-live="polite" aria-label="本题精准分析">
      <div class="analysis-head"><h4>教练反馈 · {feedback.score === null ? '本题不计分' : `${feedback.score} 分`}</h4><span class="verdict">{feedback.verdict}</span></div>
      <h4>事实核对</h4>
      <div class="fact-grid">
        <div><strong>说对了</strong>{#if feedback.correctPoints.length}<ul>{#each feedback.correctPoints as point}<li>{point}</li>{/each}</ul>{:else}<p>本题没有可确认的正确点。</p>{/if}</div>
        <div><strong>需要纠正</strong>{#if feedback.incorrectPoints.length}<ul>{#each feedback.incorrectPoints as point}<li>{point}</li>{/each}</ul>{:else}<p>没有发现与依据冲突的说法。</p>{/if}</div>
        <div><strong>还可补充</strong>{#if feedback.missingPoints.length}<ul>{#each feedback.missingPoints as point}<li>{point}</li>{/each}</ul>{:else}<p>关键事实没有明显遗漏。</p>{/if}</div>
      </div>
      <div class="coach-action"><h4>最需要改的一点</h4><p>{feedback.keyImprovement}</p><h4>为什么影响直播</h4><p>{feedback.liveImpact}</p></div>
      <h4>AI 参考回答</h4><p>{feedback.reference}</p>
      <h4>教练继续追问</h4><p>{feedback.followUp}</p>
      <details><summary>查看本次判定依据</summary><p>{question.source}</p><blockquote>{feedback.evidenceQuote}</blockquote><details><summary>查看完整资料</summary><blockquote>{question.evidence}</blockquote></details></details>
      <button class="mac-btn" on:click={next}>{index+1 === questions.length ? '查看总结' : '下一题'}</button>
    </section>{/if}
    {#if results.length}<button class="mac-btn" on:click={finish} disabled={busy}>结束并总结</button>{/if}
  {/if}
    {#if history.length}<details><summary>本主播历史训练 · {history.length} 轮</summary>{#each history as item}<details><summary>{new Date(item.date).toLocaleString()} · {item.mode} · {item.results.length} 题</summary>{#each item.results as result}{#if result?.question?.question && result?.feedback}<h4>{result.question.question}</h4><p>你的回答：{result.answer}</p><p>{result.feedback.score} 分 · {result.feedback.keyImprovement || result.feedback.feedback}</p><p>AI 参考：{result.feedback.reference}</p><p>依据：{result.question.source}</p><blockquote>{result.feedback.evidenceQuote || result.question.evidence}</blockquote>{/if}{/each}</details>{/each}</details>{/if}
</section>

<style>
  .practice { padding:18px; margin-top:12px; min-width:0; overflow-wrap:anywhere; }
  h3,h4 { margin:8px 0; } p,blockquote { white-space:pre-wrap; line-height:1.65; }
  .actions { display:flex; flex-wrap:wrap; gap:8px; }
  form { display:grid; gap:10px; } textarea { width:100%; box-sizing:border-box; }
  fieldset { min-width:0; border:1px solid var(--mac-border); border-radius:8px; } .option { display:flex; align-items:baseline; gap:8px; padding:10px 0; } select { max-width:100%; padding:8px; }
  button { margin:6px 0; } details { margin-top:12px; } summary { cursor:pointer; }
  blockquote { margin:8px 0; padding:12px; background:var(--mac-fill); }
  .beginner-head { display:flex; align-items:flex-start; justify-content:space-between; gap:16px; margin-top:16px; }
  .eyebrow { color:var(--mac-blue); font-size:13px; font-weight:700; } .aliases,.source-note { color:var(--mac-text-secondary); font-size:13px; }
  .beginner-grid { display:grid; grid-template-columns:minmax(280px,1.2fr) minmax(260px,1fr); gap:20px; align-items:start; }
  .camera-image { position:relative; display:block; width:100%; padding:0; overflow:hidden; border:1px solid var(--mac-border); border-radius:14px; background:#eef2f6; cursor:crosshair; }
  .camera-image:disabled { cursor:default; opacity:1; }
  .camera-image img { display:block; width:100%; height:auto; aspect-ratio:1; object-fit:contain; }
  .hotspot { position:absolute; width:38px; height:38px; border:3px solid #0878df; border-radius:50%; transform:translate(-50%,-50%); box-shadow:0 0 0 5px rgba(8,120,223,.2); pointer-events:none; }
  .hotspot span { position:absolute; left:50%; top:calc(100% + 8px); transform:translateX(-50%); white-space:nowrap; color:#fff; background:#075ea9; border-radius:6px; padding:4px 8px; font-size:13px; font-weight:700; }
  .hotspot.incorrect { border-color:#c2410c; box-shadow:0 0 0 5px rgba(194,65,12,.2); } .hotspot.correct { border-color:#08783e; box-shadow:0 0 0 5px rgba(8,120,62,.2); }
  .lesson-copy { padding:4px 0; } dl { display:grid; gap:8px; margin:12px 0 16px; } dl div { padding:10px 12px; border:1px solid var(--mac-border); border-radius:10px; background:var(--mac-fill); } dt { font-size:13px; color:var(--mac-text-secondary); } dd { margin:4px 0 0; font-weight:600; line-height:1.5; }
  h5 { margin:0 0 6px; font-size:15px; line-height:1.4; }
  .knowledge-list,.pain-list,.selling-list { display:grid; gap:8px; margin:10px 0 18px; }
  .knowledge-list article,.pain-list article,.selling-list article { padding:12px; border:1px solid var(--mac-border); border-radius:10px; background:var(--mac-fill); }
  .knowledge-list p,.pain-list p,.selling-list p { margin:4px 0 0; }
  .live-pitch { border-left:4px solid var(--mac-blue); background:var(--mac-fill); }
  .boundary-list { margin:8px 0 16px; padding-left:22px; line-height:1.65; }
  .source-links { display:flex; flex-wrap:wrap; gap:12px; margin:6px 0 12px; }
  .official-link { display:inline-block; margin-top:12px; min-height:24px; color:var(--mac-blue); font-weight:600; }
  .official-parts { padding:10px 12px; border:1px solid var(--mac-border); border-radius:10px; } .official-parts img { display:block; width:100%; height:auto; margin-top:10px; background:#fff; }
  .question-media { max-width:680px; margin:12px 0 16px; overflow:hidden; border:1px solid var(--mac-border); border-radius:14px; background:var(--mac-fill); } .question-media img { display:block; width:100%; max-height:460px; object-fit:contain; background:#fff; } .question-media figcaption { padding:10px 12px; color:var(--mac-text-secondary); font-size:13px; line-height:1.55; }
  .analysis-card { margin-top:16px; padding:16px; border:1px solid var(--mac-border); border-radius:14px; background:var(--mac-fill); }
  .analysis-head { display:flex; align-items:center; justify-content:space-between; gap:12px; }
  .verdict { padding:5px 10px; border:1px solid var(--mac-blue); border-radius:999px; color:var(--mac-blue); font-weight:700; }
  .fact-grid { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:10px; }
  .fact-grid > div,.coach-action { padding:12px; border:1px solid var(--mac-border); border-radius:10px; background:var(--mac-card); }
  .fact-grid ul { margin:8px 0 0; padding-left:20px; } .fact-grid p { margin:8px 0 0; color:var(--mac-text-secondary); }
  .coach-action { margin-top:12px; } .coach-action h4:not(:first-child) { margin-top:14px; }
  @media (max-width:700px) { .beginner-head { display:block; } .beginner-grid,.fact-grid { grid-template-columns:minmax(0,1fr); } .camera-image { max-width:520px; } .analysis-card { padding:12px; } }
  @media (prefers-reduced-motion:reduce) { *,*::before,*::after { scroll-behavior:auto !important; transition:none !important; animation:none !important; } }
  :focus-visible { outline:2px solid var(--mac-blue); outline-offset:3px; }
</style>
