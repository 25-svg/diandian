<script lang="ts">
  import Room from "./page/Room.svelte";
  import BSidebar from "./lib/components/BSidebar.svelte";
  import Summary from "./page/Summary.svelte";
  import Setting from "./page/Setting.svelte";
  import Account from "./page/Account.svelte";
  import About from "./page/About.svelte";
  import { log, onOpenUrl, set_title } from "./lib/invoker";
  import Clip from "./page/Clip.svelte";
  import Task from "./page/Task.svelte";
  import Archive from "./page/Archive.svelte";
  import ArchiveAnalysis from "./page/ArchiveAnalysis.svelte";
  import LiveDataDashboard from "./page/LiveDataDashboard.svelte";
  import MasterSourceDialog from "./lib/components/master/MasterSourceDialog.svelte";
  import type { RecordItem } from "./lib/db";
  import type { VideoItem } from "./lib/interface";
  import type { ClipReviewRequest } from "./lib/clipReview";
  import { getMasterBaseline, listMasterSampleBatches } from "./lib/masterScript";
  import { onMount } from "svelte";

  let active = "总览";
  let analysisArchive: RecordItem | null = null;
  let analysisVideo: VideoItem | null = null;
  let analysisRefreshToken = 0;
  let analysisMode: "legacy" | "company_deal" = "legacy";
  let analysisNavigationLock = false;
  let liveDashboardSessionId: number | null = null;
  let masterSourceVideo: VideoItem | null = null;
  let clipReviewRequest: ClipReviewRequest | null = null;

  function resolveVideoAnalysisDetail(detail: unknown): {
    video: VideoItem | null;
    mode: "legacy" | "company_deal";
  } {
    if (!detail || typeof detail !== "object") {
      return { video: null, mode: "legacy" };
    }
    const record = detail as Record<string, unknown>;
    const nested = record.video;
    if (nested && typeof nested === "object" && nested !== null && "id" in nested) {
      return {
        video: nested as VideoItem,
        mode: record.analysisMode === "company_deal" ? "company_deal" : "legacy",
      };
    }
    if ("id" in record) {
      return { video: detail as VideoItem, mode: "legacy" };
    }
    return { video: null, mode: "legacy" };
  }

  function openAnalysisPage(options: {
    archive?: RecordItem | null;
    video?: VideoItem | null;
    mode?: "legacy" | "company_deal";
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

  onMount(() => {
    void set_title("典典直播切片");
    void ensureActiveEnterpriseMaster();
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
      const { video, mode } = resolveVideoAnalysisDetail(
        (event as CustomEvent<unknown>).detail,
      );
      openAnalysisPage({ archive: null, video, mode });
    };
    const openCompanyDealReview = (event: Event) => {
      const archive = (event as CustomEvent<RecordItem>).detail;
      if (!archive) {
        alert("无法打开分析页：录播数据为空。");
        return;
      }
      if (archive.archive_kind !== "company") {
        alert("只有公司录播才能打开成交订单时间轴分析。");
        return;
      }
      openAnalysisPage({
        archive,
        video: null,
        mode: "company_deal",
      });
    };
    const openMasterBuilder = (event: Event) => {
      masterSourceVideo = (event as CustomEvent<VideoItem>).detail;
    };
    const openLiveDashboard = (event: Event) => {
      liveDashboardSessionId = (event as CustomEvent<{ sessionId: number }>).detail.sessionId;
      active = "直播数据大屏";
    };
    const openClipReview = (event: Event) => {
      clipReviewRequest = (event as CustomEvent<ClipReviewRequest>).detail;
      active = "切片";
    };
    window.addEventListener("bsr:open-archive-analysis", openArchiveAnalysis);
    window.addEventListener("bsr:open-company-deal-review", openCompanyDealReview);
    window.addEventListener("bsr:open-video-analysis", openVideoAnalysis);
    window.addEventListener("bsr:build-master", openMasterBuilder);
    window.addEventListener("bsr:open-live-dashboard", openLiveDashboard);
    window.addEventListener("bsr:open-clip-review", openClipReview);
    return () => {
      window.removeEventListener("bsr:open-archive-analysis", openArchiveAnalysis);
      window.removeEventListener("bsr:open-company-deal-review", openCompanyDealReview);
      window.removeEventListener("bsr:open-video-analysis", openVideoAnalysis);
      window.removeEventListener("bsr:build-master", openMasterBuilder);
      window.removeEventListener("bsr:open-live-dashboard", openLiveDashboard);
      window.removeEventListener("bsr:open-clip-review", openClipReview);
    };
  });

  log.info("App loaded");
</script>

<main>
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
      <div class="page" class:visible={active == "直播数据大屏"}>
        <LiveDataDashboard initialSessionId={liveDashboardSessionId} />
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

<style>
  .sidebar {
    display: flex;
    height: 100vh;
    flex: 0 0 224px;
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

  .page.visible :global(> *) {
    flex: 1 1 auto;
    min-height: 0;
    width: 100%;
  }

  .content {
    width: calc(100% - 12px);
    height: calc(100vh - 20px);
    margin: 10px 10px 10px 0;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.82);
    border-radius: var(--mac-radius-xl);
    background: var(--mac-bg-elevated);
    box-shadow: var(--mac-shadow-lg);
    backdrop-filter: blur(24px) saturate(150%);
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
</style>
