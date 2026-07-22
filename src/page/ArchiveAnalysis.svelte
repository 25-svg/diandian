<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import {
    ArrowLeft,
    BarChart3,
    BookOpenCheck,
    Check,
    Clipboard,
    Copy,
    FileSearch,
    ListChecks,
    Loader2,
    MessageSquareText,
    Play,
    RefreshCw,
    RotateCcw,
  } from "lucide-svelte";
  import TranscriptReviewPanel from "../lib/components/analysis/TranscriptReviewPanel.svelte";
  import MasterComparisonPanel from "../lib/components/analysis/MasterComparisonPanel.svelte";
  import { get_static_url, invoke } from "../lib/invoker";
  import type { RecordItem } from "../lib/db";
  import type { VideoItem } from "../lib/interface";
  import { COMMERCE_REVIEW_PROMPT } from "../lib/agent/prompts";
  import {
    analysisSourceKey,
    friendlyArchiveTranscriptError,
    normalizeBeginnerReview,
    normalizeCandidate,
    normalizeCandidates,
    selectArchiveTranscriptAction,
    transcriptRefreshFailureState,
    isCurrentMasterComparison,
    autoMatchMasterSection,
    type AnalysisSourceIdentity,
    type BeginnerReview,
    type CandidateInput,
    type CandidateType,
    type HighlightCandidate,
    type HighlightTier,
  } from "../lib/archiveAnalysis";
  import {
    analysisRequestIdentity,
    analysisWorkflowStage,
    candidateReviewIdentity,
    correctionSelectionTarget,
    firstPendingCorrectionId,
    getTranscriptAudit,
    highlightDiscoveryAction,
    highlightReviewGate,
    isCurrentCandidateReview,
    isCurrentCandidateGeneration,
    isCurrentAnalysisRequest,
    transcriptReviewLock,
    type AnalysisRequestIdentity,
    type AnalysisWorkflowStage,
    type HighlightDiscoveryAction,
    type TranscriptAuditBundle,
    type TranscriptAuditState,
    type TranscriptCorrection,
    type TranscriptReviewUpdateEvent,
  } from "../lib/transcriptReview";
  import {
    compareHighlightToMaster,
    getMasterBaseline,
    type MasterBaseline,
    type MasterComparisonResult,
  } from "../lib/masterScript";

  export let archive: RecordItem | null = null;
  export let video: VideoItem | null = null;
  export let refreshToken = 0;

  type TranscriptRefreshResult = {
    subtitle: string;
    decision: "kept" | "replaced" | "created";
    similarity: number;
    oldLength: number;
    newLength: number;
  };

  type TranscriptEntry = {
    id: number;
    start: number;
    end: number;
    text: string;
    raw: string;
  };

  type Candidate = HighlightCandidate;

  type ReviewResult = {
    beginner: BeginnerReview;
    review: string;
    spokenScript: string;
    trainingChecklist: string;
    raw: string;
    updatedAt: string;
  };

  type SavedAnalysis = {
    version: 2 | 3;
    transcript: string;
    candidates: Candidate[];
    reviews: Record<string, ReviewResult>;
    selectedCandidateId: string;
    discoveryCompleted: boolean;
    updatedAt: string;
  };

  const dispatch = createEventDispatcher();
  const allowedTypes = new Set<CandidateType>([
    "成交片段",
    "疑似成交片段",
    "问价未见成交信号",
    "转品/上链接片段",
    "讲得散片段",
    "无法判断",
  ]);

  let loadedSourceKey = "";
  let loadedRefreshToken = -1;
  let transcript = "";
  let transcriptEntries: TranscriptEntry[] = [];
  let candidates: Candidate[] = [];
  let reviews: Record<string, ReviewResult> = {};
  let selectedCandidateId = "";
  let selectedCandidate: Candidate | null = null;
  let selectedReview: ReviewResult | null = null;
  let discoveryCompleted = false;
  let stage = "等待选择录播";
  let transcriptRefreshNotice = "";
  let errorMessage = "";
  let isTranscribing = false;
  let isDiscovering = false;
  let reviewingId = "";
  let copiedAction = "";
  let playerUrl = "";
  let playerNonce = 0;
  let videoPlayerUrl = "";
  let videoElement: HTMLVideoElement | null = null;
  let highlightFilter: "全部" | HighlightTier = "全部";
  let reviewTab: "analysis" | "script" | "training" = "analysis";
  let workspaceTab: "proofreading" | "analysis" = "analysis";
  let auditBundle: TranscriptAuditBundle | null = null;
  let auditLoadStatus: TranscriptAuditState = "idle";
  let auditError = "";
  let selectedCorrectionId = "";
  let selectedTranscriptCorrection: TranscriptCorrection | null = null;
  let sourceRevision = 0;
  let transcriptRevision = 0;
  let initializeRequestSequence = 0;
  let transcriptRequestSequence = 0;
  let auditRequestSequence = 0;
  let discoveryRequestSequence = 0;
  let reviewRequestSequence = 0;
  let reviewRequestToken = 0;
  let candidateGeneration = 0;
  let criticalReviewSourceKey = "";
  let masterBaseline: MasterBaseline | null = null;
  let masterComparison: MasterComparisonResult | null = null;
  let selectedMasterSectionId = 0;
  let masterComparisonLoading = false;
  let masterComparisonError = "";
  let masterMatchStatus: "idle" | "matched" | "ambiguous" | "unmatched" = "idle";
  let masterComparisonSequence = 0;

  $: selectedCandidate = candidates.find((item) => item.id === selectedCandidateId) || null;
  $: selectedReview = selectedCandidate ? reviews[selectedCandidate.id] || null : null;
  $: coreHighlightCount = candidates.filter((item) => item.tier === "核心高光").length;
  $: candidateHighlightCount = candidates.length - coreHighlightCount;
  $: filteredCandidates = highlightFilter === "全部"
    ? candidates
    : candidates.filter((item) => item.tier === highlightFilter);
  $: currentSource = sourceIdentity(archive, video);
  $: currentSourceKey = currentSource ? analysisSourceKey(currentSource) : "";
  $: discoveryAction = currentDiscoveryAction();
  $: candidateReviewGateState = highlightReviewGate(currentWorkflowStage(), discoveryCompleted, isDiscovering);
  $: transcriptReviewLockState = transcriptReviewLock(isTranscribing);
  $: selectedTranscriptCorrection = auditBundle?.corrections.find((item) => item.id === selectedCorrectionId) || null;
  $: if (currentSourceKey && (currentSourceKey !== loadedSourceKey || refreshToken !== loadedRefreshToken)) {
    void initialize();
  } else if (!currentSourceKey && loadedSourceKey) {
    loadedSourceKey = "";
    loadedRefreshToken = refreshToken;
    resetState();
    stage = "等待选择录播";
  }

  function sourceIdentity(
    selectedArchive: RecordItem | null,
    selectedVideo: VideoItem | null,
  ): AnalysisSourceIdentity | null {
    if (selectedArchive) {
      return {
        kind: "archive",
        platform: selectedArchive.platform,
        roomId: String(selectedArchive.room_id),
        liveId: String(selectedArchive.live_id),
      };
    }
    if (selectedVideo) return { kind: "video", videoId: selectedVideo.id };
    return null;
  }

  function currentWorkflowStage(): AnalysisWorkflowStage {
    return analysisWorkflowStage({
      hasTranscript: Boolean(transcript),
      isRecognizing: isTranscribing,
      auditState: auditLoadStatus,
      pendingCriticalCount: auditBundle?.pendingCriticalCount ?? 0,
    });
  }

  function activeAnalysisRequestIdentity(): AnalysisRequestIdentity {
    return analysisRequestIdentity(currentSourceKey, sourceRevision, transcriptRevision);
  }

  function isCurrentInitializationRequest(
    request: AnalysisRequestIdentity,
    requestId: number,
  ): boolean {
    const active = activeAnalysisRequestIdentity();
    return requestId === initializeRequestSequence
      && request.sourceKey === active.sourceKey
      && request.sourceRevision === active.sourceRevision;
  }

  function invalidateTranscriptBoundRequests(): void {
    transcriptRevision += 1;
    discoveryRequestSequence += 1;
    reviewRequestSequence += 1;
    reviewRequestToken += 1;
    candidateGeneration += 1;
    isDiscovering = false;
    reviewingId = "";
  }

  function currentDiscoveryAction(requestedSourceKey = currentSourceKey): HighlightDiscoveryAction {
    return highlightDiscoveryAction({
      stage: currentWorkflowStage(),
      sourceKey: currentSourceKey,
      requestedSourceKey,
      hadPendingCriticalReview: criticalReviewSourceKey === currentSourceKey,
      discoveryCompleted,
      isDiscovering,
    });
  }

  function storageKey(sourceKey = currentSourceKey): string {
    return `bsr:content-analysis:v3:${sourceKey}`;
  }

  function sourceTitle(
    selectedArchive: RecordItem | null,
    selectedVideo: VideoItem | null,
  ): string {
    return selectedArchive?.title || selectedVideo?.title || selectedVideo?.file || "请选择录播或视频";
  }

  function formatTime(totalSeconds: number): string {
    const safe = Math.max(0, Math.floor(totalSeconds || 0));
    const hours = Math.floor(safe / 3600);
    const minutes = Math.floor((safe % 3600) / 60);
    const seconds = safe % 60;
    return hours > 0
      ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
      : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  function tierDisplay(tier: HighlightTier): string {
    return tier === "核心高光" ? "重点保留" : "建议复核";
  }

  function srtTimeToSeconds(value: string): number {
    const parts = value.replace(",", ".").split(":").map(Number);
    if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part))) return 0;
    return parts[0] * 3600 + parts[1] * 60 + parts[2];
  }

  function parseSrt(value: string): TranscriptEntry[] {
    return value
      .trim()
      .split(/\r?\n\r?\n+/)
      .map((block, index) => {
        const lines = block.split(/\r?\n/);
        const timingIndex = lines.findIndex((line) => line.includes("-->"));
        if (timingIndex < 0) return null;
        const [from, to] = lines[timingIndex].split("-->").map((item) => item.trim());
        const text = lines.slice(timingIndex + 1).join(" ").trim();
        if (!text) return null;
        return {
          id: index,
          start: srtTimeToSeconds(from),
          end: srtTimeToSeconds(to),
          text,
          raw: block,
        };
      })
      .filter(Boolean) as TranscriptEntry[];
  }

  function splitTranscriptIntoChunks(entries: TranscriptEntry[], chunkSeconds = 480, overlapSeconds = 30): string[] {
    if (!entries.length) return transcript.trim() ? [transcript] : [];
    const duration = Math.max(...entries.map((entry) => entry.end));
    const chunks: string[] = [];
    for (let start = 0; start < duration; start += chunkSeconds) {
      const end = start + chunkSeconds;
      const blocks = entries
        .filter((entry) => entry.end >= Math.max(0, start - overlapSeconds) && entry.start <= end + overlapSeconds)
        .map((entry) => entry.raw);
      if (blocks.length) chunks.push(blocks.join("\n\n"));
    }
    return chunks;
  }

  function extractJsonArray(content: string): unknown[] {
    const start = content.indexOf("[");
    const end = content.lastIndexOf("]");
    if (start < 0 || end <= start) return [];
    try {
      const value = JSON.parse(content.slice(start, end + 1));
      return Array.isArray(value) ? value : [];
    } catch {
      return [];
    }
  }

  function parseCandidates(content: string): Candidate[] {
    const typeAliases: Record<string, CandidateType> = {
      "成交": "成交片段",
      "疑似成交": "疑似成交片段",
      "问价未成交": "问价未见成交信号",
      "问价未见成交": "问价未见成交信号",
      "转品片段": "转品/上链接片段",
      "上链接片段": "转品/上链接片段",
      "转品/上链接": "转品/上链接片段",
      "讲得散": "讲得散片段",
    };
    return extractJsonArray(content)
      .map((item: any) => ({ ...item, type: typeAliases[item?.type] || item?.type }))
      .filter((item: any) => allowedTypes.has(item?.type))
      .map((item: any, index) => normalizeCandidate(item as CandidateInput, index))
      .filter((item): item is Candidate => Boolean(item));
  }

  function buildEvidenceFallback(entries: TranscriptEntry[]): Candidate[] {
    const transcriptEnd = entries.length ? entries[entries.length - 1].end : 0;
    const groups: Array<{ kind: "成交" | "链接" | "问价"; entries: TranscriptEntry[] }> = [];
    const matchKind = (text: string): "成交" | "链接" | "问价" | null => {
      if (/(恭喜.{0,8}(下单|拍下|成交)|已下单|拍下了|成交了|锁货|备注上|给.{0,8}备注)/.test(text)) return "成交";
      if (/(小黄车|置顶链接|弹.{0,6}链接|上.{0,4}链接|链接放|改价|上架)/.test(text)) return "链接";
      if (/(多少钱|什么价|价格多少|预算多少|问个价|报价)/.test(text)) return "问价";
      return null;
    };
    for (const entry of entries) {
      const kind = matchKind(entry.text);
      if (!kind) continue;
      const previous = groups[groups.length - 1];
      if (previous && previous.kind === kind && entry.start - previous.entries[previous.entries.length - 1].end <= 45) {
        previous.entries.push(entry);
      } else {
        groups.push({ kind, entries: [entry] });
      }
    }
    const fallbackItems = groups.slice(0, 8).map((group) => {
      const first = group.entries[0];
      const last = group.entries[group.entries.length - 1];
      const evidence = group.entries.map((entry) => entry.text).join("；");
      const type: CandidateType = group.kind === "成交"
        ? "成交片段"
        : group.kind === "链接"
          ? "转品/上链接片段"
          : "问价未见成交信号";
      return {
        start: Math.max(0, first.start - 45),
        end: Math.min(transcriptEnd, last.end + 25),
        type,
        confidence: group.kind === "成交" ? "中" : "低",
        tier: "候选高光" as const,
        score: group.kind === "成交" ? 64 : 48,
        product: "主商品待确认",
        evidence,
        reason: `大模型未返回可用候选，系统依据逐字稿中的明确${group.kind}动作保留该区间供人工复核`,
        verify: group.kind === "成交" ? "需核验订单时间、SKU及成交对象" : "需核验画面商品、链接编号及后续成交信号",
        signals: [group.kind === "成交" ? "成交措辞" : group.kind === "链接" ? "链接动作" : "问价回应"],
        hook: "",
        takeaway: "规则兜底发现，需人工复核后再进入母稿",
      };
    });
    return normalizeCandidates(fallbackItems);
  }

  function selectCandidates(items: Candidate[]): Candidate[] {
    const score: Record<string, number> = { 高: 3, 中: 2, 低: 1 };
    const deduped: Candidate[] = [];
    for (const item of [...items].sort((a, b) => a.start - b.start || score[b.confidence] - score[a.confidence])) {
      const duplicate = deduped.some((saved) =>
        saved.type === item.type && Math.min(saved.end, item.end) - Math.max(saved.start, item.start) > 10
      );
      if (!duplicate) deduped.push(item);
    }
    const counts = new Map<CandidateType, number>();
    const selected = deduped
      .sort((a, b) => score[b.confidence] - score[a.confidence] || a.start - b.start)
      .filter((item) => {
        const count = counts.get(item.type) || 0;
        if (count >= 3 || item.type === "无法判断") return false;
        counts.set(item.type, count + 1);
        return true;
      })
      .sort((a, b) => a.start - b.start);
    return normalizeCandidates(selected);
  }

  function resetState(): void {
    sourceRevision += 1;
    transcriptRevision += 1;
    initializeRequestSequence += 1;
    transcriptRequestSequence += 1;
    auditRequestSequence += 1;
    discoveryRequestSequence += 1;
    reviewRequestSequence += 1;
    reviewRequestToken += 1;
    candidateGeneration += 1;
    transcript = "";
    transcriptEntries = [];
    candidates = [];
    reviews = {};
    selectedCandidateId = "";
    selectedCandidate = null;
    selectedReview = null;
    discoveryCompleted = false;
    errorMessage = "";
    transcriptRefreshNotice = "";
    isTranscribing = false;
    isDiscovering = false;
    reviewingId = "";
    copiedAction = "";
    playerUrl = "";
    videoPlayerUrl = "";
    highlightFilter = "全部";
    reviewTab = "analysis";
    workspaceTab = "analysis";
    auditBundle = null;
    auditLoadStatus = "idle";
    auditError = "";
    selectedCorrectionId = "";
    criticalReviewSourceKey = "";
    masterComparison = null;
    selectedMasterSectionId = 0;
    masterComparisonLoading = false;
    masterComparisonError = "";
    masterMatchStatus = "idle";
    masterComparisonSequence += 1;
  }

  function loadSaved(requestIdentity = activeAnalysisRequestIdentity()): boolean {
    if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return false;
    const legacyKey = archive
      ? `bsr:archive-analysis:v2:${archive.platform}:${archive.room_id}:${archive.live_id}`
      : "";
    const requestStorageKey = storageKey(requestIdentity.sourceKey);
    const raw = localStorage.getItem(requestStorageKey) || (legacyKey ? localStorage.getItem(legacyKey) : null);
    if (!raw) return false;
    try {
      const saved = JSON.parse(raw) as SavedAnalysis;
      if (![2, 3].includes(saved.version) || !saved.transcript) return false;
      invalidateTranscriptBoundRequests();
      transcript = saved.transcript;
      transcriptEntries = parseSrt(transcript);
      const transcriptEnd = transcriptEntries.length ? transcriptEntries[transcriptEntries.length - 1].end : 0;
      candidates = Array.isArray(saved.candidates)
        ? normalizeCandidates(saved.candidates.map((candidate) => ({
            ...candidate,
            end: transcriptEnd ? Math.min(candidate.end, transcriptEnd) : candidate.end,
          })))
        : [];
      reviews = Object.fromEntries(
        Object.entries(saved.reviews || {}).map(([candidateId, review]) => {
          const savedReview = review as ReviewResult;
          const needsRepair = Boolean(savedReview?.raw) && (
            !savedReview.beginner ||
            /^\s*\{\s*"review"\s*:/.test(savedReview.review || "") ||
            (savedReview.spokenScript || "").startsWith("模型本次未返回独立口播字段")
          );
          return [candidateId, needsRepair ? parseReview(savedReview.raw) : savedReview];
        })
      );
      selectedCandidateId = saved.selectedCandidateId || candidates[0]?.id || "";
      selectedCandidate = candidates.find((candidate) => candidate.id === selectedCandidateId) || null;
      selectedReview = selectedCandidate ? reviews[selectedCandidate.id] || null : null;
      discoveryCompleted = Boolean(saved.discoveryCompleted);
      stage = `已恢复 ${new Date(saved.updatedAt).toLocaleString("zh-CN")} 保存的分析`;
      if (selectedCandidateId) {
        const restoredCandidate = candidates.find((candidate) => candidate.id === selectedCandidateId) || null;
        updatePlayerAndTranscript(false, restoredCandidate);
      }
      return true;
    } catch {
      localStorage.removeItem(requestStorageKey);
      return false;
    }
  }

  function saveState(requestIdentity = activeAnalysisRequestIdentity()): void {
    if (
      !currentSource
      || !transcript
      || !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)
    ) return;
    const value: SavedAnalysis = {
      version: 3,
      transcript,
      candidates,
      reviews,
      selectedCandidateId,
      discoveryCompleted,
      updatedAt: new Date().toISOString(),
    };
    try {
      localStorage.setItem(storageKey(requestIdentity.sourceKey), JSON.stringify(value));
    } catch (error) {
      errorMessage = `分析已完成，但本地保存失败：${error}`;
    }
  }

  async function initialize(): Promise<void> {
    loadedSourceKey = currentSourceKey;
    loadedRefreshToken = refreshToken;
    resetState();
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestId = ++initializeRequestSequence;
    const selectedVideo = video;
    await loadActiveMaster();
    if (selectedVideo) {
      const nextVideoPlayerUrl = await get_static_url("output", selectedVideo.file);
      if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
      videoPlayerUrl = nextVideoPlayerUrl;
    }
    if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
    const restored = loadSaved(activeAnalysisRequestIdentity());
    if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
    await smartRefreshTranscript(restored);
    if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
    if (selectedCandidate && !masterComparison && !masterComparisonLoading) {
      await autoCompareCandidateToMaster(selectedCandidate);
    }
  }

  async function loadActiveMaster(): Promise<void> {
    masterBaseline = null;
    try {
      const raw = localStorage.getItem("bsr:active-master");
      const active = raw ? JSON.parse(raw) as { scriptKey?: string } : null;
      if (active?.scriptKey) masterBaseline = await getMasterBaseline(active.scriptKey);
    } catch {
      masterBaseline = null;
    }
  }

  async function compareSelectedToMaster(sectionId: number): Promise<void> {
    const candidate = selectedCandidate;
    const source = currentSource;
    const baseline = masterBaseline;
    const section = baseline?.sections.find((item) => item.id === sectionId);
    if (!candidate || !source || !baseline || !section) return;
    selectedMasterSectionId = sectionId;
    masterComparison = null;
    masterComparisonError = "";
    masterComparisonLoading = true;
    const requestId = ++masterComparisonSequence;
    const requestedCandidateId = candidate.id;
    const requestedGeneration = candidateGeneration;
    try {
      const result = await compareHighlightToMaster({
        scriptKey: baseline.master.scriptKey,
        expectedMasterScriptId: baseline.master.id,
        source,
        sourceStartMs: Math.round(candidate.start * 1000),
        sourceEndMs: Math.round(candidate.end * 1000),
        productCardId: section.productCardId,
        sectionKind: section.sectionKind,
      });
      if (requestId !== masterComparisonSequence || !isCurrentMasterComparison(
        requestedCandidateId,
        requestedGeneration,
        selectedCandidateId,
        candidateGeneration,
      )) return;
      masterComparison = result;
    } catch (reason: any) {
      if (requestId !== masterComparisonSequence) return;
      masterComparisonError = reason?.message || String(reason);
    } finally {
      if (requestId === masterComparisonSequence) masterComparisonLoading = false;
    }
  }

  async function autoCompareCandidateToMaster(candidate: Candidate): Promise<void> {
    const baseline = masterBaseline;
    if (!baseline) {
      masterMatchStatus = "idle";
      return;
    }
    const match = autoMatchMasterSection(candidate.product, baseline.sections);
    masterMatchStatus = match.status;
    if (match.sectionId === null) {
      selectedMasterSectionId = 0;
      masterComparison = null;
      masterComparisonLoading = false;
      masterComparisonError = match.status === "ambiguous"
        ? "识别到多个相似商品章节，本片段已暂停评分，请先在逐字稿中确认商品名称。"
        : "未能识别本片段对应的母稿商品，本片段不会进入候选辅稿。";
      return;
    }
    await compareSelectedToMaster(match.sectionId);
  }

  async function smartRefreshTranscript(hasSavedAnalysis: boolean, forceTranscriptRefresh = false): Promise<void> {
    if (!currentSource || isTranscribing) return;
    invalidateTranscriptBoundRequests();
    const requestId = ++transcriptRequestSequence;
    let requestIdentity = activeAnalysisRequestIdentity();
    const selectedArchive = archive;
    const selectedVideo = video;
    isTranscribing = true;
    errorMessage = "";
    try {
      const previousTranscript = transcript;
      let nextTranscript = "";
      let transcriptChanged = false;
      let nextTranscriptRefreshNotice = "";
      if (selectedArchive) {
        let existingSubtitle = "";
        if (!forceTranscriptRefresh) {
          stage = "正在读取录播逐字稿…";
          try {
            existingSubtitle = await invoke<string>("get_archive_subtitle", {
              platform: selectedArchive.platform,
              roomId: String(selectedArchive.room_id),
              liveId: String(selectedArchive.live_id),
            });
          } catch {
            existingSubtitle = "";
          }
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
        }

        if (selectArchiveTranscriptAction(forceTranscriptRefresh, existingSubtitle) === "use-existing") {
          nextTranscript = existingSubtitle;
          transcriptChanged = Boolean(previousTranscript && previousTranscript !== nextTranscript);
          nextTranscriptRefreshNotice = "已读取录播逐字稿";
        } else {
          stage = forceTranscriptRefresh ? "正在重新识别并核对新旧逐字稿…" : "正在生成录播逐字稿…";
          const result = await invoke<TranscriptRefreshResult>("refresh_archive_subtitle", {
            platform: selectedArchive.platform,
            roomId: String(selectedArchive.room_id),
            liveId: String(selectedArchive.live_id),
          });
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
          nextTranscript = result.subtitle;
          transcriptChanged = Boolean(previousTranscript && previousTranscript !== nextTranscript);
          nextTranscriptRefreshNotice = result.decision === "kept"
            ? `内容一致 ${Math.round(result.similarity * 100)}% · 保留旧稿`
            : result.decision === "replaced"
              ? `内容不一致 ${Math.round(result.similarity * 100)}% · 已采用新稿`
              : "首次生成 · 已采用新稿";
        }
      } else if (selectedVideo) {
        stage = forceTranscriptRefresh ? "正在重新识别视频逐字稿…" : "正在读取视频逐字稿…";
        if (!forceTranscriptRefresh) {
          nextTranscript = await invoke<string>("get_video_subtitle", { id: selectedVideo.id });
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
        }
        if (!nextTranscript.trim()) {
          nextTranscript = await invoke<string>("generate_video_subtitle", {
            eventId: `analysis_video_${selectedVideo.id}_${Date.now()}`,
            id: selectedVideo.id,
          });
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
          nextTranscriptRefreshNotice = forceTranscriptRefresh ? "已重新识别" : "首次生成逐字稿";
        } else {
          nextTranscriptRefreshNotice = "已读取视频字幕";
        }
        transcriptChanged = Boolean(previousTranscript && previousTranscript !== nextTranscript);
      }
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
      if (!nextTranscript?.trim()) throw new Error("没有识别到可用文字，请检查视频是否包含清晰人声。 ");

      invalidateTranscriptBoundRequests();
      transcript = nextTranscript;
      transcriptEntries = parseSrt(transcript);
      requestIdentity = activeAnalysisRequestIdentity();
      transcriptRefreshNotice = nextTranscriptRefreshNotice;
      if (!transcriptEntries.length) throw new Error("ASR 已返回内容，但没有可用的时间戳，无法定位视频。 ");
      if (transcriptChanged) {
        candidates = [];
        reviews = {};
        selectedCandidateId = "";
        discoveryCompleted = false;
        stage = `逐字稿已更新，共 ${transcriptEntries.length} 条`;
      } else {
        stage = hasSavedAnalysis
          ? `逐字稿内容一致，已保留原分析`
          : `逐字稿准备完成，共 ${transcriptEntries.length} 条`;
      }
      saveState(requestIdentity);
    } catch (error: any) {
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
      errorMessage = selectedArchive
        ? friendlyArchiveTranscriptError(error?.message || error)
        : error?.message || String(error);
      const failureState = transcriptRefreshFailureState(Boolean(transcript));
      transcriptRefreshNotice = failureState.notice;
      stage = failureState.stage;
    } finally {
      if (isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) {
        isTranscribing = false;
      }
    }

    if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
    if (!transcript) return;
    await loadTranscriptReview();
    if (currentDiscoveryAction() === "auto") {
      await discoverCandidates(currentSourceKey, "auto");
    }
    else if (selectedCandidateId && !reviews[selectedCandidateId]) {
      const restoredCandidate = candidates.find((candidate) => candidate.id === selectedCandidateId) || null;
      await reviewSelectedCandidate(false, restoredCandidate);
    }
  }

  async function loadTranscriptReview(): Promise<void> {
    const requestSource = currentSource;
    const requestSourceKey = currentSourceKey;
    if (!requestSource || !requestSourceKey) return;
    const requestId = ++auditRequestSequence;
    const requestIdentity = activeAnalysisRequestIdentity();
    auditLoadStatus = "loading";
    auditError = "";
    if (errorMessage.startsWith("校稿读取失败")) errorMessage = "";
    try {
      const bundle = await getTranscriptAudit(requestSource);
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, auditRequestSequence)) return;
      if (analysisSourceKey(bundle.source) !== requestSourceKey) {
        throw new Error("校稿来源已变化，请重新读取。");
      }
      auditBundle = bundle;
      auditLoadStatus = bundle.corrections.length ? "loaded" : "legacy-empty";
      if (bundle.corrections.some((correction) => correction.critical)) {
        criticalReviewSourceKey = requestSourceKey;
      }
      selectedCorrectionId = firstPendingCorrectionId(bundle.corrections) || bundle.corrections[0]?.id || "";
      if (bundle.pendingCriticalCount > 0) {
        workspaceTab = "proofreading";
        const correction = bundle.corrections.find((item) => item.id === selectedCorrectionId);
        if (correction) selectTranscriptCorrection(correction);
      }
    } catch (error: any) {
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, auditRequestSequence)) return;
      auditBundle = null;
      auditLoadStatus = "error";
      auditError = `${error?.message || String(error)}。请点击“重新读取”，校稿恢复前不会开始片段分析。`;
      errorMessage = `校稿读取失败：${auditError}`;
      stage = "校稿读取失败，已暂停片段分析";
      workspaceTab = "proofreading";
    }
  }

  function selectTranscriptCorrection(correction: TranscriptCorrection): void {
    const requestIdentity = activeAnalysisRequestIdentity();
    selectedCorrectionId = correction.id;
    workspaceTab = "proofreading";
    const target = correctionSelectionTarget(correction, transcriptEntries);
    if (archive) {
      playerNonce += 1;
      const params = new URLSearchParams({
        platform: archive.platform,
        room_id: String(archive.room_id),
        live_id: String(archive.live_id),
        start: String(Math.floor(target.seekSeconds)),
        end: String(Math.max(Math.ceil(correction.endMs / 1000), Math.floor(target.seekSeconds) + 20)),
        nonce: String(playerNonce),
        embed: "1",
      });
      playerUrl = `index_live.html?${params.toString()}`;
    } else if (videoElement) {
      videoElement.currentTime = target.seekSeconds;
      void videoElement.play().catch(() => undefined);
    }
    void tick().then(() => {
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
      if (target.transcriptEntryId !== null) {
        document.getElementById(`analysis-transcript-${target.transcriptEntryId}`)?.scrollIntoView({ block: "center" });
      }
    });
  }

  function updateTranscriptReview(event: CustomEvent<TranscriptReviewUpdateEvent>): void {
    if (isTranscribing) return;
    const { bundle, requestIdentity, reviewRequestToken: eventRequestToken } = event.detail;
    if (eventRequestToken !== reviewRequestToken) return;
    if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
    if (analysisSourceKey(bundle.source) !== currentSourceKey) return;
    if ((auditBundle?.pendingCriticalCount ?? 0) > 0 || bundle.pendingCriticalCount > 0) {
      criticalReviewSourceKey = currentSourceKey;
    }
    auditBundle = bundle;
    auditLoadStatus = bundle.corrections.length ? "loaded" : "legacy-empty";
    if (bundle.correctedSrt.trim()) {
      const transcriptChanged = transcript !== bundle.correctedSrt;
      invalidateTranscriptBoundRequests();
      transcript = bundle.correctedSrt;
      transcriptEntries = parseSrt(transcript);
      if (transcriptChanged) {
        candidates = [];
        reviews = {};
        selectedCandidateId = "";
        discoveryCompleted = false;
      }
      saveState(activeAnalysisRequestIdentity());
    }
  }

  async function regenerateAndAnalyze(): Promise<void> {
    if (isTranscribing || isDiscovering) return;
    await smartRefreshTranscript(Boolean(transcript && discoveryCompleted), true);
  }

  async function continueToHighlightAnalysis(): Promise<void> {
    const requestSourceKey = currentSourceKey;
    if (currentDiscoveryAction(requestSourceKey) !== "continue") return;
    workspaceTab = "analysis";
    await discoverCandidates(requestSourceKey, "continue");
  }

  async function discoverCandidates(
    requestSourceKey = currentSourceKey,
    requestedAction: Exclude<HighlightDiscoveryAction, "blocked"> = "rerun",
  ): Promise<void> {
    if (!currentSource || !transcript || currentDiscoveryAction(requestSourceKey) !== requestedAction) return;
    const requestId = ++discoveryRequestSequence;
    const requestIdentity = activeAnalysisRequestIdentity();
    candidateGeneration += 1;
    const requestCandidateGeneration = candidateGeneration;
    reviewRequestSequence += 1;
    reviewingId = "";
    isDiscovering = true;
    errorMessage = "";
    try {
      const chunks = splitTranscriptIntoChunks(transcriptEntries);
      const found: Candidate[] = [];
      for (let index = 0; index < chunks.length; index += 1) {
        if (
          !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
          || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
        ) return;
        stage = `片段发现器正在分析文稿 ${index + 1}/${chunks.length}…`;
        const response = await invoke<string>("minimax_chat", {
          systemPrompt: `你是电商直播高光发现器。只返回 JSON 数组，不要 Markdown。每项字段必须为 start、end、type、confidence、tier、score、product、evidence、reason、verify、signals、hook、takeaway。type 只能为：成交片段、疑似成交片段、问价未见成交信号、转品/上链接片段、讲得散片段、无法判断。tier 只能为核心高光或候选高光；核心高光必须具备明确成交确认，或同时具备清晰商品、强需求、有效异议处理和购买动作，不满足时一律为候选高光。score 为0—100整数，衡量证据强度、话术可复用性和业务价值，不得为了高分编造事实。signals 输出1—4个简短证据信号；hook 是片段最值得保留的一句话；takeaway 是可沉淀进母稿的单句结论。成交片段必须有明确成交确认、锁货、备注、恭喜下单或可匹配订单；仅报价、上链接、购买CTA不得判定成交。没有订单证据时，“未成交”只能写问价未见成交信号。时间必须来自字幕且为相对录播开始的秒数。候选区间应保留触发点前30—90秒和后20—30秒。商品、价格、库存、链接号不确定时写待确认，不得编造。无合格候选时返回 []。`,
          messages: [{ role: "user", content: chunks[index] }],
        });
        if (
          !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
          || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
        ) return;
        found.push(...parseCandidates(response));
      }
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
        || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
      ) return;
      const modelCandidates = selectCandidates(found);
      const usedFallback = modelCandidates.length === 0;
      reviews = {};
      candidates = usedFallback ? selectCandidates(buildEvidenceFallback(transcriptEntries)) : modelCandidates;
      discoveryCompleted = true;
      selectedCandidateId = candidates[0]?.id || "";
      selectedCandidate = candidates[0] || null;
      selectedReview = selectedCandidate ? reviews[selectedCandidate.id] || null : null;
      stage = candidates.length
        ? `${usedFallback ? "模型漏检，已按明确动作兜底发现" : "发现"} ${candidates.length} 个候选片段，正在复盘第一个片段`
        : "片段发现完成，但没有证据充分的候选片段";
      saveState(requestIdentity);
      isDiscovering = false;
      if (selectedCandidateId) {
        await selectCandidate(candidates[0]);
      }
    } catch (error: any) {
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
        || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
      ) return;
      errorMessage = error?.message || String(error);
      stage = "片段发现失败";
    } finally {
      if (
        isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
        && isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
      ) {
        isDiscovering = false;
      }
    }
  }

  function candidateTranscript(candidate: Candidate): string {
    const entries = transcriptEntries.filter((entry) => entry.end >= candidate.start && entry.start <= candidate.end);
    return entries
      .map((entry) => `${formatTime(entry.start)}—${formatTime(entry.end)} ${entry.text}`)
      .join("\n");
  }

  function updatePlayerAndTranscript(scroll = true, candidateOverride: Candidate | null = null): void {
    const candidate = candidateOverride || selectedCandidate;
    if (!candidate) return;
    const requestIdentity = activeAnalysisRequestIdentity();
    if (archive) {
      playerNonce += 1;
      const params = new URLSearchParams({
        platform: archive.platform,
        room_id: String(archive.room_id),
        live_id: String(archive.live_id),
        start: String(Math.floor(candidate.start)),
        end: String(Math.ceil(candidate.end)),
        nonce: String(playerNonce),
        embed: "1",
      });
      playerUrl = `index_live.html?${params.toString()}`;
    } else if (videoElement) {
      videoElement.currentTime = candidate.start;
      void videoElement.play().catch(() => undefined);
    }
    if (scroll) {
      void tick().then(() => {
        if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
        const first = transcriptEntries.find((entry) => entry.end >= candidate.start);
        document.getElementById(`analysis-transcript-${first?.id}`)?.scrollIntoView({ block: "center" });
      });
    }
  }

  function seekToTranscriptEntry(entry: TranscriptEntry): void {
    if (video && videoElement) {
      videoElement.currentTime = entry.start;
      void videoElement.play().catch(() => undefined);
      return;
    }
    if (!archive) return;
    playerNonce += 1;
    const params = new URLSearchParams({
      platform: archive.platform,
      room_id: String(archive.room_id),
      live_id: String(archive.live_id),
      start: String(Math.floor(entry.start)),
      end: String(Math.ceil(entry.end + 20)),
      nonce: String(playerNonce),
      embed: "1",
    });
    playerUrl = `index_live.html?${params.toString()}`;
  }

  async function selectCandidate(candidate: Candidate): Promise<void> {
    selectedCandidateId = candidate.id;
    selectedCandidate = candidate;
    selectedReview = reviews[candidate.id] || null;
    masterComparisonSequence += 1;
    masterComparison = null;
    masterComparisonError = "";
    masterComparisonLoading = false;
    masterMatchStatus = "idle";
    selectedMasterSectionId = 0;
    reviewTab = "analysis";
    updatePlayerAndTranscript(true, candidate);
    saveState();
    const comparison = autoCompareCandidateToMaster(candidate);
    if (!reviews[candidate.id]) {
      await Promise.all([reviewSelectedCandidate(false, candidate), comparison]);
    } else {
      await comparison;
    }
  }

  function parseReview(content: string): ReviewResult {
    const cleaned = content.trim().replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/, "");
    let parsed: any = null;
    const start = cleaned.indexOf("{");
    const end = cleaned.lastIndexOf("}");
    if (start >= 0 && end > start) {
      try {
        parsed = JSON.parse(cleaned.slice(start, end + 1));
      } catch {
        parsed = null;
      }
    }
    const extractStringField = (key: string): string => {
      const marker = `"${key}"`;
      const markerIndex = cleaned.indexOf(marker);
      if (markerIndex < 0) return "";
      const colonIndex = cleaned.indexOf(":", markerIndex + marker.length);
      if (colonIndex < 0) return "";
      let cursor = colonIndex + 1;
      while (/\s/.test(cleaned[cursor] || "")) cursor += 1;
      if (cleaned[cursor] !== '"') return "";
      const valueStart = cursor;
      cursor += 1;
      let escaped = false;
      while (cursor < cleaned.length) {
        const character = cleaned[cursor];
        if (character === '"' && !escaped) {
          try {
            return JSON.parse(cleaned.slice(valueStart, cursor + 1));
          } catch {
            return "";
          }
        }
        if (character === "\\" && !escaped) escaped = true;
        else escaped = false;
        cursor += 1;
      }
      return "";
    };
    const reviewText = String(
      parsed?.professional_detail ||
      parsed?.review ||
      parsed?.review_markdown ||
      extractStringField("professional_detail") ||
      extractStringField("review") ||
      cleaned
    );
    const extractMarkdownSection = (source: string, headingPattern: RegExp): string => {
      const lines = source.split(/\r?\n/);
      const headingIndex = lines.findIndex((line) => /^#{1,4}\s+/.test(line) && headingPattern.test(line));
      if (headingIndex < 0) return "";
      let endIndex = lines.length;
      for (let index = headingIndex + 1; index < lines.length; index += 1) {
        if (/^#{1,4}\s+/.test(lines[index])) {
          endIndex = index;
          break;
        }
      }
      return lines.slice(headingIndex, endIndex).join("\n").trim();
    };
    const spokenText = String(
      parsed?.spoken_script ||
      parsed?.spokenScript ||
      extractStringField("spoken_script") ||
      extractStringField("spokenScript") ||
      extractMarkdownSection(reviewText, /下一次可直接口播|可直接口播|口播版本|忠实优化版/) ||
      extractMarkdownSection(reviewText, /可复用话术模板/)
    );
    const trainingText = String(parsed?.training_checklist || parsed?.trainingChecklist || extractStringField("training_checklist") || extractStringField("trainingChecklist"));
    return {
      beginner: normalizeBeginnerReview(parsed || {}, reviewText),
      review: reviewText,
      spokenScript: spokenText || "模型本次未返回独立口播字段，请点击“重新复盘”后再试。",
      trainingChecklist: trainingText || "模型本次未返回独立训练清单字段，请点击“重新复盘”后再试。",
      raw: content,
      updatedAt: new Date().toISOString(),
    };
  }

  async function reviewSelectedCandidate(force = false, candidateOverride: Candidate | null = null): Promise<void> {
    const candidate = candidateOverride || selectedCandidate;
    const candidateReviewGate = highlightReviewGate(currentWorkflowStage(), discoveryCompleted, isDiscovering);
    if (!candidate || reviewingId || !candidateReviewGate.allowed) return;
    if (!force && reviews[candidate.id]) return;
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestCandidateIdentity = candidateReviewIdentity(candidateGeneration, candidate.id);
    const requestId = ++reviewRequestSequence;
    reviewingId = candidate.id;
    errorMessage = "";
    stage = `复盘官正在分析 ${candidate.id}…`;
    try {
      const clipTranscript = candidateTranscript(candidate);
      const response = await invoke<string>("minimax_chat", {
        systemPrompt: `${COMMERCE_REVIEW_PROMPT}\n\n这份结果主要交给没有直播运营经验的新手阅读。最终只返回一个合法 JSON 对象，不要代码围栏。字段固定为 verdict、summary、good_points、improvements、checks、spoken_script、training_checklist、professional_detail。verdict 只能是“建议保留”“修改后保留”“建议放弃”“证据不足”；summary 用一句大白话说明原因，不超过45个汉字；good_points、improvements、checks 都是1—4条短句数组，每句只说一件事，禁止使用“链路签名、CTA、证据强度、预分类”等术语。spoken_script 给可以直接照读的口播；training_checklist 写具体练习动作；professional_detail 才承载原固定格式的专业内容。JSON 字符串中的换行必须正确转义。`,
        messages: [{
          role: "user",
          content: `请复盘当前候选片段。候选类型只是待核验线索，必须根据逐字稿重新预分类。缺少订单时间或明确成交确认时，不得写成已成交。\n\n内容：${sourceTitle()}\n候选编号：${candidate.id}\n候选类型：${candidate.type}\n候选商品：${candidate.product}\n候选时间：${formatTime(candidate.start)}—${formatTime(candidate.end)}\n候选证据：${candidate.evidence}\n待核验：${candidate.verify || "无"}\n\n逐字稿：\n${clipTranscript}`,
        }],
      });
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, reviewRequestSequence)
        || !isCurrentCandidateReview(
          requestCandidateIdentity,
          candidateGeneration,
          candidates.map((item) => item.id),
          isDiscovering,
        )
      ) return;
      const review = parseReview(response);
      reviews = { ...reviews, [candidate.id]: review };
      if (selectedCandidateId === candidate.id) selectedReview = review;
      stage = `${candidate.id} 复盘完成，结果已自动保存`;
      saveState(requestIdentity);
    } catch (error: any) {
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, reviewRequestSequence)
        || !isCurrentCandidateReview(
          requestCandidateIdentity,
          candidateGeneration,
          candidates.map((item) => item.id),
          isDiscovering,
        )
      ) return;
      errorMessage = error?.message || String(error);
      stage = `${candidate.id} 复盘失败`;
    } finally {
      if (
        isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, reviewRequestSequence)
        && isCurrentCandidateReview(
          requestCandidateIdentity,
          candidateGeneration,
          candidates.map((item) => item.id),
          isDiscovering,
        )
      ) {
        reviewingId = "";
      }
    }
  }

  async function copyText(value: string, action: string): Promise<void> {
    const requestIdentity = activeAnalysisRequestIdentity();
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
      } else {
        const textarea = document.createElement("textarea");
        textarea.value = value;
        textarea.style.position = "fixed";
        textarea.style.opacity = "0";
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand("copy");
        textarea.remove();
      }
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
      copiedAction = action;
      setTimeout(() => {
        if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
        if (copiedAction === action) copiedAction = "";
      }, 1600);
    } catch (error) {
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
      errorMessage = `复制失败：${error}`;
    }
  }

  function copySpokenScript(): void {
    if (selectedReview) void copyText(selectedReview.spokenScript, "口播");
  }

  function copyAll(): void {
    if (!selectedCandidate || !selectedReview) return;
    const content = [
      `# ${sourceTitle()}｜${selectedCandidate.id}`,
      `高光：${selectedCandidate.tier}｜${selectedCandidate.score}分｜${selectedCandidate.type}｜${selectedCandidate.product}｜${formatTime(selectedCandidate.start)}—${formatTime(selectedCandidate.end)}`,
      `## 一句话结论\n${selectedReview.beginner.verdict}：${selectedReview.beginner.summary}`,
      `## 做对了什么\n${selectedReview.beginner.goodPoints.map((item) => `- ${item}`).join("\n")}`,
      `## 下次怎么改\n${selectedReview.beginner.improvements.map((item) => `- ${item}`).join("\n")}`,
      `## 还要确认\n${selectedReview.beginner.checks.map((item) => `- ${item}`).join("\n")}`,
      "## 口播",
      selectedReview.spokenScript,
      "## 训练清单",
      selectedReview.trainingChecklist,
    ].join("\n\n");
    void copyText(content, "全部");
  }

  function buildVideoAsMaster(): void {
    if (!video) return;
    window.dispatchEvent(new CustomEvent("bsr:build-master", { detail: video }));
  }
</script>

<div class="analysis-shell">
  <header class="analysis-header">
    <div class="header-main">
      <button class="icon-button" title={video ? "返回视频列表" : "返回录播"} on:click={() => dispatch("back")}>
        <ArrowLeft size={18} />
      </button>
      <div class="min-w-0">
        <div class="title-row"><h1>典典直播切片</h1><span class="mode-pill">片段分析</span></div>
        <p title={sourceTitle(archive, video)}>{sourceTitle(archive, video)}</p>
      </div>
    </div>
    <div class="header-actions">
      <span class="stage" class:error-stage={Boolean(errorMessage)}>{errorMessage || stage}</span>
      {#if video}
        <button class="secondary-button master-button" on:click={buildVideoAsMaster}>
          <BookOpenCheck size={15} />
          设为整场母稿
        </button>
      {/if}
      <button class="secondary-button" disabled={!currentSource || isTranscribing || isDiscovering} on:click={regenerateAndAnalyze}>
        <span class:is-spinning={isTranscribing}><RotateCcw size={15} /></span>
        重新识别
      </button>
      <button
        class="secondary-button"
        disabled={!transcript || isDiscovering || isTranscribing || discoveryAction === "blocked" || discoveryAction === "continue"}
        on:click={() => discoverCandidates(currentSourceKey, discoveryAction === "rerun" ? "rerun" : "auto")}
      >
        <span class:is-spinning={isDiscovering}><RefreshCw size={15} /></span>
        重新发现
      </button>
    </div>
  </header>

  <div class="analysis-grid">
    <section class="column video-column">
      <div class="column-title">
        <div><span class="step">1</span>视频和重点片段</div>
        <span>{candidates.length} 个片段</span>
      </div>
      <div class="player-wrap">
        {#if video && videoPlayerUrl}
          <!-- svelte-ignore a11y-media-has-caption -->
          <video bind:this={videoElement} src={videoPlayerUrl} controls playsinline />
        {:else if playerUrl}
          {#key playerUrl}
            <iframe title="录播片段播放器" src={playerUrl} allow="autoplay; fullscreen" />
          {/key}
        {:else}
          <div class="empty-player">
            {#if isTranscribing || isDiscovering}
              <Loader2 size={32} class="is-spinning" />
              <span>{stage}</span>
            {:else}
              <Play size={32} />
              <span>发现片段后在这里播放</span>
            {/if}
          </div>
        {/if}
      </div>

      <div class="highlight-filters" aria-label="高光层级筛选">
        <button class:active={highlightFilter === "全部"} on:click={() => highlightFilter = "全部"}>
          全部 <span>{candidates.length}</span>
        </button>
        <button class:active={highlightFilter === "核心高光"} on:click={() => highlightFilter = "核心高光"}>
          重点 <span>{coreHighlightCount}</span>
        </button>
        <button class:active={highlightFilter === "候选高光"} on:click={() => highlightFilter = "候选高光"}>
          待复核 <span>{candidateHighlightCount}</span>
        </button>
      </div>

      <div class="candidate-list selectable">
        {#if filteredCandidates.length}
          {#each filteredCandidates as candidate}
            <button
              class="candidate-card"
              class:selected={candidate.id === selectedCandidateId}
              class:core={candidate.tier === "核心高光"}
              on:click={() => selectCandidate(candidate)}
            >
              <div class="candidate-heading">
                <strong>{candidate.id} · {candidate.product}</strong>
                <span class="tier-pill" class:core-tier={candidate.tier === "核心高光"}>{tierDisplay(candidate.tier)}</span>
              </div>
              <div class="candidate-meta">
                <span>{formatTime(candidate.start)}—{formatTime(candidate.end)}</span>
                <span>{candidate.type}</span>
                <strong>{candidate.score} 分</strong>
              </div>
              <div class="signal-row">
                {#each candidate.signals as signal}<span>{signal}</span>{/each}
              </div>
              <p>{candidate.takeaway || candidate.reason || candidate.evidence || "等待复核"}</p>
            </button>
          {/each}
          <details class="json-details">
            <summary>查看片段发现器 JSON</summary>
            <pre>{JSON.stringify(candidates, null, 2)}</pre>
          </details>
        {:else if candidates.length}
          <div class="empty-state"><FileSearch size={28} /><span>当前层级暂无片段，可切换其他筛选。</span></div>
        {:else if discoveryCompleted}
          <div class="empty-state"><FileSearch size={28} /><span>没有发现证据充分的候选片段，可点击“重新发现”。</span></div>
        {:else}
          <div class="empty-state"><FileSearch size={28} /><span>ASR 完成后将自动发现候选片段。</span></div>
        {/if}
      </div>
    </section>

    <section class="column transcript-column">
      <div class="column-title">
        <div><span class="step">2</span>逐字稿</div>
        <span title={transcriptRefreshNotice}>{transcriptRefreshNotice || `${transcriptEntries.length} 条`}</span>
      </div>
      <div class="transcript-list selectable">
        {#if transcriptEntries.length}
          {#each transcriptEntries as entry}
            <button
              id={`analysis-transcript-${entry.id}`}
              class="transcript-entry"
              class:in-candidate={Boolean(selectedCandidate && entry.end >= selectedCandidate.start && entry.start <= selectedCandidate.end)}
              class:in-correction={Boolean(
                workspaceTab === "proofreading" &&
                selectedTranscriptCorrection &&
                entry.end >= selectedTranscriptCorrection.startMs / 1000 &&
                entry.start <= selectedTranscriptCorrection.endMs / 1000
              )}
              on:click={() => seekToTranscriptEntry(entry)}
            >
              <time>{formatTime(entry.start)}</time>
              <span>{entry.text}</span>
            </button>
          {/each}
        {:else}
          <div class="empty-state large"><span class:is-spinning={isTranscribing}><Loader2 size={32} /></span><span>{stage}</span></div>
        {/if}
      </div>
    </section>

    <section class="column review-column">
      <div class="column-title">
        <div><span class="step">3</span>{workspaceTab === "proofreading" ? "校稿审核" : "这段讲得怎么样"}</div>
        {#if workspaceTab === "analysis"}<div class="copy-actions">
          <button title="复制口播" disabled={!selectedReview} on:click={copySpokenScript}>
            {#if copiedAction === "口播"}<Check size={14} />{:else}<Copy size={14} />{/if}
            复制口播
          </button>
          <button title="复制全部" disabled={!selectedReview} on:click={copyAll}>
            {#if copiedAction === "全部"}<Check size={14} />{:else}<Clipboard size={14} />{/if}
            复制全部
          </button>
        </div>{:else if auditBundle}<span>{auditBundle.pendingCriticalCount} 条待确认</span>{/if}
      </div>
      <div class="workspace-tabs" role="tablist" aria-label="右栏工作区">
        <button class:active={workspaceTab === "proofreading"} on:click={() => workspaceTab = "proofreading"}>
          校稿审核{#if auditBundle?.pendingCriticalCount}<span>{auditBundle.pendingCriticalCount}</span>{/if}
        </button>
        <button class:active={workspaceTab === "analysis"} on:click={() => workspaceTab = "analysis"}>片段分析</button>
      </div>
      {#if workspaceTab === "proofreading"}
        <div class="proofreading-content">
          {#if auditLoadStatus === "loading"}
            <div class="empty-state large"><Loader2 size={30} class="is-spinning" /><span>正在读取校稿内容…</span></div>
          {:else if auditBundle}
            <div class="review-panel-slot">
              <TranscriptReviewPanel
                bundle={auditBundle}
                {selectedCorrectionId}
                requestIdentity={activeAnalysisRequestIdentity()}
                {reviewRequestToken}
                disabled={transcriptReviewLockState.disabled}
                disabledReason={transcriptReviewLockState.reason}
                on:select={(event) => selectTranscriptCorrection(event.detail)}
                on:update={updateTranscriptReview}
                on:complete={updateTranscriptReview}
              />
            </div>
            {#if discoveryAction === "continue"}
              <div class="continue-analysis">
                <p>关键内容已经确认，可以开始识别高光片段。</p>
                <button class="primary-button" on:click={continueToHighlightAnalysis}>
                  全部处理完成，继续分析
                </button>
              </div>
            {/if}
          {:else if auditError}
            <div class="empty-state large">
              <FileSearch size={30} />
              <span>校稿内容读取失败：{auditError}</span>
              <button class="primary-button" on:click={loadTranscriptReview}>重新读取</button>
            </div>
          {:else}
            <div class="empty-state large"><FileSearch size={30} /><span>逐字稿生成后，这里会显示需要确认的内容。</span></div>
          {/if}
        </div>
      {:else}
      <div class="review-content selectable">
        {#if selectedCandidate}
          <div class="review-summary">
            <div>
              <span class="tier-label" class:core-tier={selectedCandidate.tier === "核心高光"}>{tierDisplay(selectedCandidate.tier)}</span>
              <strong>{selectedCandidate.type} · {selectedCandidate.product}</strong>
              <small>{formatTime(selectedCandidate.start)}—{formatTime(selectedCandidate.end)}</small>
            </div>
            <div class="highlight-score"><strong>{selectedCandidate.score}</strong><span>推荐分</span></div>
          </div>
          <div class="evidence-card">
            <div class="signal-row">
              {#each selectedCandidate.signals as signal}<span>{signal}</span>{/each}
            </div>
            {#if selectedCandidate.hook}<blockquote>“{selectedCandidate.hook}”</blockquote>{/if}
            <p><strong>为什么选</strong>{selectedCandidate.evidence || selectedCandidate.reason || "等待复核"}</p>
            <p class="verify-line"><strong>还要确认</strong>{selectedCandidate.verify || "暂无"}</p>
          </div>
          <MasterComparisonPanel
            baseline={masterBaseline}
            bind:selectedSectionId={selectedMasterSectionId}
            result={masterComparison}
            loading={masterComparisonLoading}
            error={masterComparisonError}
            matchStatus={masterMatchStatus}
          />
        {/if}

        {#if reviewingId && reviewingId === selectedCandidateId}
          <div class="empty-state large"><Loader2 size={32} class="is-spinning" /><span>复盘官正在生成复盘、口播和训练清单…</span></div>
        {:else if selectedReview}
          <div class="review-tabs" role="tablist" aria-label="复盘内容">
            <button class:active={reviewTab === "analysis"} on:click={() => reviewTab = "analysis"}><BarChart3 size={14} />一眼结论</button>
            <button class:active={reviewTab === "script"} on:click={() => reviewTab = "script"}><MessageSquareText size={14} />直接照着说</button>
            <button class:active={reviewTab === "training"} on:click={() => reviewTab = "training"}><ListChecks size={14} />具体怎么练</button>
          </div>
          {#if reviewTab === "analysis"}
            <article class="beginner-verdict" class:keep={selectedReview.beginner.verdict === "建议保留"}>
              <span>这段要不要留？</span>
              <h2>{selectedReview.beginner.verdict}</h2>
              <p>{selectedReview.beginner.summary}</p>
            </article>
            <section class="beginner-section good">
              <h3><Check size={16} />做对了什么</h3>
              <ul>{#each selectedReview.beginner.goodPoints as item}<li>{item}</li>{/each}</ul>
            </section>
            <section class="beginner-section improve">
              <h3><RefreshCw size={16} />下次怎么改</h3>
              <ul>{#each selectedReview.beginner.improvements as item}<li>{item}</li>{/each}</ul>
            </section>
            <section class="beginner-section check">
              <h3><FileSearch size={16} />还要确认什么</h3>
              <ul>{#each selectedReview.beginner.checks as item}<li>{item}</li>{/each}</ul>
            </section>
            <details class="professional-details">
              <summary>查看专业分析</summary>
              <pre>{selectedReview.review}</pre>
            </details>
          {:else if reviewTab === "script"}
            <article class="result-card accent">
              <h2>下一次直接照着说</h2>
              <pre>{selectedReview.spokenScript}</pre>
            </article>
          {:else}
            <article class="result-card training">
              <h2>主播具体这样练</h2>
              <pre>{selectedReview.trainingChecklist}</pre>
            </article>
          {/if}
          <button
            class="rerun-button"
            disabled={Boolean(reviewingId) || !candidateReviewGateState.allowed}
            title={candidateReviewGateState.message || "重新复盘当前片段"}
            on:click={() => reviewSelectedCandidate(true)}
          >
            <RefreshCw size={14} />重新复盘当前片段
          </button>
          {#if !candidateReviewGateState.allowed}
            <div class="review-gate-note">{candidateReviewGateState.message}</div>
          {/if}
          <div class="saved-hint">已自动保存 · {new Date(selectedReview.updatedAt).toLocaleString("zh-CN")}</div>
        {:else if selectedCandidate}
          <div class="empty-state large">
            <FileSearch size={30} />
            {#if candidateReviewGateState.allowed}
              <span>当前片段尚未复盘</span>
              <button class="primary-button" on:click={() => reviewSelectedCandidate(true)}>开始复盘</button>
            {:else}
              <span>{candidateReviewGateState.message}</span>
            {/if}
          </div>
        {:else}
          <div class="empty-state large"><FileSearch size={30} /><span>选择候选片段后自动调用复盘官。</span></div>
        {/if}
      </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .analysis-shell { height: 100%; min-width: 980px; display: flex; flex-direction: column; color: #1d1d1f; background: linear-gradient(180deg, #fbfbfd 0%, #f2f3f6 100%); }
  .analysis-header { height: 76px; flex: 0 0 76px; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 0 20px; background: rgba(255,255,255,.76); border-bottom: 1px solid rgba(0,0,0,.055); backdrop-filter: blur(24px) saturate(160%); }
  .header-main, .header-actions, .column-title, .candidate-heading, .review-summary, .copy-actions { display: flex; align-items: center; }
  .header-main { gap: 10px; min-width: 0; }
  .title-row { display: flex; align-items: center; gap: 8px; }
  .header-main h1 { margin: 0; font-size: 19px; font-weight: 700; letter-spacing: -.45px; }
  .mode-pill { padding: 3px 7px; border-radius: 999px; color: #0068d1; background: rgba(0,113,227,.09); font-size: 9px; font-weight: 650; }
  .header-main p { margin: 3px 0 0; color: #86868b; font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 390px; }
  .header-actions { gap: 8px; }
  .stage { max-width: 330px; padding: 6px 9px; color: #6e6e73; border-radius: 999px; background: rgba(118,118,128,.08); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .error-stage { color: #d70015; background: rgba(255,59,48,.08); }
  button { font: inherit; }
  .icon-button, .secondary-button, .copy-actions button, .primary-button { border: 1px solid rgba(0,0,0,.065); background: rgba(255,255,255,.88); border-radius: 10px; cursor: pointer; box-shadow: 0 2px 5px rgba(20,24,32,.04); transition: transform .16s ease, background .16s ease, box-shadow .16s ease; }
  .icon-button:hover, .secondary-button:hover, .copy-actions button:hover { background: white; box-shadow: 0 5px 13px rgba(20,24,32,.08); }
  .icon-button:active, .secondary-button:active, .copy-actions button:active { transform: scale(.97); }
  .icon-button { width: 34px; height: 34px; display: grid; place-items: center; color: #3a3a3c; }
  .secondary-button { height: 32px; display: inline-flex; align-items: center; gap: 5px; padding: 0 11px; font-size: 11px; color: #3a3a3c; }
  .master-button { color: #0068d1; border-color: rgba(0,113,227,.2); background: #f2f8ff; }
  button:disabled { cursor: not-allowed; opacity: .45; }
  .analysis-grid { min-height: 0; flex: 1; display: grid; grid-template-columns: minmax(280px, .9fr) minmax(320px, 1fr) minmax(360px, 1.15fr); gap: 13px; padding: 13px; overflow-x: auto; }
  .column { min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: rgba(255,255,255,.86); border: 1px solid rgba(255,255,255,.94); border-radius: 18px; box-shadow: 0 12px 30px rgba(41,47,58,.075), 0 1px 3px rgba(41,47,58,.05); backdrop-filter: blur(18px); }
  .column-title { height: 52px; flex: 0 0 52px; justify-content: space-between; gap: 8px; padding: 0 15px; border-bottom: 1px solid rgba(0,0,0,.055); font-size: 12px; font-weight: 650; letter-spacing: -.1px; }
  .column-title > span { color: #8e8e93; font-size: 10px; font-weight: 500; }
  .step { display: inline-grid; place-items: center; width: 22px; height: 22px; margin-right: 7px; border-radius: 8px; background: linear-gradient(145deg,#e9f4ff,#dcecff); color: #0071e3; font-size: 10px; box-shadow: inset 0 0 0 1px rgba(0,113,227,.06); }
  .player-wrap { height: 285px; flex: 0 0 285px; margin: 11px; overflow: hidden; border-radius: 14px; background: #0b0f17; box-shadow: 0 8px 22px rgba(0,0,0,.18); }
  .player-wrap iframe, .player-wrap video { width: 100%; height: 100%; border: 0; background: #000; object-fit: contain; }
  .empty-player { width: 100%; height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: #9aa4b2; font-size: 12px; text-align: center; }
  .highlight-filters { display: grid; grid-template-columns: repeat(3, 1fr); gap: 3px; margin: 0 11px 4px; padding: 3px; border-radius: 8px; background: rgba(118,118,128,.08); }
  .highlight-filters button { height: 29px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; border: 0; border-radius: 6px; color: #6e6e73; background: transparent; font-size: 10px; cursor: pointer; }
  .highlight-filters button.active { color: #1d1d1f; background: white; box-shadow: 0 1px 4px rgba(28,34,43,.1); }
  .highlight-filters button span { color: #8e8e93; font-variant-numeric: tabular-nums; }
  .candidate-list, .transcript-list, .review-content { min-height: 0; flex: 1; overflow: auto; }
  .candidate-list { padding: 0 11px 11px; }
  .candidate-card { width: 100%; display: block; margin-top: 8px; padding: 11px; text-align: left; border: 1px solid rgba(0,0,0,.065); border-radius: 13px; background: rgba(248,248,250,.8); cursor: pointer; color: inherit; transition: transform .16s ease, background .16s ease, border-color .16s ease; }
  .candidate-card:hover { transform: translateY(-1px); border-color: rgba(0,113,227,.24); background: white; }
  .candidate-card.selected { border-color: rgba(0,113,227,.4); background: #edf6ff; box-shadow: 0 0 0 1px rgba(0,113,227,.08) inset, 0 5px 14px rgba(0,113,227,.08); }
  .candidate-card.core { border-left: 3px solid #16a365; }
  .candidate-heading { justify-content: space-between; gap: 8px; font-size: 12px; }
  .candidate-heading strong { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tier-pill, .tier-label { flex: 0 0 auto; padding: 2px 6px; border-radius: 999px; background: #fff5e6; color: #a34b00; font-size: 9px; font-weight: 650; }
  .core-tier { background: #eaf8f0; color: #087c42; }
  .candidate-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 5px 9px; margin-top: 6px; color: #667085; font-size: 10px; }
  .candidate-meta strong { margin-left: auto; color: #344054; font-size: 10px; }
  .signal-row { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 7px; }
  .signal-row span { padding: 3px 6px; border-radius: 5px; color: #365c7d; background: #edf5fb; font-size: 9px; }
  .candidate-card p { margin: 6px 0 0; color: #6b7280; font-size: 11px; line-height: 1.45; }
  .json-details { margin-top: 10px; padding: 9px; border-radius: 11px; background: rgba(118,118,128,.07); font-size: 10px; }
  .json-details summary { cursor: pointer; color: #667085; }
  .json-details pre { max-height: 220px; overflow: auto; white-space: pre-wrap; word-break: break-word; }
  .transcript-list { padding: 8px; scroll-behavior: smooth; }
  .transcript-entry { width: 100%; display: grid; grid-template-columns: 58px minmax(0, 1fr); gap: 8px; padding: 9px; border: 0; border-radius: 10px; background: transparent; color: inherit; text-align: left; cursor: pointer; transition: background .14s ease; }
  .transcript-entry:hover { background: rgba(118,118,128,.07); }
  .transcript-entry.in-candidate { background: #edf5ff; }
  .transcript-entry.in-correction { background: #fff4d9; box-shadow: inset 3px 0 0 #f79009; }
  .transcript-entry time { color: #0071e3; font-size: 10px; font-variant-numeric: tabular-nums; }
  .transcript-entry span { font-size: 12px; line-height: 1.55; }
  .copy-actions { gap: 5px; }
  .copy-actions button { display: inline-flex; align-items: center; gap: 4px; padding: 6px 8px; color: #3a3a3c; font-size: 10px; }
  .review-content { padding: 10px; }
  .workspace-tabs { flex: 0 0 auto; display: grid; grid-template-columns: 1fr 1fr; gap: 3px; margin: 8px 10px 0; padding: 3px; border-radius: 7px; background: rgba(118,118,128,.08); }
  .workspace-tabs button { min-width: 0; height: 30px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; border: 0; border-radius: 5px; color: #667085; background: transparent; font-size: 10px; cursor: pointer; }
  .workspace-tabs button.active { color: #1d1d1f; background: white; box-shadow: 0 1px 4px rgba(28,34,43,.1); }
  .workspace-tabs span { min-width: 17px; padding: 1px 4px; border-radius: 8px; color: #a34b00; background: #fff0d5; font-size: 9px; }
  .proofreading-content { min-height: 0; flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .review-panel-slot { min-height: 0; flex: 1; overflow: hidden; }
  .continue-analysis { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 14px; border-top: 1px solid rgba(0,0,0,.06); background: #f4f9ff; }
  .continue-analysis p { margin: 0; color: #475467; font-size: 12px; }
  .continue-analysis .primary-button { flex: 0 0 auto; }
  .review-summary { justify-content: space-between; gap: 12px; margin-bottom: 8px; padding: 11px; border-radius: 8px; background: rgba(118,118,128,.07); font-size: 11px; }
  .review-summary > div:first-child { min-width: 0; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: 6px; }
  .review-summary strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .review-summary small { grid-column: 1 / -1; color: #6b7280; font-size: 10px; }
  .highlight-score { flex: 0 0 auto; display: grid; place-items: center; min-width: 48px; }
  .highlight-score strong { color: #087c42; font-size: 19px; line-height: 1; font-variant-numeric: tabular-nums; }
  .highlight-score span { margin-top: 3px; color: #8e8e93; font-size: 8px; }
  .evidence-card { margin-bottom: 9px; padding: 10px 11px; border: 1px solid rgba(0,0,0,.055); border-radius: 8px; background: #fafbfc; }
  .evidence-card .signal-row { margin-top: 0; }
  .evidence-card blockquote { margin: 9px 0; padding-left: 9px; border-left: 2px solid #1687f8; color: #24364b; font-size: 11px; line-height: 1.5; }
  .evidence-card p { display: grid; grid-template-columns: 52px minmax(0,1fr); gap: 6px; margin: 7px 0 0; color: #5b6577; font-size: 10px; line-height: 1.5; }
  .evidence-card p strong { color: #344054; }
  .evidence-card .verify-line { color: #8a4b20; }
  .review-tabs { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 3px; margin-bottom: 9px; padding: 3px; border-radius: 8px; background: rgba(118,118,128,.08); }
  .review-tabs button { min-width: 0; height: 31px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; padding: 0 6px; border: 0; border-radius: 6px; color: #667085; background: transparent; font-size: 10px; cursor: pointer; white-space: nowrap; }
  .review-tabs button.active { color: #1d1d1f; background: white; box-shadow: 0 1px 4px rgba(28,34,43,.1); }
  .beginner-verdict { margin-bottom: 8px; padding: 14px; border: 1px solid #d8e5f2; border-left: 4px solid #1687f8; border-radius: 8px; background: #f5faff; }
  .beginner-verdict.keep { border-color: #bfe8d0; border-left-color: #16a365; background: #f1fbf5; }
  .beginner-verdict > span { color: #667085; font-size: 10px; }
  .beginner-verdict h2 { margin: 4px 0 5px; color: #155eef; font-size: 20px; letter-spacing: 0; }
  .beginner-verdict.keep h2 { color: #087c42; }
  .beginner-verdict p { margin: 0; color: #344054; font-size: 12px; line-height: 1.6; }
  .beginner-section { margin-bottom: 8px; padding: 11px 12px; border: 1px solid rgba(0,0,0,.06); border-radius: 8px; background: white; }
  .beginner-section h3 { display: flex; align-items: center; gap: 6px; margin: 0 0 8px; color: #344054; font-size: 12px; }
  .beginner-section.good h3 { color: #087c42; }
  .beginner-section.improve h3 { color: #175cd3; }
  .beginner-section.check h3 { color: #a34b00; }
  .beginner-section ul { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
  .beginner-section li { position: relative; padding-left: 14px; color: #475467; font-size: 11px; line-height: 1.5; }
  .beginner-section li::before { content: ""; position: absolute; left: 1px; top: .62em; width: 5px; height: 5px; border-radius: 50%; background: #98a2b3; }
  .beginner-section.good li::before { background: #16a365; }
  .beginner-section.improve li::before { background: #1687f8; }
  .beginner-section.check li::before { background: #f79009; }
  .professional-details { margin-bottom: 10px; padding: 9px 11px; border-radius: 7px; color: #667085; background: rgba(118,118,128,.07); font-size: 10px; }
  .professional-details summary { cursor: pointer; font-weight: 600; }
  .professional-details pre { max-height: 360px; margin: 10px 0 0; overflow: auto; white-space: pre-wrap; word-break: break-word; color: #475467; font: inherit; line-height: 1.6; }
  .result-card { margin-bottom: 10px; padding: 13px; border: 1px solid rgba(0,0,0,.06); border-radius: 14px; background: rgba(255,255,255,.82); box-shadow: 0 4px 14px rgba(32,38,48,.04); }
  .result-card.accent { border-color: rgba(0,113,227,.15); background: linear-gradient(145deg,#f7fbff,#eef6ff); }
  .result-card.training { border-color: rgba(48,209,88,.15); background: linear-gradient(145deg,#f8fdf9,#eefaf2); }
  .result-card h2 { margin: 0 0 8px; font-size: 13px; }
  .result-card pre { margin: 0; white-space: pre-wrap; word-break: break-word; font: inherit; color: #354052; font-size: 12px; line-height: 1.65; }
  .saved-hint { padding: 4px 2px 12px; color: #98a2b3; font-size: 10px; text-align: right; }
  .rerun-button { width: 100%; height: 32px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; margin-bottom: 5px; border: 1px solid rgba(0,0,0,.07); border-radius: 7px; color: #475467; background: white; font-size: 10px; cursor: pointer; }
  .review-gate-note { margin: 2px 0 7px; color: #667085; font-size: 10px; text-align: center; }
  .empty-state { min-height: 150px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 9px; color: #8b95a5; font-size: 12px; text-align: center; }
  .empty-state.large { min-height: 260px; }
  .primary-button { padding: 7px 13px; color: white; border-color: #0071e3; background: linear-gradient(180deg,#1687f8,#0071e3); box-shadow: 0 6px 14px rgba(0,113,227,.22); }
  .selectable, .selectable * { user-select: text; }
  .selectable button { user-select: text; }
  .is-spinning { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.dark) .analysis-shell { background: linear-gradient(180deg,#202022,#18181a); color: #f5f5f7; }
  :global(.dark) .analysis-header, :global(.dark) .column { background: #2b2c2f; border-color: #3d4148; }
  :global(.dark) .column-title { border-color: #3d4148; }
  :global(.dark) .candidate-card, :global(.dark) .icon-button, :global(.dark) .secondary-button, :global(.dark) .copy-actions button { background: #303238; border-color: #494d55; color: #e8eaf0; }
  :global(.dark) .candidate-card.selected, :global(.dark) .transcript-entry.in-candidate { background: #373851; }
  :global(.dark) .transcript-entry.in-correction { background: #4b412d; }
  :global(.dark) .highlight-filters, :global(.dark) .review-tabs, :global(.dark) .workspace-tabs { background: #24262a; }
  :global(.dark) .highlight-filters button.active, :global(.dark) .review-tabs button.active, :global(.dark) .workspace-tabs button.active, :global(.dark) .rerun-button { color: #f5f5f7; background: #3a3d43; }
  :global(.dark) .evidence-card { border-color: #494d55; background: #303238; }
  :global(.dark) .beginner-verdict, :global(.dark) .beginner-section { border-color: #494d55; background: #303238; }
  :global(.dark) .beginner-verdict p, :global(.dark) .beginner-section li, :global(.dark) .professional-details pre { color: #d0d5dd; }
  :global(.dark) .result-card { background: #303238; border-color: #494d55; }
  :global(.dark) .result-card.accent { background: #34354b; }
  :global(.dark) .result-card.training { background: #293b32; }
  :global(.dark) .result-card pre { color: #e5e7eb; }
</style>
