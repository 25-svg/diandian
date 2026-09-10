<script lang="ts">
  import { ArrowRight, BookOpenCheck, Camera, CheckCircle2, MessageSquareText, RotateCcw, Sparkles } from "lucide-svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import CoachPractice from "../lib/components/CoachPractice.svelte";
  import { beginnerCameraLessons } from "../lib/coachPractice";

  const brands = [
    { name: "佳能", count: beginnerCameraLessons.filter((lesson) => lesson.model.startsWith("佳能")).length },
    { name: "富士", count: beginnerCameraLessons.filter((lesson) => lesson.model.startsWith("富士")).length },
    { name: "索尼", count: beginnerCameraLessons.filter((lesson) => lesson.model.startsWith("索尼")).length },
  ];
  const primaryLesson = beginnerCameraLessons.find((lesson) => lesson.id === "canon-r50") || beginnerCameraLessons[0];
  const trainingModeCount = 6;
  const knowledgePointCount = beginnerCameraLessons.reduce((total, lesson) => total + lesson.knowledgePoints.length, 0);
  let activeBrand = "佳能";
  $: visibleLessons = beginnerCameraLessons.filter((lesson) => lesson.model.startsWith(activeBrand));
</script>

<PageShell
  title="相机知识问答"
  subtitle="先看懂常卖机型，再用选择、判断、问答和真实评论情景巩固。"
  paddedBottom={true}
>
  <div slot="actions" class="library-count" aria-label={`当前收录 ${beginnerCameraLessons.length} 款常卖机型`}>
    <Camera size={15} aria-hidden="true" />{beginnerCameraLessons.length} 款常卖机型
  </div>

  <section class="learning-hero" aria-labelledby="camera-primary-heading">
    <div class="hero-copy">
      <span class="hero-eyebrow"><Sparkles size={14} aria-hidden="true" />推荐起点 · 佳能入门</span>
      <h2 id="camera-primary-heading">从佳能 EOS R50 开始</h2>
      <p>{primaryLesson.facts.find((fact) => fact.label === "适用人群")?.value}。先看懂参数卡，再练习如何回答顾客。</p>
      <a class="hero-action" href="#camera-practice">开始学习 R50 <ArrowRight size={16} aria-hidden="true" /></a>
    </div>
    <div class="hero-media">
      <span class="hero-orbit" aria-hidden="true"></span>
      <img src={primaryLesson.image} alt="佳能 EOS R50 参数卡原图" width="1500" height="1500" />
    </div>
  </section>

  <section class="fact-strip" aria-label="课程真实数据">
    <div><span>常卖机型</span><strong>{beginnerCameraLessons.length}</strong><small>款</small></div>
    <div><span>覆盖品牌</span><strong>{brands.length}</strong><small>个</small></div>
    <div><span>已核验知识点</span><strong>{knowledgePointCount}</strong><small>条</small></div>
    <div><span>训练方式</span><strong>{trainingModeCount}</strong><small>种</small></div>
  </section>

  <div class="section-title">
    <div><h2>选择学习方式</h2><p>先认机、再记参数，最后把知识讲成顾客听得懂的话。</p></div>
  </div>
  <section class="learning-map" aria-label="相机知识学习路径">
    <a class="path-card mac-card" href="#camera-practice">
      <div class="path-icon"><Camera size={20} aria-hidden="true" /></div>
      <div><h2>认识相机</h2><p>参数卡原图、基础部件、知识点、顾客痛点和使用边界。</p></div>
      <ul aria-label="已收录品牌">
        {#each brands as brand}<li>{brand.name}<span>{brand.count} 款</span></li>{/each}
      </ul>
      <ArrowRight class="path-arrow" size={16} aria-hidden="true" />
    </a>
    <a class="path-card mac-card" href="#camera-practice">
      <div class="path-icon"><CheckCircle2 size={20} aria-hidden="true" /></div>
      <div><h2>快速答题</h2><p>选择题、判断题和问答题，答完立即查看依据与解释。</p></div>
      <span class="path-note">适合新人打基础</span>
      <ArrowRight class="path-arrow" size={16} aria-hidden="true" />
    </a>
    <a class="path-card mac-card" href="#camera-practice">
      <div class="path-icon"><MessageSquareText size={20} aria-hidden="true" /></div>
      <div><h2>评论实战</h2><p>从已审核直播评论起题，练习把参数讲成顾客听得懂的话。</p></div>
      <span class="path-note">商品事实不脱离证据</span>
      <ArrowRight class="path-arrow" size={16} aria-hidden="true" />
    </a>
    <a class="path-card mac-card" href="#camera-practice">
      <div class="path-icon"><RotateCcw size={20} aria-hidden="true" /></div>
      <div><h2>错题复习</h2><p>训练结束后集中查看低分题、遗漏点、参考回答和原始依据。</p></div>
      <span class="path-note">错题记录保存在本机</span>
      <ArrowRight class="path-arrow" size={16} aria-hidden="true" />
    </a>
  </section>

  <section class="course-layout">
    <div class="course-panel mac-card">
      <div class="section-title compact">
        <div><h2>常卖机型课程</h2><p>按品牌逐款学习，不要求一次背完。</p></div>
      </div>
      <div class="brand-tabs" aria-label="选择相机品牌">
        {#each brands as brand}
          <button type="button" class:active={activeBrand === brand.name} aria-pressed={activeBrand === brand.name} on:click={() => activeBrand = brand.name}>{brand.name} {brand.count}款</button>
        {/each}
      </div>
      <div class="model-grid">
        {#each visibleLessons as lesson}
          <a class="model-card" href="#camera-practice">
            <img src={lesson.image} alt={`${lesson.model} 参数卡`} width="1500" height="1500" loading="lazy" />
            <div><strong>{lesson.model.replace(" EOS ", " ")}</strong><span>{lesson.knowledgePoints.length} 个知识点 · {lesson.facts.find((fact) => fact.label === "像素")?.value}</span></div>
            <ArrowRight size={15} aria-hidden="true" />
          </a>
        {/each}
      </div>
    </div>
    <aside class="advice-panel mac-card" aria-labelledby="learning-advice-heading">
      <span class="advice-label">新人建议顺序</span>
      <h2 id="learning-advice-heading">学习建议</h2>
      <ol>
        <li><span>1</span><div><strong>先看原图认机型</strong><p>知道相机叫什么，快门在哪里。</p></div></li>
        <li><span>2</span><div><strong>只记直播常用知识</strong><p>像素、焦段、闪光灯和适用人群。</p></div></li>
        <li><span>3</span><div><strong>最后练顾客问题</strong><p>把参数转成简单、自然的直播回答。</p></div></li>
      </ol>
      <p class="evidence-note">所有商品事实以内部参数卡和品牌官方资料为准。</p>
    </aside>
  </section>

  <section id="camera-practice" class="start-card" aria-labelledby="camera-start-heading">
    <div class="section-heading">
      <BookOpenCheck size={20} aria-hidden="true" />
      <div><h2 id="camera-start-heading">开始学习与答题</h2><p>先选训练方式；每次一题，答完后再看参考。</p></div>
    </div>
    <CoachPractice hostId="camera-knowledge-shared" hostName="" title="相机知识练习" />
  </section>
</PageShell>

<style>
  .library-count { min-height:36px; display:inline-flex; align-items:center; gap:7px; padding:0 11px; border:1px solid var(--mac-separator); border-radius:999px; color:var(--mac-blue); background:var(--mac-bg-card); font-size:12px; font-weight:700; }
  .learning-hero { min-height:224px; display:grid; grid-template-columns:minmax(0,1.2fr) minmax(260px,.8fr); overflow:hidden; border:1px solid rgba(255,255,255,.09); border-radius:20px; background:linear-gradient(118deg,#0e2844 0%,#173c62 62%,#1f5788 100%); box-shadow:0 18px 40px rgba(34,57,82,.12); }
  .hero-copy { z-index:1; padding:28px; color:white; }
  .hero-eyebrow { display:inline-flex; align-items:center; gap:6px; margin-bottom:13px; color:#a8d3ff; font-size:11px; font-weight:700; }
  .hero-copy h2 { margin:0 0 9px; color:white; font-size:26px; letter-spacing:-.02em; }
  .hero-copy p { max-width:600px; color:#d4e3f1; font-size:13px; line-height:1.7; }
  .hero-action { min-height:42px; display:inline-flex; align-items:center; justify-content:center; gap:8px; margin-top:20px; padding:0 17px; border-radius:10px; color:#10395f; background:white; box-shadow:0 8px 20px rgba(0,0,0,.14); font-size:12px; font-weight:750; text-decoration:none; transition:transform .18s ease,box-shadow .18s ease; }
  .hero-action:hover { transform:translateY(-2px); box-shadow:0 12px 24px rgba(0,0,0,.18); }
  .hero-action:focus-visible,.path-card:focus-visible,.model-card:focus-visible,.brand-tabs button:focus-visible { outline:3px solid var(--mac-blue-soft); outline-offset:3px; }
  .hero-media { position:relative; min-height:220px; display:grid; place-items:center; overflow:hidden; background:radial-gradient(circle at center,rgba(123,190,255,.3),transparent 64%); }
  .hero-orbit { position:absolute; width:240px; height:240px; border:1px solid rgba(255,255,255,.14); border-radius:50%; }
  .hero-media img { position:relative; width:min(82%,300px); height:196px; object-fit:cover; object-position:center 39%; border:1px solid rgba(255,255,255,.2); border-radius:18px; background:white; box-shadow:0 18px 30px rgba(0,0,0,.24); }
  .fact-strip { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:12px; }
  .fact-strip div { min-width:0; padding:14px 16px; border:1px solid var(--mac-separator); border-radius:14px; background:var(--mac-bg-card); }
  .fact-strip span { display:block; color:var(--mac-tertiary); font-size:10px; }
  .fact-strip strong { display:inline-block; margin-top:4px; color:var(--mac-label); font-size:20px; }
  .fact-strip small { margin-left:4px; color:var(--mac-secondary); font-size:10px; }
  .section-title { display:flex; align-items:flex-end; justify-content:space-between; gap:16px; }
  .section-title h2 { margin:0 0 3px; }
  .section-title.compact { margin-bottom:12px; }
  h2 { margin:4px 0; color:var(--mac-label); font-size:16px; }
  p { margin:0; color:var(--mac-secondary); font-size:12px; line-height:1.55; }
  .learning-map { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:12px; }
  .path-card { position:relative; min-width:0; display:flex; flex-direction:column; gap:12px; padding:16px; color:inherit; text-decoration:none; transition:transform .18s ease,border-color .18s ease,box-shadow .18s ease; }
  .path-card:hover { transform:translateY(-3px); border-color:rgba(22,135,248,.28); box-shadow:0 12px 26px rgba(31,58,86,.09); }
  .path-icon { width:40px; height:40px; display:grid; place-items:center; border-radius:12px; color:var(--mac-blue); background:var(--mac-blue-soft); }
  .path-card h2 { margin:0 0 5px; }
  .path-card ul { display:grid; gap:5px; margin:auto 0 0; padding:0; list-style:none; }
  .path-card li { display:flex; justify-content:space-between; gap:10px; color:var(--mac-secondary); font-size:11px; }
  .path-card li span,.path-note { color:var(--mac-tertiary); font-size:10px; }
  .path-note { margin-top:auto; }
  :global(.path-arrow) { position:absolute; right:14px; bottom:13px; color:var(--mac-blue); }
  .course-layout { display:grid; grid-template-columns:minmax(0,1.45fr) minmax(270px,.55fr); gap:14px; }
  .course-panel,.advice-panel { padding:17px; }
  .brand-tabs { display:flex; flex-wrap:wrap; gap:7px; margin-bottom:12px; }
  .brand-tabs button { min-height:36px; padding:0 12px; border:1px solid var(--mac-separator); border-radius:9px; color:var(--mac-secondary); background:var(--mac-bg); cursor:pointer; }
  .brand-tabs button.active { border-color:rgba(22,135,248,.3); color:var(--mac-blue); background:var(--mac-blue-soft); font-weight:700; }
  .model-grid { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:8px; }
  .model-card { min-width:0; display:grid; grid-template-columns:58px minmax(0,1fr) auto; align-items:center; gap:9px; min-height:72px; padding:8px; border:1px solid var(--mac-separator); border-radius:11px; color:var(--mac-label); text-decoration:none; }
  .model-card:hover { border-color:rgba(22,135,248,.3); background:var(--mac-blue-soft); }
  .model-card img { width:58px; height:52px; object-fit:cover; object-position:center 39%; border-radius:7px; background:var(--mac-bg); }
  .model-card strong,.model-card span { display:block; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .model-card strong { font-size:11px; }
  .model-card span { margin-top:4px; color:var(--mac-tertiary); font-size:9px; }
  .advice-label { color:var(--mac-blue); font-size:10px; font-weight:700; }
  .advice-panel ol { display:grid; gap:13px; margin:14px 0; padding:0; list-style:none; }
  .advice-panel li { display:grid; grid-template-columns:28px 1fr; gap:9px; align-items:flex-start; }
  .advice-panel li>span { width:28px; height:28px; display:grid; place-items:center; border-radius:9px; color:var(--mac-blue); background:var(--mac-blue-soft); font-size:11px; font-weight:800; }
  .advice-panel strong { display:block; color:var(--mac-label); font-size:11px; }
  .advice-panel li p { margin-top:3px; font-size:10px; }
  .evidence-note { padding:10px; border-radius:9px; background:var(--mac-fill); font-size:10px; }
  .start-card { min-width:0; }
  .section-heading { display:flex; align-items:flex-start; gap:9px; padding:2px 2px 0; color:var(--mac-blue); }
  .section-heading h2 { margin:0 0 3px; }
  @media (max-width:1100px) {
    .learning-map { grid-template-columns:repeat(2,minmax(0,1fr)); }
    .course-layout { grid-template-columns:1fr; }
  }
  @media (max-width:850px) {
    .learning-hero { grid-template-columns:minmax(0,1fr) 250px; }
    .model-grid { grid-template-columns:repeat(2,minmax(0,1fr)); }
  }
  @media (max-width:700px) {
    .learning-hero { grid-template-columns:1fr; }
    .hero-copy { padding:21px; }
    .hero-copy h2 { font-size:22px; }
    .hero-media { min-height:156px; }
    .hero-media img { width:min(76%,240px); height:142px; }
    .hero-orbit { width:170px; height:170px; }
    .fact-strip { grid-template-columns:repeat(2,minmax(0,1fr)); }
    .learning-map { grid-template-columns:1fr; }
    .path-card { padding:14px; }
    .model-grid { grid-template-columns:1fr; }
  }
  @media (prefers-reduced-motion:reduce) { .hero-action,.path-card { transition:none; } .hero-action:hover,.path-card:hover { transform:none; } }
</style>
