<script lang="ts">
  import { invoke, invokeSensitive, open, onOpenUrl, get_static_url, get } from "../lib/invoker";
  import { message } from "@tauri-apps/plugin-dialog";
  import { fade, scale } from "svelte/transition";
  import { Dropdown, DropdownItem } from "flowbite-svelte";
  import type { RecorderList, RecorderInfo } from "../lib/interface";
  import type { RecordItem } from "../lib/db";
  import {
    Ellipsis,
    Play,
    Plus,
    FileVideo,
    Search,
    Trash2,
    X,
    History,
    PlayIcon,
    Globe,
    UserRound,
    Pencil,
  } from "lucide-svelte";
  import BilibiliIcon from "../lib/components/BilibiliIcon.svelte";
  import DouyinIcon from "../lib/components/DouyinIcon.svelte";
  import KuaishouIcon from "../lib/components/KuaishouIcon.svelte";
  import HuyaIcon from "../lib/components/HuyaIcon.svelte";
  import TikTokIcon from "../lib/components/TikTokIcon.svelte";
  import AutoRecordIcon from "../lib/components/AutoRecordIcon.svelte";
  import GenerateWholeClipModal from "../lib/components/GenerateWholeClipModal.svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import {
    ROOM_STREAMER_MAX_LENGTH,
    normalizeRoomStreamerName,
    roomStreamerLabel,
    validateRoomStreamerName,
  } from "../lib/roomStreamer";
  import { onMount } from "svelte";

  export let room_count = 0;
  let room_active = 0;
  let room_inactive = 0;

  let summary: RecorderList = {
    count: 0,
    recorders: [],
  };

  let searchQuery = "";
  $: filteredRecorders = summary.recorders.filter((room) => {
    const query = searchQuery.toLowerCase();
    return (
      room.room_info.room_title.toLowerCase().includes(query) ||
      room.user_info.user_name.toLowerCase().includes(query) ||
      (room.current_streamer || "").toLowerCase().includes(query) ||
      room.room_info.room_id.toString().includes(query)
    );
  });

  function default_avatar(platform: string) {
    const avatarMap = {
      bilibili: "/imgs/bilibili_avatar.png",
      douyin: "/imgs/douyin.png",
      huya: "/imgs/huya_avatar.png",
      kuaishou: "/imgs/kuaishou.svg",
      tiktok: "/imgs/tiktok.png",
    };
    return avatarMap[platform] || "/imgs/huya_avatar.png";
  }

  function default_cover(platform: string) {
    const coverMap = {
      bilibili: "/imgs/bilibili.png",
      douyin: "/imgs/douyin.png",
      huya: "/imgs/huya.png",
      kuaishou: "/imgs/kuaishou.svg",
      tiktok: "/imgs/tiktok.png",
    };
    return coverMap[platform] || "/imgs/huya.png";
  }

  let avatar_cache: Map<string, string> = new Map();
  async function get_avatar_url(user_id: string, url: string) {
    if (avatar_cache.has(user_id)) {
      return avatar_cache.get(user_id);
    }
    console.log("get avatar url:", url);
    const response = await get(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const blob = await response.blob();
    const avatar_url = URL.createObjectURL(blob);
    avatar_cache.set(user_id, avatar_url);
    return avatar_url;
  }

  let image_cache: Map<string, string> = new Map();
  async function get_image_url(url: string) {
    if (image_cache.has(url)) {
      return image_cache.get(url);
    }
    console.log("get image url:", url);
    const response = await get(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const blob = await response.blob();
    const cover_url = URL.createObjectURL(blob);
    image_cache.set(url, cover_url);
    return cover_url;
  }

  async function update_summary() {
    let new_summary = (await invoke("get_recorder_list")) as RecorderList;
    room_count = new_summary.count;
    room_active = new_summary.recorders.filter(
      (room) => room.room_info.status,
    ).length;
    room_inactive = new_summary.recorders.filter(
      (room) => !room.room_info.status,
    ).length;

    // sort new_summary.recorders by live_status
    new_summary.recorders.sort((a, b) => {
      if (a.room_info.status && !b.room_info.status) return -1;
      if (!a.room_info.status && b.room_info.status) return 1;
      return 0;
    });

    // Render cards immediately with local placeholders. Douyin image hosts can
    // be slow or temporarily unreachable; they must not block the room list.
    const remoteImages = new_summary.recorders.map((room) => ({
      room,
      avatar: room.user_info.user_avatar,
      cover: room.room_info.room_cover,
    }));
    for (const { room } of remoteImages) {
      room.user_info.user_avatar = default_avatar(room.room_info.platform);
      room.room_info.room_cover = default_cover(room.room_info.platform);
    }
    summary = new_summary;

    await Promise.all(
      remoteImages.map(async ({ room, avatar, cover }) => {
        if (avatar) {
          try {
            room.user_info.user_avatar = await get_avatar_url(
              room.user_info.user_id,
              avatar,
            );
          } catch (error) {
            console.warn("加载直播间头像失败，使用默认图标", error);
          }
        }
        if (cover) {
          try {
            room.room_info.room_cover = await get_image_url(cover);
          } catch (error) {
            console.warn("加载直播间封面失败，使用默认封面", error);
          }
        }
      }),
    );
    summary = { ...new_summary, recorders: [...new_summary.recorders] };
  }
  update_summary();
  setInterval(update_summary, 5000);

  // modals
  let deleteModal = false;
  let deleteRoom = null;

  let addModal = false;
  let addRoom = "";
  let selectedPlatform = "bilibili";

  let archiveModal = false;
  let archiveRoom: RecorderInfo = null;
  let archives: RecordItem[] = [];

  let streamerModal = false;
  let streamerRoom: RecorderInfo = null;
  let streamerName = "";
  let knownStreamerNames: string[] = [];
  let streamerSaving = false;
  let streamerError = "";

  async function openStreamerEditor(room: RecorderInfo) {
    streamerRoom = room;
    streamerName = room.current_streamer || "";
    streamerError = "";
    streamerModal = true;
    try {
      knownStreamerNames = (await invoke("get_known_streamer_names")) as string[];
    } catch (error) {
      console.warn("加载主播候选失败", error);
      knownStreamerNames = [];
    }
  }

  async function saveRoomStreamer(clearAssignment = false) {
    if (!streamerRoom || streamerSaving) return;
    const nextName = clearAssignment ? "" : normalizeRoomStreamerName(streamerName);
    if (!clearAssignment) {
      streamerError = validateRoomStreamerName(nextName);
      if (streamerError) return;
    }

    streamerSaving = true;
    streamerError = "";
    try {
      await invoke("set_room_streamer", {
        platform: streamerRoom.room_info.platform,
        roomId: streamerRoom.room_info.room_id,
        streamerName: nextName,
      });
      const source: RecorderInfo["current_streamer_source"] = nextName
        ? "manual"
        : "";
      summary = {
        ...summary,
        recorders: summary.recorders.map((room) =>
          room.room_info.platform === streamerRoom.room_info.platform &&
          room.room_info.room_id === streamerRoom.room_info.room_id
            ? {
                ...room,
                current_streamer: nextName,
                current_streamer_source: source,
              }
            : room,
        ),
      };
      streamerModal = false;
      await update_summary();
    } catch (error) {
      streamerError = `保存失败：${String(error)}`;
    } finally {
      streamerSaving = false;
    }
  }

  // 分页相关状态
  let currentPage = 0;
  let pageSize = 20;
  let hasMore = true;
  let isLoading = false;
  let loadError = "";

  async function showArchives(room_id: string) {
    // 重置分页状态
    currentPage = 0;
    archives = [];
    hasMore = true;
    isLoading = false;
    loadError = "";

    updateArchives();
    archiveModal = true;
    console.log(archives);
  }

  async function updateArchives() {
    if (isLoading || !hasMore) return;

    isLoading = true;

    try {
      let new_archives = (await invoke("get_archives", {
        roomId: archiveRoom.room_info.room_id,
        offset: currentPage * pageSize,
        limit: pageSize,
      })) as RecordItem[];

      for (const archive of new_archives) {
        archive.cover = await get_static_url(
          "cache",
          `${archive.platform}/${archive.room_id}/${archive.live_id}/cover.jpg`,
        );
      }

      // 如果是第一页，直接替换；否则追加数据
      if (currentPage === 0) {
        archives = new_archives;
      } else {
        archives = [...archives, ...new_archives];
      }

      // 检查是否还有更多数据
      hasMore = new_archives.length === pageSize;

      // 按时间排序
      archives.sort((a, b) => {
        return (
          new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
        );
      });

      // 更新总数（如果后端支持的话）
      // totalCount = await invoke("get_archives_count", { roomId: archiveRoom.room_id });

      currentPage++;
    } catch (error) {
      console.error("Failed to load archives:", error);
      loadError = "加载失败，请重试";
    } finally {
      isLoading = false;
    }
  }

  async function loadMoreArchives() {
    await updateArchives();
  }

  function handleScroll(event: Event) {
    const target = event.target as HTMLElement;
    const { scrollTop, scrollHeight, clientHeight } = target;

    // 当滚动到距离底部100px时，自动加载更多
    if (
      scrollHeight - scrollTop - clientHeight < 100 &&
      hasMore &&
      !isLoading
    ) {
      loadMoreArchives();
    }
  }

  function format_ts(ts_string: string) {
    const date = new Date(ts_string);
    return date.toLocaleString();
  }

  function format_duration(duration: number) {
    duration = Math.round(duration);
    const hours = Math.floor(duration / 3600)
      .toString()
      .padStart(2, "0");
    const minutes = Math.floor((duration % 3600) / 60)
      .toString()
      .padStart(2, "0");
    const seconds = (duration % 60).toString().padStart(2, "0");

    return `${hours}:${minutes}:${seconds}`;
  }

  function format_size(size: number) {
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

  function calc_bitrate(size: number, duration: number) {
    return ((size * 8) / duration / 1024).toFixed(0);
  }

  function get_live_status_label(room: RecorderInfo) {
    if (!room.room_info.status) {
      return "直播未开始";
    }

    return room.recording ? "直播进行中" : "直播进行中（未录制）";
  }

  function is_live_but_not_recording(room: RecorderInfo) {
    return room.room_info.status && !room.recording;
  }

  function handleModalClickOutside(event) {
    // 检查点击是否在任何modal内部
    const clickedElement = event.target;

    // 检查是否点击了按钮，如果是则不关闭modal
    if (clickedElement.closest("button")) {
      return;
    }

    // 按层级顺序检查modal，优先处理最上层的modal
    // 如果点击在最上层modal内部，则不处理任何modal关闭

    // 最上层：generateWholeClipModal (handled by component)
    if (generateWholeClipModal) {
      return; // Let the component handle its own modal closing
    }

    // 第二层：archiveModal
    if (archiveModal) {
      const archiveModalEl = document.querySelector(".archive-modal");
      if (archiveModalEl) {
        if (archiveModalEl.contains(clickedElement)) {
          // 点击在archiveModal内部，不关闭任何modal
          return;
        } else {
          // 点击在archiveModal外部，关闭它
          archiveModal = false;
          return;
        }
      }
    }

    // 第三层：addModal
    if (addModal) {
      const addModalEl = document.querySelector(".add-modal");
      if (addModalEl) {
        if (addModalEl.contains(clickedElement)) {
          // 点击在addModal内部，不关闭任何modal
          return;
        } else {
          // 点击在addModal外部，关闭它
          addModal = false;
          return;
        }
      }
    }

    // 第四层：deleteModal
    if (deleteModal) {
      const deleteModalEl = document.querySelector(".delete-modal");
      if (deleteModalEl) {
        if (deleteModalEl.contains(clickedElement)) {
          // 点击在deleteModal内部，不关闭任何modal
          return;
        } else {
          // 点击在deleteModal外部，关闭它
          deleteModal = false;
          return;
        }
      }
    }
  }

  // Function to toggle auto-record state
  function toggleEnabled(room: RecorderInfo) {
    invoke("set_enable", {
      roomId: room.room_info.room_id,
      platform: room.room_info.platform,
      enabled: !room.enabled,
    });
  }

  function openUserUrl(room: RecorderInfo) {
    if (room.room_info.platform === "bilibili") {
      open("https://space.bilibili.com/" + room.user_info.user_id);
    } else if (room.room_info.platform === "douyin") {
      console.log(room.user_info);
      open("https://www.douyin.com/user/" + room.user_info.user_id);
    } else if (room.room_info.platform === "kuaishou") {
      if (room.user_info.user_id) {
        open("https://www.kuaishou.com/profile/" + room.user_info.user_id);
      } else {
        openLiveUrl(room);
      }
    } else if (room.room_info.platform === "tiktok") {
      if (room.user_info.user_id) {
        const handle = room.user_info.user_id.startsWith("@")
          ? room.user_info.user_id
          : `@${room.user_info.user_id}`;
        open(`https://www.tiktok.com/${handle}`);
      } else {
        openLiveUrl(room);
      }
    }
  }

  function openLiveUrl(room: RecorderInfo) {
    switch (room.room_info.platform) {
      case "bilibili":
        open("https://live.bilibili.com/" + room.room_info.room_id);
        break;
      case "douyin":
        open("https://live.douyin.com/" + room.room_info.room_id);
        break;
      case "huya":
        open("https://www.huya.com/" + room.room_info.room_id);
        break;
      case "kuaishou":
        open("https://live.kuaishou.com/u/" + room.room_info.room_id);
        break;
      case "tiktok":
        open(`https://www.tiktok.com/${room.room_info.room_id}/live`);
        break;
    }
  }

  function addNewRecorder(room_id: string, platform: string) {
    room_id = room_id.trim().split("?")[0];
    switch (platform) {
      case "bilibili":
        // room_id might be a link, extract the room_id from the link
        // example: https://live.bilibili.com/1234567890
        if (room_id.includes("https://") || room_id.includes("bsr://")) {
          room_id = room_id.split("/").pop();
        }
        break;
      case "douyin":
        // room_id might be a link, extract the room_id from the link
        // example: https://live.douyin.com/1234567890
        if (room_id.includes("https://") || room_id.includes("bsr://")) {
          room_id = room_id.split("/").pop();
        }
        break;
      case "huya":
        // room_id might be a link, extract the room_id from the link
        // example: https://www.huya.com/1234567890
        if (room_id.includes("https://") || room_id.includes("bsr://")) {
          room_id = room_id.split("/").pop();
        }
        break;
      case "kuaishou":
        // room_id might be a link, extract the room_id from the link
        // example: https://live.kuaishou.com/u/1234567890
        if (room_id.includes("https://") || room_id.includes("bsr://")) {
          room_id = room_id.split("/").pop();
        }
        break;
      case "tiktok":
        // room_id might be a link, extract the room_id from the link
        // example: https://www.tiktok.com/@1234567890/live -> @1234567890
        if (room_id.includes("https://") || room_id.includes("bsr://")) {
          room_id = "@" + room_id.split("@").pop().split("/")[0];
        }
        break;
    }

    invoke("add_recorder", {
      roomId: room_id,
      platform: platform,
      extra: "",
    })
      .then(() => {
        addModal = false;
        addRoom = "";
      })
      .catch(async (e) => {
        await message(e);
      });
  }

  let generateWholeClipModal = false;
  let generateWholeClipArchive: RecordItem = null;

  async function openGenerateWholeClipModal(archive: RecordItem) {
    generateWholeClipModal = true;
    generateWholeClipArchive = archive;
  }

  function handleWholeClipGenerated() {
    generateWholeClipModal = false;
    generateWholeClipArchive = null;
  }

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

        if (url.startsWith("bsr://live.bilibili.com/")) {
          platform = "bilibili";
        }

        if (url.startsWith("bsr://live.douyin.com/")) {
          platform = "douyin";
        }

        if (url.startsWith("bsr://live.kuaishou.com/")) {
          platform = "kuaishou";
        }

        if (url.startsWith("bsr://live.tiktok.com/")) {
          platform = "tiktok";
        }

        if (url && platform) {
          addModal = true;
          addRoom = url;
          selectedPlatform = platform;
        }
      }
    });
  });
</script>

<PageShell title="直播间" subtitle={`${room_active} 直播中 · ${room_inactive} 未直播`}>
  <div slot="actions" class="flex min-w-0 items-center gap-2">
    <div class="relative min-w-0">
      <Search
        class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-[color:var(--mac-tertiary)]"
      />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="搜索直播间..."
        class="mac-field w-36 pl-9 pr-3 sm:w-52"
      />
    </div>
    <button
      type="button"
      aria-label="添加新直播间"
      class="mac-btn mac-btn-primary"
      on:click={() => {
        addModal = true;
      }}
    >
      <Plus class="w-4 h-4" />
      <span class="hidden sm:inline">添加新直播间</span>
    </button>
  </div>

    <!-- Room Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
      <!-- Active Room Card -->
      {#each filteredRecorders as room (room.room_info.room_id)}
        <div
          class="mac-card flex h-full min-w-0 flex-col p-4 transition-colors hover:border-[color:var(--mac-blue)]"
        >
          <div class="relative">
            <img
              src={room.room_info.room_cover}
              alt={`${room.room_info.room_title}直播封面`}
              class={"w-full h-40 object-cover rounded-lg " +
                (room.room_info.status ? "" : "brightness-75")}
            />
            <!-- Room ID watermark -->
            <div
              class="absolute bottom-2 left-2 px-2 py-1 rounded-md bg-black/30 backdrop-blur-sm flex items-center"
            >
              <span class="text-xs text-white/80 font-mono"
                >{room.room_info.platform.toUpperCase()}#{room.room_info
                  .room_id}</span
              >
            </div>
            {#if room.enabled}
              <div
                class={"absolute top-2 left-2 p-1.5 px-2 rounded-md text-white text-xs flex items-center justify-center " +
                  (room.recording ? "bg-red-500" : "bg-yellow-500")}
              >
                <AutoRecordIcon class="w-4 h-4 text-white" />
                {#if room.recording}
                  <span class="text-white ml-1">录制中</span>
                {:else}
                  <span class="text-white ml-1">未录制</span>
                {/if}
              </div>
            {/if}
            <div
              class={"absolute bottom-2 right-2 p-1.5 px-2 rounded-md text-white text-xs flex items-center justify-center " +
                (!room.room_info.status
                  ? "bg-gray-700"
                  : is_live_but_not_recording(room)
                    ? "bg-amber-600"
                    : "bg-green-500")}
            >
              <span>{get_live_status_label(room)}</span>
            </div>
            <button
              type="button"
              aria-label={`管理直播间 ${room.room_info.room_title}`}
              aria-haspopup="menu"
              class="absolute top-2 right-2 p-1.5 rounded-lg bg-gray-900/50 hover:bg-gray-900/70 transition-colors"
            >
              <Ellipsis class="w-5 h-5 icon-white" />
            </button>
            <Dropdown class="whitespace-nowrap">
              <button
                class="px-4 py-2 flex items-center justify-between hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer"
                on:click={() => toggleEnabled(room)}
              >
                <span
                  class="text-sm text-gray-700 dark:text-gray-200 font-medium"
                  >启用直播间</span
                >
                <label class="toggle-switch ml-1">
                  <input type="checkbox" checked={room.enabled} />
                  <span class="toggle-slider"></span>
                </label>
              </button>
              <DropdownItem
                on:click={() => {
                  openLiveUrl(room);
                }}>打开网页直播间</DropdownItem
              >
              <DropdownItem
                on:click={() => {
                  openStreamerEditor(room);
                }}>修改当前主播</DropdownItem
              >
              <DropdownItem
                class="text-red-500"
                on:click={() => {
                  deleteRoom = room;
                  deleteModal = true;
                }}>移除直播间</DropdownItem
              >
            </Dropdown>
          </div>
          <div
            class="mt-3 flex min-w-0 flex-1 flex-col gap-2.5"
            data-testid={`room-card-metadata-${room.room_info.platform}-${room.room_info.room_id}`}
          >
            <div
              class="grid min-w-0 grid-cols-[20px_minmax(0,1fr)] items-start gap-2"
              data-testid={`room-title-${room.room_info.platform}-${room.room_info.room_id}`}
            >
              <div class="mt-0.5 flex h-5 w-5 items-center justify-center" aria-hidden="true">
                  {#if room.room_info.platform === "bilibili"}
                    <BilibiliIcon class="w-4 h-4" />
                  {:else if room.room_info.platform === "douyin"}
                    <DouyinIcon class="w-4 h-4" />
                  {:else if room.room_info.platform === "kuaishou"}
                    <KuaishouIcon class="w-4 h-4" />
                  {:else if room.room_info.platform === "huya"}
                    <HuyaIcon class="w-4 h-4" />
                  {:else if room.room_info.platform === "tiktok"}
                    <TikTokIcon class="w-5 h-5" />
                  {:else}
                    <Globe class="w-4 h-4 text-gray-400" />
                  {/if}
              </div>
              <h3
                class="line-clamp-2 min-h-[44px] min-w-0 text-[15px] font-medium leading-[22px] text-[color:var(--mac-label)]"
                title={room.room_info.room_title}
              >
                {room.room_info.room_title}
              </h3>
            </div>

            <button
              type="button"
              data-testid={`room-streamer-${room.room_info.platform}-${room.room_info.room_id}`}
              class="grid min-h-10 w-full min-w-0 grid-cols-[auto_auto_minmax(0,1fr)_auto_auto] items-center gap-1.5 rounded-lg border border-[color:var(--mac-separator)] bg-[color:var(--mac-control-bg)] px-2.5 py-1.5 text-sm text-[color:var(--mac-secondary)] transition-colors hover:border-[color:var(--mac-blue)] hover:text-[color:var(--mac-label)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--mac-blue)]"
              aria-label={`修改当前主播，当前为${roomStreamerLabel(room.current_streamer)}`}
              on:click={() => openStreamerEditor(room)}
            >
              <UserRound aria-hidden="true" class="h-4 w-4 shrink-0 text-[color:var(--mac-blue)]" />
              <span class="shrink-0 text-xs">当前主播</span>
              <span
                class="truncate text-left font-semibold text-[color:var(--mac-label)]"
                title={roomStreamerLabel(room.current_streamer)}
              >
                {roomStreamerLabel(room.current_streamer)}
              </span>
              {#if room.current_streamer_source === "auto"}
                <span class="shrink-0 whitespace-nowrap rounded bg-[color:var(--mac-blue-soft)] px-1.5 py-0.5 text-[10px] text-[color:var(--mac-blue)]">
                  自动识别
                </span>
              {/if}
              <Pencil aria-hidden="true" class="h-3.5 w-3.5 shrink-0 text-[color:var(--mac-tertiary)]" />
            </button>

            <div class="mt-auto flex min-w-0 items-center justify-between gap-2 border-t border-[color:var(--mac-separator)] pt-2.5">
              <button
                class="flex min-w-0 flex-1 items-center gap-2 rounded-lg p-1 text-left text-sm text-[color:var(--mac-secondary)] hover:bg-gray-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--mac-blue)] dark:hover:bg-gray-700"
                aria-label={`打开直播账号 ${room.user_info.user_name}`}
                on:click={() => {
                  openUserUrl(room);
                }}
              >
                <img
                  src={room.user_info.user_avatar}
                  alt=""
                  class="h-8 w-8 shrink-0 rounded-full object-cover"
                />
                <span class="min-w-0 flex-1">
                  <span class="block text-[11px] leading-4 text-[color:var(--mac-tertiary)]">直播账号</span>
                  <span
                    class="block truncate font-medium leading-5 text-[color:var(--mac-secondary)]"
                    data-testid={`room-account-${room.room_info.platform}-${room.room_info.room_id}`}
                    title={room.user_info.user_name}
                  >
                    {room.user_info.user_name}
                  </span>
                </span>
              </button>
              <div class="flex shrink-0 items-center space-x-1">
                {#if room.recording}
                  <button
                    type="button"
                    aria-label="打开当前直播"
                    class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700"
                    on:click={() => {
                      invoke("open_live", {
                        platform: room.room_info.platform,
                        roomId: room.room_info.room_id,
                        liveId: room.live_id,
                      });
                    }}
                  >
                    <Play class="w-5 h-5 dark:icon-white" />
                  </button>
                {/if}
                <button
                  type="button"
                  aria-label="查看直播记录"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700"
                  on:click={() => {
                    archiveRoom = room;
                    showArchives(room.room_info.room_id);
                  }}
                >
                  <History class="w-5 h-5 dark:icon-white" />
                </button>
              </div>
            </div>
          </div>
        </div>
      {/each}

      <!-- Add Room Card -->
      <button
        type="button"
        class="p-4 rounded-[14px] border-2 border-dashed border-[color:var(--mac-separator-strong)] hover:border-[color:var(--mac-blue)] transition-colors flex flex-col items-center justify-center space-y-2"
        on:click={() => {
          addModal = true;
        }}
      >
        <div
          class="w-12 h-12 rounded-full bg-[color:var(--mac-blue-soft)] flex items-center justify-center"
        >
          <Plus class="w-6 h-6 icon-primary" />
        </div>
        <span class="text-sm font-medium text-[color:var(--mac-blue)]"
          >添加新直播间</span
        >
        <span class="text-xs text-[color:var(--mac-tertiary)]"
          >配置一个新直播间以及其相关设置</span
        >
      </button>
    </div>
</PageShell>
{#if streamerModal && streamerRoom}
  <MacModal
    title="设置当前主播"
    showClose
    closeOnBackdrop
    panelClass="w-[min(440px,calc(100vw-32px))]"
    on:close={() => {
      if (!streamerSaving) streamerModal = false;
    }}
  >
    <div class="space-y-4">
      <div class="rounded-xl bg-[color:var(--mac-control-bg)] px-3.5 py-3">
        <p class="text-xs text-[color:var(--mac-tertiary)]">直播间</p>
        <p class="mt-1 truncate text-sm font-medium text-[color:var(--mac-label)]">
          {streamerRoom.room_info.room_title}
        </p>
      </div>

      <div class="space-y-2">
        <label
          for="room-streamer-name"
          class="block text-sm font-medium text-[color:var(--mac-label)]"
        >
          谁在直播
        </label>
        <input
          id="room-streamer-name"
          data-testid="room-streamer-input"
          data-modal-initial-focus
          class="mac-field w-full"
          class:border-red-500={Boolean(streamerError)}
          bind:value={streamerName}
          maxlength={ROOM_STREAMER_MAX_LENGTH}
          placeholder="输入主播姓名，如：罗雨欣"
          autocomplete="off"
          aria-invalid={Boolean(streamerError)}
          aria-describedby="room-streamer-help"
          on:input={() => {
            streamerError = "";
          }}
          on:keydown={(event) => {
            if (event.key === "Enter") saveRoomStreamer();
          }}
        />
        <div class="flex items-start justify-between gap-3 text-xs">
          <p
            id="room-streamer-help"
            role={streamerError ? "alert" : undefined}
            class={streamerError ? "text-red-500" : "text-[color:var(--mac-tertiary)]"}
          >
            {streamerError || "1–12 个字符。人工指定优先于自动识别。"}
          </p>
          <span class="shrink-0 text-[color:var(--mac-tertiary)]">
            {Array.from(streamerName).length}/{ROOM_STREAMER_MAX_LENGTH}
          </span>
        </div>
      </div>

      {#if knownStreamerNames.length > 0}
        <div class="space-y-2">
          <p class="text-xs font-medium text-[color:var(--mac-secondary)]">已有主播</p>
          <div class="flex flex-wrap gap-2" data-testid="known-streamer-options">
            {#each knownStreamerNames.slice(0, 8) as name}
              <button
                type="button"
                class="rounded-full border border-[color:var(--mac-separator)] px-3 py-1.5 text-xs text-[color:var(--mac-secondary)] hover:border-[color:var(--mac-blue)] hover:text-[color:var(--mac-blue)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--mac-blue)]"
                on:click={() => {
                  streamerName = name;
                  streamerError = "";
                }}
              >
                {name}
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <p class="rounded-lg border border-[color:var(--mac-separator)] px-3 py-2.5 text-xs leading-5 text-[color:var(--mac-secondary)]">
        {#if streamerRoom.recording}
          保存后立即同步到本次正在录制的场次，并用于该直播间后续场次。
        {:else}
          保存后用于该直播间下次及后续场次。
        {/if}
        已结束的历史场次不会改动。
      </p>
    </div>

    <svelte:fragment slot="actions">
      {#if streamerRoom.current_streamer_source === "manual"}
        <button
          type="button"
          class="mac-btn mr-auto"
          disabled={streamerSaving}
          on:click={() => saveRoomStreamer(true)}
        >
          恢复自动识别
        </button>
      {/if}
      <button
        type="button"
        class="mac-btn"
        disabled={streamerSaving}
        on:click={() => {
          streamerModal = false;
        }}
      >
        取消
      </button>
      <button
        type="button"
        data-testid="save-room-streamer"
        class="mac-btn mac-btn-primary"
        disabled={streamerSaving || !streamerName.trim()}
        on:click={() => saveRoomStreamer()}
      >
        {streamerSaving ? "保存中…" : "保存"}
      </button>
    </svelte:fragment>
  </MacModal>
{/if}
{#if deleteModal}
  <MacModal bare panelClass="w-[320px] delete-modal">
    <div class="p-6 space-y-4">
      <div class="text-center space-y-2">
        <h3 class="text-[15px] font-semibold text-[color:var(--mac-label)]">
          移除直播间
        </h3>
        <p class="text-sm text-[color:var(--mac-tertiary)]">
          此操作将移除所有相关的录制记录
        </p>
      </div>
      <div class="flex justify-center gap-3">
        <button
          type="button"
          class="mac-btn w-24"
          on:click={() => {
            deleteModal = false;
          }}
        >
          取消
        </button>
        <button
          type="button"
          class="mac-btn mac-btn-danger w-24"
          on:click={async () => {
            try {
              await invokeSensitive("remove_recorder", {
                roomId: deleteRoom.room_info.room_id,
                platform: deleteRoom.room_info.platform,
              });
              deleteModal = false;
              await update_summary();
            } catch (error) {
              alert(`移除直播间失败：${error}`);
            }
          }}
        >
          移除
        </button>
      </div>
    </div>
  </MacModal>
{/if}

{#if addModal}
  <div
    class="fixed inset-0 bg-black/20 dark:bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center"
    transition:fade={{ duration: 200 }}
  >
    <div
      class="mac-modal add-modal w-[400px] bg-white dark:bg-[#323234] rounded-xl shadow-xl overflow-hidden"
      transition:scale={{ duration: 150, start: 0.95 }}
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-gray-200 dark:border-gray-700/50">
        <h2 class="text-base font-medium text-gray-900 dark:text-white">
          添加直播间
        </h2>
      </div>

      <div class="p-6 space-y-6">
        <div class="space-y-4">
          <div class="space-y-2">
            <label
              for="platform"
              class="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              平台
            </label>
            <div
              class="flex items-center gap-2 p-0.5 bg-[#f5f5f7] dark:bg-[#1c1c1e] rounded-lg overflow-x-auto custom-scrollbar-light"
            >
              <button
                class="flex-none px-3 py-2 text-sm font-medium whitespace-nowrap rounded-md transition-colors {selectedPlatform ===
                'bilibili'
                  ? 'bg-white dark:bg-[#323234] shadow-sm text-gray-900 dark:text-white'
                  : 'text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white'}"
                on:click={() => (selectedPlatform = "bilibili")}
              >
                哔哩哔哩
              </button>
              <button
                class="flex-none px-3 py-2 text-sm font-medium whitespace-nowrap rounded-md transition-colors {selectedPlatform ===
                'douyin'
                  ? 'bg-white dark:bg-[#323234] shadow-sm text-gray-900 dark:text-white'
                  : 'text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white'}"
                on:click={() => (selectedPlatform = "douyin")}
              >
                抖音
              </button>
              <button
                class="flex-none px-3 py-2 text-sm font-medium whitespace-nowrap rounded-md transition-colors {selectedPlatform ===
                'huya'
                  ? 'bg-white dark:bg-[#323234] shadow-sm text-gray-900 dark:text-white'
                  : 'text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white'}"
                on:click={() => (selectedPlatform = "huya")}
              >
                虎牙
              </button>
              <button
                class="flex-none px-3 py-2 text-sm font-medium whitespace-nowrap rounded-md transition-colors {selectedPlatform ===
                'kuaishou'
                  ? 'bg-white dark:bg-[#323234] shadow-sm text-gray-900 dark:text-white'
                  : 'text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white'}"
                on:click={() => (selectedPlatform = "kuaishou")}
              >
                快手
              </button>
              <button
                class="flex-none px-3 py-2 text-sm font-medium whitespace-nowrap rounded-md transition-colors {selectedPlatform ===
                'tiktok'
                  ? 'bg-white dark:bg-[#323234] shadow-sm text-gray-900 dark:text-white'
                  : 'text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white'}"
                on:click={() => (selectedPlatform = "tiktok")}
              >
                TikTok
              </button>
            </div>
          </div>

          <div class="space-y-2">
            <label
              for="room_id"
              class="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              直播间 ID 或链接
            </label>
            <input
              id="room_id"
              type="text"
              bind:value={addRoom}
              class="w-full px-3 py-2 bg-[#f5f5f7] dark:bg-[#1c1c1e] border-0 rounded-lg focus:ring-2 focus:ring-blue-500 text-gray-900 dark:text-white placeholder-gray-500 dark:placeholder-gray-400"
              placeholder="请输入直播间 ID 或链接"
            />
          </div>

          <div class="flex justify-end space-x-3">
            <button
              class="px-4 py-2 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-600 rounded-lg transition-colors"
              on:click={() => {
                addModal = false;
              }}
            >
              取消
            </button>
            <button
              class="px-4 py-2 bg-[#0A84FF] hover:bg-[#0A84FF]/90 text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={!addRoom}
              on:click={() => {
                addNewRecorder(addRoom, selectedPlatform);
                addModal = false;
                addRoom = "";
              }}
            >
              添加
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if archiveModal}
  <div
    class="fixed inset-0 bg-black/20 dark:bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center"
    transition:fade={{ duration: 200 }}
  >
    <div
      class="mac-modal archive-modal w-[900px] bg-white dark:bg-[#323234] rounded-xl shadow-xl overflow-hidden flex flex-col max-h-[80vh]"
      transition:scale={{ duration: 150, start: 0.95 }}
    >
      <!-- Header -->
      <div
        class="flex justify-between items-center px-6 py-4 border-b border-gray-200 dark:border-gray-700/50"
      >
        <div class="flex items-center space-x-3">
          <h2 class="text-base font-medium text-gray-900 dark:text-white">
            直播间记录
          </h2>
          <span class="text-sm text-gray-500 dark:text-gray-400">
            {archiveRoom?.user_info.user_name} · {archiveRoom?.room_info
              .room_id}
          </span>
        </div>
        <button
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700/50 transition-colors"
          on:click={() => (archiveModal = false)}
        >
          <X class="w-5 h-5 dark:icon-white" />
        </button>
      </div>

      <div
        class="flex-1 overflow-auto custom-scrollbar-light"
        on:scroll={handleScroll}
      >
        <div class="p-6">
          <div class="overflow-x-auto custom-scrollbar-light">
            <table class="w-full">
              <thead>
                <tr class="border-b border-gray-200 dark:border-gray-700/50">
                  <th
                    class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400"
                    >直播时间</th
                  >
                  <th
                    class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400"
                    >标题</th
                  >
                  <th
                    class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400"
                    >时长</th
                  >
                  <th
                    class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400"
                    >大小</th
                  >
                  <th
                    class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400"
                    >码率</th
                  >
                  <th
                    class="px-4 py-3 text-left text-sm font-medium text-gray-500 dark:text-gray-400"
                    >操作</th
                  >
                </tr>
              </thead>
              <tbody class="divide-y divide-gray-200 dark:divide-gray-700/50">
                {#each archives as archive (archive.parent_id + ":" + archive.live_id)}
                  <tr
                    class="group hover:bg-[#f5f5f7] dark:hover:bg-[#3a3a3c] transition-colors"
                  >
                    <td class="px-4 py-3">
                      <div class="flex flex-col">
                        <span class="text-sm text-gray-900 dark:text-white"
                          >{format_ts(archive.created_at).split(" ")[0]}</span
                        >
                        <span class="text-xs text-gray-500 dark:text-gray-400"
                          >{format_ts(archive.created_at).split(" ")[1]}</span
                        >
                      </div>
                    </td>
                    <td class="px-4 py-3">
                      <div class="flex items-center space-x-3">
                        <img
                          src={archive.cover}
                          alt="cover"
                          class="w-12 h-8 rounded object-cover"
                        />
                        <span class="text-sm text-gray-900 dark:text-white"
                          >{archive.title}</span
                        >
                      </div>
                    </td>
                    <td class="px-4 py-3 text-sm text-gray-900 dark:text-white"
                      >{format_duration(archive.length)}</td
                    >
                    <td class="px-4 py-3 text-sm text-gray-900 dark:text-white"
                      >{format_size(archive.size)}</td
                    >
                    <td
                      class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400"
                      >{calc_bitrate(archive.size, archive.length)} Kbps</td
                    >
                    <td class="px-4 py-3">
                      <div class="flex items-center space-x-2">
                        <button
                          class="p-1.5 rounded-lg hover:bg-blue-500/10 transition-colors"
                          title="预览录播"
                          on:click={() => {
                            invoke("open_live", {
                              platform: archiveRoom.room_info.platform,
                              roomId: archiveRoom.room_info.room_id,
                              liveId: archive.live_id,
                            });
                          }}
                        >
                          <PlayIcon class="w-4 h-4 icon-primary" />
                        </button>
                        <button
                          class="p-1.5 rounded-lg hover:bg-blue-500/10 transition-colors"
                          title="生成完整切片"
                          on:click={() => {
                            openGenerateWholeClipModal(archive);
                          }}
                        >
                          <FileVideo class="w-4 h-4 icon-primary" />
                        </button>
                        <button
                          class="p-1.5 rounded-lg hover:bg-red-500/10 transition-colors"
                          title="删除记录"
                          on:click={() => {
                            if (!window.confirm(`确定要删除录播“${archive.title || archive.live_id}”吗？此操作无法撤销。`)) return;
                            invokeSensitive("delete_archive", {
                              platform: archiveRoom.room_info.platform,
                              roomId: archiveRoom.room_info.room_id,
                              liveId: archive.live_id,
                            })
                              .then(async () => {
                                // 删除后重新加载第一页数据
                                currentPage = 0;
                                archives = [];
                                hasMore = true;
                                loadError = "";
                                await updateArchives();
                              })
                              .catch((e) => {
                                alert(e);
                              });
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

          <!-- 加载更多区域 -->
          <div class="mt-6 flex justify-center">
            {#if loadError}
              <div class="text-red-500 dark:text-red-400 text-sm mb-3">
                {loadError}
                <button
                  class="ml-2 text-blue-500 hover:text-blue-600 underline"
                  on:click={() => {
                    loadError = "";
                    loadMoreArchives();
                  }}
                >
                  重试
                </button>
              </div>
            {/if}

            {#if isLoading && currentPage === 0}
              <div
                class="flex items-center space-x-2 text-gray-500 dark:text-gray-400"
              >
                <div
                  class="animate-spin rounded-full h-5 w-5 border-b-2 border-blue-500"
                ></div>
                <span>加载中...</span>
              </div>
            {:else if isLoading}
              <div
                class="flex items-center space-x-2 text-gray-500 dark:text-gray-400"
              >
                <div
                  class="animate-spin rounded-full h-4 w-4 border-b-2 border-blue-500"
                ></div>
                <span>加载更多...</span>
              </div>
            {:else if archives.length === 0 && !isLoading}
              <div class="text-gray-500 dark:text-gray-400 text-sm">
                暂无录制记录
              </div>
            {/if}
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Generate Whole Clip Modal -->
<GenerateWholeClipModal
  bind:showModal={generateWholeClipModal}
  archive={generateWholeClipArchive}
  roomId={generateWholeClipArchive?.room_id || ""}
  platform={generateWholeClipArchive?.platform || ""}
  on:generated={handleWholeClipGenerated}
/>

<svelte:window on:mousedown={handleModalClickOutside} />

<style>
  /* macOS style toggle switch */
  .toggle-switch {
    position: relative;
    display: inline-block;
    width: 36px;
    height: 20px;
  }

  .toggle-switch input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .toggle-slider {
    position: absolute;
    cursor: pointer;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(120, 120, 128, 0.32);
    transition: 0.2s;
    border-radius: 20px;
  }

  .toggle-slider:before {
    position: absolute;
    content: "";
    height: 16px;
    width: 16px;
    left: 2px;
    bottom: 2px;
    background-color: white;
    transition: 0.2s;
    border-radius: 50%;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  input:checked + .toggle-slider {
    background-color: #34c759;
  }

  input:checked + .toggle-slider:before {
    transform: translateX(16px);
  }

  @keyframes spin-slow {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .animate-spin-slow {
    animation: spin-slow 3s linear infinite;
  }
</style>
