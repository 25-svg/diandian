<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { AlertTriangle, Database, Loader2, RefreshCw } from "lucide-svelte";
  import { invoke } from "../../invoker";
  import type { LiveDataBoardSession } from "../../liveDashboard";

  export let session: LiveDataBoardSession | null = null;

  type CoverageItem = {
    key: string;
    label: string;
    status: string;
    responseCount?: number;
    lastEndpoint?: string | null;
    lastError?: string | null;
  };

  type CaptureProgress = {
    captureId: string;
    status?: string;
    message?: string;
    responseCount?: number;
    analysisResponseCount?: number;
    coverage?: Record<string, CoverageItem>;
  };

  type EmbeddedBounds = {
    x: number;
    y: number;
    width: number;
    height: number;
    visible: boolean;
  };

  const moduleDefinitions = [
    { key: "module.data", label: "曲线" },
    { key: "module.product", label: "商品" },
    { key: "module.audience", label: "人群" },
    { key: "module.qianchuan", label: "千川" },
  ] as const;

  let host: HTMLDivElement | null = null;
  let mounted = false;
  let opening = false;
  let opened = false;
  let activeCaptureId = "";
  let loadedSessionKey = "";
  let message = "等待打开官方罗盘";
  let error = "";
  let responseCount = 0;
  let analysisResponseCount = 0;
  let coverage: Record<string, CoverageItem> = {};
  let resizeObserver: ResizeObserver | null = null;
  let unlisten: (() => void) | null = null;
  let animationFrame = 0;

  $: sessionKey = session ? `${session.id}:${session.shopName}:${session.startedAt}` : "";
  $: if (mounted && sessionKey && sessionKey !== loadedSessionKey) {
    loadedSessionKey = sessionKey;
    void startEmbeddedCapture();
  }

  function visibleBounds(): EmbeddedBounds | null {
    if (!host) return null;
    const rect = host.getBoundingClientRect();
    const x = Math.max(0, rect.left);
    const y = Math.max(0, rect.top);
    const right = Math.min(window.innerWidth, rect.right);
    const bottom = Math.min(window.innerHeight, rect.bottom);
    const width = Math.max(0, right - x);
    const height = Math.max(0, bottom - y);
    return {
      x,
      y,
      width: Math.max(width, 160),
      height: Math.max(height, 120),
      visible: document.visibilityState === "visible" && width >= 160 && height >= 120,
    };
  }

  async function syncBounds(): Promise<void> {
    if (!opened) return;
    const bounds = visibleBounds();
    if (!bounds) return;
    try {
      await invoke("update_embedded_compass_bounds", bounds);
    } catch (reason) {
      error = `官方罗盘区域同步失败：${String(reason)}`;
    }
  }

  function scheduleBoundsSync(): void {
    window.cancelAnimationFrame(animationFrame);
    animationFrame = window.requestAnimationFrame(() => void syncBounds());
  }

  async function startEmbeddedCapture(): Promise<void> {
    const currentSession = session;
    if (!currentSession || opening) return;
    await tick();
    await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
    const bounds = visibleBounds();
    if (!bounds?.visible) {
      error = "官方罗盘区域当前不可见，请展开数据看板后重试。";
      return;
    }
    opening = true;
    opened = false;
    activeCaptureId = "";
    responseCount = 0;
    analysisResponseCount = 0;
    coverage = {};
    error = "";
    message = "正在内嵌官方罗盘，并在页面请求前启用网络采集";
    try {
      activeCaptureId = await invoke<string>("open_embedded_compass_capture", {
        targetDate: currentSession.startedAt.slice(0, 10),
        targetShopName: currentSession.shopName,
        targetStartedAt: currentSession.startedAt,
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
      });
      opened = true;
      message = "官方罗盘已打开；正在采集曲线、商品、人群和千川响应";
      scheduleBoundsSync();
    } catch (reason) {
      error = `官方罗盘打开失败：${String(reason)}`;
      message = "未启动采集";
    } finally {
      opening = false;
    }
  }

  function statusLabel(item?: CoverageItem): string {
    if (!item) return "等待";
    if (item.status === "captured") return `已拿到${item.responseCount ? ` ${item.responseCount} 条` : ""}`;
    if (item.status === "permission_denied") return "账号无权限";
    if (item.status === "unavailable") return "本场未提供";
    if (item.status === "parse_failed") return "解析失败";
    if (item.status === "untriggered") return "未收到响应";
    if (item.status === "capturing" || item.status === "triggered") return "采集中";
    return "等待";
  }

  function moduleItem(key: string): CoverageItem | undefined {
    return coverage[key];
  }

  onMount(() => {
    mounted = true;
    resizeObserver = new ResizeObserver(scheduleBoundsSync);
    if (host) resizeObserver.observe(host);
    window.addEventListener("resize", scheduleBoundsSync);
    window.addEventListener("scroll", scheduleBoundsSync, true);
    document.addEventListener("visibilitychange", scheduleBoundsSync);
    void listen<CaptureProgress>("compass-capture-progress", (event) => {
      const progress = event.payload;
      if (!progress?.captureId) return;
      if (!activeCaptureId && opening) activeCaptureId = progress.captureId;
      if (progress.captureId !== activeCaptureId) return;
      responseCount = progress.responseCount ?? responseCount;
      analysisResponseCount = progress.analysisResponseCount ?? analysisResponseCount;
      coverage = progress.coverage ?? coverage;
      message = progress.message || message;
      if (progress.status === "failed") error = progress.message || "官方罗盘采集失败";
    }).then((dispose) => { unlisten = dispose; });
  });

  onDestroy(() => {
    mounted = false;
    window.cancelAnimationFrame(animationFrame);
    resizeObserver?.disconnect();
    unlisten?.();
    window.removeEventListener("resize", scheduleBoundsSync);
    window.removeEventListener("scroll", scheduleBoundsSync, true);
    document.removeEventListener("visibilitychange", scheduleBoundsSync);
    void invoke("close_embedded_compass").catch(() => {});
  });
</script>

<section class="embedded-compass" aria-label="官方抖音罗盘直播大屏">
  <header>
    <div class="title">
      <Database size={17} aria-hidden="true" />
      <div>
        <strong>官方罗盘</strong>
        <span>{session ? `${session.shopName} · ${session.startedAt}` : "先绑定直播场次"}</span>
      </div>
    </div>
    <div class="actions">
      <span class="response-summary" aria-live="polite">{responseCount} 条响应 · {analysisResponseCount} 条可分析</span>
      <button type="button" disabled={!session || opening} on:click={startEmbeddedCapture}>
        {#if opening}<Loader2 size={15} class="is-spinning" aria-hidden="true" />{:else}<RefreshCw size={15} aria-hidden="true" />{/if}
        重新加载并采集
      </button>
    </div>
  </header>

  <div class="coverage" aria-label="罗盘模块采集状态">
    {#each moduleDefinitions as definition}
      {@const item = moduleItem(definition.key)}
      <div class:captured={item?.status === "captured"} class:error-state={item?.status === "parse_failed"}>
        <strong>{definition.label}</strong>
        <span>{statusLabel(item)}</span>
      </div>
    {/each}
  </div>

  {#if !session}
    <div class="empty"><strong>先绑定本场直播数据</strong><span>绑定后将在这里直接显示对应店铺的官方罗盘，并采集可分析响应。</span></div>
  {:else}
    <div class="native-host" bind:this={host} aria-label="官方罗盘交互区域">
      <div class="host-placeholder" aria-hidden="true">
        {#if opening}<Loader2 size={22} class="is-spinning" />{/if}
        <strong>{opening ? "正在打开官方罗盘" : "官方罗盘原始页面显示区域"}</strong>
        <span>页面显示后可直接操作；本地分析使用同一批接口响应。</span>
      </div>
    </div>
  {/if}

  <p class="status" role={error ? "alert" : "status"} aria-live="polite">
    {#if error}<AlertTriangle size={14} aria-hidden="true" />{/if}
    <span>{error || message}</span>
  </p>
</section>

<style>
  .embedded-compass { min-width: 0; min-height: 0; display: grid; grid-template-rows: auto auto minmax(240px, 1fr) auto; gap: 8px; height: 100%; padding: 10px; color: #344054; background: #fff; }
  header, .title, .actions { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .title { justify-content: flex-start; min-width: 0; color: #175cd3; }
  .title > div { min-width: 0; display: grid; gap: 2px; }
  .title strong { color: #101828; font-size: 13px; }
  .title span { overflow: hidden; color: #667085; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .actions { flex: 0 0 auto; }
  .actions button { min-height: 44px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; padding: 0 12px; border: 1px solid #d0d5dd; border-radius: 8px; background: #fff; color: #344054; font-size: 12px; font-weight: 600; cursor: pointer; }
  .actions button:disabled { opacity: .5; cursor: not-allowed; }
  .actions button:focus-visible { outline: 3px solid rgba(23, 105, 210, .35); outline-offset: 2px; }
  .response-summary { color: #667085; font-size: 11px; font-variant-numeric: tabular-nums; }
  .coverage { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; }
  .coverage > div { min-width: 0; display: grid; gap: 2px; padding: 7px 9px; border: 1px solid #e4e7ec; border-left: 4px solid #98a2b3; border-radius: 8px; background: #f9fafb; }
  .coverage > div.captured { border-left-style: solid; border-left-color: #067647; background: #ecfdf3; }
  .coverage > div.error-state { border-left-style: dashed; border-left-color: #b42318; background: #fff6f5; }
  .coverage strong { color: #344054; font-size: 11px; }
  .coverage span { color: #667085; font-size: 10px; }
  .native-host { position: relative; min-width: 0; min-height: 240px; overflow: hidden; border: 1px solid #d0d5dd; border-radius: 10px; background: #f2f4f7; }
  .host-placeholder, .empty { min-height: 240px; display: grid; place-content: center; justify-items: center; gap: 7px; padding: 20px; color: #667085; text-align: center; }
  .host-placeholder strong, .empty strong { color: #101828; font-size: 13px; }
  .host-placeholder span, .empty span { max-width: 480px; font-size: 11px; line-height: 1.5; }
  .status { min-height: 18px; display: flex; align-items: center; gap: 6px; margin: 0; color: #667085; font-size: 11px; }
  .status[role="alert"] { color: #b42318; }
  :global(.is-spinning) { animation: embedded-compass-spin .9s linear infinite; }
  @keyframes embedded-compass-spin { to { transform: rotate(360deg); } }
  @media (max-width: 860px) {
    .embedded-compass { grid-template-rows: auto auto minmax(210px, 1fr) auto; }
    header { align-items: flex-start; }
    .actions { align-items: flex-end; flex-direction: column-reverse; }
    .coverage { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .response-summary { max-width: 180px; text-align: right; }
  }
  @media (prefers-reduced-motion: reduce) { :global(.is-spinning) { animation: none; } }
</style>
