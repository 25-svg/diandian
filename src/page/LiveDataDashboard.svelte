<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { FolderOpen, RefreshCw, Upload } from "lucide-svelte";
  import { invoke } from "../lib/invoker";
  import { dashboardMetricCards } from "../lib/liveDashboard";

  type Session = {
    id: number; shopName: string; startedAt: string; paymentAmountFen: number;
    perThousandPaymentAmountFen?: number | null; viewerCount?: number | null;
    averageOnline?: number | null; averageWatchSeconds?: number | null;
    viewerConversionRate?: number | null; sourceFile: string; importedAt: string;
  };
  type Detail = { session: Session; channels: Array<{ name: string; viewerCount?: number; paymentAmountFen?: number; orderCount?: number }>; shortVideos: Array<{ title: string; viewerCount?: number; paymentAmountFen?: number; orderCount?: number }>; products: Array<{ productId: string; name: string; paymentAmountFen?: number; soldCount?: number; buyerCount?: number }> };

  let sessions: Session[] = [];
  let detail: Detail | null = null;
  let loading = true;
  let message = "";
  let downloadDir = "";
  let unlistens: Array<() => void> = [];

  const money = (fen?: number | null) => fen == null ? "— / 官方导出未提供" : `¥${(fen / 100).toLocaleString("zh-CN", { maximumFractionDigits: 2 })}`;

  async function refresh(sessionId?: number) {
    loading = true;
    try {
      sessions = await invoke<Session[]>("list_live_dashboard_sessions");
      const selected = sessionId ?? detail?.session.id ?? sessions[0]?.id;
      detail = selected ? await invoke<Detail>("get_live_dashboard_detail", { sessionId: selected }) : null;
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

  onMount(async () => {
    await refresh();
    unlistens = await Promise.all([
      listen("live-dashboard-imported", () => { message = "检测到新下载，已自动导入。"; void refresh(); }),
      listen<{ path: string; message: string }>("live-dashboard-import-failed", (event) => { message = `自动导入失败：${event.payload.message}。请确认文件为官方整场数据下载 XLSX。`; }),
      listen<{ message: string }>("live-dashboard-watch-error", (event) => { message = event.payload.message; }),
    ]);
  });
  onDestroy(() => unlistens.forEach((unlisten) => unlisten()));
</script>

<section class="dashboard">
  <header>
    <div><h1>直播数据大屏</h1><p>数据来自官方“整场数据下载” XLSX；未提供字段不做推算。</p></div>
    <div class="actions"><button on:click={chooseDownloadDir} title="设置下载目录"><FolderOpen size={16} />下载目录</button><button on:click={importWorkbook}><Upload size={16} />导入 XLSX</button><button on:click={() => refresh()} disabled={loading}><RefreshCw size={16} />刷新</button></div>
  </header>
  {#if message}<p class="message">{message}</p>{/if}
  {#if sessions.length}
    <select value={detail?.session.id} on:change={(event) => refresh(Number(event.currentTarget.value))}>
      {#each sessions as session}<option value={session.id}>{session.shopName} · {new Date(session.startedAt).toLocaleString("zh-CN")}</option>{/each}
    </select>
  {/if}
  {#if detail}
    <div class="cards">{#each dashboardMetricCards(detail.session) as card}<article><span>{card.label}</span><strong>{card.value}</strong></article>{/each}<article><span>直播间观看人数</span><strong>{detail.session.viewerCount ?? "— / 官方导出未提供"}</strong></article><article><span>平均在线人数</span><strong>{detail.session.averageOnline ?? "— / 官方导出未提供"}</strong></article><article><span>千次观看用户支付金额</span><strong>{money(detail.session.perThousandPaymentAmountFen)}</strong></article></div>
    <div class="meta">来源：{detail.session.sourceFile}　导入：{new Date(detail.session.importedAt).toLocaleString("zh-CN")}</div>
    <div class="tables"><article><h2>渠道分析</h2><table><thead><tr><th>渠道</th><th>观看人数</th><th>用户支付金额</th><th>订单</th></tr></thead><tbody>{#each detail.channels as channel}<tr><td>{channel.name}</td><td>{channel.viewerCount ?? "—"}</td><td>{money(channel.paymentAmountFen)}</td><td>{channel.orderCount ?? "—"}</td></tr>{/each}</tbody></table></article><article><h2>短视频引流</h2><table><thead><tr><th>短视频</th><th>观看人数</th><th>用户支付金额</th><th>订单</th></tr></thead><tbody>{#each detail.shortVideos as video}<tr><td>{video.title}</td><td>{video.viewerCount ?? "—"}</td><td>{money(video.paymentAmountFen)}</td><td>{video.orderCount ?? "—"}</td></tr>{/each}</tbody></table></article><article><h2>商品成交榜</h2><table><thead><tr><th>商品</th><th>支付金额</th><th>件数</th><th>人数</th></tr></thead><tbody>{#each detail.products as product}<tr><td title={product.productId}>{product.name}</td><td>{money(product.paymentAmountFen)}</td><td>{product.soldCount ?? "—"}</td><td>{product.buyerCount ?? "—"}</td></tr>{/each}</tbody></table></article></div>
  {:else if !loading}
    <div class="empty">暂无直播数据。请在罗盘下载官方 XLSX，软件会自动导入；也可点击“导入 XLSX”。</div>
  {/if}
</section>

<style>
  .dashboard{height:100%;overflow:auto;padding:24px;color:#1d1d1f}header{display:flex;justify-content:space-between;gap:16px;align-items:flex-start}h1,h2,p{margin:0}h1{font-size:24px}h2{font-size:15px}header p,.meta{margin-top:5px;color:#667085;font-size:12px}.actions{display:flex;gap:8px;flex-wrap:wrap}button,select{border:1px solid #d0d5dd;border-radius:7px;background:#fff;padding:8px 10px;color:#344054;cursor:pointer}button{display:inline-flex;align-items:center;gap:6px}.message{margin-top:12px;color:#175cd3;font-size:12px}select{margin-top:16px;min-width:320px}.cards{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px;margin-top:18px}.cards article,.tables article{border:1px solid #e5e7eb;border-radius:10px;background:#fff;padding:15px}.cards span{display:block;color:#667085;font-size:12px}.cards strong{display:block;margin-top:8px;color:#175cd3;font-size:22px}.meta{margin:14px 0}.tables{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px}table{width:100%;margin-top:10px;border-collapse:collapse;font-size:12px}th,td{padding:9px 7px;border-bottom:1px solid #eef0f2;text-align:left}th{color:#667085}td:first-child{max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.empty{margin-top:28px;padding:48px;text-align:center;border:1px dashed #cbd5e1;border-radius:10px;color:#667085}@media(max-width:1000px){.cards,.tables{grid-template-columns:1fr}}:global(.dark) .dashboard{color:#f5f5f7}:global(.dark) button,:global(.dark) select,:global(.dark) .cards article,:global(.dark) .tables article{background:#2c2d31;border-color:#46484f;color:#f5f5f7}:global(.dark) th,:global(.dark) td{border-color:#3f4249}
</style>
