<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { Activity, AlertTriangle, BarChart3, CalendarDays, Database, Download, FolderOpen, Loader2, Play, RefreshCw, TrendingDown, Upload } from "lucide-svelte";
  import { invoke } from "../lib/invoker";
  import { dashboardMetricCards } from "../lib/liveDashboard";
  import PageShell from "../lib/components/PageShell.svelte";
  import {
    applyCompassProgress,
    buildCompassQueue,
    canStartCompassDownload,
    COMPASS_TARGET_SHOPS,
    compassStatusLabel,
    markCompassSessionImported,
    normalizeCompassDate,
    type CompassQueueItem,
    type CompassSession,
  } from "../lib/compassAutoDownload";
  import {
    analyzeCompassMetricWindow,
    compassEvidenceExplanation,
    compassMetricStyle,
    normalizeCompassMetricValues,
    type CompassAudienceItem,
    type CompassDeclineEvent,
    type CompassMetricAnalysis,
    type CompassMetricPoint,
    type CompassMetricWindowAnalysis,
    type CompassQianchuanMetric,
  } from "../lib/compassAnalysis";

  type Session = {
    id: number; shopName: string; startedAt: string; endedAt: string; paymentAmountFen: number;
    perThousandPaymentAmountFen?: number | null; viewerCount?: number | null;
    averageOnline?: number | null; averageWatchSeconds?: number | null;
    viewerConversionRate?: number | null; dealBuyerCount?: number | null;
    dealItemCount?: number | null; productClickConversionRate?: number | null;
    exposureViewerRate?: number | null; qianchuanSpendFen?: number | null;
    sourceFile: string; importedAt: string;
  };
  type Detail = { session: Session; channels: Array<{ name: string; viewerCount?: number; paymentAmountFen?: number; orderCount?: number }>; shortVideos: Array<{ title: string; viewerCount?: number; paymentAmountFen?: number; orderCount?: number }>; products: Array<{ productId: string; name: string; paymentAmountFen?: number; soldCount?: number; buyerCount?: number }> };
  type CompassCaptureSummary = {
    captureId: string; targetDate: string; targetShopName: string; status: string; message: string;
    startedAt: string; finishedAt?: string | null; responseCount: number;
    businessResponseCount: number; analysisResponseCount: number; navigationResponseCount: number;
    configurationResponseCount: number; unknownResponseCount: number; httpResponseCount: number;
    websocketFrameCount: number; parseFailureCount: number;
    metricLabels: string[]; hasProductData: boolean; hasExplanationData: boolean;
    endpointCounts: Record<string, number>; sessionKeys: string[]; storagePath: string;
    coverage: Record<string, { key: string; label: string; kind: string; status: string; responseCount: number; attemptCount: number; failureCount: number; lastEndpoint?: string | null; lastTransport?: string | null; lastError?: string | null }>;
    catalogEndpointCount: number; catalogFieldCount: number; catalogPath: string;
  };
  type CompassProductAnalysis = { productId: string; productName: string; explainCount: number; clickCount: number; paymentAmount: number; soldCount: number };
  type CompassAnalysisSource = {
    kind: "archive" | "video" | string; liveId: string; platform: string;
    roomId: string; videoId?: number | null;
  };
  type CompassCaptureAnalysis = {
    captureId: string; targetDate: string; targetShopName: string; captureStatus: string; rawPath: string;
    quality: { totalResponseCount: number; businessResponseCount: number; analysisResponseCount: number; navigationResponseCount: number; configurationResponseCount: number; httpResponseCount: number; websocketFrameCount: number; parseFailureCount: number; metricCount: number; productCount: number; plannedSectionCount: number; capturedSectionCount: number; unavailableSectionCount: number; untriggeredSectionCount: number; parseFailedSectionCount: number; permissionDeniedSectionCount: number; catalogEndpointCount: number; catalogFieldCount: number; complete: boolean; issues: string[] };
    metrics: CompassMetricAnalysis[]; products: CompassProductAnalysis[];
    audience: CompassAudienceItem[]; qianchuan: CompassQianchuanMetric[];
    coverage: Array<{ key: string; label: string; kind: string; status: string; responseCount: number; attemptCount: number; failureCount: number; lastEndpoint?: string | null; lastTransport?: string | null; lastError?: string | null }>;
    findings: Array<{ level: string; title: string; summary: string; evidence: string }>;
    declineEvents: CompassDeclineEvent[]; source?: CompassAnalysisSource | null;
    aiSummary?: string | null; generatedAt: string;
  };

  export let initialSessionId: number | null = null;

  let sessions: Session[] = [];
  let detail: Detail | null = null;
  let loading = true;
  let message = "";
  let downloadDir = "";
  let unlistens: Array<() => void> = [];
  let appliedSessionId: number | null = null;
  let compassDate = yesterdayLocalDate();
  let compassQueue: CompassQueueItem[] = [];
  let compassQuerying = false;
  let compassRunning = false;
  let compassTargetShopName = "金典拍拍相机专卖店";
  let compassCaptureRunning = false;
  let activeCaptureId = "";
  let captureProgressMessage = "";
  let captureResponseCount = 0;
  let captures: CompassCaptureSummary[] = [];
  let captureAnalysis: CompassCaptureAnalysis | null = null;
  let captureAnalysisLoading = false;
  let captureAnalysisError = "";
  let selectedMetricLabels: string[] = [];
  let rangeStart = 0;
  let rangeEnd = 0;
  let fullRangeStart = 0;
  let fullRangeEnd = 0;
  let selectedShopName = "";
  let selectedSessionDate = "";

  $: shopNames = Array.from(new Set(sessions.map((session) => session.shopName))).sort((a, b) => a.localeCompare(b, "zh-CN"));
  $: shopSessions = sessions.filter((session) => session.shopName === selectedShopName);
  $: sessionDates = Array.from(new Set(shopSessions.map((session) => sessionDateKey(session.startedAt)))).sort((a, b) => b.localeCompare(a));
  $: visibleSessions = shopSessions.filter((session) => sessionDateKey(session.startedAt) === selectedSessionDate);

  type CompassBrowserProgress = {
    targetDate: string;
    targetShopName?: string;
    status: string;
    message?: string;
    sessionKey?: string;
    sessions?: CompassSession[];
    captureId?: string;
  };
  type CompassCaptureProgress = Partial<CompassCaptureSummary> & {
    captureId: string; responseCount?: number; endpoint?: string;
  };

  function yesterdayLocalDate(): string {
    const date = new Date();
    date.setDate(date.getDate() - 1);
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  }

  const money = (fen?: number | null) => fen == null ? "— / 官方导出未提供" : `¥${(fen / 100).toLocaleString("zh-CN", { maximumFractionDigits: 2 })}`;

  function sessionDateKey(value: string): string {
    return value.slice(0, 10);
  }

  function displaySessionDate(value: string): string {
    const [year, month, day] = value.split("-");
    return `${year}年${month}月${day}日`;
  }

  function displaySessionTime(value: string): string {
    return new Date(value).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
  }

  async function selectShop(event: Event) {
    selectedShopName = (event.currentTarget as HTMLSelectElement).value;
    const candidate = sessions.find((session) => session.shopName === selectedShopName);
    if (candidate) await refresh(candidate.id);
  }

  async function selectSessionDate(event: Event) {
    selectedSessionDate = (event.currentTarget as HTMLSelectElement).value;
    const candidate = sessions.find((session) =>
      session.shopName === selectedShopName && sessionDateKey(session.startedAt) === selectedSessionDate
    );
    if (candidate) await refresh(candidate.id);
  }

  async function refresh(sessionId?: number) {
    loading = true;
    try {
      sessions = await invoke<Session[]>("list_live_dashboard_sessions");
      const selected = sessionId ?? detail?.session.id ?? sessions[0]?.id;
      detail = selected ? await invoke<Detail>("get_live_dashboard_detail", { sessionId: selected }) : null;
      if (detail) {
        selectedShopName = detail.session.shopName;
        selectedSessionDate = sessionDateKey(detail.session.startedAt);
      }
      const settings = await invoke<{ downloadDir: string }>("get_live_dashboard_settings");
      downloadDir = settings.downloadDir;
      captures = await invoke<CompassCaptureSummary[]>("list_compass_captures");
    } catch (error) { message = `加载失败：${String(error)}`; }
    finally { loading = false; }
  }

  async function importWorkbook() {
    const selected = await open({ multiple: false, filters: [{ name: "Excel", extensions: ["xlsx"] }] });
    if (!selected || Array.isArray(selected)) return;
    try {
      const session = await invoke<Session>("import_live_dashboard_xlsx", { path: selected });
      message = "导入成功，已更新直播数据大屏。";
      await refresh(session.id);
    } catch (error) { message = `导入失败：${String(error)}`; }
  }

  async function chooseDownloadDir() {
    const selected = await open({ directory: true, multiple: false, defaultPath: downloadDir || undefined });
    if (!selected || Array.isArray(selected)) return;
    try { await invoke("set_live_dashboard_download_dir", { path: selected }); downloadDir = selected; message = "下载目录已更新，软件将自动导入官方 XLSX。"; }
    catch (error) { message = `设置失败：${String(error)}`; }
  }

  async function queryCompassSessions() {
    try {
      compassDate = normalizeCompassDate(compassDate);
      compassQuerying = true;
      compassRunning = false;
      compassQueue = [];
      message = `正在打开${compassTargetShopName}的抖音罗盘；首次使用请扫码登录。`;
      await invoke("query_compass_live_sessions", { targetDate: compassDate, targetShopName: compassTargetShopName });
      compassQuerying = false;
      message = "罗盘窗口已打开；可以直接点击一键下载。";
    } catch (error) {
      compassQuerying = false;
      message = `查询罗盘场次失败：${String(error)}`;
    }
  }

  async function startCompassDownloads() {
    try {
      compassDate = normalizeCompassDate(compassDate);
      compassRunning = true;
      compassQueue = compassQueue.map((item) => item.status === "skipped"
        ? item
        : { ...item, status: "waiting", message: "等待处理" });
      message = `正在串行下载并导入 ${compassTargetShopName} ${compassDate} 的全部直播场次。`;
      await invoke("start_compass_live_downloads", { targetDate: compassDate, targetShopName: compassTargetShopName });
    } catch (error) {
      compassRunning = false;
      message = `启动批量下载失败：${String(error)}`;
    }
  }

  async function startCompassCapture() {
    try {
      compassDate = normalizeCompassDate(compassDate);
      compassCaptureRunning = true;
      captureResponseCount = 0;
      captureProgressMessage = "正在打开罗盘并准备抓取接口数据";
      message = `正在完整采集 ${compassTargetShopName} ${compassDate} 的指标曲线、商品和讲解数据。`;
      activeCaptureId = await invoke<string>("start_compass_full_capture", {
        targetDate: compassDate,
        targetShopName: compassTargetShopName,
      });
    } catch (error) {
      compassCaptureRunning = false;
      captureProgressMessage = `启动失败：${String(error)}`;
      message = captureProgressMessage;
    }
  }

  function handleCaptureProgress(progress: CompassCaptureProgress) {
    if (activeCaptureId && progress.captureId !== activeCaptureId) return;
    activeCaptureId ||= progress.captureId;
    captureResponseCount = progress.responseCount ?? captureResponseCount;
    captureProgressMessage = progress.message || compassStatusLabel(progress.status || "running");
    if (["completed", "partial", "failed", "cancelled"].includes(progress.status || "")) {
      compassCaptureRunning = false;
      void refresh();
    }
  }

  async function analyzeCapture(captureId: string) {
    captureAnalysisLoading = true;
    captureAnalysisError = "";
    try {
      captureAnalysis = await invoke<CompassCaptureAnalysis>("analyze_compass_capture", {
        captureId,
        sessionId: detail?.session.id ?? null,
      });
      initializeCurveControls();
    } catch (error) {
      captureAnalysis = null;
      captureAnalysisError = `分析失败：${String(error)}`;
    } finally {
      captureAnalysisLoading = false;
    }
  }

  function initializeCurveControls(): void {
    const metrics = captureAnalysis?.metrics || [];
    const allPoints = metrics.flatMap((metric) => metric.points);
    fullRangeStart = allPoints.length ? Math.min(...allPoints.map((point) => point.sortValue)) : 0;
    fullRangeEnd = allPoints.length ? Math.max(...allPoints.map((point) => point.sortValue)) : 0;
    rangeStart = fullRangeStart;
    rangeEnd = fullRangeEnd;
    const initial = captureAnalysis?.declineEvents[0]?.metricLabel || metrics[0]?.label || "";
    selectedMetricLabels = initial ? [initial] : [];
  }

  function toggleMetric(label: string): void {
    if (selectedMetricLabels.includes(label)) {
      if (selectedMetricLabels.length === 1) return;
      selectedMetricLabels = selectedMetricLabels.filter((item) => item !== label);
      return;
    }
    selectedMetricLabels = [...selectedMetricLabels.slice(-3), label];
  }

  function resetCurveRange(): void {
    rangeStart = fullRangeStart;
    rangeEnd = fullRangeEnd;
  }

  function rangeLabel(sortValue: number): string {
    const points = captureAnalysis?.metrics.flatMap((metric) => metric.points) || [];
    return points.reduce<CompassMetricPoint | null>((nearest, point) => (
      !nearest || Math.abs(point.sortValue - sortValue) < Math.abs(nearest.sortValue - sortValue) ? point : nearest
    ), null)?.timeLabel || "—";
  }

  function trendLabel(analysis: CompassMetricWindowAnalysis): string {
    if (analysis.trendDirection === "not_applicable") return "无法计算";
    const direction = analysis.trendDirection === "rising" ? "上升" : analysis.trendDirection === "falling" ? "下降" : "稳定";
    return `${direction} ${analysis.trendPercent == null ? "" : `${analysis.trendPercent > 0 ? "+" : ""}${analysis.trendPercent.toFixed(1)}%`}`.trim();
  }

  function metricValue(value: number | null, unit: string): string {
    if (value == null || !Number.isFinite(value)) return "—";
    return `${value.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}${unit || ""}`;
  }

  function pointValid(point: CompassMetricPoint): boolean {
    return point.valid !== false && Number.isFinite(point.value);
  }

  function chartX(sortValue: number): number {
    return 32 + ((sortValue - rangeStart) / Math.max(rangeEnd - rangeStart, 1)) * 796;
  }

  function metricDisplayValue(metric: CompassMetricAnalysis, point: CompassMetricPoint): number | null {
    if (!pointValid(point)) return null;
    if (!normalizedComparison) return point.value;
    const points = metric.points.filter((item) => item.sortValue >= rangeStart && item.sortValue <= rangeEnd);
    const normalized = normalizeCompassMetricValues(points);
    const index = points.indexOf(point);
    return normalized[index] ?? null;
  }

  function chartY(metric: CompassMetricAnalysis, point: CompassMetricPoint): number {
    const value = metricDisplayValue(metric, point) ?? chartMinimum;
    return 22 + (1 - (value - chartMinimum) / Math.max(chartMaximum - chartMinimum, 1)) * 166;
  }

  function chartPath(metric: CompassMetricAnalysis): string {
    const points = metric.points.filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd);
    let penDown = false;
    return points.map((point) => {
      if (!pointValid(point)) {
        penDown = false;
        return "";
      }
      const command = penDown ? "L" : "M";
      penDown = true;
      return `${command}${chartX(point.sortValue).toFixed(1)},${chartY(metric, point).toFixed(1)}`;
    }).filter(Boolean).join(" ");
  }

  function evidenceFor(metricLabel: string, start: number, end: number): CompassDeclineEvent | null {
    return (captureAnalysis?.declineEvents || []).find((event) =>
      event.metricLabel === metricLabel
      && event.endSortValue >= start
      && event.startSortValue <= end
    ) || null;
  }

  function formatOffset(seconds?: number | null): string {
    if (seconds == null || !Number.isFinite(seconds)) return "未绑定";
    const value = Math.max(0, Math.floor(seconds));
    return `${String(Math.floor(value / 3600)).padStart(2, "0")}:${String(Math.floor(value % 3600 / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
  }

  async function openDeclineEvidence(event: CompassDeclineEvent): Promise<void> {
    const source = captureAnalysis?.source;
    if (!source) {
      message = "当前场次尚未绑定录播，先在录播分析页完成场次匹配。";
      return;
    }
    try {
      if (source.kind === "video" && source.videoId != null) {
        const video = await invoke("get_video", { id: source.videoId });
        window.dispatchEvent(new CustomEvent("bsr:open-video-analysis", {
          detail: { video, analysisMode: "company_deal", seekSeconds: event.seekSeconds ?? 0 },
        }));
        return;
      }
      const archive = await invoke("get_archive", { roomId: source.roomId, liveId: source.liveId });
      window.dispatchEvent(new CustomEvent("bsr:open-company-deal-review", {
        detail: { archive, seekSeconds: event.seekSeconds ?? 0 },
      }));
    } catch (error) {
      message = `打开对应录播失败：${String(error)}`;
    }
  }

  type CurveEventRow = {
    id: string;
    metricLabel: string;
    kind: "异常区间" | "明显变化点";
    time: string;
    sortValue: number;
    value: number;
    detail: string;
    evidence: CompassDeclineEvent | null;
  };
  let selectedMetrics: CompassMetricAnalysis[] = [];
  let windowAnalyses: CompassMetricWindowAnalysis[] = [];
  let normalizedComparison = false;
  let chartMinimum = 0;
  let chartMaximum = 1;
  let visibleDeclineEvents: CompassDeclineEvent[] = [];
  let curveEvents: CurveEventRow[] = [];

  $: selectedMetrics = (captureAnalysis?.metrics || []).filter((metric) => selectedMetricLabels.includes(metric.label));
  $: windowAnalyses = selectedMetrics.map((metric) => analyzeCompassMetricWindow(metric, { start: rangeStart, end: rangeEnd }));
  $: normalizedComparison = new Set(windowAnalyses.map((analysis) => analysis.dimension)).size > 1;
  $: {
    const values = normalizedComparison
      ? [0, 100]
      : selectedMetrics.flatMap((metric) => metric.points
          .filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd && pointValid(point))
          .map((point) => point.value));
    chartMinimum = values.length ? Math.min(...values) : 0;
    chartMaximum = values.length ? Math.max(...values) : 1;
  }
  $: visibleDeclineEvents = (captureAnalysis?.declineEvents || []).filter((event) =>
    selectedMetricLabels.includes(event.metricLabel)
    && event.endSortValue >= rangeStart
    && event.startSortValue <= rangeEnd
  );
  $: curveEvents = windowAnalyses.flatMap((analysis) => [
    ...analysis.anomalies.map((event): CurveEventRow => ({
      id: event.id,
      metricLabel: analysis.label,
      kind: "异常区间",
      time: `${event.startTime}–${event.endTime}`,
      sortValue: event.startSortValue,
      value: event.value,
      detail: `${event.direction === "high" ? "高于" : "低于"}${analysis.baselineSource === "explicit" ? "目标基线" : "统计基线"}${event.deviationPercent == null ? "" : ` ${event.deviationPercent > 0 ? "+" : ""}${event.deviationPercent.toFixed(1)}%`}`,
      evidence: evidenceFor(analysis.label, event.startSortValue, event.endSortValue),
    })),
    ...analysis.changes.map((event, index): CurveEventRow => ({
      id: `${analysis.metricId}-change-${index + 1}`,
      metricLabel: analysis.label,
      kind: "明显变化点",
      time: event.timeLabel,
      sortValue: event.sortValue,
      value: event.value,
      detail: `${event.previousValue.toLocaleString("zh-CN")} → ${event.value.toLocaleString("zh-CN")}（${event.changePercent > 0 ? "+" : ""}${event.changePercent.toFixed(1)}%）`,
      evidence: evidenceFor(analysis.label, event.sortValue, event.sortValue),
    })),
  ]).sort((left, right) => left.sortValue - right.sortValue);

  function handleCompassProgress(progress: CompassBrowserProgress) {
    if (progress.targetDate !== compassDate) return;
    if (progress.targetShopName && progress.targetShopName !== compassTargetShopName) return;
    const terminalStatuses = ["failed", "batch-finished", "shop-mismatch", "shop-unavailable"];
    compassQuerying = !["sessions-found", ...terminalStatuses].includes(progress.status);
    if (terminalStatuses.includes(progress.status)) compassRunning = false;
    if (progress.captureId && (!activeCaptureId || progress.captureId === activeCaptureId)) {
      activeCaptureId ||= progress.captureId;
      captureProgressMessage = progress.message || compassStatusLabel(progress.status);
      if (["failed", "batch-finished", "shop-mismatch", "shop-unavailable"].includes(progress.status)) {
        compassCaptureRunning = false;
      }
    }
    if (progress.sessions?.length) {
      compassQueue = buildCompassQueue(progress.sessions);
      for (const session of sessions) {
        compassQueue = markCompassSessionImported(compassQueue, session.startedAt);
      }
    }
    if (progress.sessionKey) {
      compassQueue = applyCompassProgress(compassQueue, {
        sessionKey: progress.sessionKey,
        status: progress.status === "failed" ? "failed" : progress.status as any,
        message: progress.message,
      });
    }
    if (progress.status === "batch-finished") compassRunning = false;
    message = progress.message || compassStatusLabel(progress.status);
  }

  onMount(async () => {
    await refresh(initialSessionId ?? undefined);
    if (initialSessionId != null) {
      appliedSessionId = initialSessionId;
    }
    unlistens = await Promise.all([
      listen<Session>("live-dashboard-imported", (event) => {
        message = "检测到新下载，已自动导入。";
        if (event.payload?.startedAt) {
          compassQueue = markCompassSessionImported(compassQueue, event.payload.startedAt);
        }
        void refresh();
      }),
      listen<{ path: string; message: string }>("live-dashboard-import-failed", (event) => { message = `自动导入失败：${event.payload.message}。请确认文件为官方整场数据下载 XLSX。`; }),
      listen<{ message: string }>("live-dashboard-watch-error", (event) => { message = event.payload.message; }),
      listen<CompassBrowserProgress>("compass-auto-download-progress", (event) => handleCompassProgress(event.payload)),
      listen<CompassCaptureProgress>("compass-capture-progress", (event) => handleCaptureProgress(event.payload)),
    ]);
  });
  onDestroy(() => unlistens.forEach((unlisten) => unlisten()));

  $: if (initialSessionId != null && initialSessionId !== appliedSessionId) {
    appliedSessionId = initialSessionId;
    void refresh(initialSessionId);
  }
</script>

<PageShell title="直播数据大屏" subtitle="官方 XLSX 用于汇总数据；完整采集用于分钟曲线、商品分页和讲解明细。未提供字段不做推算。">
  <div slot="actions">
    <button type="button" class="mac-btn" on:click={chooseDownloadDir} title="设置下载目录"><FolderOpen size={16} />下载目录</button>
    <button type="button" class="mac-btn" on:click={importWorkbook}><Upload size={16} />导入 XLSX</button>
    <button type="button" class="mac-btn mac-btn-primary" on:click={() => refresh()} disabled={loading}><RefreshCw size={16} />刷新</button>
  </div>

  <section class="compass-panel" aria-label="抖音罗盘自动下载">
    <div class="compass-copy">
      <div class="compass-icon"><CalendarDays size={20} /></div>
      <div><h2>自动获取抖音罗盘数据</h2><p>选择店铺和日期。下载 XLSX 用于汇总；完整采集读取直播大屏曲线、商品分页与讲解明细。首次使用需要扫码登录。</p></div>
    </div>
    <div class="compass-controls">
      <label class="compass-shop-field">
        <span>下载店铺</span>
        <select class="mac-field" bind:value={compassTargetShopName} disabled={compassQuerying || compassRunning} aria-label="下载店铺">
          {#each COMPASS_TARGET_SHOPS as shop}<option value={shop.value}>{shop.label} · {shop.value}</option>{/each}
        </select>
      </label>
      <input class="mac-field" type="date" bind:value={compassDate} disabled={compassRunning} aria-label="直播日期" />
      <button type="button" class="mac-btn" on:click={queryCompassSessions} disabled={compassQuerying || compassRunning}>
        {#if compassQuerying}<Loader2 size={15} class="spin" />{:else}<RefreshCw size={15} />{/if}
        查询当天直播
      </button>
      <button type="button" class="mac-btn mac-btn-primary" on:click={startCompassDownloads} disabled={!canStartCompassDownload(compassRunning)}>
        {#if compassRunning}<Loader2 size={15} class="spin" />{:else}<Download size={15} />{/if}
        一键下载并导入当天全部场次
      </button>
      <button type="button" class="mac-btn capture-btn" on:click={startCompassCapture} disabled={compassCaptureRunning || compassRunning || compassQuerying}>
        {#if compassCaptureRunning}<Loader2 size={15} class="spin" />{:else}<Database size={15} />{/if}
        完整采集直播大屏
      </button>
    </div>
    {#if compassCaptureRunning || captureProgressMessage}
      <div class="capture-progress" role={captureProgressMessage.includes("失败") ? "alert" : "status"} aria-live="polite">
        <div class="capture-progress-head">
          <strong>{compassCaptureRunning ? "完整采集中" : "最近采集状态"}</strong>
          <span>{captureResponseCount} 条接口响应已安全保存</span>
        </div>
        <p>{captureProgressMessage}</p>
        {#if compassCaptureRunning}<div class="capture-progress-bar"><span></span></div>{/if}
      </div>
    {/if}
    {#if compassQueue.length}
      <div class="compass-queue">
        {#each compassQueue as item (item.sessionKey)}
          <article class:success={item.status === "imported" || item.status === "skipped"} class:error={item.status === "failed"}>
            <div><strong>{new Date(item.session.startedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}–{new Date(item.session.endedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}</strong><span>{item.session.title || item.session.shopName}</span></div>
            <div class="session-metrics"><span>{item.session.orderCount ?? "—"} 单</span><span>{item.session.paymentAmountText || "—"}</span></div>
            <div class="session-status"><strong>{compassStatusLabel(item.status)}</strong><span>{item.message}</span></div>
          </article>
        {/each}
      </div>
    {/if}
    {#if captures.length && !compassCaptureRunning}
      {@const latestCapture = captures[0]}
      {@const latestCoverage = Object.values(latestCapture.coverage || {})}
      {@const coveredCount = latestCoverage.filter((item) => item.status === "captured" || item.status === "unavailable" || item.status === "permission_denied").length}
      <div class="capture-latest">
        <div><strong>最近采集结果</strong><span>{latestCapture.targetShopName || "罗盘店铺"} · {latestCapture.targetDate || new Date(latestCapture.startedAt).toLocaleDateString("zh-CN")}</span></div>
        <div><strong>{latestCapture.analysisResponseCount || 0}</strong><span>可分析响应</span></div>
        <div><strong>{coveredCount}/{latestCoverage.length || 0}</strong><span>覆盖确认</span></div>
        <div><strong>{latestCapture.catalogEndpointCount || 0}</strong><span>接口目录</span></div>
        <div><strong>{latestCapture.catalogFieldCount || 0}</strong><span>字段目录</span></div>
        <span class:success-text={latestCapture.status === "completed"}>{latestCapture.status === "completed" ? "采集完成" : latestCapture.message}</span>
        <button type="button" class="mac-btn analyze-capture-btn" on:click={() => analyzeCapture(latestCapture.captureId)} disabled={captureAnalysisLoading}>
          {#if captureAnalysisLoading}<Loader2 size={14} class="spin" />{:else}<BarChart3 size={14} />{/if}
          {latestCapture.analysisResponseCount > 0 ? "分析采集数据" : "检查采集质量"}
        </button>
      </div>
    {/if}
  </section>
  {#if captureAnalysisError}
    <div class="analysis-error" role="alert"><AlertTriangle size={17} /><span>{captureAnalysisError}</span></div>
  {/if}
  {#if captureAnalysis}
    <section class="capture-analysis" aria-label="罗盘动态数据分析">
      <header class="analysis-header">
        <div class="analysis-title"><Activity size={20} /><div><h2>直播动态数据分析</h2><p>{captureAnalysis.targetShopName} · {captureAnalysis.targetDate} · {captureAnalysis.quality.complete ? "数据完整" : "数据待补充"}</p></div></div>
        <span class:quality-ok={captureAnalysis.quality.complete} class="quality-badge">{captureAnalysis.quality.analysisResponseCount} 条可分析响应</span>
      </header>
      <div class="quality-cards">
        <article><span>可分析接口</span><strong>{captureAnalysis.quality.analysisResponseCount}</strong></article>
        <article><span>HTTP / WebSocket</span><strong>{captureAnalysis.quality.httpResponseCount || 0} / {captureAnalysis.quality.websocketFrameCount || 0}</strong></article>
        <article><span>解析失败</span><strong>{captureAnalysis.quality.parseFailureCount || 0}</strong></article>
        <article><span>分钟曲线</span><strong>{captureAnalysis.quality.metricCount}</strong></article>
        <article><span>商品</span><strong>{captureAnalysis.quality.productCount}</strong></article>
        <article><span>页面/指标覆盖</span><strong>{captureAnalysis.quality.capturedSectionCount}/{captureAnalysis.quality.plannedSectionCount}</strong></article>
        <article><span>接口目录</span><strong>{captureAnalysis.quality.catalogEndpointCount}</strong></article>
        <article><span>字段目录</span><strong>{captureAnalysis.quality.catalogFieldCount}</strong></article>
      </div>
      {#if captureAnalysis.coverage?.length}
        <details class="coverage-details" open={!captureAnalysis.quality.complete}>
          <summary>查看全部页面与指标覆盖状态</summary>
          <div class="coverage-grid">
            {#each captureAnalysis.coverage as item (item.key)}
              <article class:captured={item.status === "captured"} class:unavailable={item.status === "unavailable" || item.status === "permission_denied"} class:untriggered={item.status === "untriggered"} class:parse-failed={item.status === "parse_failed"}>
                <div><strong>{item.label}</strong><span>{item.kind}</span></div>
                <small title={item.lastError || item.lastEndpoint || ""}>{item.status === "captured" ? `已采集 · ${item.responseCount}条${item.lastTransport === "websocket" ? "WS帧" : "响应"}` : item.status === "unavailable" ? "当前场次未提供" : item.status === "permission_denied" ? "当前账号无权限" : item.status === "untriggered" ? "已点击但未捕获响应" : item.status === "parse_failed" ? `响应解析失败 · ${item.failureCount || 1}次` : item.status === "capturing" ? "正在采集" : "等待采集"}</small>
              </article>
            {/each}
          </div>
        </details>
      {/if}
      {#if captureAnalysis.quality.issues.length}
        <div class="quality-issues" role="status" aria-live="polite">
          {#each captureAnalysis.quality.issues as issue}<p><AlertTriangle size={14} />{issue}</p>{/each}
        </div>
      {/if}
      {#if captureAnalysis.metrics.length}
        <section class="curve-workbench" aria-label="按指标和时间范围分析曲线">
          <div class="curve-controls">
            <fieldset class="metric-selector">
              <legend>分析指标（最多4项）</legend>
              <div aria-label="切换或组合曲线指标">
                {#each captureAnalysis.metrics as metric}
                  <button
                    type="button"
                    aria-pressed={selectedMetricLabels.includes(metric.label)}
                    class:active={selectedMetricLabels.includes(metric.label)}
                    on:click={() => toggleMetric(metric.label)}
                  >{metric.label}</button>
                {/each}
              </div>
            </fieldset>
            <fieldset class="range-selector">
              <legend>当前分析时间范围</legend>
              <div class="range-labels"><strong>{rangeLabel(rangeStart)}</strong><span>至</span><strong>{rangeLabel(rangeEnd)}</strong></div>
              <label><span>开始时间</span><input aria-label="曲线分析开始时间" type="range" min={fullRangeStart} max={rangeEnd} step="1" bind:value={rangeStart} /></label>
              <label><span>结束时间</span><input aria-label="曲线分析结束时间" type="range" min={rangeStart} max={fullRangeEnd} step="1" bind:value={rangeEnd} /></label>
              <button type="button" class="mac-btn" on:click={resetCurveRange}>恢复整场</button>
            </fieldset>
          </div>

          {#if normalizedComparison}
            <p class="normalization-note" role="status"><AlertTriangle size={14} aria-hidden="true" />当前组合包含不同量纲，曲线按各指标窗口内最小值=0、最大值=100归一化显示；统计卡和事件表仍使用原始数值，不能直接比较绝对值。</p>
          {/if}

          <div class="metric-analysis-grid" aria-live="polite">
            {#each windowAnalyses as analysis (analysis.metricId)}
              <article>
                <header><strong>{analysis.label}</strong><span>{analysis.status === "no_data" ? "无数据" : analysis.status === "insufficient" ? "样本不足" : analysis.status === "flat" ? "平坦曲线" : `${analysis.validPointCount}个有效点`}</span></header>
                <dl>
                  <div><dt>整体趋势</dt><dd class:negative={analysis.trendDirection === "falling"}>{trendLabel(analysis)}</dd></div>
                  <div><dt>峰值</dt><dd>{metricValue(analysis.maximum, analysis.unit)}<small>{analysis.maximumTime || "—"}</small></dd></div>
                  <div><dt>低谷</dt><dd>{metricValue(analysis.minimum, analysis.unit)}<small>{analysis.minimumTime || "—"}</small></dd></div>
                  <div><dt>变化 / 异常</dt><dd>{analysis.changes.length} / {analysis.anomalies.length}<small>{analysis.missingPointCount ? `缺失${analysis.missingPointCount}点` : "数据连续"}</small></dd></div>
                </dl>
              </article>
            {/each}
          </div>

          <article class="metric-chart">
            <div class="chart-legend" aria-label="曲线图例">
              {#each selectedMetrics as metric, index (metric.label)}
                {@const style = compassMetricStyle(index)}
                <span><svg viewBox="0 0 42 16" aria-hidden="true"><line x1="1" y1="8" x2="41" y2="8" stroke={style.color} stroke-width="2.5" stroke-dasharray={style.dash} /><circle cx="21" cy="8" r="3" fill={style.color} /></svg>{metric.label}</span>
              {/each}
            </div>
            <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
            <div class="chart-scroll" role="region" tabindex="0" aria-label="可横向滚动的多指标曲线图">
              <svg viewBox="0 0 860 220" role="img" aria-label={`${selectedMetricLabels.join("、")}在${rangeLabel(rangeStart)}至${rangeLabel(rangeEnd)}的曲线、变化点和异常区间`}>
                <line x1="32" y1="188" x2="828" y2="188" class="chart-axis" />
                <line x1="32" y1="22" x2="32" y2="188" class="chart-axis" />
                {#each windowAnalyses as analysis}
                  {#each analysis.anomalies as event (event.id)}
                    <rect x={chartX(event.startSortValue)} y="22" width={Math.max(5, chartX(event.endSortValue) - chartX(event.startSortValue))} height="166" class="chart-anomaly-region"><title>{analysis.label} {event.startTime}–{event.endTime}异常</title></rect>
                    <text x={Math.min(790, chartX(event.startSortValue) + 4)} y="36" class="chart-anomaly-label">异常</text>
                  {/each}
                {/each}
                {#each selectedMetrics as metric, index (metric.label)}
                  {@const style = compassMetricStyle(index)}
                  <path d={chartPath(metric)} class="chart-line" stroke={style.color} stroke-dasharray={style.dash} />
                  {#each metric.points.filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd && pointValid(point)) as point}
                    {#if style.shape === "circle"}
                      <circle cx={chartX(point.sortValue)} cy={chartY(metric, point)} r="2.7" fill={style.color}><title>{metric.label} {point.timeLabel}：{metricValue(point.value, metric.unit)}</title></circle>
                    {:else if style.shape === "square"}
                      <rect x={chartX(point.sortValue) - 2.7} y={chartY(metric, point) - 2.7} width="5.4" height="5.4" fill={style.color}><title>{metric.label} {point.timeLabel}：{metricValue(point.value, metric.unit)}</title></rect>
                    {:else if style.shape === "triangle"}
                      <path d={`M${chartX(point.sortValue)},${chartY(metric, point)-3.5} L${chartX(point.sortValue)-3.5},${chartY(metric, point)+3} L${chartX(point.sortValue)+3.5},${chartY(metric, point)+3} Z`} fill={style.color}><title>{metric.label} {point.timeLabel}：{metricValue(point.value, metric.unit)}</title></path>
                    {:else}
                      <path d={`M${chartX(point.sortValue)},${chartY(metric, point)-3.5} L${chartX(point.sortValue)+3.5},${chartY(metric, point)} L${chartX(point.sortValue)},${chartY(metric, point)+3.5} L${chartX(point.sortValue)-3.5},${chartY(metric, point)} Z`} fill={style.color}><title>{metric.label} {point.timeLabel}：{metricValue(point.value, metric.unit)}</title></path>
                    {/if}
                  {/each}
                  {@const lastPoint = [...metric.points].reverse().find((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd && pointValid(point))}
                  {#if lastPoint}<text x={Math.min(790, chartX(lastPoint.sortValue) + 6)} y={Math.max(16, chartY(metric, lastPoint) - 6)} fill={style.color} class="chart-series-label">{metric.label}</text>{/if}
                {/each}
              </svg>
            </div>
            <details class="point-table"><summary>查看当前窗口原始数据点</summary><table><thead><tr><th>指标</th><th>时间</th><th>数值</th><th>状态</th></tr></thead><tbody>{#each selectedMetrics as metric}{#each metric.points.filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd) as point}<tr><td>{metric.label}</td><td>{point.timeLabel}</td><td>{pointValid(point) ? metricValue(point.value, metric.unit) : "—"}</td><td>{pointValid(point) ? "有效" : "缺失"}</td></tr>{/each}{/each}</tbody></table></details>
          </article>

          <section class="curve-events" aria-label="当前窗口异常和明显变化事件">
            <header><div><AlertTriangle size={18} aria-hidden="true" /><div><h2>当前窗口事件</h2><p>异常与变化同时进入表格；证据不足时只标记待核实。</p></div></div><strong>{curveEvents.length}项</strong></header>
            {#if curveEvents.length}
              <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
              <div class="event-table-wrap" role="region" tabindex="0" aria-label="当前窗口异常和变化事件表"><table><thead><tr><th>指标</th><th>类型</th><th>时间</th><th>数值</th><th>变化</th><th>证据</th><th>操作</th></tr></thead><tbody>{#each curveEvents as event (event.id)}<tr><td>{event.metricLabel}</td><td>{event.kind}</td><td>{event.time}</td><td>{event.value.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}</td><td>{event.detail}</td><td>{compassEvidenceExplanation(event.evidence)}</td><td><button type="button" class="mac-btn" disabled={!event.evidence || !captureAnalysis.source} on:click={() => event.evidence && openDeclineEvidence(event.evidence)}><Play size={13} aria-hidden="true" />回看</button></td></tr>{/each}</tbody></table></div>
            {:else}<div class="decline-empty" role="status">当前指标和时间范围没有达到阈值的明显变化或异常区间。</div>{/if}
          </section>
        </section>
      {:else}
        <div class="analysis-empty" role="status"><BarChart3 size={24} /><strong>尚未识别到曲线数据</strong><span>当前采集只有页面配置，或直播大屏指标接口没有完成返回。请重新执行完整采集。</span></div>
      {/if}
      <section class="decline-analysis" aria-label="关键下降区间">
        <div class="decline-heading">
          <div><TrendingDown size={19} /><div><h2>关键下降区间</h2><p>连续下降才标记；原因是证据假设，不把时间相关直接写成确定因果。</p></div></div>
          <span>{visibleDeclineEvents.length} 个区间</span>
        </div>
        {#if captureAnalysis.aiSummary && rangeStart === fullRangeStart && rangeEnd === fullRangeEnd && visibleDeclineEvents.length === captureAnalysis.declineEvents.length}
          <p class="decline-ai-summary"><strong>AI 复盘：</strong>{captureAnalysis.aiSummary}</p>
        {/if}
        {#if visibleDeclineEvents.length}
          <div class="decline-grid">
            {#each visibleDeclineEvents as event (event.id)}
              <article class="decline-card">
                <header>
                  <div><strong>{event.metricLabel}下降 {event.dropPercent.toFixed(1)}%</strong><span>{event.startTime}–{event.endTime}</span></div>
                  <span class="confidence">置信度 {event.confidence}</span>
                </header>
                <p class="decline-explanation">{event.explanation}</p>
                {#if event.correlatedChanges.length}
                  <div class="change-chips" aria-label="同期指标变化">
                    {#each event.correlatedChanges as change}
                      <span class:down={change.changePercent < 0}>{change.label} {change.changePercent > 0 ? "+" : ""}{change.changePercent.toFixed(1)}%</span>
                    {/each}
                  </div>
                {/if}
                {#if event.possibleCauses.length}
                  <ul>{#each event.possibleCauses as cause}<li>{cause}</li>{/each}</ul>
                {/if}
                <div class="transcript-evidence">
                  <strong>当时说了什么 · 录播 {formatOffset(event.transcriptStartSeconds)}</strong>
                  {#if event.transcriptExcerpt}<pre>{event.transcriptExcerpt}</pre>{:else}<p>当前场次没有可用逐字稿，尚不能判断具体话术原因。</p>{/if}
                </div>
                <footer>
                  <span>{event.limitations}</span>
                  <div>
                    <button type="button" class="mac-btn" on:click={() => selectedMetricLabels = [event.metricLabel]}>定位曲线</button>
                    <button type="button" class="mac-btn mac-btn-primary" on:click={() => openDeclineEvidence(event)} disabled={!captureAnalysis.source}>
                      <Play size={14} />查看对应录播
                    </button>
                  </div>
                </footer>
              </article>
            {/each}
          </div>
        {:else}
          <div class="decline-empty" role="status">未检测到“连续 3 个数据点下降 ≥15%，且最低点下降 ≥20%”的区间。</div>
        {/if}
      </section>
      <div class="analysis-columns">
        <article><h2>确定性分析结论</h2>{#each captureAnalysis.findings as finding}<div class="finding" class:warning={finding.level === "warning"}><strong>{finding.title}</strong><p>{finding.summary}</p><span>{finding.evidence}</span></div>{/each}</article>
        <article><h2>商品数据</h2>{#if captureAnalysis.products.length}<table><thead><tr><th>商品</th><th>讲解</th><th>点击</th><th>成交额</th></tr></thead><tbody>{#each captureAnalysis.products.slice(0, 20) as product}<tr><td>{product.productName}</td><td>{product.explainCount}</td><td>{product.clickCount}</td><td>¥{product.paymentAmount.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}</td></tr>{/each}</tbody></table>{:else}<p class="muted">本次响应中未识别到商品明细。</p>{/if}</article>
        <article><h2>人群画像</h2>{#if captureAnalysis.audience?.length}<table><thead><tr><th>维度</th><th>人群</th><th>数值</th></tr></thead><tbody>{#each captureAnalysis.audience.slice(0, 50) as item}<tr><td>{item.dimension}</td><td>{item.label}</td><td>{item.value.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}{item.unit}</td></tr>{/each}</tbody></table>{:else}<p class="muted">当前账号或场次未返回人群画像，不按 0 处理。</p>{/if}</article>
        <article><h2>千川投放</h2>{#if captureAnalysis.qianchuan?.length}<table><thead><tr><th>指标</th><th>数值</th><th>响应字段</th></tr></thead><tbody>{#each captureAnalysis.qianchuan as item}<tr><td>{item.label}</td><td>{item.value.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}{item.unit}</td><td title={item.sourceEndpoint}>{item.key}</td></tr>{/each}</tbody></table>{:else}<p class="muted">当前账号或场次未返回千川指标，不按 0 处理。</p>{/if}</article>
      </div>
    </section>
  {/if}
  {#if message}<p class="message">{message}</p>{/if}
  {#if sessions.length}
    <section class="session-picker" aria-label="选择直播场次">
      <label>
        <span>店铺</span>
        <select class="mac-field" value={selectedShopName} on:change={selectShop}>
          {#each shopNames as shopName}<option value={shopName}>{shopName}</option>{/each}
        </select>
      </label>
      <label>
        <span>直播日期</span>
        <select class="mac-field" value={selectedSessionDate} on:change={selectSessionDate}>
          {#each sessionDates as date}<option value={date}>{displaySessionDate(date)}</option>{/each}
        </select>
      </label>
      <label class="session-time-field">
        <span>当天场次</span>
        <select class="mac-field" value={detail?.session.id} on:change={(event) => refresh(Number(event.currentTarget.value))}>
          {#each visibleSessions as session}
            <option value={session.id}>{displaySessionTime(session.startedAt)}—{session.endedAt ? displaySessionTime(session.endedAt) : "结束时间待补"}　{money(session.paymentAmountFen)}</option>
          {/each}
        </select>
      </label>
      <p class="idm-picker-hint">视频命名不跟这三个下拉框走。软件开着时去罗盘点「下载视频」，会读取页面上的店铺和开播时间，改成「店铺_开播时间」。</p>
    </section>
  {/if}
  {#if detail}
    <div class="cards">{#each dashboardMetricCards(detail.session) as card}<article><span>{card.label}</span><strong>{card.value}</strong></article>{/each}</div>
    <div class="meta">来源：{detail.session.sourceFile}　导入：{new Date(detail.session.importedAt).toLocaleString("zh-CN")}</div>
    <div class="tables"><article><h2>渠道分析</h2><table><thead><tr><th>渠道</th><th>观看人数</th><th>用户支付金额</th><th>订单</th></tr></thead><tbody>{#each detail.channels as channel}<tr><td>{channel.name}</td><td>{channel.viewerCount ?? "—"}</td><td>{money(channel.paymentAmountFen)}</td><td>{channel.orderCount ?? "—"}</td></tr>{/each}</tbody></table></article><article><h2>短视频引流</h2><table><thead><tr><th>短视频</th><th>观看人数</th><th>用户支付金额</th><th>订单</th></tr></thead><tbody>{#each detail.shortVideos as video}<tr><td>{video.title}</td><td>{video.viewerCount ?? "—"}</td><td>{money(video.paymentAmountFen)}</td><td>{video.orderCount ?? "—"}</td></tr>{/each}</tbody></table></article><article><h2>商品成交榜</h2><table><thead><tr><th>商品</th><th>支付金额</th><th>件数</th><th>人数</th></tr></thead><tbody>{#each detail.products as product}<tr><td title={product.productId}>{product.name}</td><td>{money(product.paymentAmountFen)}</td><td>{product.soldCount ?? "—"}</td><td>{product.buyerCount ?? "—"}</td></tr>{/each}</tbody></table></article></div>
  {:else if !loading}
    <div class="empty">暂无直播数据。请在罗盘下载官方 XLSX，软件会自动导入；也可点击“导入 XLSX”。</div>
  {/if}
</PageShell>

<style>
  h2, p { margin: 0; }
  h2 { font-size: 15px; color: var(--mac-label); }
  .meta { margin: 14px 0; color: var(--mac-tertiary); font-size: 12px; }
  .message { margin: 0; color: var(--mac-blue); font-size: 12px; }
  .compass-panel {
    border: 1px solid color-mix(in srgb, var(--mac-blue) 22%, var(--mac-separator));
    border-radius: var(--mac-radius-lg);
    background: linear-gradient(135deg, #f8fbff, #fff);
    padding: 16px;
  }
  .compass-copy, .compass-controls { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .compass-copy p { margin-top: 3px; color: var(--mac-tertiary); font-size: 12px; }
  .compass-icon {
    display: grid; place-items: center; width: 38px; height: 38px;
    border-radius: 10px; background: var(--mac-blue-soft); color: var(--mac-blue);
  }
  .compass-controls { margin-top: 14px; align-items: flex-end; }
  .compass-controls input { min-width: 150px; }
  .compass-shop-field { display: grid; gap: 5px; min-width: 300px; }
  .compass-shop-field > span { color: var(--mac-tertiary); font-size: 11px; font-weight: 600; }
  .compass-shop-field select { width: 100%; height: 34px; }
  .capture-btn {
    border-color: color-mix(in srgb, #6750a4 34%, var(--mac-separator));
    background: color-mix(in srgb, #6750a4 9%, var(--mac-bg-card));
    color: #5b43a0;
  }
  .capture-progress {
    display: grid; gap: 7px; margin-top: 14px; padding: 12px 13px;
    border: 1px solid color-mix(in srgb, #6750a4 26%, var(--mac-separator));
    border-radius: 11px; background: color-mix(in srgb, #6750a4 5%, var(--mac-bg-card));
  }
  .capture-progress-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .capture-progress-head strong { color: #5b43a0; font-size: 13px; }
  .capture-progress-head span, .capture-progress p { color: var(--mac-tertiary); font-size: 12px; }
  .capture-progress-bar { height: 4px; overflow: hidden; border-radius: 999px; background: color-mix(in srgb, #6750a4 14%, transparent); }
  .capture-progress-bar span { display: block; width: 34%; height: 100%; border-radius: inherit; background: #6750a4; animation: capture-slide 1.4s ease-in-out infinite; }
  @keyframes capture-slide { from { transform: translateX(-110%); } to { transform: translateX(300%); } }
  @media (prefers-reduced-motion: reduce) {
    .capture-progress-bar span { width: 100%; opacity: .55; animation: none; transform: none; }
  }
  .capture-latest {
    display: grid; grid-template-columns: minmax(220px, 1.4fr) repeat(4, minmax(80px, .45fr)) minmax(140px, .8fr) auto;
    gap: 14px; align-items: center; margin-top: 12px; padding: 11px 13px;
    border: 1px solid var(--mac-separator); border-radius: 11px; background: var(--mac-bg-card);
  }
  .capture-latest > div { display: grid; gap: 2px; }
  .capture-latest strong { font-size: 13px; color: var(--mac-label); }
  .capture-latest span { color: var(--mac-tertiary); font-size: 11px; }
  .capture-latest > span { text-align: right; }
  .capture-latest .success-text { color: #16803c; font-weight: 650; }
  .analyze-capture-btn { justify-self: end; white-space: nowrap; }
  .analysis-error, .quality-issues {
    display: flex; align-items: center; gap: 8px; padding: 11px 13px;
    border: 1px solid #f4b8b1; border-radius: 11px; background: #fff7f6; color: #a13a2f; font-size: 12px;
  }
  .capture-analysis {
    display: grid; gap: 14px; padding: 17px; border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-lg); background: var(--mac-bg-card);
  }
  .analysis-header, .analysis-title { display: flex; align-items: center; gap: 10px; }
  .analysis-header { justify-content: space-between; }
  .analysis-title { color: var(--mac-blue); }
  .analysis-title p { margin-top: 3px; color: var(--mac-tertiary); font-size: 12px; }
  .quality-badge { padding: 5px 9px; border-radius: 999px; background: #fff5eb; color: #a45116; font-size: 11px; font-weight: 650; }
  .quality-badge.quality-ok { background: #ecfdf3; color: #16803c; }
  .quality-cards { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 9px; }
  .quality-cards article { padding: 11px 12px; border: 1px solid var(--mac-separator); border-radius: 10px; background: var(--mac-bg); }
  .quality-cards span { display: block; color: var(--mac-tertiary); font-size: 11px; }
  .quality-cards strong { display: block; margin-top: 5px; font-size: 19px; }
  .coverage-details { border: 1px solid var(--mac-separator); border-radius: 11px; padding: 10px 12px; background: var(--mac-bg); }
  .coverage-details summary { color: var(--mac-blue); font-size: 12px; cursor: pointer; }
  .coverage-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 7px; margin-top: 10px; }
  .coverage-grid article { display: grid; gap: 5px; min-width: 0; padding: 8px 9px; border: 1px solid var(--mac-separator); border-radius: 9px; background: var(--mac-bg-card); }
  .coverage-grid article.captured { border-color: #abefc6; background: #f6fef9; }
  .coverage-grid article.unavailable { border-style: dashed; opacity: .72; }
  .coverage-grid article.untriggered { border-color: #f5c26b; background: #fffaf0; }
  .coverage-grid article.parse-failed { border-color: #fda29b; background: #fff6f5; }
  .coverage-grid article > div { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .coverage-grid strong { overflow: hidden; color: var(--mac-label); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .coverage-grid span, .coverage-grid small { color: var(--mac-tertiary); font-size: 9px; }
  .quality-issues { display: grid; align-items: initial; }
  .quality-issues p { display: flex; align-items: center; gap: 7px; }
  .curve-workbench { display: grid; gap: 12px; min-width: 0; }
  .curve-controls { display: grid; grid-template-columns: minmax(0, 1.45fr) minmax(280px, .8fr); gap: 12px; }
  .curve-controls fieldset { min-width: 0; margin: 0; padding: 11px 12px 12px; border: 1px solid var(--mac-separator); border-radius: 11px; }
  .curve-controls legend { padding: 0 5px; color: var(--mac-secondary); font-size: 12px; font-weight: 650; }
  .metric-selector > div { display: flex; flex-wrap: wrap; gap: 8px; }
  .metric-selector button {
    min-height: 44px; padding: 8px 12px; border: 1px solid var(--mac-separator); border-radius: 999px;
    background: var(--mac-bg); color: var(--mac-secondary); font-size: 12px; cursor: pointer;
  }
  .metric-selector button.active { border-color: var(--mac-blue); background: var(--mac-blue-soft); color: var(--mac-blue); font-weight: 700; box-shadow: inset 0 0 0 1px var(--mac-blue); }
  .metric-selector button:focus-visible, .range-selector input:focus-visible, .chart-scroll:focus-visible, .event-table-wrap:focus-visible { outline: 3px solid color-mix(in srgb, var(--mac-blue) 48%, transparent); outline-offset: 2px; }
  .range-selector { display: grid; gap: 8px; }
  .range-labels { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: var(--mac-secondary); font-size: 12px; }
  .range-selector label { display: grid; grid-template-columns: 68px minmax(0, 1fr); align-items: center; gap: 8px; color: var(--mac-tertiary); font-size: 11px; }
  .range-selector input { width: 100%; min-width: 0; accent-color: var(--mac-blue); }
  .range-selector .mac-btn { min-height: 36px; justify-self: end; }
  .normalization-note { display: flex; align-items: flex-start; gap: 7px; margin: 0; padding: 9px 11px; border: 1px solid #f5c26b; border-radius: 9px; background: #fffaf0; color: #7a4b00; font-size: 11px; line-height: 1.5; }
  .metric-analysis-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  .metric-analysis-grid article { min-width: 0; padding: 12px; border: 1px solid var(--mac-separator); border-radius: 11px; background: var(--mac-bg); }
  .metric-analysis-grid header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .metric-analysis-grid header strong { font-size: 13px; }
  .metric-analysis-grid header span { color: var(--mac-tertiary); font-size: 10px; }
  .metric-analysis-grid dl { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; margin: 10px 0 0; }
  .metric-analysis-grid dl div { min-width: 0; padding-top: 7px; border-top: 1px solid var(--mac-separator); }
  .metric-analysis-grid dt { color: var(--mac-tertiary); font-size: 10px; }
  .metric-analysis-grid dd { display: grid; gap: 2px; margin: 4px 0 0; font-size: 13px; font-weight: 650; }
  .metric-analysis-grid dd.negative { color: #b42318; }
  .metric-analysis-grid dd small { overflow: hidden; color: var(--mac-tertiary); font-size: 9px; font-weight: 400; text-overflow: ellipsis; white-space: nowrap; }
  .metric-chart { overflow: hidden; border: 1px solid var(--mac-separator); border-radius: 12px; background: var(--mac-bg); }
  .chart-legend { display: flex; flex-wrap: wrap; gap: 8px 14px; padding: 11px 14px 4px; }
  .chart-legend span { display: inline-flex; align-items: center; gap: 6px; color: var(--mac-secondary); font-size: 11px; font-weight: 650; }
  .chart-legend svg { width: 42px; height: 16px; }
  .chart-scroll { min-width: 0; overflow-x: auto; padding: 0 10px 8px; }
  .chart-scroll svg { display: block; width: 100%; min-width: 640px; height: 230px; }
  .chart-axis { stroke: color-mix(in srgb, var(--mac-tertiary) 36%, transparent); stroke-width: 1; }
  .chart-line { fill: none; stroke-width: 2.5; stroke-linecap: round; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
  .chart-anomaly-region { fill: rgba(217, 45, 32, .09); stroke: #d92d20; stroke-width: .7; stroke-dasharray: 3 3; }
  .chart-anomaly-label { fill: #a11a12; font-size: 9px; font-weight: 700; }
  .chart-series-label { font-size: 9px; font-weight: 700; paint-order: stroke; stroke: var(--mac-bg); stroke-width: 3px; }
  .point-table { border-top: 1px solid var(--mac-separator); padding: 9px 14px 13px; }
  .point-table summary { color: var(--mac-blue); font-size: 12px; cursor: pointer; }
  .point-table table { max-height: 280px; }
  .curve-events { display: grid; gap: 9px; min-width: 0; }
  .curve-events > header, .curve-events > header > div { display: flex; align-items: center; gap: 8px; }
  .curve-events > header { justify-content: space-between; }
  .curve-events > header > div { color: #b42318; }
  .curve-events h2 { font-size: 14px; }
  .curve-events p { margin-top: 2px; color: var(--mac-tertiary); font-size: 10px; }
  .curve-events > header > strong { font-size: 12px; }
  .event-table-wrap { max-width: 100%; overflow: auto; border: 1px solid var(--mac-separator); border-radius: 10px; }
  .event-table-wrap table { min-width: 920px; margin: 0; }
  .event-table-wrap td:nth-child(6) { min-width: 250px; white-space: normal; line-height: 1.45; }
  .event-table-wrap .mac-btn { min-height: 36px; white-space: nowrap; }
  .analysis-empty { display: grid; place-items: center; gap: 7px; padding: 34px; border: 1px dashed var(--mac-separator-strong); border-radius: 12px; text-align: center; color: var(--mac-tertiary); }
  .analysis-empty strong { color: var(--mac-label); }
  .analysis-empty span { max-width: 560px; font-size: 12px; line-height: 1.55; }
  .decline-analysis { display: grid; gap: 12px; padding-top: 2px; }
  .decline-heading, .decline-heading > div, .decline-card header, .decline-card footer { display: flex; align-items: center; gap: 10px; }
  .decline-heading, .decline-card header, .decline-card footer { justify-content: space-between; }
  .decline-heading > div { color: #d92d20; }
  .decline-heading h2 { font-size: 15px; }
  .decline-heading p { margin-top: 3px; color: var(--mac-tertiary); font-size: 11px; }
  .decline-heading > span, .confidence { padding: 5px 8px; border-radius: 999px; background: #fff1f0; color: #b42318; font-size: 11px; white-space: nowrap; }
  .decline-ai-summary { margin: 0; padding: 11px 13px; border-radius: 10px; background: var(--mac-blue-soft); color: var(--mac-secondary); font-size: 12px; line-height: 1.6; }
  .decline-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 11px; }
  .decline-card { display: grid; align-content: start; gap: 10px; min-width: 0; padding: 14px; border: 1px solid color-mix(in srgb, #d92d20 24%, var(--mac-separator)); border-radius: 12px; background: var(--mac-bg); }
  .decline-card header > div { display: grid; gap: 3px; }
  .decline-card header strong { color: #b42318; font-size: 13px; }
  .decline-card header span, .decline-card footer > span { color: var(--mac-tertiary); font-size: 10px; line-height: 1.45; }
  .decline-explanation { margin: 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  .change-chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .change-chips span { padding: 4px 7px; border-radius: 999px; background: #ecfdf3; color: #067647; font-size: 10px; }
  .change-chips span.down { background: #fff1f0; color: #b42318; }
  .decline-card ul { display: grid; gap: 5px; margin: 0; padding-left: 17px; color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .transcript-evidence { min-width: 0; padding: 10px; border-radius: 9px; background: var(--mac-bg-card); }
  .transcript-evidence strong { color: var(--mac-label); font-size: 11px; }
  .transcript-evidence pre, .transcript-evidence p { max-height: 132px; margin: 7px 0 0; overflow: auto; white-space: pre-wrap; color: var(--mac-secondary); font: 11px/1.55 inherit; }
  .decline-card footer { align-items: flex-end; padding-top: 2px; }
  .decline-card footer > span { max-width: 54%; }
  .decline-card footer > div { display: flex; gap: 7px; }
  .decline-empty { padding: 22px; border: 1px dashed var(--mac-separator-strong); border-radius: 11px; text-align: center; color: var(--mac-tertiary); font-size: 12px; }
  .analysis-columns { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
  .analysis-columns > article { padding: 14px; border: 1px solid var(--mac-separator); border-radius: 11px; }
  .finding { margin-top: 10px; padding: 10px; border-radius: 9px; background: var(--mac-bg); }
  .finding.warning { background: #fff8ed; }
  .finding strong { font-size: 12px; }
  .finding p, .finding span, .muted { margin-top: 4px; color: var(--mac-tertiary); font-size: 11px; line-height: 1.5; }
  .compass-queue { display: grid; gap: 8px; margin-top: 14px; }
  .compass-queue article {
    display: grid;
    grid-template-columns: minmax(220px, 1.4fr) minmax(130px, 0.6fr) minmax(220px, 1fr);
    gap: 16px; align-items: center;
    border: 1px solid var(--mac-separator); border-radius: 11px;
    background: var(--mac-bg-card); padding: 11px 13px;
  }
  .compass-queue article.success { border-color: #a6f4c5; background: #f6fef9; }
  .compass-queue article.error { border-color: #fecdca; background: #fffbfa; }
  .compass-queue article div { display: grid; gap: 2px; }
  .compass-queue article span { color: var(--mac-tertiary); font-size: 12px; }
  .session-metrics { grid-auto-flow: column; justify-content: start; gap: 16px !important; }
  .session-status strong { font-size: 12px; color: var(--mac-blue); }
  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .session-picker {
    display: grid; grid-template-columns: minmax(220px, 1.15fr) minmax(170px, 0.7fr) minmax(280px, 1fr);
    gap: 12px; padding: 14px; border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-md); background: var(--mac-bg-card);
  }
  .session-picker label { display: grid; gap: 6px; min-width: 0; }
  .session-picker label > span { color: var(--mac-tertiary); font-size: 12px; font-weight: 600; }
  .session-picker select { width: 100%; min-width: 0; height: 36px; }
  .idm-picker-hint {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--mac-tertiary);
    font-size: 12px;
    line-height: 1.5;
  }
  .cards {
    display: grid; grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 12px;
  }
  .cards article, .tables article {
    border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md);
    background: var(--mac-bg-card); padding: 15px;
  }
  .cards span { display: block; color: var(--mac-tertiary); font-size: 12px; }
  .cards strong {
    display: block; margin-top: 8px; color: var(--mac-blue);
    font-size: 22px; letter-spacing: -0.4px;
  }
  .tables { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
  table { width: 100%; margin-top: 10px; border-collapse: collapse; font-size: 12px; }
  th, td { padding: 9px 7px; border-bottom: 1px solid var(--mac-separator); text-align: left; }
  th { color: var(--mac-tertiary); }
  td:first-child { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty {
    margin-top: 8px; padding: 48px; text-align: center;
    border: 1px dashed var(--mac-separator-strong); border-radius: var(--mac-radius-md);
    color: var(--mac-tertiary);
  }
  @media (max-width: 1200px) { .cards { grid-template-columns: repeat(3, minmax(0, 1fr)); } }
  @media (max-width: 1000px) {
    .cards, .tables { grid-template-columns: 1fr; }
    .compass-queue article { grid-template-columns: 1fr; }
    .capture-latest { grid-template-columns: repeat(3, minmax(80px, 1fr)); }
    .capture-latest > div:first-child, .capture-latest > span, .analyze-capture-btn { grid-column: 1 / -1; text-align: left; justify-self: start; }
    .session-picker { grid-template-columns: 1fr; }
    .analysis-columns, .decline-grid { grid-template-columns: 1fr; }
    .coverage-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .curve-controls { grid-template-columns: 1fr; }
    .metric-analysis-grid { grid-template-columns: 1fr; }
  }
  @media (max-width: 700px) {
    .quality-cards { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .analysis-header { align-items: flex-start; }
    .metric-analysis-grid dl { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .curve-controls fieldset { padding-inline: 9px; }
    .metric-selector button { flex: 1 1 calc(50% - 8px); min-width: 0; }
    .chart-scroll svg { min-width: 560px; }
  }
  @media (max-width: 420px) {
    .quality-cards, .coverage-grid { grid-template-columns: 1fr; }
    .metric-selector button { flex-basis: 100%; }
    .range-selector label { grid-template-columns: 1fr; }
    .analysis-header, .curve-events > header { display: grid; }
  }
  :global(.dark) .compass-panel {
    background: color-mix(in srgb, var(--mac-blue) 8%, var(--mac-bg-card));
    border-color: color-mix(in srgb, var(--mac-blue) 28%, var(--mac-separator));
  }
  :global(.dark) .compass-queue article.success { background: #163528; border-color: #24553a; }
  :global(.dark) .compass-queue article.error { background: #3a2320; border-color: #6b3530; }
  :global(.dark) .analysis-error, :global(.dark) .quality-issues { background: #3a2320; border-color: #6b3530; color: #f5a59b; }
  :global(.dark) .normalization-note { background: #3b2d1d; border-color: #76552a; color: #f4c77b; }
  :global(.dark) .finding.warning { background: #3b2d1d; }
  :global(.dark) .decline-heading > span, :global(.dark) .confidence { background: #3a2320; color: #f5a59b; }
</style>
