<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertCircle, ArrowLeft, Loader2, Sparkles } from "lucide-svelte";
  import { get_static_url, invoke } from "../invoker";
  import {
    clipTranscriptToSrt,
    parseClipTranscript,
    type ClipReviewItem,
    type ClipReviewRequest,
  } from "../clipReview";
  import { formatWorkspaceClock, type WorkspaceTranscriptEntry } from "../companyAnalysisWorkspace";
  import {
    buildScriptQualityUserMessage,
    mergeScriptQualityAnnotations,
    parseScriptQualityBundle,
    scriptIssueKindLabel,
    scriptQualitySystemPrompt,
    splitTranscriptForScriptQuality,
    type ScriptIssueAnnotation,
  } from "../scriptQuality";

  export let request: ClipReviewRequest;

  const dispatch = createEventDispatcher<{ close: void }>();
  let activeIndex = 0;
  let activeItem: ClipReviewItem | null = null;
  let activeKey = "";
  let loadedKey = "";
  let videoUrl = "";
  let videoError = "";
  let videoElement: HTMLVideoElement | null = null;
  let editableEntries: WorkspaceTranscriptEntry[] = [];
  let analysisLoading = false;
  let analysisError = "";
  let qualitySummary = "";
  let annotations: ScriptIssueAnnotation[] = [];
  let selectedCueId: number | null = null;
  let savingTranscript = false;
  let transcriptSaveMessage = "";
  let transcriptLoading = false;
  let transcriptError = "";

  $: if (activeIndex >= request.items.length) activeIndex = 0;
  $: activeItem = request.items[activeIndex] ?? null;
  $: activeKey = activeItem ? `${request.taskId}:${activeItem.video.id}` : "";
  $: if (activeItem && activeKey && activeKey !== loadedKey) {
    loadedKey = activeKey;
    editableEntries = activeItem.transcriptEntries.map((entry) => ({ ...entry }));
    videoUrl = "";
    videoError = "";
    qualitySummary = "";
    annotations = [];
    selectedCueId = null;
    transcriptSaveMessage = "";
    transcriptLoading = false;
    transcriptError = "";
    void prepareActiveItem(activeItem, activeKey);
  }
  $: annotationByCue = new Map(annotations.map((annotation) => [annotation.cueId, annotation]));

  async function prepareActiveItem(item: ClipReviewItem, key: string): Promise<void> {
    try {
      const nextUrl = await get_static_url("output", item.video.file);
      if (loadedKey !== key) return;
      videoUrl = nextUrl;
    } catch (error) {
      if (loadedKey !== key) return;
      videoError = `无法加载切片视频：${String(error)}`;
    }
    if (!editableEntries.length) {
      transcriptLoading = true;
      transcriptError = "";
      try {
        let subtitle = "";
        try {
          subtitle = await invoke<string>("get_video_subtitle", { id: item.video.id });
        } catch (error) {
          console.warn("Existing clip transcript must be regenerated:", error);
        }
        if (!subtitle.trim()) {
          subtitle = await invoke<string>("generate_video_subtitle", {
            eventId: `clip_review_${item.video.id}_${Date.now()}`,
            id: item.video.id,
          });
        }
        if (loadedKey !== key) return;
        editableEntries = parseClipTranscript(subtitle);
        if (!editableEntries.length) transcriptError = "切片文稿为空，无法进行 AI 分析。";
      } catch (error) {
        if (loadedKey !== key) return;
        transcriptError = `文稿加载失败：${String(error).replace(/^Error:\s*/i, "")}`;
      } finally {
        if (loadedKey === key) transcriptLoading = false;
      }
    }
    await runAnalysis(key);
  }

  async function runAnalysis(expectedKey = activeKey): Promise<void> {
    if (!editableEntries.length || analysisLoading) return;
    const entries = editableEntries.map((entry) => ({ ...entry, text: entry.text.trim() })).filter((entry) => entry.text);
    if (!entries.length) {
      analysisError = "文稿为空，无法进行 AI 分析。";
      return;
    }
    analysisLoading = true;
    analysisError = "";
    qualitySummary = "";
    annotations = [];
    try {
      const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
      const chunks = splitTranscriptForScriptQuality(entries);
      const batches: ScriptIssueAnnotation[][] = [];
      const summaries: string[] = [];
      for (const chunk of chunks) {
        const response = await invoke<string>("minimax_chat", {
          systemPrompt: scriptQualitySystemPrompt(),
          messages: [{ role: "user", content: buildScriptQualityUserMessage(chunk) }],
        });
        if (loadedKey !== expectedKey) return;
        const bundle = parseScriptQualityBundle(response, entriesById);
        batches.push(bundle.annotations);
        if (bundle.summary) summaries.push(bundle.summary);
      }
      if (loadedKey !== expectedKey) return;
      annotations = mergeScriptQualityAnnotations(batches);
      qualitySummary = summaries.join("\n") || "本段未发现需要重点修改的话术，可继续人工检查文稿。";
    } catch (error) {
      if (loadedKey !== expectedKey) return;
      analysisError = String(error).replace(/^Error:\s*/i, "");
    } finally {
      if (loadedKey === expectedKey) analysisLoading = false;
    }
  }

  function seekTo(seconds: number): void {
    if (!videoElement) return;
    videoElement.currentTime = Math.max(0, seconds);
    void videoElement.play().catch(() => undefined);
  }

  function updateEntryText(entryId: number, text: string): void {
    editableEntries = editableEntries.map((entry) => entry.id === entryId ? { ...entry, text } : entry);
    transcriptSaveMessage = "有未保存修改";
  }

  async function saveTranscript(): Promise<void> {
    if (!activeItem || savingTranscript) return;
    savingTranscript = true;
    transcriptSaveMessage = "";
    try {
      await invoke("update_video_subtitle", {
        id: activeItem.video.id,
        subtitle: clipTranscriptToSrt(editableEntries),
      });
      transcriptSaveMessage = "已保存";
    } catch (error) {
      transcriptSaveMessage = `保存失败：${String(error).replace(/^Error:\s*/i, "")}`;
    } finally {
      savingTranscript = false;
    }
  }

  function selectIssue(annotation: ScriptIssueAnnotation): void {
    selectedCueId = annotation.cueId;
    seekTo(annotation.startMs / 1000);
  }
</script>

<div class="clip-review-shell">
  <header class="review-header">
    <button type="button" class="back-button" on:click={() => dispatch("close")}>
      <ArrowLeft size={16} /> 返回切片列表
    </button>
    <div class="header-copy">
      <strong>成交切片复盘</strong>
      <span>左侧看视频，中间校对文稿，右侧查看 AI 改法建议</span>
    </div>
    {#if request.items.length > 1}
      <nav class="clip-tabs" aria-label="本次生成的切片">
        {#each request.items as item, index (item.video.id)}
          <button type="button" class:active={index === activeIndex} on:click={() => { activeIndex = index; }}>
            {index + 1}. {item.video.title}
          </button>
        {/each}
      </nav>
    {/if}
  </header>

  {#if activeItem}
    <main class="review-grid">
      <section class="review-pane video-pane" aria-label="切片视频">
        <header>
          <strong>切片视频</strong>
          <span>{formatWorkspaceClock(activeItem.sourceEndSec - activeItem.sourceStartSec)}</span>
        </header>
        <div class="video-stage">
          {#if videoError}
            <div class="empty-state error"><AlertCircle size={24} /><span>{videoError}</span></div>
          {:else if videoUrl}
            <!-- svelte-ignore a11y-media-has-caption -->
            <video
              bind:this={videoElement}
              src={videoUrl}
              controls
              preload="metadata"
              playsinline
              on:error={() => { videoError = "切片文件存在，但播放器暂时无法解码。"; }}
            ></video>
          {:else}
            <div class="empty-state"><Loader2 size={28} class="spin" /><span>正在加载切片视频…</span></div>
          {/if}
        </div>
        <div class="clip-meta">
          <strong>{activeItem.video.title}</strong>
          <span>原视频区间：{formatWorkspaceClock(activeItem.sourceStartSec)} — {formatWorkspaceClock(activeItem.sourceEndSec)}</span>
          {#if activeItem.reason}<p>{activeItem.reason}</p>{/if}
        </div>
      </section>

      <section class="review-pane transcript-pane" aria-label="可编辑切片文稿">
        <header>
          <strong>文稿</strong>
          <div class="transcript-tools">
            <span>{transcriptSaveMessage || `${editableEntries.length} 句 · 可直接修改`}</span>
            <button type="button" disabled={savingTranscript || !editableEntries.length} on:click={() => void saveTranscript()}>
              {savingTranscript ? "保存中…" : "保存文稿"}
            </button>
          </div>
        </header>
        <div class="transcript-list">
          {#if transcriptLoading}
            <div class="empty-state"><Loader2 size={28} class="spin" /><span>正在读取切片文稿；旧切片缺少文稿时会自动识别…</span></div>
          {:else if transcriptError}
            <div class="empty-state error"><AlertCircle size={24} /><span>{transcriptError}</span></div>
          {:else if editableEntries.length}
            {#each editableEntries as entry (entry.id)}
              {@const issue = annotationByCue.get(entry.id)}
              <div class="transcript-row" class:has-issue={Boolean(issue)} class:selected={selectedCueId === entry.id}>
                <button type="button" class="time-button" title="跳到这一句" on:click={() => seekTo(entry.start)}>
                  {formatWorkspaceClock(entry.start)}
                </button>
                <textarea
                  rows="2"
                  value={entry.text}
                  aria-label={`文稿 ${formatWorkspaceClock(entry.start)}`}
                  on:input={(event) => updateEntryText(entry.id, event.currentTarget.value)}
                ></textarea>
              </div>
            {/each}
          {:else}
            <div class="empty-state"><AlertCircle size={24} /><span>此切片范围没有可用文稿。</span></div>
          {/if}
        </div>
      </section>

      <section class="review-pane analysis-pane" aria-label="AI 文稿分析">
        <header>
          <strong>AI 文稿分析</strong>
          <button type="button" disabled={analysisLoading || !editableEntries.length} on:click={() => void runAnalysis()}>
            <Sparkles size={14} />{analysisLoading ? "分析中…" : "重新分析"}
          </button>
        </header>
        <div class="analysis-body">
          {#if analysisLoading}
            <div class="analysis-loading"><Loader2 size={26} class="spin" /><span>AI 正在分析本段成交话术…</span></div>
          {/if}
          {#if analysisError}
            <div class="analysis-error">{analysisError}</div>
          {/if}
          {#if qualitySummary}
            <article class="summary-card">
              <strong>整体结论</strong>
              <p>{qualitySummary}</p>
            </article>
          {/if}
          {#if annotations.length}
            <div class="issue-list">
              {#each annotations as annotation (annotation.cueId)}
                <button type="button" class:selected={selectedCueId === annotation.cueId} on:click={() => selectIssue(annotation)}>
                  <span class="issue-label">{scriptIssueKindLabel(annotation.kind)} · {formatWorkspaceClock(annotation.startMs / 1000)}</span>
                  <strong>{annotation.originalText}</strong>
                  <p>{annotation.reason}</p>
                  <em>{annotation.suggestion}</em>
                </button>
              {/each}
            </div>
          {:else if qualitySummary && !analysisLoading}
            <div class="no-issues">AI 暂未标出重点问题。</div>
          {/if}
        </div>
      </section>
    </main>
  {/if}
</div>

<style>
  .clip-review-shell { width: 100%; height: 100%; min-width: 0; min-height: 0; display: grid; grid-template-rows: auto minmax(0, 1fr); padding: 14px; box-sizing: border-box; overflow: hidden; background: #f5f7fa; }
  .review-header { display: flex; align-items: center; gap: 14px; min-width: 0; padding: 0 2px 12px; }
  .back-button { display: inline-flex; align-items: center; gap: 6px; flex: 0 0 auto; padding: 7px 10px; border: 1px solid #d0d5dd; border-radius: 8px; background: #fff; color: #344054; font-size: 12px; cursor: pointer; }
  .header-copy { min-width: 0; display: grid; gap: 2px; }
  .header-copy strong { color: #101828; font-size: 17px; }
  .header-copy span { color: #667085; font-size: 12px; }
  .clip-tabs { margin-left: auto; min-width: 0; display: flex; gap: 6px; overflow-x: auto; }
  .clip-tabs button { max-width: 190px; padding: 6px 9px; border: 1px solid #d0d5dd; border-radius: 8px; background: #fff; color: #475467; font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  .clip-tabs button.active { border-color: #2e90fa; background: #eff8ff; color: #175cd3; }
  .review-grid { min-width: 0; min-height: 0; display: grid; grid-template-columns: minmax(300px, 36%) minmax(270px, 32%) minmax(270px, 32%); gap: 10px; overflow: hidden; }
  .review-pane { min-width: 0; min-height: 0; display: grid; grid-template-rows: auto minmax(0, 1fr); border: 1px solid #e4e7ec; border-radius: 12px; background: #fff; overflow: hidden; }
  .review-pane > header { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 42px; padding: 8px 11px; border-bottom: 1px solid #eef2f6; }
  .review-pane > header strong { color: #101828; font-size: 13px; }
  .review-pane > header span { color: #667085; font-size: 11px; }
  .video-pane { grid-template-rows: auto minmax(0, 1fr) auto; }
  .video-stage { min-height: 0; display: flex; align-items: stretch; justify-content: stretch; background: #05070c; overflow: hidden; }
  .video-stage video { width: 100%; height: 100%; max-width: 100%; max-height: 100%; object-fit: contain; background: #05070c; }
  .clip-meta { display: grid; gap: 4px; padding: 10px 12px; border-top: 1px solid #eef2f6; }
  .clip-meta strong { font-size: 12px; color: #101828; }
  .clip-meta span, .clip-meta p { margin: 0; font-size: 11px; line-height: 1.5; color: #667085; }
  .transcript-list, .analysis-body { min-height: 0; overflow: auto; }
  .transcript-tools { display: flex; align-items: center; gap: 7px; }
  .transcript-tools button { padding: 5px 8px; border: 1px solid #d0d5dd; border-radius: 7px; background: #fff; color: #344054; font-size: 10px; cursor: pointer; }
  .transcript-tools button:disabled { opacity: .5; cursor: not-allowed; }
  .transcript-list { padding: 7px; }
  .transcript-row { display: grid; grid-template-columns: 50px minmax(0, 1fr); gap: 7px; padding: 7px 4px; border-bottom: 1px solid #f2f4f7; border-left: 3px solid transparent; }
  .transcript-row.has-issue { border-left-color: #f79009; background: #fffcf5; }
  .transcript-row.selected { background: #eff8ff; }
  .time-button { align-self: start; padding: 4px 2px; border: 0; background: transparent; color: #175cd3; font-size: 11px; font-variant-numeric: tabular-nums; cursor: pointer; }
  .transcript-row textarea { width: 100%; min-height: 50px; resize: vertical; box-sizing: border-box; border: 1px solid transparent; border-radius: 7px; padding: 5px 7px; background: transparent; color: #1f2937; font: inherit; font-size: 12px; line-height: 1.55; }
  .transcript-row textarea:hover, .transcript-row textarea:focus { border-color: #b2ddff; background: #fff; outline: none; }
  .analysis-pane > header button { display: inline-flex; align-items: center; gap: 5px; padding: 6px 9px; border: 1px solid #2e90fa; border-radius: 7px; background: #eff8ff; color: #175cd3; font-size: 11px; cursor: pointer; }
  .analysis-pane > header button:disabled { opacity: .5; cursor: not-allowed; }
  .analysis-body { display: flex; flex-direction: column; gap: 9px; padding: 9px; }
  .analysis-loading, .empty-state { min-height: 120px; display: grid; place-content: center; justify-items: center; gap: 8px; color: #667085; font-size: 12px; text-align: center; }
  .analysis-error { padding: 9px 10px; border: 1px solid #fecdca; border-radius: 8px; background: #fef3f2; color: #b42318; font-size: 12px; line-height: 1.5; }
  .summary-card { padding: 10px; border: 1px solid #b2ddff; border-radius: 9px; background: #eff8ff; }
  .summary-card strong { color: #175cd3; font-size: 12px; }
  .summary-card p { margin: 5px 0 0; white-space: pre-wrap; color: #344054; font-size: 12px; line-height: 1.6; }
  .issue-list { display: grid; gap: 8px; }
  .issue-list button { display: grid; gap: 5px; padding: 10px; border: 1px solid #e4e7ec; border-radius: 9px; background: #fff; text-align: left; cursor: pointer; }
  .issue-list button:hover, .issue-list button.selected { border-color: #84caff; background: #f5fbff; }
  .issue-label { color: #b54708; font-size: 10px; font-weight: 600; }
  .issue-list strong { color: #101828; font-size: 12px; line-height: 1.45; }
  .issue-list p, .issue-list em { margin: 0; font-size: 11px; line-height: 1.55; }
  .issue-list p { color: #667085; }
  .issue-list em { color: #027a48; font-style: normal; }
  .no-issues { padding: 18px; color: #667085; font-size: 12px; text-align: center; }
  .empty-state.error { color: #b42318; }
  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1180px) { .review-grid { grid-template-columns: minmax(280px, 38%) minmax(240px, 31%) minmax(240px, 31%); } }
  @media (max-width: 900px) { .clip-review-shell { overflow: auto; } .review-grid { grid-template-columns: 1fr; grid-template-rows: minmax(320px, 52vh) minmax(300px, 44vh) minmax(300px, 44vh); overflow: visible; } .review-header { flex-wrap: wrap; } .clip-tabs { width: 100%; margin-left: 0; } }
</style>
