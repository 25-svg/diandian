<script lang="ts">
  import ActivationGate from "./lib/components/ActivationGate.svelte";
  import Room from "./page/Room.svelte";
  import BSidebar from "./lib/components/BSidebar.svelte";
  import Summary from "./page/Summary.svelte";
  import Setting from "./page/Setting.svelte";
  import Account from "./page/Account.svelte";
  import About from "./page/About.svelte";
  import { log, onOpenUrl, set_title } from "./lib/invoker";
  import Clip from "./page/Clip.svelte";
  import Task from "./page/Task.svelte";
  import ScenarioTraining from "./page/ScenarioTraining.svelte";
  import StreamerCoach from "./page/StreamerCoach.svelte";
  import CameraKnowledgeQuiz from "./page/CameraKnowledgeQuiz.svelte";
  import AnchorKnowledge from "./page/AnchorKnowledge.svelte";
  import Archive from "./page/Archive.svelte";
  import ArchiveAnalysis from "./page/ArchiveAnalysis.svelte";
  import MasterSourceDialog from "./lib/components/master/MasterSourceDialog.svelte";
  import StartupWizard from "./lib/components/StartupWizard.svelte";
  import type { RecordItem } from "./lib/db";
  import { isClipVideo, type StartupReadiness, type VideoItem } from "./lib/interface";
  import { findArchiveSourceVideo } from "./lib/archiveVideoBinding";
  import { watchPrivateUpdateStatus, type PrivateUpdateStatus } from "./lib/appUpdater";
  import { buildExistingClipReviewRequest, type ClipReviewRequest } from "./lib/clipReview";
  import { getMasterBaseline, listMasterSampleBatches } from "./lib/masterScript";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  let active = "总览";
  let analysisArchive: RecordItem | null = null;
  let analysisVideo: VideoItem | null = null;
  let analysisRefreshToken = 0;
  let analysisMode: "legacy" | "company_deal" = "legacy";
  let analysisFocusTab: "" | "clip_review" = "";
  let analysisInitialSeekSeconds: number | null = null;
  let analysisNavigationLock = false;
  let masterSourceVideo: VideoItem | null = null;
  let clipReviewRequest: ClipReviewRequest | null = null;
  let showMiniMaxSetup = false;
  let miniMaxApiKey = "";
  let miniMaxSetupError = "";
  let miniMaxSetupSaving = false;
  let miniMaxSetupComplete = false;
  let startupReadiness: StartupReadiness | null = null;
  let showStartupWizard = false;
  let startupReadinessLoading = false;
  let privateUpdateStatus: PrivateUpdateStatus | null = null;
  let stopUpdateStatusPolling: () => void = () => undefined;

  async function loadStartupReadiness(showWhenIncomplete = true): Promise<void> {
    startupReadinessLoading = true;
    try {
      startupReadiness = await invoke<StartupReadiness>("get_startup_readiness");
      if (showWhenIncomplete) {
        showStartupWizard = !startupReadiness.wizardCompleted;
      }
    } catch (error: any) {
      console.warn("Failed to inspect startup readiness", error);
    } finally {
      startupReadinessLoading = false;
    }
  }

  async function checkMiniMaxSetup(): Promise<void> {
    try {
      const status = await invoke<{ configured: boolean }>("get_minimax_setup_status");
      showMiniMaxSetup = !status.configured;
    } catch (error: any) {
      console.warn("Failed to inspect MiniMax setup status", error);
    }
  }

  async function initializeMiniMax(): Promise<void> {
    if (!miniMaxApiKey.trim() || miniMaxSetupSaving) return;
    miniMaxSetupSaving = true;
    miniMaxSetupError = "";
    miniMaxSetupComplete = false;
    try {
      await invoke("initialize_minimax_api_key", { apiKey: miniMaxApiKey.trim() });
      miniMaxApiKey = "";
      miniMaxSetupComplete = true;
      setTimeout(() => {
        showMiniMaxSetup = false;
        miniMaxSetupComplete = false;
      }, 900);
    } catch (error: any) {
      miniMaxSetupError = error?.message || String(error);
    } finally {
      miniMaxSetupSaving = false;
    }
  }

  function resolveVideoAnalysisDetail(detail: unknown): {
    video: VideoItem | null;
    mode: "legacy" | "company_deal";
    focusTab: "" | "clip_review";
    seekSeconds: number | null;
  } {
    if (!detail || typeof detail !== "object") {
      return { video: null, mode: "legacy", focusTab: "", seekSeconds: null };
    }
    const record = detail as Record<string, unknown>;
    const nested = record.video;
    const focusTab = record.focusTab === "clip_review" ? "clip_review" : "";
    const seekSeconds = typeof record.seekSeconds === "number" ? record.seekSeconds : null;
    if (nested && typeof nested === "object" && nested !== null && "id" in nested) {
      return {
        video: nested as VideoItem,
        mode: record.analysisMode === "company_deal" ? "company_deal" : "legacy",
        focusTab,
        seekSeconds,
      };
    }
    if ("id" in record) {
      return { video: detail as VideoItem, mode: "legacy", focusTab, seekSeconds };
    }
    return { video: null, mode: "legacy", focusTab: "", seekSeconds: null };
  }

  function openAnalysisPage(options: {
    archive?: RecordItem | null;
    video?: VideoItem | null;
    mode?: "legacy" | "company_deal";
    focusTab?: "" | "clip_review";
    seekSeconds?: number | null;
  }): void {
    const archive = options.archive ?? null;
    const video = options.video ?? null;
    if (!archive && !video) {
      alert("无法打开分析页：未找到对应录播或视频。");
      return;
    }
    analysisNavigationLock = true;
    analysisArchive = archive;
    analysisVideo = video;
    analysisMode = options.mode ?? "legacy";
    analysisFocusTab = options.focusTab ?? "";
    analysisInitialSeekSeconds = options.seekSeconds ?? null;
    analysisRefreshToken += 1;
    active = "录播分析";
    queueMicrotask(() => {
      analysisNavigationLock = false;
    });
  }

  async function ensureActiveEnterpriseMaster(): Promise<void> {
    try {
      const current = JSON.parse(localStorage.getItem("bsr:active-master") || "{}") as { scriptKey?: string };
      if (current.scriptKey) return;

      const batches = (await listMasterSampleBatches())
        .filter(({ batch }) => batch.purpose === "enterprise" && batch.status === "published")
        .sort((left, right) => right.batch.id - left.batch.id);

      for (const { batch } of batches) {
        const scriptKey = `MS-BATCH-${batch.id}`;
        try {
          const baseline = await getMasterBaseline(scriptKey);
          localStorage.setItem("bsr:active-master", JSON.stringify({
            scriptKey: baseline.master.scriptKey,
            masterScriptId: baseline.master.id,
          }));
          return;
        } catch {
          // Try the next published enterprise batch if this one has no usable baseline.
        }
      }
    } catch {
      // First launch remains usable even when no enterprise master has been published.
    }
  }

  function startAuthorizedApp() {
    void set_title("典典直播切片");
    void ensureActiveEnterpriseMaster();
    void checkMiniMaxSetup();
    void loadStartupReadiness();
    stopUpdateStatusPolling();
    stopUpdateStatusPolling = watchPrivateUpdateStatus((status) => {
      privateUpdateStatus = status;
    });
  }
  onMount(() => () => stopUpdateStatusPolling());
  onMount(() => {
    const openMiniMaxSetup = () => {
      miniMaxSetupError = "";
      miniMaxSetupComplete = false;
      showMiniMaxSetup = true;
    };
    window.addEventListener("bsr:open-minimax-setup", openMiniMaxSetup);
    return () => window.removeEventListener("bsr:open-minimax-setup", openMiniMaxSetup);
  });
  onMount(() => {
    const navigateFromShell = (event: Event) => {
      const page = String((event as CustomEvent<unknown>).detail || "");
      if (!page) return;
      if (page !== "录播分析") {
        analysisArchive = null;
        analysisVideo = null;
      }
      active = page;
    };
    window.addEventListener("bsr:navigate", navigateFromShell);
    return () => window.removeEventListener("bsr:navigate", navigateFromShell);
  });
  onMount(async () => {
    await onOpenUrl((urls: string[]) => {
      console.log("Received Deep Link:", urls);
      if (urls.length > 0) {
        const url = urls[0];
        // extract platform and room_id from url
        // url example:
        // bsr://live.bilibili.com/167537?live_from=85001&spm_id_from=333.1365.live_users.item.click
        // bsr://live.douyin.com/200525029536

        let platform = "";
        let room_id = "";

        if (url.startsWith("bsr://live.bilibili.com/")) {
          // 1. remove bsr://live.bilibili.com/
          // 2. remove all query params
          room_id = url.replace("bsr://live.bilibili.com/", "").split("?")[0];
          platform = "bilibili";
        }

        if (url.startsWith("bsr://live.douyin.com/")) {
          room_id = url.replace("bsr://live.douyin.com/", "").split("?")[0];
          platform = "douyin";
        }

        if (url.startsWith("bsr://live.kuaishou.com/")) {
          room_id = url.replace("bsr://live.kuaishou.com/", "").split("?")[0];
          room_id = room_id.replace(/^u\//, "");
          platform = "kuaishou";
        }

        if (url.startsWith("bsr://live.tiktok.com/")) {
          room_id = url.replace("bsr://live.tiktok.com/", "").split("?")[0];
          platform = "tiktok";
        }

        if (platform && room_id) {
          // switch to room page
          active = "直播间";
        }
      }
    });
  });

  onMount(() => {
    const openArchiveAnalysis = (event: Event) => {
      openAnalysisPage({
        archive: (event as CustomEvent<RecordItem>).detail,
        video: null,
        mode: "legacy",
      });
    };
    const openVideoAnalysis = (event: Event) => {
      const { video, mode, focusTab, seekSeconds } = resolveVideoAnalysisDetail(
        (event as CustomEvent<unknown>).detail,
      );
      // Clips belong in the three-column review workspace. Opening them in
      // ArchiveAnalysis treats a short MP4 as a full session and queues remux/ASR.
      if (video && isClipVideo(video)) {
        analysisArchive = null;
        analysisVideo = null;
        clipReviewRequest = buildExistingClipReviewRequest(video, "");
        active = "切片";
        return;
      }
      openAnalysisPage({ archive: null, video, mode, focusTab, seekSeconds });
    };
    const openCompanyDealReview = async (event: Event) => {
      const detail = (event as CustomEvent<RecordItem | { archive: RecordItem; seekSeconds?: number }>).detail;
      const archive = detail && typeof detail === "object" && "archive" in detail
        ? detail.archive
        : detail as RecordItem;
      const seekSeconds = detail && typeof detail === "object" && "archive" in detail && typeof detail.seekSeconds === "number"
        ? detail.seekSeconds
        : null;
      if (!archive) {
        alert("无法打开分析页：录播数据为空。");
        return;
      }
      if (archive.archive_kind !== "company") {
        alert("只有公司录播才能打开成交订单时间轴分析。");
        return;
      }
      try {
        const videos = await invoke<VideoItem[]>("get_all_videos");
        const sourceVideo = findArchiveSourceVideo(archive, videos);
        if (sourceVideo) {
          openAnalysisPage({
            archive,
            video: sourceVideo,
            mode: "company_deal",
            seekSeconds,
          });
          return;
        }
      } catch (error) {
        console.warn("Unable to resolve archive source video", error);
      }
      openAnalysisPage({
        archive,
        video: null,
        mode: "company_deal",
        seekSeconds,
      });
    };
    const openMasterBuilder = (_event: Event) => {
      alert("已废弃「整场进母稿」。请从成交话术精炼后，经 Clip「母稿样本批次」发布到「主播知识库」的 视频/成交、话术、分析建议。");
    };
    const openClipReview = (event: Event) => {
      clipReviewRequest = (event as CustomEvent<ClipReviewRequest>).detail;
      active = "切片";
    };
    window.addEventListener("bsr:open-archive-analysis", openArchiveAnalysis);
    window.addEventListener("bsr:open-company-deal-review", openCompanyDealReview);
    window.addEventListener("bsr:open-video-analysis", openVideoAnalysis);
    window.addEventListener("bsr:build-master", openMasterBuilder);
    window.addEventListener("bsr:open-clip-review", openClipReview);
    return () => {
      window.removeEventListener("bsr:open-archive-analysis", openArchiveAnalysis);
      window.removeEventListener("bsr:open-company-deal-review", openCompanyDealReview);
      window.removeEventListener("bsr:open-video-analysis", openVideoAnalysis);
      window.removeEventListener("bsr:build-master", openMasterBuilder);
      window.removeEventListener("bsr:open-clip-review", openClipReview);
    };
  });

  log.info("App loaded");
</script>

<ActivationGate on:authorized={startAuthorizedApp}>
<main>
  {#if privateUpdateStatus && ["downloading", "waiting_for_idle", "installing", "failed"].includes(privateUpdateStatus.status)}
    <div class:failed={privateUpdateStatus.status === "failed"} class="private-update-status" role="status" aria-live="polite">
      {privateUpdateStatus.message}
    </div>
  {/if}
  <div class="wrap">
    <div class="sidebar">
      <BSidebar
        bind:activeUrl={active}
        on:activeChange={(e) => {
          // Leaving analysis via sidebar should clear stale source so it does not
          // reopen an empty analysis page on the next click.
          if (e.detail !== "录播分析") {
            analysisArchive = null;
            analysisVideo = null;
          }
          active = e.detail;
        }}
      />
    </div>
    <div class="content">
      <div class="page" class:visible={active == "总览"}>
        <Summary />
      </div>
      <div class="page" class:visible={active == "直播间"}>
        <Room />
      </div>
      <div class="page" class:visible={active == "录播"}>
        <Archive />
      </div>
      <div class="page" class:visible={active == "录播分析"}>
        <ArchiveAnalysis
          archive={analysisArchive}
          video={analysisVideo}
          refreshToken={analysisRefreshToken}
          analysisMode={analysisMode}
          focusTab={analysisFocusTab}
          initialSeekSeconds={analysisInitialSeekSeconds}
          on:back={() => {
            active = analysisVideo ? "切片" : "录播";
          }}
        />
      </div>
      <div class="page" class:visible={active == "切片"}>
        <Clip
          reviewRequest={clipReviewRequest}
          on:closeReview={() => { clipReviewRequest = null; }}
        />
      </div>
      <div class="page" class:visible={active == "任务"}>
        <Task />
      </div>
      <div class="page" class:visible={active == "情景训练"}>
        <ScenarioTraining />
      </div>
      <div class="page" class:visible={active == "主播教练"}>
        <StreamerCoach on:navigate={(event) => active = event.detail.page} />
      </div>
      <div class="page" class:visible={active == "相机知识问答"}>
        <CameraKnowledgeQuiz />
      </div>
      <div class="page" class:visible={active == "主播知识库"}>
        <AnchorKnowledge />
      </div>
      <div class="page" class:visible={active == "账号"}>
        <Account />
      </div>
      <div class="page" class:visible={active == "设置"}>
        <Setting />
      </div>
      <div class="page" class:visible={active == "关于"}>
        <About />
      </div>
    </div>
  </div>
</main>

{#if masterSourceVideo}
  <MasterSourceDialog
    videoId={masterSourceVideo.id}
    videoTitle={masterSourceVideo.title || masterSourceVideo.file || "整场直播母稿"}
    on:close={() => masterSourceVideo = null}
    on:published={(event) => {
      localStorage.setItem("bsr:active-master", JSON.stringify(event.detail));
      masterSourceVideo = null;
    }}
  />
{/if}

{#if showStartupWizard && startupReadiness}
  <StartupWizard
    readiness={startupReadiness}
    refreshing={startupReadinessLoading}
    on:refresh={() => void loadStartupReadiness(false)}
    on:navigate={(event) => {
      active = event.detail.page;
      showStartupWizard = false;
    }}
    on:close={() => showStartupWizard = false}
    on:completed={() => {
      showStartupWizard = false;
      startupReadiness = startupReadiness ? { ...startupReadiness, wizardCompleted: true } : null;
    }}
  />
{:else if showMiniMaxSetup}
  <div class="minimax-setup-backdrop" role="presentation">
    <section class="minimax-setup-card" role="dialog" aria-modal="true" aria-labelledby="minimax-setup-title">
      <div class="minimax-setup-icon">AI</div>
      <div>
        <p class="minimax-setup-eyebrow">首次使用</p>
        <h2 id="minimax-setup-title">配置成交话术 AI 分析</h2>
        <p class="minimax-setup-copy">转文稿继续使用本地 FunASR。MiniMax 仅用于成交链路分析和自动切片，密钥将使用当前 Windows 用户加密保存。</p>
      </div>
      <label for="minimax-first-key">MiniMax API Key</label>
      <input
        id="minimax-first-key"
        type="password"
        autocomplete="off"
        placeholder="粘贴完整 MiniMax API Key"
        bind:value={miniMaxApiKey}
        on:keydown={(event) => {
          if (event.key === "Enter") void initializeMiniMax();
        }}
      />
      {#if miniMaxSetupError}
        <p class="minimax-setup-error">{miniMaxSetupError}</p>
      {/if}
      {#if miniMaxSetupComplete}
        <p class="minimax-setup-success">连接成功，安全配置已保存。</p>
      {/if}
      <div class="minimax-setup-actions">
        <button class="minimax-skip" type="button" disabled={miniMaxSetupSaving} on:click={() => showMiniMaxSetup = false}>暂时跳过</button>
        <button class="minimax-save" type="button" disabled={miniMaxSetupSaving || miniMaxApiKey.trim().length < 12} on:click={() => void initializeMiniMax()}>
          {miniMaxSetupSaving ? "正在测试连接…" : "测试并保存"}
        </button>
      </div>
      <p class="minimax-setup-hint">未配置时仍可录制、播放和转文稿；使用 AI 分析时系统会再次提示。</p>
    </section>
  </div>
{/if}

</ActivationGate>

<style>
  .sidebar {
    display: flex;
    height: 100vh;
    flex: 0 0 224px;
    min-width: 0;
  }

  .wrap {
    display: flex;
    flex-direction: row;
    height: 100vh;
    overflow: hidden;
    background:
      radial-gradient(circle at 82% -10%, rgba(164, 210, 255, 0.32), transparent 34%),
      linear-gradient(135deg, var(--mac-bg-window) 0%, var(--mac-bg) 100%);
  }

  .visible {
    opacity: 1 !important;
    height: 100% !important;
    transform: translateX(0) !important;
    min-height: 0;
    pointer-events: auto !important;
  }

  .page {
    opacity: 0;
    height: 0;
    transform: translateX(100%);
    overflow: hidden;
    pointer-events: none;
    transition:
      opacity 0.5s ease-in-out,
      transform 0.3s ease-in-out;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .page {
      transform: none;
      transition: none;
    }
  }

  .page.visible :global(> *) {
    flex: 1 1 auto;
    min-height: 0;
    width: 100%;
  }

  .content {
    flex: 1 1 0;
    width: auto;
    min-width: 0;
    height: calc(100vh - 20px);
    margin: 10px 10px 10px 0;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.82);
    border-radius: var(--mac-radius-xl);
    background: var(--mac-bg-elevated);
    box-shadow: var(--mac-shadow-lg);
    backdrop-filter: blur(24px) saturate(150%);
  }

  @media (max-width: 700px) {
    .wrap {
      flex-direction: column;
    }
    .sidebar {
      width: 100%;
      height: auto;
      flex: 0 0 auto;
    }
    .content {
      align-self: stretch;
      width: auto;
      height: auto;
      min-height: 0;
      margin: 0 6px 6px;
    }
  }

  :global(.dark) .wrap {
    background:
      radial-gradient(circle at 82% -10%, rgba(28, 91, 148, 0.28), transparent 34%),
      var(--mac-bg);
  }
  :global(.dark) .content {
    border-color: rgba(255, 255, 255, 0.08);
    background: var(--mac-bg-elevated);
    box-shadow: var(--mac-shadow-lg);
  }

  .minimax-setup-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10000;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(15, 23, 42, 0.48);
    backdrop-filter: blur(12px);
  }
  .private-update-status {
    position: fixed;
    z-index: 9000;
    top: 16px;
    right: 18px;
    max-width: min(420px, calc(100vw - 36px));
    padding: 10px 14px;
    border: 1px solid rgba(37, 99, 235, 0.22);
    border-radius: 10px;
    background: rgba(239, 246, 255, 0.96);
    color: #1e3a8a;
    box-shadow: 0 10px 28px rgba(15, 23, 42, 0.16);
    font-size: 13px;
  }
  .private-update-status.failed { border-color: rgba(185, 28, 28, 0.24); background: rgba(254, 242, 242, 0.96); color: #991b1b; }
  .minimax-setup-card {
    width: min(520px, calc(100vw - 32px));
    display: grid;
    gap: 14px;
    padding: 28px;
    border: 1px solid rgba(255, 255, 255, 0.8);
    border-radius: 22px;
    background: rgba(255, 255, 255, 0.97);
    box-shadow: 0 28px 90px rgba(15, 23, 42, 0.28);
    color: #172033;
  }
  .minimax-setup-icon {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 14px;
    background: linear-gradient(135deg, #1677ff, #6d5dfc);
    color: white;
    font-weight: 800;
  }
  .minimax-setup-eyebrow { margin: 0 0 4px; color: #1677ff; font-size: 12px; font-weight: 700; }
  .minimax-setup-card h2 { margin: 0; font-size: 22px; }
  .minimax-setup-copy, .minimax-setup-hint { margin: 8px 0 0; color: #667085; font-size: 13px; line-height: 1.6; }
  .minimax-setup-card label { font-size: 13px; font-weight: 700; }
  .minimax-setup-card input {
    width: 100%;
    box-sizing: border-box;
    padding: 12px 14px;
    border: 1px solid #d0d5dd;
    border-radius: 11px;
    outline: none;
    font: inherit;
  }
  .minimax-setup-card input:focus { border-color: #1677ff; box-shadow: 0 0 0 3px rgba(22, 119, 255, 0.12); }
  .minimax-setup-error, .minimax-setup-success { margin: 0; padding: 10px 12px; border-radius: 9px; font-size: 13px; }
  .minimax-setup-error { background: #fff1f0; color: #c62828; }
  .minimax-setup-success { background: #ecfdf3; color: #027a48; }
  .minimax-setup-actions { display: flex; justify-content: flex-end; gap: 10px; }
  .minimax-setup-actions button { padding: 10px 16px; border-radius: 10px; font: inherit; font-weight: 700; cursor: pointer; }
  .minimax-skip { border: 1px solid #d0d5dd; background: white; color: #475467; }
  .minimax-save { border: 0; background: #1677ff; color: white; }
  .minimax-setup-actions button:disabled { cursor: not-allowed; opacity: 0.5; }
  :global(.dark) .minimax-setup-card { border-color: rgba(255,255,255,.1); background: #182033; color: #f8fafc; }
  :global(.dark) .minimax-setup-card input { border-color: #475467; background: #101828; color: #f8fafc; }
  :global(.dark) .minimax-skip { border-color: #475467; background: #101828; color: #e4e7ec; }
</style>
