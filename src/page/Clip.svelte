<script lang="ts">
  import { invoke, invokeSensitive, TAURI_ENV, get_static_url } from "../lib/invoker";
  import { isClipVideo, type VideoItem } from "../lib/interface";
  import {
    anchorStatusLabel,
    canManuallyEditAnchor,
    shouldAutoDetectAnchor,
  } from "../lib/anchorDetection";
  import {
    normalizeNasArchiveView,
    type VideoArchiveRow,
  } from "../lib/nasStorage";
  import { parseImportedVideoNote } from "../lib/importedArchive";
  import ImportVideoDialog from "../lib/components/ImportVideoDialog.svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import {
    createMasterSampleBatch,
    getMasterSampleBatch,
    generateMasterSampleBatchDraft,
    getMasterBaseline,
    publishMasterSampleBatchDraft,
    publishCorrectedMasterSampleBatchVersion,
    listMasterSampleBatches,
    listUnbatchedMasterSourceVideoIds,
    startMasterSampleBatchProcessing,
    canActivateEnterpriseMaster,
    enterpriseMasterResumeAction,
    hostScriptAutomationAction,
    readyVideoIdsForEnterpriseMaster,
    shouldAutoGenerateEnterpriseDraft,
    masterBatchPurposeLabel,
    friendlyMasterError,
    buildTransactionMasterView,
    listSupportCandidates,
    type MasterSampleBatchDetail,
    type MasterSampleBatchSummary,
    type MasterSampleBatchPurpose,
    type BatchMasterDraft,
    type SupportCandidate,
  } from "../lib/masterScript";
  import { createEventDispatcher, onMount, onDestroy, tick } from "svelte";
  import ClipReviewWorkspace from "../lib/components/ClipReviewWorkspace.svelte";
  import {
    buildExistingClipReviewRequest,
    type ClipReviewRequest,
  } from "../lib/clipReview";
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
    Upload,
    Home,
    FileVideo,
    Scissors,
    Download,
    RotateCw,
    Edit,
    FileSearch,
    BookOpenCheck,
    Layers3,
    CheckCircle2,
    Circle,
    Loader2,
    FileText,
    Sparkles,
    FolderOpen,
    UserRound,
    X,
  } from "lucide-svelte";
  import { AnnotationOutline } from "flowbite-svelte-icons";
  import BilibiliIcon from "../lib/components/BilibiliIcon.svelte";
  import DouyinIcon from "../lib/components/DouyinIcon.svelte";
  import KuaishouIcon from "../lib/components/KuaishouIcon.svelte";
  import HuyaIcon from "../lib/components/HuyaIcon.svelte";
  import TikTokIcon from "../lib/components/TikTokIcon.svelte";

  export let reviewRequest: ClipReviewRequest | null = null;
  const dispatch = createEventDispatcher<{ closeReview: void }>();

  let videos: VideoItem[] = [];
  let filteredVideos: VideoItem[] = [];
  let loading = false;
  let openingClipReviewId: number | null = null;
  let sortBy = "created_at";
  let sortOrder = "desc";
  let selectedRoomId = null;
  let roomIds: string[] = [];

  let selectedVideos: Set<number> = new Set();
  let videoArchives = new Map<number, VideoArchiveRow>();
  let nasArchivePollingTimer: number | null = null;
  let deleteArchivedFile = false;
  let showDeleteConfirm = false;
  let videoToDelete: VideoItem | null = null;
  let showImportDialog = false;
  let backgroundImportFileName = "";
  let importAsMasterBatch = false;
  let masterSampleBatches: MasterSampleBatchSummary[] = [];
  let showMasterBatchDialog = false;
  let masterBatchVideoIds: number[] = [];
  let masterBatchTitle = "";
  let masterBatchHostLabel = "头部主播已审核样本";
  let masterBatchPurpose: MasterSampleBatchPurpose = "sample";
  let masterBatchTargetCount = 6;
  let creatingMasterBatch = false;
  let masterBatchError = "";
  let showMasterBatchProgress = false;
  let loadingMasterBatchProgress = false;
  let masterBatchProgressError = "";
  let selectedMasterSampleBatch: MasterSampleBatchDetail | null = null;
  let startingMasterBatch = false;
  let generatingMasterBatchDraft = false;
  let publishingMasterBatchDraft = false;
  let publishingCorrectedMasterBatch = false;
  let correctedMasterVersionPublished = false;
  let organizingHostScripts = false;
  let buildingEnterpriseMaster = false;
  let enterpriseBatchAwaitingAutoDraft: number | null = null;
  let masterBatchProgressPollingTimer: number | null = null;
  let masterBatchClockTimer: number | null = null;
  let masterBatchNow = Date.now();
  let transactionMasterCandidates: SupportCandidate[] = [];
  let masterBatchMemberships = new Map<number, {
    batchId: number;
    batchTitle: string;
    batchStatus: string;
    processingStatus: string;
  }>();

  // 编辑备注相关状态
  let showEditNoteDialog = false;
  let videoToEditNote: VideoItem | null = null;
  let editingNote = "";
  let anchorDetectionQueueRunning = false;
  let showEditAnchorDialog = false;
  let videoToEditAnchor: VideoItem | null = null;
  let editingAnchorName = "";

  onMount(async () => {
    masterBatchClockTimer = window.setInterval(() => {
      masterBatchNow = Date.now();
    }, 1000);
    await Promise.all([loadVideos(), loadMasterSampleBatches(), loadVideoArchives()]);
    nasArchivePollingTimer = window.setInterval(loadVideoArchives, 5000);
  });

  onDestroy(() => {
    stopProgressPolling();
    stopMasterBatchProgressPolling();
    if (masterBatchClockTimer !== null) {
      window.clearInterval(masterBatchClockTimer);
      masterBatchClockTimer = null;
    }
    if (nasArchivePollingTimer !== null) {
      window.clearInterval(nasArchivePollingTimer);
      nasArchivePollingTimer = null;
    }
  });

  let importProgressInfo = null;
  let progressPollingTimer = null;

  /**
   * 响应式语句：当检测到转换任务时，确保 loading 状态为 true
   */
  /**
   * 启动进度轮询定时器
   * 每3秒检查一次导入任务进度
   */
  function startProgressPolling() {
    if (progressPollingTimer) {
      clearTimeout(progressPollingTimer);
    }
    progressPollingTimer = setTimeout(async () => {
      await checkImportProgress();
    }, 3000);
  }

  /**
   * 检查导入任务进度
   * 如果任务仍在进行中，更新进度信息并继续轮询
   * 如果任务完成，停止轮询并重新加载视频列表
   */
  async function checkImportProgress() {
    try {
      const importProgress = await invoke("get_import_progress");

      if (importProgress) {
        // 仍有进行中的任务，更新进度信息并继续轮询
        importProgressInfo = importProgress;
        startProgressPolling();
      } else {
        // 任务已完成，延迟100ms确保状态同步后再重新加载
        importProgressInfo = null;
        stopProgressPolling();

        // 延迟处理避免状态竞争
        setTimeout(async () => {
          // 重新加载视频列表（不包括进度检查）
          await loadVideoList();
        }, 100);
      }
    } catch (error) {
      console.error("轮询检查进度失败:", error);

      // 不要立即重置状态，避免在网络错误时丢失进度
      // 保持现有状态一段时间，然后重置
      setTimeout(() => {
        importProgressInfo = null;
        stopProgressPolling();
      }, 5000);
    }
  }

  /**
   * 停止进度轮询定时器
   */
  function stopProgressPolling() {
    if (progressPollingTimer) {
      clearTimeout(progressPollingTimer);
      progressPollingTimer = null;
    }
  }

  /**
   * 格式化毫秒转换为可读的时间长度
   * @param milliseconds 毫秒数
   * @returns 格式化的时间字符串（如：1小时30分20秒）
   */
  function formatProgressDuration(milliseconds: number): string {
    const seconds = Math.floor(milliseconds / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (hours > 0) {
      return `${hours}小时${minutes % 60}分${seconds % 60}秒`;
    } else if (minutes > 0) {
      return `${minutes}分${seconds % 60}秒`;
    } else {
      return `${seconds}秒`;
    }
  }

  /**
   * 加载视频列表
   * 1. 扫描导入目录中的新视频文件并自动导入
   * 2. 检查是否有正在进行的转换任务
   * 3. 加载并显示所有视频
   */
  async function loadVideos() {
    loading = true;
    try {
      await loadVideoList();
    } catch (error) {
      console.error("Failed to load videos:", error);
      importProgressInfo = null;
      stopProgressPolling();
      loading = false;
    }
  }

  /**
   * 加载视频数据列表
   * 从后端获取所有视频数据，包含封面信息，并应用筛选条件
   */
  async function loadVideoList() {
    try {
      // 获取所有视频
      const allVideos: VideoItem[] = [];
      const roomIdsSet = new Set<string>();
      const tempVideos = await invoke<VideoItem[]>("get_all_videos");

      for (const video of tempVideos) {
        video.cover = await get_static_url("output", video.cover);
      }

      for (const video of tempVideos) {
        roomIdsSet.add(video.room_id);
        allVideos.push(video);
      }

      videos = allVideos;
      roomIds = Array.from(roomIdsSet).sort();

      applyFilters();
      void runPendingAnchorDetections();
    } catch (error) {
      console.error("加载视频列表失败:", error);
      throw error;
    } finally {
      // 只有在没有转换任务且没有轮询定时器时才设置 loading = false
      loading = false;
    }
  }

  async function loadVideoArchives(): Promise<void> {
    try {
      const rows = await invoke<VideoArchiveRow[]>("list_video_archives");
      videoArchives = new Map(rows.map((row) => [row.videoId, row]));
    } catch (error) {
      console.error("Failed to load NAS archive states:", error);
    }
  }

  async function retryVideoArchive(videoId: number): Promise<void> {
    try {
      await invoke("retry_video_archive", { videoId });
      await loadVideoArchives();
    } catch (error) {
      alert(`无法重新转存：${error}`);
    }
  }

  function replaceVideoInLists(updated: VideoItem): void {
    const current = videos.find((video) => video.id === updated.id);
    const normalized = {
      ...updated,
      cover: current?.cover || updated.cover,
    };
    videos = videos.map((video) =>
      video.id === normalized.id ? normalized : video
    );
    applyFilters();
  }

  async function detectAnchor(
    video: VideoItem,
    force = false,
  ): Promise<VideoItem | null> {
    replaceVideoInLists({
      ...video,
      anchor_detection_status: "running",
      anchor_detection_error: "",
    });
    try {
      const updated = await invoke<VideoItem>("detect_video_anchor", {
        videoId: video.id,
        force,
      });
      replaceVideoInLists(updated);
      return updated;
    } catch (error) {
      console.error("Failed to detect video anchor:", error);
      replaceVideoInLists({
        ...video,
        anchor_detection_status: "failed",
        anchor_detection_error: String(error),
      });
      return null;
    }
  }

  async function runPendingAnchorDetections(): Promise<void> {
    if (anchorDetectionQueueRunning) return;
    const pending = videos.filter(shouldAutoDetectAnchor);
    if (pending.length === 0) return;

    anchorDetectionQueueRunning = true;
    try {
      for (const video of pending) {
        const updated = await detectAnchor(
          video,
          video.anchor_detection_status === "running",
        );
        if (
          updated?.anchor_detection_error.includes("API Key") ||
          updated?.anchor_detection_error.includes("尚未配置")
        ) {
          break;
        }
      }
    } finally {
      anchorDetectionQueueRunning = false;
    }
  }

  function openEditAnchorDialog(video: VideoItem): void {
    videoToEditAnchor = video;
    editingAnchorName = video.anchor_name || "";
    showEditAnchorDialog = true;
  }

  function closeEditAnchorDialog(): void {
    showEditAnchorDialog = false;
    videoToEditAnchor = null;
    editingAnchorName = "";
  }

  async function saveAnchorName(): Promise<void> {
    if (!videoToEditAnchor) return;
    try {
      const updated = await invoke<VideoItem>("save_video_anchor_manual", {
        videoId: videoToEditAnchor.id,
        anchorName: editingAnchorName,
      });
      replaceVideoInLists(updated);
      closeEditAnchorDialog();
    } catch (error) {
      alert(`主播姓名无法保存：${String(error)}`);
    }
  }

  async function openVideoArchiveLocation(videoId: number): Promise<void> {
    try {
      await invoke("open_video_archive_location", { videoId });
    } catch (error) {
      alert(String(error));
    }
  }

  function selectedVideosIncludeNasArchive(): boolean {
    if (videoToDelete) {
      return videoArchives.get(videoToDelete.id)?.status === "archived";
    }
    return Array.from(selectedVideos).some(
      (id) => videoArchives.get(id)?.status === "archived",
    );
  }

  function applyFilters() {
    let filtered = [...videos];

    // Apply room filter
    if (selectedRoomId !== null) {
      filtered = filtered.filter((video) => video.room_id === selectedRoomId);
    }

    // Apply sorting
    filtered.sort((a, b) => {
      let aValue: any, bValue: any;

      switch (sortBy) {
        case "title":
          aValue = a.title.toLowerCase();
          bValue = b.title.toLowerCase();
          break;
          bValue = b.note;
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
          aValue = a.created_at;
          bValue = b.created_at;
      }

      if (sortOrder === "asc") {
        return aValue > bValue ? 1 : -1;
      } else {
        return aValue < bValue ? 1 : -1;
      }
    });

    filteredVideos = filtered;
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
    return new Date(dateString).toLocaleString();
  }

  function formatPlatform(platform: string | undefined) {
    if (!platform) return "未知";
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
      case "imported":
        return "导入视频";
      case "clip":
        return "切片";
      default:
        return platform;
    }
  }

  function getRoomUrl(platform: string | undefined, roomId: string) {
    if (!platform) return null;
    if (roomId.startsWith("http")) return roomId;
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

  function toggleSort(field: string) {
    if (sortBy === field) {
      sortOrder = sortOrder === "asc" ? "desc" : "asc";
    } else {
      sortBy = field;
      sortOrder = "asc";
    }
    applyFilters();
  }

  function toggleVideoSelection(id: number) {
    if (selectedVideos.has(id)) {
      selectedVideos.delete(id);
    } else {
      selectedVideos.add(id);
    }
    selectedVideos = selectedVideos; // Trigger reactivity
  }

  function selectAllVideos() {
    const currentVideos = filteredVideos;
    if (selectedVideos.size === currentVideos.length) {
      selectedVideos.clear();
    } else {
      currentVideos.forEach((video) => selectedVideos.add(video.id));
    }
    selectedVideos = selectedVideos; // Trigger reactivity
  }

  async function deleteVideo(video: VideoItem) {
    try {
      await invokeSensitive("delete_video", {
        id: video.id,
        deleteArchivedFile,
      });
      await loadVideos();
      showDeleteConfirm = false;
      videoToDelete = null;
      deleteArchivedFile = false;
    } catch (error) {
      console.error("Failed to delete video:", error);
      alert(`删除失败：${error}`);
    }
  }

  async function deleteSelectedVideos() {
    try {
      const failures: string[] = [];
      for (const id of selectedVideos) {
        try {
          await invokeSensitive("delete_video", { id, deleteArchivedFile });
        } catch (error) {
          failures.push(`视频 ${id}：${error}`);
        }
      }
      selectedVideos.clear();
      await loadVideos();
      showDeleteConfirm = false;
      videoToDelete = null;
      deleteArchivedFile = false;
      if (failures.length > 0) {
        alert(`有 ${failures.length} 个视频删除失败：\n${failures.join("\n")}`);
      }
    } catch (error) {
      console.error("Failed to delete selected videos:", error);
    }
  }

  function isAbsoluteMediaPath(file: string): boolean {
    return /^(?:[a-zA-Z]:[\\/]|\\\\)/.test(file.trim());
  }

  async function resolveLocalMediaPath(file: string): Promise<string> {
    const trimmed = file.trim();
    if (!trimmed) throw new Error("视频没有文件路径");
    if (isAbsoluteMediaPath(trimmed)) return trimmed;
    const config = await invoke<{ output: string }>("get_config");
    const base = String(config?.output || "").replace(/[\\/]+$/, "");
    if (!base) throw new Error("未配置本地输出目录");
    return `${base}\\${trimmed.replace(/^[\\/]+/, "")}`;
  }

  async function playVideo(video: VideoItem) {
    try {
      // Always resolve via backend so archived NAS UNC paths can be remapped to Z:\
      // (filenames with [imported] break shell open / cmd start on raw UNC).
      await invoke("open_video_externally", { id: video.id });
    } catch (error) {
      console.error("Failed to open video externally:", error);
      try {
        const path = await resolveLocalMediaPath(video.file);
        await invoke("show_in_folder", { path });
      } catch (fallbackError) {
        alert(`无法打开视频：${fallbackError || error}`);
      }
    }
  }

  async function openExistingClipReview(video: VideoItem): Promise<void> {
    if (openingClipReviewId !== null) return;
    openingClipReviewId = video.id;
    try {
      const request = buildExistingClipReviewRequest(video, "");
      window.dispatchEvent(new CustomEvent("bsr:open-clip-review", { detail: request }));
    } catch (error) {
      alert(`无法打开切片复盘：${String(error).replace(/^Error:\s*/i, "")}`);
    } finally {
      openingClipReviewId = null;
    }
  }

  async function analyzeVideo(video: VideoItem) {
    if (!video?.id) {
      alert("无法分析：视频数据无效。");
      return;
    }
    if (isClipVideo(video)) {
      await openExistingClipReview(video);
      return;
    }
    const purpose = parseImportedVideoNote(video.note || "").analysisPurpose;
    const analysisMode = purpose === "competitor_benchmark" ? "legacy" : "company_deal";
    window.dispatchEvent(new CustomEvent("bsr:open-video-analysis", {
      detail: { video, analysisMode },
    }));
  }

  function buildMasterFromVideo(_video: VideoItem) {
    // 已废弃：整场视频不得直接设为母稿
    alert("已废弃「设为整场母稿」。请从成交话术精炼后，经切片复盘验证并人工确认，再勾选样本创建「母稿样本批次」入库。");
  }

  async function loadMasterSampleBatches() {
    try {
      masterSampleBatches = await listMasterSampleBatches();
      const details = await Promise.allSettled(
        masterSampleBatches.map((summary) => getMasterSampleBatch(summary.batch.id)),
      );
      const memberships = new Map<number, {
        batchId: number;
        batchTitle: string;
        batchStatus: string;
        processingStatus: string;
      }>();
      for (const detail of details) {
        if (detail.status !== "fulfilled") continue;
        for (const item of detail.value.items) {
          memberships.set(item.videoId, {
            batchId: detail.value.batch.id,
            batchTitle: detail.value.batch.title,
            batchStatus: detail.value.batch.status,
            processingStatus: item.processingStatus,
          });
        }
      }
      masterBatchMemberships = memberships;
    } catch (error) {
      console.error("加载母稿样本批次失败:", error);
    }
  }

  function masterBatchStatusLabel(
    status: string,
    purpose: MasterSampleBatchPurpose = "sample",
  ) {
    return {
      collecting: "正在收集样本",
      processing: "正在逐场整理",
      ready_for_synthesis: "可以生成最终母稿",
      draft_ready: "等待审核母稿草稿",
      published: purpose === "enterprise" ? "已发布公司统一话术" : "主播话术已整理",
      failed: "需要处理异常",
    }[status] || status;
  }

  function masterBatchItemStatusLabel(status: string) {
    return {
      pending: "等待逐场处理",
      transcribing: "正在转写",
      reviewing: "等待审核",
      ready: "本场已完成",
      failed: "本场处理异常",
    }[status] || status;
  }

  function masterBatchMembershipFor(videoId: number) {
    return masterBatchMemberships.get(videoId) || null;
  }

  function masterBatchProgress(detail: MasterSampleBatchDetail) {
    const total = detail.items.length;
    const collected = Math.min(total, detail.batch.targetSampleCount);
    const transcribed = detail.items.filter((item) => ["reviewing", "ready"].includes(item.processingStatus)).length;
    const structured = detail.items.filter((item) => item.processingStatus === "ready").length;
    const transcribing = detail.items.filter((item) => item.processingStatus === "transcribing").length;
    const synthesisGenerating = detail.synthesis?.status === "generating";
    const synthesisStarted = ["draft_ready", "published"].includes(detail.batch.status);
    const published = detail.batch.status === "published";
    const collectionRatio = detail.batch.targetSampleCount > 0 ? collected / detail.batch.targetSampleCount : 0;
    const transcriptRatio = total > 0 ? transcribed / total : 0;
    const structureRatio = total > 0 ? structured / total : 0;
    const percent = Math.round(
      Math.min(collectionRatio, 1) * 20
      + transcriptRatio * 35
      + structureRatio * 25
      + (synthesisStarted ? 15 : 0)
      + (published ? 5 : 0),
    );
    return { total, collected, transcribed, structured, transcribing, synthesisGenerating, synthesisStarted, published, percent };
  }

  function masterBatchSynthesisStatus(detail: MasterSampleBatchDetail): string | null {
    const synthesis = detail.synthesis;
    if (synthesis?.status !== "generating") return null;
    const startedAt = new Date(synthesis.updatedAt).getTime();
    const elapsed = Number.isFinite(startedAt)
      ? formatProgressDuration(Math.max(0, masterBatchNow - startedAt))
      : "刚刚";
    return `已提交给 MiniMax，正在等待模型返回（已等待 ${elapsed}）。系统每 2 秒检查一次状态；完成后会自动显示草稿，失败会显示原因。`;
  }

  function batchMasterDraft(detail: MasterSampleBatchDetail): BatchMasterDraft | null {
    const draftJson = detail.synthesis?.draftJson;
    if (!draftJson) return null;
    try {
      return JSON.parse(draftJson) as BatchMasterDraft;
    } catch (error) {
      console.error("母稿草稿解析失败:", error);
      return null;
    }
  }

  function stopMasterBatchProgressPolling() {
    if (masterBatchProgressPollingTimer !== null) {
      window.clearTimeout(masterBatchProgressPollingTimer);
      masterBatchProgressPollingTimer = null;
    }
  }

  function scheduleMasterBatchProgressPolling(batchId: number) {
    stopMasterBatchProgressPolling();
    masterBatchProgressPollingTimer = window.setTimeout(() => {
      void refreshMasterBatchProgress(batchId);
    }, 2000);
  }

  async function refreshMasterBatchProgress(batchId: number) {
    try {
      let detail = await getMasterSampleBatch(batchId);
      if (
        enterpriseBatchAwaitingAutoDraft === batchId
        && shouldAutoGenerateEnterpriseDraft(
          detail.batch.purpose,
          detail.batch.status,
          detail.synthesis?.status ?? null,
        )
      ) {
        enterpriseBatchAwaitingAutoDraft = null;
        detail = await generateMasterSampleBatchDraft(batchId);
      }
      selectedMasterSampleBatch = detail;
      correctedMasterVersionPublished = false;
      if (detail.batch.status === "published") {
        try {
          const baseline = await getMasterBaseline(`MS-BATCH-${detail.batch.id}`);
          correctedMasterVersionPublished = baseline.master.version !== "1.0.0";
          if (correctedMasterVersionPublished && canActivateEnterpriseMaster(detail.batch.purpose)) {
            localStorage.setItem("bsr:active-master", JSON.stringify({ scriptKey: baseline.master.scriptKey, masterScriptId: baseline.master.id }));
          }
        } catch {
          // The original published V1.0 remains usable even if a baseline lookup fails.
        }
      }
      await loadMasterSampleBatches();
      if (detail.batch.status === "processing" || detail.synthesis?.status === "generating") {
        scheduleMasterBatchProgressPolling(batchId);
      } else {
        stopMasterBatchProgressPolling();
      }
    } catch (error) {
      masterBatchProgressError = `无法读取批次进度：${String(error)}`;
      stopMasterBatchProgressPolling();
    }
  }

  async function openMasterBatchProgress(batchId: number) {
    showMasterBatchProgress = true;
    loadingMasterBatchProgress = true;
    masterBatchProgressError = "";
    selectedMasterSampleBatch = null;
    try {
      const [, candidates] = await Promise.all([
        refreshMasterBatchProgress(batchId),
        listSupportCandidates().catch(() => [] as SupportCandidate[]),
      ]);
      transactionMasterCandidates = candidates;
    } catch (error) {
      masterBatchProgressError = `无法读取批次进度：${String(error)}`;
    } finally {
      loadingMasterBatchProgress = false;
    }
  }

  function closeMasterBatchProgress() {
    showMasterBatchProgress = false;
    stopMasterBatchProgressPolling();
  }

  async function startSelectedMasterBatchProcessing() {
    if (!selectedMasterSampleBatch || startingMasterBatch) return;
    const isRetry =
      selectedMasterSampleBatch.batch.status === "failed" ||
      selectedMasterSampleBatch.batch.status === "processing";
    const action = isRetry ? "继续未完成场次" : "开始逐场处理";
    if (!confirm(`${action}会依次调用 ASR 服务处理尚未完成的整场直播。确认开始吗？`)) return;
    startingMasterBatch = true;
    masterBatchProgressError = "";
    try {
      selectedMasterSampleBatch = await startMasterSampleBatchProcessing(selectedMasterSampleBatch.batch.id);
      await loadMasterSampleBatches();
      scheduleMasterBatchProgressPolling(selectedMasterSampleBatch.batch.id);
    } catch (error) {
      masterBatchProgressError = `无法启动逐场处理：${String(error)}`;
    } finally {
      startingMasterBatch = false;
    }
  }

  async function generateSelectedMasterBatchDraft() {
    if (!selectedMasterSampleBatch || generatingMasterBatchDraft) return;
    if (!confirm("系统将根据全部样本的已校正逐字稿提炼规律，并调用 MiniMax 生成待审核母稿草稿。固定话术只会引用主播原话。确认开始吗？")) return;
    generatingMasterBatchDraft = true;
    masterBatchProgressError = "";
    try {
      selectedMasterSampleBatch = await generateMasterSampleBatchDraft(selectedMasterSampleBatch.batch.id);
      await loadMasterSampleBatches();
    } catch (error) {
      masterBatchProgressError = `无法生成最终母稿草稿：${String(error)}`;
      if (selectedMasterSampleBatch) {
        await refreshMasterBatchProgress(selectedMasterSampleBatch.batch.id);
      }
    } finally {
      generatingMasterBatchDraft = false;
    }
  }

  async function publishSelectedMasterBatchDraft() {
    if (!selectedMasterSampleBatch || publishingMasterBatchDraft) return;
    const isEnterprise = canActivateEnterpriseMaster(selectedMasterSampleBatch.batch.purpose);
    const hostRaw = (selectedMasterSampleBatch.batch.hostLabel || "").trim() || "该主播";
    const hostKb = hostRaw.endsWith("知识库") ? hostRaw : `${hostRaw}知识库`;
    const message = isEnterprise
      ? `确认已核对章节规律、固定原话和证据编号吗？发布后会写入 Obsidian「${hostKb}」的 视频/成交、话术、分析建议，并成为后续录播评分的企业母稿 V1.0。`
      : `确认已核对章节规律、固定原话和证据编号吗？该批次会写入 Obsidian「${hostKb}」的 视频/成交、话术、分析建议，不会成为后续录播的评分母稿。`;
    if (!confirm(message)) return;
    publishingMasterBatchDraft = true;
    masterBatchProgressError = "";
    try {
      const master = await publishMasterSampleBatchDraft(selectedMasterSampleBatch.batch.id);
      if (canActivateEnterpriseMaster(selectedMasterSampleBatch.batch.purpose)) {
        localStorage.setItem("bsr:active-master", JSON.stringify({ scriptKey: master.scriptKey, masterScriptId: master.id }));
      }
      selectedMasterSampleBatch = await getMasterSampleBatch(selectedMasterSampleBatch.batch.id);
      await loadMasterSampleBatches();
    } catch (error) {
      masterBatchProgressError = `无法发布到主播知识库：${String(error)}`;
    } finally {
      publishingMasterBatchDraft = false;
    }
  }

  async function publishCorrectedMasterBatchVersion() {
    if (!selectedMasterSampleBatch || publishingCorrectedMasterBatch) return;
    if (!confirm("系统将保留 V1.0，并把可确定的型号格式错误发布为 V1.1。无法确定的原话不会擅自改写。确认继续吗？")) return;
    publishingCorrectedMasterBatch = true;
    masterBatchProgressError = "";
    try {
      const master = await publishCorrectedMasterSampleBatchVersion(selectedMasterSampleBatch.batch.id);
      if (canActivateEnterpriseMaster(selectedMasterSampleBatch.batch.purpose)) {
        localStorage.setItem("bsr:active-master", JSON.stringify({ scriptKey: master.scriptKey, masterScriptId: master.id }));
      }
      correctedMasterVersionPublished = true;
      await loadMasterSampleBatches();
    } catch (error) {
      if (String(error).includes("已存在校正版本")) {
        await refreshMasterBatchProgress(selectedMasterSampleBatch.batch.id);
      } else {
        masterBatchProgressError = `无法发布校正母稿：${String(error)}`;
      }
    } finally {
      publishingCorrectedMasterBatch = false;
    }
  }

  function openMasterBatchDialog(videoIds = Array.from(selectedVideos), purpose: MasterSampleBatchPurpose = "sample") {
    const ids = Array.from(new Set(videoIds));
    if (ids.length === 0) {
      alert("请至少勾选已整理的成交话术样本/证据视频，再创建母稿样本批次。");
      return;
    }
    masterBatchVideoIds = ids;
    const date = new Date().toLocaleDateString("zh-CN").replaceAll("/", "-");
    masterBatchPurpose = purpose;
    masterBatchTitle = purpose === "enterprise"
      ? `金典拍拍直播母稿 ${date}`
      : `头部主播样本批次 ${date}`;
    masterBatchHostLabel = "";
    masterBatchTargetCount = Math.max(6, ids.length);
    masterBatchError = "";
    showMasterBatchDialog = true;
  }

  async function organizeHostScripts() {
    if (organizingHostScripts) return;
    const sourceBatches = masterSampleBatches.filter((summary) => summary.batch.purpose === "sample");
    if (sourceBatches.length === 0) {
      alert("暂时没有可整理的主播录播。请先导入并完成至少一场头部主播录播的转写。");
      return;
    }

    organizingHostScripts = true;
    let focusBatchId = sourceBatches[0].batch.id;
    try {
      for (const summary of sourceBatches) {
        const action = hostScriptAutomationAction(summary.batch.status);
        if (action === "process") {
          await startMasterSampleBatchProcessing(summary.batch.id);
          focusBatchId = summary.batch.id;
        } else if (action === "generate") {
          await generateMasterSampleBatchDraft(summary.batch.id);
          focusBatchId = summary.batch.id;
        } else if (action === "review" || action === "wait") {
          focusBatchId = summary.batch.id;
        }
      }
      await loadMasterSampleBatches();
      await openMasterBatchProgress(focusBatchId);
    } catch (error) {
      alert(`主播话术整理未能启动：${String(error)}`);
    } finally {
      organizingHostScripts = false;
    }
  }

  async function openEnterpriseMasterBatchDialog() {
    if (buildingEnterpriseMaster) return;
    buildingEnterpriseMaster = true;
    try {
      const existingBatches = await Promise.all(
        masterSampleBatches
          .filter((summary) => summary.batch.purpose === "enterprise" && summary.batch.status !== "published")
          .map((summary) => getMasterSampleBatch(summary.batch.id)),
      );
      const actionPriority = { review: 0, generate: 1, wait: 2, ready: 3, none: 4 };
      const resumableBatch = existingBatches
        .map((detail) => ({
          detail,
          action: enterpriseMasterResumeAction(
            detail.batch.purpose,
            detail.batch.status,
            detail.synthesis?.status ?? null,
          ),
        }))
        .filter((entry) => entry.action !== "none")
        .sort((left, right) => actionPriority[left.action] - actionPriority[right.action])[0];
      if (resumableBatch) {
        if (resumableBatch.action === "generate") {
          await generateMasterSampleBatchDraft(resumableBatch.detail.batch.id);
        }
        await openMasterBatchProgress(resumableBatch.detail.batch.id);
        return;
      }

      const details = await Promise.all(
        masterSampleBatches
          .filter((summary) => (
            summary.batch.purpose === "sample"
            && ["ready_for_synthesis", "draft_ready", "published"].includes(summary.batch.status)
          ))
          .map((summary) => getMasterSampleBatch(summary.batch.id)),
      );
      const videoIds = readyVideoIdsForEnterpriseMaster(details);
      if (videoIds.length === 0) {
        alert("还没有可汇总的主播话术。请先点击“整理主播话术”，完成至少一位头部主播的录播整理。");
        return;
      }
      const hostLabel = details
        .map((detail) => (detail.batch.hostLabel || "").trim())
        .find((label) => label.length > 0) || "";
      if (!hostLabel) {
        alert("样本批次缺少主播名。请先在样本批次填写主播名（如：于千惠），再汇总公司统一话术。");
        return;
      }
      const date = new Date().toLocaleDateString("zh-CN").replaceAll("/", "-");
      const detail = await createMasterSampleBatch({
        title: `金典拍拍公司统一话术 ${date}`,
        hostLabel,
        purpose: "enterprise",
        targetSampleCount: videoIds.length,
        videoIds,
      });
      enterpriseBatchAwaitingAutoDraft = detail.batch.id;
      await startMasterSampleBatchProcessing(detail.batch.id);
      await loadMasterSampleBatches();
      await openMasterBatchProgress(detail.batch.id);
    } catch (error) {
      alert(`公司统一话术未能启动：${String(error)}`);
    } finally {
      buildingEnterpriseMaster = false;
    }
  }

  function startMasterBatchImport() {
    importAsMasterBatch = true;
    showImportDialog = true;
  }

  async function createSelectedMasterBatch() {
    if (!masterBatchTitle.trim()) {
      masterBatchError = "请填写样本批次名称。";
      return;
    }
    if (!masterBatchHostLabel.trim()) {
      masterBatchError = "请填写主播名（样本来源）。发布后将创建「{主播名}知识库」。";
      return;
    }
    creatingMasterBatch = true;
    masterBatchError = "";
    try {
      await createMasterSampleBatch({
        title: masterBatchTitle,
        hostLabel: masterBatchHostLabel,
        purpose: masterBatchPurpose,
        targetSampleCount: masterBatchTargetCount,
        videoIds: masterBatchVideoIds,
      });
      selectedVideos = new Set();
      showMasterBatchDialog = false;
      await loadMasterSampleBatches();
    } catch (error) {
      masterBatchError = String(error);
    } finally {
      creatingMasterBatch = false;
    }
  }

  async function handleVideoImported(event: CustomEvent<{ videoId?: number; videoIds?: number[] }>) {
    backgroundImportFileName = "";
    await loadVideos();
    const videoIds = event.detail?.videoIds || (event.detail?.videoId ? [event.detail.videoId] : []);
    if (importAsMasterBatch) {
      importAsMasterBatch = false;
      const existingSourceVideoIds = await listUnbatchedMasterSourceVideoIds();
      const batchVideoIds = Array.from(new Set([...existingSourceVideoIds, ...videoIds]));
      selectedVideos = new Set(batchVideoIds);
      openMasterBatchDialog(batchVideoIds);
      return;
    }
    if (videoIds.length !== 1 || !event.detail?.videoId) return;
    const importedVideo = videos.find((item) => item.id === event.detail.videoId)
      || await invoke<VideoItem>("get_video", { id: event.detail.videoId });
    analyzeVideo(importedVideo);
  }

  function handleImageError(event: Event) {
    // 如果图片加载失败，隐藏图片元素并显示默认图标
    const target = event.target as HTMLImageElement;
    target.style.display = "none";
    if (target.parentElement) {
      target.parentElement.innerHTML =
        '<svg class="w-6 h-6 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"></path></svg>';
    }
  }

  async function exportVideo(video: VideoItem) {
    // download video
    const video_url = await get_static_url("output", video.file);
    const video_name = video.title;
    const a = document.createElement("a");
    a.href = video_url;
    a.download = video_name;
    a.click();
  }

  // 编辑备注相关函数
  function openEditNoteDialog(video: VideoItem) {
    videoToEditNote = video;
    editingNote = video.note || "";
    showEditNoteDialog = true;
  }

  function closeEditNoteDialog() {
    showEditNoteDialog = false;
    videoToEditNote = null;
    editingNote = "";
  }

  async function saveNote() {
    if (!videoToEditNote) return;

    try {
      await invoke("update_video_note", {
        id: videoToEditNote.id,
        note: editingNote,
      });

      // 更新本地数据
      videoToEditNote.note = editingNote;

      // 更新筛选后的视频列表
      const index = filteredVideos.findIndex(
        (v) => v.id === videoToEditNote.id
      );
      if (index !== -1) {
        filteredVideos[index].note = editingNote;
      }

      // 更新所有视频列表
      const allIndex = videos.findIndex((v) => v.id === videoToEditNote.id);
      if (allIndex !== -1) {
        videos[allIndex].note = editingNote;
      }

      closeEditNoteDialog();
    } catch (error) {
      console.error("Failed to update video note:", error);
    }
  }

  // 键盘事件处理
  function handleKeydown(event: KeyboardEvent) {
    if (showEditNoteDialog && event.key === "Escape") {
      closeEditNoteDialog();
    }
    if (showEditAnchorDialog && event.key === "Escape") {
      closeEditAnchorDialog();
    }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events -->
<svelte:window on:keydown={handleKeydown} />

{#if reviewRequest}
  <ClipReviewWorkspace request={reviewRequest} on:close={() => dispatch("closeReview")} />
{/if}

<div class:hidden={Boolean(reviewRequest)} class="contents">
<PageShell
  title="切片"
  subtitle="管理所有录播产生的切片；如需生成切片，请从直播间列表进入录播预览页面操作。"
>
  <div slot="actions">
    <button
      type="button"
      class="mac-btn mac-btn-primary"
      on:click={organizeHostScripts}
      disabled={organizingHostScripts}
    >
      <Sparkles class="w-4 h-4" />
      <span>{organizingHostScripts ? "正在整理..." : "整理已入库主播话术"}</span>
    </button>
    <button
      type="button"
      class="mac-btn"
      on:click={openEnterpriseMasterBatchDialog}
      disabled={buildingEnterpriseMaster}
    >
      <Layers3 class="w-4 h-4" />
      <span>{buildingEnterpriseMaster ? "正在汇总..." : "汇总公司统一话术"}</span>
    </button>
    <button
      type="button"
      class="mac-btn mac-btn-success"
      on:click={() => (showImportDialog = true)}
    >
      <Upload class="w-4 h-4" />
      <span>导入视频</span>
    </button>
    <button
      type="button"
      class="mac-btn mac-btn-primary"
      on:click={loadVideos}
      disabled={loading}
    >
      <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      <span>刷新</span>
    </button>
  </div>

    {#if masterSampleBatches.length > 0}
      <section class="mac-card border-[color:var(--mac-blue)]/25 bg-[color:var(--mac-blue-soft)] px-4 py-3">
        <div class="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
          <span class="font-semibold text-[color:var(--mac-label)]">主播话术与公司统一话术</span>
          {#each masterSampleBatches.slice(0, 2) as summary}
            <span class="text-[color:var(--mac-secondary)]">
              {masterBatchPurposeLabel(summary.batch.purpose)} · {summary.batch.title}：{summary.sampleCount}/{summary.batch.targetSampleCount} 场已入库
            </span>
            <span class="text-[color:var(--mac-blue)]">
              {masterBatchStatusLabel(summary.batch.status, summary.batch.purpose)}
            </span>
            <button
              type="button"
              class="mac-btn"
              style="height:28px;padding:0 10px;font-size:12px"
              on:click={() => openMasterBatchProgress(summary.batch.id)}
            >查看进度</button>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Filters -->
    <div class="mac-card space-y-4 p-4">
      <div class="flex justify-between items-center flex-wrap gap-4">
        <div class="flex space-x-3">
          <select
            bind:value={selectedRoomId}
            on:change={applyFilters}
            class="mac-field cursor-pointer"
          >
            <option value={null}>所有直播间</option>
            {#each roomIds as roomId}
              <option value={roomId}>{roomId}</option>
            {/each}
          </select>
        </div>

        <div class="flex items-center space-x-2">
          <span class="text-sm text-[color:var(--mac-tertiary)]">排序:</span>
          <button
            type="button"
            class="px-3 py-1.5 text-sm font-medium rounded-[8px] transition-colors {sortBy ===
            'room_id'
              ? 'bg-[color:var(--mac-blue)] text-white'
              : 'bg-[color:var(--mac-fill)] text-[color:var(--mac-secondary)] hover:bg-[color:var(--mac-fill-hover)]'}"
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
            type="button"
            class="px-3 py-1.5 text-sm font-medium rounded-[8px] transition-colors {sortBy ===
            'title'
              ? 'bg-[color:var(--mac-blue)] text-white'
              : 'bg-[color:var(--mac-fill)] text-[color:var(--mac-secondary)] hover:bg-[color:var(--mac-fill-hover)]'}"
            on:click={() => toggleSort("title")}
          >
            文件名
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
            'note'
              ? 'bg-[color:var(--mac-blue)] text-white'
              : 'bg-[color:var(--mac-fill)] text-[color:var(--mac-secondary)] hover:bg-[color:var(--mac-fill-hover)]'}"
            on:click={() => toggleSort("note")}
          >
            备注
            {#if sortBy === "note"}
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
              ? 'bg-[color:var(--mac-blue)] text-white'
              : 'bg-[color:var(--mac-fill)] text-[color:var(--mac-secondary)] hover:bg-[color:var(--mac-fill-hover)]'}"
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
              ? 'bg-[color:var(--mac-blue)] text-white'
              : 'bg-[color:var(--mac-fill)] text-[color:var(--mac-secondary)] hover:bg-[color:var(--mac-fill-hover)]'}"
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
              ? 'bg-[color:var(--mac-blue)] text-white'
              : 'bg-[color:var(--mac-fill)] text-[color:var(--mac-secondary)] hover:bg-[color:var(--mac-fill-hover)]'}"
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

    <!-- Bulk Actions -->
    {#if selectedVideos.size > 0}
      <div class="mac-card flex items-center justify-between p-4">
        <span class="text-sm text-[color:var(--mac-tertiary)]">
          已选择 {selectedVideos.size} 个视频
        </span>
        <div class="flex items-center gap-2">
          <button type="button" class="mac-btn" on:click={() => openMasterBatchDialog()}>
            <Layers3 class="w-4 h-4" />
            <span>创建母稿样本批次</span>
          </button>
          <button
            type="button"
            class="mac-btn"
            style="border-color:transparent;background:var(--mac-red);color:#fff"
            on:click={() => {
              showDeleteConfirm = true;
              videoToDelete = null;
              deleteArchivedFile = false;
            }}
          >
            <Trash2 class="w-4 h-4" />
            <span>删除选中</span>
          </button>
        </div>
      </div>
    {/if}

    {#if backgroundImportFileName}
      <div class="mx-6 mb-3 flex items-center justify-between gap-3 rounded-lg border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-800 dark:border-blue-900 dark:bg-blue-950/40 dark:text-blue-200">
        <span class="min-w-0 truncate">正在后台导入：{backgroundImportFileName}</span>
        <span class="shrink-0 text-xs">可继续浏览、分析和操作其他录播</span>
      </div>
    {/if}

    <!-- Video List -->
    <div class="mac-card overflow-hidden">
      {#if loading}
        <div
          class="flex flex-col items-center justify-center p-12 space-y-4 text-gray-500 dark:text-gray-400"
        >
          {#if importProgressInfo}
            <!-- 优化的转换进度显示 -->
            <div class="text-center space-y-3 max-w-md">
              <!-- 主要信息：醒目的转换状态 -->
              <div class="flex items-center justify-center space-x-3">
                <RotateCw
                  class="w-7 h-7 text-blue-600 dark:text-blue-400 animate-spin"
                />
                <span
                  class="text-xl font-semibold text-blue-600 dark:text-blue-400"
                >
                  正在转换视频
                </span>
              </div>

              <!-- 副信息：文件名显示在小字部分 -->
              <div
                class="text-sm text-gray-500 dark:text-gray-400 break-all px-4"
              >
                {importProgressInfo.fileName ||
                  importProgressInfo.file_name ||
                  "正在准备..."}
              </div>
            </div>
          {:else}
            <!-- 普通加载状态 -->
            <RefreshCw class="w-8 h-8 animate-spin" />
            <span>加载中...</span>
          {/if}
        </div>
      {:else if filteredVideos.length === 0}
        <div
          class="flex flex-col items-center justify-center p-12 space-y-4 text-gray-500 dark:text-gray-400"
        >
          <Video class="w-12 h-12" />
          <h3 class="text-lg font-medium text-gray-900 dark:text-white">
            暂无视频
          </h3>
          <p class="text-sm">
            {selectedRoomId !== null
              ? "没有找到匹配的视频"
              : "还没有录制任何视频切片"}
          </p>
        </div>
      {:else}
        <div class="overflow-x-auto custom-scrollbar-light">
          <table class="w-full table-fixed whitespace-nowrap">
            <thead>
              <tr class="border-b border-gray-200 dark:border-gray-700/50">
                <th class="px-4 py-3 text-left w-12">
                  <input
                    type="checkbox"
                    checked={selectedVideos.size === filteredVideos.length &&
                      filteredVideos.length > 0}
                    on:change={selectAllVideos}
                    class="rounded border-gray-300 dark:border-gray-600"
                  />
                </th>
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-44"
                  >直播间</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-64"
                  >视频</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-36"
                  >主播</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-20"
                  >备注</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-20"
                  >时长</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-24"
                  >大小</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-28"
                  >创建时间</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-28"
                  >投稿状态</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-44"
                  >NAS 存储</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-40"
                  >母稿状态</th
                >
                <th
                  class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400 w-28"
                  >操作</th
                >
              </tr>
            </thead>
            <tbody class="divide-y divide-gray-200 dark:divide-gray-700/50">
              {#each filteredVideos as video (video.id)}
                {@const membership = masterBatchMembershipFor(video.id)}
                {@const nasArchive = videoArchives.get(video.id)}
                {@const nasView = normalizeNasArchiveView(nasArchive)}
                <tr
                  class="group hover:bg-[#f5f5f7] dark:hover:bg-[#3a3a3c] transition-colors"
                >
                  <td class="px-4 py-3 w-12">
                    <input
                      type="checkbox"
                      checked={selectedVideos.has(video.id)}
                      on:change={() => toggleVideoSelection(video.id)}
                      class="rounded border-gray-300 dark:border-gray-600"
                    />
                  </td>

                  <td class="px-4 py-3 w-28">
                    <div class="flex items-center space-x-2">
                      {#if video.platform === "imported"}
                        <FileVideo class="table-icon text-gray-400" />
                        <span class="text-sm text-gray-800 dark:text-gray-200"
                          >外部视频</span
                        >
                      {:else if video.platform === "clip"}
                        <Scissors class="table-icon text-gray-400" />
                        <span class="text-sm text-gray-800 dark:text-gray-200"
                          >视频切片</span
                        >
                      {:else if video.platform === "bilibili"}
                        <BilibiliIcon class="w-4 h-4 flex-shrink-0" />
                        {#if getRoomUrl(video.platform, video.room_id)}
                          <a
                            href={getRoomUrl(video.platform, video.room_id)}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-blue-500 hover:text-blue-700 text-sm"
                            title={`打开 ${formatPlatform(video.platform)} 直播间`}
                          >
                            {video.room_id}
                          </a>
                        {:else}
                          <span class="text-sm text-gray-900 dark:text-white"
                            >{video.room_id}</span
                          >
                        {/if}
                      {:else if video.platform === "douyin"}
                        <DouyinIcon class="w-4 h-4 flex-shrink-0" />
                        {#if getRoomUrl(video.platform, video.room_id)}
                          <a
                            href={getRoomUrl(video.platform, video.room_id)}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-blue-500 hover:text-blue-700 text-sm"
                            title={`打开 ${formatPlatform(video.platform)} 直播间`}
                          >
                            {video.room_id}
                          </a>
                        {:else}
                          <span class="text-sm text-gray-900 dark:text-white"
                            >{video.room_id}</span
                          >
                        {/if}
                      {:else if video.platform === "kuaishou"}
                        <KuaishouIcon class="w-4 h-4 flex-shrink-0" />
                        {#if getRoomUrl(video.platform, video.room_id)}
                          <a
                            href={getRoomUrl(video.platform, video.room_id)}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-blue-500 hover:text-blue-700 text-sm"
                            title={`打开 ${formatPlatform(video.platform)} 直播间`}
                          >
                            {video.room_id}
                          </a>
                        {:else}
                          <span class="text-sm text-gray-900 dark:text-white"
                            >{video.room_id}</span
                          >
                        {/if}
                      {:else if video.platform === "huya"}
                        <HuyaIcon class="w-4 h-4 flex-shrink-0" />
                        {#if getRoomUrl(video.platform, video.room_id)}
                          <a
                            href={getRoomUrl(video.platform, video.room_id)}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-blue-500 hover:text-blue-700 text-sm"
                            title={`打开 ${formatPlatform(video.platform)} 直播间`}
                          >
                            {video.room_id}
                          </a>
                        {:else}
                          <span class="text-sm text-gray-900 dark:text-white"
                            >{video.room_id}</span
                          >
                        {/if}
                      {:else if video.platform === "tiktok"}
                        <TikTokIcon class="w-5 h-5 flex-shrink-0" />
                        {#if getRoomUrl(video.platform, video.room_id)}
                          <a
                            href={getRoomUrl(video.platform, video.room_id)}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-blue-500 hover:text-blue-700 text-sm"
                            title={`打开 ${formatPlatform(video.platform)} 直播间`}
                          >
                            {video.room_id}
                          </a>
                        {:else}
                          <span class="text-sm text-gray-900 dark:text-white"
                            >{video.room_id}</span
                          >
                        {/if}
                      {:else}
                        <Globe class="table-icon text-gray-400" />
                        {#if getRoomUrl(video.platform, video.room_id)}
                          <a
                            href={getRoomUrl(video.platform, video.room_id)}
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-blue-500 hover:text-blue-700 text-sm"
                            title={`打开 ${formatPlatform(video.platform)} 直播间`}
                          >
                            {video.room_id}
                          </a>
                        {:else}
                          <span class="text-sm text-gray-900 dark:text-white"
                            >{video.room_id}</span
                          >
                        {/if}
                      {/if}
                    </div>
                  </td>

                  <td class="px-4 py-3 w-64">
                    <div class="flex items-center space-x-3 w-full">
                      <div
                        class="w-12 h-8 rounded-lg overflow-hidden bg-gray-100 dark:bg-gray-700 flex items-center justify-center flex-shrink-0"
                      >
                        {#if video.cover && video.cover.trim() !== ""}
                          <img
                            src={video.cover}
                            alt="封面"
                            class="w-full h-full object-cover"
                            on:error={handleImageError}
                          />
                        {:else}
                          <!-- 默认视频图标 -->
                          <svg
                            class="w-6 h-6 text-gray-400"
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                          >
                            <path
                              stroke-linecap="round"
                              stroke-linejoin="round"
                              stroke-width="2"
                              d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                            ></path>
                          </svg>
                        {/if}
                      </div>
                      <div class="min-w-0 flex-1 w-64">
                        <p
                          class="text-sm font-medium text-gray-900 dark:text-white truncate w-full"
                          title={video.title || video.file}
                        >
                          {video.title || video.file}
                        </p>
                        <p
                          class="text-xs text-gray-500 dark:text-gray-400 truncate w-full"
                          title={video.file}
                        >
                          {video.file}
                        </p>
                      </div>
                    </div>
                  </td>

                  <td class="px-4 py-3 w-36">
                    <div class="flex items-center gap-2 min-w-0">
                      <UserRound class="w-4 h-4 flex-shrink-0 text-gray-400" />
                      <div
                        class="min-w-0 flex-1"
                        title={video.anchor_detection_error || "主播识别状态"}
                      >
                        {#if canManuallyEditAnchor(video)}
                          <button
                            type="button"
                            class="block max-w-full truncate text-left text-sm font-medium text-blue-600 hover:text-blue-700 hover:underline dark:text-blue-400"
                            title="点击人工填写主播姓名"
                            on:click|stopPropagation={() => openEditAnchorDialog(video)}
                          >
                            未识别 · 点击修改
                          </button>
                        {:else}
                          <span
                            class="block truncate text-sm font-medium text-gray-800 dark:text-gray-100"
                          >
                            {video.anchor_name ||
                              anchorStatusLabel(video)}
                          </span>
                        {/if}
                      </div>
                      {#if video.anchor_detection_status === "running"}
                        <Loader2 class="w-4 h-4 animate-spin text-blue-600" />
                      {/if}
                    </div>
                  </td>

                  <td class="px-4 py-3 w-20">
                    <div class="flex items-center space-x-2">
                      <AnnotationOutline class="table-icon text-gray-400" />
                      <span class="text-sm text-gray-800 truncate"
                        >{video.note}</span
                      >
                    </div>
                  </td>

                  <td class="px-4 py-3 w-20">
                    <div class="flex items-center space-x-2">
                      <Clock class="table-icon text-gray-400" />
                      <span class="text-sm text-gray-800"
                        >{formatDuration(video.length)}</span
                      >
                    </div>
                  </td>

                  <td class="px-4 py-3 w-24">
                    <div class="flex items-center space-x-2">
                      <HardDrive class="table-icon text-gray-400" />
                      <span class="text-sm text-gray-800"
                        >{formatSize(video.size)}</span
                      >
                    </div>
                  </td>

                  <td class="px-4 py-3 w-28">
                    <div class="flex items-center space-x-2">
                      <Calendar class="table-icon text-gray-400" />
                      <span class="text-sm text-gray-800 truncate"
                        >{formatDate(video.created_at)}</span
                      >
                    </div>
                  </td>

                  <td class="px-4 py-3 w-28">
                    <div class="flex items-center space-x-2">
                      <Upload class="table-icon text-gray-400" />
                      {#if video.bvid}
                        <a
                          href={`https://www.bilibili.com/video/${video.bvid}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          class="text-blue-500 hover:text-blue-700 text-sm truncate"
                          title={video.bvid}
                        >
                          {video.bvid}
                        </a>
                      {:else}
                        <span class="text-gray-500 dark:text-gray-400 text-sm"
                          >未投稿</span
                        >
                      {/if}
                    </div>
                  </td>

                  <td class="px-4 py-3 w-44">
                    <div class="flex min-w-0 items-center gap-2">
                      <span
                        class:bg-emerald-500={nasView.tone === "success"}
                        class:bg-blue-500={nasView.tone === "info"}
                        class:bg-amber-500={nasView.tone === "warning"}
                        class:bg-red-500={nasView.tone === "danger"}
                        class:bg-gray-400={nasView.tone === "neutral"}
                        class="h-2 w-2 shrink-0 rounded-full"
                      ></span>
                      <button
                        class="min-w-0 truncate text-left text-sm text-gray-700"
                        title={nasView.detail}
                        disabled={nasArchive?.status !== "archived"}
                        on:click={() => openVideoArchiveLocation(video.id)}
                      >
                        {nasView.label}
                      </button>
                      {#if nasArchive?.status === "archived"}
                        <button
                          class="shrink-0 rounded p-1 hover:bg-gray-100"
                          title="打开 NAS 文件夹"
                          on:click={() => openVideoArchiveLocation(video.id)}
                        >
                          <FolderOpen class="h-4 w-4 text-emerald-600" />
                        </button>
                      {:else if nasView.canRetry}
                        <button
                          class="shrink-0 rounded p-1 hover:bg-gray-100"
                          title="立即重试"
                          on:click={() => retryVideoArchive(video.id)}
                        >
                          <RefreshCw class="h-4 w-4 text-blue-600" />
                        </button>
                      {/if}
                    </div>
                    {#if nasArchive?.status === "uploading" && video.size > 0}
                      <div class="mt-1 h-1 overflow-hidden rounded bg-gray-200">
                        <div
                          class="h-full bg-blue-500"
                          style={`width:${Math.min(100, Math.round((nasArchive.uploadedBytes / video.size) * 100))}%`}
                        ></div>
                      </div>
                    {/if}
                  </td>

                  <td class="px-4 py-3 w-40">
                    {#if membership}
                      <button
                        class="flex max-w-full items-center gap-2 text-left"
                        title={`已进入 ${membership.batchTitle}，${masterBatchItemStatusLabel(membership.processingStatus)}`}
                        on:click={() => openMasterBatchProgress(membership.batchId)}
                      >
                        <BookOpenCheck class="table-icon shrink-0 text-emerald-600" />
                        <span class="min-w-0">
                          <span class="block truncate text-sm font-medium text-emerald-700 dark:text-emerald-300">已入母稿库</span>
                          <span class="block truncate text-xs text-gray-500 dark:text-gray-400">{masterBatchItemStatusLabel(membership.processingStatus)}</span>
                        </span>
                      </button>
                    {:else}
                      <div class="flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400">
                        <Circle class="table-icon" />
                        <span>未入母稿库</span>
                      </div>
                    {/if}
                  </td>

                  <td class="px-4 py-3 w-44">
                    <div class="flex items-center space-x-2">
                      <button
                        type="button"
                        class="p-1.5 rounded-lg opacity-40 cursor-not-allowed"
                        title="已废弃：请从成交话术精炼后经切片复盘入库"
                        disabled
                        on:click={() => buildMasterFromVideo(video)}
                      >
                        <BookOpenCheck class="w-4 h-4 text-emerald-600" />
                      </button>
                      <button
                        type="button"
                        class="p-1.5 rounded-lg hover:bg-purple-500/10 transition-colors"
                        title={isClipVideo(video) ? "打开切片三栏复盘" : "分析整场视频"}
                        disabled={openingClipReviewId !== null}
                        on:click|stopPropagation={() => analyzeVideo(video)}
                      >
                        {#if openingClipReviewId === video.id}
                          <Loader2 class="w-4 h-4 animate-spin text-purple-500" />
                        {:else}
                          <FileSearch class="w-4 h-4 text-purple-500" />
                        {/if}
                      </button>
                      <button
                        type="button"
                        class="p-1.5 rounded-lg hover:bg-blue-500/10 transition-colors"
                        title="打开所在文件夹并用系统播放器播放"
                        on:click|stopPropagation={() => playVideo(video)}
                      >
                        <Play class="w-4 h-4 text-blue-500" />
                      </button>
                      <button
                        class="p-1.5 rounded-lg hover:bg-green-500/10 transition-colors"
                        title="编辑备注"
                        on:click={() => openEditNoteDialog(video)}
                      >
                        <Edit class="w-4 h-4 text-green-500" />
                      </button>
                      {#if !TAURI_ENV}
                        <button
                          class="p-1.5 rounded-lg hover:bg-blue-500/10 transition-colors"
                          title="导出"
                          on:click={async () => await exportVideo(video)}
                        >
                          <Download class="w-4 h-4 text-blue-500" />
                        </button>
                      {/if}
                      <button
                        class="p-1.5 rounded-lg hover:bg-red-500/10 transition-colors"
                        title="删除"
                        on:click={() => {
                          videoToDelete = video;
                          deleteArchivedFile = false;
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
</PageShell>
</div>

{#if showEditAnchorDialog && videoToEditAnchor}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4"
    role="presentation"
    on:click={closeEditAnchorDialog}
  >
    <section
      class="w-full max-w-md rounded-lg bg-white shadow-xl dark:bg-gray-800"
      role="dialog"
      aria-modal="true"
      aria-labelledby="edit-anchor-title"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <header
        class="flex items-center justify-between border-b border-gray-200 px-5 py-4 dark:border-gray-700"
      >
        <div>
          <h2
            id="edit-anchor-title"
            class="text-base font-semibold text-gray-900 dark:text-white"
          >
            确认主播姓名
          </h2>
          <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
            人工确认后，系统不会再次自动覆盖。
          </p>
        </div>
        <button
          class="p-1.5 rounded hover:bg-gray-100 dark:hover:bg-gray-700"
          title="关闭"
          on:click={closeEditAnchorDialog}
        >
          <X class="w-5 h-5" />
        </button>
      </header>
      <div class="px-5 py-4">
        <label
          for="anchor-name-input"
          class="block text-sm font-medium text-gray-700 dark:text-gray-200"
        >
          主播姓名
        </label>
        <input
          id="anchor-name-input"
          bind:value={editingAnchorName}
          maxlength="12"
          placeholder="例如：小鱼"
          class="mt-2 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-600 dark:bg-gray-900 dark:text-white"
        />
      </div>
      <footer
        class="flex justify-end gap-2 border-t border-gray-200 px-5 py-4 dark:border-gray-700"
      >
        <button
          class="rounded-lg border border-gray-300 px-4 py-2 text-sm text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          on:click={closeEditAnchorDialog}
        >
          取消
        </button>
        <button
          class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!editingAnchorName.trim()}
          on:click={saveAnchorName}
        >
          确认姓名
        </button>
      </footer>
    </section>
  </div>
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
          {#if videoToDelete}
            确定要删除视频 "{videoToDelete.title || videoToDelete.file}" 吗？
          {:else}
            确定要删除选中的 {selectedVideos.size} 个视频吗？
          {/if}
        </p>
        <p class="text-xs text-[color:var(--mac-red)]">此操作无法撤销。</p>
        {#if selectedVideosIncludeNasArchive()}
          <label class="mt-3 flex items-start gap-3 rounded-lg bg-amber-50 p-3 text-left text-sm text-amber-900 dark:bg-amber-950/40 dark:text-amber-100">
            <input
              type="checkbox"
              class="mt-0.5 h-4 w-4 rounded border-amber-300"
              bind:checked={deleteArchivedFile}
            />
            <span>
              <strong class="block font-medium">同时删除 NAS 上的视频文件</strong>
              不勾选时只移除系统记录，NAS 文件会保留。
            </span>
          </label>
        {/if}
      </div>
      <div class="flex justify-center gap-3">
        <button
          type="button"
          class="mac-btn w-24"
          on:click={() => {
            showDeleteConfirm = false;
            videoToDelete = null;
            deleteArchivedFile = false;
          }}
        >
          取消
        </button>
        <button
          type="button"
          class="mac-btn mac-btn-danger w-24"
          on:click={() => {
            if (videoToDelete) {
              deleteVideo(videoToDelete);
            } else {
              deleteSelectedVideos();
            }
          }}
        >
          删除
        </button>
      </div>
    </div>
  </MacModal>
{/if}

<!-- Edit Note Dialog -->
{#if showEditNoteDialog && videoToEditNote}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 bg-black/30 dark:bg-black/50 z-50 flex items-center justify-center p-4"
    on:click={closeEditNoteDialog}
  >
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div
      class="mac-modal w-[480px] max-w-full bg-white dark:bg-[#2d2d30] rounded-xl shadow-xl border border-gray-200 dark:border-gray-600 overflow-hidden"
      on:click|stopPropagation
      role="dialog"
      aria-labelledby="edit-note-title"
      aria-describedby="edit-note-description"
    >
      <!-- Header -->
      <div
        class="px-6 pt-6 pb-4 border-b border-gray-100 dark:border-gray-700/50"
      >
        <div class="flex items-center space-x-3">
          <div
            class="w-8 h-8 rounded-full bg-blue-100 dark:bg-blue-900/30 flex items-center justify-center"
          >
            <Edit class="w-4 h-4 text-blue-600 dark:text-blue-400" />
          </div>
          <div class="min-w-0 flex-1">
            <h3
              id="edit-note-title"
              class="text-base font-semibold text-gray-900 dark:text-white"
            >
              编辑切片备注
            </h3>
            <p
              id="edit-note-description"
              class="text-sm text-gray-500 dark:text-gray-400 truncate mt-0.5"
            >
              {videoToEditNote.title || videoToEditNote.file}
            </p>
          </div>
        </div>
      </div>

      <!-- Content -->
      <div class="px-6 py-5">
        <div class="space-y-3">
          <label
            for="edit-note-textarea"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            备注内容
          </label>
          <div class="relative">
            <textarea
              id="edit-note-textarea"
              bind:value={editingNote}
              placeholder="为这个切片添加备注信息，如高光时刻、重要内容等..."
              class="w-full px-3 py-2 text-sm
                     bg-white dark:bg-gray-800
                     border border-gray-300 dark:border-gray-500 rounded-md
                     text-gray-900 dark:text-white placeholder-gray-500 dark:placeholder-gray-400
                     focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500
                     transition-colors duration-150 resize-none"
              rows="5"
              on:keydown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                  saveNote();
                }
              }}
            ></textarea>
            <!-- Helper text -->
            <div class="mt-2 text-xs text-gray-500 dark:text-gray-400">
              按 ⌘+Enter 快速保存
            </div>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div
        class="px-6 py-4 bg-gray-50 dark:bg-gray-800 border-t border-gray-200 dark:border-gray-600"
      >
        <div class="flex justify-end space-x-3">
          <button
            class="w-24 px-4 py-2 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-600 rounded-lg transition-colors"
            on:click={closeEditNoteDialog}
          >
            取消
          </button>
          <button
            class="w-24 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded-lg transition-colors"
            on:click={saveNote}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- 导入视频对话框 -->
<ImportVideoDialog
  bind:showDialog={showImportDialog}
  roomId={selectedRoomId}
  on:imported={handleVideoImported}
  on:backgrounded={(event) => { backgroundImportFileName = event.detail.fileName; }}
/>

{#if showMasterBatchDialog}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4">
    <div class="mac-modal w-full max-w-lg overflow-hidden rounded-xl border border-gray-200 bg-white shadow-xl dark:border-gray-700 dark:bg-[#323234]">
      <div class="border-b border-gray-100 px-6 py-5 dark:border-gray-700">
        <h3 class="text-base font-semibold text-gray-900 dark:text-white">{masterBatchPurpose === "enterprise" ? "汇总金典拍拍企业母稿" : "建立头部主播样本批次"}</h3>
        <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {masterBatchPurpose === "enterprise"
            ? `已选 ${masterBatchVideoIds.length} 份已整理成交话术样本/证据，将直接复用现有逐字稿与证据，不会重新调用 ASR。发布后进入「{主播名}知识库」的 视频/成交、话术、分析建议（视频只写路径引用，不拷贝文件）。`
            : `已选 ${masterBatchVideoIds.length} 份样本。请确保已是整理后的成交话术证据。发布后进入「{主播名}知识库」的 视频/成交、话术、分析建议（非整场视频直接炼母稿；视频只引用路径）。`}
        </p>
      </div>
      <div class="space-y-4 px-6 py-5">
        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300">
          批次名称
          <input bind:value={masterBatchTitle} class="mt-1.5 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-600 dark:bg-gray-800 dark:text-white" />
        </label>
        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300">
          主播名（样本来源）
          <input bind:value={masterBatchHostLabel} placeholder="必填，例如：于千惠（将创建「于千惠知识库」）" class="mt-1.5 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-600 dark:bg-gray-800 dark:text-white" />
        </label>
        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300">
          计划收集场次
          <input bind:value={masterBatchTargetCount} type="number" min="1" max="20" class="mt-1.5 w-28 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-600 dark:bg-gray-800 dark:text-white" />
        </label>
        {#if masterBatchError}
          {@const errorHint = friendlyMasterError(masterBatchError)}
          <p class="text-sm leading-6 text-red-600 dark:text-red-400"><strong>{errorHint.title}</strong><br />{errorHint.detail}<br />{errorHint.nextAction}</p>
        {/if}
      </div>
      <div class="flex justify-end gap-3 border-t border-gray-100 bg-gray-50 px-6 py-4 dark:border-gray-700 dark:bg-gray-800">
        <button class="rounded-lg px-4 py-2 text-sm text-gray-700 hover:bg-gray-200 dark:text-gray-200 dark:hover:bg-gray-700" on:click={() => showMasterBatchDialog = false}>取消</button>
        <button class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50" disabled={creatingMasterBatch} on:click={createSelectedMasterBatch}>
          {creatingMasterBatch ? "正在保存..." : masterBatchPurpose === "enterprise" ? "创建企业母稿汇总" : "保存样本批次"}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showMasterBatchProgress}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4">
    <div class="mac-modal flex max-h-[86vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-gray-200 bg-white shadow-xl dark:border-gray-700 dark:bg-[#323234]">
      <div class="flex items-start justify-between border-b border-gray-100 px-6 py-5 dark:border-gray-700">
        <div>
          <h3 class="text-base font-semibold text-gray-900 dark:text-white">{selectedMasterSampleBatch?.batch.purpose === "enterprise" ? "公司统一话术生成进度" : "主播话术整理进度"}</h3>
          {#if selectedMasterSampleBatch}
            <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{selectedMasterSampleBatch.batch.title} · {selectedMasterSampleBatch.batch.hostLabel || "未填写主播"}</p>
          {/if}
        </div>
        <button class="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700" title="关闭" on:click={closeMasterBatchProgress}>
          <X size={18} />
        </button>
      </div>

      <div class="min-h-0 overflow-y-auto px-6 py-5">
        {#if loadingMasterBatchProgress}
          <div class="flex min-h-48 items-center justify-center gap-3 text-sm text-gray-500 dark:text-gray-400"><Loader2 class="animate-spin" size={20} />正在读取真实进度</div>
        {:else if masterBatchProgressError}
          {@const errorHint = friendlyMasterError(masterBatchProgressError)}
          <p class="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm leading-6 text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300"><strong>{errorHint.title}</strong><br />{errorHint.detail}<br />{errorHint.nextAction}</p>
        {:else if selectedMasterSampleBatch}
          {@const progress = masterBatchProgress(selectedMasterSampleBatch)}
          {@const synthesisStatus = masterBatchSynthesisStatus(selectedMasterSampleBatch)}
          <section class="mb-5 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-blue-100 bg-blue-50/60 px-4 py-3 dark:border-blue-900/50 dark:bg-blue-950/20">
            <p class="text-sm text-blue-900 dark:text-blue-100">
              {#if progress.synthesisGenerating}
                正在汇总 7 场样本的证据并提炼规律，生成完成后会自动显示草稿。
              {:else if selectedMasterSampleBatch.synthesis?.status === "failed"}
                上次母稿草稿生成已停止，可以重新生成；不会重新进行视频转写。
              {:else if selectedMasterSampleBatch.batch.status === "processing"}
                正在逐场整理。若应用刚重启或任务异常中断，可点击“继续未完成场次”恢复队列。
              {:else if selectedMasterSampleBatch.batch.status === "ready_for_synthesis"}
                全部逐场稿件已准备完成，可以生成最终母稿草稿。
              {:else if selectedMasterSampleBatch.batch.status === "draft_ready"}
                最终母稿草稿已生成，先人工核对规律和固定原话，再决定是否发布。
              {:else if selectedMasterSampleBatch.batch.status === "published"}
                {#if selectedMasterSampleBatch.batch.purpose === "enterprise"}
                  {correctedMasterVersionPublished
                    ? "校正 V1.1 已发布，已自动作为后续录播评分母稿；V1.0 保留用于追溯。"
                    : selectedMasterSampleBatch.correctableModelFormatIssueCount === 0
                      ? "该批次已发布为企业母稿，已按当前规则完成型号格式校正，无需生成 V1.1。"
                      : "该批次已发布为企业母稿。发现可确定的 ASR 型号格式错字时，可生成校正 V1.1；V1.0 会保留用于追溯。"}
                {:else}
                  该批次已发布为头部主播样本，只作为企业母稿的证据来源，不参与后续录播评分。
                {/if}
              {:else}
                逐场处理尚未开始，确认后才会调用 ASR 服务。
              {/if}
            </p>
            {#if selectedMasterSampleBatch.batch.status === "ready_for_synthesis" || selectedMasterSampleBatch.batch.status === "draft_ready" || selectedMasterSampleBatch.synthesis?.status === "failed"}
              <button class="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50" disabled={generatingMasterBatchDraft || progress.synthesisGenerating} on:click={generateSelectedMasterBatchDraft}>
                {generatingMasterBatchDraft || progress.synthesisGenerating ? "正在提炼..." : selectedMasterSampleBatch.batch.status === "draft_ready" ? "按新规则重新生成" : "生成最终母稿草稿"}
              </button>
            {:else if selectedMasterSampleBatch.batch.status !== "published"}
              <button class="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50" disabled={startingMasterBatch} on:click={startSelectedMasterBatchProcessing}>
                {startingMasterBatch ? "正在启动..." : (selectedMasterSampleBatch.batch.status === "failed" || selectedMasterSampleBatch.batch.status === "processing") ? "继续未完成场次" : "开始逐场处理"}
              </button>
            {/if}
            {#if selectedMasterSampleBatch.batch.status === "draft_ready"}
              <button class="rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:cursor-not-allowed disabled:opacity-50" disabled={publishingMasterBatchDraft} on:click={publishSelectedMasterBatchDraft}>
                {publishingMasterBatchDraft ? "正在发布..." : "审核通过并发布 V1.0"}
              </button>
            {:else if selectedMasterSampleBatch.batch.status === "published" && !correctedMasterVersionPublished && selectedMasterSampleBatch.correctableModelFormatIssueCount !== 0}
              <button class="rounded-md bg-amber-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-50" disabled={publishingCorrectedMasterBatch} on:click={publishCorrectedMasterBatchVersion}>
                {publishingCorrectedMasterBatch ? "正在生成 V1.1..." : "生成校正 V1.1"}
              </button>
            {/if}
          </section>
          {#if synthesisStatus}
            <section class="mb-5 flex items-start gap-2 rounded-lg border border-blue-200 bg-white px-4 py-3 text-sm text-blue-900 shadow-sm dark:border-blue-900/60 dark:bg-[#2b2b2d] dark:text-blue-100" aria-live="polite">
              <Loader2 class="mt-0.5 shrink-0 animate-spin text-blue-600" size={17} />
              <div><p class="font-medium">母稿草稿正在生成</p><p class="mt-1 leading-6 text-blue-800/80 dark:text-blue-200/80">{synthesisStatus}</p></div>
            </section>
          {/if}
          {#if selectedMasterSampleBatch.synthesis?.status === "failed" && selectedMasterSampleBatch.synthesis.error}
            {@const errorHint = friendlyMasterError(selectedMasterSampleBatch.synthesis.error)}
            <section class="mb-5 flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/20 dark:text-amber-100">
              <RotateCw class="mt-0.5 shrink-0 text-amber-600" size={17} />
              <div><p class="font-medium">{errorHint.title}</p><p class="mt-1 leading-6 text-amber-800/80 dark:text-amber-200/80">{errorHint.detail}<br />{errorHint.nextAction}</p></div>
            </section>
          {/if}
          <section class="border-b border-gray-100 pb-5 dark:border-gray-700">
            <div class="flex items-end justify-between gap-4">
              <div>
                <p class="text-sm font-medium text-gray-900 dark:text-white">整体完成度</p>
                <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
                  {progress.transcribed === 0 && selectedMasterSampleBatch.batch.status === "collecting"
                    ? "样本已入库，尚未启动逐场处理；当前没有后台转写任务在运行。"
                    : masterBatchStatusLabel(
                      selectedMasterSampleBatch.batch.status,
                      selectedMasterSampleBatch.batch.purpose,
                    )}
                </p>
              </div>
              <span class="text-2xl font-semibold text-blue-600 dark:text-blue-300">{progress.percent}%</span>
            </div>
            <div class="mt-3 h-2 overflow-hidden rounded-full bg-gray-100 dark:bg-gray-700">
              <div class="h-full rounded-full bg-blue-600 transition-all" style={`width: ${progress.percent}%`}></div>
            </div>
          </section>

          <section class="py-5">
            <ol class="space-y-3">
              <li class="flex items-start gap-3">
                <CheckCircle2 class="mt-0.5 shrink-0 text-emerald-600" size={18} />
                <div><p class="text-sm font-medium text-gray-900 dark:text-white">1. 样本入库</p><p class="text-sm text-gray-500 dark:text-gray-400">{progress.collected}/{selectedMasterSampleBatch.batch.targetSampleCount} 场已入库</p></div>
              </li>
              <li class="flex items-start gap-3">
                {#if progress.transcribing > 0}<Loader2 class="mt-0.5 shrink-0 text-blue-600 animate-spin" size={18} />{:else if progress.transcribed === progress.total && progress.total > 0}<CheckCircle2 class="mt-0.5 shrink-0 text-emerald-600" size={18} />{:else}<Circle class="mt-0.5 shrink-0 text-gray-400" size={18} />{/if}
                <div><p class="text-sm font-medium text-gray-900 dark:text-white">2. 逐场转写与事实校正</p><p class="text-sm text-gray-500 dark:text-gray-400">{progress.transcribed}/{progress.total} 场完成转写；尚未开始的场次不会产生模型费用。</p></div>
              </li>
              <li class="flex items-start gap-3">
                {#if progress.structured === progress.total && progress.total > 0}<CheckCircle2 class="mt-0.5 shrink-0 text-emerald-600" size={18} />{:else if progress.structured > 0}<FileText class="mt-0.5 shrink-0 text-blue-600" size={18} />{:else}<Circle class="mt-0.5 shrink-0 text-gray-400" size={18} />{/if}
                <div><p class="text-sm font-medium text-gray-900 dark:text-white">3. 单场话术整理归档</p><p class="text-sm text-gray-500 dark:text-gray-400">{progress.structured}/{progress.total} 场已生成可审计逐字稿，可用于跨场对比。</p></div>
              </li>
              <li class="flex items-start gap-3">
                {#if progress.synthesisGenerating}<Loader2 class="mt-0.5 shrink-0 animate-spin text-blue-600" size={18} />{:else if progress.synthesisStarted}<CheckCircle2 class="mt-0.5 shrink-0 text-emerald-600" size={18} />{:else}<Circle class="mt-0.5 shrink-0 text-gray-400" size={18} />{/if}
                <div><p class="text-sm font-medium text-gray-900 dark:text-white">4. 生成最终母稿</p><p class="text-sm text-gray-500 dark:text-gray-400">{progress.synthesisGenerating ? synthesisStatus : progress.synthesisStarted ? "已生成待审核草稿，固定话术均保留主播原话。" : "需在所有样本完成后启动。"}</p></div>
              </li>
              <li class="flex items-start gap-3">
                {#if progress.published}<CheckCircle2 class="mt-0.5 shrink-0 text-emerald-600" size={18} />{:else}<Circle class="mt-0.5 shrink-0 text-gray-400" size={18} />{/if}
                <div><p class="text-sm font-medium text-gray-900 dark:text-white">5. 审核并发布</p><p class="text-sm text-gray-500 dark:text-gray-400">{selectedMasterSampleBatch.batch.purpose === "enterprise" ? "人工审核通过后才会成为后续录播的唯一评分基准。" : "样本发布后只作为证据来源；汇总为企业母稿后才会用于评分。"}</p></div>
              </li>
            </ol>
          </section>

          {#if batchMasterDraft(selectedMasterSampleBatch)}
            {@const draft = batchMasterDraft(selectedMasterSampleBatch)}
            {#if draft}
              {@const transactionView = buildTransactionMasterView({ draft, candidates: transactionMasterCandidates })}
              <section class="mb-5 border-t border-gray-100 pt-5 dark:border-gray-700">
                <div class="mb-4 flex items-start gap-3">
                  <Sparkles class="mt-0.5 shrink-0 text-blue-600" size={19} />
                  <div><h4 class="text-sm font-semibold text-gray-900 dark:text-white">成交片段型企业母稿</h4><p class="mt-1 text-sm text-gray-500 dark:text-gray-400">整场直播只作为证据来源；正式母稿由企业规则、完整成交范例和局部训练素材组成。</p></div>
                </div>
                <div class="space-y-3">
                  <article class="rounded-lg border border-blue-100 bg-blue-50/50 p-4 dark:border-blue-900/50 dark:bg-blue-950/20">
                    <div class="flex items-center justify-between gap-3"><h5 class="text-sm font-semibold text-blue-950 dark:text-blue-100">1. 企业成交规则</h5><span class="text-xs text-blue-700 dark:text-blue-300">{transactionView.rules.length} 条</span></div>
                    <p class="mt-1 text-xs text-blue-800/70 dark:text-blue-200/70">用于判断成交链路是否完整，不直接把整场逐字稿作为评分标准。</p>
                    <div class="mt-3 space-y-2">
                      {#each transactionView.rules as rule}
                        <div><p class="text-sm font-medium text-gray-900 dark:text-white">{rule.title}</p><p class="mt-1 text-sm leading-6 text-gray-600 dark:text-gray-300">{rule.detail}</p>{#if rule.evidenceIds.length}<p class="mt-1 text-xs text-blue-700 dark:text-blue-300">证据：{rule.evidenceIds.join("、")}</p>{/if}</div>
                      {/each}
                    </div>
                  </article>
                  <article class="rounded-lg border border-emerald-200 bg-emerald-50/60 p-4 dark:border-emerald-900/50 dark:bg-emerald-950/20">
                    <div class="flex items-center justify-between gap-3"><h5 class="text-sm font-semibold text-emerald-950 dark:text-emerald-100">2. 完整成交范例</h5><span class="text-xs text-emerald-700 dark:text-emerald-300">{transactionView.completeExamples.length} 段</span></div>
                    <p class="mt-1 text-xs text-emerald-800/70 dark:text-emerald-200/70">七段成交链路完整、按企业标准达到 85 分并经人工审核后才进入这里。</p>
                    {#if transactionView.completeExamples.length}
                      <div class="mt-3 space-y-3">{#each transactionView.completeExamples as example}<div class="rounded-md bg-white/80 p-3 dark:bg-black/10"><div class="flex justify-between gap-3 text-xs text-emerald-700 dark:text-emerald-300"><span>{example.sourceKey}</span><strong>{example.score} 分</strong></div><blockquote class="mt-2 border-l-2 border-emerald-500 pl-3 text-sm leading-6 text-gray-800 whitespace-pre-wrap dark:text-gray-100">{example.text}</blockquote></div>{/each}</div>
                    {:else}
                      <p class="mt-3 rounded-md bg-white/80 px-3 py-4 text-sm text-gray-500 dark:bg-black/10 dark:text-gray-400">暂时没有审核通过的完整成交范例。后续录播分析达到 85 分后会进入候选审核。</p>
                    {/if}
                  </article>
                  <article class="rounded-lg border border-amber-200 bg-amber-50/70 p-4 dark:border-amber-900/50 dark:bg-amber-950/20">
                    <div class="flex items-center justify-between gap-3"><h5 class="text-sm font-semibold text-amber-950 dark:text-amber-100">3. 优秀局部训练素材</h5><span class="text-xs text-amber-700 dark:text-amber-300">{transactionView.trainingMaterials.length} 段</span></div>
                    <p class="mt-1 text-xs text-amber-800/70 dark:text-amber-200/70">链路不完整但某句话值得学习，只用于新人训练，不参与企业母稿评分。</p>
                    {#if transactionView.trainingMaterials.length}
                      <div class="mt-3 space-y-3">{#each transactionView.trainingMaterials as material}<blockquote class="rounded-md border-l-2 border-amber-500 bg-white/80 p-3 text-sm leading-6 text-gray-800 whitespace-pre-wrap dark:bg-black/10 dark:text-gray-100">{material.text}</blockquote>{/each}</div>
                    {:else}
                      <p class="mt-3 rounded-md bg-white/80 px-3 py-4 text-sm text-gray-500 dark:bg-black/10 dark:text-gray-400">暂时没有人工保留的局部训练素材。</p>
                    {/if}
                  </article>
                  <details class="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-[#2b2b2d]">
                    <summary class="cursor-pointer text-sm font-semibold text-gray-900 dark:text-white">查看原始章节证据（{draft.sections.length} 个）</summary>
                    <div class="mt-3 space-y-3">{#each draft.sections as section, index}<div class="rounded-md bg-gray-50 p-3 dark:bg-black/10"><h6 class="text-sm font-medium text-gray-900 dark:text-white">{index + 1}. {section.title}</h6><p class="mt-1 text-xs text-gray-500 dark:text-gray-400">{section.purpose}</p><div class="mt-2 border-l-2 border-gray-300 pl-3 text-sm leading-6 text-gray-700 whitespace-pre-wrap dark:text-gray-200">{section.fixedSpeech}</div></div>{/each}</div>
                  </details>
                </div>
              </section>
            {/if}
          {/if}

          <section class="border-t border-gray-100 pt-5 dark:border-gray-700">
            <div class="mb-3 flex items-center justify-between"><h4 class="text-sm font-semibold text-gray-900 dark:text-white">样本明细</h4><span class="text-xs text-gray-500 dark:text-gray-400">共 {progress.total} 场</span></div>
            <div class="divide-y divide-gray-100 overflow-hidden rounded-lg border border-gray-200 dark:divide-gray-700 dark:border-gray-700">
              {#each selectedMasterSampleBatch.items as item}
                <div class="flex items-center justify-between gap-4 px-4 py-3">
                  <div class="min-w-0"><p class="truncate text-sm text-gray-900 dark:text-white">{item.displayOrder}. {item.videoTitle}</p>{#if item.error}<p class="mt-1 truncate text-xs text-red-600 dark:text-red-300">{item.error}</p>{/if}</div>
                  <span class="shrink-0 text-xs font-medium {item.processingStatus === 'ready' ? 'text-emerald-700 dark:text-emerald-300' : item.processingStatus === 'failed' ? 'text-red-600 dark:text-red-300' : 'text-gray-500 dark:text-gray-400'}">{masterBatchItemStatusLabel(item.processingStatus)}</span>
                </div>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* macOS style modal */
  .mac-modal {
    box-shadow:
      0 20px 25px -5px rgba(0, 0, 0, 0.1),
      0 10px 10px -5px rgba(0, 0, 0, 0.04);
  }

  :global(.dark) .mac-modal {
    box-shadow:
      0 20px 25px -5px rgba(0, 0, 0, 0.3),
      0 10px 10px -5px rgba(0, 0, 0, 0.1);
  }

  /* fixed icon size in tables */
  :global(.table-icon) {
    width: 1rem; /* 16px, same as Tailwind w-4 */
    height: 1rem; /* 16px, same as Tailwind h-4 */
    flex: 0 0 auto;
  }

  /* macOS style textarea */
  .mac-modal textarea {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
      sans-serif;
  }

  .mac-modal textarea:focus {
    box-shadow: 0 0 0 3px rgba(0, 122, 255, 0.3);
  }
</style>
