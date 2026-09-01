<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { invoke } from "../invoker";
  import type { StartupReadiness } from "../interface";

  export let readiness: StartupReadiness;
  export let refreshing = false;

  const dispatch = createEventDispatcher<{
    navigate: { page: string };
    refresh: void;
    completed: void;
    close: void;
  }>();
  let saving = false;
  let error = "";

  async function toggleAutostart(): Promise<void> {
    if (saving) return;
    saving = true;
    error = "";
    try {
      await invoke("update_autostart_enabled", { enabled: !readiness.autostartEnabled });
      dispatch("refresh");
    } catch (cause: any) {
      error = cause?.message || String(cause);
    } finally {
      saving = false;
    }
  }

  async function complete(): Promise<void> {
    if (!readiness.readyForRecording || saving) return;
    saving = true;
    error = "";
    try {
      await invoke("complete_startup_wizard");
      dispatch("completed");
    } catch (cause: any) {
      error = cause?.message || String(cause);
    } finally {
      saving = false;
    }
  }

  const statusLabel = (ok: boolean) => ok ? "已完成" : "待处理";
</script>

<div class="wizard-backdrop" role="presentation">
  <section class="wizard-card" role="dialog" aria-modal="true" aria-labelledby="startup-title">
    <header>
      <div class="wizard-icon">✓</div>
      <div>
        <p class="eyebrow">首次启动检查</p>
        <h2 id="startup-title">让录播在后台稳定运行</h2>
        <p>先完成录制必需项。关闭窗口后程序仍留在托盘，Windows 登录后会自动启动。</p>
      </div>
    </header>

    <div class="checks">
      <div class:ok={readiness.accountCount > 0} class="check-row">
        <span class="dot"></span>
        <div><strong>主播抖音账号</strong><small>{readiness.accountCount > 0 ? `已绑定 ${readiness.accountCount} 个账号` : "需要扫码绑定主播账号"}</small></div>
        <span class="state">{statusLabel(readiness.accountCount > 0)}</span>
        {#if readiness.accountCount === 0}<button on:click={() => dispatch("navigate", { page: "账号" })}>去绑定</button>{/if}
      </div>
      <div class:ok={readiness.recorderCount > 0} class="check-row">
        <span class="dot"></span>
        <div><strong>监控直播间</strong><small>{readiness.recorderCount > 0 ? `已监控 ${readiness.recorderCount} 个直播间` : "至少添加一个要自动录制的直播间"}</small></div>
        <span class="state">{statusLabel(readiness.recorderCount > 0)}</span>
        {#if readiness.recorderCount === 0}<button on:click={() => dispatch("navigate", { page: "直播间" })}>去添加</button>{/if}
      </div>
      <div class:ok={readiness.autostartEnabled} class="check-row">
        <span class="dot"></span>
        <div><strong>Windows 开机自启</strong><small>开机后自动进入托盘，不要求打开页面</small></div>
        <span class="state">{statusLabel(readiness.autostartEnabled)}</span>
        <button disabled={saving} on:click={() => void toggleAutostart()}>{readiness.autostartEnabled ? "关闭" : "开启"}</button>
      </div>
      <div class:ok={readiness.ffmpegOk && readiness.cacheOk && readiness.outputOk} class="check-row">
        <span class="dot"></span>
        <div><strong>录制与切片环境</strong><small>{readiness.ffmpegOk && readiness.cacheOk && readiness.outputOk ? "FFmpeg、录制目录和切片目录均可写" : (readiness.cacheDetail || readiness.outputDetail || readiness.ffmpegDetail)}</small></div>
        <span class="state">{statusLabel(readiness.ffmpegOk && readiness.cacheOk && readiness.outputOk)}</span>
        {#if !readiness.ffmpegOk || !readiness.cacheOk || !readiness.outputOk}<button on:click={() => dispatch("navigate", { page: "设置" })}>去检查</button>{/if}
      </div>
      <div class:ok={readiness.funasrOk} class="check-row optional">
        <span class="dot"></span>
        <div><strong>本地转文稿</strong><small>{readiness.funasrOk ? "FunASR 运行环境已找到" : "不影响录制；下播后暂时不能自动转文稿"}</small></div>
        <span class="state">{statusLabel(readiness.funasrOk)}</span>
      </div>
      <div class:ok={readiness.nasConfigured} class="check-row optional">
        <span class="dot"></span>
        <div><strong>NAS 存储</strong><small>{readiness.nasConfigured ? "NAS 目录可访问" : "可先录到本机，稍后在设置中连接 NAS"}</small></div>
        <span class="state">{readiness.nasConfigured ? "已连接" : "可稍后"}</span>
        {#if !readiness.nasConfigured}<button on:click={() => dispatch("navigate", { page: "设置" })}>去设置</button>{/if}
      </div>
    </div>

    {#if error}<p class="error">{error}</p>{/if}
    <footer>
      <button class="secondary" disabled={refreshing} on:click={() => dispatch("refresh")}>{refreshing ? "正在检查…" : "重新检查"}</button>
      <button class="secondary" on:click={() => dispatch("close")}>稍后继续</button>
      <button class="primary" disabled={!readiness.readyForRecording || saving} on:click={() => void complete()}>完成基础配置</button>
    </footer>
    {#if !readiness.readyForRecording}<p class="hint">完成账号、直播间、开机自启和录制环境后即可进入系统。</p>{/if}
  </section>
</div>

<style>
  .wizard-backdrop{position:fixed;inset:0;z-index:10020;display:grid;place-items:center;padding:20px;background:rgba(15,23,42,.5);backdrop-filter:blur(12px)}
  .wizard-card{width:min(760px,calc(100vw - 32px));max-height:calc(100vh - 40px);overflow:auto;padding:26px;border:1px solid rgba(255,255,255,.75);border-radius:24px;background:#fff;box-shadow:0 28px 90px rgba(15,23,42,.3);color:#172033}
  header{display:flex;gap:16px;align-items:flex-start} .wizard-icon{width:48px;height:48px;display:grid;place-items:center;flex:0 0 auto;border-radius:15px;background:linear-gradient(135deg,#1677ff,#22c55e);color:#fff;font-size:24px;font-weight:800}
  .eyebrow{margin:0 0 4px;color:#1677ff;font-size:12px;font-weight:800} h2{margin:0;font-size:24px} header p:last-child{margin:8px 0 0;color:#667085;font-size:13px}
  .checks{display:grid;gap:9px;margin:22px 0 16px}.check-row{display:grid;grid-template-columns:12px minmax(0,1fr) auto auto;gap:11px;align-items:center;padding:13px 14px;border:1px solid #e4e7ec;border-radius:13px;background:#f8fafc}.check-row.ok{border-color:#b7ebc6;background:#f0fdf4}.check-row.optional{opacity:.92}.dot{width:10px;height:10px;border-radius:50%;background:#f59e0b}.ok .dot{background:#22c55e}.check-row strong,.check-row small{display:block}.check-row strong{font-size:14px}.check-row small{margin-top:3px;color:#667085;font-size:12px}.state{color:#667085;font-size:12px;font-weight:700}.check-row.ok .state{color:#15803d}.check-row button,footer button{border-radius:9px;padding:8px 12px;font:inherit;font-size:12px;font-weight:700;cursor:pointer}.check-row button{border:1px solid #b9c7dc;background:white;color:#1677ff}
  footer{display:flex;justify-content:flex-end;gap:10px}.secondary{border:1px solid #d0d5dd;background:white;color:#475467}.primary{border:0;background:#1677ff;color:#fff}.primary:disabled,button:disabled{cursor:not-allowed;opacity:.5}.error,.hint{margin:10px 0;padding:10px 12px;border-radius:9px;font-size:12px}.error{background:#fff1f0;color:#c62828}.hint{background:#eff6ff;color:#245b9e}
  :global(.dark) .wizard-card{border-color:rgba(255,255,255,.1);background:#182033;color:#f8fafc}:global(.dark) .check-row{border-color:#344054;background:#101828}:global(.dark) .check-row.ok{border-color:#236b43;background:#10261c}:global(.dark) .check-row button,:global(.dark) .secondary{border-color:#475467;background:#101828;color:#dbeafe}
  @media(max-width:680px){.check-row{grid-template-columns:12px minmax(0,1fr) auto}.check-row button{grid-column:2/4;justify-self:start}footer{flex-wrap:wrap}.wizard-card{padding:20px}}
</style>
