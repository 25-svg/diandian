<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import {
    ArrowLeft,
    BarChart3,
    BookOpenCheck,
    Check,
    Clipboard,
    Copy,
    Download,
    FileSearch,
    ListChecks,
    Loader2,
    MessageSquareText,
    Minus,
    Maximize2,
    Minimize2,
    Pause,
    Play,
    Plus,
    RefreshCw,
    RotateCcw,
    Upload,
  } from "lucide-svelte";
  import TranscriptReviewPanel from "../lib/components/analysis/TranscriptReviewPanel.svelte";
  import MasterComparisonPanel from "../lib/components/analysis/MasterComparisonPanel.svelte";
  import DealTimelinePanel from "../lib/components/analysis/DealTimelinePanel.svelte";
  import CompanyAnalysisWorkspace from "../lib/components/analysis/CompanyAnalysisWorkspace.svelte";
  import type { CompanyAnalysisTab } from "../lib/companyAnalysisWorkspace";
  import {
    buildDealClipContexts,
    buildDealClipUserPrompt,
    dealClipContextCacheKey,
    DEAL_CLIP_SYSTEM_PROMPT,
    finalizeDealClipFromAi,
    mergeDealClipRanges,
    selectDealClipContext,
    type DealClipRange,
  } from "../lib/dealOrderAutoClip";
  import { buildClipReviewRequest, clipTranscriptToSrt } from "../lib/clipReview";
  import { get_static_url, invoke, TAURI_ENV } from "../lib/invoker";
  import type { RecordItem } from "../lib/db";
  import { isClipVideo, type VideoItem } from "../lib/interface";
  import { COMMERCE_REVIEW_PROMPT } from "../lib/agent/prompts";
  import { operationalScriptLabel } from "../lib/scriptTaxonomy";
  import { buildImportedArchiveLiveId, isImportedArchive } from "../lib/importedArchive";
  import { dashboardMetricCards, openLiveDashboard, type LiveDashboardMetrics } from "../lib/liveDashboard";
  import { buildSessionDiagnosis } from "../lib/sessionDiagnosis";
  import {
    buildOptimizationPlan,
    formatOptimizationPlanMarkdown,
    optimizationPlanFileName,
  } from "../lib/optimizationPlan";
  import { analyzeTranscriptSignals } from "../lib/transcriptSignals";
  import {
    analyzeSegmentDealSignals,
    buildDealMinuteBuckets,
    buildPeakDealMinuteLabel,
    dealReviewSeekOffset,
    parsePaymentEventsPayload,
    paymentEventsStorageKey,
    type PaymentEvent,
    type PaymentEventsSummary,
  } from "../lib/orderDealTimeline";
  import { scanComplianceRisks } from "../lib/complianceRules";
  import {
    buildScriptQualityUserMessage,
    mergeScriptQualityAnnotations,
    parseScriptQualityBundle,
    scriptQualitySystemPrompt,
    splitTranscriptForScriptQuality,
    type ScriptIssueAnnotation,
  } from "../lib/scriptQuality";
  import {
    ANALYSIS_ZOOM_MAX,
    ANALYSIS_ZOOM_MIN,
    normalizeAnalysisZoom,
    stepAnalysisZoom,
  } from "../lib/analysisZoom";
  import {
    analysisSourceKey,
    buildCompetitorDiscoveryPrompt,
    competitorDiscoveryOutcome,
    friendlyArchiveTranscriptError,
    findActiveArchiveSubtitleTask,
    findActiveVideoSubtitleTask,
    applyCandidateContextVerification,
    buildCandidateDiscoveryPrompt,
    candidateOutcomeLabel,
    candidateNeedsLegacyContextVerification,
    isMasterScoreEligible,
    isStructuredDiscoveryCandidate,
    missingSalesChainStages,
    normalizeBeginnerReview,
    normalizeCandidates,
    parseCandidateDiscoveryResponse,
    parseAnalysisProfile,
    selectDiscoveryCandidates,
    mergeSalesCandidates,
    sessionCandidateWorkPlan,
    summarizeSessionReview,
    selectArchiveTranscriptAction,
    transcriptRefreshFailureState,
    isCurrentMasterComparison,
    autoMatchMasterSection,
    autoMatchMasterScene,
    interruptedChainContextWindow,
    type AnalysisSourceIdentity,
    type BeginnerReview,
    type CandidateType,
    type CandidateVerificationStatus,
    type HighlightCandidate,
  } from "../lib/archiveAnalysis";
  import {
    analysisRequestIdentity,
    candidateReviewIdentity,
    candidateSelectionCancelsReview,
    correctionSelectionTarget,
    firstPendingCorrectionId,
    getTranscriptAudit,
    highlightDiscoveryAction,
    highlightReviewGate,
    highlightWorkflowStage,
    isCurrentCandidateReview,
    isCurrentCandidateGeneration,
    isCurrentAnalysisRequest,
    sessionReviewShouldContinue,
    transcriptReviewChangesTranscript,
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
    listMasterSampleBatches,
    canActivateEnterpriseMaster,
    type MasterBaseline,
    type MasterComparisonResult,
  } from "../lib/masterScript";
  import { parseSavedScriptQuality, scriptQualityStorageKey } from "../lib/scriptQualityPersistence";
  import { playbackPresentation } from "../lib/playbackPresentation";

  export let archive: RecordItem | null = null;
  export let video: VideoItem | null = null;
  export let refreshToken = 0;
  export let analysisMode: "legacy" | "company_deal" = "legacy";

  type TranscriptRefreshResult = {
    subtitle: string;
    decision: "kept" | "replaced" | "created" | "resumed";
    similarity: number;
    oldLength: number;
    newLength: number;
  };

  type BackgroundTask = {
    id: string;
    task_type?: string;
    taskType?: string;
    status: string;
    metadata: string;
    message?: string;
  };

  type VideoPlaybackSource = {
    file: string;
    requiresPreparation: boolean;
    ready: boolean;
    preparing: boolean;
    message: string;
  };

  type VideoPlaybackPreview = {
    file: string;
    startOffset: number;
    message: string;
  };

  declare const shaka: any;

  type TranscriptEntry = {
    id: number;
    start: number;
    end: number;
    text: string;
    raw: string;
  };

  type Candidate = HighlightCandidate;

  type BoundLiveDashboardSession = LiveDashboardMetrics & {
    id: number;
    accountKey: string;
    shopName: string;
    startedAt: string;
    sourceFile: string;
  };

  type LiveDashboardBindingCandidate = {
    session: BoundLiveDashboardSession;
    timeDeltaSeconds: number;
    accountMatch: boolean;
    shopNameMatch: boolean;
  };

  type LiveDashboardBindingResult = {
    session: BoundLiveDashboardSession | null;
    candidates: LiveDashboardBindingCandidate[];
    matchMethod: string | null;
  };

  type ReviewResult = {
    beginner: BeginnerReview;
    review: string;
    spokenScript: string;
    trainingChecklist: string;
    raw: string;
    updatedAt: string;
  };

  type SavedAnalysis = {
    version: 2 | 3 | 4 | 5 | 6;
    transcript: string;
    candidates: Candidate[];
    reviews: Record<string, ReviewResult>;
    masterComparisons?: Record<string, MasterComparisonResult>;
    selectedCandidateId: string;
    discoveryCompleted: boolean;
    updatedAt: string;
    sourceTitle?: string;
    diagnosis?: ReturnType<typeof buildSessionDiagnosis>;
  };

  const dispatch = createEventDispatcher();
  const ANALYSIS_ZOOM_STORAGE_KEY = "bsr:analysis-page-zoom:v1";
  const allowedTypes = new Set<CandidateType>([
    "完整成交链路",
    "关键话术片段",
    "成交收口",
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
  let playbackLoadError = "";
  let transcriptLoadError = "";
  let isTranscribing = false;
  let isDiscovering = false;
  let reviewingId = "";
  let copiedAction = "";
  let playerUrl = "";
  let playerNonce = 0;
  let videoPlayerUrl = "";
  let playbackFileKey = "";
  let videoElement: HTMLVideoElement | null = null;
  let nativePlayerHost: HTMLDivElement | null = null;
  let nativePlayerActive = false;
  let nativePlayerPaused = false;
  let nativePlayerExpanded = false;
  let nativePlayerTransitioning = false;
  let nativePlaybackAnchorSec = 0;
  let nativePlaybackStartedAt = 0;
  let nativePlaybackPositionSec = 0;
  let playbackMode: "native" | "hls" = "native";
  let hlsPlaylistUrl = "";
  let shakaPlayer: any = null;
  let playbackSource: VideoPlaybackSource | null = null;
  let isPreparingPlayback = false;
  let playbackDecodeRetried = false;
  let rawPlaybackFailed = false;
  let embeddedPreviewActive = false;
  let embeddedPreviewRequest = 0;
  let embeddedPreviewStartOffset = 0;
  let embeddedPlaybackPositionSec = 0;
  let embeddedPlaybackPlaying = false;
  let embeddedPlayerShell: HTMLDivElement | null = null;
  let playbackObserverSequence = 0;
  $: playbackView = playbackPresentation(playbackSource, rawPlaybackFailed);
  $: nativePlaybackDurationSec = transcriptEntries.length
    ? Math.max(0, transcriptEntries[transcriptEntries.length - 1].end)
    : 0;
  $: fullPlaybackDurationSec = nativePlaybackDurationSec || Math.max(0, Number(video?.length) || 0);
  const TASK_POLL_INTERVAL_MS = 3_000;
  const TASK_POLL_MAX_ATTEMPTS = 120;
  let highlightFilter: "全部" | "完整成交链路（已核验）" | "其他可评分片段" = "全部";
  let reviewTab: "analysis" | "script" | "training" = "analysis";
  let workspaceTab: "proofreading" | "analysis" = "analysis";
  let companyWorkspaceTab: CompanyAnalysisTab = "deal_speech";
  let selectedDealOffsetSec: number | null = null;
  let dealAutoClipping = false;
  let dealAutoClipProgress = "";
  let dealAutoClipError = "";
  let dealAutoClipSequence = 0;
  let dealSpeechRefineCache: Record<string, DealClipRange> = {};
  let dealSpeechRefineFailures: Record<string, string> = {};
  let dealSpeechRefineSequence = 0;
  let dealSpeechRefining = false;
  let dealSpeechRefineProgress = "";
  let dealSpeechRefineError = "";
  let dealSpeechRefineKey = "";
  let activeDealSpeechRange: DealClipRange | null = null;
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
  let contextVerificationSequence = 0;
  let reviewRequestSequence = 0;
  let reviewRequestToken = 0;
  let candidateGeneration = 0;
  let masterBaseline: MasterBaseline | null = null;
  let masterComparison: MasterComparisonResult | null = null;
  let masterComparisons: Record<string, MasterComparisonResult> = {};
  let selectedMasterSectionId = 0;
  let masterComparisonLoading = false;
  let masterComparisonError = "";
  let masterMatchStatus: "idle" | "matched" | "ambiguous" | "unmatched" = "idle";
  let masterComparisonSequence = 0;
  let isSessionReviewing = false;
  let sessionReviewCompleted = 0;
  let sessionReviewAbort = false;
  let sessionReviewTargetId = "";
  let analysisZoom = 100;
  let liveDashboardBinding: LiveDashboardBindingResult | null = null;
  let liveDashboardBindingLoading = false;
  let liveDashboardBindingError = "";
  let selectedLiveDashboardSessionId = "";
  let paymentEvents: PaymentEvent[] = [];
  let paymentEventsSummary: PaymentEventsSummary | null = null;
  let paymentEventsSourceLabel = "";
  let paymentEventsError = "";
  let paymentEventsInput: HTMLInputElement | null = null;
  let paymentEventsLoading = false;
  let dealRefinementInput: HTMLInputElement | null = null;
  let dealRefinementLoading = false;
  let isScriptQualityAnalyzing = false;
  let scriptQualityError = "";
  let scriptQualitySummary = "";
  let scriptQualityAnnotations: ScriptIssueAnnotation[] = [];
  let selectedScriptCueId: number | null = null;
  let scriptQualityRequestSequence = 0;
  $: analysisProfile = parseAnalysisProfile(video?.note);
  $: analysisPurpose = analysisMode === "company_deal" ? "enterprise_review" : analysisProfile.analysisPurpose;

  function persistAnalysisZoom(value: number): void {
    analysisZoom = normalizeAnalysisZoom(value);
    try {
      localStorage.setItem(ANALYSIS_ZOOM_STORAGE_KEY, String(analysisZoom));
    } catch {
      // Storage can be unavailable; keep the zoom for this session.
    }
  }

  function handleAnalysisWheel(event: WheelEvent): void {
    if (!event.ctrlKey) return;
    event.preventDefault();
    persistAnalysisZoom(stepAnalysisZoom(analysisZoom, event.deltaY < 0 ? 1 : -1));
  }

  onMount(() => {
    try {
      analysisZoom = normalizeAnalysisZoom(localStorage.getItem(ANALYSIS_ZOOM_STORAGE_KEY));
    } catch {
      analysisZoom = 100;
    }
  });

  onDestroy(() => {
    void disposeShakaPlayer();
    void stopNativePlayer();
    if (video) {
      void invoke("stop_video_playback_preview", { id: video.id }).catch(() => undefined);
    }
  });

  onMount(() => {
    const ticker = window.setInterval(() => {
      if (!nativePlayerActive || nativePlayerPaused || !nativePlaybackStartedAt) return;
      const elapsed = (Date.now() - nativePlaybackStartedAt) / 1000;
      nativePlaybackPositionSec = nativePlaybackDurationSec
        ? Math.min(nativePlaybackDurationSec, nativePlaybackAnchorSec + elapsed)
        : nativePlaybackAnchorSec + elapsed;
    }, 250);
    return () => window.clearInterval(ticker);
  });

  $: selectedCandidate = candidates.find((item) => item.id === selectedCandidateId) || null;
  $: selectedReview = selectedCandidate ? reviews[selectedCandidate.id] || null : null;
  $: reusableCandidates = candidates.filter(isStructuredDiscoveryCandidate);
  $: sessionReviewSummary = summarizeSessionReview(reusableCandidates, masterComparisons, reviews);
  $: transcriptSignalMap = Object.fromEntries(candidates.map((candidate) => [
    candidate.id,
    analyzeTranscriptSignals(transcriptEntries, candidate.start, candidate.end),
  ]));
  $: dealSignalMap = Object.fromEntries(candidates.map((candidate) => [
    candidate.id,
    paymentEvents.length
      ? analyzeSegmentDealSignals(paymentEvents, candidate.start, candidate.end)
      : undefined,
  ]));
  $: peakDealMinuteLabel = paymentEvents.length
    ? buildPeakDealMinuteLabel(buildDealMinuteBuckets(paymentEvents))
    : null;
  /** Bottom KPI board only for full live sessions; clipped MP4s never show it. */
  $: showCompanyDataBoard = analysisMode === "company_deal" && !isClipVideo(video);
  /** Binding key: douyin live_id, or import:{videoId} for externally imported full sessions. */
  $: dashboardBindLiveId = resolveDashboardBindLiveId(archive, video);
  $: showCompanyDashboardBind = showCompanyDataBoard && Boolean(dashboardBindLiveId);
  $: topDealPeak = buildDealMinuteBuckets(paymentEvents)
    .sort((left, right) => right.orderCount - left.orderCount || right.totalPayAmountFen - left.totalPayAmountFen)[0];
  $: complianceFindings = scanComplianceRisks(transcriptEntries);
  $: sessionDiagnosis = buildSessionDiagnosis({
    candidates,
    masterComparisons,
    auditBundle,
    sessionReviewSummary,
    transcriptSignals: transcriptSignalMap,
    dealSignals: dealSignalMap,
    complianceFindings,
    peakDealMinuteLabel,
    paymentEventsLoaded: paymentEvents.length > 0,
  });
  $: verifiedChainCount = candidates.filter(isMasterScoreEligible).length;
  $: pendingOrLocalCount = candidates.length - verifiedChainCount;
  $: filteredCandidates = highlightFilter === "全部"
    ? candidates
    : highlightFilter === "完整成交链路（已核验）"
      ? candidates.filter(isMasterScoreEligible)
      : candidates.filter((item) => !isMasterScoreEligible(item));
  $: currentSource = sourceIdentity(archive, video);
  $: currentSourceKey = currentSource ? analysisSourceKey(currentSource) : "";
  // A desktop restart or a late archive prop update can clear the iframe URL
  // after initialization. Recreate the full-session player from the selected
  // company archive instead of leaving the left panel empty.
  $: if (analysisMode === "company_deal" && archive && !video && !playerUrl) {
    loadFullArchivePlayer();
  }
  $: discoveryAction = currentDiscoveryAction();
  $: candidateReviewGateState = highlightReviewGate(currentHighlightWorkflowStage(), discoveryCompleted, isDiscovering);
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

  // Transcript correction remains available for accuracy, but it must not block
  // the operational workflow of finding and reviewing sales moments.
  function currentHighlightWorkflowStage(): AnalysisWorkflowStage {
    return highlightWorkflowStage({
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
    if (analysisMode === "company_deal") return "blocked";
    return highlightDiscoveryAction({
      stage: currentHighlightWorkflowStage(),
      sourceKey: currentSourceKey,
      requestedSourceKey,
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

  /** live_id used to persist 直播大屏 binding for this analysis source. */
  function resolveDashboardBindLiveId(
    selectedArchive: RecordItem | null,
    selectedVideo: VideoItem | null,
  ): string {
    if (selectedArchive?.platform === "douyin" && selectedArchive.live_id) {
      return String(selectedArchive.live_id);
    }
    if (selectedArchive && isImportedArchive(selectedArchive) && selectedArchive.live_id) {
      return String(selectedArchive.live_id);
    }
    if (
      analysisMode === "company_deal"
      && selectedVideo
      && !isClipVideo(selectedVideo)
    ) {
      return buildImportedArchiveLiveId(selectedVideo.id);
    }
    return "";
  }

  function sessionsAsBindCandidates(
    sessions: BoundLiveDashboardSession[],
  ): LiveDashboardBindingCandidate[] {
    return [...sessions]
      .sort((left, right) => String(right.startedAt).localeCompare(String(left.startedAt)))
      .map((session) => ({
        session,
        timeDeltaSeconds: 0,
        accountMatch: false,
        shopNameMatch: false,
      }));
  }

  async function loadLiveDashboardBinding(
    selectedArchive: RecordItem | null = archive,
    selectedVideo: VideoItem | null = video,
  ): Promise<void> {
    liveDashboardBinding = null;
    liveDashboardBindingError = "";
    selectedLiveDashboardSessionId = "";
    const liveId = resolveDashboardBindLiveId(selectedArchive, selectedVideo);
    if (!liveId || isClipVideo(selectedVideo)) return;

    liveDashboardBindingLoading = true;
    try {
      let resolved: LiveDashboardBindingResult | null = null;
      if (selectedArchive?.platform === "douyin") {
        resolved = await invoke<LiveDashboardBindingResult>("resolve_live_dashboard_for_record", {
          platform: selectedArchive.platform,
          roomId: String(selectedArchive.room_id),
          liveId: String(selectedArchive.live_id),
        });
      } else {
        // Imported / video-only: still resolve so previously manual-bound sessions load.
        resolved = await invoke<LiveDashboardBindingResult>("resolve_live_dashboard_for_record", {
          platform: selectedArchive?.platform || selectedVideo?.platform || "imported",
          roomId: String(selectedArchive?.room_id || selectedVideo?.room_id || "bsr:import"),
          liveId,
        });
      }

      // Already bound, or douyin auto-match / shortlist is available.
      if (resolved?.session) {
        liveDashboardBinding = resolved;
        return;
      }
      if (selectedArchive?.platform === "douyin" && (resolved?.candidates?.length ?? 0) > 0) {
        liveDashboardBinding = resolved;
        return;
      }

      // Imported full session (or unmatched): manual picker over all XLSX sessions.
      const sessions = await invoke<BoundLiveDashboardSession[]>("list_live_dashboard_sessions");
      liveDashboardBinding = {
        session: null,
        candidates: sessionsAsBindCandidates(sessions),
        matchMethod: null,
      };
      if (!sessions.length) {
        liveDashboardBindingError = "尚未导入直播大屏 XLSX。请先到「直播数据大屏」导入对应场次（如 7/28 08:15），再回来手动绑定。";
      }
    } catch (error: any) {
      liveDashboardBindingError = error?.message || String(error);
    } finally {
      liveDashboardBindingLoading = false;
    }
  }

  async function bindSelectedLiveDashboard(): Promise<void> {
    const liveId = dashboardBindLiveId || resolveDashboardBindLiveId(archive, video);
    if (!selectedLiveDashboardSessionId || !liveId) return;
    liveDashboardBindingLoading = true;
    liveDashboardBindingError = "";
    try {
      liveDashboardBinding = await invoke<LiveDashboardBindingResult>("bind_live_dashboard_session", {
        liveId,
        sessionId: Number(selectedLiveDashboardSessionId),
      });
      selectedLiveDashboardSessionId = "";
      stage = "已绑定直播大屏场次";
    } catch (error: any) {
      liveDashboardBindingError = error?.message || String(error);
    } finally {
      liveDashboardBindingLoading = false;
    }
  }

  async function rebindLiveDashboardSession(): Promise<void> {
    liveDashboardBindingLoading = true;
    liveDashboardBindingError = "";
    try {
      const sessions = await invoke<BoundLiveDashboardSession[]>("list_live_dashboard_sessions");
      liveDashboardBinding = {
        session: liveDashboardBinding?.session ?? null,
        candidates: sessionsAsBindCandidates(sessions),
        matchMethod: liveDashboardBinding?.matchMethod ?? null,
      };
      if (!sessions.length) {
        liveDashboardBindingError = "尚未导入直播大屏 XLSX。请先到「直播数据大屏」导入对应场次。";
      }
    } catch (error: any) {
      liveDashboardBindingError = error?.message || String(error);
    } finally {
      liveDashboardBindingLoading = false;
    }
  }

  function openBoundLiveDashboard(): void {
    const sessionId = liveDashboardBinding?.session?.id;
    if (sessionId == null) return;
    openLiveDashboard(sessionId);
  }

  function loadPaymentEvents(sourceKey: string): void {
    paymentEvents = [];
    paymentEventsSummary = null;
    paymentEventsSourceLabel = "";
    paymentEventsError = "";
    if (!sourceKey) return;
    try {
      const raw = localStorage.getItem(paymentEventsStorageKey(sourceKey));
      if (!raw) return;
      const saved = JSON.parse(raw) as {
        events?: PaymentEvent[];
        summary?: PaymentEventsSummary;
        sourceLabel?: string;
      };
      if (!Array.isArray(saved.events)) return;
      paymentEvents = saved.events;
      paymentEventsSummary = saved.summary ?? null;
      paymentEventsSourceLabel = saved.sourceLabel || "";
    } catch (error: any) {
      paymentEventsError = error?.message || String(error);
    }
  }

  function savePaymentEvents(sourceKey: string): void {
    if (!sourceKey || !paymentEvents.length) return;
    try {
      localStorage.setItem(paymentEventsStorageKey(sourceKey), JSON.stringify({
        events: paymentEvents,
        summary: paymentEventsSummary,
        sourceLabel: paymentEventsSourceLabel,
        updatedAt: new Date().toISOString(),
      }));
    } catch (error: any) {
      paymentEventsError = `成交订单已加载，但本地保存失败：${error?.message || String(error)}`;
    }
  }

  function clearPaymentEvents(): void {
    paymentEvents = [];
    paymentEventsSummary = null;
    paymentEventsSourceLabel = "";
    paymentEventsError = "";
    if (currentSourceKey) {
      try {
        localStorage.removeItem(paymentEventsStorageKey(currentSourceKey));
      } catch {
        // ignore storage failures
      }
    }
  }

  type FetchDoudianPaymentEventsResult = {
    summary?: unknown;
    events?: unknown;
    stats?: unknown;
    fetchedAt?: string;
    sourceLabel?: string;
  };

  function applyPaymentEventsBundle(bundle: ReturnType<typeof parsePaymentEventsPayload>): void {
    if (!bundle || !bundle.events.length) {
      paymentEventsError = "没有可用的成交订单";
      return;
    }
    paymentEvents = bundle.events;
    paymentEventsSummary = bundle.summary ?? null;
    paymentEventsSourceLabel = bundle.sourceLabel || "order.searchList";
    if (currentSourceKey) savePaymentEvents(currentSourceKey);
    stage = `已加载 ${bundle.events.length} 笔成交订单，可与片段时间轴对齐`;
  }

  async function fetchPaymentEventsFromApi(): Promise<void> {
    if (!archive || archive.platform !== "douyin") {
      paymentEventsError = "仅抖音录播支持一键拉取成交订单";
      return;
    }
    paymentEventsLoading = true;
    paymentEventsError = "";
    try {
      const result = await invoke<FetchDoudianPaymentEventsResult>("fetch_doudian_payment_events", {
        roomId: String(archive.room_id),
        liveId: String(archive.live_id),
        liveStartedAt: liveDashboardBinding?.session?.startedAt ?? null,
        liveEndedAt: null,
      });
      applyPaymentEventsBundle(parsePaymentEventsPayload({
        events: result.events,
        summary: result.summary,
        source_label: result.sourceLabel || "order.searchList",
      }));
    } catch (error: any) {
      paymentEventsError = error?.message || String(error);
    } finally {
      paymentEventsLoading = false;
    }
  }

  function openPaymentEventsPicker(): void {
    paymentEventsInput?.click();
  }

  async function handlePaymentEventsSelected(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    paymentEventsError = "";
    try {
      const raw = JSON.parse(await file.text());
      const bundle = parsePaymentEventsPayload(raw);
      if (!bundle || !bundle.events.length) {
        paymentEventsError = "文件里没有可用的 pay_time / offset_sec 订单事件";
        return;
      }
      paymentEventsSourceLabel = bundle.sourceLabel || file.name;
      applyPaymentEventsBundle(bundle);
    } catch (error: any) {
      paymentEventsError = error?.message || String(error);
    }
  }

  function openDealRefinementPicker(): void {
    if (analysisPurpose !== "enterprise_review") return;
    dealRefinementInput?.click();
  }

  async function handleDealRefinementSelected(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !masterBaseline || !currentSource) return;
    dealRefinementLoading = true;
    paymentEventsError = "";
    try {
      const result = await invoke<{ imported: number; skippedWeak: number; skippedInvalid: number; errors: string[] }>(
        "import_deal_refinement_candidates",
        {
          request: {
            scriptKey: masterBaseline.master.scriptKey,
            expectedMasterScriptId: masterBaseline.master.id,
            source: currentSource,
            payloadJson: await file.text(),
          },
        },
      );
      stage = `订单锚定候选：已送入对比 ${result.imported} 条；弱证据跳过 ${result.skippedWeak} 条；无效 ${result.skippedInvalid} 条。仍需人工审批。`;
      if (result.errors.length) paymentEventsError = result.errors.join("；");
    } catch (error: any) {
      paymentEventsError = error?.message || String(error);
    } finally {
      dealRefinementLoading = false;
    }
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

  const candidateTypeAliases: Record<string, CandidateType> = {
      "成交": "成交收口",
      "成交片段": "成交收口",
      "疑似成交": "关键话术片段",
      "疑似成交片段": "关键话术片段",
      "成交链路": "完整成交链路",
      "完整链路": "完整成交链路",
      "关键话术": "关键话术片段",
      "问价未成交": "问价未见成交信号",
      "问价未见成交": "问价未见成交信号",
      "转品片段": "转品/上链接片段",
      "上链接片段": "转品/上链接片段",
      "转品/上链接": "转品/上链接片段",
      "讲得散": "讲得散片段",
  };

  function normalizeCandidateType(value: unknown): CandidateType | string {
    const text = String(value || "").trim();
    return candidateTypeAliases[text] || text;
  }

  function candidateDisplayType(candidate: Candidate): string {
    if (isStructuredDiscoveryCandidate(candidate)) {
      if (candidate.verificationStatus === "checking") return `${candidate.type} · 正在核验`;
      if (candidate.verificationStatus === "failed") return `${candidate.type} · 待重试`;
      if (candidate.verificationStatus === "verified_complete") return "完整成交链路 · 已核验";
      return candidate.type;
    }
    if (candidate.verificationStatus === "verified_complete") return "完整成交链路，已核验";
    if (candidate.verificationStatus === "partial") return "局部话术素材";
    if (candidate.verificationStatus === "closing") return "成交收口证据";
    if (candidate.verificationStatus === "checking") return "正在查找前后对话";
    if (candidate.verificationStatus === "failed") return "自动核验未完成";
    return "成交线索";
  }

  function candidateStatusDetail(candidate: Candidate): string {
    if (isStructuredDiscoveryCandidate(candidate) && candidate.whySelected) {
      return candidate.interrupted
        ? `${candidate.whySelected} 这段话受到过打断，使用时请结合前后文。`
        : candidate.whySelected;
    }
    if (candidate.verificationStatus === "verified_complete") {
      return "系统确认：这是一段从客户需求到确认成交的完整对话，可以对照企业标准话术。";
    }
    if (candidate.verificationStatus === "checking") return "系统正在自动回查前后字幕，无需人工补写。";
    if (candidate.verificationStatus === "closing") {
      return "这段只有下单或成交收尾，可保留作成交记录，但不是完整话术示范。";
    }
    const missing = missingSalesChainStages(candidate.chainStages);
    if (candidate.verificationStatus === "partial" && missing.length) {
      return `已找到：${candidate.chainStages.join("、") || "局部对话"}；还缺：${missing.join("、")}。`;
    }
    if (candidate.verificationStatus === "failed") {
      return "系统暂时没有查完前后对话，先不做评分；再次打开会自动重试。";
    }
    return "打开后系统会自动查找前后对话，再判断这是不是完整成交过程。";
  }

  function candidateDisposition(candidate: Candidate): string {
    if (candidate.verificationStatus === "verified_complete") return "可以对照企业标准话术，系统将给出可复用分。";
    if (candidate.verificationStatus === "closing") return "保留为成交记录，不进入候选辅稿。";
    if (candidate.verificationStatus === "partial") return "保留为局部话术素材，可用于训练，但不进入候选辅稿。";
    if (candidate.verificationStatus === "failed") return "暂不处理；再次打开该片段后，系统会自动重试。";
    return "等待系统自动查找前后对话，暂不进入评分。";
  }

  function extractJsonObject(content: string): Record<string, unknown> | null {
    const cleaned = content.trim().replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/, "");
    const start = cleaned.indexOf("{");
    const end = cleaned.lastIndexOf("}");
    if (start < 0 || end <= start) return null;
    try {
      const parsed = JSON.parse(cleaned.slice(start, end + 1));
      return parsed && typeof parsed === "object" && !Array.isArray(parsed)
        ? parsed as Record<string, unknown>
        : null;
    } catch {
      return null;
    }
  }

  function candidateContextTranscript(candidate: Candidate): string {
    const { start, end } = interruptedChainContextWindow(candidate.start, candidate.end);
    return transcriptEntries
      .filter((entry) => entry.end >= start && entry.start <= end)
      .map((entry) => entry.raw)
      .join("\n\n");
  }

  function parseCandidateContextVerification(content: string, candidate: Candidate): Candidate | null {
    const parsed = extractJsonObject(content);
    if (!parsed) return null;
    const start = Number(parsed.start);
    const end = Number(parsed.end);
    const type = normalizeCandidateType(parsed.type);
    if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start || !allowedTypes.has(type)) return null;
    const transcriptEnd = transcriptEntries.length ? transcriptEntries[transcriptEntries.length - 1].end : end;
    const safeStart = Math.max(0, start);
    const safeEnd = Math.min(end, transcriptEnd);
    if (safeEnd <= safeStart) return null;
    return applyCandidateContextVerification(candidate, {
      start: safeStart,
      end: safeEnd,
      type,
      product: String(parsed.product || candidate.product),
      evidence: String(parsed.evidence || candidate.evidence),
      reason: String(parsed.reason || candidate.reason),
      verify: String(parsed.verify || candidate.verify),
      signals: parsed.signals,
      chainStages: parsed.chainStages,
      hook: String(parsed.hook || candidate.hook),
      takeaway: String(parsed.takeaway || candidate.takeaway),
    });
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
    const fallbackItems = groups.map((group) => {
      const first = group.entries[0];
      const last = group.entries[group.entries.length - 1];
      const evidence = group.entries.map((entry) => entry.text).join("；");
      const type: CandidateType = group.kind === "成交"
        ? "成交收口"
        : group.kind === "链接"
          ? "转品/上链接片段"
          : "问价未见成交信号";
      return {
        start: Math.max(0, first.start - 45),
        end: Math.min(transcriptEnd, last.end + 25),
        type,
        confidence: group.kind === "成交" ? "中" : "低",
        tier: "候选高光" as const,
        score: group.kind === "成交" ? 55 : 48,
        product: "主商品待确认",
        evidence,
        reason: `大模型未返回可用候选，系统依据逐字稿中的明确${group.kind}动作保留该区间供人工复核；该片段不等于完整成交链路`,
        verify: group.kind === "成交" ? "需核验订单时间、SKU及成交对象" : "需核验画面商品、链接编号及后续成交信号",
        signals: [group.kind === "成交" ? "成交措辞" : group.kind === "链接" ? "链接动作" : "问价回应"],
        hook: "",
        takeaway: "规则兜底发现，需人工复核后再进入母稿",
      };
    });
    return normalizeCandidates(fallbackItems);
  }

  function resetState(preservePayments = false): void {
    sourceRevision += 1;
    transcriptRevision += 1;
    initializeRequestSequence += 1;
    transcriptRequestSequence += 1;
    auditRequestSequence += 1;
    discoveryRequestSequence += 1;
    reviewRequestSequence += 1;
    reviewRequestToken += 1;
    candidateGeneration += 1;
    contextVerificationSequence += 1;
    transcript = "";
    transcriptEntries = [];
    candidates = [];
    reviews = {};
    selectedCandidateId = "";
    selectedCandidate = null;
    selectedReview = null;
    discoveryCompleted = false;
    errorMessage = "";
    playbackLoadError = "";
    transcriptLoadError = "";
    transcriptRefreshNotice = "";
    isTranscribing = false;
    isDiscovering = false;
    reviewingId = "";
    copiedAction = "";
    playerUrl = "";
    void disposeShakaPlayer();
    void stopNativePlayer();
    videoPlayerUrl = "";
    playbackFileKey = "";
    playbackMode = "native";
    nativePlayerActive = false;
    playbackSource = null;
    isPreparingPlayback = false;
    playbackDecodeRetried = false;
    rawPlaybackFailed = false;
    playbackObserverSequence += 1;
    highlightFilter = "全部";
    reviewTab = "analysis";
    workspaceTab = "analysis";
    companyWorkspaceTab = "deal_speech";
    selectedDealOffsetSec = null;
    dealAutoClipping = false;
    dealAutoClipProgress = "";
    dealAutoClipError = "";
    dealAutoClipSequence += 1;
    dealSpeechRefineCache = {};
    dealSpeechRefineFailures = {};
    dealSpeechRefineSequence += 1;
    dealSpeechRefining = false;
    dealSpeechRefineProgress = "";
    dealSpeechRefineError = "";
    dealSpeechRefineKey = "";
    activeDealSpeechRange = null;
    auditBundle = null;
    auditLoadStatus = "idle";
    auditError = "";
    selectedCorrectionId = "";
    masterComparison = null;
    masterComparisons = {};
    selectedMasterSectionId = 0;
    masterComparisonLoading = false;
    masterComparisonError = "";
    masterMatchStatus = "idle";
    masterComparisonSequence += 1;
    isSessionReviewing = false;
    sessionReviewCompleted = 0;
    sessionReviewAbort = false;
    sessionReviewTargetId = "";
    liveDashboardBinding = null;
    liveDashboardBindingLoading = false;
    liveDashboardBindingError = "";
    selectedLiveDashboardSessionId = "";
    if (!preservePayments) {
      paymentEvents = [];
      paymentEventsSummary = null;
      paymentEventsSourceLabel = "";
      paymentEventsError = "";
      paymentEventsLoading = false;
    }
    isScriptQualityAnalyzing = false;
    scriptQualityError = "";
    scriptQualitySummary = "";
    scriptQualityAnnotations = [];
    selectedScriptCueId = null;
    scriptQualityRequestSequence += 1;
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
      // Version 5 removed the old per-type candidate cap. Version 6 adds
      // context-verification state; older candidates are safely restored as
      // unverified and will be checked when opened.
      if ((saved.version !== 5 && saved.version !== 6) || !saved.transcript) return false;
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
            /^\s*\{\s*"(?:verdict|summary)"\s*:/.test(savedReview.beginner?.summary || "") ||
            (savedReview.spokenScript || "").startsWith("模型本次未返回独立口播字段")
          );
          return [candidateId, needsRepair
            ? parseReview(savedReview.raw)
            : { ...savedReview, beginner: normalizeBeginnerReview(savedReview.beginner, savedReview.review) }];
        })
      );
      masterComparisons = Object.fromEntries(
        Object.entries(saved.masterComparisons || {}).filter(([candidateId]) => {
          const candidate = candidates.find((item) => item.id === candidateId);
          return Boolean(candidate && isStructuredDiscoveryCandidate(candidate));
        }),
      );
      selectedCandidateId = saved.selectedCandidateId || candidates[0]?.id || "";
      selectedCandidate = candidates.find((candidate) => candidate.id === selectedCandidateId) || null;
      selectedReview = selectedCandidate ? reviews[selectedCandidate.id] || null : null;
      discoveryCompleted = Boolean(saved.discoveryCompleted);
      stage = `已恢复 ${new Date(saved.updatedAt).toLocaleString("zh-CN")} 保存的分析`;
      if (selectedCandidateId && !isCompanyFullSessionPlayback()) {
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
      version: 6,
      transcript,
      candidates,
      reviews,
      masterComparisons,
      selectedCandidateId,
      discoveryCompleted,
      updatedAt: new Date().toISOString(),
      sourceTitle: sourceTitle(archive, video),
      diagnosis: sessionDiagnosis,
    };
    try {
      localStorage.setItem(storageKey(requestIdentity.sourceKey), JSON.stringify(value));
    } catch (error) {
      errorMessage = `分析已完成，但本地保存失败：${error}`;
    }
  }

  function loadSavedScriptQuality(sourceKey: string): void {
    const saved = parseSavedScriptQuality(localStorage.getItem(scriptQualityStorageKey(sourceKey)));
    if (!saved) return;
    scriptQualitySummary = saved.summary;
    scriptQualityAnnotations = saved.annotations;
    selectedScriptCueId = saved.annotations[0]?.cueId ?? null;
  }

  function saveScriptQuality(sourceKey: string): void {
    try {
      localStorage.setItem(scriptQualityStorageKey(sourceKey), JSON.stringify({
        summary: scriptQualitySummary,
        annotations: scriptQualityAnnotations,
      }));
    } catch (error) {
      scriptQualityError = `复盘已完成，但本地保存失败：${error}`;
    }
  }

  async function initialize(): Promise<void> {
    loadedSourceKey = currentSourceKey;
    loadedRefreshToken = refreshToken;
    if (currentSourceKey) loadPaymentEvents(currentSourceKey);
    resetState(true);
    void loadLiveDashboardBinding(archive, video);
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestId = ++initializeRequestSequence;
    const selectedVideo = video;
    stage = "正在加载录播复盘数据…";
    if (selectedVideo) {
      void loadVideoPlaybackSource(selectedVideo, requestIdentity, requestId);
    } else if (isCompanyFullSessionPlayback() && archive) {
      loadFullArchivePlayer();
    }
    analysisMode === "company_deal" ? void loadActiveMaster() : await loadActiveMaster();
    if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
    const restored = loadSaved(activeAnalysisRequestIdentity());
    if (currentSourceKey) loadSavedScriptQuality(currentSourceKey);
    if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
    const finishPostTranscriptWork = async () => {
      if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
      if (selectedCandidate?.verificationStatus === "unverified") {
        await selectCandidate(selectedCandidate);
      } else if (selectedCandidate && !masterComparison && !masterComparisonLoading) {
        await autoCompareCandidateToMaster(selectedCandidate);
      }
    };
    if (restored && transcript.trim()) {
      stage = `已恢复 ${transcriptEntries.length} 条逐字稿，可继续查看视频与成交数据`;
      void loadTranscriptReview().then(async () => {
        if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
        if (analysisMode !== "company_deal" && currentDiscoveryAction() === "auto" && !discoveryCompleted) {
          await discoverCandidates(currentSourceKey, "auto");
        }
        await finishPostTranscriptWork();
      });
    } else {
      void smartRefreshTranscript(restored).then(finishPostTranscriptWork);
    }
  }

  function isTransportStream(file: string): boolean {
    // A single large TS cannot safely be treated as one HLS segment: Shaka
    // starts reading the entire recording and freezes the desktop webview.
    // This path is reserved for real, segmented HLS manifests. Raw TS keeps
    // the normal decode-fallback path until backend HLS segmentation is ready.
    return /\.m3u8(?:$|[?#])/i.test(file);
  }

  function isRawTransportStream(file: string): boolean {
    return /\.(?:ts|m2ts|mts)(?:$|[?#])/i.test(file);
  }

  function nativePlayerBounds(): { x: number; y: number; width: number; height: number } {
    // CSS pixels relative to the webview viewport. Rust maps them through
    // ClientToScreen + the window scale factor so ffplay covers this host.
    const rect = nativePlayerHost?.getBoundingClientRect();
    return {
      x: Math.round(rect?.left || 0),
      y: Math.round(rect?.top || 0),
      width: Math.max(1, Math.round(rect?.width || 1)),
      height: Math.max(1, Math.round(rect?.height || 1)),
    };
  }

  async function waitForNativePlayerHost(): Promise<boolean> {
    // The host is created by a keyed Svelte branch. Requiring several stable
    // animation frames made the native path race with that first render and
    // incorrectly report "host not ready" after a visible one-frame flash.
    for (let attempt = 0; attempt < 60; attempt += 1) {
      await tick();
      const bounds = nativePlayerBounds();
      if (nativePlayerHost && bounds.width > 8 && bounds.height > 8) {
        return true;
      }
      await new Promise((resolve) => window.requestAnimationFrame(() => resolve()));
    }
    return false;
  }

  async function syncNativePlayerBounds(): Promise<void> {
    if (!video || !nativePlayerActive || nativePlayerTransitioning) return;
    if (!(await waitForNativePlayerHost())) return;
    await invoke("resize_native_video_playback", {
      id: video.id,
      bounds: nativePlayerBounds(),
    });
  }

  async function stopNativePlayer(): Promise<void> {
    if (!video) return;
    const wasActive = nativePlayerActive;
    nativePlayerActive = false;
    nativePlayerPaused = false;
    nativePlayerExpanded = false;
    if (!wasActive) return;
    try {
      await invoke("stop_native_video_playback", { id: video.id });
    } catch {
      // Closing analysis must not be blocked by a player-process cleanup error.
    }
  }

  async function startNativePlayer(offsetSec = 0): Promise<void> {
    if (!video) return;
    nativePlayerTransitioning = true;
    try {
      nativePlayerActive = true;
      nativePlayerPaused = false;
      nativePlaybackAnchorSec = Math.max(0, offsetSec);
      nativePlaybackPositionSec = nativePlaybackAnchorSec;
      nativePlaybackStartedAt = Date.now();
      playbackMode = "native";
      videoPlayerUrl = "";
      if (!(await waitForNativePlayerHost())) {
        nativePlayerActive = false;
        throw new Error("播放区域尚未就绪，无法启动原生 TS 播放器。");
      }
      await invoke("start_native_video_playback", {
        id: video.id,
        offsetSec,
        bounds: nativePlayerBounds(),
      });
      // FFplay/SDL can apply an initial window size after process startup.
      // Correct that one layout pass without restarting the decoder.
      window.setTimeout(() => void syncNativePlayerBounds(), 180);
    } catch (error) {
      nativePlayerActive = false;
      throw error;
    } finally {
      nativePlayerTransitioning = false;
    }
  }

  async function pauseNativePlayer(): Promise<void> {
    if (!video || !nativePlayerActive || nativePlayerPaused || nativePlayerTransitioning) return;
    nativePlayerTransitioning = true;
    nativePlaybackPositionSec = Math.max(0, nativePlaybackAnchorSec + (Date.now() - nativePlaybackStartedAt) / 1000);
    nativePlaybackAnchorSec = nativePlaybackPositionSec;
    nativePlaybackStartedAt = 0;
    nativePlayerPaused = true;
    try {
      await invoke("stop_native_video_playback", { id: video.id });
    } finally {
      nativePlayerTransitioning = false;
    }
  }

  async function seekNativePlayer(offsetSec: number): Promise<void> {
    await startNativePlayer(Math.max(0, offsetSec));
  }

  function handleNativeProgressChange(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    if (!nativePlayerTransitioning) void seekNativePlayer(Number(input.value));
  }

  async function toggleNativePlayerExpanded(): Promise<void> {
    nativePlayerExpanded = !nativePlayerExpanded;
    await tick();
    await syncNativePlayerBounds();
  }

  async function disposeShakaPlayer(): Promise<void> {
    const player = shakaPlayer;
    shakaPlayer = null;
    if (player) {
      try {
        await player.destroy();
      } catch {
        // The player may already be torn down by the webview.
      }
    }
    if (hlsPlaylistUrl) URL.revokeObjectURL(hlsPlaylistUrl);
    hlsPlaylistUrl = "";
  }

  async function requireEmbeddedPlaybackCopy(): Promise<void> {
    await disposeShakaPlayer();
    await stopNativePlayer();
    videoPlayerUrl = "";
    playbackMode = "native";
    rawPlaybackFailed = true;
    playbackSource = playbackSource
      ? {
          ...playbackSource,
          requiresPreparation: true,
          ready: false,
          preparing: false,
          message: "原始 TS 过大，已跳过会阻塞界面的直接解封装；请生成项目内可播放版本。",
        }
      : playbackSource;
  }

  function makeSingleFileHlsPlaylist(mediaUrl: string): string {
    // Shaka demuxes the original TS inside MediaSource.  No MP4 is produced;
    // the manifest merely describes the one imported transport-stream file.
    return [
      "#EXTM3U",
      "#EXT-X-VERSION:3",
      "#EXT-X-PLAYLIST-TYPE:VOD",
      "#EXT-X-TARGETDURATION:86400",
      "#EXTINF:86400,",
      mediaUrl,
      "#EXT-X-ENDLIST",
      "",
    ].join("\n");
  }

  async function mountTransportStreamPlayer(mediaUrl: string, segmentedManifest = false): Promise<void> {
    await disposeShakaPlayer();
    hlsPlaylistUrl = segmentedManifest
      ? mediaUrl
      : URL.createObjectURL(new Blob(
        [makeSingleFileHlsPlaylist(mediaUrl)],
        { type: "application/vnd.apple.mpegurl" },
      ));
    playbackMode = "hls";
    videoPlayerUrl = "";
    await tick();
    if (!videoElement) throw new Error("播放器元素未就绪。");
    if (typeof shaka === "undefined") {
      throw new Error("项目内置播放器加载失败，请刷新页面后重试。");
    }
    if (!shaka.Player.isBrowserSupported()) {
      throw new Error("当前系统 WebView 不支持内嵌 TS 播放。");
    }
    const player = new shaka.Player(videoElement);
    shakaPlayer = player;
    player.addEventListener("error", () => void handleVideoElementError());
    await player.load(hlsPlaylistUrl);
  }

  function updateEmbeddedPlaybackPosition(): void {
    if (!embeddedPreviewActive || !videoElement) return;
    const absolutePosition = embeddedPreviewStartOffset + Math.max(0, videoElement.currentTime || 0);
    embeddedPlaybackPositionSec = fullPlaybackDurationSec
      ? Math.min(fullPlaybackDurationSec, absolutePosition)
      : absolutePosition;
  }

  function handleEmbeddedSeekChange(event: Event): void {
    const target = Number((event.currentTarget as HTMLInputElement).value);
    if (!Number.isFinite(target) || isPreparingPlayback) return;
    embeddedPlaybackPositionSec = Math.max(0, target);
    void startEmbeddedTransportPreview(embeddedPlaybackPositionSec);
  }

  function toggleEmbeddedPlayback(): void {
    if (!videoElement || isPreparingPlayback) return;
    if (videoElement.paused) void videoElement.play().catch(() => undefined);
    else videoElement.pause();
  }

  function handleEmbeddedPlaybackEnded(): void {
    updateEmbeddedPlaybackPosition();
    if (!embeddedPreviewActive || isPreparingPlayback) return;
    const nextOffset = Math.max(
      embeddedPlaybackPositionSec,
      embeddedPreviewStartOffset + Math.max(0, videoElement?.duration || 0),
    );
    if (fullPlaybackDurationSec && nextOffset >= fullPlaybackDurationSec - 0.5) {
      embeddedPlaybackPlaying = false;
      return;
    }
    void startEmbeddedTransportPreview(nextOffset);
  }

  async function toggleEmbeddedFullscreen(): Promise<void> {
    if (!embeddedPlayerShell) return;
    if (document.fullscreenElement === embeddedPlayerShell) {
      await document.exitFullscreen();
      return;
    }
    await embeddedPlayerShell.requestFullscreen();
  }

  async function startEmbeddedTransportPreview(offsetSec = 0): Promise<void> {
    const selectedVideo = video;
    if (!selectedVideo) return;
    const requestId = ++embeddedPreviewRequest;
    embeddedPreviewActive = true;
    isPreparingPlayback = true;
    playbackLoadError = "";
    // Force the player branch to be rendered before Shaka binds its video node.
    playbackMode = "hls";
    videoPlayerUrl = "";
    try {
      const preview = await invoke<VideoPlaybackPreview>("prepare_video_playback_preview", {
        id: selectedVideo.id,
        offsetSec: Math.max(0, offsetSec),
      });
      if (requestId !== embeddedPreviewRequest || !video || video.id !== selectedVideo.id) return;
      playbackFileKey = preview.file;
      embeddedPreviewStartOffset = preview.startOffset;
      embeddedPlaybackPositionSec = preview.startOffset;
      // The preview is already browser-compatible. Mark it ready before
      // mounting Shaka so the keyed `<video>` element exists on the next tick.
      playbackSource = {
        file: preview.file,
        requiresPreparation: false,
        ready: true,
        preparing: false,
        message: preview.message,
      };
      isPreparingPlayback = false;
      const previewUrl = await get_static_url("output", preview.file);
      await mountTransportStreamPlayer(previewUrl, true);
      if (requestId !== embeddedPreviewRequest) return;
      rawPlaybackFailed = false;
      playbackLoadError = "";
      void videoElement?.play().catch(() => undefined);
    } catch (error: any) {
      if (requestId !== embeddedPreviewRequest) return;
      embeddedPreviewActive = false;
      isPreparingPlayback = false;
      rawPlaybackFailed = true;
      playbackLoadError = `内嵌 TS 播放流启动失败：${error?.message || String(error)}`;
    }
  }

  function retryAnalysisPlayback(): void {
    if (!video) return;
    if (isRawTransportStream(video.file)) {
      const currentOffset = embeddedPreviewStartOffset + (videoElement?.currentTime || 0);
      void startEmbeddedTransportPreview(Math.max(0, currentOffset));
      return;
    }
    void prepareVideoForPlayback({ force: true });
  }

  async function setVideoPlayerSource(file: string): Promise<void> {
    if (!file) {
      await disposeShakaPlayer();
      videoPlayerUrl = "";
      playbackFileKey = "";
      playbackMode = "native";
      return;
    }
    const nextMode = isTransportStream(file) ? "hls" : "native";
    if (
      playbackFileKey === file
      && (
        (nextMode === "hls" && shakaPlayer)
        || (nextMode === "native" && !isRawTransportStream(file) && videoPlayerUrl)
      )
    ) {
      return;
    }
    playbackFileKey = file;
    const mediaUrl = await get_static_url("output", file);
    if (isRawTransportStream(file)) {
      // Never create an ffplay window here. Windows child/owned windows cannot
      // be composited reliably above a WebView2 surface: they detach from the
      // project on resize and may duplicate or turn black in fullscreen.
      // Keep playback in the application WebView and let the backend provide
      // the compatible embedded source instead.
      await disposeShakaPlayer();
      await stopNativePlayer();
      await startEmbeddedTransportPreview(0);
      return;
    }
    embeddedPreviewActive = false;
    if (video) {
      await invoke("stop_video_playback_preview", { id: video.id }).catch(() => undefined);
    }
    if (nextMode === "hls") {
      // `.m3u8` is already a real backend-generated manifest. Do not wrap it
      // again as a fake single-file playlist, otherwise Shaka receives a
      // playlist as though it were a TS segment and fails to seek/play.
      await mountTransportStreamPlayer(mediaUrl, true);
      return;
    }
    await disposeShakaPlayer();
    await stopNativePlayer();
    playbackMode = "native";
    videoPlayerUrl = mediaUrl;
  }

  async function loadVideoPlaybackSource(
    selectedVideo: VideoItem,
    requestIdentity: AnalysisRequestIdentity,
    requestId: number,
  ): Promise<void> {
    try {
      const nextPlaybackSource = await invoke<VideoPlaybackSource>("get_video_playback_source", { id: selectedVideo.id });
      if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
      playbackSource = nextPlaybackSource;
      // Clear the old failed-player state before the raw TS path toggles the
      // native host. Otherwise Svelte keeps the retry placeholder mounted and
      // there is no element for the native decoder to attach to.
      rawPlaybackFailed = false;
      playbackLoadError = "";
      // `file` points to the original recording until a fallback copy exists.
      // Try it immediately: supported TS/HLS sources should be analyzable
      // without waiting for a whole-session MP4 conversion.
      await setVideoPlayerSource(nextPlaybackSource.file);
      if (nextPlaybackSource.preparing && !isRawTransportStream(nextPlaybackSource.file)) {
        void observeVideoPlaybackUntilReady(selectedVideo.id);
      }
    } catch (error: any) {
      if (!isCurrentInitializationRequest(requestIdentity, requestId)) return;
      playbackLoadError = `播放器暂不可用：${error?.message || String(error)}`;
    }
  }

  async function loadActiveMaster(): Promise<void> {
    masterBaseline = null;
    try {
      const raw = localStorage.getItem("bsr:active-master");
      const active = raw ? JSON.parse(raw) as { scriptKey?: string } : null;
      if (!active?.scriptKey) return;
      const batchId = /^MS-BATCH-(\d+)$/.exec(active.scriptKey)?.[1];
      if (batchId) {
        const batches = await listMasterSampleBatches();
        const batch = batches.find((item) => item.batch.id === Number(batchId));
        if (!batch || !canActivateEnterpriseMaster(batch.batch.purpose)) {
          localStorage.removeItem("bsr:active-master");
          return;
        }
      }
      masterBaseline = await getMasterBaseline(active.scriptKey);
    } catch {
      masterBaseline = null;
    }
  }

  async function compareCandidateToMaster(
    candidate: Candidate,
    sectionId: number,
    background = false,
  ): Promise<void> {
    const source = currentSource;
    const baseline = masterBaseline;
    const section = baseline?.sections.find((item) => item.id === sectionId);
    if (!source || !baseline || !section) return;
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestGeneration = candidateGeneration;
    const requestId = background ? 0 : ++masterComparisonSequence;
    if (!background) {
      selectedMasterSectionId = sectionId;
      masterComparison = null;
      masterComparisonError = "";
      masterComparisonLoading = true;
    }
    const requestedCandidateId = candidate.id;
    try {
      const result = await compareHighlightToMaster({
        scriptKey: baseline.master.scriptKey,
        expectedMasterScriptId: baseline.master.id,
        masterSectionId: section.id,
        source,
        sourceStartMs: Math.round(candidate.start * 1000),
        sourceEndMs: Math.round(candidate.end * 1000),
        candidateType: candidate.type,
        chainStages: candidate.chainStages,
        candidateSegment: {
          segmentId: candidate.id,
          segmentType: candidate.type,
          scene: candidate.scene,
          customerNeed: candidate.customerNeed,
          originalText: candidate.originalText,
          keySentence: candidate.keySentence,
          outcome: candidate.outcome,
          interrupted: candidate.interrupted,
          whySelected: candidate.whySelected,
        },
        productCardId: section.productCardId,
        sectionKind: section.sectionKind,
        comparisonMode: analysisPurpose === "competitor_benchmark"
          ? "competitor_benchmark"
          : "enterprise_upgrade",
        competitorName: analysisPurpose === "competitor_benchmark"
          ? analysisProfile.competitorName || null
          : null,
      });
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)
        || requestGeneration !== candidateGeneration
        || !candidates.some((item) => item.id === requestedCandidateId)
      ) return;
      if (!background && (
        requestId !== masterComparisonSequence
        || !isCurrentMasterComparison(
          requestedCandidateId,
          requestGeneration,
          selectedCandidateId,
          candidateGeneration,
        )
      )) return;
      masterComparisons = { ...masterComparisons, [candidate.id]: result };
      if (selectedCandidateId === candidate.id) {
        masterComparison = result;
        selectedMasterSectionId = result.comparison.masterSectionId || sectionId;
        masterMatchStatus = result.comparison.masterSectionId ? "matched" : "unmatched";
      }
      saveState();
    } catch (reason: any) {
      if (!background && requestId === masterComparisonSequence) {
        masterComparisonError = reason?.message || String(reason);
      }
    } finally {
      if (!background && requestId === masterComparisonSequence) masterComparisonLoading = false;
    }
  }

  async function compareSelectedToMaster(sectionId: number): Promise<void> {
    if (!selectedCandidate) return;
    await compareCandidateToMaster(selectedCandidate, sectionId);
  }

  async function autoCompareCandidateToMaster(
    candidate: Candidate,
    force = false,
    background = false,
  ): Promise<void> {
    const baseline = masterBaseline;
    if (!baseline) {
      if (!background) masterMatchStatus = "idle";
      return;
    }
    const existing = masterComparisons[candidate.id];
    if (existing && !force) {
      if (!background && selectedCandidateId === candidate.id) {
        masterComparison = existing;
        selectedMasterSectionId = existing.comparison.masterSectionId || 0;
        masterMatchStatus = existing.comparison.masterSectionId ? "matched" : "unmatched";
      }
      return;
    }
    if (!isStructuredDiscoveryCandidate(candidate)) {
      if (!background) {
        masterMatchStatus = "idle";
        selectedMasterSectionId = 0;
        masterComparison = null;
        masterComparisonLoading = false;
        masterComparisonError = candidate.verificationStatus === "failed"
          ? "暂不能评分：系统尚未完成前后对话核验，请重新发现候选片段。"
          : "暂不能评分：这条是旧版候选数据，缺少原话、入选理由或时间证据，请重新发现候选片段。";
      }
      return;
    }
    const productMatch = autoMatchMasterSection(candidate.product, baseline.sections);
    const match = productMatch.status === "unmatched"
      ? autoMatchMasterScene(candidate, baseline.sections)
      : productMatch;
    if (!background && selectedCandidateId === candidate.id) masterMatchStatus = match.status;
    if (match.sectionId === null) {
      if (!background && selectedCandidateId === candidate.id) {
        selectedMasterSectionId = 0;
        masterComparison = null;
        masterComparisonLoading = false;
        masterComparisonError = match.status === "ambiguous"
          ? "识别到多个相似商品章节，本片段已暂停评分，请先在逐字稿中确认商品名称。"
          : "未能识别本片段对应的母稿商品，本片段不会进入候选辅稿。";
      }
      return;
    }
    await compareCandidateToMaster(candidate, match.sectionId, background);
  }

  function handleUpgradeReviewRetried(event: CustomEvent<{
    reviewId: number;
    upgradeReview: MasterComparisonResult["upgradeReview"];
  }>): void {
    if (!masterComparison || !selectedCandidate || !event.detail.upgradeReview) return;
    masterComparison = {
      ...masterComparison,
      upgradeReviewId: event.detail.reviewId,
      upgradeReview: event.detail.upgradeReview,
      upgradeReviewError: null,
    };
    masterComparisons = {
      ...masterComparisons,
      [selectedCandidate.id]: masterComparison,
    };
    saveState();
  }

  async function runFullSessionReview(): Promise<void> {
    if (!masterBaseline || !currentSource || !reusableCandidates.length || isSessionReviewing) return;
    if (reviewingId) {
      errorMessage = "当前片段仍在生成复盘，请等待完成或点击“取消当前复盘”后再开始整场复盘。";
      return;
    }
    const sessionGate = highlightReviewGate(
      currentHighlightWorkflowStage(),
      discoveryCompleted,
      isDiscovering,
    );
    if (!sessionGate.allowed) {
      errorMessage = sessionGate.message;
      return;
    }
    const requestGeneration = candidateGeneration;
    isSessionReviewing = true;
    sessionReviewAbort = false;
    sessionReviewCompleted = reusableCandidates.filter((candidate) =>
      Boolean(reviews[candidate.id]) && Boolean(masterComparisons[candidate.id])
    ).length;
    sessionReviewTargetId = "";
    workspaceTab = "analysis";
    try {
      for (const candidate of reusableCandidates) {
        if (!sessionReviewShouldContinue(sessionReviewAbort, requestGeneration, candidateGeneration)) break;
        const workPlan = sessionCandidateWorkPlan(
          Boolean(reviews[candidate.id]),
          Boolean(masterComparisons[candidate.id]),
        );
        if (!workPlan.length) continue;
        sessionReviewTargetId = candidate.id;
        for (const work of workPlan) {
          if (!sessionReviewShouldContinue(sessionReviewAbort, requestGeneration, candidateGeneration)) break;
          if (work === "novice_review") {
            const completed = await reviewSelectedCandidate(false, candidate);
            if (!completed) {
              stage = `${candidate.id} 小白复盘未完成，整场复盘已暂停，可稍后继续`;
              sessionReviewAbort = true;
              break;
            }
          } else {
            await autoCompareCandidateToMaster(candidate, true, true);
          }
        }
        if (!sessionReviewShouldContinue(sessionReviewAbort, requestGeneration, candidateGeneration)) break;
        sessionReviewCompleted += 1;
      }
    } finally {
      sessionReviewTargetId = "";
      isSessionReviewing = false;
      sessionReviewAbort = false;
      saveState();
    }
  }

  function cancelFullSessionReview(): void {
    if (!isSessionReviewing) return;
    sessionReviewAbort = true;
    stage = `正在取消整场复盘，已完成 ${sessionReviewCompleted}/${reusableCandidates.length} 段`;
  }

  function cancelCurrentReview(showNotice = true): void {
    if (!reviewingId) return;
    reviewRequestSequence += 1;
    reviewingId = "";
    if (showNotice) stage = "已取消当前片段复盘，可以继续查看或选择其他片段";
  }

  function waitForTaskPoll(): Promise<void> {
    return new Promise((resolve) => window.setTimeout(resolve, TASK_POLL_INTERVAL_MS));
  }

  async function refreshVideoPlaybackSource(videoId: number): Promise<VideoPlaybackSource> {
    const source = await invoke<VideoPlaybackSource>("get_video_playback_source", { id: videoId });
    if (!video || video.id !== videoId) return source;
    playbackSource = source;
    await setVideoPlayerSource(source.file);
    rawPlaybackFailed = false;
    return source;
  }

  async function observeVideoPlaybackUntilReady(videoId: number): Promise<void> {
    const observerId = ++playbackObserverSequence;
    isPreparingPlayback = true;
    playbackLoadError = "";
    try {
      for (let attempt = 0; attempt < TASK_POLL_MAX_ATTEMPTS; attempt += 1) {
        if (observerId !== playbackObserverSequence) return;
        if (!video || video.id !== videoId) return;
        const source = await invoke<VideoPlaybackSource>("get_video_playback_source", { id: videoId });
        if (observerId !== playbackObserverSequence) return;
        if (
          playbackSource?.ready !== source.ready
          || playbackSource?.preparing !== source.preparing
          || playbackSource?.file !== source.file
          || playbackSource?.message !== source.message
        ) {
          playbackSource = source;
        }
        if (source.ready) {
          await setVideoPlayerSource(source.file);
          playbackLoadError = "";
          playbackDecodeRetried = false;
          rawPlaybackFailed = false;
          return;
        }
        if (source.requiresPreparation && !source.preparing) {
          playbackLoadError = source.message || "可播放版本任务未能继续，请点重试。";
          return;
        }
        await waitForTaskPoll();
      }
      playbackLoadError = "可播放版本仍在后台处理；请稍后点播放器重试，或在任务页查看进度。";
    } catch (error: any) {
      if (observerId !== playbackObserverSequence) return;
      playbackLoadError = error?.message || String(error);
    } finally {
      if (observerId === playbackObserverSequence && video?.id === videoId) {
        isPreparingPlayback = false;
      }
    }
  }

  async function handleVideoElementError(): Promise<void> {
    if (!video || isPreparingPlayback || playbackSource?.preparing) return;
    if (embeddedPreviewActive) {
      // Do not turn one failed short preview into an hours-long whole-recording
      // MP4 conversion. The user can retry the embedded stream, while the
      // original TS and system responsiveness remain intact.
      playbackLoadError = "内嵌 TS 预览流加载失败，请点重试后重新打开。";
      return;
    }
    if (playbackDecodeRetried) {
      playbackLoadError = "浏览器无法播放当前视频。请在任务页查看转码进度，或点重试。";
      return;
    }
    playbackDecodeRetried = true;
    playbackLoadError = "";
    rawPlaybackFailed = true;
    await disposeShakaPlayer();
    videoPlayerUrl = "";
    playbackFileKey = "";
    playbackMode = "native";
    playbackSource = playbackSource
      ? {
          ...playbackSource,
          requiresPreparation: true,
          ready: false,
          preparing: false,
          message: "检测到浏览器无法解码，请点击生成可播放版本。",
        }
      : playbackSource;
  }

  async function prepareVideoForPlayback(options: { force?: boolean } = {}): Promise<void> {
    const selectedVideo = video;
    if (!selectedVideo) {
      playbackLoadError = "当前没有可转换的视频。";
      return;
    }
    if (isPreparingPlayback) {
      playbackLoadError = playbackLoadError || "可播放版本正在生成中，请稍候或到任务页查看进度。";
      return;
    }
    // Analysis starts a non-forced background preparation automatically. Manual
    // retries can still explicitly replace a stuck or incomplete result.
    const forceConversion = options.force ?? false;
    playbackObserverSequence += 1;
    isPreparingPlayback = true;
    playbackLoadError = "";
    // Show preparing UI immediately; otherwise a slow/hanging invoke looks like a dead button.
    playbackSource = {
      file: playbackSource?.file || selectedVideo.file,
      requiresPreparation: true,
      ready: false,
      preparing: true,
      message: "正在排队生成可播放版本…",
    };
    try {
      const source = await invoke<VideoPlaybackSource>("prepare_video_playback", {
        eventId: `analysis_playback_${selectedVideo.id}_${Date.now()}`,
        id: selectedVideo.id,
        force: forceConversion,
      });
      if (!video || video.id !== selectedVideo.id) {
        isPreparingPlayback = false;
        return;
      }
      playbackSource = source;
      if (source.ready) {
        isPreparingPlayback = false;
        rawPlaybackFailed = false;
        await setVideoPlayerSource(source.file);
        playbackDecodeRetried = false;
        return;
      }
      if (source.requiresPreparation || source.preparing) {
        void observeVideoPlaybackUntilReady(selectedVideo.id);
        return;
      }
      isPreparingPlayback = false;
      playbackSource = {
        ...source,
        requiresPreparation: true,
        ready: false,
        preparing: false,
        message: source.message || "无法开始生成可播放版本，请重试。",
      };
      playbackLoadError = playbackSource.message;
    } catch (error: any) {
      isPreparingPlayback = false;
      if (!video || video.id !== selectedVideo.id) return;
      playbackLoadError = error?.message || String(error);
      if (playbackSource?.requiresPreparation && !playbackSource.ready) {
        playbackSource = { ...playbackSource, preparing: false };
      }
    }
  }

  async function resumeOrRefreshArchiveTranscript(
    selectedArchive: RecordItem,
    forceTranscriptRefresh: boolean,
  ): Promise<TranscriptRefreshResult> {
    const platform = selectedArchive.platform;
    const roomId = String(selectedArchive.room_id);
    const liveId = String(selectedArchive.live_id);
    const tasks = await invoke<BackgroundTask[]>("get_tasks");
    const activeTask = findActiveArchiveSubtitleTask(tasks, platform, roomId, liveId);
    if (activeTask) {
      stage = forceTranscriptRefresh
        ? "已有转写任务进行中，等待完成后再重新识别…"
        : activeTask.message.trim()
          ? `正在继续转写：${activeTask.message}`
          : "正在继续转写逐字稿…";
    } else if (forceTranscriptRefresh) {
      stage = "正在重新识别并核对新旧逐字稿…";
    } else {
      stage = "正在生成录播逐字稿…";
    }

    return invoke<TranscriptRefreshResult>("refresh_archive_subtitle", {
      platform,
      roomId,
      liveId,
      force: forceTranscriptRefresh,
    });
  }

  async function waitForVideoTranscriptTask(videoId: number): Promise<string> {
    for (let attempt = 0; attempt < TASK_POLL_MAX_ATTEMPTS; attempt += 1) {
      try {
        const subtitle = await invoke<string>("get_video_subtitle", { id: videoId });
        if (subtitle.trim()) return subtitle;
      } catch {
        // A transcript has not been written yet. Keep following the active task.
      }

      const tasks = await invoke<BackgroundTask[]>("get_tasks");
      const task = findActiveVideoSubtitleTask(tasks, videoId);
      if (!task) {
        throw new Error("逐字稿任务已经结束，但没有生成可用文稿。请点击重新识别后再试。");
      }
      stage = task.message.trim()
        ? `正在继续转写：${task.message}`
        : "正在继续转写逐字稿…";
      await waitForTaskPoll();
    }
    throw new Error("逐字稿仍在后台处理；已停止本页等待。请稍后刷新或在任务页查看进度。");
  }

  async function resumeOrGenerateVideoTranscript(videoId: number): Promise<string> {
    const tasks = await invoke<BackgroundTask[]>("get_tasks");
    const activeTask = findActiveVideoSubtitleTask(tasks, videoId);
    if (activeTask) {
      stage = activeTask.message.trim()
        ? `正在继续转写：${activeTask.message}`
        : "正在继续转写逐字稿…";
      return waitForVideoTranscriptTask(videoId);
    }

    try {
      return await invoke<string>("generate_video_subtitle", {
        eventId: `analysis_video_${videoId}_${Date.now()}`,
        id: videoId,
      });
    } catch (error: any) {
      if (!String(error?.message || error).includes("已有逐字稿任务")) throw error;
      return waitForVideoTranscriptTask(videoId);
    }
  }

  async function smartRefreshTranscript(hasSavedAnalysis: boolean, forceTranscriptRefresh = false): Promise<void> {
    if (!currentSource || isTranscribing) return;
    invalidateTranscriptBoundRequests();
    const requestId = ++transcriptRequestSequence;
    let requestIdentity = activeAnalysisRequestIdentity();
    const selectedArchive = archive;
    const selectedVideo = video;
    isTranscribing = true;
    transcriptLoadError = "";
    try {
      const previousTranscript = transcript;
      let nextTranscript = "";
      let transcriptChanged = false;
      let nextTranscriptRefreshNotice = "";
      if (selectedArchive) {
        let existingSubtitle = "";
        const platform = selectedArchive.platform;
        const roomId = String(selectedArchive.room_id);
        const liveId = String(selectedArchive.live_id);
        const tasks = await invoke<BackgroundTask[]>("get_tasks");
        const activeArchiveTask = findActiveArchiveSubtitleTask(tasks, platform, roomId, liveId);
        if (!forceTranscriptRefresh) {
          stage = activeArchiveTask
            ? activeArchiveTask.message.trim()
              ? `正在继续转写：${activeArchiveTask.message}`
              : "正在继续转写逐字稿…"
            : "正在读取录播逐字稿…";
          if (!activeArchiveTask) {
            try {
              existingSubtitle = await invoke<string>("get_archive_subtitle", {
                platform,
                roomId,
                liveId,
              });
            } catch {
              existingSubtitle = "";
            }
          }
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
        }

        if (!activeArchiveTask && selectArchiveTranscriptAction(forceTranscriptRefresh, existingSubtitle) === "use-existing") {
          nextTranscript = existingSubtitle;
          transcriptChanged = Boolean(previousTranscript && previousTranscript !== nextTranscript);
          nextTranscriptRefreshNotice = "已读取录播逐字稿";
        } else {
          stage = forceTranscriptRefresh ? "正在重新识别并核对新旧逐字稿…" : "正在生成录播逐字稿…";
          const result = await resumeOrRefreshArchiveTranscript(selectedArchive, forceTranscriptRefresh);
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
          nextTranscript = result.subtitle;
          transcriptChanged = Boolean(previousTranscript && previousTranscript !== nextTranscript);
          nextTranscriptRefreshNotice = result.decision === "resumed"
            ? "已接续转写任务"
            : result.decision === "kept"
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
          nextTranscript = await resumeOrGenerateVideoTranscript(selectedVideo.id);
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, transcriptRequestSequence)) return;
          nextTranscriptRefreshNotice = forceTranscriptRefresh ? "已重新识别" : "已恢复逐字稿任务";
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
      transcriptLoadError = selectedArchive
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
    if (isCompanyFullSessionPlayback()) {
      seekFullArchivePlayback(target.seekSeconds);
    } else if (archive) {
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
    auditBundle = bundle;
    auditLoadStatus = bundle.corrections.length ? "loaded" : "legacy-empty";
    if (bundle.correctedSrt.trim()) {
      const transcriptChanged = transcriptReviewChangesTranscript(transcript, bundle.correctedSrt);
      if (transcriptChanged) {
        invalidateTranscriptBoundRequests();
        transcript = bundle.correctedSrt;
        transcriptEntries = parseSrt(transcript);
        candidates = [];
        reviews = {};
        masterComparisons = {};
        masterComparison = null;
        selectedCandidateId = "";
        discoveryCompleted = false;
        saveState(activeAnalysisRequestIdentity());
      }
    }
  }

  async function completeTranscriptReview(event: CustomEvent<TranscriptReviewUpdateEvent>): Promise<void> {
    updateTranscriptReview(event);
    const { bundle, requestIdentity, reviewRequestToken: eventRequestToken } = event.detail;
    if (isTranscribing || isDiscovering || bundle.pendingCriticalCount > 0) return;
    if (eventRequestToken !== reviewRequestToken) return;
    if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)) return;
    const requestSourceKey = analysisSourceKey(bundle.source);
    if (requestSourceKey !== currentSourceKey || currentDiscoveryAction(requestSourceKey) !== "auto") return;

    workspaceTab = "analysis";
    await discoverCandidates(requestSourceKey, "auto");
  }

  async function regenerateAndAnalyze(): Promise<void> {
    if (isTranscribing || isDiscovering) return;
    await smartRefreshTranscript(Boolean(transcript && discoveryCompleted), true);
  }

  function startDiscoveryFromHeader(): void {
    const action = currentDiscoveryAction();
    if (action === "blocked") return;
    workspaceTab = "analysis";
    void discoverCandidates(currentSourceKey, action);
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
      let productDictionary = "";
      try {
        productDictionary = await invoke<string>("get_enterprise_product_dictionary");
      } catch {
        // A disconnected knowledge base must not prevent transcript analysis.
      }
      const discoveryPrompt = analysisPurpose === "competitor_benchmark"
        ? buildCompetitorDiscoveryPrompt(productDictionary)
        : buildCandidateDiscoveryPrompt(productDictionary);
      for (let index = 0; index < chunks.length; index += 1) {
        if (
          !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
          || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
        ) return;
        stage = `片段发现器正在分析文稿 ${index + 1}/${chunks.length}…`;
        const response = await invoke<string>("minimax_chat", {
          systemPrompt: discoveryPrompt,
          messages: [{ role: "user", content: chunks[index] }],
        });
        if (
          !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
          || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
        ) return;
        const parsed = parseCandidateDiscoveryResponse(response).map((candidate) =>
          analysisPurpose === "competitor_benchmark"
            ? { ...candidate, outcome: competitorDiscoveryOutcome(candidate.outcome) }
            : candidate,
        );
        found.push(...parsed);
      }
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, discoveryRequestSequence)
        || !isCurrentCandidateGeneration(requestCandidateGeneration, candidateGeneration)
      ) return;
      const modelCandidates = selectDiscoveryCandidates(mergeSalesCandidates(found));
      const usedFallback = modelCandidates.length === 0;
      reviews = {};
      masterComparisons = {};
      masterComparison = null;
      candidates = usedFallback
        ? selectDiscoveryCandidates(mergeSalesCandidates(buildEvidenceFallback(transcriptEntries)))
        : modelCandidates;
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
        void selectCandidate(candidates[0]);
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

  async function verifyCandidateContext(candidate: Candidate): Promise<Candidate> {
    if (candidate.verificationStatus !== "unverified" && candidate.verificationStatus !== "failed") return candidate;
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestId = ++contextVerificationSequence;
    const checking = { ...candidate, verificationStatus: "checking" as CandidateVerificationStatus };
    candidates = candidates.map((item) => item.id === candidate.id ? checking : item);
    stage = `${candidate.id} 正在自动查找前后相关对话`;
    saveState(requestIdentity);
    try {
      const response = await invoke<string>("minimax_chat", {
        systemPrompt: "你是电商直播成交链路核验器。只返回一个合法 JSON 对象，不要 Markdown。字段必须为 start、end、type、product、evidence、reason、verify、signals、chainStages、hook、takeaway。你会收到候选前后最多5分钟字幕。请只根据字幕核验同一轮客户对话，并把 start、end 扩展到实际开始和结束的位置；时间必须是相对整场录播开始的秒数。同一商品、同一用户或同一需求在5分钟内恢复时，允许拼接被其他用户短暂打断的内容，并在reason说明被打断及恢复依据；切换商品、无法对应用户或形成新需求时禁止拼接。type 只能为：完整成交链路、关键话术片段、成交收口。完整成交链路必须按实际出现顺序覆盖全部七步：客户需求/疑问、产品匹配、卖点或价值说明、风险消除/售后承诺、价格/链接/优惠、引导下单、确认成交。缺少任何一步时，必须标为关键话术片段；只有内容仅为下单、备注、恭喜、发货等成交收尾时，才标为成交收口。chainStages 只能填写字幕中真实出现的环节。不得补写主播原话，不得编造价格、链接、库存或订单。",
        messages: [{
          role: "user",
          content: `候选编号：${candidate.id}\n原候选时间：${formatTime(candidate.start)}—${formatTime(candidate.end)}\n原候选商品：${candidate.product}\n\n前后字幕：\n${candidateContextTranscript(candidate)}`,
        }],
      });
      if (
        requestId !== contextVerificationSequence
        || !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)
      ) return candidate;
      const verified = parseCandidateContextVerification(response, checking);
      if (!verified) throw new Error("模型没有返回可用的上下文核验结果");
      candidates = candidates.map((item) => item.id === candidate.id ? verified : item);
      const { [candidate.id]: _review, ...remainingReviews } = reviews;
      const { [candidate.id]: _comparison, ...remainingComparisons } = masterComparisons;
      reviews = remainingReviews;
      masterComparisons = remainingComparisons;
      stage = verified.verificationStatus === "verified_complete"
        ? `${candidate.id} 已确认完整成交链路`
        : `${candidate.id} 已核验为${candidateDisplayType(verified)}`;
      saveState(requestIdentity);
      return verified;
    } catch (error: any) {
      if (
        requestId !== contextVerificationSequence
        || !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), 0, 0)
      ) return candidate;
      const failed = { ...candidate, verificationStatus: "failed" as CandidateVerificationStatus };
      candidates = candidates.map((item) => item.id === candidate.id ? failed : item);
      stage = `${candidate.id} 自动回查未完成`;
      errorMessage = `片段自动回查失败：${error?.message || String(error)}`;
      saveState(requestIdentity);
      return failed;
    }
  }

  function isCompanyFullSessionPlayback(): boolean {
    return analysisMode === "company_deal";
  }

  function loadFullArchivePlayer(): void {
    if (!archive || video) return;
    playerNonce += 1;
    const params = new URLSearchParams({
      platform: archive.platform,
      room_id: String(archive.room_id),
      live_id: String(archive.live_id),
      start: "0",
      end: "0",
      nonce: String(playerNonce),
      embed: "1",
    });
    playerUrl = `index_live.html?${params.toString()}`;
  }

  function seekFullArchivePlayback(offsetSec: number): void {
    const safeOffset = Math.max(0, Math.floor(offsetSec));
    if (video && embeddedPreviewActive) {
      void startEmbeddedTransportPreview(safeOffset);
      return;
    }
    if (video && nativePlayerActive) {
      void startNativePlayer(safeOffset).catch((error) => {
        playbackLoadError = error?.message || String(error);
      });
      return;
    }
    if (video && videoElement) {
      videoElement.currentTime = safeOffset;
      void videoElement.play().catch(() => undefined);
      return;
    }
    if (!archive) return;
    const iframe = document.querySelector(".company-player-shell iframe") as HTMLIFrameElement | null;
    iframe?.contentWindow?.postMessage({ type: "bsr:embed-seek", offsetSec: safeOffset }, "*");
  }

  function updatePlayerAndTranscript(scroll = true, candidateOverride: Candidate | null = null): void {
    if (isCompanyFullSessionPlayback()) return;
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
    if (isCompanyFullSessionPlayback()) {
      seekFullArchivePlayback(entry.start);
      return;
    }
    if (video && embeddedPreviewActive) {
      void startEmbeddedTransportPreview(entry.start);
      return;
    }
    if (video && nativePlayerActive) {
      void startNativePlayer(entry.start).catch((error) => {
        playbackLoadError = error?.message || String(error);
      });
      return;
    }
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

  $: canDealAutoClip = Boolean(
    analysisMode === "company_deal"
      && video
      && video.status !== -1
      && paymentEvents.length
      && transcriptEntries.length
      && !isTranscribing,
  );

  function seekToDealPeak(): void {
    if (!topDealPeak) return;
    seekToDealOffset(topDealPeak.offsetSec);
  }

  function seekToDealOffset(offsetSec: number): void {
    const entry = transcriptEntries.find((item) => item.end >= dealReviewSeekOffset(offsetSec));
    if (entry) seekToTranscriptEntry(entry);
  }

  function handleCompanyWorkspaceSeek(startSec: number): void {
    const entry =
      transcriptEntries.find((item) => item.start === startSec)
      ?? transcriptEntries.find((item) => item.start <= startSec && item.end >= startSec);
    if (entry) seekToTranscriptEntry(entry);
  }

  function handleCompanyDealSelect(offsetSec: number): void {
    selectedDealOffsetSec = offsetSec;
    const cached = lookupDealSpeechRefine(offsetSec);
    if (cached) {
      activeDealSpeechRange = cached;
      handleCompanyWorkspaceSeek(cached.start);
    } else {
      activeDealSpeechRange = null;
      seekToDealOffset(offsetSec);
    }
    void ensureDealSpeechRefined(offsetSec);
  }

  function handleCompanyDealSeek(offsetSec: number): void {
    selectedDealOffsetSec = offsetSec;
    handleCompanyWorkspaceSeek(offsetSec);
    void ensureDealSpeechRefined(offsetSec);
  }

  function lookupDealSpeechRefine(offsetSec: number | null): DealClipRange | null {
    if (offsetSec == null || !paymentEvents.length || !transcriptEntries.length) return null;
    const contexts = buildDealClipContexts(paymentEvents, transcriptEntries)
      .filter((context) => context.transcriptLines.length > 0);
    const context = selectDealClipContext(contexts, offsetSec);
    if (!context) return null;
    return dealSpeechRefineCache[dealClipContextCacheKey(context)] ?? null;
  }

  function syncActiveDealSpeechRange(offsetSec: number | null = selectedDealOffsetSec): void {
    activeDealSpeechRange = lookupDealSpeechRefine(offsetSec);
  }

  async function ensureDealSpeechRefined(offsetSec: number | null, force = false): Promise<void> {
    if (analysisMode !== "company_deal" || offsetSec == null) return;
    if (!paymentEvents.length || !transcriptEntries.length || isTranscribing) return;

    const contexts = buildDealClipContexts(paymentEvents, transcriptEntries)
      .filter((context) => context.transcriptLines.length > 0);
    const context = selectDealClipContext(contexts, offsetSec);
    if (!context) {
      dealSpeechRefineError = "订单时间附近没有可用逐字稿，无法定位成交链路";
      dealSpeechRefineProgress = "";
      activeDealSpeechRange = null;
      return;
    }

    const cacheKey = dealClipContextCacheKey(context);
    const cached = force ? null : dealSpeechRefineCache[cacheKey] ?? null;
    if (cached) {
      dealSpeechRefineKey = cacheKey;
      dealSpeechRefineError = "";
      dealSpeechRefineProgress = "";
      activeDealSpeechRange = cached;
      return;
    }

    if (!force && dealSpeechRefineFailures[cacheKey]) {
      dealSpeechRefineKey = cacheKey;
      dealSpeechRefineError = dealSpeechRefineFailures[cacheKey];
      dealSpeechRefineProgress = "";
      if (selectedDealOffsetSec === offsetSec) {
        syncActiveDealSpeechRange(offsetSec);
      }
      return;
    }

    if (dealSpeechRefining && dealSpeechRefineKey === cacheKey && !force) return;

    const requestIdentity = activeAnalysisRequestIdentity();
    const requestId = ++dealSpeechRefineSequence;
    dealSpeechRefining = true;
    dealSpeechRefineKey = cacheKey;
    dealSpeechRefineError = "";
    dealSpeechRefineProgress = `AI 正在定位「${context.productNames[0] || "该商品"}」成交链路…`;
    if (force) {
      const { [cacheKey]: _removed, ...restFailures } = dealSpeechRefineFailures;
      dealSpeechRefineFailures = restFailures;
    }
    try {
      const response = await invoke<string>("minimax_chat", {
        systemPrompt: DEAL_CLIP_SYSTEM_PROMPT,
        messages: [{ role: "user", content: buildDealClipUserPrompt(context) }],
      });
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealSpeechRefineSequence)
      ) {
        return;
      }
      const range = finalizeDealClipFromAi(context, response);
      if (!range) {
        throw new Error("无法得到有效起止边界");
      }
      dealSpeechRefineCache = { ...dealSpeechRefineCache, [cacheKey]: range };
      const { [cacheKey]: _cleared, ...restFailures } = dealSpeechRefineFailures;
      dealSpeechRefineFailures = restFailures;
      dealSpeechRefineProgress = range.reason
        ? `已定位：${range.title || "成交链路"}（${range.reason}）`
        : `已定位：${range.title || "成交链路"}`;
      dealSpeechRefineError = "";
      if (selectedDealOffsetSec === offsetSec) {
        activeDealSpeechRange = range;
        handleCompanyWorkspaceSeek(range.start);
      }
    } catch (error: any) {
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealSpeechRefineSequence)
      ) {
        return;
      }
      const message = error?.message || String(error);
      dealSpeechRefineFailures = { ...dealSpeechRefineFailures, [cacheKey]: message };
      dealSpeechRefineError = message;
      dealSpeechRefineProgress = "";
      if (selectedDealOffsetSec === offsetSec) {
        syncActiveDealSpeechRange(offsetSec);
      }
    } finally {
      if (
        isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealSpeechRefineSequence)
      ) {
        dealSpeechRefining = false;
      }
    }
  }

  function retryDealSpeechRefine(): void {
    if (selectedDealOffsetSec == null) return;
    void ensureDealSpeechRefined(selectedDealOffsetSec, true);
  }

  async function waitForDealAutoClipTask(taskId: string): Promise<BackgroundTask> {
    const maxAttempts = 400;
    for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
      const tasks = await invoke<BackgroundTask[]>("get_tasks");
      const task = tasks.find((candidate) => candidate.id === taskId);
      if (!task) throw new Error("自动切片任务记录不存在，请到任务页检查。");
      if (task.status === "success") return task;
      if (["failed", "cancelled", "interrupted"].includes(task.status)) {
        throw new Error(task.message?.trim() || `自动切片任务已${task.status}`);
      }
      dealAutoClipProgress = task.message?.trim() || "正在生成完整成交链路视频…";
      stage = dealAutoClipProgress;
      await waitForTaskPoll();
    }
    throw new Error("自动切片仍在后台处理，请到任务页查看进度。");
  }

  async function runDealOrderAutoClip(): Promise<void> {
    if (!canDealAutoClip || dealAutoClipping || !video) return;
    const selectedVideo = video;
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestId = ++dealAutoClipSequence;
    dealAutoClipping = true;
    dealAutoClipError = "";
    dealAutoClipProgress = "正在按商品订单簇追溯完整成交链路…";
    stage = "AI 正在定位完整成交链路…";
    const errors: string[] = [];
    try {
      const allContexts = buildDealClipContexts(paymentEvents, transcriptEntries)
        .filter((context) => context.transcriptLines.length > 0);
      const selectedContext = selectDealClipContext(allContexts, selectedDealOffsetSec);
      const contexts = selectedContext ? [selectedContext] : [];
      if (!contexts.length) {
        throw new Error("订单时间附近没有可用逐字稿，无法追溯完整成交链路");
      }

      const draftRanges: DealClipRange[] = [];
      for (let index = 0; index < contexts.length; index += 1) {
        if (
          !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealAutoClipSequence)
          || !video
          || video.id !== selectedVideo.id
        ) {
          return;
        }
        const context = contexts[index];
        dealAutoClipProgress = `Minimax 分析完整成交链路 ${index + 1}/${contexts.length}（同商品 ${context.orderCount} 单）…`;
        stage = dealAutoClipProgress;
        try {
          const response = await invoke<string>("minimax_chat", {
            systemPrompt: DEAL_CLIP_SYSTEM_PROMPT,
            messages: [{ role: "user", content: buildDealClipUserPrompt(context) }],
          });
          const range = finalizeDealClipFromAi(context, response);
          if (!range) {
            errors.push(`链路 ${index + 1}: 无法得到有效起止边界`);
            continue;
          }
          draftRanges.push(range);
          const cacheKey = dealClipContextCacheKey(context);
          dealSpeechRefineCache = { ...dealSpeechRefineCache, [cacheKey]: range };
          if (selectedDealOffsetSec != null) syncActiveDealSpeechRange(selectedDealOffsetSec);
        } catch (error: any) {
          errors.push(`链路 ${index + 1}: ${error?.message || String(error)}`);
        }
      }

      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealAutoClipSequence)
        || !video
        || video.id !== selectedVideo.id
      ) {
        return;
      }

      const ranges = mergeDealClipRanges(draftRanges);
      if (!ranges.length) {
        throw new Error(errors.length ? `全部成交链路分析失败：${errors.join("；")}` : "没有可导出的完整成交链路");
      }

      dealAutoClipProgress = `正在提交 ${ranges.length} 条完整成交链路视频到后台队列…`;
      stage = dealAutoClipProgress;
      const batchEventId = `deal_auto_clip_batch_${selectedVideo.id}_${Date.now()}`;
      const queued = await invoke<{ taskId: string; count: number }>("queue_deal_auto_clips", {
        eventId: batchEventId,
        parentVideoId: selectedVideo.id,
        ranges: ranges.map((range) => ({
          start: range.start,
          end: range.end,
          title: range.title,
        })),
      });

      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealAutoClipSequence)
      ) {
        return;
      }

      dealAutoClipProgress = `已提交 ${queued.count} 条完整成交链路视频，正在等待生成完成…`;
      dealAutoClipError = "";
      stage = dealAutoClipProgress;
      const completedTask = await waitForDealAutoClipTask(queued.taskId);
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealAutoClipSequence)
        || !video
        || video.id !== selectedVideo.id
      ) {
        return;
      }
      const generatedVideos = await invoke<VideoItem[]>("get_all_videos");
      const reviewRequest = buildClipReviewRequest({
        taskId: queued.taskId,
        parentVideoId: selectedVideo.id,
        ranges,
        transcriptEntries,
        videos: generatedVideos,
        taskMetadata: completedTask.metadata,
      });
      if (!reviewRequest) {
        throw new Error("切片已经生成，但未找到对应的视频结果。请在切片列表中刷新查看。");
      }
      await Promise.allSettled(reviewRequest.items.map((item) => invoke("update_video_subtitle", {
        id: item.video.id,
        subtitle: clipTranscriptToSrt(item.transcriptEntries),
      })));
      const summary = errors.length
        ? `切片完成；另有 ${errors.length} 条成交链路未获得有效 AI 边界。`
        : "切片完成，正在进入成交切片复盘页…";
      dealAutoClipProgress = summary;
      stage = summary;
      dealAutoClipping = false;
      window.dispatchEvent(new CustomEvent("bsr:open-clip-review", { detail: reviewRequest }));
    } catch (error: any) {
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealAutoClipSequence)
      ) {
        return;
      }
      dealAutoClipError = error?.message || String(error);
      dealAutoClipProgress = "";
      stage = "自动切片失败";
    } finally {
      if (
        isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, dealAutoClipSequence)
      ) {
        dealAutoClipping = false;
      }
    }
  }

  async function runFullSessionScriptQualityReview(): Promise<void> {
    if (!transcriptEntries.length || isScriptQualityAnalyzing || isTranscribing) return;
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestId = ++scriptQualityRequestSequence;
    isScriptQualityAnalyzing = true;
    scriptQualityError = "";
    scriptQualitySummary = "";
    scriptQualityAnnotations = [];
    selectedScriptCueId = null;
    companyWorkspaceTab = "ai_review";
    const entriesById = new Map(transcriptEntries.map((entry) => [entry.id, entry]));
    const systemPrompt = scriptQualitySystemPrompt();
    const chunks = splitTranscriptForScriptQuality(transcriptEntries);
    const batches: ScriptIssueAnnotation[][] = [];
    let summaryParts: string[] = [];
    const chunkWarnings: string[] = [];
    try {
      for (const chunk of chunks) {
        if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, scriptQualityRequestSequence)) return;
        stage = `话术复盘 ${chunk.index + 1}/${chunk.total}…`;
        try {
          const response = await invoke<string>("minimax_chat", {
            systemPrompt,
            messages: [{ role: "user", content: buildScriptQualityUserMessage(chunk) }],
          });
          if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, scriptQualityRequestSequence)) return;
          const bundle = parseScriptQualityBundle(response, entriesById);
          if (bundle.summary) summaryParts.push(bundle.summary);
          batches.push(bundle.annotations);
          // Long sessions are chunked. Preserve completed chunks immediately so
          // leaving the page does not discard an otherwise expensive AI run.
          scriptQualityAnnotations = mergeScriptQualityAnnotations(batches);
          scriptQualitySummary = summaryParts.filter(Boolean).slice(0, 2).join(" ");
          if (currentSourceKey) saveScriptQuality(currentSourceKey);
        } catch (chunkError: any) {
          const message = chunkError?.message || String(chunkError);
          chunkWarnings.push(`第 ${chunk.index + 1}/${chunk.total} 块：${message}`);
          batches.push([]);
        }
      }
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, scriptQualityRequestSequence)) return;
      scriptQualityAnnotations = mergeScriptQualityAnnotations(batches);
      scriptQualitySummary = summaryParts.filter(Boolean).slice(0, 2).join(" ");
      if (chunkWarnings.length) {
        scriptQualityError = chunkWarnings.length === chunks.length
          ? chunkWarnings[0]
          : `部分分析块失败（${chunkWarnings.length}/${chunks.length}）：${chunkWarnings[0]}`;
      }
      stage = scriptQualityAnnotations.length
        ? `话术复盘完成，标出 ${scriptQualityAnnotations.length} 处可改进${chunkWarnings.length ? `（${chunkWarnings.length} 块失败）` : ""}`
        : chunkWarnings.length === chunks.length
          ? "话术复盘失败"
          : "话术复盘完成，未发现明显可改进句子";
      if (scriptQualityAnnotations.length) {
        selectedScriptCueId = scriptQualityAnnotations[0].cueId;
      }
      if (currentSourceKey) saveScriptQuality(currentSourceKey);
    } catch (error: any) {
      if (!isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, scriptQualityRequestSequence)) return;
      scriptQualityError = error?.message || String(error);
      stage = "话术复盘失败";
    } finally {
      if (isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, scriptQualityRequestSequence)) {
        isScriptQualityAnalyzing = false;
      }
    }
  }

  $: if (analysisMode === "company_deal" && paymentEvents.length && selectedDealOffsetSec == null) {
    selectedDealOffsetSec = paymentEvents[0]?.offsetSec ?? null;
  }

  $: if (
    analysisMode === "company_deal"
    && selectedDealOffsetSec != null
    && paymentEvents.length
    && transcriptEntries.length
    && !isTranscribing
  ) {
    void ensureDealSpeechRefined(selectedDealOffsetSec);
  }

  async function selectCandidate(candidate: Candidate): Promise<void> {
    if (candidateSelectionCancelsReview(reviewingId, candidate.id)) {
      cancelCurrentReview(false);
    }
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
    saveState();
    const verifiedCandidate = candidateNeedsLegacyContextVerification(candidate)
      ? await verifyCandidateContext(candidate)
      : candidate;
    if (selectedCandidateId !== candidate.id) return;
    selectedCandidate = verifiedCandidate;
    updatePlayerAndTranscript(true, verifiedCandidate);
    await autoCompareCandidateToMaster(verifiedCandidate);
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
            return cleaned
              .slice(valueStart + 1, cursor)
              .replace(/\r?\n/g, "\n")
              .replace(/\\n/g, "\n")
              .replace(/\\"/g, '"');
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
      extractMarkdownSection(reviewText, /话术发展建议|参考改写建议|忠实优化版/) ||
      extractMarkdownSection(reviewText, /话术结构参考/)
    );
    const trainingText = String(parsed?.training_checklist || parsed?.trainingChecklist || extractStringField("training_checklist") || extractStringField("trainingChecklist"));
    return {
      beginner: normalizeBeginnerReview(parsed || {}, cleaned),
      review: reviewText,
      spokenScript: spokenText || "模型本次未返回独立的话术发展建议，请点击“重新复盘”后再试。",
      trainingChecklist: trainingText || "模型本次未返回独立训练清单字段，请点击“重新复盘”后再试。",
      raw: content,
      updatedAt: new Date().toISOString(),
    };
  }

  async function reviewSelectedCandidate(force = false, candidateOverride: Candidate | null = null): Promise<boolean> {
    const candidate = candidateOverride || selectedCandidate;
    const candidateReviewGate = highlightReviewGate(currentHighlightWorkflowStage(), discoveryCompleted, isDiscovering);
    if (!candidate || reviewingId || !candidateReviewGate.allowed) return false;
    if (!force && reviews[candidate.id]) return true;
    const requestIdentity = activeAnalysisRequestIdentity();
    const requestCandidateIdentity = candidateReviewIdentity(candidateGeneration, candidate.id);
    const requestId = ++reviewRequestSequence;
    reviewingId = candidate.id;
    errorMessage = "";
    stage = `复盘官正在分析 ${candidate.id}…`;
    try {
      const clipTranscript = candidateTranscript(candidate);
      const response = await invoke<string>("minimax_chat", {
        systemPrompt: `${COMMERCE_REVIEW_PROMPT}\n\n这份结果主要交给没有直播运营经验的新手阅读。最终只返回一个合法 JSON 对象，不要代码围栏。字段固定为 verdict、summary、good_points、improvements、checks、spoken_script、training_checklist、professional_detail。verdict 只能是“建议保留”“修改后保留”“建议放弃”“证据不足”；summary 用一句大白话说明原因，不超过45个汉字；good_points、improvements、checks 都是1—4条短句数组，每句只说一件事，禁止使用“链路签名、CTA、证据强度、预分类”等术语。spoken_script 只能给参考改写建议，必须明确不是标准稿且不可直接照念；training_checklist 写培训讨论点和具体练习方向；professional_detail 才承载原固定格式的专业内容。JSON 字符串中的换行必须正确转义。`,
        messages: [{
          role: "user",
          content: `请复盘当前候选片段。候选类型只是待核验线索，必须根据逐字稿重新预分类。完整成交链路必须依次覆盖：客户需求/疑问、产品匹配、卖点或价值说明、风险消除/售后承诺、价格/链接/优惠、引导下单、确认成交；缺任一环节则只作局部复盘，不得建议进入母稿或辅稿。缺少订单时间或明确成交确认时，不得写成已成交。\n\n内容：${sourceTitle()}\n候选编号：${candidate.id}\n候选类型：${candidate.type}\n实际链路：${candidate.chainStages.join(" → ") || "未形成完整链路"}\n候选商品：${candidate.product}\n候选时间：${formatTime(candidate.start)}—${formatTime(candidate.end)}\n候选证据：${candidate.evidence}\n待核验：${candidate.verify || "无"}\n\n逐字稿：\n${clipTranscript}`,
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
      ) return false;
      const review = parseReview(response);
      reviews = { ...reviews, [candidate.id]: review };
      if (selectedCandidateId === candidate.id) selectedReview = review;
      stage = `${candidate.id} 复盘完成，结果已自动保存`;
      saveState(requestIdentity);
      return true;
    } catch (error: any) {
      if (
        !isCurrentAnalysisRequest(requestIdentity, activeAnalysisRequestIdentity(), requestId, reviewRequestSequence)
        || !isCurrentCandidateReview(
          requestCandidateIdentity,
          candidateGeneration,
          candidates.map((item) => item.id),
          isDiscovering,
        )
      ) return false;
      errorMessage = error?.message || String(error);
      stage = `${candidate.id} 复盘失败`;
      return false;
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
    if (selectedReview) void copyText(selectedReview.spokenScript, "话术发展建议");
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
      "> 免责声明：以下内容是 AI 生成的话术发展建议，不是企业标准稿，不可直接照念，须经培训负责人审定。",
      "",
      "## 话术发展建议（不可直接照念，须培训负责人审定）",
      selectedReview.spokenScript,
      "## 培训讨论点 / 练习方向",
      selectedReview.trainingChecklist,
    ].join("\n\n");
    void copyText(content, "全部");
  }

  function currentOptimizationPlanMarkdown(): string {
    const title = sourceTitle(archive, video);
    const plan = buildOptimizationPlan({
      sessionTitle: title,
      diagnosedAt: new Date().toLocaleString("zh-CN"),
      masterScriptVersion: masterBaseline?.master.version || "",
      diagnosis: sessionDiagnosis,
      candidates,
      comparisons: masterComparisons,
      reviews,
    });
    return formatOptimizationPlanMarkdown(plan);
  }

  async function exportOptimizationPlan(): Promise<void> {
    const title = sourceTitle(archive, video);
    const content = currentOptimizationPlanMarkdown();
    if (TAURI_ENV) {
      const path = await save({
        title: "导出下场优化计划",
        defaultPath: optimizationPlanFileName(title),
        filters: [{ name: "Markdown 文档", extensions: ["md"] }],
      });
      if (!path) return;
      await invoke("export_to_file", { fileName: path, content });
      stage = `下场优化计划已导出到 ${path}`;
      return;
    }
    const blob = new Blob([content], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = optimizationPlanFileName(title);
    anchor.click();
    URL.revokeObjectURL(url);
  }

  function buildVideoAsMaster(): void {
    if (!video) return;
    window.dispatchEvent(new CustomEvent("bsr:build-master", { detail: video }));
  }
</script>

<div class="analysis-zoom-viewport" on:wheel|nonpassive={handleAnalysisWheel}>
<div class="analysis-shell" style={`--analysis-zoom:${analysisZoom / 100};`}>
  <header class="analysis-header">
    <div class="header-main">
      <button class="icon-button" title={video ? "返回视频列表" : "返回录播"} on:click={() => dispatch("back")}>
        <ArrowLeft size={18} />
      </button>
      <div class="min-w-0">
        <div class="title-row"><h1>典典直播切片</h1><span class="mode-pill">{analysisMode === "company_deal" ? "公司录播分析" : analysisPurpose === "competitor_benchmark" ? "竞品对照模式" : "企业复盘模式"}</span></div>
        <p title={sourceTitle(archive, video)}>{sourceTitle(archive, video)}</p>
        {#if analysisPurpose === "competitor_benchmark"}
          <small>竞品录播无成交峰数据；仅评估可迁移价值，须人工审定后写入案例库。</small>
        {/if}
      </div>
    </div>
    <div class="header-actions">
      <div class="zoom-control" aria-label="片段分析页面缩放">
        <button
          title="缩小片段分析页面"
          aria-label="缩小片段分析页面"
          disabled={analysisZoom <= ANALYSIS_ZOOM_MIN}
          on:click={() => persistAnalysisZoom(stepAnalysisZoom(analysisZoom, -1))}
        ><Minus size={14} /></button>
        <button
          class="zoom-value"
          title="恢复片段分析页面到 100%"
          aria-label="恢复片段分析页面到 100%"
          on:click={() => persistAnalysisZoom(100)}
        >{analysisZoom}%</button>
        <button
          title="放大片段分析页面"
          aria-label="放大片段分析页面"
          disabled={analysisZoom >= ANALYSIS_ZOOM_MAX}
          on:click={() => persistAnalysisZoom(stepAnalysisZoom(analysisZoom, 1))}
        ><Plus size={14} /></button>
      </div>
      <span class="stage" class:error-stage={Boolean(errorMessage)}>{errorMessage || stage}</span>
      {#if video}
        <button class="secondary-button master-button" on:click={buildVideoAsMaster}>
          <BookOpenCheck size={15} />
          设为整场母稿
        </button>
      {/if}
      {#if archive?.platform === "douyin" && liveDashboardBinding?.session}
        <button class="secondary-button" on:click={openBoundLiveDashboard}>
          <BarChart3 size={15} />
          直播大屏
        </button>
      {/if}
      <button class="secondary-button" disabled={!currentSource || isTranscribing || isDiscovering} on:click={regenerateAndAnalyze}>
        <span class:is-spinning={isTranscribing}><RotateCcw size={15} /></span>
        重新识别
      </button>
      {#if analysisMode !== "company_deal"}
      <button
        class="secondary-button"
        disabled={!transcript || isDiscovering || isTranscribing || discoveryAction === "blocked"}
        on:click={startDiscoveryFromHeader}
      >
        <span class:is-spinning={isDiscovering}><RefreshCw size={15} /></span>
        重新发现
      </button>
      {/if}
    </div>
  </header>

  {#if archive?.platform === "douyin" && analysisMode !== "company_deal"}
    <section class="live-dashboard-summary" aria-label="直播数据大屏">
      {#if liveDashboardBindingLoading}
        <span>正在匹配直播数据大屏…</span>
      {:else if liveDashboardBinding?.session}
        <div class="live-dashboard-summary-heading">
          <div>
            <strong>{liveDashboardBinding.session.shopName || "已绑定直播数据大屏"}</strong>
            <span>{liveDashboardBinding.session.accountKey} · {liveDashboardBinding.session.startedAt} · {liveDashboardBinding.session.sourceFile}</span>
          </div>
          <div class="live-dashboard-summary-actions">
            <button type="button" class="live-dashboard-open-button" on:click={openBoundLiveDashboard}>
              <BarChart3 size={14} />
              打开直播大屏
            </button>
            <small>{liveDashboardBinding.matchMethod === "manual" ? "人工绑定" : "自动匹配"}</small>
          </div>
        </div>
        <div class="live-dashboard-metrics">
          {#each dashboardMetricCards(liveDashboardBinding.session) as metric}
            <div class="live-dashboard-metric">
              <span>{metric.label}</span>
              <strong>{metric.value}</strong>
            </div>
          {/each}
        </div>
      {:else if liveDashboardBinding?.candidates?.length}
        <div class="live-dashboard-summary-heading">
          <div>
            <strong>未自动绑定直播数据大屏</strong>
            <span>存在多个或账号不确定的候选场次，请人工确认。</span>
          </div>
        </div>
        <div class="live-dashboard-bind-controls">
          <select bind:value={selectedLiveDashboardSessionId} aria-label="选择直播数据场次">
            <option value="">选择候选场次</option>
            {#each liveDashboardBinding.candidates as candidate}
              <option value={candidate.session.id}>
                {candidate.session.startedAt} · {candidate.session.shopName || candidate.session.accountKey}
              </option>
            {/each}
          </select>
          <button class="secondary-button" disabled={!selectedLiveDashboardSessionId} on:click={bindSelectedLiveDashboard}>
            绑定场次
          </button>
        </div>
      {:else}
        <span>未找到可绑定的官方 XLSX 直播数据。请先在直播数据大屏导入对应场次。</span>
      {/if}
      {#if liveDashboardBindingError}
        <small class="live-dashboard-error">{liveDashboardBindingError}</small>
      {/if}
    </section>

    {#if analysisPurpose === "enterprise_review"}
    <section class="payment-events-summary" aria-label="成交订单时间轴">
      <div class="payment-events-heading">
        <div>
          <strong>成交订单时间轴</strong>
          <span>
            {#if paymentEventsSummary}
              已导入 {paymentEventsSummary.eventCount} 笔 · 合计 ¥{paymentEventsSummary.totalPayAmountYuan.toLocaleString("zh-CN")}
              {#if paymentEventsSourceLabel} · {paymentEventsSourceLabel}{/if}
            {:else}
              导入 `doudian_order_events.py` 生成的 JSON，或点击「一键拉取成交」
            {/if}
          </span>
        </div>
        <div class="payment-events-actions">
          <button
            type="button"
            class="secondary-button"
            disabled={paymentEventsLoading || !archive}
            on:click={fetchPaymentEventsFromApi}
          >
            {#if paymentEventsLoading}
              <Loader2 size={14} class="is-spinning" />
            {:else}
              <RefreshCw size={14} />
            {/if}
            一键拉取成交
          </button>
          <button type="button" class="secondary-button" on:click={openPaymentEventsPicker}>
            <Upload size={14} />
            导入成交 JSON
          </button>
          <button type="button" class="secondary-button" disabled={dealRefinementLoading || !masterBaseline || !currentSource} on:click={openDealRefinementPicker} title="只导入 verified 商品讲解；仍需人工审批">
            <Upload size={14} />
            {dealRefinementLoading ? "导入订单候选中" : "导入订单锚定候选"}
          </button>
          {#if paymentEvents.length}
            <button type="button" class="text-button" on:click={clearPaymentEvents}>清除</button>
          {/if}
        </div>
      </div>
      {#if peakDealMinuteLabel}
        <button type="button" class="payment-events-peak" on:click={seekToDealPeak} title="跳到高峰前两分钟的逐字稿">
          成交高峰：{peakDealMinuteLabel} · 定位候选时段（±2 分钟）
        </button>
      {/if}
      {#if paymentEventsError}
        <small class="live-dashboard-error">{paymentEventsError}</small>
      {/if}
    </section>
    {:else}
    <section class="payment-events-summary" aria-label="竞品对照说明">
      <strong>竞品对照模式</strong>
      <span>无订单、成交峰或 GMV 数据；片段发现只基于逐字稿语言信号。</span>
    </section>
    {/if}
    <input
      bind:this={paymentEventsInput}
      type="file"
      accept="application/json,.json"
      class="hidden-file-input"
      on:change={handlePaymentEventsSelected}
    />
    <input bind:this={dealRefinementInput} type="file" accept="application/json,.json" class="hidden-file-input" on:change={handleDealRefinementSelected} />
  {/if}

  {#if analysisMode === "company_deal" && analysisPurpose === "enterprise_review" && video && !archive}
    <section class="payment-events-summary" aria-label="成交订单时间轴">
      <div class="payment-events-heading">
        <div>
          <strong>成交订单时间轴</strong>
          <span>
            {#if paymentEventsSummary}
              已导入 {paymentEventsSummary.eventCount} 笔 · 合计 ¥{paymentEventsSummary.totalPayAmountYuan.toLocaleString("zh-CN")}
              {#if paymentEventsSourceLabel} · {paymentEventsSourceLabel}{/if}
            {:else}
              导入 `payment-events` JSON，与录播时间对齐后在「成交话术」Tab 查看
            {/if}
          </span>
        </div>
        <div class="payment-events-actions">
          <button type="button" class="secondary-button" on:click={openPaymentEventsPicker}>
            <Upload size={14} />
            导入成交 JSON
          </button>
          {#if paymentEvents.length}
            <button type="button" class="text-button" on:click={clearPaymentEvents}>清除</button>
          {/if}
        </div>
      </div>
      {#if peakDealMinuteLabel}
        <button type="button" class="payment-events-peak" on:click={seekToDealPeak} title="跳到高峰前两分钟的逐字稿">
          成交高峰：{peakDealMinuteLabel} · 定位候选时段（±2 分钟）
        </button>
      {/if}
      {#if paymentEventsError}
        <small class="live-dashboard-error">{paymentEventsError}</small>
      {/if}
    </section>
    <input
      bind:this={paymentEventsInput}
      type="file"
      accept="application/json,.json"
      class="hidden-file-input"
      on:change={handlePaymentEventsSelected}
    />
  {/if}

  {#if analysisMode === "company_deal"}
  <div class="analysis-main">
  <CompanyAnalysisWorkspace
    bind:activeTab={companyWorkspaceTab}
    events={paymentEvents}
    transcriptEntries={transcriptEntries}
    selectedOffsetSec={selectedDealOffsetSec}
    transcriptReady={Boolean(transcript)}
    scriptQualityAnalyzing={isScriptQualityAnalyzing}
    scriptQualityError={scriptQualityError}
    scriptQualitySummary={scriptQualitySummary}
    scriptQualityAnnotations={scriptQualityAnnotations}
    selectedScriptCueId={selectedScriptCueId}
    canAutoClip={canDealAutoClip}
    autoClipping={dealAutoClipping}
    autoClipProgress={dealAutoClipProgress}
    autoClipError={dealAutoClipError}
    refinedSpeechRange={activeDealSpeechRange}
    speechRefining={dealSpeechRefining}
    speechRefineProgress={dealSpeechRefineProgress}
    speechRefineError={dealSpeechRefineError}
    dashboardLoading={liveDashboardBindingLoading}
    dashboardError={liveDashboardBindingError}
    dashboardSession={liveDashboardBinding?.session ?? null}
    dashboardMatchMethod={liveDashboardBinding?.matchMethod ?? null}
    dashboardCandidates={liveDashboardBinding?.candidates ?? []}
    bind:selectedDashboardSessionId={selectedLiveDashboardSessionId}
    orderSummary={null}
    showDashboardBind={showCompanyDashboardBind}
    showDataBoard={showCompanyDataBoard}
    on:seek={(event) => handleCompanyDealSeek(event.detail)}
    on:select={(event) => handleCompanyDealSelect(event.detail)}
    on:analyzeScriptQuality={() => void runFullSessionScriptQualityReview()}
    on:selectScriptCue={(event) => { selectedScriptCueId = event.detail; }}
    on:autoClip={() => void runDealOrderAutoClip()}
    on:retrySpeechRefine={() => retryDealSpeechRefine()}
    on:openDashboard={() => openBoundLiveDashboard()}
    on:bindDashboardSession={() => void bindSelectedLiveDashboard()}
    on:rebindDashboardSession={() => void rebindLiveDashboardSession()}
  >
    <div slot="player" class="company-player-shell">
      {#if playbackLoadError}
        <div class="module-error" role="alert">
          <span>{playbackLoadError}</span>
          {#if video}<button type="button" on:click={retryAnalysisPlayback}>重试</button>{/if}
        </div>
      {/if}
      {#if video && !playbackSource && !playbackLoadError}
        <div class="empty-player">
          <Loader2 size={32} class="is-spinning" />
          <strong>正在加载视频…</strong>
          <span>原始文件保留不变，加载完成后直接播放。</span>
        </div>
      {:else if video && playbackView !== "player"}
        <div class="empty-player playback-preparation">
          {#if playbackView === "preparing" || isPreparingPlayback}
            <Loader2 size={32} class="is-spinning" />
            <strong>正在生成可播放版本</strong>
            <span>{playbackSource?.message || "正在转换为 H.264 MP4…"}</span>
          {:else}
            <Play size={32} />
            <strong>原始视频暂时无法在应用内解码</strong>
            <span>系统会先直接播放原 TS；只有当前 WebView 不支持其编码时，才可按需生成 .playable.mp4。切片始终使用原 TS/NAS。</span>
            {#if playbackLoadError}
              <span class="playback-inline-error" role="alert">{playbackLoadError}</span>
            {/if}
            <button type="button" class="primary-button playback-button" on:click={() => void prepareVideoForPlayback({ force: true })}>生成可播放版本</button>
          {/if}
        </div>
      {:else if video && (videoPlayerUrl || playbackMode === "hls" || nativePlayerActive)}
        {#key playbackFileKey}
        {#if nativePlayerActive}
          <div class:expanded={nativePlayerExpanded} class="native-player-shell">
            <div class="native-player-host" bind:this={nativePlayerHost}>
              <div class="native-player-fallback" aria-live="polite">
                <span class="native-source-badge">原始 TS</span>
                {#if nativePlayerTransitioning}
                  <strong>正在定位播放位置…</strong>
                {/if}
              </div>
            </div>
            <div class="native-player-controls" aria-label="视频播放控制">
              <input
                class="native-player-progress"
                aria-label="播放进度"
                type="range"
                min="0"
                max={Math.max(1, nativePlaybackDurationSec)}
                step="0.1"
                value={nativePlaybackPositionSec}
                disabled={nativePlayerTransitioning}
                on:change={handleNativeProgressChange}
              />
              <div class="native-player-control-row">
                <button
                  type="button"
                  class="native-player-icon-button"
                  title={nativePlayerPaused ? "继续播放" : "暂停播放"}
                  aria-label={nativePlayerPaused ? "继续播放" : "暂停播放"}
                  disabled={nativePlayerTransitioning}
                  on:click={() => void (nativePlayerPaused ? startNativePlayer(nativePlaybackPositionSec) : pauseNativePlayer())}
                >
                  {#if nativePlayerPaused}<Play size={16} fill="currentColor" />{:else}<Pause size={16} fill="currentColor" />{/if}
                </button>
                <span class="native-player-time">{formatTime(nativePlaybackPositionSec)} <i>/</i> {nativePlaybackDurationSec ? formatTime(nativePlaybackDurationSec) : "--:--"}</span>
                <span class="native-player-label">原始 TS</span>
                <button
                  type="button"
                  class="native-player-icon-button native-player-expand-button"
                  title={nativePlayerExpanded ? "退出沉浸模式" : "沉浸播放"}
                  aria-label={nativePlayerExpanded ? "退出沉浸模式" : "沉浸播放"}
                  disabled={nativePlayerTransitioning}
                  on:click={() => void toggleNativePlayerExpanded()}
                >
                  {#if nativePlayerExpanded}<Minimize2 size={16} />{:else}<Maximize2 size={16} />{/if}
                </button>
              </div>
            </div>
          </div>
        {:else}
          {#if embeddedPreviewActive}
            <div class="embedded-player-shell" bind:this={embeddedPlayerShell}>
              <!-- svelte-ignore a11y-media-has-caption -->
              <video
                bind:this={videoElement}
                playsinline
                preload="metadata"
                on:click={toggleEmbeddedPlayback}
                on:timeupdate={updateEmbeddedPlaybackPosition}
                on:play={() => embeddedPlaybackPlaying = true}
                on:pause={() => embeddedPlaybackPlaying = false}
                on:ended={handleEmbeddedPlaybackEnded}
                on:error={() => void handleVideoElementError()}
              />
              <div class="embedded-player-controls" aria-label="整场视频播放控制">
                <input
                  class="embedded-player-progress"
                  aria-label="整场播放进度"
                  type="range"
                  min="0"
                  max={Math.max(1, fullPlaybackDurationSec)}
                  step="0.1"
                  value={embeddedPlaybackPositionSec}
                  disabled={isPreparingPlayback}
                  on:change={handleEmbeddedSeekChange}
                />
                <div class="embedded-player-control-row">
                  <button type="button" class="embedded-player-icon-button" disabled={isPreparingPlayback} on:click={toggleEmbeddedPlayback} aria-label={embeddedPlaybackPlaying ? "暂停" : "播放"}>
                    {#if embeddedPlaybackPlaying}<Pause size={16} fill="currentColor" />{:else}<Play size={16} fill="currentColor" />{/if}
                  </button>
                  <span class="embedded-player-time">{formatTime(embeddedPlaybackPositionSec)} <i>/</i> {fullPlaybackDurationSec ? formatTime(fullPlaybackDurationSec) : "--:--"}</span>
                  <span class="embedded-player-label">整场 TS · 按需加载</span>
                  <button type="button" class="embedded-player-icon-button embedded-player-expand-button" on:click={() => void toggleEmbeddedFullscreen()} aria-label="全屏">
                    <Maximize2 size={16} />
                  </button>
                </div>
              </div>
            </div>
          {:else}
            {#if embeddedPreviewActive}
              <div class="embedded-player-shell" bind:this={embeddedPlayerShell}>
                <!-- svelte-ignore a11y-media-has-caption -->
                <video
                  bind:this={videoElement}
                  playsinline
                  preload="metadata"
                  on:click={toggleEmbeddedPlayback}
                  on:timeupdate={updateEmbeddedPlaybackPosition}
                  on:play={() => embeddedPlaybackPlaying = true}
                  on:pause={() => embeddedPlaybackPlaying = false}
                  on:ended={handleEmbeddedPlaybackEnded}
                  on:error={() => void handleVideoElementError()}
                />
                <div class="embedded-player-controls" aria-label="整场视频播放控制">
                  <input class="embedded-player-progress" aria-label="整场播放进度" type="range" min="0" max={Math.max(1, fullPlaybackDurationSec)} step="0.1" value={embeddedPlaybackPositionSec} disabled={isPreparingPlayback} on:change={handleEmbeddedSeekChange} />
                  <div class="embedded-player-control-row">
                    <button type="button" class="embedded-player-icon-button" disabled={isPreparingPlayback} on:click={toggleEmbeddedPlayback} aria-label={embeddedPlaybackPlaying ? "暂停" : "播放"}>
                      {#if embeddedPlaybackPlaying}<Pause size={16} fill="currentColor" />{:else}<Play size={16} fill="currentColor" />{/if}
                    </button>
                    <span class="embedded-player-time">{formatTime(embeddedPlaybackPositionSec)} <i>/</i> {fullPlaybackDurationSec ? formatTime(fullPlaybackDurationSec) : "--:--"}</span>
                    <span class="embedded-player-label">整场 TS · 按需加载</span>
                    <button type="button" class="embedded-player-icon-button embedded-player-expand-button" on:click={() => void toggleEmbeddedFullscreen()} aria-label="全屏"><Maximize2 size={16} /></button>
                  </div>
                </div>
              </div>
            {:else}
              <!-- svelte-ignore a11y-media-has-caption -->
              <video bind:this={videoElement} src={playbackMode === "native" ? videoPlayerUrl : undefined} controls playsinline preload="metadata" on:error={() => void handleVideoElementError()} />
            {/if}
          {/if}
        {/if}
        {/key}
      {:else if playerUrl}
        {#key playerUrl}
          <iframe title="录播播放器" src={playerUrl} allow="autoplay; fullscreen" />
        {/key}
      {:else}
        <div class="empty-player">
          {#if isTranscribing}
            <Loader2 size={32} class="is-spinning" />
            <span>{stage}</span>
          {:else}
            <Play size={32} />
            <span>加载录播后在此播放</span>
          {/if}
        </div>
      {/if}
    </div>
  </CompanyAnalysisWorkspace>
  </div>
  {:else}
  <div class="analysis-grid">
    <section class="column video-column">
      <div class="column-title">
        <div><span class="step">1</span>视频和重点片段</div>
        <span>{candidates.length} 个片段</span>
      </div>
      <div class="player-wrap">
        {#if playbackLoadError}
          <div class="module-error" role="alert">
            <span>{playbackLoadError}</span>
            {#if video}<button type="button" on:click={retryAnalysisPlayback}>重试</button>{/if}
          </div>
        {/if}
        {#if video && playbackView !== "player"}
          <div class="empty-player playback-preparation">
            {#if playbackView === "preparing" || isPreparingPlayback}
              <Loader2 size={32} class="is-spinning" />
              <strong>正在生成可播放版本</strong>
              <span>{playbackSource?.message || "正在转换为 H.264 MP4…"}</span>
            {:else}
              <Play size={32} />
              <strong>原始视频暂时无法在应用内解码</strong>
              <span>{playbackSource?.message || "已先尝试直接播放原始 TS；如仍失败，可生成 .playable.mp4，原文件保持不变。"}</span>
              {#if playbackLoadError}
                <span class="playback-inline-error" role="alert">{playbackLoadError}</span>
              {/if}
              <button type="button" class="primary-button playback-button" on:click={() => void prepareVideoForPlayback({ force: true })}>生成可播放版本</button>
            {/if}
          </div>
        {:else if video && (videoPlayerUrl || playbackMode === "hls" || nativePlayerActive)}
          {#key playbackFileKey}
          {#if nativePlayerActive}
            <div class:expanded={nativePlayerExpanded} class="native-player-shell">
              <div class="native-player-host" bind:this={nativePlayerHost}>
                <div class="native-player-fallback"><strong>正在打开原始 TS…</strong></div>
              </div>
              <div class="native-player-controls">
                <button type="button" on:click={() => void (nativePlayerPaused ? startNativePlayer(nativePlaybackPositionSec) : pauseNativePlayer())}>{nativePlayerPaused ? "继续" : "暂停"}</button>
                <input aria-label="播放进度" type="range" min="0" max={Math.max(1, nativePlaybackDurationSec)} step="0.1" value={nativePlaybackPositionSec} on:change={handleNativeProgressChange} />
                <span>{formatTime(nativePlaybackPositionSec)} / {nativePlaybackDurationSec ? formatTime(nativePlaybackDurationSec) : "--:--"}</span>
                <button type="button" on:click={() => void toggleNativePlayerExpanded()}>{nativePlayerExpanded ? "还原" : "放大"}</button>
              </div>
              </div>
          {:else}
            <!-- svelte-ignore a11y-media-has-caption -->
            <video bind:this={videoElement} src={playbackMode === "native" ? videoPlayerUrl : undefined} controls playsinline preload="metadata" on:error={() => void handleVideoElementError()} />
          {/if}
          {/key}
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
        <button class:active={highlightFilter === "完整成交链路（已核验）"} on:click={() => highlightFilter = "完整成交链路（已核验）"}>
          完整成交链路（已核验） <span>{verifiedChainCount}</span>
        </button>
        <button class:active={highlightFilter === "其他可评分片段"} on:click={() => highlightFilter = "其他可评分片段"}>
          其他可评分片段 <span>{pendingOrLocalCount}</span>
        </button>
      </div>

      {#if analysisPurpose === "enterprise_review"}
        <DealTimelinePanel events={paymentEvents} on:seek={(event) => seekToDealOffset(event.detail)} />
      {/if}

      <div class="candidate-list selectable">
        {#if filteredCandidates.length}
          {#each filteredCandidates as candidate}
            <button
              class="candidate-card"
              class:selected={candidate.id === selectedCandidateId}
              class:core={candidate.verificationStatus === "verified_complete"}
              on:click={() => selectCandidate(candidate)}
            >
              <div class="candidate-heading">
                <strong>{candidate.scene || candidate.product}</strong>
                <span class="tier-pill" class:core-tier={candidate.verificationStatus === "verified_complete"}>
                  {isStructuredDiscoveryCandidate(candidate) ? candidateOutcomeLabel(candidate.outcome) : candidateDisplayType(candidate)}
                </span>
              </div>
              <div class="candidate-meta">
                <span>{formatTime(candidate.start)}—{formatTime(candidate.end)}</span>
                <span class="operational-label">{operationalScriptLabel(candidate.type)}</span>
                {#if candidate.interrupted}<span class="interrupted-label">受到打断</span>{/if}
              </div>
              <div class="result-signal">结果信号：{transcriptSignalMap[candidate.id]?.label || "未发现明显信号"}</div>
              {#if dealSignalMap[candidate.id]}
                <div class="deal-signal">成交信号：{dealSignalMap[candidate.id]?.label}</div>
              {/if}
              {#if candidate.keySentence}
                <blockquote class="candidate-key-sentence">“{candidate.keySentence}”</blockquote>
              {:else}
                <div class="signal-row">
                  {#each candidate.signals as signal}<span>{signal}</span>{/each}
                </div>
              {/if}
              {#if candidate.chainStages.length}
                <div class="chain-row"><span>链路</span>{candidate.chainStages.join(" → ")}</div>
              {:else if candidate.type === "成交收口"}
                <div class="chain-row incomplete"><span>链路</span>仅成交收口，不参与母稿评分</div>
              {/if}
              <p>{candidateStatusDetail(candidate)}</p>
            </button>
          {/each}
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
      {#if transcriptLoadError}
        <div class="module-error" role="alert">
          <span>{transcriptLoadError}</span>
          <button type="button" on:click={() => void regenerateAndAnalyze()}>重试</button>
        </div>
      {/if}
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
        <div><span class="step">3</span>{workspaceTab === "proofreading" ? "校稿审核" : "片段复盘"}</div>
        {#if workspaceTab === "analysis"}<div class="copy-actions">
          <button title="复制话术发展建议" disabled={!selectedReview} on:click={copySpokenScript}>
            {#if copiedAction === "话术发展建议"}<Check size={14} />{:else}<Copy size={14} />{/if}
            复制发展建议
          </button>
          <button title="复制全部" disabled={!selectedReview} on:click={copyAll}>
            {#if copiedAction === "全部"}<Check size={14} />{:else}<Clipboard size={14} />{/if}
            复制全部
          </button>
        </div>{:else if auditBundle}<span>{auditBundle.pendingCriticalCount} 条待确认</span>{/if}
      </div>
      {#if discoveryCompleted && candidates.length}
        <details class="session-diagnosis" open>
          <summary>
            <span>本场诊断摘要</span>
            <strong>{sessionDiagnosis.headline}</strong>
          </summary>
          <div class="diagnosis-actions">
            <button on:click={() => copyText(currentOptimizationPlanMarkdown(), "优化计划")}><Copy size={13} />复制计划</button>
            <button on:click={exportOptimizationPlan}><Download size={13} />导出下场优化计划</button>
          </div>
          <div class="diagnosis-grid">
            {#if sessionDiagnosis.highlights.length}
              <section><h3>值得保留</h3><ul>{#each sessionDiagnosis.highlights as item}<li>{item}</li>{/each}</ul></section>
            {/if}
            {#if sessionDiagnosis.issues.length}
              <section><h3>主要问题</h3><ul>{#each sessionDiagnosis.issues as item}<li>{item}</li>{/each}</ul></section>
            {/if}
            {#if sessionDiagnosis.confirmations.length}
              <section><h3>还要确认</h3><ul>{#each sessionDiagnosis.confirmations as item}<li>{item}</li>{/each}</ul></section>
            {/if}
            {#if sessionDiagnosis.nextActions.length}
              <section class="next-actions"><h3>下场动作</h3><ol>{#each sessionDiagnosis.nextActions as item}<li>{item}</li>{/each}</ol></section>
            {/if}
          </div>
        </details>
      {/if}
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
                on:complete={completeTranscriptReview}
              />
            </div>
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
              <span class="tier-label" class:core-tier={selectedCandidate.verificationStatus === "verified_complete"}>{candidateDisplayType(selectedCandidate)}</span>
              <strong>{selectedCandidate.scene || selectedCandidate.product}</strong>
              <small>{formatTime(selectedCandidate.start)}—{formatTime(selectedCandidate.end)}</small>
            </div>
            <div class="highlight-score">
              <strong>{isStructuredDiscoveryCandidate(selectedCandidate) ? candidateOutcomeLabel(selectedCandidate.outcome) : selectedCandidate.verificationStatus === "verified_complete" ? "已检查" : "待检查"}</strong>
              <span>{selectedCandidate.interrupted ? "话术受到打断" : selectedCandidate.verificationStatus === "verified_complete" ? "可对照企业标准话术" : "等待下一步复核"}</span>
            </div>
          </div>
          <div class="evidence-card">
            <div class="signal-row">
              {#each selectedCandidate.signals as signal}<span>{signal}</span>{/each}
            </div>
            {#if selectedCandidate.customerNeed}
              <p><strong>客户需求</strong>{selectedCandidate.customerNeed}</p>
            {/if}
            {#if selectedCandidate.keySentence}
              <p class="key-sentence-detail"><strong>保留原话</strong>“{selectedCandidate.keySentence}”</p>
            {:else if selectedCandidate.hook}
              <blockquote>“{selectedCandidate.hook}”</blockquote>
            {/if}
            <p><strong>为什么选中</strong>{selectedCandidate.whySelected || selectedCandidate.reason || "系统正在整理"}</p>
            {#if selectedCandidate.evidenceItems.length}
              <div class="evidence-list">
                <strong>真实证据</strong>
                {#each selectedCandidate.evidenceItems as item}
                  <p><time>{item.time}</time><span>{item.quote}</span></p>
                {/each}
              </div>
            {:else}
              <p><strong>真实证据</strong>{selectedCandidate.evidence || "系统正在整理"}</p>
            {/if}
            <p class="chain-summary"><strong>这段对话走到哪一步</strong>{selectedCandidate.chainStages.join(" → ") || "系统还没有确认完整过程"}</p>
            <p class="chain-summary"><strong>系统判断</strong>{candidateStatusDetail(selectedCandidate)}</p>
            <p class="verify-line"><strong>使用前请确认</strong>{selectedCandidate.verify || "暂时没有需要补充确认的内容"}</p>
          </div>
          {#if isStructuredDiscoveryCandidate(selectedCandidate)}
            {#if (auditBundle?.pendingCriticalCount || 0) > 0}
              <div class="review-gate-note">
                逐字稿关键项待确认，当前评分仅供参考，不可进入候选辅稿或母稿。
              </div>
            {/if}
            <MasterComparisonPanel
              baseline={masterBaseline}
              bind:selectedSectionId={selectedMasterSectionId}
              result={masterComparison}
              loading={masterComparisonLoading}
              error={masterComparisonError}
              matchStatus={masterMatchStatus}
              on:upgrade-retried={handleUpgradeReviewRetried}
            />
          {:else}
            <section class="candidate-disposition">
              <strong>这段怎么处理</strong>
              <p>{candidateDisposition(selectedCandidate)}</p>
            </section>
          {/if}
          <section class="session-review-card">
            <div>
              <strong>整场母稿复盘</strong>
              <p>
                完整链路已复盘 {sessionReviewSummary.reviewedCount}/{sessionReviewSummary.totalCandidates} 段 · 平均分 {sessionReviewSummary.averageScore ?? "-"} ·
                可复用分 85 分及以上 {sessionReviewSummary.highScoreCount} 段 · 待复盘 {sessionReviewSummary.pendingCount} 段 ·
                未匹配 {sessionReviewSummary.unmatchedCount} 段 · 需注意 {sessionReviewSummary.riskCount} 段
                {#if isSessionReviewing && sessionReviewTargetId} · 后台正在处理 {sessionReviewTargetId}{/if}
              </p>
            </div>
            <button
              class="secondary-button master-button"
              class:cancel-button={isSessionReviewing}
              disabled={!masterBaseline || !reusableCandidates.length}
              on:click={isSessionReviewing ? cancelFullSessionReview : runFullSessionReview}
            >
              {isSessionReviewing ? `取消整场复盘 ${sessionReviewCompleted}/${reusableCandidates.length}` : "开始整场复盘"}
            </button>
          </section>
        {/if}

        {#if selectedReview || (reviewingId && reviewingId === selectedCandidateId)}
          <div class="review-tabs" role="tablist" aria-label="复盘内容">
            <button class:active={reviewTab === "analysis"} on:click={() => reviewTab = "analysis"}><BarChart3 size={14} />一眼结论</button>
            <button class:active={reviewTab === "script"} on:click={() => reviewTab = "script"}><MessageSquareText size={14} />话术发展建议</button>
            <button class:active={reviewTab === "training"} on:click={() => reviewTab = "training"}><ListChecks size={14} />练习方向</button>
          </div>
          {#if reviewingId && reviewingId === selectedCandidateId}
            <div class="review-loading-banner">
              <Loader2 size={18} class="is-spinning" />
              <span>{selectedReview ? "正在更新当前复盘，下面仍可查看上次保存结果。" : "复盘官正在生成分析、话术发展建议和练习方向…"}</span>
              <button on:click={() => cancelCurrentReview()}>取消当前复盘</button>
            </div>
          {/if}
          {#if selectedReview && reviewTab === "analysis"}
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
          {:else if selectedReview && reviewTab === "script"}
            <article class="result-card accent">
              <h2>话术发展建议（不可直接照念，须培训负责人审定）</h2>
              <pre>{selectedReview.spokenScript}</pre>
            </article>
          {:else if selectedReview}
            <article class="result-card training">
              <h2>培训讨论点 / 练习方向</h2>
              <pre>{selectedReview.trainingChecklist}</pre>
            </article>
          {/if}
          {#if selectedReview}
            <button
              class="rerun-button"
              disabled={Boolean(reviewingId) || !candidateReviewGateState.allowed}
              title={candidateReviewGateState.message || "重新复盘当前片段"}
              on:click={() => reviewSelectedCandidate(true)}
            >
              <RefreshCw size={14} />重新复盘当前片段
            </button>
          {/if}
          {#if !candidateReviewGateState.allowed}
            <div class="review-gate-note">{candidateReviewGateState.message}</div>
          {/if}
          {#if selectedReview}
            <div class="saved-hint">已自动保存 · {new Date(selectedReview.updatedAt).toLocaleString("zh-CN")}</div>
          {/if}
        {:else if selectedCandidate}
          <div class="empty-state large">
            {#if isSessionReviewing}
              <Loader2 size={30} class="is-spinning" />
              <span>
                {sessionReviewTargetId === selectedCandidate.id
                  ? "整场复盘正在处理当前片段…"
                  : `当前片段正在排队，整场复盘已完成 ${sessionReviewCompleted}/${reusableCandidates.length} 段`}
              </span>
            {:else if candidateReviewGateState.allowed}
              <FileSearch size={30} />
              <span>当前片段尚未复盘</span>
              <button class="primary-button" on:click={() => reviewSelectedCandidate(true)}>开始复盘</button>
            {:else}
              <FileSearch size={30} />
              <span>{candidateReviewGateState.message}</span>
            {/if}
          </div>
        {:else}
          <div class="empty-state large">
            {#if isDiscovering}
              <Loader2 size={30} class="is-spinning" />
              <span>正在根据当前逐字稿识别候选片段…</span>
            {:else if !discoveryCompleted}
              <FileSearch size={30} />
              <span>逐字稿准备好后会自动识别候选片段，关键内容可以稍后校对。</span>
            {:else}
              <FileSearch size={30} />
              <span>没有发现证据充分的候选片段，可从左侧点击“重新发现”。</span>
            {/if}
          </div>
        {/if}
      </div>
      {/if}
    </section>
  </div>
  {/if}
</div>
</div>

<style>
  .company-player-shell { width: 100%; height: 100%; min-height: 280px; display: flex; flex-direction: column; min-width: 0; }
  .company-player-shell .module-error { flex: 0 0 auto; margin: 0 0 8px; }
  .company-player-shell .empty-player { width: 100%; flex: 1 1 auto; min-height: 280px; display: grid; place-items: center; gap: 8px; padding: 16px; border-radius: 10px; background: #0f172a; color: #cbd5e1; text-align: center; }
  .company-player-shell .playback-preparation { color: #e2e8f0; }
  .company-player-shell video,
  .company-player-shell iframe { width: 100%; height: 100%; min-height: 280px; border: 0; border-radius: 10px; background: #0f172a; }
  .analysis-zoom-viewport { width: 100%; height: 100%; min-height: 0; flex: 1 1 auto; display: flex; flex-direction: column; overflow: hidden; }
  .analysis-shell { flex: 1 1 auto; min-height: 0; width: 100%; display: flex; flex-direction: column; color: #1d1d1f; background: linear-gradient(180deg, #fbfbfd 0%, #f2f3f6 100%); zoom: var(--analysis-zoom); }
  .analysis-main { flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; padding: 0 0 10px; }
  .analysis-main :global(.company-workspace) { flex: 1 1 auto; min-height: 0; height: 100%; overflow: hidden; }
  .analysis-header { height: 76px; flex: 0 0 76px; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 0 20px; background: rgba(255,255,255,.76); border-bottom: 1px solid rgba(0,0,0,.055); backdrop-filter: blur(24px) saturate(160%); }
  .header-main, .header-actions, .column-title, .candidate-heading, .review-summary, .copy-actions { display: flex; align-items: center; }
  .header-main { gap: 10px; min-width: 0; }
  .title-row { display: flex; align-items: center; gap: 8px; }
  .header-main h1 { margin: 0; font-size: 19px; font-weight: 700; letter-spacing: -.45px; }
  .mode-pill { padding: 3px 7px; border-radius: 999px; color: #0068d1; background: rgba(0,113,227,.09); font-size: 9px; font-weight: 650; }
  .header-main p { margin: 3px 0 0; color: #86868b; font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 390px; }
  .header-actions { gap: 8px; }
  .zoom-control { height: 30px; display: inline-flex; align-items: center; flex: 0 0 auto; padding: 1px; border: 1px solid rgba(0,0,0,.08); border-radius: 8px; background: rgba(255,255,255,.78); }
  .zoom-control button { width: 28px; height: 28px; display: inline-grid; place-items: center; padding: 0; border: 0; border-radius: 6px; color: #475467; background: transparent; cursor: pointer; }
  .zoom-control button:hover:not(:disabled) { color: #0071e3; background: rgba(0,113,227,.08); }
  .zoom-control button:disabled { color: #c5c8ce; cursor: default; }
  .zoom-control .zoom-value { width: 48px; font-size: 10px; font-variant-numeric: tabular-nums; }
  .stage { max-width: 330px; padding: 6px 9px; color: #6e6e73; border-radius: 999px; background: rgba(118,118,128,.08); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .error-stage { color: #d70015; background: rgba(255,59,48,.08); }
  .module-error { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin: 8px; padding: 8px 10px; color: #b42318; background: #fff4f2; border: 1px solid #fecdca; border-radius: 8px; font-size: 11px; }
  .module-error button { flex: none; padding: 4px 8px; color: #175cd3; background: white; border: 1px solid #b2ddff; border-radius: 6px; cursor: pointer; }
  .live-dashboard-summary { display: grid; gap: 8px; margin: 10px 13px 0; padding: 11px 13px; border: 1px solid #cfe1f7; border-radius: 13px; background: linear-gradient(135deg, #f6fbff, #eef5ff); color: #475467; font-size: 10px; }
  .live-dashboard-summary-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .live-dashboard-summary-heading div { min-width: 0; display: grid; gap: 3px; }
  .live-dashboard-summary-heading strong { color: #175cd3; font-size: 12px; }
  .live-dashboard-summary-heading span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .live-dashboard-summary-actions { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }
  .live-dashboard-open-button { display: inline-flex; align-items: center; gap: 6px; border: 1px solid #bfd4ff; border-radius: 999px; background: #eff4ff; color: #175cd3; padding: 4px 10px; font-size: 12px; cursor: pointer; }
  .live-dashboard-summary-heading small { flex: 0 0 auto; padding: 3px 7px; border-radius: 999px; color: #087c42; background: #eaf8f0; }
  .live-dashboard-metrics { display: grid; grid-template-columns: repeat(11, minmax(84px, 1fr)); gap: 6px; overflow-x: auto; }
  .live-dashboard-metric { min-width: 84px; display: grid; gap: 4px; padding: 7px; border: 1px solid rgba(0,113,227,.1); border-radius: 8px; background: rgba(255,255,255,.78); }
  .live-dashboard-metric span { color: #667085; font-size: 9px; line-height: 1.35; }
  .live-dashboard-metric strong { color: #1d4ed8; font-size: 13px; font-variant-numeric: tabular-nums; }
  .live-dashboard-bind-controls { display: flex; align-items: center; gap: 8px; }
  .live-dashboard-bind-controls select { min-width: 300px; height: 32px; padding: 0 9px; border: 1px solid #b8d9ff; border-radius: 8px; color: #344054; background: white; font: inherit; }
  .live-dashboard-error { color: #b42318; }
  .payment-events-summary {
    display: grid;
    gap: 8px;
    margin: 10px 13px 0;
    padding: 11px 13px;
    border: 1px solid #d1fadf;
    border-radius: 13px;
    background: linear-gradient(135deg, #f6fff9, #eefcf3);
    color: #475467;
    font-size: 10px;
  }
  .payment-events-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .payment-events-heading div { min-width: 0; display: grid; gap: 3px; }
  .payment-events-heading strong { color: #027a48; font-size: 12px; }
  .payment-events-heading span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .payment-events-actions { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }
  .payment-events-peak { color: #027a48; font-size: 10px; }
  .hidden-file-input { display: none; }
  .text-button { border: none; background: transparent; color: #667085; cursor: pointer; font-size: 11px; padding: 0; }
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
  .native-player-shell { width: 100%; height: 100%; min-height: 240px; display: flex; flex-direction: column; overflow: hidden; background: #020617; position: relative; isolation: isolate; }
  .native-player-shell.expanded { position: fixed; inset: 0; z-index: 10000; min-height: 0; }
  .native-player-host { width: 100%; min-height: 0; flex: 1 1 auto; background: #000; position: relative; }
  .native-player-controls { flex: 0 0 62px; display: flex; flex-direction: column; gap: 7px; padding: 7px 12px 9px; box-sizing: border-box; color: #f8fafc; background: linear-gradient(180deg, rgba(2,6,23,.91), #020617); box-shadow: 0 -10px 26px rgba(0,0,0,.26); }
  .native-player-progress { width: 100%; height: 4px; margin: 0; cursor: pointer; accent-color: #1d9bf0; }
  .native-player-progress:disabled { cursor: wait; opacity: .55; }
  .native-player-control-row { min-width: 0; display: flex; align-items: center; gap: 10px; }
  .native-player-icon-button { width: 28px; height: 28px; flex: 0 0 28px; display: grid; place-items: center; padding: 0; border: 0; border-radius: 50%; color: #f8fafc; background: rgba(255,255,255,.14); cursor: pointer; transition: background .15s ease, transform .15s ease; }
  .native-player-icon-button:hover:not(:disabled) { background: rgba(255,255,255,.24); transform: scale(1.06); }
  .native-player-icon-button:disabled { cursor: wait; opacity: .55; }
  .native-player-time { min-width: 0; color: #f1f5f9; font-size: 11px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .native-player-time i { padding: 0 2px; color: #94a3b8; font-style: normal; }
  .native-player-label { min-width: 0; overflow: hidden; margin-left: 2px; color: #94a3b8; font-size: 10px; white-space: nowrap; text-overflow: ellipsis; }
  .native-player-expand-button { margin-left: auto; }
  .embedded-player-shell { width: 100%; height: 100%; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: #000; }
  .company-player-shell .embedded-player-shell video,
  .player-wrap .embedded-player-shell video { width: 100%; height: auto; min-height: 0; flex: 1 1 auto; object-fit: contain; border-radius: 0; }
  .embedded-player-controls { flex: 0 0 58px; display: flex; flex-direction: column; gap: 6px; padding: 7px 12px 8px; box-sizing: border-box; color: #f8fafc; background: linear-gradient(180deg, rgba(2,6,23,.94), #020617); box-shadow: 0 -8px 22px rgba(0,0,0,.3); }
  .embedded-player-progress { width: 100%; height: 4px; margin: 0; cursor: pointer; accent-color: #1687f8; }
  .embedded-player-progress:disabled { cursor: wait; opacity: .5; }
  .embedded-player-control-row { min-width: 0; display: flex; align-items: center; gap: 10px; }
  .embedded-player-icon-button { width: 28px; height: 28px; flex: 0 0 28px; display: grid; place-items: center; padding: 0; border: 0; border-radius: 50%; color: #f8fafc; background: rgba(255,255,255,.14); cursor: pointer; }
  .embedded-player-icon-button:hover:not(:disabled) { background: rgba(255,255,255,.24); }
  .embedded-player-time { color: #f1f5f9; font-size: 11px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .embedded-player-time i { padding: 0 2px; color: #94a3b8; font-style: normal; }
  .embedded-player-label { min-width: 0; overflow: hidden; color: #94a3b8; font-size: 10px; white-space: nowrap; text-overflow: ellipsis; }
  .embedded-player-expand-button { margin-left: auto; }
  .embedded-player-shell:fullscreen { width: 100vw; height: 100vh; }
  .native-player-fallback {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    justify-content: flex-start;
    padding: 10px;
    color: #dbe7ff;
    text-align: center;
    pointer-events: none;
  }
  .native-player-fallback strong { padding: 5px 8px; border-radius: 6px; background: rgba(2,6,23,.66); font-size: 11px; font-weight: 500; }
  .native-source-badge { align-self: flex-start; padding: 4px 7px; border: 1px solid rgba(255,255,255,.16); border-radius: 999px; color: #e0f2fe; background: rgba(2,6,23,.64); font-size: 10px; font-weight: 650; letter-spacing: .02em; }
  .empty-player { width: 100%; height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: #9aa4b2; font-size: 12px; text-align: center; }
  .playback-preparation { padding: 24px; box-sizing: border-box; }
  .playback-preparation strong { color: #e7edf8; font-size: 14px; }
  .playback-preparation .playback-button { min-height: 32px; padding: 0 12px; font-size: 11px; }
  .playback-inline-error { max-width: 36em; color: #fecaca; font-size: 11px; line-height: 1.45; }
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
  .operational-label { color: #175cd3; font-weight: 650; }
  .result-signal { margin-top: 6px; color: #5b6472; font-size: 9px; }
  .deal-signal { margin-top: 4px; color: #027a48; font-size: 9px; font-weight: 650; }
  .interrupted-label { color: #9a6700; font-weight: 650; }
  .signal-row { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 7px; }
  .signal-row span { padding: 3px 6px; border-radius: 5px; color: #365c7d; background: #edf5fb; font-size: 9px; }
  .candidate-key-sentence { margin: 8px 0 0; padding-left: 8px; border-left: 2px solid #1687f8; color: #24364b; font-size: 11px; line-height: 1.5; }
  .chain-row { display: flex; gap: 6px; margin-top: 7px; color: #17663a; font-size: 9px; line-height: 1.45; }
  .chain-row > span { flex: 0 0 auto; color: #667085; }
  .chain-row.incomplete { color: #9a6700; }
  .candidate-card p { margin: 6px 0 0; color: #6b7280; font-size: 11px; line-height: 1.45; }
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
  .session-diagnosis { flex: 0 0 auto; margin: 8px 10px 0; border: 1px solid #cfe1f7; border-radius: 8px; background: #f7fbff; overflow: hidden; }
  .session-diagnosis summary { display: grid; gap: 3px; padding: 9px 11px; cursor: pointer; }
  .session-diagnosis summary span { color: #175cd3; font-size: 10px; font-weight: 700; }
  .session-diagnosis summary strong { color: #24364b; font-size: 12px; line-height: 1.4; }
  .diagnosis-actions { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 10px 8px; }
  .diagnosis-actions button { display: inline-flex; align-items: center; gap: 4px; min-height: 28px; padding: 0 8px; border: 1px solid #b8d9ff; border-radius: 6px; background: white; color: #175cd3; font-size: 9px; cursor: pointer; }
  .diagnosis-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 7px; padding: 0 10px 10px; }
  .diagnosis-grid section { min-width: 0; padding: 8px; border-radius: 7px; background: white; }
  .diagnosis-grid h3 { margin: 0 0 5px; color: #344054; font-size: 10px; }
  .diagnosis-grid ul, .diagnosis-grid ol { margin: 0; padding-left: 16px; color: #475467; font-size: 9px; line-height: 1.55; }
  .diagnosis-grid .next-actions { border-left: 3px solid #1687f8; }
  .workspace-tabs { flex: 0 0 auto; display: grid; grid-template-columns: 1fr 1fr; gap: 3px; margin: 8px 10px 0; padding: 3px; border-radius: 7px; background: rgba(118,118,128,.08); }
  .workspace-tabs button { min-width: 0; height: 30px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; border: 0; border-radius: 5px; color: #667085; background: transparent; font-size: 10px; cursor: pointer; }
  .workspace-tabs button.active { color: #1d1d1f; background: white; box-shadow: 0 1px 4px rgba(28,34,43,.1); }
  .workspace-tabs span { min-width: 17px; padding: 1px 4px; border-radius: 8px; color: #a34b00; background: #fff0d5; font-size: 9px; }
  .proofreading-content { min-height: 0; flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .review-panel-slot { min-height: 0; flex: 1; overflow: hidden; }
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
  .evidence-card .chain-summary { color: #17663a; }
  .evidence-card .verify-line { color: #8a4b20; }
  .evidence-card .key-sentence-detail { padding: 8px; border-left: 2px solid #1687f8; background: #f3f8ff; color: #24364b; }
  .evidence-list { margin-top: 9px; color: #5b6577; font-size: 10px; }
  .evidence-list > strong { color: #344054; }
  .evidence-list p { grid-template-columns: 52px minmax(0, 1fr); margin-top: 5px; }
  .evidence-list time { color: #1769aa; font-variant-numeric: tabular-nums; }
  .candidate-disposition { margin: 9px 0; padding: 11px; border: 1px solid #f3d7a4; border-left: 3px solid #f79009; border-radius: 8px; background: #fffaf0; }
  .candidate-disposition strong { display: block; color: #8a4b00; font-size: 12px; }
  .candidate-disposition p { margin: 4px 0 0; color: #6b5b3e; font-size: 11px; line-height: 1.5; }
  .session-review-card { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin: 9px 0; padding: 11px; border: 1px solid rgba(0,113,227,.14); border-radius: 8px; background: #f4f9ff; }
  .session-review-card div { min-width: 0; }
  .session-review-card strong { display: block; color: #1d1d1f; font-size: 12px; }
  .session-review-card p { margin: 4px 0 0; color: #667085; font-size: 10px; line-height: 1.45; }
  .session-review-card button { flex: 0 0 auto; }
  .session-review-card .cancel-button { border-color: #f0a3a3; color: #b42318; background: #fff5f5; }
  .review-loading-banner { display: flex; align-items: center; gap: 8px; margin-bottom: 9px; padding: 10px; border: 1px solid #cfe3ff; border-radius: 8px; background: #f4f9ff; color: #475467; font-size: 11px; }
  .review-loading-banner span { min-width: 0; flex: 1; }
  .review-loading-banner button { flex: 0 0 auto; height: 28px; padding: 0 9px; border: 1px solid #f0a3a3; border-radius: 6px; background: white; color: #b42318; font-size: 10px; cursor: pointer; }
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
  :global(.dark) .live-dashboard-summary { border-color: #36506f; background: #263546; color: #cbd5e1; }
  :global(.dark) .live-dashboard-metric, :global(.dark) .live-dashboard-bind-controls select { border-color: #495a70; background: #303b49; color: #e8eaf0; }
  :global(.dark) .live-dashboard-metric strong, :global(.dark) .live-dashboard-summary-heading strong { color: #93c5fd; }
  :global(.dark) .column-title { border-color: #3d4148; }
  :global(.dark) .candidate-card, :global(.dark) .icon-button, :global(.dark) .secondary-button, :global(.dark) .copy-actions button { background: #303238; border-color: #494d55; color: #e8eaf0; }
  :global(.dark) .zoom-control { border-color: #494d55; background: #303238; }
  :global(.dark) .zoom-control button { color: #e8eaf0; }
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
