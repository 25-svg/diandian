<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { CalendarDays, Clipboard, Download, FolderOpen, Loader2, RefreshCw, Upload } from "lucide-svelte";
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
  let selectedShopName = "";
  let selectedSessionDate = "";
  let idmPreparedKey = "";

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

  async function copyDownloadName(value: string): Promise<boolean> {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
        return true;
      }
    } catch (_) {}
    try {
      const input = document.createElement("textarea");
      input.value = value;
      input.style.position = "fixed";
      input.style.opacity = "0";
      document.body.appendChild(input);
      input.select();
      const copied = document.execCommand("copy");
      input.remove();
      return copied;
    } catch (_) {
      return false;
    }
  }

  async function prepareIdmVideoDownload(session: Pick<Session, "shopName" | "startedAt">) {
    const key = `${session.shopName}|${session.startedAt}`;
    try {
      const prepared = await invoke<{ suggestedFileName: string; watcherStarted: boolean }>(
        "prepare_idm_download_filename",
        { shopName: session.shopName, startedAt: session.startedAt },
      );
      idmPreparedKey = key;
      const copied = await copyDownloadName(prepared.suggestedFileName);
      message = `已准备：${prepared.suggestedFileName}。现在点击网页“下载该视频”，IDM 将自动填名${copied ? "；文件名也已复制" : ""}。`;
    } catch (error) {
      message = `准备 IDM 文件名失败：${String(error)}`;
    }
  }

  function handleCompassProgress(progress: CompassBrowserProgress) {
    if (progress.targetDate !== compassDate) return;
    if (progress.targetShopName && progress.targetShopName !== compassTargetShopName) return;
    const terminalStatuses = ["failed", "batch-finished", "shop-mismatch", "shop-unavailable"];
    compassQuerying = !["sessions-found", ...terminalStatuses].includes(progress.status);
    if (terminalStatuses.includes(progress.status)) compassRunning = false;
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
      listen<{ path: string }>("idm-download-name-applied", (event) => {
        idmPreparedKey = "";
        message = `IDM 文件名已自动填写：${event.payload.path}`;
      }),
      listen("idm-download-name-timeout", () => {
        idmPreparedKey = "";
        message = "两分钟内未检测到 IDM 下载窗口；标准文件名已复制，可手动粘贴。";
      }),
    ]);
  });
  onDestroy(() => unlistens.forEach((unlisten) => unlisten()));

  $: if (initialSessionId != null && initialSessionId !== appliedSessionId) {
    appliedSessionId = initialSessionId;
    void refresh(initialSessionId);
  }
</script>

<PageShell title="直播数据大屏" subtitle="数据来自官方“整场数据下载” XLSX；未提供字段不做推算。">
  <div slot="actions">
    <button type="button" class="mac-btn" on:click={chooseDownloadDir} title="设置下载目录"><FolderOpen size={16} />下载目录</button>
    <button type="button" class="mac-btn" on:click={importWorkbook}><Upload size={16} />导入 XLSX</button>
    <button type="button" class="mac-btn mac-btn-primary" on:click={() => refresh()} disabled={loading}><RefreshCw size={16} />刷新</button>
  </div>

  <section class="compass-panel" aria-label="抖音罗盘自动下载">
    <div class="compass-copy">
      <div class="compass-icon"><CalendarDays size={20} /></div>
      <div><h2>自动获取抖音罗盘数据</h2><p>选择日期后，一键下载并导入当天全部直播场次；首次使用需要扫码登录。</p></div>
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
    </div>
    {#if compassQueue.length}
      <div class="compass-queue">
        {#each compassQueue as item (item.sessionKey)}
          <article class:success={item.status === "imported" || item.status === "skipped"} class:error={item.status === "failed"}>
            <div><strong>{new Date(item.session.startedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}–{new Date(item.session.endedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}</strong><span>{item.session.title || item.session.shopName}</span></div>
            <div class="session-metrics"><span>{item.session.orderCount ?? "—"} 单</span><span>{item.session.paymentAmountText || "—"}</span></div>
            <div class="session-status"><strong>{compassStatusLabel(item.status)}</strong><span>{item.message}</span></div>
            <button type="button" class="mac-btn idm-name-btn" on:click={() => prepareIdmVideoDownload(item.session)}>
              <Clipboard size={14} />
              {idmPreparedKey === `${item.session.shopName}|${item.session.startedAt}` ? "等待 IDM…" : "准备视频下载"}
            </button>
          </article>
        {/each}
      </div>
    {/if}
  </section>
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
      <div class="idm-picker-action">
        <span>视频下载</span>
        <button type="button" class="mac-btn" disabled={!detail?.session} on:click={() => detail?.session && prepareIdmVideoDownload(detail.session)}>
          <Clipboard size={14} />准备 IDM 文件名
        </button>
      </div>
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
  .compass-queue { display: grid; gap: 8px; margin-top: 14px; }
  .compass-queue article {
    display: grid;
    grid-template-columns: minmax(220px, 1.4fr) minmax(130px, 0.6fr) minmax(220px, 1fr) auto;
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
  .idm-name-btn { white-space: nowrap; }
  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .session-picker {
    display: grid; grid-template-columns: minmax(220px, 1.15fr) minmax(170px, 0.7fr) minmax(280px, 1fr) auto;
    gap: 12px; padding: 14px; border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-md); background: var(--mac-bg-card);
  }
  .session-picker label { display: grid; gap: 6px; min-width: 0; }
  .session-picker label > span { color: var(--mac-tertiary); font-size: 12px; font-weight: 600; }
  .session-picker select { width: 100%; min-width: 0; height: 36px; }
  .idm-picker-action { display: grid; gap: 6px; align-content: end; }
  .idm-picker-action > span { color: var(--mac-tertiary); font-size: 12px; font-weight: 600; }
  .idm-picker-action button { height: 36px; white-space: nowrap; }
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
    .session-picker { grid-template-columns: 1fr; }
  }
  :global(.dark) .compass-panel {
    background: color-mix(in srgb, var(--mac-blue) 8%, var(--mac-bg-card));
    border-color: color-mix(in srgb, var(--mac-blue) 28%, var(--mac-separator));
  }
  :global(.dark) .compass-queue article.success { background: #163528; border-color: #24553a; }
  :global(.dark) .compass-queue article.error { background: #3a2320; border-color: #6b3530; }
</style>
