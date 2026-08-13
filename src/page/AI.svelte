<script lang="ts">
  import { onMount } from "svelte";
  import { aiFetch } from "../lib/aiFetch";
  import { invoke } from "../lib/invoker";
  import { Settings, Send, Sparkles, Trash2, Zap, MessageSquare, Bot, Upload, Database, Clipboard } from "lucide-svelte";
  import createAgent, { type AgentMode } from "../lib/agent/agent";
  import { COMMERCE_REVIEW_PROMPT } from "../lib/agent/prompts";
  import { tools } from "../lib/agent/tools";
  import {
    HumanMessage,
    AIMessage,
    ToolMessage,
  } from "@langchain/core/messages";
  import HumanMessageComponent from "../lib/components/HumanMessage.svelte";
  import AIMessageComponent from "../lib/components/AIMessage.svelte";
  import ProcessingMessageComponent from "../lib/components/ProcessingMessage.svelte";
  import ToolMessageComponent from "../lib/components/ToolMessage.svelte";
  import SettingsModal from "../lib/components/ai/SettingsModal.svelte";
  import ImportVideoDialog from "../lib/components/ImportVideoDialog.svelte";
  import MasterSourceDialog from "../lib/components/master/MasterSourceDialog.svelte";
  import SupportCandidateQueue from "../lib/components/master/SupportCandidateQueue.svelte";
  import CompetitorReferenceQueue from "../lib/components/analysis/CompetitorReferenceQueue.svelte";
  import ReviewSampleDialog from "../lib/components/ai/ReviewSampleDialog.svelte";
  import type { ReviewSample, VideoItem } from "../lib/interface";

  let messages: any[] = [];
  let inputMessage = "";
  let isProcessing = false;
  let messageContainer: HTMLElement;
  let inputAreaHeight = 0;
  let agent = null;
  let agentMode: AgentMode = "commerce-review";
  let showImportDialog = false;
  let masterSourceVideo: VideoItem | null = null;
  let showSupportCandidateQueue = false;
  let showCompetitorReferenceQueue = false;
  let showSampleDialog = false;
  let importWorkflowStatus = "";
  let adminMode = false;
  let transcriptReviewItems: any[] = [];
  let transcriptReviewArchive: any = null;
  const WHOLE_SESSION_MINING_PROMPT_PREFIX = "请对最新一场已结束的直播执行整场样本挖掘";

  type MiningCandidate = {
    start: number;
    end: number;
    type: "成交片段" | "疑似成交片段" | "问价未见成交信号" | "转品/上链接片段" | "讲得散片段" | "无法判断";
    confidence: "高" | "中" | "低";
    product: string;
    evidence: string;
    reason: string;
    verify: string;
  };

  function formatMiningTime(totalSeconds: number): string {
    const safe = Math.max(0, Math.floor(totalSeconds || 0));
    const hours = Math.floor(safe / 3600);
    const minutes = Math.floor((safe % 3600) / 60);
    const seconds = safe % 60;
    return hours > 0
      ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
      : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  onMount(() => {
    const handleArchiveMining = (event: Event) => {
      const archive = (event as CustomEvent).detail;
      void runWholeSessionMining(archive);
    };
    const handleArchiveTranscription = (event: Event) => {
      const archive = (event as CustomEvent).detail;
      void runArchiveTranscription(archive);
    };
    window.addEventListener("bsr:analyze-archive", handleArchiveMining);
    window.addEventListener("bsr:transcribe-archive", handleArchiveTranscription);
    return () => {
      window.removeEventListener("bsr:analyze-archive", handleArchiveMining);
      window.removeEventListener("bsr:transcribe-archive", handleArchiveTranscription);
    };
  });

  async function runArchiveTranscription(archive: any) {
    if (isProcessing) return;
    isProcessing = true;
    importWorkflowStatus = `正在读取《${archive.title}》的逐字稿...`;
    const request = new HumanMessage({ content: `请将录播《${archive.title}》生成逐字稿` });
    request.additional_kwargs = { timestamp: new Date().toISOString() };
    messages = [...messages, request];
    scrollToBottom();
    try {
      // An explicit click always runs the current pipeline. This prevents an
      // older subtitle.srt from silently bypassing new hotwords, constrained
      // proofreading and audit-artifact generation.
      importWorkflowStatus = "正在提取音频并使用 FunASR 生成整场逐字稿...";
      const subtitle = await invoke<string>("generate_archive_subtitle", {
        platform: archive.platform,
        roomId: String(archive.room_id),
        liveId: String(archive.live_id),
      });
      if (!subtitle.trim()) throw new Error("没有识别到可用文字，请检查录播是否包含清晰人声。 ");

      let audit: any = null;
      try {
        audit = await invoke("get_archive_transcript_audit", {
          platform: archive.platform,
          roomId: String(archive.room_id),
          liveId: String(archive.live_id),
        });
      } catch {
        // Older transcripts do not have the V1.2 audit bundle.
      }
      let reviewItems: any[] = [];
      try {
        reviewItems = audit?.review_json ? JSON.parse(audit.review_json) : [];
      } catch {
        reviewItems = [];
      }
      reviewItems = reviewItems.map((item) => ({
        ...item,
        correction: item.correction || item.recognized || "",
        saving: false,
      }));
      transcriptReviewItems = reviewItems;
      transcriptReviewArchive = archive;
      const reviewSummary = reviewItems.length
        ? `\n\n### 待回听事实（${reviewItems.length}项）\n${reviewItems.slice(0, 30).map((item) =>
            `- ${formatMiningTime((item.start_ms || 0) / 1000)}–${formatMiningTime((item.end_ms || 0) / 1000)}｜${item.type}｜${item.recognized}｜${item.status}`
          ).join("\n")}`
        : "\n\n### 待回听事实\n未发现需要核验的价格、型号、成色或链接编号，或该文稿由旧版本生成。";
      const auditStatus = audit?.raw_srt
        ? "原始稿已独立保存；下方展示校准稿。任何校对都不会覆盖原始识别结果。"
        : "这是旧版已存在文稿，尚无原始稿/校准稿审计包；重新生成后可获得完整追溯文件。";
      const result = new AIMessage({
        content: `## 整场逐字稿已完成\n\n**录播：** ${archive.title}\n**可追溯状态：** ${auditStatus}\n**下一步：** 先核验待回听事实，再返回录播页点击“分析片段”。${reviewSummary}\n\n### 校准逐字稿\n\n${subtitle}`,
      });
      result.additional_kwargs = {
        timestamp: new Date().toISOString(),
        isTranscript: true,
        sourceArchive: archive,
      };
      messages = [...messages, result];
      localStorage.setItem("messages", JSON.stringify(messages));
      window.dispatchEvent(new CustomEvent("bsr:archive-transcript-ready", { detail: archive }));
    } catch (error) {
      const message = error?.message || String(error);
      const failed = new AIMessage({ content: `❌ **整场逐字稿生成失败**\n\n${message}` });
      failed.additional_kwargs = { timestamp: new Date().toISOString(), isError: true };
      messages = [...messages, failed];
      window.dispatchEvent(new CustomEvent("bsr:archive-transcript-failed", {
        detail: { archive, error: message },
      }));
    } finally {
      importWorkflowStatus = "";
      isProcessing = false;
      scrollToBottom();
    }
  }

  async function replayTranscriptFact(item: any) {
    if (!transcriptReviewArchive) return;
    await invoke("open_live", {
      platform: transcriptReviewArchive.platform,
      roomId: String(transcriptReviewArchive.room_id),
      liveId: String(transcriptReviewArchive.live_id),
      start: Math.floor((item.start_ms || 0) / 1000),
      end: Math.ceil((item.end_ms || 0) / 1000),
    });
  }

  async function saveTranscriptFact(index: number, item: any) {
    if (!transcriptReviewArchive || !item.correction?.trim() || item.saving) return;
    item.saving = true;
    transcriptReviewItems = [...transcriptReviewItems];
    try {
      const audit: any = await invoke("resolve_archive_review_item", {
        platform: transcriptReviewArchive.platform,
        roomId: String(transcriptReviewArchive.room_id),
        liveId: String(transcriptReviewArchive.live_id),
        index,
        correction: item.correction.trim(),
      });
      const updated = audit?.review_json ? JSON.parse(audit.review_json) : [];
      transcriptReviewItems = updated.map((entry) => ({
        ...entry,
        correction: entry.correction || entry.recognized || "",
        saving: false,
      }));
    } catch (error) {
      item.saving = false;
      item.error = error?.message || String(error);
      transcriptReviewItems = [...transcriptReviewItems];
    }
  }

  // 设置相关状态
  let showSettings = false;
  let isLoadingModels = false;
  let settings = {
    provider: "minimax" as "openai" | "minimax" | "ollama",
    endpoint: "https://api.minimaxi.com/anthropic",
    api_key: "",
    model: "MiniMax-VL-01"
  };

  let availableModels = [];
  const toolCallStates = new Map<string, 'confirmed' | 'rejected' | 'none'>();

  // 预设提示词
  const generalPresetPrompts = [
    { title: "录制直播", description: "添加新的直播间并开始录制", prompt: "我该如何添加新的直播间？", icon: "📡" },
    { title: "查看任务", description: "显示所有录制任务状态", prompt: "显示我所有的录制任务", icon: "📊" },
    { title: "管理账户", description: "查看已添加的账户信息", prompt: "显示可用的账号信息", icon: "👤" },
    { title: "生成切片", description: "分析录播并生成精彩片段", prompt: "分析最新录制的录播有哪些精彩部分，选择一段生成切片", icon: "✂️" },
    { title: "视频转码", description: "转换视频格式", prompt: "帮我将视频转码为mp4格式", icon: "🎬" },
    { title: "提取音频", description: "从视频中提取音频", prompt: "帮我提取视频中的音频", icon: "🎵" }
  ];

  const commercePresetPrompts = [
    { title: "整场自动挖样本", description: "从整场录播定位并切出五类训练片段", prompt: "请对最新一场已结束的直播执行整场样本挖掘：先获取或生成带时间戳逐字稿，再按时间顺序分块筛选候选片段，合并重叠区间，分别寻找成交、疑似成交、问价未见成交信号、转品/上链接、讲得散片段。每个候选必须给出时间范围、原文证据和分类置信度；不得把报价或上链接当成成交。然后调用切片工具生成候选视频，并对每个切片按 V1.1 做复盘。", icon: "🧪" },
    { title: "复盘逐字稿", description: "粘贴逐字稿后判断片段类型", prompt: "请按直播成交片段复盘官 V1.1 分析我接下来提交的逐字稿；信息不足时仅做片段级判断。\n\n[请在这里粘贴逐字稿]", icon: "📋" },
    { title: "成交片段诊断", description: "拆解成交机制并生成参考表达", prompt: "请复盘我指定的成交片段，核验证据、拆解逐句话术，并输出须经人工定稿的参考改写建议。", icon: "🎯" },
    { title: "未成交诊断", description: "定位问价后没有推进的断点", prompt: "请分析我指定的问价未成交片段，找出交易链路停在哪一步，并给出最小修改口播。", icon: "🔎" },
    { title: "转品与上链接", description: "检查商品、价格、链接和CTA承接", prompt: "请分析我指定的转品或上链接片段，检查商品与链接是否一致、承接是否清楚。", icon: "🔗" },
    { title: "讲得散诊断", description: "识别打断、重复和主线缺失", prompt: "请分析我指定的直播片段为什么讲得散，并重排成连续的交易主线。", icon: "🧭" },
    { title: "轻量回归检查", description: "检查V1.1分类、标签和事实边界", prompt: "请对我指定的既有样本做 V1.1 轻量回归检查，不需要完整重写复盘。", icon: "✅" }
  ];

  $: presetPrompts = agentMode === "commerce-review" ? commercePresetPrompts : generalPresetPrompts;
  $: aiReady = Boolean(agent) || settings.provider === "minimax";
  $: assistantTitle = agentMode === "commerce-review" ? "直播成交片段复盘官" : "小轴";
  $: assistantDescription = agentMode === "commerce-review"
    ? "读取录播字幕，预分类片段，核验证据并生成须经人工定稿的参考表达。"
    : "管理直播录制、生成精彩切片并分析弹幕内容。";
  $: lastReviewContent = getLastReviewContent(messages);

  function getLastReviewContent(items: any[]): string {
    for (let i = items.length - 1; i >= 0; i -= 1) {
      const item = items[i];
      if (!(item instanceof AIMessage) || item.tool_calls?.length) continue;
      if (typeof item.content === "string") return item.content;
      if (Array.isArray(item.content)) {
        return item.content
          .map((block: any) => typeof block === "string" ? block : block?.text || "")
          .filter(Boolean)
          .join("\n");
      }
    }
    return "";
  }

  function openSettings() { showSettings = true; }
  function closeSettings() { showSettings = false; }

  async function saveSettings() {
    if (settings.provider === 'minimax') {
      settings.endpoint = 'https://api.minimaxi.com/anthropic';
      settings.model = settings.model || 'MiniMax-VL-01';
    }
    if (settings.provider === 'minimax' && settings.api_key) {
      await invoke('update_openai_api_key', { openaiApiKey: settings.api_key });
    }
    // Provider keys are backend-only. Never persist them in WebView storage.
    const safeSettings = { ...settings, api_key: '' };
    localStorage.setItem('ai_settings', JSON.stringify(safeSettings));
    settings.api_key = '';
    if (settings.provider === 'ollama') {
      if (settings.endpoint || settings.model) {
        agent = createAgent({
          provider: 'ollama',
          baseURL: settings.endpoint || 'http://localhost:11434',
          model: settings.model || 'llama2',
          mode: agentMode,
        });
      } else {
        agent = null;
      }
    } else {
      if (settings.api_key && settings.endpoint) {
        agent = createAgent({
          provider: settings.provider,
          apiKey: settings.api_key,
          baseURL: settings.endpoint,
          model: settings.model || undefined,
          mode: agentMode,
        });
        await loadModels();
      } else {
        agent = null;
      }
    }
    closeSettings();
  }

  async function fetchModels(endpoint: string, apiKey: string) {
    try {
      const response = await aiFetch(`${endpoint.replace(/\/+$/, '')}/models`, {
        headers: { 'Authorization': `Bearer ${apiKey}`, 'Content-Type': 'application/json' }
      });
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
      const data = await response.json();
      if (data.data && Array.isArray(data.data)) {
        return data.data.map((model: any) => ({ value: model.id, label: model.id }));
      }
      return [];
    } catch (error) {
      console.error('Failed to fetch models:', error);
      throw error;
    }
  }

  async function loadModels() {
    if (settings.provider === 'minimax') {
      availableModels = [{ value: 'MiniMax-VL-01', label: 'MiniMax-VL-01' }];
      settings.model = settings.model || 'MiniMax-VL-01';
      return;
    }
    if (settings.endpoint && settings.api_key) {
      isLoadingModels = true;
      try {
        const models = await fetchModels(settings.endpoint, settings.api_key);
        if (models.length > 0) availableModels = models;
      } catch (error) {
        console.error('Failed to load models:', error);
      } finally {
        isLoadingModels = false;
      }
    }
  }

  function loadSettings() {
    const savedSettings = localStorage.getItem('ai_settings');
    if (savedSettings) {
      const parsed = JSON.parse(savedSettings);
      delete parsed.api_key;
      settings = { ...settings, ...parsed, api_key: '' };
      // Migrate older generic OpenAI-compatible MiniMax settings to the
      // dedicated provider and the supported OpenAI-compatible endpoint.
      if (settings.endpoint?.includes('minimaxi.com')) {
        settings.provider = 'minimax';
        settings.endpoint = 'https://api.minimaxi.com/anthropic';
        settings.model = settings.model || 'MiniMax-VL-01';
        localStorage.setItem('ai_settings', JSON.stringify({ ...settings, api_key: '' }));
      }
      if (settings.provider === 'ollama') {
        if (settings.endpoint || settings.model) {
          agent = createAgent({
            provider: 'ollama',
            baseURL: settings.endpoint || 'http://localhost:11434',
            model: settings.model || 'llama2',
            mode: agentMode,
          });
        }
      } else {
        if (settings.api_key && settings.endpoint) {
          agent = createAgent({
            provider: settings.provider,
            apiKey: settings.api_key,
            baseURL: settings.endpoint,
            model: settings.model || undefined,
            mode: agentMode,
          });
          loadModels();
        }
      }
    }
  }

  function getToolCallState(message: any): 'confirmed' | 'rejected' | 'none' {
    if (message.tool_calls && message.tool_calls.length > 0) {
      return toolCallStates.get(message.tool_calls[0]?.id) || 'none';
    }
    return 'none';
  }

  function scrollToBottom() {
    if (messageContainer) {
      setTimeout(() => { messageContainer.scrollTop = messageContainer.scrollHeight; }, 100);
    }
  }

  function formatTime(timestamp: string): string {
    const date = new Date(timestamp);
    return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  }

  function isSensitiveToolCall(message: any): boolean {
    if (!message.tool_calls || message.tool_calls.length === 0) return false;
    const sensitiveTools = ['delete_recorder', 'delete_archive', 'delete_video'];
    return message.tool_calls.some((toolCall: any) => sensitiveTools.includes(toolCall.name));
  }

  async function handlePresetPrompt(prompt: string) {
    if (prompt.startsWith(WHOLE_SESSION_MINING_PROMPT_PREFIX)) {
      await runWholeSessionMining();
      return;
    }
    inputMessage = prompt;
    await sendMessage();
  }

  function srtTimeToSeconds(value: string): number {
    const parts = value.replace(",", ".").split(":").map(Number);
    return parts.length === 3 ? parts[0] * 3600 + parts[1] * 60 + parts[2] : 0;
  }

  function splitSrtIntoMiningChunks(srt: string, chunkSeconds = 480, overlapSeconds = 30): string[] {
    const entries = srt.trim().split(/\r?\n\r?\n+/).map((block) => {
      const lines = block.split(/\r?\n/);
      const timingIndex = lines.findIndex((line) => line.includes("-->"));
      if (timingIndex < 0) return null;
      const [from, to] = lines[timingIndex].split("-->").map((item) => item.trim());
      return {
        start: srtTimeToSeconds(from),
        end: srtTimeToSeconds(to),
        block,
      };
    }).filter(Boolean) as Array<{ start: number; end: number; block: string }>;
    if (!entries.length) return srt.trim() ? [srt] : [];

    const duration = Math.max(...entries.map((entry) => entry.end));
    const chunks: string[] = [];
    for (let start = 0; start < duration; start += chunkSeconds) {
      const end = start + chunkSeconds;
      const blocks = entries
        .filter((entry) => entry.end >= Math.max(0, start - overlapSeconds) && entry.start <= end + overlapSeconds)
        .map((entry) => entry.block);
      if (blocks.length) chunks.push(blocks.join("\n\n"));
    }
    return chunks;
  }

  function parseMiningCandidates(content: string): MiningCandidate[] {
    const match = content.match(/\[[\s\S]*\]/);
    if (!match) return [];
    try {
      const parsed = JSON.parse(match[0]);
      if (!Array.isArray(parsed)) return [];
      const allowedTypes = new Set(["成交片段", "疑似成交片段", "问价未见成交信号", "转品/上链接片段", "讲得散片段", "无法判断"]);
      return parsed.filter((item) =>
        Number.isFinite(Number(item?.start)) &&
        Number.isFinite(Number(item?.end)) &&
        Number(item.end) > Number(item.start) &&
        allowedTypes.has(item?.type)
      ).map((item) => ({
        start: Math.max(0, Number(item.start)),
        end: Math.max(0, Number(item.end)),
        type: item.type,
        confidence: ["高", "中", "低"].includes(item.confidence) ? item.confidence : "低",
        product: String(item.product || "主商品待确认"),
        evidence: String(item.evidence || ""),
        reason: String(item.reason || ""),
        verify: String(item.verify || ""),
      }));
    } catch {
      return [];
    }
  }

  function selectMiningCandidates(items: MiningCandidate[]): MiningCandidate[] {
    const score = { "高": 3, "中": 2, "低": 1 };
    const deduped: MiningCandidate[] = [];
    for (const item of items.sort((a, b) => a.start - b.start || score[b.confidence] - score[a.confidence])) {
      const duplicate = deduped.some((saved) =>
        saved.type === item.type && Math.min(saved.end, item.end) - Math.max(saved.start, item.start) > 10
      );
      if (!duplicate) deduped.push(item);
    }
    const counts = new Map<string, number>();
    return deduped
      .sort((a, b) => score[b.confidence] - score[a.confidence] || a.start - b.start)
      .filter((item) => {
        const count = counts.get(item.type) || 0;
        if (count >= 3 || item.type === "无法判断") return false;
        counts.set(item.type, count + 1);
        return true;
      })
      .sort((a, b) => a.start - b.start);
  }

  async function runWholeSessionMining(preferredArchive: any = null) {
    if (isProcessing) return;
    isProcessing = true;
    importWorkflowStatus = "正在查找最近一场已结束的录播...";
    const request = new HumanMessage({ content: WHOLE_SESSION_MINING_PROMPT_PREFIX });
    request.additional_kwargs = { timestamp: new Date().toISOString() };
    messages = [...messages, request];
    scrollToBottom();

    try {
      const [records, recorderList, videos] = await Promise.all([
        invoke<any[]>("get_recent_record", { roomId: "0", offset: 0, limit: 30 }),
        invoke<any>("get_recorder_list"),
        invoke<any[]>("get_all_videos"),
      ]);
      const activeLiveIds = new Set(
        (recorderList?.recorders || []).filter((item: any) => item.recording).map((item: any) => String(item.live_id))
      );
      if (preferredArchive && activeLiveIds.has(String(preferredArchive.live_id))) {
        throw new Error("这场直播仍在录制。请在直播结束、文件停止增长后再执行整场分析。 ");
      }
      const archive = preferredArchive || (records || []).find((item: any) => !activeLiveIds.has(String(item.live_id)));
      const sourceVideo = !preferredArchive && !archive
        ? (videos || []).find((item: any) => item.platform === "imported" || item.room_id === "bsr:import")
        : null;
      if (!archive && !sourceVideo) {
        throw new Error("没有找到已结束的录播或已导入的完整视频。正在录制的文件不会参与整场分析。 ");
      }
      const sourceTitle = archive?.title || sourceVideo?.title || sourceVideo?.file || "整场录播";

      importWorkflowStatus = `正在读取《${sourceTitle}》的逐字稿...`;
      let subtitle = "";
      if (archive) {
        let hasCurrentPipelineAudit = false;
        try {
          const audit = await invoke<any>("get_archive_transcript_audit", {
            platform: archive.platform,
            roomId: String(archive.room_id),
            liveId: String(archive.live_id),
          });
          hasCurrentPipelineAudit = Boolean(
            audit?.evidence_json?.trim() ||
              audit?.raw_srt?.trim() ||
              audit?.corrected_srt?.trim(),
          );
        } catch {
          // A missing audit means the subtitle predates the current pipeline.
        }
        try {
          subtitle = await invoke<string>("get_archive_subtitle", {
            platform: archive.platform,
            roomId: String(archive.room_id),
            liveId: String(archive.live_id),
          });
        } catch {
          // Missing subtitles are generated below.
        }
        if (!subtitle.trim() || !hasCurrentPipelineAudit) {
          if (preferredArchive) {
            importWorkflowStatus = "检测到旧版逐字稿，正在使用当前识别链路重新生成...";
          } else {
            importWorkflowStatus = "正在使用当前识别链路生成整场逐字稿，这一步可能需要几分钟...";
          }
          subtitle = await invoke<string>("generate_archive_subtitle", {
            platform: archive.platform,
            roomId: String(archive.room_id),
            liveId: String(archive.live_id),
          });
        }
      } else {
        subtitle = await invoke<string>("get_video_subtitle", { id: sourceVideo.id });
        if (!subtitle.trim()) {
          importWorkflowStatus = "正在使用 FunASR 生成导入录播的逐字稿，这一步可能需要几分钟...";
          subtitle = await invoke<string>("generate_video_subtitle", {
            eventId: `whole_mining_source_${sourceVideo.id}_${Date.now()}`,
            id: sourceVideo.id,
          });
        }
      }
      const chunks = splitSrtIntoMiningChunks(subtitle);
      if (!chunks.length) throw new Error("逐字稿为空，无法挖掘片段。 ");

      const candidates: MiningCandidate[] = [];
      for (let index = 0; index < chunks.length; index += 1) {
        importWorkflowStatus = `正在分析逐字稿分块 ${index + 1}/${chunks.length}...`;
        const response = await invoke<string>("minimax_chat", {
          systemPrompt: `你是直播样本候选定位器。只返回 JSON 数组，不要 Markdown。每项字段必须为 start、end、type、confidence、product、evidence、reason、verify。type 只能为：成交片段、疑似成交片段、问价未见成交信号、转品/上链接片段、讲得散片段、无法判断。成交片段必须有明确成交确认、锁货、备注、恭喜下单或可匹配订单；仅报价、上链接、购买CTA不得判定成交。没有订单证据时，“未成交”只能写问价未见成交信号。时间必须来自字幕且为相对录播开始的秒数。建议保留触发点前30—90秒和后20—30秒。商品、价格、库存、链接号不确定时写待确认，不得编造。无合格候选时返回 []。`,
          messages: [{ role: "user", content: chunks[index] }],
        });
        candidates.push(...parseMiningCandidates(response));
      }

      const selected = selectMiningCandidates(candidates);
      if (!selected.length) throw new Error("模型没有找到证据充分的候选片段；程序没有为了凑数而生成切片。 ");

      const generated: any[] = [];
      const reviews: Array<{ candidate: MiningCandidate; content: string }> = [];
      for (let index = 0; index < selected.length; index += 1) {
        const candidate = selected[index];
        importWorkflowStatus = `正在生成候选切片 ${index + 1}/${selected.length}：${candidate.type}...`;
        const clipTitle = `[${candidate.type}][${candidate.product}]${candidate.reason || "直播候选片段"}`;
        const video = archive
          ? await invoke<any>("clip_range", {
              eventId: `whole_mining_${Date.now()}_${index}`,
              params: {
                room_id: String(archive.room_id),
                live_id: String(archive.live_id),
                ranges: [{ start: candidate.start, end: candidate.end }],
                danmu: false,
                local_offset: 0,
                title: clipTitle,
                note: `原文证据：${candidate.evidence}\n证据强度：${candidate.confidence}\n待核验：${candidate.verify || "无"}\n信息不足时仅做片段级判断。`,
                cover: "",
                platform: archive.platform,
                fix_encoding: false,
                transition: "none",
              },
            })
          : await invoke<any>("clip_video", {
              eventId: `whole_mining_video_${Date.now()}_${index}`,
              parentVideoId: sourceVideo.id,
              startTime: candidate.start,
              endTime: candidate.end,
              clipTitle,
            });
        generated.push({ candidate, video });

        if (video?.id) {
          importWorkflowStatus = `正在复盘候选切片 ${index + 1}/${selected.length}：${candidate.type}...`;
          let clipSubtitle = await invoke<string>("get_video_subtitle", { id: video.id });
          if (!clipSubtitle?.trim()) {
            clipSubtitle = await invoke<string>("generate_video_subtitle", {
              eventId: `whole_mining_subtitle_${video.id}_${Date.now()}`,
              id: video.id,
            });
          }
          const review = await invoke<string>("minimax_chat", {
            systemPrompt: COMMERCE_REVIEW_PROMPT,
            messages: [{
              role: "user",
              content: `请复盘下面这段由整场录播自动定位的候选片段。候选类型只作为待核验线索，你必须根据逐字稿重新预分类。没有订单时间或明确成交确认时，不得写成已成交。\n\n候选类型：${candidate.type}\n候选商品：${candidate.product}\n候选时间：${candidate.start.toFixed(1)}s—${candidate.end.toFixed(1)}s\n\n逐字稿：\n${clipSubtitle}`,
            }],
          });
          reviews.push({ candidate, content: review });
          await invoke("update_video_note", {
            id: video.id,
            note: `原文证据：${candidate.evidence}\n证据强度：${candidate.confidence}\n待核验：${candidate.verify || "无"}\n\n【自动复盘 V1.1】\n${review}`,
          });
        }
      }

      const rows = selected.map((item, index) =>
        `${index + 1}. ${item.start.toFixed(1)}s—${item.end.toFixed(1)}s｜${item.type}｜${item.product}｜${item.confidence}｜${item.evidence}`
      ).join("\n");
      const reviewSummary = reviews.map((item, index) =>
        `\n\n---\n\n### 片段 ${index + 1}：${item.candidate.type}｜${item.candidate.product}\n\n${item.content}`
      ).join("");
      const result = new AIMessage({
        content: `## 整场样本挖掘完成\n\n**录播：** ${sourceTitle}\n**来源：** ${archive ? "已结束录播" : "导入的完整视频"}\n**分析分块：** ${chunks.length}\n**生成切片：** ${generated.length}\n**完成复盘：** ${reviews.length}\n\n${rows}\n\n切片已进入“切片”页面，完整复盘也已写入每条切片备注。分类只代表对应观察窗口；没有片段级订单时，不把相关话术写成成交因果。${reviewSummary}`,
      });
      result.additional_kwargs = { timestamp: new Date().toISOString(), wholeSessionMining: true };
      messages = [...messages, result];
      localStorage.setItem("messages", JSON.stringify(messages));
    } catch (error) {
      const failed = new AIMessage({ content: `❌ **整场样本挖掘失败**\n\n${error?.message || String(error)}` });
      failed.additional_kwargs = { timestamp: new Date().toISOString(), isError: true };
      messages = [...messages, failed];
    } finally {
      importWorkflowStatus = "";
      isProcessing = false;
      scrollToBottom();
    }
  }

  async function sendMessage() {
    if (!inputMessage.trim() || isProcessing || (!agent && settings.provider !== 'minimax')) return;
    const userMessage = new HumanMessage({ content: inputMessage });
    userMessage.additional_kwargs = { ...userMessage.additional_kwargs, timestamp: new Date().toISOString() };
    messages = [...messages, userMessage];
    inputMessage = "";
    isProcessing = true;
    scrollToBottom();
    try {
      if (settings.provider === 'minimax') {
        await continueMiniMaxFlow();
      } else {
        await continueAgentFlow([userMessage]);
      }
    } finally {
      isProcessing = false;
      scrollToBottom();
    }
  }

  // Define tool whitelist - tools that auto-execute without user confirmation
  const autoExecuteTools = new Set([
    'get_accounts',
    'get_recorder_list',
    'get_recorder_info',
    'get_archives',
    'get_archive',
    'get_background_tasks',
    'get_videos',
    'get_all_videos',
    'get_video',
    'get_video_cover',
    'get_video_typelist',
    'get_video_subtitle',
    'get_danmu_record',
    'get_recent_record',
    'get_recent_record_all',
    'get_archive_subtitle',
    'get_video_metadata',
    'get_archive_metadata',
    'get_review_samples',
  ]);

  async function continueAgentFlow(newMessages: any[]) {
    const config = { configurable: { thread_id: "1" } };

    try {
      const stream = await agent.stream({ messages: newMessages }, config);

      let lastAIMessage = null;
      for await (const chunk of stream) {
        if (chunk.agent) {
          const agentMessages = chunk.agent.messages;
          for (const msg of agentMessages) {
            if (msg instanceof AIMessage) {
              msg.additional_kwargs = { ...msg.additional_kwargs, timestamp: new Date().toISOString() };
              messages = [...messages, msg];
              lastAIMessage = msg;
              scrollToBottom();
            }
          }
        }
      }

    localStorage.setItem('messages', JSON.stringify(messages));
    localStorage.setItem('ai_messages_mode', agentMode);
    const toolCallStatesObj = Object.fromEntries(toolCallStates);
    localStorage.setItem('toolCallStates', JSON.stringify(toolCallStatesObj));

    // After stream ends, check if we need to auto-execute whitelisted tools
    if (lastAIMessage?.tool_calls && lastAIMessage.tool_calls.length > 0) {
      const autoExecutableTools = lastAIMessage.tool_calls.filter(
        (toolCall: any) => autoExecuteTools.has(toolCall.name)
      );

      // If all tool calls are whitelisted, auto-execute them
      if (autoExecutableTools.length === lastAIMessage.tool_calls.length) {
        const toolResults: any[] = [];

        for (const toolCall of autoExecutableTools) {
          // Mark as confirmed
          toolCallStates.set(toolCall.id, 'confirmed');

          // Execute tool
          const tool = tools.find(t => t.name === toolCall.name);
          if (!tool) {
            console.error(`Tool ${toolCall.name} not found`);
            const errorDetails = {
              error: true,
              message: `Tool ${toolCall.name} not found`,
              tool: toolCall.name,
            };
            const errorMessage = new ToolMessage({
              name: toolCall.name,
              content: JSON.stringify(errorDetails),
              tool_call_id: toolCall.id || `tool_${Date.now()}`,
            });
            errorMessage.additional_kwargs = { ...errorMessage.additional_kwargs, timestamp: new Date().toISOString() };
            toolResults.push(errorMessage);
            continue;
          }

          try {
            const result = await tool.invoke(toolCall.args);
            const resultMessage = new ToolMessage({
              name: toolCall.name,
              content: typeof result === 'string' ? result : JSON.stringify(result),
              tool_call_id: toolCall.id || `tool_${Date.now()}`,
            });
            resultMessage.additional_kwargs = { ...resultMessage.additional_kwargs, timestamp: new Date().toISOString() };
            toolResults.push(resultMessage);
          } catch (error) {
            console.error(`Error executing tool ${toolCall.name}:`, error);
            const errorDetails = {
              error: true,
              message: error.message || String(error),
              tool: toolCall.name,
              args: toolCall.args,
            };
            const errorMessage = new ToolMessage({
              name: toolCall.name,
              content: JSON.stringify(errorDetails),
              tool_call_id: toolCall.id || `tool_${Date.now()}`,
            });
            errorMessage.additional_kwargs = { ...errorMessage.additional_kwargs, timestamp: new Date().toISOString() };
            toolResults.push(errorMessage);
          }
        }

        // Add all tool results to messages
        messages = [...messages, ...toolResults];
        scrollToBottom();

        // Continue agent flow with all tool results
        await continueAgentFlow(toolResults);
      }
    }
    } catch (error) {
      console.error('LLM API Error:', error);

      // Create error message to display to user
      const errorMessage = new AIMessage({
        content: `❌ **LLM API 错误**\n\n${error.message || String(error)}\n\n请检查：\n- API 端点是否正确\n- API 密钥是否有效\n- 网络连接是否正常\n- 模型名称是否正确`,
      });
      errorMessage.additional_kwargs = {
        ...errorMessage.additional_kwargs,
        timestamp: new Date().toISOString(),
        isError: true
      };
      messages = [...messages, errorMessage];
      scrollToBottom();
    }
  }

  async function handleToolCallConfirm(toolCall: any) {
    isProcessing = true;
    try {
      // 立即更新状态为 confirmed
      toolCallStates.set(toolCall.id, 'confirmed');
      messages = [...messages]; // 触发响应式更新
      scrollToBottom();

      const tool = tools.find(t => t.name === toolCall.name);
      if (!tool) {
        throw new Error(`Tool ${toolCall.name} not found`);
      }

      let resultMessage: ToolMessage;
      try {
        const result = await tool.invoke(toolCall.args);
        resultMessage = new ToolMessage({
          name: toolCall.name,
          content: typeof result === 'string' ? result : JSON.stringify(result),
          tool_call_id: toolCall.id || `tool_${Date.now()}`,
        });
      } catch (error) {
        // Wrap error as tool result instead of throwing
        console.error(`Error executing tool ${toolCall.name}:`, error);
        const errorDetails = {
          error: true,
          message: error.message || String(error),
          tool: toolCall.name,
          args: toolCall.args,
        };
        resultMessage = new ToolMessage({
          name: toolCall.name,
          content: JSON.stringify(errorDetails),
          tool_call_id: toolCall.id || `tool_${Date.now()}`,
        });
      }

      resultMessage.additional_kwargs = { ...resultMessage.additional_kwargs, timestamp: new Date().toISOString() };

      // 添加工具结果消息到对话中
      messages = [...messages, resultMessage];
      scrollToBottom();

      await continueAgentFlow([resultMessage]);
    } finally {
      isProcessing = false;
      scrollToBottom();
    }
  }

  async function handleToolCallReject(toolCall: any) {
    // 立即更新状态为 rejected
    toolCallStates.set(toolCall.id, 'rejected');
    messages = [...messages]; // 触发响应式更新
    scrollToBottom();

    const resultMessage = new ToolMessage({
      name: toolCall.name,
      content: "用户选择拒绝执行工具",
      tool_call_id: toolCall.id || `tool_${Date.now()}`,
    });
    resultMessage.additional_kwargs = { ...resultMessage.additional_kwargs, timestamp: new Date().toISOString() };

    // 添加拒绝消息到对话中
    messages = [...messages, resultMessage];
    scrollToBottom();

    await continueAgentFlow([resultMessage]);
    scrollToBottom();
  }

  async function clearConversation() {
    messages = [];
    toolCallStates.clear();
    localStorage.removeItem('messages');
    localStorage.removeItem('toolCallStates');
    localStorage.removeItem('ai_messages_mode');
    if (settings.provider === 'ollama') {
      if (settings.endpoint || settings.model) {
        agent = createAgent({
          provider: 'ollama',
          baseURL: settings.endpoint || 'http://localhost:11434',
          model: settings.model || 'llama2',
          mode: agentMode,
        });
      }
    } else {
      if (settings.api_key && settings.endpoint) {
        agent = createAgent({
          provider: settings.provider,
          apiKey: settings.api_key,
          baseURL: settings.endpoint,
          model: settings.model || undefined,
          mode: agentMode,
        });
      }
    }
    scrollToBottom();
  }

  async function continueMiniMaxFlow() {
    try {
      const latestTranscriptIndex = messages.findLastIndex(
        (message) => message.additional_kwargs?.isTranscript,
      );
      const relevantMessages = latestTranscriptIndex >= 0
        ? messages.slice(latestTranscriptIndex)
        : messages;
      const history = relevantMessages
        .filter((message) => message instanceof HumanMessage || message instanceof AIMessage)
        .filter((message) => !message.additional_kwargs?.isError)
        .map((message) => ({
          role: message instanceof HumanMessage ? 'user' : 'assistant',
          content: typeof message.content === 'string' ? message.content : JSON.stringify(message.content),
        }));
      const content = await invoke<string>('minimax_chat', {
        systemPrompt: COMMERCE_REVIEW_PROMPT,
        messages: history,
      });
      const response = new AIMessage({ content });
      response.additional_kwargs = { timestamp: new Date().toISOString() };
      messages = [...messages, response];
      localStorage.setItem('messages', JSON.stringify(messages));
    } catch (error) {
      const response = new AIMessage({
        content: `❌ **MiniMax 调用失败**\n\n${error?.message || String(error)}`,
      });
      response.additional_kwargs = {
        timestamp: new Date().toISOString(),
        isError: true,
      };
      messages = [...messages, response];
    }
  }

  async function handleVideoImported(
    event: CustomEvent<{ videoId?: number; videoIds?: number[]; asMaster?: boolean }>,
  ) {
    showImportDialog = false;
    try {
      const requestedVideoId = Number(event.detail?.videoId);
      let importedVideo: VideoItem | null = null;

      if (Number.isFinite(requestedVideoId) && requestedVideoId > 0) {
        importedVideo = await invoke<VideoItem>('get_video', { id: requestedVideoId });
      }

      if (!importedVideo) {
        const videos = await invoke<VideoItem[]>('get_all_videos');
        importedVideo = videos
          .filter((video) => video?.platform === 'imported' || video?.room_id === 'bsr:import')
          .sort((left, right) => {
            const leftTime = Date.parse(left?.created_at || '') || Number(left?.id) || 0;
            const rightTime = Date.parse(right?.created_at || '') || Number(right?.id) || 0;
            return rightTime - leftTime;
          })[0] || videos.sort((left, right) => Number(right?.id || 0) - Number(left?.id || 0))[0];
      }

      if (!importedVideo) {
        throw new Error("没有找到刚导入的视频，请重新导入后再试。");
      }
      if (event.detail?.asMaster) {
        alert("已废弃「导入后作为整场直播母稿」。请从成交话术精炼后，经 Clip「母稿样本批次」发布到「主播知识库」的 视频/成交、话术、分析建议。");
        return;
      }
      window.dispatchEvent(new CustomEvent("bsr:open-video-analysis", { detail: importedVideo }));
    } catch (error) {
      const errorMessage = new AIMessage({
        content: `❌ **视频转写失败**\n\n${error?.message || String(error)}`,
      });
      errorMessage.additional_kwargs = {
        timestamp: new Date().toISOString(),
        isError: true,
      };
      messages = [...messages, errorMessage];
    }
  }

  function handleMasterPublished(event: CustomEvent<{ scriptKey: string; masterScriptId: number }>): void {
    localStorage.setItem("bsr:active-master", JSON.stringify(event.detail));
    masterSourceVideo = null;
  }

  async function changeAgentMode(event: Event) {
    const nextMode = (event.currentTarget as HTMLSelectElement).value as AgentMode;
    if (nextMode === agentMode) return;
    agentMode = nextMode;
    localStorage.setItem('ai_agent_mode', agentMode);
    await clearConversation();
  }

  function handleCompareSample(event: CustomEvent<ReviewSample>) {
    const sample = event.detail;
    showSampleDialog = false;
    inputMessage = `请读取样本库中的 ${sample.sample_no}（${sample.product || sample.category}）作为${sample.is_b_baseline === 1 ? 'B基线' : '历史样本'}，与我接下来指定或最新导入的片段进行 A/B 对照。先核验两边事实边界，再输出链路签名对照、逐句差异、关键模块迁移、最小修改补丁和融合口播。`;
  }

  function handleKeyPress(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  onMount(async () => {
    const savedMode = localStorage.getItem('ai_agent_mode');
    if (savedMode === 'general' || savedMode === 'commerce-review') {
      agentMode = savedMode;
    }
    try {
      const config = await invoke<any>('get_config');
      adminMode = Boolean(config?.admin_mode);
    } catch (error) {
      console.warn('Failed to read admin mode:', error);
    }
    loadSettings();
    const messageMode = localStorage.getItem('ai_messages_mode');
    const previousMessages = messageMode === agentMode
      ? JSON.parse(localStorage.getItem('messages') || '[]')
      : [];
    messages = previousMessages.map((message: any) => {
      if (message.id.includes('HumanMessage')) {
        const msg = new HumanMessage(message.kwargs);
        if (message.additional_kwargs?.timestamp) {
          msg.additional_kwargs = { ...msg.additional_kwargs, timestamp: message.additional_kwargs.timestamp };
        }
        return msg;
      } else if (message.id.includes('AIMessage')) {
        const msg = new AIMessage(message.kwargs);
        if (message.additional_kwargs?.timestamp) {
          msg.additional_kwargs = { ...msg.additional_kwargs, timestamp: message.additional_kwargs.timestamp };
        }
        return msg;
      } else if (message.id.includes('ToolMessage')) {
        const msg = new ToolMessage(message.kwargs);
        if (message.additional_kwargs?.timestamp) {
          msg.additional_kwargs = { ...msg.additional_kwargs, timestamp: message.additional_kwargs.timestamp };
        }
        return msg;
      }
    });
    toolCallStates.clear();
    const toolCallStatesString = localStorage.getItem('toolCallStates');
    if (toolCallStatesString) {
      const toolCallStatesObj = JSON.parse(toolCallStatesString);
      for (const [key, value] of Object.entries(toolCallStatesObj)) {
        toolCallStates.set(key, value as 'confirmed' | 'rejected' | 'none');
      }
    }
    scrollToBottom();
  });
</script>

<div class="flex h-full bg-gradient-to-br from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900">
  <!-- Main Content -->
  <div class="flex-1 flex flex-col relative">
    <!-- Messages Area -->
    <div class="flex-1 overflow-y-auto" bind:this={messageContainer}>
      <div class="max-w-4xl mx-auto px-6 py-8" style="padding-bottom: {inputAreaHeight + 16}px;">
        {#if !agent && settings.provider !== 'minimax'}
          <!-- Welcome State -->
          <div class="flex items-center justify-center min-h-[500px]">
            <div class="max-w-md text-center space-y-6">
              <div class="inline-flex items-center justify-center w-20 h-20 rounded-3xl bg-gradient-to-br from-amber-400 to-orange-500 shadow-xl">
                <span class="text-4xl">🍊</span>
              </div>
              <div class="space-y-3">
                <h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100">欢迎使用 AI 助手</h2>
                <p class="text-gray-600 dark:text-gray-400 leading-relaxed">
                  配置您的 AI 模型以开始智能对话。支持 OpenAI 兼容 API 和本地 Ollama 模型。
                </p>
              </div>
              {#if adminMode}
              <button
                on:click={openSettings}
                class="inline-flex items-center space-x-2 px-6 py-3 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-xl hover:bg-gray-800 dark:hover:bg-gray-200 transition-colors font-medium shadow-lg"
              >
                <Settings class="w-4 h-4" />
                <span>开始配置</span>
              </button>
              {:else}
                <p class="text-sm text-amber-600 dark:text-amber-400">AI 服务尚未就绪，请联系管理员。</p>
              {/if}
              <div class="flex items-center justify-center space-x-6 text-sm text-gray-500 pt-4">
                <div class="flex items-center space-x-1.5">
                  <Zap class="w-4 h-4" />
                  <span>快速响应</span>
                </div>
                <div class="flex items-center space-x-1.5">
                  <Sparkles class="w-4 h-4" />
                  <span>智能分析</span>
                </div>
              </div>
            </div>
          </div>
        {:else if messages.length === 0}
          <!-- Empty State with Prompts -->
          <div class="space-y-8">
            <div class="text-center space-y-3">
              <div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-gradient-to-br from-amber-400 to-orange-500">
                <span class="text-2xl">🍊</span>
              </div>
              <div>
                <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">你好！我是{assistantTitle}</h2>
                <p class="text-sm text-gray-600 dark:text-gray-400 mt-2">
                  {assistantDescription} 点击下方卡片快速开始。
                </p>
              </div>
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {#each presetPrompts as prompt}
                <button
                  on:click={() => handlePresetPrompt(prompt.prompt)}
                  class="group p-4 text-left bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:border-gray-300 dark:hover:border-gray-600 hover:shadow-md transition-all"
                >
                  <div class="flex items-start space-x-3">
                    <span class="text-2xl flex-shrink-0">{prompt.icon}</span>
                    <div class="flex-1 min-w-0">
                      <div class="text-sm font-medium text-gray-900 dark:text-gray-100 mb-1">{prompt.title}</div>
                      <p class="text-xs text-gray-500 dark:text-gray-500 leading-relaxed">{prompt.description}</p>
                    </div>
                  </div>
                </button>
              {/each}
            </div>
          </div>
        {:else}
          <!-- Messages -->
          <div class="space-y-6">
            {#each messages as message, index (index)}
              {#if message instanceof HumanMessage}
                <HumanMessageComponent {message} {formatTime} />
              {:else if message instanceof AIMessage}
                <AIMessageComponent
                  {message}
                  {formatTime}
                  onToolCallConfirm={handleToolCallConfirm}
                  onToolCallReject={handleToolCallReject}
                  toolCallState={getToolCallState(message)}
                  isSensitiveToolCall={isSensitiveToolCall(message)}
                />
              {:else if message instanceof ToolMessage}
                <ToolMessageComponent {message} {formatTime} />
              {/if}
            {/each}
          </div>
        {/if}

        {#if transcriptReviewItems.length > 0 && transcriptReviewArchive}
          <div class="mt-6 rounded-xl border border-amber-200 bg-amber-50 p-4 dark:border-amber-900/60 dark:bg-amber-950/20">
            <div class="mb-3 flex items-center justify-between">
              <div>
                <h3 class="text-sm font-semibold text-amber-900 dark:text-amber-200">关键事实待回听</h3>
                <p class="mt-0.5 text-xs text-amber-700 dark:text-amber-400">点击“回听”只播放该事实前后约5秒，不用重新观看整场。</p>
              </div>
              <span class="rounded-full bg-amber-100 px-2 py-1 text-xs text-amber-800 dark:bg-amber-900/50 dark:text-amber-200">{transcriptReviewItems.length} 项</span>
            </div>
            <div class="max-h-64 space-y-2 overflow-y-auto pr-1">
              {#each transcriptReviewItems as item, index}
                <div class="rounded-lg border border-amber-100 bg-white px-3 py-2 text-xs dark:border-amber-900/40 dark:bg-gray-900">
                  <div class="flex items-center gap-2">
                    <span class="w-24 shrink-0 text-gray-500">{formatMiningTime((item.start_ms || 0) / 1000)}–{formatMiningTime((item.end_ms || 0) / 1000)}</span>
                    <span class="w-20 shrink-0 font-medium text-gray-700 dark:text-gray-200">{item.type}</span>
                    <input class="min-w-0 flex-1 rounded-md border border-gray-200 bg-white px-2 py-1.5 text-gray-900 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100" bind:value={item.correction} title={item.context} />
                    <button class="shrink-0 rounded-md bg-amber-600 px-2.5 py-1.5 font-medium text-white hover:bg-amber-700" on:click={() => replayTranscriptFact(item)}>▶ 回听</button>
                    <button class="shrink-0 rounded-md bg-emerald-600 px-2.5 py-1.5 font-medium text-white hover:bg-emerald-700 disabled:opacity-50" disabled={item.saving} on:click={() => saveTranscriptFact(index, item)}>{item.status === "已人工确认" ? "✓ 已确认" : item.saving ? "保存中" : "确认修正"}</button>
                  </div>
                  {#if item.error}<p class="mt-1 text-red-600">{item.error}</p>{/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}

        {#if isProcessing}
          <div class="mt-6">
            {#if importWorkflowStatus}
              <div class="flex items-center gap-3 rounded-xl border border-blue-100 bg-blue-50 px-4 py-3 text-sm text-blue-700 dark:border-blue-900/50 dark:bg-blue-950/30 dark:text-blue-300">
                <span class="h-4 w-4 animate-spin rounded-full border-2 border-blue-500 border-t-transparent"></span>
                <span>{importWorkflowStatus}</span>
              </div>
            {:else}
              <ProcessingMessageComponent />
            {/if}
          </div>
        {/if}
      </div>
    </div>

    <!-- Floating Input Area -->
    <div bind:offsetHeight={inputAreaHeight} class="absolute bottom-0 left-0 right-0 px-6 pb-4 pt-8 bg-gradient-to-t from-gray-50 via-gray-50 to-transparent dark:from-gray-950 dark:via-gray-950">
      <div class="max-w-4xl mx-auto">
        <div class="bg-white dark:bg-gray-900 rounded-2xl border border-gray-200 dark:border-gray-800 shadow-lg overflow-hidden">
          <!-- Textarea -->
          <div class="relative">
            <textarea
              bind:value={inputMessage}
              on:keypress={handleKeyPress}
              placeholder={!aiReady ? "请先配置 AI 模型..." : "输入您的消息..."}
              class="w-full px-4 pt-3 pb-3 border-0 bg-transparent text-gray-900 dark:text-gray-100 placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:ring-0 resize-none min-h-[52px] max-h-[200px] text-[15px] leading-relaxed disabled:opacity-50 disabled:cursor-not-allowed"
              rows="1"
              disabled={isProcessing || !aiReady}
            ></textarea>
          </div>

          <!-- Bottom bar: model info + actions -->
          <div class="flex items-center justify-between px-4 py-2 border-t border-gray-100 dark:border-gray-800/50">
            <div class="flex items-center space-x-3">
              <label class="flex items-center space-x-1.5 px-2 py-1 text-xs text-gray-500 dark:text-gray-400">
                <Sparkles class="w-3.5 h-3.5" />
                <select
                  value={agentMode}
                  on:change={changeAgentMode}
                  disabled={isProcessing}
                  class="bg-transparent border-0 p-0 pr-5 text-xs text-gray-600 dark:text-gray-300 focus:ring-0 disabled:opacity-50"
                  title="切换智能体模式会清空当前对话"
                >
                  <option value="commerce-review">成交片段复盘官</option>
                  <option value="general">通用剪辑助手</option>
                </select>
              </label>

              <!-- Model info -->
              {#if adminMode}
              <button
                on:click={openSettings}
                class="flex items-center space-x-1.5 px-2 py-1 text-xs text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
                title="点击配置模型"
              >
                <Bot class="w-3.5 h-3.5" />
                {#if aiReady}
                  <span>{settings.provider === 'minimax' ? 'MiniMax' : settings.provider === 'ollama' ? 'Ollama' : 'OpenAI'} · {settings.model || '未设置模型'}</span>
                {:else}
                  <span>未配置模型</span>
                {/if}
              </button>
              {:else}
                <div class="flex items-center space-x-1.5 px-2 py-1 text-xs text-gray-500 dark:text-gray-400">
                  <Bot class="w-3.5 h-3.5" />
                  <span>MiniMax · 自动校对与复盘</span>
                </div>
              {/if}

              <button
                class="flex items-center space-x-1 px-2 py-1 text-xs text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                on:click={clearConversation}
                disabled={!aiReady || isProcessing}
                title="清空对话"
              >
                <Trash2 class="w-3.5 h-3.5" />
                <span>清空</span>
              </button>
            </div>

            <div class="flex items-center space-x-2">
              {#if agentMode === "commerce-review"}
                <button
                  class="px-2.5 py-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors flex items-center space-x-1.5 text-sm"
                  on:click={() => showSupportCandidateQueue = true}
                  disabled={isProcessing}
                  title="审核达到 85 分的候选辅稿"
                >
                  <Clipboard class="w-3.5 h-3.5" />
                  <span>候选辅稿</span>
                </button>
                <button
                  class="px-2.5 py-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors flex items-center space-x-1.5 text-sm"
                  on:click={() => showCompetitorReferenceQueue = true}
                  disabled={isProcessing}
                  title="竞品对照：人工批准后写入案例库"
                >
                  <Clipboard class="w-3.5 h-3.5" />
                  <span>竞品参考</span>
                </button>
                <button
                  class="px-2.5 py-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors flex items-center space-x-1.5 text-sm"
                  on:click={() => showSampleDialog = true}
                  disabled={isProcessing}
                  title="打开M1样本库"
                >
                  <Database class="w-3.5 h-3.5" />
                  <span>样本库</span>
                </button>
                <button
                  class="px-2.5 py-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors flex items-center space-x-1.5 text-sm"
                  on:click={() => showImportDialog = true}
                  disabled={isProcessing}
                  title="导入视频并打开三栏分析"
                >
                  <Upload class="w-3.5 h-3.5" />
                  <span>导入并分析</span>
                </button>
              {/if}
              {#if inputMessage.trim()}
                <span class="text-xs text-gray-400 dark:text-gray-600">{inputMessage.length}</span>
              {/if}
              <button
                class="px-3 py-1.5 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-lg hover:bg-gray-800 dark:hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center space-x-1.5 text-sm font-medium"
                disabled={!inputMessage.trim() || isProcessing || !aiReady}
                on:click={sendMessage}
              >
                <Send class="w-3.5 h-3.5" />
                <span>发送</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>

  <!-- Settings Modal -->
  {#if adminMode}
  <SettingsModal
    {showSettings}
    bind:settings
    {availableModels}
    {isLoadingModels}
    onClose={closeSettings}
    onSave={saveSettings}
    onLoadModels={loadModels}
  />
  {/if}

  <ImportVideoDialog
    bind:showDialog={showImportDialog}
    roomId={null}
    on:imported={handleVideoImported}
  />

  {#if masterSourceVideo}
    <MasterSourceDialog
      videoId={masterSourceVideo.id}
      videoTitle={masterSourceVideo.title || masterSourceVideo.file || "整场直播母稿"}
      on:close={() => masterSourceVideo = null}
      on:published={handleMasterPublished}
    />
  {/if}

  {#if showSupportCandidateQueue}
    <SupportCandidateQueue on:close={() => showSupportCandidateQueue = false} />
  {/if}
  {#if showCompetitorReferenceQueue}
    <CompetitorReferenceQueue on:close={() => showCompetitorReferenceQueue = false} />
  {/if}

  <ReviewSampleDialog
    bind:showDialog={showSampleDialog}
    reviewContent={lastReviewContent}
    on:compare={handleCompareSample}
  />
</div>

<style>
  :global(.overflow-y-auto) {
    scrollbar-width: thin;
    scrollbar-color: rgb(209 213 219) transparent;
  }
  :global(.dark .overflow-y-auto) {
    scrollbar-color: rgb(55 65 81) transparent;
  }
  :global(.overflow-y-auto::-webkit-scrollbar) {
    width: 6px;
  }
  :global(.overflow-y-auto::-webkit-scrollbar-track) {
    background: transparent;
  }
  :global(.overflow-y-auto::-webkit-scrollbar-thumb) {
    background: rgb(209 213 219);
    border-radius: 3px;
  }
  :global(.dark .overflow-y-auto::-webkit-scrollbar-thumb) {
    background: rgb(55 65 81);
  }
</style>
