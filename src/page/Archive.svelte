<script lang="ts">
  import { invoke, invokeSensitive, get_static_url, open } from "../lib/invoker";
  import type { RecordItem } from "../lib/db";
  import {
    chunkArchiveDeleteGroups,
    countArchiveDeleteTargets,
    groupArchivesForDeletion,
  } from "../lib/archiveDelete";
  import {
    anchorStatusLabel,
    canManuallyEditAnchor,
    shouldAutoDetectAnchor,
  } from "../lib/anchorDetection";
  import {
    formatArchiveDashboardIdentity,
    type LiveDashboardBindingSummary,
  } from "../lib/liveDashboard";
  import { COMPASS_TARGET_SHOPS, inferCompassShopFromTexts } from "../lib/compassAutoDownload";
  import {
    pruneArchiveSelection,
    selectArchiveRange,
  } from "../lib/archiveSelection";
  import { applyLoadedArchiveClassification, preferAutoClassifiedArchives } from "../lib/archiveKind";
  import { canOpenCompanyDealReview } from "../lib/companyDealReview";
  import {
    buildImportedVideoNote,
    getImportedVideoId,
    isImportedArchive,
    mergeVideoIntoImportedArchive,
    videoToImportedArchive,
  } from "../lib/importedArchive";
  import { onDestroy, onMount, tick } from "svelte";
  import {
    Play,
    Trash2,
    Calendar,
    Clock,
    HardDrive,
    RefreshCw,
    ChevronDown,
    ChevronUp,
    Video,
    Globe,
    Home,
    FileVideo,
    History,
    BrainCircuit,
    FileText,
    UserRound,
    Loader2,
    X,
    BarChart3,
    Upload,
  } from "lucide-svelte";
  import BilibiliIcon from "../lib/components/BilibiliIcon.svelte";
  import DouyinIcon from "../lib/components/DouyinIcon.svelte";
  import KuaishouIcon from "../lib/components/KuaishouIcon.svelte";
  import HuyaIcon from "../lib/components/HuyaIcon.svelte";
  import TikTokIcon from "../lib/components/TikTokIcon.svelte";
  import GenerateWholeClipModal from "../lib/components/GenerateWholeClipModal.svelte";
  import ImportVideoDialog from "../lib/components/ImportVideoDialog.svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import StreamerArchiveFilters from "../lib/components/streamer/StreamerArchiveFilters.svelte";
  import StreamerContentPassThrough from "../lib/components/streamer/StreamerContentPassThrough.svelte";
  import StreamerDirectory from "../lib/components/streamer/StreamerDirectory.svelte";
  import StreamerProfileWorkspace from "../lib/components/streamer/StreamerProfileWorkspace.svelte";
  import {
    STREAMER_VIRTUAL_AVATAR_STORAGE_KEY,
    avatarSelectionForStreamer,
    parseStreamerVirtualAvatarSelections,
    upsertStreamerVirtualAvatarSelection,
    type StreamerVirtualAvatarId,
    type StreamerVirtualAvatarSelection,
  } from "../lib/streamerAvatar";
  import {
    PENDING_STREAMER_KEY,
    MIN_SKILL_SCORE_SOURCES,
    SKILL_RATING_PERIOD_LABELS,
    STREAMER_SKILL_DIMENSIONS,
    appendAiSkillRatingVersion,
    appendSkillRatingVersion,
    assessSkillEvidenceWithAi,
    buildRatingConfidence,
    buildSkillScoreTable,
    buildSkillTrend,
    buildStreamerRadarSeries,
    confirmSkillEvidence,
    filterStreamerArchives,
    findStreamerArchiveGroup,
    getStreamerTrainingTask,
    groupCompanyArchivesByStreamer,
    isArchiveActivelyRecording,
    isSkillRatingVersionWithinSources,
    normalizeStreamerName,
    parseSkillRatingHistory,
    readContentAnalysisStorage,
    scopeSkillRatingHistoryToSources,
    streamerArchiveSourceKey,
    type ContentAnalysisCacheParseResult,
    type SkillEvidence,
    type SkillRatingPeriod,
    type SkillRatingVersion,
    type StreamerArchiveAnalysisStatus,
    type StreamerArchiveGroup,
    type StreamerArchiveLiveStatus,
    type StreamerSkillDimensionKey,
  } from "../lib/streamerProfile";
  import {
    buildStreamerSkillProfileUserMessage,
    parseStreamerSkillAiRatings,
    streamerSkillProfileSystemPrompt,
  } from "../lib/streamerSkillAi";
  import type { RecorderInfo, RecorderList } from "src/lib/interface";
  import type { VideoItem } from "../lib/interface";

  let archives: RecordItem[] = [];
  let filteredArchives: RecordItem[] = [];
  let loading = false;
  let sortBy = "created_at";
  let sortOrder = "desc";
  let archiveKindTab: "company" | "competitor" = "company";
  type RoomOption = {
    id: string;
    label: string;
  };

  let selectedRoomId: string | null = null;
  let roomOptions: RoomOption[] = [];
  let roomAccountById = new Map<string, string>();

  let selectedArchives: Set<string> = new Set();
  let lastSelectedArchiveId: string | null = null;
  let showDeleteConfirm = false;
  let archiveToDelete: RecordItem | null = null;
  let isDeletingArchives = false;
  let deleteProgress = { done: 0, total: 0 };
  type LiveDashboardBindingResult = {
    session: LiveDashboardBindingSummary & { id: number } | null;
    matchMethod: string | null;
  };

  let liveDashboardBindings = new Map<string, LiveDashboardBindingSummary>();

  // 生成完整录播相关状态
  let showWholeClipModal = false;
  let wholeClipArchive: RecordItem | null = null;

  // 分页相关状态
  let currentPage = 1;
  let pageSize = 20;
  let totalPages = 1;
  let totalCount = 0;
  let isLoading = false;
  let loadError = "";

  // 页面大小选项
  const pageSizeOptions = [10, 20, 50, 100];

  // 所有数据缓存
  let allArchives = [];
  let allRooms: RecorderInfo[] = [];
  let statusRefreshTimer: ReturnType<typeof setInterval> | null = null;
  let transcriptReady = new Set<string>(
    JSON.parse(localStorage.getItem("archive-transcript-ready") || "[]")
  );
  let transcriptGenerating = new Set<string>();
  let transcriptStatus = "";
  let showFactCardModal = false;
  let factCardArchive: RecordItem | null = null;
  let factProducts = "";
  let factPrices = "";
  let factConditions = "";
  let factLinks = "";
  let factInventory = "";
  let factAliases = "";
  let factCardError = "";
  let anchorQueueRunning = false;
  let anchorToEdit: RecordItem | null = null;
  let editingAnchorName = "";
  let anchorEditError = "";
  let showImportDialog = false;
  let importDefaultAnalysisPurpose: "enterprise_review" | "competitor_benchmark" = "enterprise_review";
  let StreamerProfileShell: any = StreamerContentPassThrough;

  $: importDefaultAnalysisPurpose = archiveKindTab === "competitor"
    ? "competitor_benchmark"
    : "enterprise_review";
  $: StreamerProfileShell = archiveKindTab === "company" && selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? StreamerProfileWorkspace
    : StreamerContentPassThrough;
  $: importTitlePlaceholder = archiveKindTab === "company"
    ? "例如：7/28 金典拍拍相机专场"
    : "例如：XX 相机二手店专场";

  type StreamerProfileTab = "streams" | "trends" | "training" | "clips" | "knowledge";

  const SKILL_RATINGS_STORAGE_KEY = "bsr:streamer-skill-ratings:v1";
  const SKILL_REVIEWER_STORAGE_KEY = "bsr:streamer-skill-reviewer:v1";
  const EMPTY_CONTENT_ANALYSIS: ContentAnalysisCacheParseResult = {
    sources: [],
    evidenceCandidates: [],
    summary: {
      sourceCount: 0,
      degradedSourceCount: 0,
      damagedSourceCount: 0,
      candidateCount: 0,
      evidenceCandidateCount: 0,
      completionPercent: 0,
    },
  };
  const EMPTY_PROFILE_HEADER = {
    name: "待确认",
    avatarIndex: 0,
    sessionCount: 0,
    totalDurationLabel: "0 分钟",
    recentLiveLabel: "暂无直播",
    trainingStatus: "不参与评分",
    dataRangeLabel: "暂无数据",
    scoreConfidenceLabel: "数据不足",
  };

  let selectedStreamerKey: string | null = null;
  let returnFocusStreamerKey: string | null = null;
  let streamerProfileTab: StreamerProfileTab = "streams";
  let profileDateFrom = "";
  let profileDateTo = "";
  let profileSessionQuery = "";
  let profileProductQuery = "";
  let profileLiveStatus: StreamerArchiveLiveStatus = "all";
  let profileAnalysisStatus: StreamerArchiveAnalysisStatus = "all";
  let contentAnalysisData: ContentAnalysisCacheParseResult = EMPTY_CONTENT_ANALYSIS;
  let skillRatingHistory: SkillRatingVersion[] = [];
  let streamerGroups: StreamerArchiveGroup<RecordItem>[] = [];
  let confirmedStreamerGroups: StreamerArchiveGroup<RecordItem>[] = [];
  let pendingStreamerGroup: StreamerArchiveGroup<RecordItem> | null = null;
  let selectedStreamerGroup: StreamerArchiveGroup<RecordItem> | null = null;
  let selectedActiveSkillRatingHistory: SkillRatingVersion[] = [];
  let analysisCompletionBySource = new Map<string, number>();
  let recoveringMissingStreamerProfile = false;
  let streamerVirtualAvatarSelections: StreamerVirtualAvatarSelection[] = [];

  let showSkillRatingModal = false;
  type SkillProfileSetupStep = 1 | 2 | 3 | 4;
  type TranscriptRefreshResult = {
    subtitle: string;
    decision: "kept" | "replaced" | "created" | "resumed";
    similarity: number;
    oldLength: number;
    newLength: number;
  };
  type SkillProfilePreparationState = {
    status: "idle" | "preparing" | "ready" | "partial";
    totalCount: number;
    readyCount: number;
    failedCount: number;
  };
  const EMPTY_SKILL_PROFILE_PREPARATION: SkillProfilePreparationState = {
    status: "idle",
    totalCount: 0,
    readyCount: 0,
    failedCount: 0,
  };
  let showSkillProfileSetupModal = false;
  let skillProfileSetupStep: SkillProfileSetupStep = 1;
  let selectedSkillProfileSetupSourceIds = new Set<string>();
  let skillProfilePreparation: SkillProfilePreparationState = EMPTY_SKILL_PROFILE_PREPARATION;
  let skillProfilePreparationRunId = 0;
  let skillAiEvaluating = false;
  let skillAiError = "";
  let skillAiResultSummary = "";
  let ratingEvidenceSourceScope = new Set<string>();
  let returnToSkillProfileSetup = false;
  let ratingDimension: StreamerSkillDimensionKey = "needs_confirmation";
  let ratingPeriod: SkillRatingPeriod = "current";
  let ratingScore = 80;
  let ratingBasis = "";
  let ratingReviewer = "";
  let ratingReviewConfirmed = false;
  let selectedRatingEvidenceIds = new Set<string>();
  let ratingError = "";

  let showSkillEvidenceModal = false;
  let evidenceDimension: StreamerSkillDimensionKey = "needs_confirmation";
  let evidenceDimensionLabel = "需求确认";

  function readSkillRatingHistory(): SkillRatingVersion[] {
    try {
      const parsed = JSON.parse(localStorage.getItem(SKILL_RATINGS_STORAGE_KEY) || "[]");
      return parseSkillRatingHistory(parsed);
    } catch {
      return [];
    }
  }

  function refreshStreamerProfileStorage(): void {
    let nextContentAnalysis = EMPTY_CONTENT_ANALYSIS;
    try {
      nextContentAnalysis = readContentAnalysisStorage(localStorage);
    } catch {
      nextContentAnalysis = EMPTY_CONTENT_ANALYSIS;
    }
    skillRatingHistory = readSkillRatingHistory();
    contentAnalysisData = nextContentAnalysis;
    analysisCompletionBySource = buildAnalysisCompletionMap(nextContentAnalysis, skillRatingHistory);
    ratingReviewer = localStorage.getItem(SKILL_REVIEWER_STORAGE_KEY) || ratingReviewer;
    try {
      streamerVirtualAvatarSelections = parseStreamerVirtualAvatarSelections(
        JSON.parse(localStorage.getItem(STREAMER_VIRTUAL_AVATAR_STORAGE_KEY) || "[]"),
      );
    } catch {
      streamerVirtualAvatarSelections = [];
    }
  }

  function virtualAvatarForGroup(
    group: StreamerArchiveGroup<RecordItem>,
    selections: readonly StreamerVirtualAvatarSelection[],
  ): StreamerVirtualAvatarSelection {
    return avatarSelectionForStreamer(
      selections,
      group.streamerKey,
      group.avatarIndex,
    );
  }

  function updateSelectedStreamerVirtualAvatar(avatarId: StreamerVirtualAvatarId): void {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending) return;
    try {
      const next = upsertStreamerVirtualAvatarSelection(
        streamerVirtualAvatarSelections,
        selectedStreamerGroup.streamerKey,
        avatarId,
      );
      localStorage.setItem(STREAMER_VIRTUAL_AVATAR_STORAGE_KEY, JSON.stringify(next));
      streamerVirtualAvatarSelections = next;
      transcriptStatus = `已为${selectedStreamerGroup.streamerName}更新虚拟角色形象；能力数据保持不变。`;
    } catch (error) {
      transcriptStatus = `虚拟角色保存失败：${error instanceof Error ? error.message : String(error)}`;
    }
  }

  function buildAnalysisCompletionMap(
    data: ContentAnalysisCacheParseResult,
    ratings: readonly SkillRatingVersion[],
  ): Map<string, number> {
    const completion = new Map(
      data.sources.map((source) => [source.sourceKey, source.completionPercent]),
    );
    // The ability profile may be built directly from archive transcripts, so
    // those rows do not necessarily have a full content-analysis cache. Once
    // a recording is actually cited by a retained rating, it has completed a
    // real AI analysis path and should no longer remain visually "未分析".
    ratings.forEach((rating) => {
      rating.evidence.forEach((evidence) => {
        const sourceId = evidence.sourceId.trim();
        if (sourceId) completion.set(sourceId, 100);
      });
    });
    return completion;
  }

  function profileFilters() {
    return {
      dateFrom: profileDateFrom,
      dateTo: profileDateTo,
      sessionQuery: profileSessionQuery,
      productQuery: profileProductQuery,
      liveStatus: profileLiveStatus,
      analysisStatus: profileAnalysisStatus,
    };
  }

  function resetProfileFilters(): void {
    profileDateFrom = "";
    profileDateTo = "";
    profileSessionQuery = "";
    profileProductQuery = "";
    profileLiveStatus = "all";
    profileAnalysisStatus = "all";
    selectedRoomId = null;
  }

  function analysisCompletionForArchive(
    archive: RecordItem,
    completionIndex = analysisCompletionBySource,
  ): number {
    return completionIndex.get(streamerArchiveSourceKey(archive)) ?? 0;
  }

  function analysisStatusLabel(
    archive: RecordItem,
    completionIndex = analysisCompletionBySource,
  ): string {
    const completion = analysisCompletionForArchive(archive, completionIndex);
    if (completion >= 100) return "已分析";
    if (completion > 0) return `分析中 ${completion}%`;
    return "未分析";
  }

  function trainingStatusFor(group: StreamerArchiveGroup<RecordItem>): string {
    return getStreamerTrainingTask(group.streamerName) ? "训练任务已配置" : "待配置训练";
  }

  function distinctEvidenceCount(rows: ReturnType<typeof buildSkillScoreTable>): number {
    return new Set(rows.flatMap((row) => row.evidence.map((item) => item.id))).size;
  }

  function radarValueMap(values: readonly (number | null)[]): Record<string, number | null> {
    const mapped: Record<string, number | null> = {};
    STREAMER_SKILL_DIMENSIONS.forEach((dimension, index) => {
      mapped[dimension.key] = values[index] ?? null;
    });
    return mapped;
  }

  function activeSkillRatingsForGroup(
    history: readonly SkillRatingVersion[],
    group: StreamerArchiveGroup<RecordItem>,
  ): SkillRatingVersion[] {
    return scopeSkillRatingHistoryToSources(
      history,
      group.streamerKey,
      group.archives.map((archive) => streamerArchiveSourceKey(archive)),
    );
  }

  $: analysisCompletionBySource = buildAnalysisCompletionMap(contentAnalysisData, skillRatingHistory);
  $: streamerGroups = groupCompanyArchivesByStreamer(allArchives);
  $: confirmedStreamerGroups = streamerGroups.filter((group) => !group.isPending);
  $: pendingStreamerGroup = streamerGroups.find((group) => group.isPending) || null;
  $: selectedStreamerGroup = selectedStreamerKey
    ? findStreamerArchiveGroup(streamerGroups, selectedStreamerKey)
    : null;
  $: selectedActiveSkillRatingHistory = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? activeSkillRatingsForGroup(skillRatingHistory, selectedStreamerGroup)
    : [];
  $: directoryProfiles = confirmedStreamerGroups.map((group) => {
    const confidence = buildRatingConfidence(
      activeSkillRatingsForGroup(skillRatingHistory, group),
      group.streamerKey,
      "current",
    );
    return {
      key: group.streamerKey,
      name: group.streamerName,
      avatarIndex: group.avatarIndex,
      virtualAvatar: virtualAvatarForGroup(group, streamerVirtualAvatarSelections),
      liveCount: group.sessionCount,
      totalDurationSeconds: group.totalDurationSeconds,
      recentLiveAt: group.latestLiveAt,
      analysisCompleteCount: group.archives.filter((archive) =>
        analysisCompletionForArchive(archive, analysisCompletionBySource) >= 100
      ).length,
      skillDimensionCount: confidence.qualifiedDimensionCount,
      analysisGenerated: confidence.qualifiedDimensionCount >= 5,
      trainingStatus: trainingStatusFor(group),
      scoreConfidence: confidence.qualifiedDimensionCount > 0 ? `${confidence.coveragePercent}%` : null,
      isLive: group.archives.some((archive) => isArchiveActivelyRecording(archive, allRooms)),
    };
  });
  $: selectedConfidence = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? buildRatingConfidence(selectedActiveSkillRatingHistory, selectedStreamerGroup.streamerKey, "current")
    : null;
  $: selectedProfileHeader = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? {
      name: selectedStreamerGroup.streamerName,
      avatarIndex: selectedStreamerGroup.avatarIndex,
      virtualAvatar: virtualAvatarForGroup(selectedStreamerGroup, streamerVirtualAvatarSelections),
      sessionCount: selectedStreamerGroup.sessionCount,
      totalDurationLabel: formatDuration(selectedStreamerGroup.totalDurationSeconds),
      recentLiveLabel: selectedStreamerGroup.latestLiveAt
        ? formatDate(selectedStreamerGroup.latestLiveAt)
        : "暂无直播",
      trainingStatus: trainingStatusFor(selectedStreamerGroup),
      dataRangeLabel: selectedStreamerGroup.dataRange
        ? `${formatDate(selectedStreamerGroup.dataRange.from).split(" ")[0]} 至 ${formatDate(selectedStreamerGroup.dataRange.to).split(" ")[0]}`
        : "暂无数据",
      scoreConfidenceLabel: selectedConfidence && selectedConfidence.qualifiedDimensionCount > 0
        ? `${selectedConfidence.coveragePercent}%`
        : "数据不足",
      scoreConfidenceDetail: selectedConfidence?.label || "尚无可用于 AI 评估的分析证据",
    }
    : null;
  $: currentSkillRows = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? buildSkillScoreTable(selectedActiveSkillRatingHistory, selectedStreamerGroup.streamerKey, "current")
    : [];
  $: previousSkillRows = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? buildSkillScoreTable(selectedActiveSkillRatingHistory, selectedStreamerGroup.streamerKey, "previous")
    : [];
  $: profileScoreRows = STREAMER_SKILL_DIMENSIONS.map((dimension, index) => {
    const current = currentSkillRows[index];
    const previous = previousSkillRows[index];
    return {
      dimensionKey: dimension.key,
      dimensionLabel: dimension.label,
      currentScore: current?.score ?? null,
      previousScore: previous?.score ?? null,
      currentSampleCount: current?.sampleCount ?? 0,
      previousSampleCount: previous?.sampleCount ?? 0,
      basis: current?.basis || previous?.basis || "等待可用分析证据",
      evidenceCount: distinctEvidenceCount([...(current ? [current] : []), ...(previous ? [previous] : [])]),
    };
  });
  $: profileRadarSeries = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? buildStreamerRadarSeries(selectedActiveSkillRatingHistory, selectedStreamerGroup.streamerKey).map((series) => ({
      key: series.period,
      label: series.label,
      values: radarValueMap(series.values),
    }))
    : [];
  $: profileTrendPoints = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? buildSkillTrend(selectedActiveSkillRatingHistory, selectedStreamerGroup.streamerKey).map((point) => ({
      id: point.ratingId,
      periodLabel: `${SKILL_RATING_PERIOD_LABELS[point.period]} · v${point.version}`,
      dimensionLabel: point.label,
      score: point.score,
      sampleCount: point.sampleCount,
      confidenceLabel: point.score === null ? "样本不足" : "已达门槛",
    }))
    : [];
  $: profileTrainingIssues = currentSkillRows
    .filter((row) => row.score !== null && row.score < 75)
    .map((row) => ({
      id: row.ratingId || `training:${row.dimension}`,
      title: `${row.label}需要专项训练`,
      dimensionKey: row.dimension,
      dimensionLabel: row.label,
      detail: row.basis || "请结合已审核证据安排训练。",
      priorityLabel: row.score !== null && row.score < 60 ? "高优先级" : "待训练",
      evidenceCount: row.evidence.length,
    }));
  // A high dimension score does not prove that every supporting excerpt is an
  // excellent clip. Keep this section empty until evidence has a separate,
  // explicit human "excellent" classification.
  $: profileExcellentClips = [];
  $: profileKnowledgeAssets = [];
  $: selectedStreamerSourceKeys = new Set(
    (selectedStreamerGroup?.archives || []).map((archive) => streamerArchiveSourceKey(archive)),
  );
  $: selectedStreamerEvidenceCandidates = contentAnalysisData.evidenceCandidates.filter((evidence) =>
    selectedStreamerSourceKeys.has(evidence.sourceId)
  );
  $: selectedSkillProfileSetupArchives = (selectedStreamerGroup?.archives || []).filter((archive) =>
    selectedSkillProfileSetupSourceIds.has(streamerArchiveSourceKey(archive))
  );
  $: selectedSkillProfileSetupAnalyzedCount = selectedSkillProfileSetupArchives.filter((archive) =>
    analysisCompletionForArchive(archive) >= 100
  ).length;
  $: selectedSkillProfileSetupEvidenceCandidates = selectedStreamerEvidenceCandidates.filter((evidence) =>
    selectedSkillProfileSetupSourceIds.has(evidence.sourceId)
  );
  $: canUsePreparedTranscriptEvidence = skillProfilePreparation.readyCount >= MIN_SKILL_SCORE_SOURCES
    && selectedSkillProfileSetupArchives.length >= MIN_SKILL_SCORE_SOURCES;
  $: scopedRatingEvidenceCandidates = ratingEvidenceSourceScope.size
    ? selectedStreamerEvidenceCandidates.filter((evidence) => ratingEvidenceSourceScope.has(evidence.sourceId))
    : selectedStreamerEvidenceCandidates;
  $: skillProfileSetup = {
    totalSessionCount: selectedStreamerGroup?.sessionCount || 0,
    analyzedSessionCount: (selectedStreamerGroup?.archives || []).filter((archive) =>
      analysisCompletionForArchive(archive) >= 100
    ).length,
    evidenceCandidateCount: selectedStreamerEvidenceCandidates.length,
    qualifiedDimensionCount: currentSkillRows.filter((row) => row.score !== null).length,
    minimumDimensionsForRadar: 5,
    preparation: skillProfilePreparation,
  };
  $: selectedDimensionVersions = selectedStreamerGroup && !selectedStreamerGroup.isPending
    ? skillRatingHistory
      .filter((rating) =>
        rating.streamerKey === selectedStreamerGroup?.streamerKey
        && rating.dimension === evidenceDimension
      )
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt) || right.version - left.version)
    : [];
  $: if (
    archiveKindTab === "company"
    && selectedStreamerKey
    && !selectedStreamerGroup
    && !loading
  ) {
    void recoverMissingStreamerProfile();
  }

  function archiveKey(archive: RecordItem): string {
    return `${archive.platform}:${archive.room_id}:${archive.live_id}`;
  }

  async function resolveArchiveCoverUrl(archive: RecordItem): Promise<string> {
    if (!archive.cover?.trim()) return "";
    return get_static_url(
      "cache",
      `${archive.platform}/${archive.room_id}/${archive.live_id}/cover.jpg`,
    );
  }

  async function selectStreamerProfile(streamerKey: string): Promise<void> {
    refreshStreamerProfileStorage();
    returnFocusStreamerKey = streamerKey;
    selectedStreamerKey = streamerKey;
    streamerProfileTab = "streams";
    resetProfileFilters();
    currentPage = 1;
    clearArchiveSelection();
    applyFilters();
    await tick();
    document.querySelector<HTMLElement>(
      ".profile-workspace .quiet-button, .pending-profile-header .mac-btn",
    )?.focus();
  }

  async function backToStreamerDirectory(): Promise<void> {
    const focusKey = returnFocusStreamerKey || selectedStreamerKey;
    selectedStreamerKey = null;
    streamerProfileTab = "streams";
    resetProfileFilters();
    currentPage = 1;
    clearArchiveSelection();
    applyFilters();
    await tick();
    const target = [...document.querySelectorAll<HTMLElement>("[data-streamer-key]")]
      .find((element) => element.dataset.streamerKey === focusKey);
    target?.focus();
  }

  async function recoverMissingStreamerProfile(): Promise<void> {
    if (recoveringMissingStreamerProfile || !selectedStreamerKey) return;
    recoveringMissingStreamerProfile = true;
    const missingKey = selectedStreamerKey;
    selectedStreamerKey = null;
    returnFocusStreamerKey = null;
    streamerProfileTab = "streams";
    resetProfileFilters();
    currentPage = 1;
    clearArchiveSelection();
    applyFilters();
    transcriptStatus = "当前主播分组已变化，已安全返回主播列表。";
    await tick();
    const cards = [...document.querySelectorAll<HTMLElement>("[data-streamer-key]")];
    const target = cards.find((element) => element.dataset.streamerKey === missingKey)
      || cards[0]
      || document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
    target?.focus();
    recoveringMissingStreamerProfile = false;
  }

  function isRatingVersionActive(
    version: SkillRatingVersion,
    sourceKeys: ReadonlySet<string>,
  ): boolean {
    return isSkillRatingVersionWithinSources(version, sourceKeys);
  }

  function switchArchiveKind(kind: "company" | "competitor"): void {
    archiveKindTab = kind;
    selectedStreamerKey = null;
    returnFocusStreamerKey = null;
    streamerProfileTab = "streams";
    resetProfileFilters();
    currentPage = 1;
    clearArchiveSelection();
    applyFilters();
  }

  function handleProfileTab(event: CustomEvent<{ tab: StreamerProfileTab }>): void {
    streamerProfileTab = event.detail.tab;
  }

  function openSkillProfileSetup(): void {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending) return;
    const recommendedSources = selectedStreamerGroup.archives
      .slice(0, 5)
      .map((archive) => streamerArchiveSourceKey(archive));
    selectedSkillProfileSetupSourceIds = new Set(recommendedSources);
    skillProfileSetupStep = canUsePreparedTranscriptEvidence ? 2 : 1;
    skillAiError = "";
    skillAiResultSummary = "";
    showSkillProfileSetupModal = true;
  }

  function toggleSkillProfileSetupArchive(sourceId: string): void {
    const next = new Set(selectedSkillProfileSetupSourceIds);
    if (next.has(sourceId)) next.delete(sourceId);
    else next.add(sourceId);
    selectedSkillProfileSetupSourceIds = next;
  }

  function closeSkillProfileSetup(): void {
    showSkillProfileSetupModal = false;
    skillProfileSetupStep = 1;
    skillAiError = "";
  }

  async function prepareSkillProfileTranscript(archive: RecordItem): Promise<"ready" | "failed"> {
    let existing = "";
    try {
      existing = await invoke<string>("get_archive_subtitle", {
        platform: archive.platform,
        roomId: String(archive.room_id),
        liveId: String(archive.live_id),
      });
    } catch {
      // A missing subtitle is the expected first-run state. Continue to the
      // refresh command below so the backend can create its background task.
      existing = "";
    }
    if (existing.trim()) return "ready";
    try {
      const result = await invoke<TranscriptRefreshResult>("refresh_archive_subtitle", {
        platform: archive.platform,
        roomId: String(archive.room_id),
        liveId: String(archive.live_id),
        force: false,
      });
      return result.subtitle.trim() ? "ready" : "failed";
    } catch {
      return "failed";
    }
  }

  function transcriptTextForSkillEvidence(subtitle: string): string {
    const compact = subtitle
      .replace(/^\d+\s*$/gmu, "")
      .replace(/^\d{1,2}:\d{2}:\d{2}[,.]\d{3}\s*-->.*$/gmu, "")
      .replace(/\s+/gu, " ")
      .trim();
    return compact.length > 2800 ? `${compact.slice(0, 2800)}…` : compact;
  }

  async function collectSkillProfileAiEvidence(): Promise<SkillEvidence[]> {
    const analyzed = [...selectedSkillProfileSetupEvidenceCandidates];
    const sourceIds = new Set(analyzed.map((item) => item.sourceId));
    const transcriptEvidence = await Promise.all(selectedSkillProfileSetupArchives
      .filter((archive) => !sourceIds.has(streamerArchiveSourceKey(archive)))
      .map(async (archive): Promise<SkillEvidence | null> => {
        try {
          const subtitle = await invoke<string>("get_archive_subtitle", {
            platform: archive.platform,
            roomId: String(archive.room_id),
            liveId: String(archive.live_id),
          });
          const transcript = transcriptTextForSkillEvidence(subtitle);
          if (!transcript) return null;
          const sourceId = streamerArchiveSourceKey(archive);
          return {
            id: `${sourceId}#transcript`,
            sourceId,
            candidateId: "transcript",
            dimension: null,
            summary: `${archive.title || archive.live_id}的录播字幕原文`,
            basis: transcript,
            reviewStatus: "pending_review",
            humanConfirmed: false,
            reviewedBy: null,
            reviewedAt: null,
            sourceUpdatedAt: archive.created_at || null,
          };
        } catch {
          return null;
        }
      }));
    return [...analyzed, ...transcriptEvidence.filter((item): item is SkillEvidence => Boolean(item))];
  }

  async function runSkillProfilePreparation(
    archivesToPrepare: readonly RecordItem[],
    runId: number,
  ): Promise<void> {
    const queue = [...archivesToPrepare];
    const workerCount = Math.min(2, queue.length);
    let readyCount = 0;
    let failedCount = 0;
    const updateProgress = () => {
      if (runId !== skillProfilePreparationRunId) return;
      skillProfilePreparation = {
        status: queue.length || readyCount + failedCount < archivesToPrepare.length ? "preparing" : failedCount ? "partial" : "ready",
        totalCount: archivesToPrepare.length,
        readyCount,
        failedCount,
      };
    };
    const worker = async () => {
      while (queue.length) {
        const archive = queue.shift();
        if (!archive) return;
        const result = await prepareSkillProfileTranscript(archive);
        if (result === "ready") readyCount += 1;
        else failedCount += 1;
        updateProgress();
      }
    };
    await Promise.all(Array.from({ length: workerCount }, worker));
    if (runId !== skillProfilePreparationRunId) return;
    transcriptStatus = failedCount
      ? `已准备 ${readyCount}/${archivesToPrepare.length} 场录播；${failedCount} 场未能生成文稿，可稍后重试。`
      : `已准备 ${readyCount} 场录播文稿。现在可继续由 AI 基于原始文稿生成能力画像。`;
    refreshStreamerProfileStorage();
    if (!failedCount && readyCount >= MIN_SKILL_SCORE_SOURCES) {
      skillProfileSetupStep = 2;
      showSkillProfileSetupModal = true;
    }
  }

  function establishSkillProfile(): void {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending) return;
    if (selectedSkillProfileSetupEvidenceCandidates.length || canUsePreparedTranscriptEvidence) {
      skillProfileSetupStep = 3;
      void runSkillProfileAiEvaluation();
      return;
    }
    const archivesToPrepare = selectedSkillProfileSetupArchives.filter((archive) => !isArchiveRecording(archive));
    if (!archivesToPrepare.length) {
      skillAiError = "所选录播仍在直播中，结束录制后才能生成能力画像。";
      return;
    }
    const runId = ++skillProfilePreparationRunId;
    skillProfilePreparation = {
      status: "preparing",
      totalCount: archivesToPrepare.length,
      readyCount: 0,
      failedCount: 0,
    };
    transcriptStatus = `已建立${selectedStreamerGroup.streamerName}的基础档案，正在后台准备 ${archivesToPrepare.length} 场录播文稿。`;
    closeSkillProfileSetup();
    void runSkillProfilePreparation(archivesToPrepare, runId);
  }

  async function runSkillProfileAiEvaluation(): Promise<void> {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending || skillAiEvaluating) return;
    skillAiEvaluating = true;
    skillAiError = "";
    skillAiResultSummary = "";
    try {
      const aiEvidence = await collectSkillProfileAiEvidence();
      if (aiEvidence.length < MIN_SKILL_SCORE_SOURCES) {
        skillAiResultSummary = `已完成可用证据检查；当前只有 ${aiEvidence.length} 个可读取的录播来源，暂不生成猜测分数。`;
        transcriptStatus = `${selectedStreamerGroup.streamerName}的能力画像已建立；补齐至少 ${MIN_SKILL_SCORE_SOURCES} 场可读取录播后，可继续生成真实评分。`;
        skillProfileSetupStep = 4;
        return;
      }
      const response = await invoke<string>("minimax_chat", {
        systemPrompt: streamerSkillProfileSystemPrompt(),
        messages: [{
          role: "user",
          content: buildStreamerSkillProfileUserMessage(
            selectedStreamerGroup.streamerName,
            aiEvidence,
          ),
        }],
      });
      const ratings = parseStreamerSkillAiRatings(response, aiEvidence);
      const candidateById = new Map(aiEvidence.map((item) => [item.id, item]));
      const now = new Date().toISOString();
      let nextHistory = skillRatingHistory;
      let addedCount = 0;
      for (const rating of ratings) {
        if (rating.score === null) continue;
        const evidence = rating.evidenceIds
          .map((id) => candidateById.get(id))
          .filter((item): item is SkillEvidence => Boolean(item))
          .map((item) => assessSkillEvidenceWithAi(item, rating.dimension, now));
        const appended = appendAiSkillRatingVersion(nextHistory, {
          streamerKey: selectedStreamerGroup.streamerKey,
          period: "current",
          dimension: rating.dimension,
          score: rating.score,
          basis: rating.basis,
          evidence,
          reviewedBy: "AI 能力评估",
          createdAt: now,
        });
        nextHistory = appended.history;
        addedCount += 1;
      }
      if (!addedCount) {
        skillAiResultSummary = "AI 已完成证据检查，但没有维度满足跨两个不同场次来源的评分门槛；已保留“数据不足”。";
        transcriptStatus = `${selectedStreamerGroup.streamerName}的能力画像已建立；当前没有满足评分门槛的维度。`;
        skillProfileSetupStep = 4;
        return;
      }
      skillRatingHistory = nextHistory;
      localStorage.setItem(SKILL_RATINGS_STORAGE_KEY, JSON.stringify(nextHistory));
      skillAiResultSummary = `AI 已基于 ${aiEvidence.length} 个可追溯录播来源形成 ${addedCount} 个维度评分。`;
      transcriptStatus = skillAiResultSummary;
      skillProfileSetupStep = 4;
    } catch (error) {
      skillAiError = error instanceof Error ? error.message : String(error);
    } finally {
      skillAiEvaluating = false;
    }
  }

  function openSkillProfileSetupStreams(): void {
    closeSkillProfileSetup();
    streamerProfileTab = "streams";
  }

  function beginSkillRating(dimensionKey: StreamerSkillDimensionKey): void {
    ratingDimension = dimensionKey;
    ratingPeriod = "current";
    ratingScore = 80;
    ratingBasis = "";
    ratingReviewConfirmed = false;
    selectedRatingEvidenceIds = new Set();
    ratingError = "";
    showSkillRatingModal = true;
  }

  function openSkillProfileSetupRating(dimensionKey: StreamerSkillDimensionKey): void {
    if (!selectedSkillProfileSetupEvidenceCandidates.length) return;
    ratingEvidenceSourceScope = new Set(selectedSkillProfileSetupSourceIds);
    returnToSkillProfileSetup = true;
    showSkillProfileSetupModal = false;
    beginSkillRating(dimensionKey);
  }

  function closeSkillRating(): void {
    showSkillRatingModal = false;
    ratingEvidenceSourceScope = new Set();
    if (returnToSkillProfileSetup) {
      returnToSkillProfileSetup = false;
      skillProfileSetupStep = 3;
      showSkillProfileSetupModal = true;
    }
  }

  function openSkillRating(event: CustomEvent<{ dimensionKey: string; dimensionLabel: string }>): void {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending) return;
    returnToSkillProfileSetup = false;
    ratingEvidenceSourceScope = new Set();
    beginSkillRating(event.detail.dimensionKey as StreamerSkillDimensionKey);
  }

  function toggleRatingEvidence(evidenceId: string): void {
    const next = new Set(selectedRatingEvidenceIds);
    if (next.has(evidenceId)) next.delete(evidenceId);
    else next.add(evidenceId);
    selectedRatingEvidenceIds = next;
  }

  function saveSkillRating(): void {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending) return;
    ratingError = "";
    const reviewer = ratingReviewer.trim();
    if (!reviewer) {
      ratingError = "请填写人工审核人";
      return;
    }
    if (!ratingReviewConfirmed) {
      ratingError = "请确认已逐条查看所选直播或切片证据";
      return;
    }
    const candidates = scopedRatingEvidenceCandidates.filter((evidence) =>
      selectedRatingEvidenceIds.has(evidence.id)
    );
    if (candidates.length === 0) {
      ratingError = "至少选择 1 条待审核证据";
      return;
    }

    const now = new Date().toISOString();
    try {
      const evidence = candidates.map((candidate) => confirmSkillEvidence(candidate, {
        dimension: ratingDimension,
        reviewedBy: reviewer,
        reviewedAt: now,
      }));
      const result = appendSkillRatingVersion(skillRatingHistory, {
        streamerKey: selectedStreamerGroup.streamerKey,
        period: ratingPeriod,
        dimension: ratingDimension,
        score: Number(ratingScore),
        basis: ratingBasis,
        evidence,
        reviewedBy: reviewer,
        createdAt: now,
      });
      skillRatingHistory = result.history;
      localStorage.setItem(SKILL_RATINGS_STORAGE_KEY, JSON.stringify(result.history));
      localStorage.setItem(SKILL_REVIEWER_STORAGE_KEY, reviewer);
      closeSkillRating();
      const sourceCount = new Set(evidence.map((item) => item.sourceId.trim().toLowerCase())).size;
      transcriptStatus = `已保存 ${STREAMER_SKILL_DIMENSIONS.find((item) => item.key === ratingDimension)?.label || "能力"}评分 v${result.added.version}。${sourceCount < MIN_SKILL_SCORE_SOURCES ? `当前只有 ${sourceCount} 个不同来源，仍显示数据不足。` : ""}`;
    } catch (error) {
      ratingError = error instanceof Error ? error.message : String(error);
    }
  }

  function newestActiveEvidenceForDimension(dimensionKey: StreamerSkillDimensionKey): SkillEvidence | null {
    const versions = selectedActiveSkillRatingHistory
      .filter((version) => version.dimension === dimensionKey)
      .filter((version) => isRatingVersionActive(version, selectedStreamerSourceKeys))
      .sort((left, right) => right.version - left.version || right.createdAt.localeCompare(left.createdAt));
    for (const version of versions) {
      const evidence = version.evidence.find((item) => selectedStreamerSourceKeys.has(item.sourceId));
      if (evidence) return evidence;
    }
    return null;
  }

  function openSkillEvidence(event: CustomEvent<{ dimensionKey: string; dimensionLabel: string }>): void {
    evidenceDimension = event.detail.dimensionKey as StreamerSkillDimensionKey;
    evidenceDimensionLabel = event.detail.dimensionLabel;
    const evidence = newestActiveEvidenceForDimension(evidenceDimension);
    if (evidence) {
      openEvidenceSource(evidence);
      return;
    }
    showSkillEvidenceModal = true;
  }

  function evidenceSourceTitle(sourceId: string): string {
    const summary = contentAnalysisData.sources.find((source) => source.sourceKey === sourceId);
    return summary?.sourceTitle || sourceId;
  }

  function evidenceTranscriptSearchText(evidence: SkillEvidence): string {
    const quoted = Array.from(evidence.basis.matchAll(/[“"‘']([^“"”’']{3,96})[”"’']/gu))
      .map((match) => match[1].trim())
      .sort((left, right) => right.length - left.length);
    const fallback = evidence.basis
      .replace(/^录播字幕原文[：:]\s*/u, "")
      .replace(/\s+/gu, " ")
      .trim();
    return (quoted[0] || fallback || evidence.summary).slice(0, 96);
  }

  function openEvidenceSource(evidence: SkillEvidence): void {
    const archive = selectedStreamerGroup?.archives.find((item) =>
      streamerArchiveSourceKey(item) === evidence.sourceId
    );
    if (!archive) {
      transcriptStatus = "该证据的原始直播已不在当前档案中。";
      return;
    }
    try {
      sessionStorage.setItem("bsr:evidence-transcript-focus:v1", JSON.stringify({
        sourceKey: evidence.sourceId,
        query: evidenceTranscriptSearchText(evidence),
        summary: evidence.summary,
      }));
    } catch {
      // The transcript page can still open when storage is unavailable.
    }
    showSkillEvidenceModal = false;
    openCompanyDealReview(archive);
  }

  async function openStreamerTraining(): Promise<void> {
    if (!selectedStreamerGroup || selectedStreamerGroup.isPending) return;
    const task = getStreamerTrainingTask(selectedStreamerGroup.streamerName);
    if (!task) {
      transcriptStatus = `${selectedStreamerGroup.streamerName}尚未配置对应训练任务。`;
      return;
    }
    try {
      await open(task.url);
      transcriptStatus = `已打开训练任务：${task.title}`;
    } catch (error) {
      try {
        await navigator.clipboard.writeText(task.title);
        transcriptStatus = `无法直接打开训练任务，任务名称已复制：${task.title}`;
      } catch {
        transcriptStatus = `无法打开训练任务：${String(error)}`;
      }
    }
  }

  function replaceArchive(updated: RecordItem): void {
    const current = allArchives.find(
      (archive) => archive.live_id === updated.live_id,
    );
    const normalized = {
      ...updated,
      cover: current?.cover || updated.cover,
    };
    allArchives = allArchives.map((archive) =>
      archive.live_id === normalized.live_id ? normalized : archive,
    );
    updatePagination();
  }

  async function detectArchiveAnchor(
    archive: RecordItem,
    force = false,
  ): Promise<RecordItem | null> {
    if (isImportedArchive(archive)) {
      const videoId = getImportedVideoId(archive);
      if (!videoId) return null;
      replaceArchive({
        ...archive,
        anchor_detection_status: "running",
        anchor_detection_error: "",
      });
      try {
        const updated = await invoke<VideoItem>("detect_video_anchor", {
          videoId,
          force,
        });
        const merged = mergeVideoIntoImportedArchive(archive, updated);
        replaceArchive(merged);
        return merged;
      } catch (error) {
        replaceArchive({
          ...archive,
          anchor_detection_status: "failed",
          anchor_detection_error: String(error),
        });
        return null;
      }
    }

    replaceArchive({
      ...archive,
      anchor_detection_status: "running",
      anchor_detection_error: "",
    });
    try {
      const updated = await invoke<RecordItem>("detect_archive_anchor", {
        platform: archive.platform,
        roomId: String(archive.room_id),
        liveId: String(archive.live_id),
        force,
      });
      replaceArchive(updated);
      return updated;
    } catch (error) {
      replaceArchive({
        ...archive,
        anchor_detection_status: "failed",
        anchor_detection_error: String(error),
      });
      return null;
    }
  }

  async function runPendingAnchorDetections(): Promise<void> {
    if (anchorQueueRunning) return;
    const pending = allArchives.filter(
      (archive) =>
        !isArchiveRecording(archive) &&
        archive.length > 0 &&
        shouldAutoDetectAnchor(archive),
    );
    if (pending.length === 0) return;

    anchorQueueRunning = true;
    try {
      for (const archive of pending) {
        const updated = await detectArchiveAnchor(
          archive,
          archive.anchor_detection_status === "running",
        );
        if (updated?.anchor_detection_error.includes("API Key")) break;
      }
    } finally {
      anchorQueueRunning = false;
    }
  }

  function openArchiveAnchorDialog(archive: RecordItem): void {
    anchorToEdit = archive;
    editingAnchorName = archive.anchor_name || "";
    anchorEditError = "";
  }

  function closeArchiveAnchorDialog(): void {
    anchorToEdit = null;
    editingAnchorName = "";
    anchorEditError = "";
  }

  async function saveArchiveAnchorName(): Promise<void> {
    if (!anchorToEdit) return;
    anchorEditError = "";
    try {
      if (isImportedArchive(anchorToEdit)) {
        const videoId = getImportedVideoId(anchorToEdit);
        if (!videoId) return;
        const updated = await invoke<VideoItem>("save_video_anchor_manual", {
          videoId,
          anchorName: editingAnchorName,
        });
        replaceArchive(mergeVideoIntoImportedArchive(anchorToEdit, updated));
        closeArchiveAnchorDialog();
        return;
      }
      const updated = await invoke<RecordItem>("save_archive_anchor_manual", {
        liveId: String(anchorToEdit.live_id),
        anchorName: editingAnchorName,
      });
      replaceArchive(updated);
      closeArchiveAnchorDialog();
    } catch (error) {
      anchorEditError = String(error);
    }
  }

  function isTranscriptReady(archive: RecordItem): boolean {
    return transcriptReady.has(archiveKey(archive));
  }

  function generateArchiveTranscript(archive: RecordItem) {
    if (isArchiveRecording(archive) || transcriptGenerating.has(archiveKey(archive))) return;
    factCardArchive = archive;
    factCardError = "";
    const saved = localStorage.getItem(`archive-fact-card:${archive.room_id}`);
    if (saved) {
      try {
        const card = JSON.parse(saved);
        factProducts = (card.products || []).join("、");
        factPrices = (card.prices || []).join("、");
        factConditions = (card.conditions || []).join("、");
        factLinks = (card.link_numbers || []).join("、");
        factInventory = (card.inventory_phrases || []).join("、");
        factAliases = Object.entries(card.aliases || {}).map(([from, to]) => `${from}=${to}`).join("\n");
      } catch {
        // Ignore a damaged local draft and show an empty optional card.
      }
    }
    showFactCardModal = true;
  }

  function listValues(value: string): string[] {
    return value.split(/[、,，\n]/).map((item) => item.trim()).filter(Boolean);
  }

  async function startTranscriptWithFactCard() {
    if (!factCardArchive) return;
    const archive = factCardArchive;
    const aliases: Record<string, string> = {};
    for (const line of factAliases.split(/\n/).map((item) => item.trim()).filter(Boolean)) {
      const separator = line.includes("=") ? "=" : "：";
      const [from, to] = line.split(separator).map((item) => item.trim());
      if (!from || !to) {
        factCardError = `别名“${line}”格式不正确，请写成：二四七零二点八=24-70 F2.8`;
        return;
      }
      aliases[from] = to;
    }
    const factCard = {
      products: listValues(factProducts),
      aliases,
      prices: listValues(factPrices).map(Number).filter(Number.isFinite),
      conditions: listValues(factConditions),
      link_numbers: listValues(factLinks).map(Number).filter(Number.isFinite),
      inventory_phrases: listValues(factInventory),
    };
    try {
      await invoke("save_archive_fact_card", {
        platform: archive.platform,
        roomId: String(archive.room_id),
        liveId: String(archive.live_id),
        factCard,
      });
      localStorage.setItem(`archive-fact-card:${archive.room_id}`, JSON.stringify(factCard));
    } catch (error) {
      factCardError = `保存参数卡失败：${error}`;
      return;
    }
    showFactCardModal = false;
    const key = archiveKey(archive);
    transcriptGenerating = new Set([...transcriptGenerating, key]);
    transcriptStatus = `正在助手中为《${archive.title}》生成整场逐字稿...`;
    window.dispatchEvent(new CustomEvent("bsr:transcribe-archive", { detail: archive }));
  }

  onMount(() => {
    const handleReady = (event: Event) => {
      const archive = (event as CustomEvent).detail as RecordItem;
      const key = archiveKey(archive);
      transcriptReady = new Set([...transcriptReady, key]);
      localStorage.setItem("archive-transcript-ready", JSON.stringify([...transcriptReady]));
      const next = new Set(transcriptGenerating);
      next.delete(key);
      transcriptGenerating = next;
      transcriptStatus = `《${archive.title}》逐字稿已完成，现在可以点击“分析片段”。`;
    };
    const handleFailed = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      const key = archiveKey(detail.archive);
      const next = new Set(transcriptGenerating);
      next.delete(key);
      transcriptGenerating = next;
      transcriptStatus = `逐字稿生成失败：${detail.error}`;
    };
    window.addEventListener("bsr:archive-transcript-ready", handleReady);
    window.addEventListener("bsr:archive-transcript-failed", handleFailed);
    return () => {
      window.removeEventListener("bsr:archive-transcript-ready", handleReady);
      window.removeEventListener("bsr:archive-transcript-failed", handleFailed);
    };
  });

  onMount(() => {
    refreshStreamerProfileStorage();
    const refresh = () => {
      refreshStreamerProfileStorage();
      applyFilters();
    };
    window.addEventListener("focus", refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      window.removeEventListener("storage", refresh);
    };
  });

  function isArchiveRecording(archive: RecordItem): boolean {
    return isArchiveActivelyRecording(archive, allRooms);
  }

  function pendingIdentityStatusLabel(archive: RecordItem): string {
    const status = String(archive.anchor_detection_status || "").trim().toLowerCase();
    const error = String(archive.anchor_detection_error || "").trim().toLowerCase();
    if (
      status === "conflict"
      || status === "ambiguous"
      || /冲突|不一致|歧义|conflict|inconsisten|ambiguous/u.test(error)
    ) return "身份冲突";
    if (String(archive.anchor_confidence || "").trim().toLowerCase() === "low") return "低置信识别";
    if (status === "running") return "正在识别";
    if (status === "failed") return "未识别";
    if (status === "pending" || status === "not_requested") return "等待识别";
    return "身份未确认";
  }

  function hasActiveRecording(): boolean {
    return allRooms.some((room) => room.recording);
  }

  async function refreshActiveRecordingStats() {
    try {
      const recorderList: RecorderList = await invoke("get_recorder_list");
      allRooms = recorderList.recorders || [];
      const recordingRooms = allRooms.filter((room) => room.recording);
      if (recordingRooms.length === 0) {
        return;
      }

      let changed = false;
      for (const room of recordingRooms) {
        const roomArchives = await invoke<RecordItem[]>("get_archives", {
          roomId: room.room_info.room_id,
          offset: 0,
          limit: 20,
        });

        for (const updated of roomArchives) {
          const index = allArchives.findIndex((archive) => archiveKey(archive) === archiveKey(updated));
          if (index >= 0) {
            const current = allArchives[index];
            const refreshKeys: Array<keyof RecordItem> = [
              "title",
              "length",
              "size",
              "anchor_name",
              "anchor_source",
              "anchor_confidence",
              "anchor_detection_status",
              "anchor_detection_error",
              "anchor_detected_at",
              "archive_kind",
              "classification_source",
            ];
            if (refreshKeys.some((key) => current[key] !== updated[key])) {
              allArchives[index] = {
                ...current,
                ...Object.fromEntries(refreshKeys.map((key) => [key, updated[key]])),
              };
              changed = true;
            }
            continue;
          }

          if (String(updated.live_id) !== String(room.live_id)) {
            continue;
          }

          updated.cover = await resolveArchiveCoverUrl(updated);
          allArchives = [updated, ...allArchives];
          changed = true;
        }
      }

      if (changed) {
        allArchives = [...allArchives];
        allArchives.sort(
          (a, b) =>
            new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
        );
        totalCount = allArchives.length;
        applyFilters();
      }
    } catch (error) {
      console.warn("Failed to refresh active recording stats:", error);
    }
  }

  function analyzeWholeArchive(archive: RecordItem) {
    if (isArchiveRecording(archive)) return;
    if (isImportedArchive(archive)) {
      void openImportedArchiveAnalysis(archive, "legacy");
      return;
    }
    window.dispatchEvent(new CustomEvent("bsr:open-archive-analysis", { detail: archive }));
  }

  onMount(async () => {
    // 从本地存储恢复分页大小设置
    const savedPageSize = localStorage.getItem("archive-page-size");
    if (savedPageSize && pageSizeOptions.includes(parseInt(savedPageSize))) {
      pageSize = parseInt(savedPageSize);
    }

    await loadArchives();

    // Keep the page fresh while waiting for the first index row, and while
    // an active recording is still accumulating duration/size in the database.
    statusRefreshTimer = setInterval(() => {
      if (isLoading) {
        return;
      }
      if (allArchives.length === 0) {
        void loadArchives();
        return;
      }
      if (hasActiveRecording()) {
        void refreshActiveRecordingStats();
      } else {
        void runPendingAnchorDetections();
      }
    }, 5000);
  });

  onDestroy(() => {
    if (statusRefreshTimer) clearInterval(statusRefreshTimer);
  });

  const ARCHIVE_LOAD_BATCH_SIZE = 100;
  const ARCHIVE_LOAD_MAX_PAGES = 500;

  async function loadAllRoomArchives(roomId: string): Promise<RecordItem[]> {
    const loaded: RecordItem[] = [];
    const seen = new Set<string>();
    let offset = 0;

    for (let page = 0; page < ARCHIVE_LOAD_MAX_PAGES; page += 1) {
      const batch = await invoke<RecordItem[]>("get_archives", {
        roomId,
        offset,
        limit: ARCHIVE_LOAD_BATCH_SIZE,
      });
      if (!Array.isArray(batch) || batch.length === 0) break;

      let added = 0;
      for (const archive of batch) {
        const key = archiveKey(archive);
        if (seen.has(key)) continue;
        seen.add(key);
        loaded.push(archive);
        added += 1;
      }

      if (batch.length < ARCHIVE_LOAD_BATCH_SIZE || added === 0) break;
      offset += batch.length;
    }

    return loaded;
  }

  /**
   * 初始化加载所有录播数据
   */
  async function loadArchives() {
    if (isLoading) return;

    isLoading = true;
    loading = true;
    loadError = "";

    try {
      // 获取所有直播间列表
      const recorderList: RecorderList = await invoke("get_recorder_list");
      allRooms = recorderList.recorders || [];

      // 收集所有直播间，用账号名/直播间标题+直播间号展示
      roomAccountById = new Map(
        allRooms
          .map((room: RecorderInfo) => {
            const id = String(room.room_info.room_id);
            const name = room.user_info?.user_name?.trim() || "";
            return [id, name] as const;
          })
          .filter((entry) => entry[1]),
      );
      roomOptions = allRooms
        .map((room: RecorderInfo) => {
          const id = room.room_info.room_id;
          const name =
            room.user_info?.user_name?.trim() ||
            room.room_info?.room_title?.trim() ||
            "";
          const label = name ? `${name} (${id})` : id;
          return { id, label };
        })
        .sort((a, b) => a.label.localeCompare(b.label));

      // 加载所有录播数据
      const roomResults = await Promise.all(allRooms.map(async (room) => {
        try {
          let roomArchives = await loadAllRoomArchives(room.room_info.room_id);

          // 账号/店铺名是归档归类的主要依据：标题不含“金典拍拍”时，
          // 也要识别为公司录播；用户手工调整过的归类不会被此规则覆盖。
          const accountName = `${room.user_info?.user_name || ""} ${room.room_info?.room_title || ""}`;
          roomArchives = roomArchives.map((archive) => applyLoadedArchiveClassification(archive, accountName));
          if (roomArchives.length > 0) {
            void invoke<RecordItem[]>("auto_classify_archive_kinds", {
                archives: roomArchives.map((archive) => ({
                  liveId: archive.live_id,
                  accountName,
                })),
              }).then((reclassified) => {
                const latest = preferAutoClassifiedArchives(roomArchives, reclassified);
                const byId = new Map(latest.map((archive) => [archive.live_id, archive]));
                allArchives = allArchives.map((archive) => byId.get(archive.live_id) || archive);
                applyFilters();
              }).catch((error) => console.warn("Automatic archive classification unavailable:", error));
          }

          // 处理封面
          for (const archive of roomArchives) {
            archive.cover = await resolveArchiveCoverUrl(archive);
          }

          return roomArchives;
        } catch (error) {
          console.warn(`Failed to load archives for room ${room}:`, error);
          return [] as RecordItem[];
        }
      }));
      allArchives = roomResults.flat();

      const importedVideos = (await invoke<VideoItem[]>("get_all_videos"))
        .filter((video) => video.platform === "imported" || video.room_id === "bsr:import");
      for (const video of importedVideos) {
        if (video.cover) {
          video.cover = await get_static_url("output", video.cover);
        }
      }
      allArchives = [...allArchives, ...importedVideos.map(videoToImportedArchive)];

      // 按创建时间排序
      allArchives.sort((a, b) => {
        return (
          new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
        );
      });

      totalCount = allArchives.length;
      updatePagination();
      void runPendingAnchorDetections();
      void loadLiveDashboardBindings().then(() => autoResolveLiveDashboardBindings());
    } catch (error) {
      console.error("Failed to load archives:", error);
      loadError = "加载失败，请重试";
    } finally {
      isLoading = false;
      loading = false;
    }
  }

  /**
   * 更新分页信息和当前页数据
   */
  function updatePagination() {
    totalPages = Math.ceil(totalCount / pageSize);
    if (currentPage > totalPages) {
      currentPage = totalPages || 1;
    }
    applyFilters();
  }

  /**
   * 跳转到指定页面
   */
  function goToPage(page: number) {
    if (page < 1 || page > totalPages || page === currentPage) return;
    currentPage = page;
    applyFilters();
  }

  /**
   * 上一页
   */
  function prevPage() {
    if (currentPage > 1) {
      currentPage--;
      applyFilters();
    }
  }

  /**
   * 下一页
   */
  function nextPage() {
    if (currentPage < totalPages) {
      currentPage++;
      applyFilters();
    }
  }

  /**
   * 更改页面大小
   */
  function changePageSize(newPageSize: number) {
    pageSize = newPageSize;
    currentPage = 1; // 重置到第一页

    // 保存到本地存储
    localStorage.setItem("archive-page-size", newPageSize.toString());

    applyFilters();
  }

  function applyFilters() {
    let filtered: RecordItem[] = [...allArchives];

    filtered = filtered.filter((archive) => (archive.archive_kind || "competitor") === archiveKindTab);

    if (archiveKindTab === "company" && selectedStreamerKey) {
      const group = findStreamerArchiveGroup(
        groupCompanyArchivesByStreamer(filtered),
        selectedStreamerKey,
      );
      filtered = group ? [...group.archives] : [];
      filtered = filterStreamerArchives(filtered, profileFilters(), {
        runtimeRecorders: allRooms,
        analysisCompletionBySource,
      });
    }

    // Apply room filter
    if (selectedRoomId !== null) {
      filtered = filtered.filter(
        (archive) =>
          !isImportedArchive(archive) && archive.room_id === selectedRoomId,
      );
    }

    // Apply sorting
    filtered.sort((a, b) => {
      let aValue: any, bValue: any;

      switch (sortBy) {
        case "title":
          aValue = a.title.toLowerCase();
          bValue = b.title.toLowerCase();
          break;
        case "length":
          aValue = a.length;
          bValue = b.length;
          break;
        case "size":
          aValue = a.size;
          bValue = b.size;
          break;
        case "created_at":
          aValue = new Date(a.created_at);
          bValue = new Date(b.created_at);
          break;
        case "room_id":
          aValue = a.room_id;
          bValue = b.room_id;
          break;
        case "platform":
          aValue = (a.platform || "").toLowerCase();
          bValue = (b.platform || "").toLowerCase();
          break;
        default:
          aValue = new Date(a.created_at);
          bValue = new Date(b.created_at);
      }

      if (sortOrder === "asc") {
        return aValue > bValue ? 1 : -1;
      } else {
        return aValue < bValue ? 1 : -1;
      }
    });

    // 更新总数和分页信息
    totalCount = filtered.length;
    totalPages = Math.ceil(totalCount / pageSize);

    // 确保当前页在有效范围内
    if (currentPage > totalPages && totalPages > 0) {
      currentPage = totalPages;
    }

    // Apply pagination
    const startIndex = (currentPage - 1) * pageSize;
    const endIndex = startIndex + pageSize;
    filteredArchives = filtered.slice(startIndex, endIndex);

    selectedArchives = pruneArchiveSelection(
      selectedArchives,
      filtered.map((archive) => archive.live_id)
    );
    if (
      lastSelectedArchiveId &&
      !filteredArchives.some(
        (archive) => archive.live_id === lastSelectedArchiveId
      )
    ) {
      lastSelectedArchiveId = null;
    }

    // 更新archives用于其他功能
    archives = filtered;
  }

  async function openImportedArchiveAnalysis(
    archive: RecordItem,
    mode: "legacy" | "company_deal",
  ): Promise<void> {
    const videoId = getImportedVideoId(archive);
    if (!videoId) {
      loadError = "无法打开分析：导入录播缺少视频 ID。";
      alert(loadError);
      return;
    }
    try {
      const video = await invoke<VideoItem>("get_video", { id: videoId });
      window.dispatchEvent(new CustomEvent("bsr:open-video-analysis", {
        detail: { video, analysisMode: mode },
      }));
    } catch (error: any) {
      loadError = error?.message || String(error);
      alert(`无法打开分析页：${loadError}`);
    }
  }

  function openCompanyDealReview(archive: RecordItem): void {
    if (!canOpenCompanyDealReview(archive.archive_kind)) {
      alert("只有公司录播才能打开成交订单时间轴分析。");
      return;
    }
    if (isArchiveRecording(archive)) {
      alert("录制结束后才能分析整场。");
      return;
    }
    if (isImportedArchive(archive)) {
      void openImportedArchiveAnalysis(archive, "company_deal");
      return;
    }
    window.dispatchEvent(new CustomEvent("bsr:open-company-deal-review", { detail: archive }));
  }

  async function changeArchiveKind(archive: RecordItem, archiveKind: "company" | "competitor"): Promise<void> {
    try {
      if (isImportedArchive(archive)) {
        const videoId = getImportedVideoId(archive);
        if (!videoId) return;
        const video = await invoke<VideoItem>("get_video", { id: videoId });
        await invoke("update_video_note", {
          id: videoId,
          note: buildImportedVideoNote(archiveKind, video.note),
        });
        replaceArchive({ ...archive, archive_kind: archiveKind });
        applyFilters();
        return;
      }
      const updated = await invoke<RecordItem>("set_archive_kind", { liveId: archive.live_id, archiveKind });
      replaceArchive(updated);
    } catch (error: any) {
      loadError = error?.message || String(error);
    }
  }

  function formatSize(size: number) {
    if (size < 1024) {
      return `${size} B`;
    } else if (size < 1024 * 1024) {
      return `${(size / 1024).toFixed(2)} KiB`;
    } else if (size < 1024 * 1024 * 1024) {
      return `${(size / 1024 / 1024).toFixed(2)} MiB`;
    } else {
      return `${(size / 1024 / 1024 / 1024).toFixed(2)} GiB`;
    }
  }

  function formatDuration(seconds: number) {
    seconds = Math.round(seconds);
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;

    if (hours > 0) {
      return `${hours}:${minutes.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
    } else {
      return `${minutes}:${secs.toString().padStart(2, "0")}`;
    }
  }

  function formatDate(dateString: string) {
    const date = new Date(dateString);
    return date.toLocaleString();
  }

  function formatPlatform(platform: string) {
    switch (platform.toLowerCase()) {
      case "bilibili":
        return "B站";
      case "douyin":
        return "抖音";
      case "huya":
        return "虎牙";
      case "kuaishou":
        return "快手";
      case "tiktok":
        return "TikTok";
      case "youtube":
        return "YouTube";
      default:
        return platform;
    }
  }

  function getRoomUrl(platform: string, roomId: string) {
    if (roomId.startsWith("http")) {
      return roomId;
    }
    switch (platform.toLowerCase()) {
      case "bilibili":
        return `https://live.bilibili.com/${roomId}`;
      case "douyin":
        return `https://live.douyin.com/${roomId}`;
      case "huya":
        return `https://www.huya.com/${roomId}`;
      case "kuaishou":
        return `https://live.kuaishou.com/u/${roomId}`;
      case "tiktok":
        return `https://www.tiktok.com/${roomId}/live`;
      case "youtube":
        return `https://www.youtube.com/channel/${roomId}`;
      default:
        return null;
    }
  }

  function calcBitrate(size: number, duration: number) {
    if (!duration || duration <= 0 || !size || size <= 0) {
      return "0";
    }
    return ((size * 8) / duration / 1024).toFixed(0);
  }

  function getArchiveKey(archive: RecordItem) {
    return `${archive.platform}-${archive.room_id}-${archive.parent_id}-${archive.live_id}`;
  }

  function toggleSort(field: string) {
    if (sortBy === field) {
      sortOrder = sortOrder === "asc" ? "desc" : "asc";
    } else {
      sortBy = field;
      sortOrder = "asc";
    }
    applyFilters();
  }

  function toggleArchiveSelection(liveId: string) {
    const next = new Set(selectedArchives);
    next.has(liveId) ? next.delete(liveId) : next.add(liveId);
    selectedArchives = next;
    lastSelectedArchiveId = liveId;
  }

  function handleArchiveRowClick(event: MouseEvent, archive: RecordItem) {
    const target = event.target;
    if (target instanceof Element && target.closest("[data-no-row-select]")) {
      return;
    }
    const orderedIds = filteredArchives.map((item) => item.live_id);
    const additive = event.ctrlKey || event.metaKey;
    selectedArchives = selectArchiveRange(
      orderedIds,
      selectedArchives,
      archive.live_id,
      event.shiftKey ? lastSelectedArchiveId : null,
      additive
    );
    lastSelectedArchiveId = archive.live_id;
  }

  function handleArchiveRowKeydown(event: KeyboardEvent, archive: RecordItem) {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    toggleArchiveSelection(archive.live_id);
  }

  function clearArchiveSelection() {
    selectedArchives = new Set();
    lastSelectedArchiveId = null;
  }

  function yieldToUi(): Promise<void> {
    return new Promise((resolve) => {
      requestAnimationFrame(() => resolve());
    });
  }

  function removeArchivesLocally(liveIds: Set<string>) {
    if (liveIds.size === 0) return;
    const remaining = new Set(liveIds);
    allArchives = allArchives.filter((archive) => !remaining.has(archive.live_id));
    if (selectedArchives.size > 0) {
      selectedArchives = new Set(
        [...selectedArchives].filter((liveId) => !remaining.has(liveId))
      );
    }
    totalCount = allArchives.length;
    updatePagination();
  }

  function markArchivesDeletedLocally(liveIds: Iterable<string>) {
    const removed = new Set(liveIds);
    removeArchivesLocally(removed);
    if (removed.size > 0) {
      liveDashboardBindings = new Map(
        [...liveDashboardBindings.entries()].filter(([liveId]) => !removed.has(liveId))
      );
    }
  }

  async function loadLiveDashboardBindings() {
    const liveIds = allArchives
      .filter((archive) => archive.platform === "douyin")
      .map((archive) => archive.live_id);
    if (liveIds.length === 0) {
      liveDashboardBindings = new Map();
      return;
    }
    try {
      const rows = await invoke<LiveDashboardBindingSummary[]>(
        "list_live_dashboard_bindings_for_live_ids",
        { liveIds }
      );
      liveDashboardBindings = new Map(rows.map((row) => [row.liveId, row]));
    } catch (error) {
      console.warn("Failed to load live dashboard bindings:", error);
    }
  }

  async function autoResolveLiveDashboardBindings() {
    const pending = allArchives
      .filter(
        (archive) =>
          archive.platform === "douyin" && !liveDashboardBindings.has(archive.live_id)
      )
      .slice(0, 15);
    for (const archive of pending) {
      try {
        const result = await invoke<LiveDashboardBindingResult>(
          "resolve_live_dashboard_for_record",
          {
            platform: archive.platform,
            roomId: archive.room_id,
            liveId: archive.live_id,
          }
        );
        if (!result.session) continue;
        liveDashboardBindings = new Map(liveDashboardBindings).set(archive.live_id, {
          liveId: archive.live_id,
          sessionId: result.session.id,
          matchMethod: result.matchMethod || "auto_account",
          accountKey: result.session.accountKey,
          shopName: result.session.shopName,
          startedAt: result.session.startedAt,
          paymentAmountFen: result.session.paymentAmountFen,
          dealItemCount: result.session.dealItemCount,
        });
      } catch (error) {
        console.warn("Auto resolve live dashboard failed:", archive.live_id, error);
      }
    }
  }

  function shopLabelForExcel(shopName: string): string {
    const known = COMPASS_TARGET_SHOPS.find((shop) => shop.value === shopName);
    return known ? `${known.label} · ${shopName}` : shopName;
  }

  function getArchiveAccountForExcel(archive: RecordItem): { accountName: string; shopLabel: string } {
    const accountName =
      roomAccountById.get(String(archive.room_id)) ||
      archive.anchor_name?.trim() ||
      "";
    const binding = liveDashboardBindings.get(archive.live_id);
    const shopName =
      binding?.shopName?.trim() ||
      inferCompassShopFromTexts([archive.title, archive.anchor_name, accountName]);
    return {
      accountName: accountName || "未识别账号",
      shopLabel: shopLabelForExcel(shopName),
    };
  }

  function getArchiveIdentity(archive: RecordItem) {
    if (isImportedArchive(archive)) {
      return {
        primary: archive.title || "外部导入录播",
        secondary: "手动导入 · 可在切片页同步管理",
        hasDashboard: false,
      };
    }
    return formatArchiveDashboardIdentity(
      { title: archive.title, anchorName: archive.anchor_name },
      liveDashboardBindings.get(archive.live_id)
    );
  }

  function selectAllArchives() {
    const currentArchives = filteredArchives;
    if (selectedArchives.size === currentArchives.length) {
      clearArchiveSelection();
    } else {
      selectedArchives = new Set(
        currentArchives.map((archive) => archive.live_id)
      );
      lastSelectedArchiveId =
        currentArchives[currentArchives.length - 1]?.live_id || null;
    }
  }

  async function deleteArchive(archive: RecordItem) {
    if (isDeletingArchives) return;
    isDeletingArchives = true;
    deleteProgress = { done: 0, total: 1 };
    showDeleteConfirm = false;
    archiveToDelete = null;
    try {
      if (isImportedArchive(archive)) {
        const videoId = getImportedVideoId(archive);
        if (!videoId) return;
        await invokeSensitive("delete_video", { id: videoId });
      } else {
        await invokeSensitive("delete_archive", {
          platform: archive.platform,
          roomId: archive.room_id,
          liveId: archive.live_id,
        });
      }
      markArchivesDeletedLocally([archive.live_id]);
      deleteProgress = { done: 1, total: 1 };
    } catch (error) {
      console.error("Failed to delete archive:", error);
      alert(`删除录播失败：${error}`);
    } finally {
      isDeletingArchives = false;
      deleteProgress = { done: 0, total: 0 };
    }
  }

  async function deleteArchiveWithFallback(
    platform: string,
    roomId: string,
    liveId: string
  ) {
    await invokeSensitive("delete_archive", {
      platform,
      roomId,
      liveId,
    });
    markArchivesDeletedLocally([liveId]);
  }

  async function deleteSelectedArchives() {
    if (isDeletingArchives) return;
    const selected = archives.filter((archive) => selectedArchives.has(archive.live_id));
    const importedSelected = selected.filter(isImportedArchive);
    const regularSelected = selected.filter((archive) => !isImportedArchive(archive));
    const groups = groupArchivesForDeletion(regularSelected, selectedArchives);
    const batches = chunkArchiveDeleteGroups(groups);
    const total = importedSelected.length + countArchiveDeleteTargets(groups);
    if (total === 0) return;

    isDeletingArchives = true;
    deleteProgress = { done: 0, total };
    showDeleteConfirm = false;
    archiveToDelete = null;
    const failures: string[] = [];
    let done = 0;

    try {
      for (const archive of importedSelected) {
        try {
          const videoId = getImportedVideoId(archive);
          if (!videoId) continue;
          await invokeSensitive("delete_video", { id: videoId });
          markArchivesDeletedLocally([archive.live_id]);
          done += 1;
          deleteProgress = { done, total };
        } catch (singleError) {
          failures.push(`${archive.title}：${singleError}`);
        }
        await yieldToUi();
      }

      for (const group of batches) {
        const batchIds = [...group.liveIds];
        try {
          await invokeSensitive("delete_archives", group);
          markArchivesDeletedLocally(batchIds);
          done += batchIds.length;
          deleteProgress = { done, total };
        } catch {
          for (const liveId of batchIds) {
            try {
              await deleteArchiveWithFallback(group.platform, group.roomId, liveId);
              done += 1;
              deleteProgress = { done, total };
            } catch (singleError) {
              failures.push(
                `${group.platform} / ${group.roomId} / ${liveId}：${singleError}`
              );
            }
            await yieldToUi();
          }
        }
        await yieldToUi();
      }

      if (done > 0) {
        clearArchiveSelection();
      }
      if (failures.length > 0) {
        const preview = failures.slice(0, 8).join("\n");
        const suffix =
          failures.length > 8 ? `\n... 另有 ${failures.length - 8} 条` : "";
        alert(`有 ${failures.length} 个录播删除失败：\n${preview}${suffix}`);
      }
    } catch (error) {
      console.error("Failed to delete selected archives:", error);
      alert(`删除录播失败：${error}`);
    } finally {
      isDeletingArchives = false;
      deleteProgress = { done: 0, total: 0 };
    }
  }

  async function playArchive(archive: RecordItem) {
    try {
      if (isImportedArchive(archive)) {
        const videoId = getImportedVideoId(archive);
        if (!videoId) return;
        try {
          // Backend remaps archived NAS UNC -> mapped drive (Z:) before Explorer/player.
          await invoke("open_video_externally", { id: videoId });
          return;
        } catch (error) {
          try {
            const video = await invoke<{ file: string }>("get_video", { id: videoId });
            const file = String(video?.file || "").trim();
            if (!file) throw new Error("视频没有文件路径");
            let path = file;
            if (!/^(?:[a-zA-Z]:[\\/]|\\\\)/.test(file)) {
              const config = await invoke<{ output: string }>("get_config");
              const base = String(config?.output || "").replace(/[\\/]+$/, "");
              path = `${base}\\${file.replace(/^[\\/]+/, "")}`;
            }
            await invoke("show_in_folder", { path });
            return;
          } catch {
            alert(`无法播放：${error}`);
            return;
          }
        }
      }
      await invoke("open_live", {
        platform: archive.platform,
        roomId: archive.room_id,
        liveId: archive.live_id,
      });
    } catch (error) {
      console.error("Failed to play archive:", error);
      alert(`无法播放：${error}`);
    }
  }

  function openWholeClipModal(archive: RecordItem) {
    wholeClipArchive = archive;
    showWholeClipModal = true;
  }

  function handleWholeClipGenerated() {
    // 生成完成后可以刷新列表或显示通知
    console.log("完整录播生成已开始");
  }

  async function handleArchiveVideoImported(
    event: CustomEvent<{ videoId?: number; videoIds?: number[] }>,
  ): Promise<void> {
    const videoId = event.detail.videoId ?? event.detail.videoIds?.at(-1);
    if (!videoId) {
      transcriptStatus = "录播视频已导入，可在下方列表查看。";
      await loadArchives();
      return;
    }
    try {
      const importedVideo = await invoke<VideoItem>("get_video", { id: videoId });
      if (importedVideo.cover) {
        importedVideo.cover = await get_static_url("output", importedVideo.cover);
      }
      const importedArchive = videoToImportedArchive(importedVideo);
      allArchives = [
        importedArchive,
        ...allArchives.filter((archive) => archive.live_id !== importedArchive.live_id),
      ];
      totalCount = allArchives.length;
      updatePagination();
      transcriptStatus = `《${importedVideo.title}》已加入${archiveKindTab === "company" ? "公司" : "竞品"}录播列表，正在打开分析页…`;
      window.dispatchEvent(new CustomEvent("bsr:open-video-analysis", {
        detail: {
          video: importedVideo,
          analysisMode: archiveKindTab === "company" ? "company_deal" : "legacy",
        },
      }));
    } catch (error: any) {
      transcriptStatus = `导入成功，但打开分析页失败：${error?.message || String(error)}`;
      await loadArchives();
    }
  }
</script>

<PageShell
  title="录播档案"
  subtitle="公司录播按主播聚合；身份未确认的场次单独进入待确认，竞品录播保持独立。"
  paddedBottom={selectedArchives.size > 0 || isDeletingArchives}
>
  <div slot="actions">
    <button
      type="button"
      class="mac-btn mac-btn-success"
      on:click={() => (showImportDialog = true)}
      title="导入已下载的录播视频，转写后可对照订单时间测试成交话术"
    >
      <Upload class="w-4 h-4" />
      <span>导入录播视频</span>
    </button>
    <button
      type="button"
      class="mac-btn mac-btn-primary"
      on:click={loadArchives}
      disabled={loading}
    >
      <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      <span>刷新</span>
    </button>
  </div>

    {#if transcriptStatus}
      <div class="rounded-[11px] border border-[color:var(--mac-blue)]/20 bg-[color:var(--mac-blue-soft)] px-4 py-3 text-sm text-[color:var(--mac-blue)]">
        {transcriptStatus}
      </div>
    {/if}

    <div class="mac-segmented" role="tablist" aria-label="录播档案分类">
      <button type="button" role="tab" aria-selected={archiveKindTab === "company"} class:mac-segment-active={archiveKindTab === "company"} on:click={() => switchArchiveKind("company")}>公司录播</button>
      <button type="button" role="tab" aria-selected={archiveKindTab === "competitor"} class:mac-segment-active={archiveKindTab === "competitor"} on:click={() => switchArchiveKind("competitor")}>竞品录播</button>
    </div>

    {#if archiveKindTab === "company" && (!selectedStreamerKey || !selectedStreamerGroup)}
      <div class="mac-card p-5">
        {#if loadError}
          <div class="directory-state" role="alert">
            <strong>录播档案加载失败</strong>
            <span>{loadError}</span>
            <button type="button" class="mac-btn mac-btn-primary" on:click={loadArchives}>重试</button>
          </div>
        {:else if loading && allArchives.length === 0}
          <div class="directory-state" role="status" aria-live="polite">
            <RefreshCw class="w-6 h-6 animate-spin" aria-hidden="true" />
            <strong>正在加载主播档案</strong>
            <span>正在读取公司录播与主播身份状态…</span>
          </div>
        {:else}
          <StreamerDirectory
            profiles={directoryProfiles}
            pendingCount={pendingStreamerGroup?.sessionCount || 0}
            on:select={(event) => selectStreamerProfile(event.detail.key)}
            on:pending={() => selectStreamerProfile(PENDING_STREAMER_KEY)}
          />
        {/if}
      </div>
    {:else}
      {#if archiveKindTab === "company" && selectedStreamerGroup?.isPending}
        <section class="mac-card pending-profile-header" aria-labelledby="pending-streamer-title">
          <div>
            <p class="pending-profile-eyebrow">公司录播 · 身份治理</p>
            <h2 id="pending-streamer-title">待确认录播</h2>
            <p>这里只收纳未识别、识别冲突或尚未可靠确认的场次；不会自动并入任何主播档案。</p>
          </div>
          <button type="button" class="mac-btn" on:click={backToStreamerDirectory}>返回主播列表</button>
        </section>
      {/if}

      <svelte:component
        this={StreamerProfileShell}
        profile={selectedProfileHeader || EMPTY_PROFILE_HEADER}
        radarSeries={profileRadarSeries}
        scoreRows={profileScoreRows}
        trendPoints={profileTrendPoints}
        trainingIssues={profileTrainingIssues}
        excellentClips={profileExcellentClips}
        knowledgeAssets={profileKnowledgeAssets}
        skillProfileSetup={skillProfileSetup}
        activeTab={streamerProfileTab}
        on:back={backToStreamerDirectory}
        on:training={openStreamerTraining}
        on:tab={handleProfileTab}
        on:evidence={openSkillEvidence}
        on:setup={openSkillProfileSetup}
        on:avatar={(event) => updateSelectedStreamerVirtualAvatar(event.detail.avatarId)}
      >
      <div class="profile-streams-stack">
      {#if archiveKindTab === "company"}
        <StreamerArchiveFilters
          bind:dateFrom={profileDateFrom}
          bind:dateTo={profileDateTo}
          bind:sessionQuery={profileSessionQuery}
          bind:productQuery={profileProductQuery}
          bind:liveStatus={profileLiveStatus}
          bind:analysisStatus={profileAnalysisStatus}
          on:change={() => { currentPage = 1; applyFilters(); }}
          on:clear={() => { currentPage = 1; }}
        />
      {/if}

    <div class="mac-card space-y-4 p-4">
      <div class="flex justify-between items-center flex-wrap gap-4">
        <!-- 左侧：筛选器和分页 -->
        <div class="flex space-x-3">
          <select
            bind:value={selectedRoomId}
            on:change={applyFilters}
            class="mac-field cursor-pointer"
          >
            <option value={null}>所有直播间</option>
            {#each roomOptions as option}
              <option value={option.id}>{option.label}</option>
            {/each}
          </select>

          <!-- 分页控制 -->
          {#if totalCount > 0}
            <div
              class="flex items-center space-x-3 px-4 py-2 rounded-[10px] border border-[color:var(--mac-separator)] bg-[color:var(--mac-fill)]"
            >
              <!-- 记录统计 -->
              <div class="flex items-center space-x-1">
                <span
                  class="text-sm font-medium text-[color:var(--mac-blue)]"
                >
                  {totalCount}
                </span>
                <span class="text-sm text-[color:var(--mac-tertiary)]"
                  >条记录</span
                >
              </div>

              <!-- 分隔线 -->
              <div class="h-4 w-px bg-[color:var(--mac-separator-strong)]"></div>

              <!-- 每页大小选择 -->
              <div class="flex items-center space-x-2">
                <span class="text-sm text-gray-600 dark:text-gray-400"
                  >每页</span
                >
                <select
                  bind:value={pageSize}
                  on:change={() => changePageSize(pageSize)}
                  class="px-2 py-1 text-sm bg-white dark:bg-gray-700 border border-gray-300 dark:border-gray-500 rounded-md text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 cursor-pointer min-w-[50px]"
                >
                  {#each pageSizeOptions as size}
                    <option value={size}>{size}</option>
                  {/each}
                </select>
                <span class="text-sm text-gray-600 dark:text-gray-400">条</span>
              </div>

              <!-- 分页导航 -->
              {#if totalPages > 1}
                <!-- 分隔线 -->
                <div class="h-4 w-px bg-gray-300 dark:bg-gray-600"></div>

                <div class="flex items-center space-x-2">
                  <button
                    class="p-1.5 text-gray-500 dark:text-gray-400 hover:text-blue-600 dark:hover:text-blue-400 hover:bg-white dark:hover:bg-gray-700 rounded-md transition-all duration-200 disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent"
                    on:click={prevPage}
                    disabled={currentPage === 1}
                    title="上一页"
                  >
                    <ChevronUp class="w-4 h-4 rotate-[-90deg]" />
                  </button>

                  <div
                    class="flex items-center px-2 py-1 bg-white dark:bg-gray-700 rounded-md border border-gray-200 dark:border-gray-500 min-w-[60px] justify-center"
                  >
                    <span
                      class="text-sm font-medium text-gray-700 dark:text-gray-300"
                    >
                      {currentPage}
                    </span>
                    <span class="text-sm text-gray-400 dark:text-gray-500 mx-1"
                      >/</span
                    >
                    <span class="text-sm text-gray-500 dark:text-gray-400">
                      {totalPages}
                    </span>
                  </div>

                  <button
                    class="p-1.5 text-gray-500 dark:text-gray-400 hover:text-blue-600 dark:hover:text-blue-400 hover:bg-white dark:hover:bg-gray-700 rounded-md transition-all duration-200 disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent"
                    on:click={nextPage}
                    disabled={currentPage === totalPages}
                    title="下一页"
                  >
                    <ChevronDown class="w-4 h-4 rotate-[-90deg]" />
                  </button>
                </div>
              {/if}
            </div>
          {/if}
        </div>

        <!-- 右侧：排序按钮 -->
        <div class="flex items-center space-x-2">
          <span class="text-sm text-gray-600 dark:text-gray-400">排序:</span>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors {sortBy ===
            'room_id'
              ? 'bg-blue-500 text-white'
              : 'bg-gray-100 dark:bg-gray-700/50 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600'}"
            on:click={() => toggleSort("room_id")}
          >
            直播间号
            {#if sortBy === "room_id"}
              {#if sortOrder === "asc"}
                <ChevronUp class="w-3 h-3 inline ml-1" />
              {:else}
                <ChevronDown class="w-3 h-3 inline ml-1" />
              {/if}
            {/if}
          </button>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors {sortBy ===
            'title'
              ? 'bg-blue-500 text-white'
              : 'bg-gray-100 dark:bg-gray-700/50 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600'}"
            on:click={() => toggleSort("title")}
          >
            标题
            {#if sortBy === "title"}
              {#if sortOrder === "asc"}
                <ChevronUp class="w-3 h-3 inline ml-1" />
              {:else}
                <ChevronDown class="w-3 h-3 inline ml-1" />
              {/if}
            {/if}
          </button>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors {sortBy ===
            'length'
              ? 'bg-blue-500 text-white'
              : 'bg-gray-100 dark:bg-gray-700/50 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600'}"
            on:click={() => toggleSort("length")}
          >
            时长
            {#if sortBy === "length"}
              {#if sortOrder === "asc"}
                <ChevronUp class="w-3 h-3 inline ml-1" />
              {:else}
                <ChevronDown class="w-3 h-3 inline ml-1" />
              {/if}
            {/if}
          </button>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors {sortBy ===
            'size'
              ? 'bg-blue-500 text-white'
              : 'bg-gray-100 dark:bg-gray-700/50 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600'}"
            on:click={() => toggleSort("size")}
          >
            大小
            {#if sortBy === "size"}
              {#if sortOrder === "asc"}
                <ChevronUp class="w-3 h-3 inline ml-1" />
              {:else}
                <ChevronDown class="w-3 h-3 inline ml-1" />
              {/if}
            {/if}
          </button>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors {sortBy ===
            'created_at'
              ? 'bg-blue-500 text-white'
              : 'bg-gray-100 dark:bg-gray-700/50 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600'}"
            on:click={() => toggleSort("created_at")}
          >
            创建时间
            {#if sortBy === "created_at"}
              {#if sortOrder === "asc"}
                <ChevronUp class="w-3 h-3 inline ml-1" />
              {:else}
                <ChevronDown class="w-3 h-3 inline ml-1" />
              {/if}
            {/if}
          </button>
        </div>
      </div>

    </div>

    <!-- Archive List -->
    <div class="mac-card overflow-hidden">
      {#if loadError}
        <div
          class="flex flex-col items-center justify-center p-12 space-y-4 text-gray-500 dark:text-gray-400"
        >
          <div class="text-red-500 dark:text-red-400 text-lg">加载失败</div>
          <p class="text-sm">{loadError}</p>
          <button
            type="button"
            class="mac-btn mac-btn-primary"
            on:click={loadArchives}
          >
            重试
          </button>
        </div>
      {:else if loading && allArchives.length === 0}
        <div
          class="flex flex-col items-center justify-center p-12 space-y-4 text-gray-500 dark:text-gray-400"
        >
          <RefreshCw class="w-8 h-8 animate-spin" />
          <span>加载录播列表中...</span>
        </div>
      {:else if filteredArchives.length === 0}
        <div
          class="flex flex-col items-center justify-center p-12 space-y-4 text-gray-500 dark:text-gray-400"
        >
          <History class="w-12 h-12" />
          <h3 class="text-lg font-medium text-gray-900 dark:text-white">
            暂无录播
          </h3>
          <p class="text-sm">
            {selectedRoomId !== null
              ? "该直播间还没有录播记录"
              : archiveKindTab === "company"
                ? "还没有录播记录，可点击右上角「导入录播视频」导入外部 TS/MP4"
                : "还没有竞品录播记录，可导入外部视频或从抖音录播归类"}
          </p>
        </div>
      {:else}
        <div class="mac-table-wrap custom-scrollbar-light">
          <table class="mac-table">
            <thead>
              <tr>
                <th class="w-12">
                  <label
                    class="inline-flex h-10 w-10 items-center justify-center rounded-[8px] cursor-pointer select-none hover:bg-[color:var(--mac-fill)]"
                    title="全选当前页"
                  >
                    <input
                      type="checkbox"
                      checked={selectedArchives.size ===
                        filteredArchives.length && filteredArchives.length > 0}
                      on:change={selectAllArchives}
                      class="w-5 h-5 rounded-md border-gray-300 dark:border-gray-600 accent-[color:var(--mac-blue)] cursor-pointer"
                    />
                    <span class="sr-only">全选当前页</span>
                  </label>
                </th>
                <th class="w-24">直播时间</th>
                <th class="w-36">直播间</th>
                <th class="w-40">账号</th>
                <th>标题</th>
                <th class="w-28">主播</th>
                <th class="w-24">时长</th>
                <th class="w-20">大小</th>
                <th class="w-24">码率</th>
                <th class="w-24">分析状态</th>
                <th class="w-56">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredArchives as archive (getArchiveKey(archive))}
                {@const identity = getArchiveIdentity(archive)}
                {@const account = getArchiveAccountForExcel(archive)}
                <tr
                  class="archive-selectable-row group cursor-pointer select-none transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[color:var(--mac-blue)]"
                  class:archive-row-selected={selectedArchives.has(archive.live_id)}
                  aria-selected={selectedArchives.has(archive.live_id)}
                  tabindex="0"
                  on:click={(event) => handleArchiveRowClick(event, archive)}
                  on:keydown={(event) => handleArchiveRowKeydown(event, archive)}
                >
                  <td class="px-2 py-2" data-no-row-select>
                    <label
                      class="flex w-10 h-10 items-center justify-center rounded-lg cursor-pointer hover:bg-blue-500/10"
                      title={selectedArchives.has(archive.live_id) ? "取消选择" : "选择录播"}
                    >
                      <input
                        type="checkbox"
                        checked={selectedArchives.has(archive.live_id)}
                        on:change={() => toggleArchiveSelection(archive.live_id)}
                        class="w-5 h-5 rounded-md border-gray-300 dark:border-gray-600 accent-blue-500 cursor-pointer"
                      />
                    </label>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <div class="flex flex-col min-w-0">
                      <span class="text-sm text-gray-900 dark:text-white truncate"
                        >{formatDate(archive.created_at).split(" ")[0]}</span
                      >
                      <span class="text-xs text-gray-500 dark:text-gray-400 truncate"
                        >{formatDate(archive.created_at).split(" ")[1]}</span
                      >
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <div class="flex min-w-0 items-center gap-2">
                      {#if isImportedArchive(archive)}
                        <Upload class="w-4 h-4 flex-shrink-0 text-emerald-500" />
                        <span class="truncate text-sm font-medium text-emerald-700 dark:text-emerald-300"
                          >外部导入</span
                        >
                      {:else if archive.platform === "bilibili"}
                        <BilibiliIcon class="w-4 h-4 flex-shrink-0" />
                      {:else if archive.platform === "douyin"}
                        <DouyinIcon class="w-4 h-4 flex-shrink-0" />
                      {:else if archive.platform === "kuaishou"}
                        <KuaishouIcon class="w-4 h-4 flex-shrink-0" />
                      {:else if archive.platform === "huya"}
                        <HuyaIcon class="w-4 h-4 flex-shrink-0" />
                      {:else if archive.platform === "tiktok"}
                        <TikTokIcon class="w-5 h-5 flex-shrink-0" />
                      {:else}
                        <Globe class="w-4 h-4 flex-shrink-0 text-gray-400" />
                      {/if}
                      {#if getRoomUrl(archive.platform, archive.room_id)}
                        <a
                          data-no-row-select
                          href={getRoomUrl(archive.platform, archive.room_id)}
                          target="_blank"
                          rel="noopener noreferrer"
                          class="min-w-0 truncate text-blue-500 hover:text-blue-700 text-sm"
                          title={`打开 ${formatPlatform(archive.platform)} 直播间`}
                        >
                          {archive.room_id}
                        </a>
                      {:else}
                        <span class="min-w-0 truncate text-sm text-gray-900 dark:text-white"
                          >{archive.room_id}</span
                        >
                      {/if}
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <div class="min-w-0">
                      <span
                        class="block truncate text-sm font-medium text-gray-900 dark:text-white"
                        title={account.accountName}
                      >{account.accountName}</span>
                      <p class="truncate text-xs text-gray-500 dark:text-gray-400" title={`对应 Excel：${account.shopLabel}`}>
                        {account.shopLabel}
                      </p>
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <div class="flex min-w-0 items-center gap-3">
                      {#if archive.cover}
                        <img
                          src={archive.cover}
                          alt={`${archive.title}封面`}
                          class="w-12 h-8 rounded object-cover flex-shrink-0"
                        />
                      {:else}
                        <div
                          class="flex h-8 w-12 flex-shrink-0 items-center justify-center rounded border border-gray-200 bg-gray-100 text-gray-400 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-300"
                          role="img"
                          aria-label={`暂无封面：${archive.title}`}
                          title="暂无封面"
                        >
                          <Video class="h-4 w-4" aria-hidden="true" />
                        </div>
                      {/if}
                      <div class="min-w-0 flex-1 overflow-hidden">
                        <div class="flex min-w-0 items-center gap-2">
                          <span
                            class="min-w-0 truncate text-sm font-medium text-gray-900 dark:text-white"
                            title={identity.primary}
                          >{identity.primary}</span>
                          {#if isImportedArchive(archive)}
                            <span class="inline-flex shrink-0 items-center rounded-full bg-emerald-50 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-300">
                              外部导入
                            </span>
                          {:else if identity.hasDashboard}
                            <span class="inline-flex shrink-0 items-center rounded-full bg-blue-50 px-2 py-0.5 text-[11px] font-medium text-blue-700 dark:bg-blue-500/10 dark:text-blue-300">
                              已绑大屏
                            </span>
                          {/if}
                        </div>
                        <p class="truncate text-xs text-gray-500 dark:text-gray-400" title={identity.secondary}>
                          {identity.secondary}
                        </p>
                        {#if archive.title && archive.title !== identity.primary}
                          <p class="truncate text-[11px] text-gray-400 dark:text-gray-500" title={archive.title}>
                            {archive.title}
                          </p>
                        {/if}
                      </div>
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden" data-no-row-select>
                    <div class="flex min-w-0 items-center gap-2">
                      <UserRound class="w-4 h-4 flex-shrink-0 text-gray-400" />
                      <div
                        class="min-w-0 flex-1 overflow-hidden"
                        title={archive.anchor_detection_error || "主播识别状态"}
                      >
                        {#if archiveKindTab === "company" && selectedStreamerGroup?.isPending && !isArchiveRecording(archive)}
                          <button
                            type="button"
                            class="block max-w-full truncate text-left text-sm font-medium text-blue-600 hover:text-blue-700 hover:underline dark:text-blue-400"
                            title="点击人工确认主播姓名；保存后会移出待确认"
                            on:click|stopPropagation={() => openArchiveAnchorDialog(archive)}
                          >
                            待确认 · {pendingIdentityStatusLabel(archive)} · 点击确认
                          </button>
                        {:else if canManuallyEditAnchor(archive)}
                          <button
                            type="button"
                            class="block max-w-full truncate text-left text-sm font-medium text-blue-600 hover:text-blue-700 hover:underline dark:text-blue-400"
                            title="点击人工填写主播姓名"
                            on:click|stopPropagation={() => openArchiveAnchorDialog(archive)}
                          >
                            未识别 · 点击修改
                          </button>
                        {:else}
                          <span class="block truncate text-sm font-medium text-gray-800 dark:text-gray-100">
                            {archiveKindTab === "company" && selectedStreamerGroup?.isPending
                              ? `待确认 · ${pendingIdentityStatusLabel(archive)}`
                              : archiveKindTab === "company"
                              ? normalizeStreamerName(archive.anchor_name) || anchorStatusLabel(archive)
                              : archive.anchor_name || anchorStatusLabel(archive)}
                          </span>
                        {/if}
                      </div>
                      {#if archive.anchor_detection_status === "running"}
                        <Loader2 class="w-4 h-4 flex-shrink-0 animate-spin text-blue-600" />
                      {/if}
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <div class="flex min-w-0 items-center gap-1.5">
                      <Clock class="w-4 h-4 flex-shrink-0 text-gray-400" />
                      <span class="truncate text-sm text-gray-900 dark:text-white"
                        >{formatDuration(archive.length)}</span
                      >
                      {#if isArchiveRecording(archive)}
                        <span
                          class="inline-flex shrink-0 items-center rounded-full bg-red-100 px-1.5 py-0.5 text-[10px] font-medium text-red-700"
                          >录制中</span
                        >
                      {/if}
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <div class="flex min-w-0 items-center gap-1.5">
                      <HardDrive class="w-4 h-4 flex-shrink-0 text-gray-400" />
                      <span class="truncate text-sm text-gray-900 dark:text-white"
                        >{formatSize(archive.size)}</span
                      >
                    </div>
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <span class="block truncate text-sm text-gray-500 dark:text-gray-400"
                      >{calcBitrate(archive.size, archive.length)} Kbps</span
                    >
                  </td>

                  <td class="px-3 py-3 overflow-hidden">
                    <span
                      class="archive-analysis-status"
                      class:is-complete={analysisCompletionForArchive(archive, analysisCompletionBySource) >= 100}
                      class:is-progress={analysisCompletionForArchive(archive, analysisCompletionBySource) > 0 && analysisCompletionForArchive(archive, analysisCompletionBySource) < 100}
                    >{analysisStatusLabel(archive, analysisCompletionBySource)}</span>
                  </td>

                  <td class="px-3 py-3 overflow-hidden" data-no-row-select>
                    <div class="flex flex-wrap items-center gap-1" data-no-row-select>
                      <button
                        class="p-1.5 rounded-lg hover:bg-blue-500/10 transition-colors"
                        title={isImportedArchive(archive) ? "用系统播放器打开原文件" : "预览录播"}
                        on:click={() => playArchive(archive)}
                      >
                        <Play class="w-4 h-4 text-blue-500" />
                      </button>
                      {#if !isImportedArchive(archive)}
                      <button
                        class="p-1.5 rounded-lg hover:bg-blue-500/10 transition-colors"
                        title="生成完整切片"
                        on:click={() => openWholeClipModal(archive)}
                      >
                        <FileVideo class="w-4 h-4 text-blue-500" />
                      </button>
                      {/if}
                      {#if archive.archive_kind === "company"}
                      <button
                        class="inline-flex items-center gap-1 px-1.5 py-1 rounded-lg hover:bg-emerald-500/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed text-xs text-emerald-700"
                        title="按成交订单时间轴复盘公司录播"
                        disabled={isArchiveRecording(archive)}
                        on:click={() => openCompanyDealReview(archive)}
                      >
                        <BarChart3 class="w-4 h-4" />
                        <span>分析</span>
                      </button>
                      {:else}
                      <button
                        class="inline-flex items-center gap-1 px-1.5 py-1 rounded-lg hover:bg-violet-500/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed text-xs text-violet-600"
                        title={isArchiveRecording(archive) ? "录制结束后才能分析整场" : "自动生成文稿并分析候选片段"}
                        disabled={isArchiveRecording(archive)}
                        on:click={() => analyzeWholeArchive(archive)}
                      >
                        <BrainCircuit class="w-4 h-4" />
                        <span>分析</span>
                      </button>
                      {/if}
                      <button
                        class="inline-flex items-center gap-1 px-1.5 py-1 rounded-lg hover:bg-gray-100 transition-colors text-xs text-gray-600 dark:hover:bg-gray-700 dark:text-gray-300"
                        title="人工修改录播分类"
                        on:click={() => changeArchiveKind(archive, archive.archive_kind === "company" ? "competitor" : "company")}
                      >
                        {archive.archive_kind === "company" ? "移至竞品" : "移至公司"}
                      </button>
                      <button
                        class="p-1.5 rounded-lg hover:bg-red-500/10 transition-colors"
                        title="删除记录"
                        on:click={() => {
                          archiveToDelete = archive;
                          showDeleteConfirm = true;
                        }}
                      >
                        <Trash2 class="w-4 h-4 text-red-500" />
                      </button>
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
      </div>
      </svelte:component>
    {/if}
</PageShell>

{#if selectedArchives.size > 0 || isDeletingArchives}
  <div
    class="fixed left-1/2 bottom-6 z-40 -translate-x-1/2 flex h-14 min-w-[360px] max-w-[calc(100vw-2rem)] items-center justify-between gap-6 rounded-[14px] border border-[color:var(--mac-separator)] bg-[color:var(--mac-bg-elevated)] px-3 shadow-mac-lg backdrop-blur-xl"
    aria-live="polite"
  >
    <div class="flex items-center gap-3 pl-2">
      {#if isDeletingArchives}
        <Loader2 class="w-5 h-5 animate-spin text-red-500" />
        <span class="text-sm font-medium text-gray-800 dark:text-gray-100 whitespace-nowrap">
          正在删除 {deleteProgress.done}/{deleteProgress.total}
        </span>
      {:else}
        <span class="flex h-6 min-w-[24px] items-center justify-center rounded-full bg-[color:var(--mac-blue)] px-1.5 text-xs font-semibold text-white">
          {selectedArchives.size}
        </span>
        <span class="text-sm font-medium text-gray-800 dark:text-gray-100 whitespace-nowrap">
          已选择录播
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-1">
      <button
        class="inline-flex h-10 items-center gap-2 rounded-lg px-3 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-black/5 dark:hover:bg-white/10 transition-colors disabled:opacity-50"
        title="取消全部选择"
        disabled={isDeletingArchives}
        on:click={clearArchiveSelection}
      >
        <X class="w-4 h-4" />
        <span>取消选择</span>
      </button>
      <button
        class="inline-flex h-10 items-center gap-2 rounded-lg bg-red-600 px-4 text-sm font-medium text-white hover:bg-red-700 transition-colors disabled:cursor-not-allowed disabled:opacity-60"
        title="删除选中的录播"
        disabled={isDeletingArchives}
        on:click={() => {
          showDeleteConfirm = true;
          archiveToDelete = null;
        }}
      >
        {#if isDeletingArchives}
          <Loader2 class="w-4 h-4 animate-spin" />
        {:else}
          <Trash2 class="w-4 h-4" />
        {/if}
        <span>删除</span>
      </button>
    </div>
  </div>
{/if}

{#if anchorToEdit}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/25 p-4 backdrop-blur-sm"
    role="presentation"
    on:click={closeArchiveAnchorDialog}
    on:keydown|stopPropagation
  >
    <div
      class="mac-modal w-[420px] rounded-xl bg-white p-6 shadow-xl dark:bg-[#323234]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="archive-anchor-dialog-title"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <div class="flex items-center justify-between">
        <h3 id="archive-anchor-dialog-title" class="text-base font-semibold text-gray-900 dark:text-white">
          确认主播姓名
        </h3>
        <button
          class="rounded-lg p-1.5 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700"
          title="关闭"
          on:click={closeArchiveAnchorDialog}
        >
          <X class="w-4 h-4" />
        </button>
      </div>
      <p class="mt-2 text-sm text-gray-500 dark:text-gray-400">
        人工确认后将锁定姓名，后续自动识别不会覆盖。
      </p>
      <label class="mt-4 block text-sm font-medium text-gray-700 dark:text-gray-200">
        主播姓名
        <input
          class="mac-field mt-2 w-full"
          bind:value={editingAnchorName}
          maxlength="12"
          placeholder="例如：小鱼"
        />
      </label>
      {#if anchorEditError}
        <p class="mt-2 text-sm text-red-600">{anchorEditError}</p>
      {/if}
      <div class="mt-5 flex justify-end gap-3">
        <button
          type="button"
          class="mac-btn"
          on:click={closeArchiveAnchorDialog}
        >
          取消
        </button>
        <button
          type="button"
          class="mac-btn mac-btn-primary"
          disabled={!editingAnchorName.trim()}
          on:click={saveArchiveAnchorName}
        >
          确认并锁定
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showSkillProfileSetupModal && selectedStreamerGroup && !selectedStreamerGroup.isPending}
  <MacModal
    title={`建立能力画像 · 第 ${skillProfileSetupStep}/4 步`}
    panelClass="w-[760px] max-w-[calc(100vw-2rem)]"
    showClose
    closeOnBackdrop
    on:close={closeSkillProfileSetup}
  >
    <div class="skill-setup-wizard">
      <ol class="skill-setup-steps" aria-label="能力画像建档步骤">
        {#each ["选择录播", "检查分析", "AI 评估", "预览画像"] as label, index}
          <li class:active={skillProfileSetupStep === index + 1} class:complete={skillProfileSetupStep > index + 1}>
            <span>{index + 1}</span>{label}
          </li>
        {/each}
      </ol>

      {#if skillProfileSetupStep === 1}
        <div class="skill-setup-copy">
          <strong>选择代表性录播</strong>
          <p>默认已勾选最近 5 场。建议覆盖不同日期或商品；能力分数至少需要 {MIN_SKILL_SCORE_SOURCES} 个不同场次来源。</p>
        </div>
        <fieldset class="skill-setup-session-list">
          <legend>本次建档使用的录播</legend>
          {#each selectedStreamerGroup.archives as archive, index (archiveKey(archive))}
            {@const sourceId = streamerArchiveSourceKey(archive)}
            <label class="skill-setup-session">
              <input
                type="checkbox"
                checked={selectedSkillProfileSetupSourceIds.has(sourceId)}
                data-modal-initial-focus={index === 0}
                on:change={() => toggleSkillProfileSetupArchive(sourceId)}
              />
              <span>
                <strong>{archive.title || archive.live_id}</strong>
                <small>{formatDate(archive.created_at)} · {analysisStatusLabel(archive)}</small>
              </span>
            </label>
          {/each}
        </fieldset>
        <p class="skill-setup-selection-status" role="status">
          已选择 {selectedSkillProfileSetupArchives.length} 场录播；系统会先建立基础档案，再自动准备可用文稿。
        </p>
        {#if selectedSkillProfileSetupArchives.length < MIN_SKILL_SCORE_SOURCES}
          <p class="skill-setup-warning" role="status">当前只选择 {selectedSkillProfileSetupArchives.length} 场；可以继续，但保存后仍会显示“数据不足”。</p>
        {/if}
        <div class="skill-modal-actions">
          <button type="button" class="mac-btn" on:click={closeSkillProfileSetup}>稍后处理</button>
          <button
            type="button"
            class="mac-btn mac-btn-primary"
            disabled={selectedSkillProfileSetupArchives.length === 0}
            on:click={establishSkillProfile}
          >{selectedSkillProfileSetupEvidenceCandidates.length || canUsePreparedTranscriptEvidence ? "继续由 AI 评估" : "建立画像并后台准备"}</button>
        </div>
      {:else if skillProfileSetupStep === 2}
        <div class="skill-setup-copy">
          <strong>检查 AI 可用证据</strong>
          <p>AI 优先使用既有分析证据；已准备的录播原始文稿同样可作为可追溯来源。每项评分仍至少需要 {MIN_SKILL_SCORE_SOURCES} 个不同场次。</p>
        </div>
        <dl class="skill-setup-summary">
          <div><dt>已选择录播</dt><dd>{selectedSkillProfileSetupArchives.length} 场</dd></div>
          <div><dt>完成分析</dt><dd>{selectedSkillProfileSetupAnalyzedCount} 场</dd></div>
          <div><dt>可用评估来源</dt><dd>{selectedSkillProfileSetupEvidenceCandidates.length || (canUsePreparedTranscriptEvidence ? skillProfilePreparation.readyCount : 0)} 条</dd></div>
        </dl>
        <div class="skill-setup-evidence-list">
          {#each selectedSkillProfileSetupArchives as archive (archiveKey(archive))}
            {@const sourceId = streamerArchiveSourceKey(archive)}
            {@const sourceEvidenceCount = selectedStreamerEvidenceCandidates.filter((evidence) => evidence.sourceId === sourceId).length}
            <article>
              <strong>{archive.title || archive.live_id}</strong>
              <span>{analysisStatusLabel(archive)} · {sourceEvidenceCount ? `分析证据 ${sourceEvidenceCount} 条` : "已准备文稿后可由 AI 读取"}</span>
            </article>
          {/each}
        </div>
        {#if selectedSkillProfileSetupEvidenceCandidates.length === 0 && !canUsePreparedTranscriptEvidence}
          <p class="skill-setup-warning" role="status">所选录播尚未形成分析证据或可读取文稿；先建立基础画像，系统会后台准备录播文稿。</p>
        {/if}
        <div class="skill-modal-actions">
          <button type="button" class="mac-btn" on:click={() => (skillProfileSetupStep = 1)}>上一步</button>
          {#if selectedSkillProfileSetupEvidenceCandidates.length || canUsePreparedTranscriptEvidence}
            <button type="button" class="mac-btn mac-btn-primary" on:click={() => (skillProfileSetupStep = 3)}>下一步：AI 评估</button>
          {:else}
            <button type="button" class="mac-btn mac-btn-primary" on:click={establishSkillProfile}>建立画像并后台准备</button>
          {/if}
        </div>
      {:else if skillProfileSetupStep === 3}
        <div class="skill-setup-copy">
          <strong>让 AI 根据证据生成能力画像</strong>
          <p>AI 只读取本次选择录播的既有分析证据或原始字幕文稿。某个维度没有至少 {MIN_SKILL_SCORE_SOURCES} 个不同场次来源时，AI 必须显示“数据不足”，不会补成 0 分。</p>
        </div>
        <details class="skill-ai-rubric">
          <summary>查看 AI 评分规则</summary>
          <p>固定评估需求确认、产品专业、信任建立、表达结构、节奏控场、异议处理、成交推进、风险合规八项；每项分数必须引用所选录播内至少两个不同来源的证据，并返回评分依据与证据编号。</p>
        </details>
        {#if skillAiError}
          <p class="skill-rating-error" role="alert">{skillAiError}</p>
        {/if}
        <div class="skill-modal-actions">
          <button type="button" class="mac-btn" on:click={() => (skillProfileSetupStep = 2)}>上一步</button>
          <button
            type="button"
            class="mac-btn mac-btn-primary"
            data-modal-initial-focus
            disabled={skillAiEvaluating}
            on:click={runSkillProfileAiEvaluation}
          >{skillAiEvaluating ? "AI 评估中…" : "开始 AI 评估"}</button>
        </div>
      {:else}
        <div class="skill-setup-preview" role="status">
          <strong>{selectedStreamerGroup.streamerName}的能力画像进度</strong>
          {#if skillAiResultSummary}<p>{skillAiResultSummary}</p>{/if}
          <p>当前已有 {skillProfileSetup.qualifiedDimensionCount}/8 个维度达到每维 {MIN_SKILL_SCORE_SOURCES} 个场次来源门槛。</p>
          {#if skillProfileSetup.qualifiedDimensionCount >= skillProfileSetup.minimumDimensionsForRadar}
            <p>能力画像已可以显示雷达图；继续补齐其余维度可形成完整八维对比。</p>
          {:else}
            <p>继续审核能力维度；达到 {skillProfileSetup.minimumDimensionsForRadar} 个有效维度后，档案页将显示真实雷达线。</p>
          {/if}
        </div>
        <div class="skill-modal-actions">
          <button type="button" class="mac-btn" on:click={() => (skillProfileSetupStep = 3)}>重新 AI 评估</button>
          <button type="button" class="mac-btn mac-btn-primary" on:click={closeSkillProfileSetup}>返回能力画像</button>
        </div>
      {/if}
    </div>
  </MacModal>
{/if}

{#if showSkillRatingModal && selectedStreamerGroup && !selectedStreamerGroup.isPending}
  <MacModal
    title={`人工评分 · ${STREAMER_SKILL_DIMENSIONS.find((item) => item.key === ratingDimension)?.label || "能力维度"}`}
    panelClass="w-[720px] max-w-[calc(100vw-2rem)]"
    showClose
    closeOnBackdrop
    on:close={closeSkillRating}
  >
    <div class="skill-rating-form">
      <p class="skill-rating-notice">
        只允许使用已逐条人工核对的直播/切片证据。少于 2 个不同场次来源可以保存版本，但分数继续显示“数据不足”。
      </p>

      <div class="skill-rating-grid">
        <label>
          <span>评分周期</span>
          <select class="mac-field" bind:value={ratingPeriod}>
            <option value="current">当前周期</option>
            <option value="previous">上一周期</option>
          </select>
        </label>
        <label>
          <span>人工分数（0–100）</span>
          <input class="mac-field" type="number" min="0" max="100" step="1" bind:value={ratingScore} />
        </label>
        <label>
          <span>审核人</span>
          <input class="mac-field" maxlength="40" bind:value={ratingReviewer} placeholder="填写真实审核人" />
        </label>
      </div>

      <label class="skill-rating-basis">
        <span>评分依据</span>
        <textarea
          class="mac-field"
          rows="3"
          maxlength="500"
          bind:value={ratingBasis}
          placeholder="说明为什么给出该分数，以及证据共同支持了什么判断"
        ></textarea>
      </label>

      <fieldset class="evidence-picker">
        <legend>选择并人工确认的证据</legend>
        {#if scopedRatingEvidenceCandidates.length}
          <div class="evidence-picker-list custom-scrollbar-light">
            {#each scopedRatingEvidenceCandidates as evidence (evidence.id)}
              <label class="evidence-option">
                <input
                  type="checkbox"
                  checked={selectedRatingEvidenceIds.has(evidence.id)}
                  on:change={() => toggleRatingEvidence(evidence.id)}
                />
                <span>
                  <strong>{evidenceSourceTitle(evidence.sourceId)}</strong>
                  <small>{evidence.summary}</small>
                  <small>{evidence.basis}</small>
                </span>
              </label>
            {/each}
          </div>
        {:else}
          <div class="evidence-empty">
            暂无同时具备候选片段、逐段复盘和母稿对照的证据。请先完成场次分析与人工复盘。
          </div>
        {/if}
      </fieldset>

      <label class="review-confirmation">
        <input type="checkbox" bind:checked={ratingReviewConfirmed} />
        <span>我已逐条打开并核对所选直播/切片证据，确认它们支持本维度评分。</span>
      </label>

      {#if ratingError}
        <p class="skill-rating-error" role="alert">{ratingError}</p>
      {/if}

      <div class="skill-modal-actions">
        <button type="button" class="mac-btn" on:click={closeSkillRating}>取消</button>
        <button type="button" class="mac-btn mac-btn-primary" on:click={saveSkillRating}>保存新版本</button>
      </div>
    </div>
  </MacModal>
{/if}

{#if showSkillEvidenceModal}
  <MacModal
    title={`${evidenceDimensionLabel} · 评分证据与版本`}
    panelClass="w-[760px] max-w-[calc(100vw-2rem)]"
    showClose
    closeOnBackdrop
    on:close={() => (showSkillEvidenceModal = false)}
  >
    <div class="skill-evidence-history">
      <p>每次 AI 评估都会追加版本；旧版本不会覆盖。以下展示该维度的评分依据和可回看的分析证据。</p>
      {#if selectedDimensionVersions.length}
        {#each selectedDimensionVersions as version (version.id)}
          {@const versionSampleCount = new Set(version.evidence.map((item) => item.sourceId.trim().toLowerCase())).size}
          {@const versionActive = isRatingVersionActive(version, selectedStreamerSourceKeys)}
          <article class="rating-version-card">
            <header>
              <div>
                <strong>
                  {SKILL_RATING_PERIOD_LABELS[version.period]} ·
                  {!versionActive
                    ? `已失效 · 原始${version.source === "ai" ? "AI" : "人工"}记录 ${version.score} 分`
                    : versionSampleCount < MIN_SKILL_SCORE_SOURCES
                    ? `数据不足 · 原始${version.source === "ai" ? "AI" : "人工"}记录 ${version.score} 分`
                    : `${version.score} 分`}
                  · v{version.version}
                </strong>
                <span>{formatDate(version.createdAt)} · {version.source === "ai" ? "AI 评估" : "人工修正"} {version.reviewedBy}</span>
              </div>
              <span>{versionSampleCount} 个来源</span>
            </header>
            {#if !versionActive}
              <p class="rating-version-excluded" role="status">
                已不计入当前评分：所绑定证据已不属于此主播的公司录播档案。
              </p>
            {/if}
            <p>{version.basis}</p>
            <div class="version-evidence-list">
              {#each version.evidence as evidence (evidence.id)}
                <button type="button" on:click={() => openEvidenceSource(evidence)}>
                  <span>
                    <strong>{evidenceSourceTitle(evidence.sourceId)}</strong>
                    <small>{evidence.summary}</small>
                  </span>
                  <span>查看原始证据</span>
                </button>
              {/each}
            </div>
          </article>
        {/each}
      {:else}
        <div class="evidence-empty">该维度尚无 AI 评分版本。先建立能力画像，让 AI 根据已有分析证据评估。</div>
      {/if}
    </div>
  </MacModal>
{/if}

<!-- Delete Confirmation Modal -->
{#if showDeleteConfirm}
  <MacModal bare panelClass="w-[400px]">
    <div class="p-6 space-y-4">
      <div class="text-center space-y-2">
        <h3 class="text-[15px] font-semibold text-[color:var(--mac-label)]">
          确认删除
        </h3>
        <p class="text-sm text-[color:var(--mac-tertiary)]">
          {#if archiveToDelete}
            确定要删除录播 "{archiveToDelete.title}" 吗？
          {:else}
            确定要删除选中的 {selectedArchives.size} 个录播吗？
          {/if}
        </p>
        <p class="text-xs text-[color:var(--mac-red)]">此操作无法撤销。</p>
      </div>
      <div class="flex justify-center gap-3">
        <button
          type="button"
          class="mac-btn w-24"
          disabled={isDeletingArchives}
          on:click={() => {
            showDeleteConfirm = false;
            archiveToDelete = null;
          }}
        >
          取消
        </button>
        <button
          type="button"
          class="mac-btn mac-btn-danger w-24 inline-flex items-center justify-center gap-2"
          disabled={isDeletingArchives}
          on:click={() => {
            if (archiveToDelete) {
              deleteArchive(archiveToDelete);
            } else {
              deleteSelectedArchives();
            }
          }}
        >
          {#if isDeletingArchives}
            <Loader2 class="w-4 h-4 animate-spin" />
          {/if}
          删除
        </button>
      </div>
    </div>
  </MacModal>
{/if}

{#if showFactCardModal && factCardArchive}
  <div class="fixed inset-0 bg-black/25 dark:bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4">
    <div class="mac-modal w-[620px] max-h-[90vh] overflow-y-auto bg-white dark:bg-[#323234] rounded-xl shadow-xl">
      <div class="p-6 space-y-4">
        <div>
          <h3 class="text-base font-semibold text-gray-900 dark:text-white">转写参数卡（可选）</h3>
          <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
            只填写已经确认的信息。留空也能转写；未确认的价格、成色和链接号会进入待回听清单。
          </p>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <label class="text-xs text-gray-600 dark:text-gray-300">
            商品和型号
            <input class="mt-1 w-full rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-[#252527] px-3 py-2 text-sm" bind:value={factProducts} placeholder="佳能R6二代、佳能70-200 F2.8" />
          </label>
          <label class="text-xs text-gray-600 dark:text-gray-300">
            已确认价格
            <input class="mt-1 w-full rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-[#252527] px-3 py-2 text-sm" bind:value={factPrices} placeholder="5839、6999" />
          </label>
          <label class="text-xs text-gray-600 dark:text-gray-300">
            已确认成色
            <input class="mt-1 w-full rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-[#252527] px-3 py-2 text-sm" bind:value={factConditions} placeholder="99新、95新" />
          </label>
          <label class="text-xs text-gray-600 dark:text-gray-300">
            已确认链接号
            <input class="mt-1 w-full rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-[#252527] px-3 py-2 text-sm" bind:value={factLinks} placeholder="56、1、2" />
          </label>
        </div>
        <label class="block text-xs text-gray-600 dark:text-gray-300">
          已确认库存表述
          <input class="mt-1 w-full rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-[#252527] px-3 py-2 text-sm" bind:value={factInventory} placeholder="在仓现货" />
        </label>
        <label class="block text-xs text-gray-600 dark:text-gray-300">
          直播间别名（每行一个）
          <textarea class="mt-1 w-full h-20 resize-none rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-[#252527] px-3 py-2 text-sm" bind:value={factAliases} placeholder={'二四七零二点八=24-70 F2.8\nR六二代=R6二代'}></textarea>
        </label>
        {#if factCardError}
          <p class="text-xs text-red-600">{factCardError}</p>
        {/if}
        <div class="flex justify-end gap-3 pt-1">
          <button class="px-4 py-2 text-sm text-gray-600 dark:text-gray-300 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700" on:click={() => (showFactCardModal = false)}>取消</button>
          <button class="px-4 py-2 text-sm text-white bg-emerald-600 hover:bg-emerald-700 rounded-lg" on:click={startTranscriptWithFactCard}>保存并生成文稿</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- 生成完整录播Modal -->
<GenerateWholeClipModal
  bind:showModal={showWholeClipModal}
  archive={wholeClipArchive}
  roomId={wholeClipArchive?.room_id || ""}
  platform={wholeClipArchive?.platform || ""}
  on:generated={handleWholeClipGenerated}
/>

<ImportVideoDialog
  bind:showDialog={showImportDialog}
  roomId={null}
  dialogTitle="导入录播视频"
  defaultAnalysisPurpose={importDefaultAnalysisPurpose}
  titlePlaceholder={importTitlePlaceholder}
  showMasterImportOption={false}
  on:imported={handleArchiveVideoImported}
/>

<style>
  /* fixed icon size in tables */
  :global(.table-icon) {
    width: 1rem; /* 16px, same as Tailwind w-4 */
    height: 1rem; /* 16px, same as Tailwind h-4 */
    flex: 0 0 auto;
  }

  .archive-row-selected {
    background: var(--mac-blue-soft);
    box-shadow: inset 3px 0 0 var(--mac-blue);
  }

  .archive-row-selected:hover {
    background: rgba(0, 113, 227, 0.14);
  }

  :global(.dark) .archive-row-selected {
    background: var(--mac-blue-soft);
    box-shadow: inset 3px 0 0 var(--mac-blue);
  }

  :global(.dark) .archive-row-selected:hover {
    background: rgba(10, 132, 255, 0.22);
  }

  .profile-streams-stack {
    display: grid;
    gap: 14px;
    min-width: 0;
  }

  .directory-state {
    display: grid;
    min-height: 220px;
    place-items: center;
    align-content: center;
    gap: 9px;
    padding: 28px;
    color: var(--mac-secondary);
    text-align: center;
  }

  .directory-state strong { color: var(--mac-label); font-size: 15px; }
  .directory-state span { max-width: 520px; font-size: 12px; line-height: 1.5; }
  .directory-state .mac-btn { margin-top: 5px; }

  .pending-profile-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 20px;
  }

  .pending-profile-header > div { min-width: 0; }
  .pending-profile-eyebrow {
    margin: 0 0 4px;
    color: var(--mac-orange);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }
  .pending-profile-header h2 { margin: 0; color: var(--mac-label); font-size: 22px; }
  .pending-profile-header p:not(.pending-profile-eyebrow) {
    margin: 7px 0 0;
    color: var(--mac-secondary);
    font-size: 12px;
    line-height: 1.55;
  }

  .archive-analysis-status {
    display: inline-flex;
    min-height: 24px;
    align-items: center;
    padding: 0 7px;
    border: 1px solid var(--mac-separator);
    border-radius: 999px;
    color: var(--mac-secondary);
    background: var(--mac-fill);
    font-size: 10px;
    font-weight: 650;
    white-space: nowrap;
  }
  .archive-analysis-status.is-progress { color: var(--mac-blue); background: var(--mac-blue-soft); }
  .archive-analysis-status.is-complete {
    border-color: rgba(20, 125, 69, 0.30);
    color: #0b6b38;
    background: rgba(48, 209, 88, 0.18);
    font-weight: 750;
  }

  .skill-rating-form,
  .skill-evidence-history { display: grid; gap: 14px; max-height: 72vh; overflow-y: auto; padding-right: 2px; }
  .skill-setup-wizard { display: grid; gap: 16px; }
  .skill-setup-steps { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; padding: 0; margin: 0; list-style: none; }
  .skill-setup-steps li { display: flex; align-items: center; justify-content: center; gap: 6px; min-width: 0; padding: 8px 5px; border-radius: 9px; color: var(--mac-secondary); background: var(--mac-fill); font-size: 11px; font-weight: 650; text-align: center; }
  .skill-setup-steps li span { display: inline-grid; flex: 0 0 auto; width: 18px; height: 18px; place-items: center; border: 1px solid currentColor; border-radius: 50%; font-size: 10px; }
  .skill-setup-steps li.active { color: var(--mac-blue); background: var(--mac-blue-soft); }
  .skill-setup-steps li.complete { color: var(--mac-green); }
  .skill-setup-copy { display: grid; gap: 5px; }
  .skill-setup-copy strong { color: var(--mac-label); font-size: 14px; }
  .skill-setup-copy p { margin: 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  .skill-ai-rubric { padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--mac-blue) 22%, var(--mac-separator)); border-radius: var(--mac-radius-md); color: var(--mac-secondary); background: color-mix(in srgb, var(--mac-blue) 5%, var(--mac-bg)); font-size: 12px; line-height: 1.55; }
  .skill-ai-rubric summary { cursor: pointer; color: var(--mac-label); font-weight: 650; }
  .skill-ai-rubric p { margin: 8px 0 0; }
  .skill-setup-session-list { display: grid; gap: 7px; min-width: 0; max-height: min(420px, calc(90vh - 340px)); margin: 0; padding: 0 4px 0 0; border: 0; overflow-y: auto; overscroll-behavior: contain; }
  .skill-setup-session-list legend { margin-bottom: 2px; color: var(--mac-secondary); font-size: 11px; font-weight: 650; }
  .skill-setup-session { display: flex; align-items: flex-start; gap: 9px; padding: 10px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md); background: var(--mac-bg); cursor: pointer; }
  .skill-setup-session:hover { background: var(--mac-fill); }
  .skill-setup-session input { width: 16px; height: 16px; margin-top: 2px; accent-color: var(--mac-blue); }
  .skill-setup-session > span { display: grid; min-width: 0; gap: 3px; }
  .skill-setup-session strong { color: var(--mac-label); font-size: 12px; }
  .skill-setup-session small { color: var(--mac-secondary); font-size: 11px; }
  .skill-setup-selection-status { margin: -4px 0 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.45; }
  .skill-setup-warning { margin: 0; padding: 9px 10px; border-radius: var(--mac-radius-md); color: var(--mac-orange); background: color-mix(in srgb, var(--mac-orange) 8%, var(--mac-bg-card)); font-size: 11px; line-height: 1.5; }
  .skill-setup-summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); margin: 0; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md); overflow: hidden; }
  .skill-setup-summary > div { min-width: 0; padding: 10px; background: var(--mac-bg); }
  .skill-setup-summary > div + div { border-left: 1px solid var(--mac-separator); }
  .skill-setup-summary dt { color: var(--mac-secondary); font-size: 10px; font-weight: 600; }
  .skill-setup-summary dd { margin: 5px 0 0; color: var(--mac-label); font-size: 14px; font-weight: 700; }
  .skill-setup-evidence-list { display: grid; gap: 7px; }
  .skill-setup-evidence-list article { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 9px 10px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md); background: var(--mac-bg); }
  .skill-setup-evidence-list strong { min-width: 0; color: var(--mac-label); font-size: 12px; overflow-wrap: anywhere; }
  .skill-setup-evidence-list span { flex: 0 0 auto; color: var(--mac-secondary); font-size: 11px; white-space: nowrap; }
  .skill-setup-preview { display: grid; gap: 8px; padding: 18px; border: 1px solid var(--mac-blue); border-radius: var(--mac-radius-lg); color: var(--mac-label); background: var(--mac-blue-soft); text-align: center; }
  .skill-setup-preview strong { font-size: 15px; }
  .skill-setup-preview p { margin: 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  .skill-rating-notice,
  .skill-evidence-history > p {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-md);
    color: var(--mac-secondary);
    background: var(--mac-blue-soft);
    font-size: 11px;
    line-height: 1.55;
  }
  .skill-rating-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .skill-rating-grid label,
  .skill-rating-basis { display: grid; gap: 6px; min-width: 0; }
  .skill-rating-grid label > span,
  .skill-rating-basis > span,
  .evidence-picker legend { color: var(--mac-secondary); font-size: 11px; font-weight: 650; }
  .skill-rating-basis textarea { min-height: 82px; padding-top: 9px; resize: vertical; }
  .evidence-picker { min-width: 0; margin: 0; padding: 0; border: 0; }
  .evidence-picker legend { margin-bottom: 7px; }
  .evidence-picker-list { display: grid; gap: 7px; max-height: 240px; overflow-y: auto; }
  .evidence-option {
    display: flex;
    min-width: 0;
    align-items: flex-start;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-md);
    background: var(--mac-bg-card);
    cursor: pointer;
  }
  .evidence-option:hover { background: var(--mac-fill); }
  .evidence-option input,
  .review-confirmation input { flex: 0 0 auto; width: 18px; height: 18px; margin-top: 1px; accent-color: var(--mac-blue); }
  .evidence-option > span { display: grid; min-width: 0; gap: 3px; }
  .evidence-option strong { color: var(--mac-label); font-size: 11px; }
  .evidence-option small { color: var(--mac-secondary); font-size: 10px; line-height: 1.45; overflow-wrap: anywhere; }
  .review-confirmation {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    color: var(--mac-label);
    font-size: 11px;
    line-height: 1.5;
    cursor: pointer;
  }
  .evidence-empty {
    padding: 20px;
    border: 1px dashed var(--mac-separator-strong);
    border-radius: var(--mac-radius-md);
    color: var(--mac-secondary);
    background: var(--mac-fill);
    font-size: 11px;
    line-height: 1.55;
    text-align: center;
  }
  .skill-rating-error { margin: 0; color: var(--mac-red); font-size: 11px; }
  .skill-modal-actions { display: flex; justify-content: flex-end; gap: 9px; padding-top: 10px; border-top: 1px solid var(--mac-separator); background: var(--mac-bg-card); }
  .rating-version-card {
    display: grid;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-lg);
    background: var(--mac-bg-card);
  }
  .rating-version-card header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .rating-version-card header > div { display: grid; gap: 3px; }
  .rating-version-card header strong { color: var(--mac-label); font-size: 12px; }
  .rating-version-card header span { color: var(--mac-secondary); font-size: 10px; }
  .rating-version-card > p { margin: 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .version-evidence-list { display: grid; gap: 6px; }
  .version-evidence-list button {
    display: flex;
    min-height: 44px;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid var(--mac-separator);
    border-radius: 9px;
    color: var(--mac-blue);
    background: var(--mac-fill);
    font: inherit;
    font-size: 10px;
    text-align: left;
    cursor: pointer;
  }
  .version-evidence-list button:hover { background: var(--mac-fill-hover); }
  .version-evidence-list button:focus-visible { outline: 3px solid var(--mac-blue-soft); outline-offset: 2px; }
  .version-evidence-list button > span:first-child { display: grid; min-width: 0; gap: 2px; }
  .version-evidence-list button strong { color: var(--mac-label); font-size: 11px; }
  .version-evidence-list button small { color: var(--mac-secondary); overflow-wrap: anywhere; }
  .rating-version-excluded {
    margin: 0;
    padding: 8px 10px;
    border-radius: 8px;
    color: var(--mac-red);
    background: color-mix(in srgb, var(--mac-red) 10%, transparent);
    font-size: 11px;
    font-weight: 650;
    line-height: 1.45;
  }

  @media (max-width: 820px) {
    .pending-profile-header { align-items: stretch; flex-direction: column; }
    .skill-rating-grid { grid-template-columns: 1fr; }
    .skill-setup-steps { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .skill-setup-summary { grid-template-columns: 1fr; }
    .skill-setup-summary > div + div { border-top: 1px solid var(--mac-separator); border-left: 0; }
    .skill-setup-evidence-list article { display: grid; gap: 4px; }
    .skill-setup-evidence-list span { white-space: normal; }
  }

</style>
