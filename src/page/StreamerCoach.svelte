<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    BookOpenCheck,
    Bot,
    CheckCircle2,
    CircleAlert,
    ClipboardList,
    MessageSquareText,
    PlayCircle,
    Radio,
    RefreshCw,
    Send,
    Sparkles,
    Users,
  } from "lucide-svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import {
    COACH_ANALYSIS_CACHE_PREFIX,
    DEFAULT_COACH_HOSTS,
    buildCoachHostSummaries,
    buildCoachSystemPrompt,
    coachMessageStorageKey,
    parseCoachAnalysisEntries,
    parseCoachMessages,
    type CoachHost,
    type CoachHostSummary,
    type CoachMessage,
    type CoachSession,
  } from "../lib/streamerCoach";

  type CoachPhase = "preflight" | "live" | "review" | "training" | "chat";

  const dispatch = createEventDispatcher<{ navigate: { page: string } }>();
  const phases: readonly { key: CoachPhase; label: string }[] = [
    { key: "preflight", label: "开播前" },
    { key: "live", label: "本场跟播" },
    { key: "review", label: "下播复盘" },
    { key: "training", label: "训练任务" },
    { key: "chat", label: "和教练沟通" },
  ];
  const usageSteps: readonly { key: CoachPhase; label: string; description: string }[] = [
    { key: "preflight", label: "开播前", description: "看上场问题和本场准备清单" },
    { key: "live", label: "本场跟播", description: "确认评论、回答和片段是否接上" },
    { key: "review", label: "下播复盘", description: "看证据、问题和下一次动作" },
    { key: "training", label: "训练任务", description: "从真实评论进入专项练习" },
    { key: "chat", label: "和教练沟通", description: "继续追问自己的具体问题" },
  ];

  let phase: CoachPhase = "preflight";
  let hosts: CoachHost[] = [...DEFAULT_COACH_HOSTS];
  let selectedHostId = hosts[0].id;
  let sessions: CoachSession[] = [];
  let summaries: CoachHostSummary[] = [];
  let messages: CoachMessage[] = [];
  let draft = "";
  let sending = false;
  let errorMessage = "";
  let refreshedAt = "";
  let liveStatus = "等待本场录制或复盘证据";

  $: currentSummary = summaries.find((item) => item.host.id === selectedHostId)
    || buildCoachHostSummaries([hosts.find((item) => item.id === selectedHostId) || hosts[0]], sessions)[0];
  $: currentSessions = currentSummary?.sessions || [];
  $: latestSession = currentSessions[0] || null;
  $: if (selectedHostId) loadMessages(selectedHostId);

  function storageEntries(): Array<readonly [string, string | null]> {
    const entries: Array<readonly [string, string | null]> = [];
    for (let index = 0; index < localStorage.length; index += 1) {
      const key = localStorage.key(index);
      if (key?.startsWith(COACH_ANALYSIS_CACHE_PREFIX)) {
        entries.push([key, localStorage.getItem(key)] as const);
      }
    }
    return entries;
  }

  function load(): void {
    sessions = parseCoachAnalysisEntries(storageEntries(), hosts);
    const discoveredHosts = sessions
      .filter((session) => session.identityConfirmed && !hosts.some((host) => host.id === session.hostId))
      .map((session) => ({ id: session.hostId, displayName: session.hostName }));
    if (discoveredHosts.length) {
      hosts = [...hosts, ...discoveredHosts.filter((host, index, all) =>
        all.findIndex((item) => item.id === host.id) === index
      )];
    }
    summaries = buildCoachHostSummaries(hosts, sessions);
    refreshedAt = new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
    liveStatus = sessions.length
      ? "已连接本机场次证据，等待最新场次更新"
      : "尚无本机场次证据";
  }

  function loadMessages(hostId: string): void {
    if (typeof localStorage === "undefined") return;
    messages = parseCoachMessages(localStorage.getItem(coachMessageStorageKey(hostId)));
  }

  function saveMessages(next: CoachMessage[]): void {
    messages = next.slice(-80);
    localStorage.setItem(coachMessageStorageKey(selectedHostId), JSON.stringify(messages));
  }

  function selectHost(hostId: string): void {
    selectedHostId = hostId;
    errorMessage = "";
  }

  function navigate(page: string): void {
    dispatch("navigate", { page });
  }

  function evidenceLabel(session: CoachSession): string {
    if (session.evidenceStatus === "comment_present") {
      return `评论记录存在${session.commentCount !== null ? ` · ${session.commentCount} 条` : ""}`;
    }
    if (session.evidenceStatus === "speech_video_only") return "仅口播/视频，无可用评论";
    return "评论完整性待确认";
  }

  function evidenceClass(session: CoachSession): string {
    if (session.evidenceStatus === "comment_present") return "good";
    if (session.evidenceStatus === "speech_video_only") return "plain";
    return "warning";
  }

  function formatTime(seconds: number | null): string {
    if (seconds === null) return "时间待确认";
    const minutes = Math.floor(seconds / 60);
    const remain = Math.floor(seconds % 60);
    return `${String(minutes).padStart(2, "0")}:${String(remain).padStart(2, "0")}`;
  }

  function coachFixtureEnabled(): boolean {
    if (!import.meta.env.DEV && import.meta.env.MODE !== "test") return false;
    return new URLSearchParams(location.search).get("coachFixture") === "acceptance-v1";
  }

  async function sendMessage(): Promise<void> {
    const question = draft.trim();
    if (!question || sending || !currentSummary) return;
    const userMessage: CoachMessage = {
      id: `coach-user-${Date.now()}`,
      role: "user",
      content: question,
      createdAt: new Date().toISOString(),
      evidenceSourceKeys: currentSessions.map((session) => session.sourceKey).slice(0, 8),
    };
    saveMessages([...messages, userMessage]);
    draft = "";
    sending = true;
    errorMessage = "";
    try {
      const answer = coachFixtureEnabled()
        ? "【自动测试夹具｜AI教练建议，需人工审核】先确认观众用途和预算；当前场次证据只支持指出成交推进偏弱，具体价格、库存和成色仍需核实。下一步：把确认需求后的收口练成一句可执行选择。"
        : await invoke<string>("minimax_chat", {
          systemPrompt: buildCoachSystemPrompt(currentSummary.host, currentSessions),
          messages: [...messages, userMessage].slice(-12).map((message) => ({
            role: message.role,
            content: message.content,
          })),
        });
      const assistantMessage: CoachMessage = {
        id: `coach-assistant-${Date.now()}`,
        role: "assistant",
        content: answer.trim() || "AI教练本次未返回内容，请稍后重试。",
        createdAt: new Date().toISOString(),
        evidenceSourceKeys: currentSessions.map((session) => session.sourceKey).slice(0, 8),
      };
      saveMessages([...messages, assistantMessage]);
    } catch (error: any) {
      errorMessage = error?.message || String(error);
    } finally {
      sending = false;
    }
  }

  function phaseKeydown(event: KeyboardEvent, currentIndex: number): void {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const offset = event.key === "ArrowRight" ? 1 : -1;
    const nextIndex = (currentIndex + offset + phases.length) % phases.length;
    phase = phases[nextIndex].key;
    document.getElementById(`coach-tab-${phase}`)?.focus();
  }

  onMount(() => {
    load();
    const refresh = () => load();
    window.addEventListener("storage", refresh);
    window.addEventListener("focus", refresh);
    return () => {
      window.removeEventListener("storage", refresh);
      window.removeEventListener("focus", refresh);
    };
  });
</script>

<PageShell
  title="AI 主播教练中心"
  subtitle="主播先选择自己，再按开播前、跟播、复盘、训练和沟通五步使用；AI 建议不等于主播原话。"
>
  <div slot="actions" class="header-actions">
    <span class="host-only-badge"><Bot size={15} />主播使用版</span>
    <button type="button" class="mac-btn" on:click={load} title="刷新本机场次证据">
      <RefreshCw size={15} />刷新{refreshedAt ? ` · ${refreshedAt}` : ""}
    </button>
  </div>

  <section class="host-selector mac-card" aria-labelledby="coach-host-heading">
    <div class="section-heading compact">
      <div>
        <span class="eyebrow">第一步 · 选择自己</span>
        <h2 id="coach-host-heading">当前主播：{currentSummary?.host.displayName || "待选择"}</h2>
      </div>
      <span class="isolation-note"><CheckCircle2 size={14} />只读取自己的场次</span>
    </div>
    <div class="host-list" role="list" aria-label="选择主播">
      {#each summaries as summary}
        <button
          type="button"
          class:active={selectedHostId === summary.host.id}
          aria-pressed={selectedHostId === summary.host.id}
          on:click={() => selectHost(summary.host.id)}
        >
          <span class="avatar" aria-hidden="true">{summary.host.displayName.slice(0, 1)}</span>
          <span><strong>{summary.host.displayName}</strong><small>{summary.sessionCount} 场证据</small></span>
        </button>
      {/each}
    </div>
  </section>

  <section class="usage-guide mac-card" aria-labelledby="coach-usage-heading">
    <div class="guide-heading">
      <span class="eyebrow">每天怎么用</span>
      <h2 id="coach-usage-heading">按这 5 步使用主播教练</h2>
      <p>不需要一次看完。点当前要做的事，教练会自动带入这位主播自己的场次依据。</p>
    </div>
    <ol class="usage-steps">
      {#each usageSteps as item, index}
        <li>
          <button type="button" class:active={phase === item.key} aria-pressed={phase === item.key} on:click={() => phase = item.key}>
            <span>{index + 1}</span>
            <strong>{item.label}</strong>
            <small>{item.description}</small>
          </button>
        </li>
      {/each}
    </ol>
  </section>

  <div class="phase-tabs" role="tablist" aria-label="教练工作阶段">
      {#each phases as item, index}
        <button
          id={`coach-tab-${item.key}`}
          type="button"
          role="tab"
          aria-selected={phase === item.key}
          aria-controls={`coach-panel-${item.key}`}
          tabindex={phase === item.key ? 0 : -1}
          on:keydown={(event) => phaseKeydown(event, index)}
          on:click={() => phase = item.key}
        >{item.label}</button>
      {/each}
    </div>

    <div id={`coach-panel-${phase}`} class="phase-panel" role="tabpanel" aria-labelledby={`coach-tab-${phase}`} tabindex="0">
      {#if phase === "preflight"}
        <div class="metric-grid">
          <article class="metric mac-card"><span>已归属场次</span><strong>{currentSummary?.sessionCount || 0}</strong><small>身份确认后才计入</small></article>
          <article class="metric mac-card"><span>可复看片段</span><strong>{currentSummary?.reviewedEvidenceCount || 0}</strong><small>含时间点或逐字证据</small></article>
          <article class="metric mac-card"><span>待确认事项</span><strong>{currentSummary?.pendingConfirmationCount || 0}</strong><small>不进入确定性话术</small></article>
          <article class="metric mac-card"><span>场次均分</span><strong>{currentSummary?.averageScore ?? "—"}</strong><small>无评分时不猜分</small></article>
        </div>
        <div class="two-column">
          <article class="mac-card content-card">
            <div class="card-title"><ClipboardList size={17} /><h3>本场前先盯这几件事</h3></div>
            {#if currentSummary?.nextActions.length}
              <ol class="action-list">
                {#each currentSummary.nextActions as action}<li>{action}</li>{/each}
              </ol>
            {:else}
              <div class="empty-state"><CircleAlert size={22} /><strong>还没有已确认的上场行动</strong><p>先完成一场录播复盘，教练再从证据生成准备清单。</p></div>
            {/if}
          </article>
          <article class="mac-card content-card">
            <div class="card-title"><Sparkles size={17} /><h3>反复出现的问题</h3></div>
            {#if currentSummary?.recurringIssues.length}
              <ul class="issue-list">
                {#each currentSummary.recurringIssues as issue}<li><span>{issue.label}</span><strong>{issue.count} 场</strong></li>{/each}
              </ul>
            {:else}
              <div class="empty-state"><CheckCircle2 size={22} /><strong>数据不足，不下结论</strong><p>至少积累可归属场次后再显示反复问题。</p></div>
            {/if}
          </article>
        </div>
      {:else if phase === "live"}
        <div class="two-column live-grid">
          <article class="mac-card content-card live-card">
            <div class="live-status"><span></span><strong>{liveStatus}</strong></div>
            <h3>本场证据接入状态</h3>
            <div class="pipeline" aria-label="本场证据流程">
              <div class:done={Boolean(latestSession)}><Radio size={18} /><span>录制/导入</span></div>
              <div class:done={latestSession?.evidenceStatus === "comment_present"}><MessageSquareText size={18} /><span>评论记录</span></div>
              <div class:done={Boolean(latestSession?.evidenceItems.length)}><PlayCircle size={18} /><span>回答与片段</span></div>
              <div class:done={Boolean(latestSession?.diagnosis)}><Bot size={18} /><span>教练复盘</span></div>
            </div>
            <p class="boundary-note">软件录制场次可检查评论文件；导入视频若无评论，只分析口播/视频，不补造评论。</p>
          </article>
          <article class="mac-card content-card">
            <div class="card-title"><BookOpenCheck size={17} /><h3>本场教练动作</h3></div>
            <ul class="check-list">
              <li><CheckCircle2 size={15} />先标记真实评论，不直接把弹幕当事实。</li>
              <li><CheckCircle2 size={15} />对齐主播回答和视频时间点，再进入训练候选。</li>
              <li><CheckCircle2 size={15} />价格、库存、成色、售后不清楚就标“待确认”。</li>
              <li><CheckCircle2 size={15} />下播后输出本场问题、参考回答和下一轮训练。</li>
            </ul>
            <button type="button" class="mac-btn mac-btn-primary" on:click={() => navigate("直播间")}><Radio size={15} />进入直播间</button>
          </article>
        </div>
      {:else if phase === "review"}
        <div class="session-list">
          {#each currentSessions as session}
            <article class="mac-card session-card">
              <div class="session-head">
                <div><span class="eyebrow">{new Date(session.updatedAt).toLocaleString("zh-CN")}</span><h3>{session.title}</h3></div>
                <span class={`evidence-badge ${evidenceClass(session)}`}>{evidenceLabel(session)}</span>
              </div>
              <p class="diagnosis">{session.diagnosis?.headline || "尚无已保存整场诊断"}</p>
              <div class="session-detail-grid">
                <section><h4>本场问题</h4>{#if session.diagnosis?.issues.length}<ul>{#each session.diagnosis.issues as issue}<li>{issue}</li>{/each}</ul>{:else}<p>暂无经审核问题</p>{/if}</section>
                <section><h4>下次动作</h4>{#if session.diagnosis?.nextActions.length}<ul>{#each session.diagnosis.nextActions as action}<li>{action}</li>{/each}</ul>{:else}<p>等待复盘</p>{/if}</section>
                <section class="evidence-section"><h4>回答与视频片段</h4>
                  {#if session.evidenceItems.length}
                    {#each session.evidenceItems.slice(0, 3) as item}
                      <details><summary>{formatTime(item.startSec)} · {item.label}</summary><p>{item.evidence || "逐字证据待确认"}</p>{#if item.suggestion}<p class="suggestion">AI复盘建议，需人工审核：{item.suggestion}</p>{/if}</details>
                    {/each}
                  {:else}<p>尚未形成可展示的回答＋片段证据。</p>{/if}
                </section>
              </div>
            </article>
          {:else}
            <div class="empty-state mac-card large"><CircleAlert size={28} /><strong>{currentSummary?.host.displayName} 暂无已确认场次</strong><p>不会用其他主播场次填充。完成录播分析并确认主播身份后自动出现。</p><button type="button" class="mac-btn mac-btn-primary" on:click={() => navigate("录播")}>去录播</button></div>
          {/each}
        </div>
      {:else if phase === "training"}
        <div class="two-column">
          <article class="mac-card content-card accent-card"><div class="card-title"><Users size={17} /><h3>从真实评论起题</h3></div><p>第一问来自已审核评论；AI只根据主播回答延伸一问，连续训练需求确认、事实边界、信任与成交推进。</p><button type="button" class="mac-btn mac-btn-primary" on:click={() => navigate("情景训练")}>进入情景训练</button></article>
          <article class="mac-card content-card"><div class="card-title"><ClipboardList size={17} /><h3>本主播建议专项</h3></div>{#if currentSummary?.recurringIssues.length}<ul class="check-list">{#each currentSummary.recurringIssues as issue}<li><CircleAlert size={15} />{issue.label}</li>{/each}</ul>{:else}<div class="empty-state"><CircleAlert size={22} /><strong>先积累训练依据</strong><p>没有已确认问题时，不自动编一套专属题。</p></div>{/if}</article>
        </div>
      {:else}
        <div class="chat-layout">
          <section class="mac-card chat-card" aria-label={`与${currentSummary?.host.displayName || "主播"}教练沟通`}>
            <div class="chat-context"><Bot size={18} /><div><strong>{currentSummary?.host.displayName}的专属教练</strong><span>已加载 {currentSessions.length} 场本主播证据</span></div></div>
            <div class="messages" aria-live="polite">
              {#each messages as message}
                <article class:assistant={message.role === "assistant"} class:user={message.role === "user"}>
                  <span>{message.role === "assistant" ? "AI教练" : "我"}</span><p>{message.content}</p>
                </article>
              {:else}
                <div class="empty-state chat-empty"><MessageSquareText size={25} /><strong>可以直接问教练</strong><p>例如：“我上场最该改哪一点？”“遇到成色质疑怎么追问？”</p></div>
              {/each}
            </div>
            {#if errorMessage}<p class="error" role="alert">{errorMessage}</p>{/if}
            <form class="composer" on:submit|preventDefault={sendMessage}>
              <label for="coach-question">给教练留言</label>
              <textarea id="coach-question" class="mac-field" bind:value={draft} placeholder="说说你遇到的问题；Ctrl+Enter 发送" on:keydown={(event) => { if (event.ctrlKey && event.key === "Enter") void sendMessage(); }}></textarea>
              <div><small>AI教练建议需人工审核；未确认商品事实不会进入回答。</small><button type="submit" class="mac-btn mac-btn-primary" disabled={sending || !draft.trim()}><Send size={15} />{sending ? "思考中…" : "发送"}</button></div>
            </form>
          </section>
          <aside class="mac-card evidence-context"><h3>本次回答依据</h3><p>只读取当前主播最近 8 场摘要，绝不串用其他主播。</p>{#each currentSessions.slice(0, 5) as session}<div><strong>{session.title}</strong><span>{evidenceLabel(session)}</span></div>{:else}<div class="no-evidence">暂无场次依据；教练只能给通用方法。</div>{/each}</aside>
        </div>
      {/if}
    </div>
</PageShell>

<style>
  .header-actions,.host-only-badge,.section-heading,.isolation-note,.card-title,.live-status,.chat-context { display:flex; align-items:center; }
  .header-actions { gap:8px; flex-wrap:wrap; }
  .host-only-badge { gap:5px; min-height:30px; padding:0 10px; border-radius:999px; color:var(--mac-blue); background:var(--mac-blue-soft); font-size:11px; font-weight:700; }
  .phase-tabs button { border:0; cursor:pointer; color:var(--mac-secondary); background:transparent; }
  .phase-tabs button[aria-selected="true"] { color:var(--mac-label); background:var(--mac-bg-card); box-shadow:0 1px 4px rgba(0,0,0,.09); }
  button:focus-visible,textarea:focus-visible,summary:focus-visible { outline:2px solid var(--mac-blue); outline-offset:2px; }
  .host-selector { padding:16px; }
  .section-heading { justify-content:space-between; gap:12px; }
  .section-heading h2,.guide-heading h2 { margin:3px 0 0; font-size:20px; }
  .eyebrow { color:var(--mac-blue); font-size:10px; font-weight:700; letter-spacing:.06em; }
  .isolation-note { gap:5px; color:var(--mac-green-solid); font-size:11px; }
  .host-list { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:8px; margin-top:14px; }
  .host-list button { display:flex; align-items:center; gap:9px; min-width:0; padding:10px; border:1px solid var(--mac-separator); border-radius:12px; cursor:pointer; text-align:left; color:var(--mac-label); background:var(--mac-bg-card); }
  .host-list button.active { border-color:var(--mac-blue); background:var(--mac-blue-soft); box-shadow:0 0 0 1px var(--mac-blue); }
  .host-list button > span:last-child { min-width:0; display:flex; flex-direction:column; }
  .host-list strong { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:13px; }
  .host-list small { margin-top:2px; color:var(--mac-tertiary); font-size:10px; }
  .avatar { width:32px; height:32px; display:grid; place-items:center; flex:0 0 32px; border-radius:10px; color:#fff; background:linear-gradient(145deg,#5ab0ff,var(--mac-blue)); font-size:13px; font-weight:700; }
  .usage-guide { display:grid; grid-template-columns:minmax(200px,.75fr) minmax(0,2fr); gap:18px; padding:16px; }
  .guide-heading p { margin:7px 0 0; color:var(--mac-tertiary); font-size:11px; line-height:1.55; }
  .usage-steps { display:grid; grid-template-columns:repeat(5,minmax(0,1fr)); gap:7px; margin:0; padding:0; list-style:none; }
  .usage-steps li { min-width:0; }
  .usage-steps button { width:100%; min-height:88px; display:flex; flex-direction:column; align-items:flex-start; gap:4px; padding:9px; border:1px solid var(--mac-separator); border-radius:10px; cursor:pointer; text-align:left; color:var(--mac-label); background:var(--mac-bg-card); }
  .usage-steps button.active { border-color:var(--mac-blue); background:var(--mac-blue-soft); box-shadow:0 0 0 1px var(--mac-blue); }
  .usage-steps button > span { width:20px; height:20px; display:grid; place-items:center; border-radius:50%; color:var(--mac-blue); background:var(--mac-blue-soft); font-size:10px; font-weight:700; }
  .usage-steps button.active > span { color:#fff; background:var(--mac-blue); }
  .usage-steps strong { font-size:11px; }
  .usage-steps small { color:var(--mac-tertiary); font-size:9px; line-height:1.35; }
  .phase-tabs { display:flex; gap:4px; padding:4px; overflow-x:auto; border-radius:12px; background:var(--mac-fill); }
  .phase-tabs button { min-width:max-content; min-height:34px; padding:0 16px; border-radius:9px; font-size:12px; font-weight:600; }
  .phase-panel { min-width:0; }
  .metric-grid { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:10px; }
  .metric { min-width:0; padding:15px; display:flex; flex-direction:column; }
  .metric span { color:var(--mac-tertiary); font-size:11px; }
  .metric strong { margin:5px 0 2px; color:var(--mac-blue); font-size:25px; line-height:1; }
  .metric small { color:var(--mac-quaternary); font-size:9px; }
  .two-column,.chat-layout { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; margin-top:12px; }
  .content-card { min-width:0; padding:17px; }
  .card-title { gap:7px; color:var(--mac-blue); }
  .card-title h3,.live-card h3,.evidence-context h3 { margin:0; color:var(--mac-label); font-size:14px; }
  .action-list,.issue-list,.check-list { margin:14px 0 0; padding:0; list-style:none; }
  .action-list { counter-reset:coach-action; }
  .action-list li { display:flex; gap:9px; margin:9px 0; color:var(--mac-secondary); font-size:12px; line-height:1.5; }
  .action-list li:before { counter-increment:coach-action; content:counter(coach-action); width:20px; height:20px; display:grid; place-items:center; flex:0 0 20px; border-radius:50%; color:var(--mac-blue); background:var(--mac-blue-soft); font-weight:700; }
  .issue-list li { display:flex; justify-content:space-between; gap:12px; padding:9px 0; border-bottom:1px solid var(--mac-separator); font-size:12px; }
  .issue-list li:last-child { border-bottom:0; }
  .issue-list strong { color:var(--mac-orange); }
  .empty-state { min-height:110px; display:flex; flex-direction:column; align-items:center; justify-content:center; padding:14px; text-align:center; color:var(--mac-tertiary); }
  .empty-state strong { margin-top:7px; color:var(--mac-secondary); font-size:12px; }
  .empty-state p { max-width:440px; margin:5px 0 0; font-size:11px; line-height:1.5; }
  .empty-state.large { min-height:260px; }
  .empty-state.large .mac-btn { margin-top:14px; }
  .live-grid { margin-top:0; }
  .live-status { gap:8px; color:var(--mac-secondary); font-size:11px; }
  .live-status span { width:8px; height:8px; border-radius:50%; background:var(--mac-orange); box-shadow:0 0 0 4px rgba(255,159,10,.14); }
  .live-card h3 { margin-top:18px; }
  .pipeline { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:7px; margin:13px 0; }
  .pipeline div { min-height:64px; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:6px; border:1px solid var(--mac-separator); border-radius:10px; color:var(--mac-tertiary); font-size:10px; text-align:center; }
  .pipeline div.done { color:var(--mac-blue); border-color:rgba(0,113,227,.35); background:var(--mac-blue-soft); }
  .boundary-note,.accent-card p,.evidence-context p { color:var(--mac-tertiary); font-size:11px; line-height:1.6; }
  .check-list { display:grid; gap:9px; margin-bottom:15px; }
  .check-list li { display:flex; align-items:flex-start; gap:7px; color:var(--mac-secondary); font-size:12px; line-height:1.45; }
  .check-list li :global(svg) { flex:0 0 auto; color:var(--mac-green-solid); margin-top:1px; }
  .session-list { display:grid; gap:10px; }
  .session-card { min-width:0; padding:17px; }
  .session-head { display:flex; justify-content:space-between; align-items:flex-start; gap:12px; }
  .session-head h3 { margin:4px 0 0; font-size:15px; }
  .evidence-badge { flex:0 0 auto; padding:4px 8px; border-radius:999px; font-size:9px; }
  .evidence-badge.good { color:#15803d; background:#dcfce7; }.evidence-badge.warning { color:#a16207; background:#fef3c7; }.evidence-badge.plain { color:var(--mac-secondary); background:var(--mac-fill); }
  .diagnosis { margin:12px 0; padding:10px 12px; border-left:3px solid var(--mac-blue); border-radius:7px; color:var(--mac-secondary); background:var(--mac-blue-soft); font-size:12px; }
  .session-detail-grid { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:10px; }
  .session-detail-grid section { min-width:0; padding:10px; border-radius:9px; background:var(--mac-fill); }
  .session-detail-grid h4 { margin:0 0 7px; font-size:11px; }
  .session-detail-grid ul { margin:0; padding-left:16px; color:var(--mac-secondary); font-size:10px; line-height:1.55; }
  .session-detail-grid p { margin:5px 0; color:var(--mac-tertiary); font-size:10px; line-height:1.5; }
  details { margin:6px 0; } summary { cursor:pointer; color:var(--mac-blue); font-size:10px; } .suggestion { padding-top:5px; border-top:1px solid var(--mac-separator); }
  .accent-card { border-color:rgba(0,113,227,.32); background:linear-gradient(145deg,var(--mac-bg-card),var(--mac-blue-soft)); }
  .chat-layout { grid-template-columns:minmax(0,1fr) 250px; margin-top:0; }
  .chat-card { min-width:0; display:flex; flex-direction:column; min-height:480px; overflow:hidden; }
  .chat-context { gap:9px; padding:14px 16px; border-bottom:1px solid var(--mac-separator); }
  .chat-context :global(svg) { color:var(--mac-blue); }.chat-context div { display:flex; flex-direction:column; }.chat-context strong { font-size:12px; }.chat-context span { color:var(--mac-tertiary); font-size:9px; }
  .messages { min-height:0; flex:1; display:flex; flex-direction:column; gap:10px; padding:15px; overflow:auto; }
  .messages article { max-width:84%; }.messages article.user { align-self:flex-end; }.messages article.assistant { align-self:flex-start; }
  .messages article span { display:block; margin-bottom:3px; color:var(--mac-tertiary); font-size:9px; }.messages article.user span { text-align:right; }
  .messages article p { margin:0; padding:9px 11px; border-radius:12px; white-space:pre-wrap; color:var(--mac-label); background:var(--mac-fill); font-size:12px; line-height:1.55; }.messages article.user p { color:#fff; background:var(--mac-blue); }
  .chat-empty { margin:auto; }.error { margin:0 15px 8px; color:var(--mac-red); font-size:11px; }
  .composer { display:grid; gap:7px; padding:13px; border-top:1px solid var(--mac-separator); }.composer label { font-size:10px; font-weight:600; }.composer textarea { width:100%; min-height:74px; box-sizing:border-box; }.composer > div { display:flex; justify-content:space-between; align-items:center; gap:10px; }.composer small { color:var(--mac-tertiary); font-size:9px; }
  .evidence-context { min-width:0; padding:15px; align-self:start; }.evidence-context > div { display:flex; flex-direction:column; gap:2px; padding:9px 0; border-bottom:1px solid var(--mac-separator); }.evidence-context > div:last-child { border-bottom:0; }.evidence-context strong { font-size:10px; }.evidence-context span { color:var(--mac-tertiary); font-size:9px; }.evidence-context .no-evidence { color:var(--mac-tertiary); font-size:10px; }
  @media (max-width:1100px) { .usage-guide { grid-template-columns:1fr; }.usage-steps { grid-template-columns:repeat(5,minmax(110px,1fr)); overflow-x:auto; padding-bottom:3px; } }
  @media (max-width:1000px) { .host-list,.metric-grid { grid-template-columns:repeat(2,minmax(0,1fr)); }.session-detail-grid { grid-template-columns:repeat(2,minmax(0,1fr)); }.evidence-section { grid-column:1/-1; } }
  @media (max-width:700px) { :global(.mac-page) { padding:14px 12px 22px; }.header-actions { width:100%; }.header-actions .mac-btn { flex:1; }.host-selector,.usage-guide { padding:12px; }.host-list { grid-template-columns:repeat(2,minmax(0,1fr)); }.usage-steps { grid-template-columns:1fr; overflow:visible; }.usage-steps button { min-height:0; display:grid; grid-template-columns:24px minmax(72px,.6fr) 1.4fr; align-items:center; }.phase-tabs button { padding:0 12px; }.metric-grid,.two-column,.chat-layout { grid-template-columns:1fr; }.session-head { flex-direction:column; }.session-detail-grid { grid-template-columns:1fr; }.evidence-section { grid-column:auto; }.chat-card { min-height:430px; }.evidence-context { order:-1; }.pipeline { grid-template-columns:repeat(2,minmax(0,1fr)); }.composer > div { align-items:flex-end; }.composer small { max-width:65%; } }
  @media (max-width:380px) { .host-list { grid-template-columns:1fr; }.metric-grid { grid-template-columns:1fr 1fr; }.metric { padding:12px; }.metric strong { font-size:21px; }.isolation-note { max-width:92px; text-align:right; }.phase-tabs { margin-inline:-2px; } }
  @media (prefers-reduced-motion:reduce) { * { scroll-behavior:auto !important; transition:none !important; animation:none !important; } }
</style>
