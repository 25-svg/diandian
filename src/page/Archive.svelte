<script lang="ts">
  import { invoke, invokeSensitive, get_static_url } from "../lib/invoker";
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
    openLiveDashboard,
    type LiveDashboardBindingSummary,
  } from "../lib/liveDashboard";
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
  import { onDestroy, onMount } from "svelte";
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

  $: importDefaultAnalysisPurpose = archiveKindTab === "competitor"
    ? "competitor_benchmark"
    : "enterprise_review";
  $: importTitlePlaceholder = archiveKindTab === "company"
    ? "例如：7/28 金典拍拍相机专场"
    : "例如：XX 相机二手店专场";

  function archiveKey(archive: RecordItem): string {
    return `${archive.platform}:${archive.room_id}:${archive.live_id}`;
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

  function isArchiveRecording(archive: RecordItem): boolean {
    return allRooms.some((room) => room.recording && String(room.live_id) === String(archive.live_id));
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
          const index = allArchives.findIndex(
            (archive) => String(archive.live_id) === String(updated.live_id),
          );
          if (index >= 0) {
            if (
              allArchives[index].length !== updated.length ||
              allArchives[index].size !== updated.size
            ) {
              allArchives[index] = {
                ...allArchives[index],
                length: updated.length,
                size: updated.size,
              };
              changed = true;
            }
            continue;
          }

          if (String(updated.live_id) !== String(room.live_id)) {
            continue;
          }

          updated.cover = await get_static_url(
            "cache",
            `${updated.platform}/${updated.room_id}/${updated.live_id}/cover.jpg`,
          );
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
          let roomArchives = await invoke<RecordItem[]>("get_archives", {
            roomId: room.room_info.room_id,
            offset: 0,
            limit: 100, // 每个直播间获取更多数据
          });

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
            archive.cover = await get_static_url(
              "cache",
              `${archive.platform}/${archive.room_id}/${archive.live_id}/cover.jpg`
            );
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

  function openArchiveLiveDashboard(archive: RecordItem) {
    const binding = liveDashboardBindings.get(archive.live_id);
    if (!binding) return;
    openLiveDashboard(binding.sessionId);
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
  subtitle="管理所有直播间的录播记录，可以查看、播放和管理历史直播内容。"
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
      <button type="button" role="tab" aria-selected={archiveKindTab === "company"} class:mac-segment-active={archiveKindTab === "company"} on:click={() => { archiveKindTab = "company"; currentPage = 1; applyFilters(); }}>公司录播</button>
      <button type="button" role="tab" aria-selected={archiveKindTab === "competitor"} class:mac-segment-active={archiveKindTab === "competitor"} on:click={() => { archiveKindTab = "competitor"; currentPage = 1; applyFilters(); }}>竞品录播</button>
    </div>

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
                <th>账号 / 标题</th>
                <th class="w-28">主播</th>
                <th class="w-24">时长</th>
                <th class="w-20">大小</th>
                <th class="w-24">码率</th>
                <th class="w-56">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredArchives as archive (getArchiveKey(archive))}
                {@const identity = getArchiveIdentity(archive)}
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
                    <div class="flex min-w-0 items-center gap-3">
                      {#if archive.cover}
                        <img
                          src={archive.cover}
                          alt="封面"
                          class="w-12 h-8 rounded object-cover flex-shrink-0"
                        />
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
                        {#if canManuallyEditAnchor(archive)}
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
                            {archive.anchor_name || anchorStatusLabel(archive)}
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
                      <button
                        class="inline-flex items-center gap-1 px-1.5 py-1 rounded-lg hover:bg-blue-500/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed text-xs text-blue-600"
                        title={liveDashboardBindings.has(archive.live_id) ? "打开直播数据大屏" : "请先在录播分析页绑定罗盘数据"}
                        disabled={!liveDashboardBindings.has(archive.live_id)}
                        on:click={() => openArchiveLiveDashboard(archive)}
                      >
                        <BarChart3 class="w-4 h-4" />
                        <span>大屏</span>
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
</style>
