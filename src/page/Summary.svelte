<script lang="ts">
  import { get_static_url, invoke, invokeSensitive } from "../lib/invoker";
  import type { RecorderList, DiskInfo, RecorderHealthRow } from "../lib/interface";
  import type { RecordItem, TaskRow } from "../lib/db";
  import { onDestroy, onMount } from "svelte";
  import ReviewPipelineCard from "../lib/components/ReviewPipelineCard.svelte";
  import { archiveReviewSourceKey, buildReviewPipeline, type ReviewPipelineSnapshot } from "../lib/reviewPipeline";
  import { paymentEventsStorageKey } from "../lib/orderDealTimeline";
  import { clipReviewRequestStorageKey } from "../lib/clipReview";
  const INTERVAL = 5000;
  import { scale } from "svelte/transition";
  import {
    CalendarCheck,
    Clock,
    Database,
    HardDrive,
    Play,
    RefreshCw,
    Trash2,
    Users,
    Video,
  } from "lucide-svelte";
  import PageShell from "../lib/components/PageShell.svelte";

  let summary: RecorderList = {
    count: 0,
    recorders: [],
  };

  let disk_info: DiskInfo = {
    disk: "",
    total: 0,
    free: 0,
  };

  let total = 0;
  let online = 0;
  let disk_usage = 0;
  let account_count = 0;
  let total_length = 0;
  let today_record_count = 0;
  let recent_records: RecordItem[] = [];
  let activeDropdown = null;
  let loading = false;
  let offset = 0;
  let hasMore = true;
  let hasNewRecords = false;
  let recorderHealth: RecorderHealthRow[] = [];
  let backgroundTasks: TaskRow[] = [];
  let reviewPipelines: Record<string, ReviewPipelineSnapshot> = {};
  let pipelineLoading = false;
  let retryingPipelines = new Set<string>();
  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  $: recorderIssues = recorderHealth.filter((item) =>
    ["live_waiting", "retry_wait", "needs_attention", "login_required"].includes(item.status),
  );
  $: recordingRooms = recorderHealth.filter((item) => item.status === "recording").length;
  $: startingRooms = recorderHealth.filter((item) => item.status === "recording_starting").length;
  const RECORDS_PER_PAGE = 5;

  async function update_summary() {
    summary = (await invoke("get_recorder_list")) as RecorderList;
    total = summary.count;
    online = summary.recorders.filter((r) => r.room_info.status).length;
    try {
      recorderHealth = await invoke("get_recorder_health");
    } catch (error) {
      console.warn("get_recorder_health failed:", error);
    }
    try {
      backgroundTasks = await invoke("get_tasks");
    } catch (error) {
      console.warn("get_tasks failed:", error);
    }

    disk_usage = await get_archive_disk_usage();

    // get disk info
    try {
      disk_info = await invoke("get_disk_info");
    } catch (error) {
      console.warn("get_disk_info failed:", error);
      disk_info = { disk: "", total: 0, free: 0 };
    }
    account_count = await get_account_count();

    // get total length
    total_length = await get_total_length();

    // get today record count
    today_record_count = await get_today_record_count();

    // check for new records
    if (recent_records.length > 0) {
      const latestRecords = (await invoke("get_recent_record", {
        roomId: "",
        offset: 0,
        limit: 1,
      })) as RecordItem[];

      if (
        latestRecords.length > 0 &&
        (!recent_records[0] ||
          latestRecords[0].live_id !== recent_records[0].live_id)
      ) {
        hasNewRecords = true;
      }
    } else {
      // Initial load
      await loadMoreRecords();
    }
    void updateReviewPipelines();
  }

  async function loadMoreRecords() {
    if (loading || (!hasMore && !hasNewRecords)) return;

    loading = true;
    const newRecords = (await invoke("get_recent_record", {
      roomId: "",
      offset: hasNewRecords ? 0 : offset,
      limit: RECORDS_PER_PAGE,
    })) as RecordItem[];

    for (const record of newRecords) {
      record.cover = await get_static_url(
        "cache",
        `${record.platform}/${record.room_id}/${record.live_id}/cover.jpg`,
      );
    }

    if (hasNewRecords) {
      recent_records = newRecords;
      offset = newRecords.length;
      hasNewRecords = false;
      hasMore = true;
    } else {
      if (newRecords.length < RECORDS_PER_PAGE) {
        hasMore = false;
      }
      recent_records = [...recent_records, ...newRecords];
      offset += newRecords.length;
    }

    loading = false;
    void updateReviewPipelines();
  }

  async function updateReviewPipelines() {
    if (pipelineLoading || recent_records.length === 0) return;
    pipelineLoading = true;
    try {
      const entries = await Promise.all(recent_records.slice(0, 3).map(async (record) => {
        let hasTranscript = false;
        try {
          const subtitle = await invoke<string>("get_archive_subtitle", {
            platform: record.platform,
            roomId: String(record.room_id),
            liveId: String(record.live_id),
          });
          hasTranscript = Boolean(subtitle.trim());
        } catch {
          hasTranscript = false;
        }
        const sourceKey = archiveReviewSourceKey(record);
        const hasOrders = Boolean(localStorage.getItem(paymentEventsStorageKey(sourceKey)));
        const hasGeneratedClips = Boolean(localStorage.getItem(clipReviewRequestStorageKey(sourceKey)));
        return [record.live_id, buildReviewPipeline({
          record,
          tasks: backgroundTasks,
          hasTranscript,
          hasOrders,
          hasGeneratedClips,
        })] as const;
      }));
      reviewPipelines = Object.fromEntries(entries);
    } finally {
      pipelineLoading = false;
    }
  }

  function openReview(record: RecordItem) {
    window.dispatchEvent(new CustomEvent(
      record.archive_kind === "company" ? "bsr:open-company-deal-review" : "bsr:open-archive-analysis",
      { detail: record },
    ));
  }

  async function retryReviewPipeline(record: RecordItem) {
    if (retryingPipelines.has(record.live_id)) return;
    retryingPipelines = new Set([...retryingPipelines, record.live_id]);
    try {
      await invoke("retry_auto_review_pipeline", {
        platform: record.platform,
        roomId: String(record.room_id),
        liveId: String(record.live_id),
      });
      await update_summary();
    } catch (error) {
      console.error("retry_auto_review_pipeline failed:", error);
    } finally {
      const next = new Set(retryingPipelines);
      next.delete(record.live_id);
      retryingPipelines = next;
    }
  }

  function handleScroll(event) {
    const target = event.target;
    // If we're at the top and there are new records, load them
    if (target.scrollTop === 0 && hasNewRecords) {
      loadMoreRecords();
      return;
    }

    // Otherwise check if we need to load more old records
    const bottom =
      target.scrollHeight - target.scrollTop - target.clientHeight < 50;
    if (bottom && !hasNewRecords) {
      loadMoreRecords();
    }
  }

  onMount(() => {
    void update_summary();
    refreshTimer = setInterval(() => void update_summary(), INTERVAL);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  async function get_archive_disk_usage() {
    const total_size = (await invoke("get_archive_disk_usage")) as number;
    return total_size;
  }

  async function get_total_length(): Promise<number> {
    return await invoke("get_total_length");
  }

  async function get_today_record_count(): Promise<number> {
    return await invoke("get_today_record_count");
  }

  async function get_account_count(): Promise<number> {
    return await invoke("get_account_count");
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

  function format_time(time: number) {
    time = Math.round(time);
    const hours = Math.floor(time / 3600);
    const minutes = Math.floor((time % 3600) / 60);
    const seconds = time % 60;
    // two digits
    return `${hours.toString().padStart(2, "0")}:${minutes
      .toString()
      .padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }

  // format date to YYYY-MM-DD HH:MM:SS
  function format_date(date: string) {
    return new Date(date).toLocaleString();
  }

  function toggleDropdown(id) {
    if (activeDropdown === id) {
      activeDropdown = null;
    } else {
      activeDropdown = id;
    }
  }

  function handleClickOutside(event) {
    if (
      activeDropdown !== null &&
      !event.target.closest(".dropdown-container")
    ) {
      activeDropdown = null;
    }
  }

  async function deleteRecord(record: RecordItem) {
    if (!window.confirm(`确定要删除录播“${record.title || record.live_id}”吗？此操作无法撤销。`)) {
      return;
    }
    try {
      await invokeSensitive("delete_archive", {
        platform: record.platform,
        roomId: record.room_id,
        liveId: record.live_id,
      });

      // Remove the record from the list
      recent_records = recent_records.filter(
        (r) => r.live_id !== record.live_id,
      );

      // Update stats
      disk_usage -= record.size;
      total_length -= record.length;
      if (
        new Date(record.created_at).toDateString() === new Date().toDateString()
      ) {
        today_record_count--;
      }
    } catch (error) {
      alert(error);
    }
  }

  async function refreshRecords() {
    // Reset pagination
    offset = 0;
    hasMore = true;
    recent_records = [];
    // Load records from beginning
    await loadMoreRecords();
  }
</script>

<svelte:window on:click={handleClickOutside} />

<PageShell title="总览" subtitle="查看缓存占用、直播间状态与最近录播。" on:scroll={handleScroll}>
    <section class:warning={recorderIssues.length > 0} class="recorder-health-banner">
      <div>
        <strong>{recorderIssues.length > 0 ? "录制健康需要关注" : "后台录制监控正常"}</strong>
        <p>
          {#if recorderIssues.length > 0}
            {recorderIssues.length} 个直播间等待恢复或需要处理；系统会按 1、5、30 分钟自动重试。
          {:else}
            {recordingRooms > 0
              ? `${recordingRooms} 个直播间录像文件正在持续写入`
              : startingRooms > 0
                ? `${startingRooms} 个直播间已启动录制，正在确认文件写入`
                : "程序留在托盘并持续等待开播"}
          {/if}
        </p>
      </div>
      {#if recorderIssues.length > 0}
        <div class="health-issues">
          {#each recorderIssues.slice(0, 3) as issue}
            <span title={issue.message}>{issue.roomId} · {issue.status === "needs_attention" ? "需要处理" : issue.status === "login_required" ? "重新扫码" : `自动恢复 ${issue.retryCount}/3`}</span>
          {/each}
        </div>
      {:else}
        <span class="health-ok">{recordingRooms > 0 ? "已验证录制" : startingRooms > 0 ? "正在确认" : "运行中"}</span>
      {/if}
    </section>

    <section class="review-flow" aria-labelledby="review-flow-title">
      <div class="review-flow-heading">
        <div>
          <p class="eyebrow">下播后自动处理</p>
          <h2 id="review-flow-title">一键复盘进度</h2>
          <p>主播只看这里：订单、05直播大屏指标、文稿、成交链路、切片和审核状态会持续更新。</p>
        </div>
        <button type="button" on:click={() => void updateReviewPipelines()} disabled={pipelineLoading} aria-label="刷新复盘进度">
          <RefreshCw class={pipelineLoading ? "spin" : ""} aria-hidden="true" />
          {pipelineLoading ? "刷新中" : "刷新进度"}
        </button>
      </div>
      {#if recent_records.length === 0 && !loading}
        <div class="review-flow-empty">
          <Video aria-hidden="true" />
          <strong>还没有可复盘的录播</strong>
          <span>直播结束并保存录像后，会自动出现在这里。</span>
        </div>
      {:else}
        <div class="review-flow-list">
          {#each recent_records.slice(0, 3) as record (record.live_id)}
            {#if reviewPipelines[record.live_id]}
              <ReviewPipelineCard
                {record}
                pipeline={reviewPipelines[record.live_id]}
                retrying={retryingPipelines.has(record.live_id)}
                on:open={(event) => openReview(event.detail)}
                on:retry={(event) => void retryReviewPipeline(event.detail)}
              />
            {/if}
          {/each}
        </div>
      {/if}
    </section>

    <!-- Stats Grid -->
    <div class="grid grid-cols-3 gap-6">
      <!-- Cache Size -->
      <div class="mac-card p-6 hover:border-[color:var(--mac-blue)] transition-colors">
        <div class="flex items-center space-x-3">
          <div class="p-3 rounded-[10px] bg-[color:var(--mac-blue)]">
            <HardDrive class="w-6 h-6 icon-white" />
          </div>
          <div>
            <p class="text-sm text-gray-600 dark:text-gray-400">缓存占用</p>
            <p class="text-2xl font-semibold text-gray-900 dark:text-white">
              {format_size(disk_usage)}
            </p>
          </div>
        </div>
      </div>

      <div
        class="mac-card p-6 hover:border-[color:var(--mac-blue)] transition-colors"
      >
        <div class="flex items-center space-x-3">
          <div class="p-3 rounded-lg bg-orange-500">
            <Database class="w-6 h-6 icon-white" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="flex items-baseline justify-between">
              <p class="text-sm text-gray-600 dark:text-gray-400">磁盘使用</p>
              <p class="text-xs text-gray-500 dark:text-gray-400">
                {format_size(disk_info.free)}剩余
              </p>
            </div>
            <p class="text-2xl font-semibold text-gray-900 dark:text-white">
              {format_size(disk_info.total - disk_info.free)}
            </p>
            <div
              class="w-full h-1 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden mt-1.5"
            >
              <div
                class="h-full bg-orange-500 rounded-full"
                style="width: {disk_info.total > 0
                  ? ((disk_info.total - disk_info.free) / disk_info.total) * 100
                  : 0}%"
              ></div>
            </div>
          </div>
        </div>
      </div>

      <!-- Active Rooms -->
      <div
        class="mac-card p-6 hover:border-[color:var(--mac-blue)] transition-colors"
      >
        <div class="flex items-center space-x-3">
          <div class="p-3 rounded-lg bg-green-500">
            <Video class="w-6 h-6 icon-white" />
          </div>
          <div>
            <p class="text-sm text-gray-600 dark:text-gray-400">直播间</p>
            <p class="text-2xl font-semibold text-gray-900 dark:text-white">
              {online} / {total}
            </p>
          </div>
        </div>
      </div>

      <!-- Connected Accounts -->
      <div
        class="mac-card p-6 hover:border-[color:var(--mac-blue)] transition-colors"
      >
        <div class="flex items-center space-x-3">
          <div class="p-3 rounded-lg bg-purple-500">
            <Users class="w-6 h-6 icon-white" />
          </div>
          <div>
            <p class="text-sm text-gray-600 dark:text-gray-400">账号</p>
            <p class="text-2xl font-semibold text-gray-900 dark:text-white">
              {account_count}
            </p>
          </div>
        </div>
      </div>

      <!-- Total Recording Time -->
      <div
        class="mac-card p-6 hover:border-[color:var(--mac-blue)] transition-colors"
      >
        <div class="flex items-center space-x-3">
          <div class="p-3 rounded-lg bg-indigo-500">
            <Clock class="w-6 h-6 icon-white" />
          </div>
          <div>
            <p class="text-sm text-gray-600 dark:text-gray-400">总缓存时长</p>
            <p class="text-2xl font-semibold text-gray-900 dark:text-white">
              {format_time(total_length)}
            </p>
          </div>
        </div>
      </div>

      <!-- Today's Recordings -->
      <div
        class="mac-card p-6 hover:border-[color:var(--mac-blue)] transition-colors"
      >
        <div class="flex items-center space-x-3">
          <div class="p-3 rounded-lg bg-pink-500">
            <CalendarCheck class="w-6 h-6 icon-white" />
          </div>
          <div>
            <p class="text-sm text-gray-600 dark:text-gray-400">
              今日缓存直播数
            </p>
            <p class="text-2xl font-semibold text-gray-900 dark:text-white">
              {today_record_count}
            </p>
          </div>
        </div>
      </div>
    </div>

    <!-- Recent Recordings -->
    <div class="space-y-4">
      <div class="flex justify-between items-center">
        <div class="flex items-center space-x-3">
          <h2 class="text-lg font-semibold text-gray-900 dark:text-white">
            最近的直播记录
          </h2>
          <button
            class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700/50 transition-colors text-gray-500 dark:text-gray-400"
            on:click={refreshRecords}
          >
            <RefreshCw class="w-5 h-5 dark:icon-white" />
          </button>
        </div>
        {#if hasNewRecords}
          <button
            class="px-3 py-1 text-sm text-[color:var(--mac-blue)] bg-[color:var(--mac-blue-soft)] rounded-full hover:brightness-95 transition-colors"
            on:click={loadMoreRecords}
          >
            记录有更新 • 点击刷新
          </button>
        {/if}
      </div>
      <div class="space-y-3">
        <!-- Recording Items -->
        {#each recent_records as record}
          <div
            class="p-4 rounded-[11px] bg-[color:var(--mac-bg-card)] border border-[color:var(--mac-separator)] flex items-center justify-between hover:border-[color:var(--mac-blue)] transition-colors"
          >
            <div class="flex items-center space-x-4">
              {#if record.cover}
                <img
                  src={record.cover}
                  class="w-32 h-18 rounded-lg object-cover"
                  alt="Gaming stream thumbnail"
                  on:error={(e) =>
                    console.error("Image error in template:", record.cover, e)}
                />
              {:else}
                <div
                  class="w-32 h-20 rounded-lg bg-gray-200 dark:bg-gray-700 flex items-center justify-center"
                >
                  <Video class="w-8 h-8 text-gray-400 dark:text-gray-500" />
                </div>
              {/if}
              <div>
                <h3 class="font-medium text-gray-900 dark:text-white">
                  {record.title}
                </h3>
                <p class="text-sm text-gray-600 dark:text-gray-400">
                  {record.platform} • {record.room_id} • {format_date(
                    record.created_at,
                  )} • {format_size(record.size)}
                </p>
              </div>
            </div>
            <div class="flex items-center space-x-2">
              <button
                class="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700"
                on:click={() => {
                  invoke("open_live", {
                    platform: record.platform,
                    roomId: record.room_id,
                    liveId: record.live_id,
                  });
                }}
              >
                <Play class="w-5 h-5 dark:icon-white" />
              </button>
              <div class="relative dropdown-container">
                <button
                  class="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 text-red-600 dark:text-red-400"
                  on:click|stopPropagation={() =>
                    toggleDropdown(record.live_id)}
                >
                  <Trash2 class="w-5 h-5 icon-danger" />
                </button>
                {#if activeDropdown === record.live_id}
                  <div
                    class="absolute right-0 mt-2 w-48 rounded-lg shadow-lg bg-white dark:bg-[#3c3c3e] border border-gray-200 dark:border-gray-700 backdrop-blur-xl bg-opacity-90 dark:bg-opacity-90 z-50"
                    style="transform-origin: top right;"
                    in:scale={{ duration: 100, start: 0.95 }}
                    out:scale={{ duration: 100, start: 0.95 }}
                  >
                    <div
                      class="px-4 py-3 border-b border-gray-200 dark:border-gray-700"
                    >
                      <h3
                        class="text-sm font-medium text-gray-900 dark:text-white"
                      >
                        确认删除
                      </h3>
                      <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                        此操作无法撤销
                      </p>
                    </div>
                    <div class="p-2 flex space-x-2">
                      <button
                        class="flex-1 px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700/50 rounded-md transition-colors"
                        on:click={() => {
                          activeDropdown = null;
                        }}
                      >
                        取消
                      </button>
                      <button
                        class="flex-1 px-3 py-1.5 text-sm text-white bg-red-600 hover:bg-red-700 rounded-md transition-colors"
                        on:click={() => {
                          deleteRecord(record);
                          activeDropdown = null;
                        }}
                      >
                        删除
                      </button>
                    </div>
                  </div>
                {/if}
              </div>
            </div>
          </div>
        {/each}

        {#if loading}
          <div class="flex justify-center py-4">
            <div
              class="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"
            ></div>
          </div>
        {/if}

        {#if !hasMore && recent_records.length > 0}
          <div class="text-center py-4 text-gray-500 dark:text-gray-400">
            没有更多记录了
          </div>
        {/if}
      </div>
    </div>
</PageShell>

<style>
  .review-flow{margin-bottom:20px;padding:18px;border:1px solid var(--mac-separator);border-radius:18px;background:linear-gradient(145deg,rgba(239,246,255,.78),var(--mac-bg-card) 46%)}
  .review-flow-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px;margin-bottom:14px}.review-flow-heading .eyebrow{margin:0 0 3px;color:#1677ff;font-size:11px;font-weight:800;letter-spacing:.04em}.review-flow-heading h2{margin:0;color:var(--mac-text-primary);font-size:18px;font-weight:760}.review-flow-heading p:last-child{margin:5px 0 0;color:var(--mac-text-secondary);font-size:12px}.review-flow-heading button{min-height:40px;display:inline-flex;align-items:center;gap:7px;flex:0 0 auto;padding:0 12px;border:1px solid #b9d4ff;border-radius:10px;background:#fff;color:#175cd3;font-size:12px;font-weight:750;cursor:pointer}.review-flow-heading button:disabled{cursor:not-allowed;opacity:.6}.review-flow-heading button:focus-visible{outline:3px solid rgba(22,119,255,.24);outline-offset:2px}.review-flow-heading button :global(svg){width:15px;height:15px}.review-flow-heading button :global(svg.spin){animation:summary-spin .9s linear infinite}.review-flow-list{display:grid;gap:11px}.review-flow-empty{min-height:132px;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:5px;border:1px dashed #cbd5e1;border-radius:13px;color:var(--mac-text-secondary);text-align:center}.review-flow-empty :global(svg){width:26px;height:26px;margin-bottom:3px}.review-flow-empty strong{color:var(--mac-text-primary);font-size:13px}.review-flow-empty span{font-size:11px}@keyframes summary-spin{to{transform:rotate(360deg)}}
  .recorder-health-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-bottom: 18px;
    padding: 15px 18px;
    border: 1px solid #b7ebc6;
    border-radius: 14px;
    background: linear-gradient(135deg, #f0fdf4, #effcf8);
    color: #14532d;
  }
  .recorder-health-banner.warning {
    border-color: #fed7aa;
    background: linear-gradient(135deg, #fff7ed, #fffbeb);
    color: #9a3412;
  }
  .recorder-health-banner strong { font-size: 14px; }
  .recorder-health-banner p { margin: 4px 0 0; font-size: 12px; opacity: .8; }
  .health-ok { padding: 6px 10px; border-radius: 999px; background: #dcfce7; color: #15803d; font-size: 12px; font-weight: 800; }
  .health-issues { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 7px; }
  .health-issues span { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 6px 9px; border-radius: 999px; background: rgba(255,255,255,.78); font-size: 11px; font-weight: 700; }
  :global(.dark) .recorder-health-banner { border-color: #236b43; background: #10261c; color: #bbf7d0; }
  :global(.dark) .recorder-health-banner.warning { border-color: #854d0e; background: #2b1d0b; color: #fed7aa; }
  :global(.dark) .health-issues span { background: rgba(15,23,42,.65); }
  :global(.dark) .review-flow{background:linear-gradient(145deg,rgba(23,37,84,.42),var(--mac-bg-card) 46%)}:global(.dark) .review-flow-heading button{border-color:#1e40af;background:#172554;color:#bfdbfe}:global(.dark) .review-flow-empty{border-color:#475467}
  @media (max-width: 760px) { .recorder-health-banner { align-items: flex-start; flex-direction: column; } .health-issues { justify-content: flex-start; }.review-flow-heading{flex-direction:column}.review-flow-heading button{width:100%;justify-content:center} }
  @media(prefers-reduced-motion:reduce){.review-flow-heading button :global(svg.spin){animation:none}}
</style>
