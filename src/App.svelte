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
  import AI from "./page/AI.svelte";
  import Archive from "./page/Archive.svelte";
  import ArchiveAnalysis from "./page/ArchiveAnalysis.svelte";
  import type { RecordItem } from "./lib/db";
  import type { VideoItem } from "./lib/interface";
  import { onMount } from "svelte";

  let active = "总览";
  let analysisArchive: RecordItem | null = null;
  let analysisVideo: VideoItem | null = null;
  let analysisRefreshToken = 0;
  onMount(() => {
    void set_title("典典直播切片");
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
      analysisArchive = (event as CustomEvent<RecordItem>).detail;
      analysisVideo = null;
      analysisRefreshToken += 1;
      active = "录播分析";
    };
    const openVideoAnalysis = (event: Event) => {
      analysisVideo = (event as CustomEvent<VideoItem>).detail;
      analysisArchive = null;
      analysisRefreshToken += 1;
      active = "录播分析";
    };
    const openArchiveTranscription = () => {
      active = "助手";
    };
    window.addEventListener("bsr:open-archive-analysis", openArchiveAnalysis);
    window.addEventListener("bsr:open-video-analysis", openVideoAnalysis);
    window.addEventListener("bsr:transcribe-archive", openArchiveTranscription);
    return () => {
      window.removeEventListener("bsr:open-archive-analysis", openArchiveAnalysis);
      window.removeEventListener("bsr:open-video-analysis", openVideoAnalysis);
      window.removeEventListener("bsr:transcribe-archive", openArchiveTranscription);
    };
  });

  // HMR can preserve this route while resetting the selected analysis source.
  $: if (active === "录播分析" && !analysisArchive && !analysisVideo) {
    active = "录播";
  }

  log.info("App loaded");
</script>

<main>
  <div class="wrap">
    <div class="sidebar">
      <BSidebar
        bind:activeUrl={active}
        on:activeChange={(e) => {
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
          on:back={() => {
            active = analysisVideo ? "切片" : "录播";
          }}
        />
      </div>
      <div class="page" class:visible={active == "切片"}>
        <Clip />
      </div>
      <div class="page" class:visible={active == "任务"}>
        <Task />
      </div>
      <div class="page" class:visible={active == "助手"}>
        <AI />
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
      radial-gradient(circle at 82% -10%, rgba(164, 210, 255, .35), transparent 34%),
      linear-gradient(135deg, #f3f4f7 0%, #e9ebf0 100%);
  }

  .visible {
    opacity: 1 !important;
    height: 100% !important;
    transform: translateX(0) !important;
  }

  .page {
    opacity: 0;
    height: 0;
    transform: translateX(100%);
    overflow: hidden;
    transition:
      opacity 0.5s ease-in-out,
      transform 0.3s ease-in-out;
    display: flex;
    flex-direction: column;
  }

  .content {
    width: calc(100% - 12px);
    height: calc(100vh - 20px);
    margin: 10px 10px 10px 0;
    overflow: hidden;
    border: 1px solid rgba(255,255,255,.82);
    border-radius: 20px;
    background: rgba(255,255,255,.88);
    box-shadow: 0 18px 50px rgba(47, 53, 66, .12), 0 2px 6px rgba(47, 53, 66, .05);
    backdrop-filter: blur(24px) saturate(150%);
  }

  :global(.dark) .wrap { background: radial-gradient(circle at 82% -10%, rgba(28,91,148,.28), transparent 34%), #18181a; }
  :global(.dark) .content { border-color: rgba(255,255,255,.08); background: rgba(37,37,40,.9); box-shadow: 0 18px 50px rgba(0,0,0,.32); }
</style>
