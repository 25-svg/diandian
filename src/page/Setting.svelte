<script lang="ts">
  import { invoke, TAURI_ENV, normalizeEndpoint } from "../lib/invoker";
  import { open } from "@tauri-apps/plugin-dialog";
  import { clickOutside } from "../lib/actions/clickOutside";
  import KnowledgeVaultSettings from "../lib/components/settings/KnowledgeVaultSettings.svelte";
  import NasVideoStorageSettings from "../lib/components/settings/NasVideoStorageSettings.svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";

  import type { Config } from "../lib/interface";
  import {
    Bell,
    HardDrive,
    AlertTriangle,
    FileText,
    Captions,
    DiscAlbum,
    SquareBottomDashedScissors,
  } from "lucide-svelte";
  import { onDestroy, onMount } from "svelte";

  let setting_model: Config = {
    cache: "",
    output: "",
    primary_uid: 0,
    live_start_notify: true,
    live_end_notify: true,
    clip_notify: true,
    post_notify: true,
    auto_cleanup: true,
    auto_subtitle: false,
    subtitle_generator_type: "funasr",
    openai_api_endpoint: "",
    openai_api_key: "",
    volcengine_api_key: "",
    volcengine_app_id: "",
    volcengine_access_token: "",
    volcengine_resource_id: "volc.seedasr.auc",
    volcengine_boosting_table_id: "",
    volcengine_correct_table_id: "",
    admin_mode: false,
    powerlive_key: "",
    knowledge_vault_path: "",
    whisper_model: "",
    whisper_prompt: "",
    clip_name_format: "",
    auto_generate: {
      enabled: false,
      encode_danmu: false,
    },
    nas_video_storage: {
      enabled: false,
      root_path: "",
      archive_recordings: true,
      archive_imports: true,
      delete_local_after_archive: true,
    },
    autostart_enabled: true,
    startup_wizard_completed: false,
    status_check_interval: 30, // 默认30秒
    whisper_language: "",


    webhook_url: "",
    danmu_ass_options: {
      font_size: 36,
      opacity: 0.8,
    },
  };

  let showModal = false;
  let show_clip_name_help = false;
  let cacheChanging = false;
  let outputChanging = false;
  let storageMessage = "";
  let storageError = "";
  type StorageRuntimeStatus = {
    preferredCache: string;
    activeCache: string;
    usingFallback: boolean;
    preferredAvailable: boolean;
    detail: string;
  };
  let storageRuntimeStatus: StorageRuntimeStatus = {
    preferredCache: "",
    activeCache: "",
    usingFallback: false,
    preferredAvailable: false,
    detail: "",
  };
  let storageRuntimePoll: ReturnType<typeof setInterval> | null = null;
  let endpoint = localStorage.getItem("endpoint") || "";
  let endpointValue = endpoint;

  function handleEndpointChange() {
    endpointValue = normalizeEndpoint(endpointValue);
    localStorage.setItem("endpoint", endpointValue);
    // reload page
    location.reload();
  }

  async function get_config() {
    let config: Config = await invoke("get_config");
    setting_model = config;
  }

  async function browse_folder(defaultPath?: string) {
    const selected = await open({
      directory: true,
      defaultPath: defaultPath?.trim() || undefined,
    });
    return Array.isArray(selected) ? selected[0] : selected;
  }

  function formatStorageError(reason: unknown): string {
    return String(reason)
      .replace(/^Error:\s*/i, "")
      .replace(/^Failed to invoke [^:]+:\s*/i, "");
  }

  function validateStorageFolder(path: string, label: string): string | null {
    if (!path.trim()) {
      return `${label}不能为空`;
    }
    if (path.includes("#recycle")) {
      return `${label}不能选择 NAS 回收站 (#recycle)，请先新建正常文件夹`;
    }
    return null;
  }

  async function syncStorageMigrationStatus() {
    try {
      const status = await invoke<{ cache: boolean; output: boolean }>(
        "get_storage_migration_status",
      );
      cacheChanging = status.cache;
      outputChanging = status.output;
    } catch {
      cacheChanging = false;
      outputChanging = false;
    }
  }

  async function update_notify() {
    await invoke("update_notify", {
      liveStartNotify: setting_model.live_start_notify,
      liveEndNotify: setting_model.live_end_notify,
      clipNotify: setting_model.clip_notify,
      postNotify: setting_model.post_notify,
    });
  }

  async function syncStorageRuntimeStatus(autoRecover = false) {
    try {
      storageRuntimeStatus = await invoke<StorageRuntimeStatus>(
        "get_storage_runtime_status",
      );
      if (
        autoRecover
        && storageRuntimeStatus.usingFallback
        && storageRuntimeStatus.preferredAvailable
        && !cacheChanging
        && !outputChanging
      ) {
        await restorePreferredCache(true);
      }
    } catch {
      storageRuntimeStatus = {
        preferredCache: setting_model.cache,
        activeCache: setting_model.cache,
        usingFallback: false,
        preferredAvailable: false,
        detail: "",
      };
    }
  }

  async function restorePreferredCache(automatic = false) {
    if (
      cacheChanging
      || outputChanging
      || !storageRuntimeStatus.preferredCache
    ) {
      return;
    }
    cacheChanging = true;
    storageError = "";
    storageMessage = automatic
      ? "首选缓存路径已恢复，正在自动迁回临时缓存…"
      : "正在迁回首选缓存路径…";
    try {
      await invoke("set_cache_path", {
        cachePath: storageRuntimeStatus.preferredCache,
      });
      await get_config();
      storageMessage = "已恢复首选缓存路径";
    } catch (error) {
      storageMessage = "";
      storageError = `恢复首选缓存路径失败：${formatStorageError(error)}`;
    } finally {
      cacheChanging = false;
      await syncStorageRuntimeStatus(false);
    }
  }

  async function update_autostart() {
    try {
      await invoke("update_autostart_enabled", { enabled: setting_model.autostart_enabled });
    } catch (error) {
      setting_model.autostart_enabled = !setting_model.autostart_enabled;
      alert(`开机自启设置失败：${formatStorageError(error)}`);
    }
  }

  async function handleCacheChange() {
    showModal = true;
  }

  async function handleOutputChange() {
    if (outputChanging || cacheChanging) {
      return;
    }
    storageMessage = "";
    storageError = "";
    const new_folder = await browse_folder(setting_model.output);
    if (!new_folder) {
      return;
    }
    const validationError = validateStorageFolder(new_folder, "切片保存路径");
    if (validationError) {
      storageError = validationError;
      return;
    }
    outputChanging = true;
    try {
      await invoke("set_output_path", {
        outputPath: new_folder,
      });
      setting_model.output = new_folder;
      storageMessage = "切片保存路径已更新";
    } catch (e) {
      storageError = formatStorageError(e);
    } finally {
      outputChanging = false;
    }
  }

  async function handleLogFolder() {
    await invoke("open_log_folder");
  }

  async function confirmChange() {
    if (cacheChanging || outputChanging) {
      return;
    }
    showModal = false;
    storageMessage = "";
    storageError = "";
    const new_folder = await browse_folder(setting_model.cache);
    if (!new_folder) {
      return;
    }
    const validationError = validateStorageFolder(new_folder, "缓存路径");
    if (validationError) {
      storageError = validationError;
      return;
    }
    cacheChanging = true;
    try {
      await invoke("set_cache_path", {
        cachePath: new_folder,
      });
      setting_model.cache = new_folder;
      storageMessage = "缓存路径已更新";
    } catch (e) {
      storageError = formatStorageError(e);
    } finally {
      cacheChanging = false;
      await syncStorageRuntimeStatus(false);
    }
  }

  async function update_subtitle_setting() {
    await invoke("update_subtitle_setting", {
      autoSubtitle: setting_model.auto_subtitle,
    });
  }

  async function update_status_check_interval() {
    if (setting_model.status_check_interval < 10) {
      setting_model.status_check_interval = 10; // 最小值为10秒
    }
    await invoke("update_status_check_interval", {
      interval: setting_model.status_check_interval,
    });
  }

  async function update_webhook_url() {
    await invoke("update_webhook_url", {
      webhookUrl: setting_model.webhook_url,
    });
  }

  async function handleWhisperModelPathChange() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Whisper Model", extensions: ["bin"] }],
    });
    if (selected) {
      setting_model.whisper_model = Array.isArray(selected) ? selected[0] : selected;
      await invoke("update_whisper_model", { whisperModel: setting_model.whisper_model });
    }
  }

  async function update_danmu_ass_options() {
    await invoke("update_danmu_ass_options", {
      fontSize: setting_model.danmu_ass_options.font_size,
      opacity: setting_model.danmu_ass_options.opacity,
    });
  }

  onMount(async () => {
    await get_config();
    await syncStorageMigrationStatus();
    await syncStorageRuntimeStatus(true);
    storageRuntimePoll = setInterval(() => {
      void syncStorageRuntimeStatus(true);
    }, 10_000);
  });

  onDestroy(() => {
    if (storageRuntimePoll) {
      clearInterval(storageRuntimePoll);
      storageRuntimePoll = null;
    }
  });
</script>

<PageShell title="设置" subtitle="管理系统偏好、通知、存储与字幕相关选项。">
      <!-- Settings Sections -->
      <div class="space-y-6 pb-6">
        <div class="space-y-4">
          <h2
            class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
          >
            <FileText class="w-5 h-5 dark:icon-white" />
            <span>基础设置</span>
          </h2>
          <div
            class="mac-card divide-y divide-[color:var(--mac-separator)]"
          >
            <div class="p-4">
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="text-sm font-medium text-gray-900 dark:text-white">
                    Windows 开机自动运行
                  </h3>
                  <p class="text-sm text-gray-500 dark:text-gray-400">
                    登录 Windows 后自动进入托盘，后台检测直播并优先保证录制
                  </p>
                </div>
                <label class="relative inline-block w-11 h-6">
                  <input
                    type="checkbox"
                    class="peer opacity-0 w-0 h-0"
                    bind:checked={setting_model.autostart_enabled}
                    on:change={update_autostart}
                  />
                  <span
                    class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                  ></span>
                </label>
              </div>
            </div>
            <div class="p-4">
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="text-sm font-medium text-gray-900 dark:text-white">
                    直播间状态检查间隔
                  </h3>
                  <p class="text-sm text-gray-500 dark:text-gray-400">
                    设置直播间状态检查的时间间隔，单位为秒，过于频繁可能会触发风控
                  </p>
                </div>
                <div class="flex items-center space-x-2">
                  <input
                    type="number"
                    class="mac-field w-24"
                    bind:value={setting_model.status_check_interval}
                    on:blur={update_status_check_interval}
                  />
                </div>
              </div>
            </div>
            <div class="p-4">
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="text-sm font-medium text-gray-900 dark:text-white">
                    Webhook URL
                  </h3>
                  <p class="text-sm text-gray-500 dark:text-gray-400">
                    设置 Webhook URL，用于接收事件通知，见<a
                      href="https://bsr.xinrea.cn/usage/features/webhook.html"
                      class="text-[color:var(--mac-blue)] hover:opacity-80"
                      target="_blank">Webhook 文档</a
                    >
                  </p>
                </div>
                <div class="flex items-center space-x-2">
                  <input
                    type="text"
                    class="mac-field w-96"
                    bind:value={setting_model.webhook_url}
                    on:change={update_webhook_url}
                    placeholder="https://example.com/webhook"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
        <!-- API Server Settings -->
        {#if !TAURI_ENV}
          <div class="space-y-4">
            <h2
              class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
            >
              <FileText class="w-5 h-5 dark:icon-white" />
              <span>API 服务器配置</span>
            </h2>
            <div
              class="mac-card divide-y divide-[color:var(--mac-separator)]"
            >
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      API 服务器地址
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      设置 API 服务器的地址
                    </p>
                  </div>
                  <div class="flex items-center space-x-2">
                    <input
                      type="text"
                      class="mac-field w-96"
                      bind:value={endpointValue}
                      on:blur={handleEndpointChange}
                      placeholder="http://localhost:3000"
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>
        {/if}

        {#if TAURI_ENV || endpoint != ""}
          <!-- Storage Settings -->
          {#if TAURI_ENV}
            <div class="space-y-4">
              <h2
                class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
              >
                <HardDrive class="w-5 h-5 dark:icon-white" />
                <span>存储设置</span>
              </h2>
              <div
                class="mac-card divide-y divide-[color:var(--mac-separator)]"
              >
                {#if cacheChanging || outputChanging}
                  <div class="p-4 text-sm text-blue-700 bg-blue-50" role="status" aria-live="polite">
                    {#if cacheChanging && outputChanging}
                      缓存与切片目录正在迁移，请勿关闭程序。
                    {:else if cacheChanging}
                      缓存目录正在迁移，请勿关闭程序。
                    {:else}
                      切片目录正在迁移（D 盘 → NAS 可能较慢），请勿关闭程序。
                    {/if}
                  </div>
                {/if}
                {#if storageRuntimeStatus.usingFallback}
                  <div class="p-4 bg-amber-50 text-amber-900" role="status" aria-live="polite" aria-atomic="true">
                    <div class="flex items-start justify-between gap-4">
                      <div class="flex items-start gap-3 min-w-0">
                        <AlertTriangle class="w-5 h-5 mt-0.5 flex-shrink-0 text-amber-600" aria-hidden="true" />
                        <div class="space-y-1 min-w-0">
                          <p class="text-sm font-semibold">正在使用本机临时缓存</p>
                          <p class="text-sm">{storageRuntimeStatus.detail}</p>
                          <p class="text-xs break-all">
                            首选路径：{storageRuntimeStatus.preferredCache}
                          </p>
                          <p class="text-xs break-all">
                            当前使用：{storageRuntimeStatus.activeCache}
                          </p>
                        </div>
                      </div>
                      <button
                        type="button"
                        class="mac-btn flex-shrink-0"
                        disabled={!storageRuntimeStatus.preferredAvailable || cacheChanging || outputChanging}
                        title={storageRuntimeStatus.preferredAvailable ? "将临时缓存迁回首选路径" : "首选路径当前仍不可用"}
                        on:click={() => restorePreferredCache(false)}
                      >
                        {storageRuntimeStatus.preferredAvailable ? "立即恢复" : "等待恢复"}
                      </button>
                    </div>
                  </div>
                {/if}
                {#if storageMessage}
                  <div class="p-4 text-sm text-emerald-700 bg-emerald-50">
                    {storageMessage}
                  </div>
                {/if}
                {#if storageError}
                  <div class="p-4 text-sm text-red-700 bg-red-50">
                    {storageError}
                  </div>
                {/if}
                <!-- Cache Location -->
                <div class="p-4">
                  <div class="flex items-center justify-between">
                    <div>
                      <h3
                        class="text-sm font-medium text-gray-900 dark:text-white"
                      >
                        首选缓存路径
                      </h3>
                      <p class="text-sm text-gray-500 dark:text-gray-400">
                        {setting_model.cache}
                      </p>
                    </div>
                    <button
                      class="mac-btn"
                      disabled={cacheChanging || outputChanging}
                      on:click={handleCacheChange}
                    >
                      {cacheChanging ? "迁移中..." : "变更"}
                    </button>
                  </div>
                </div>
                <div class="p-4">
                  <div class="flex items-center justify-between">
                    <div>
                      <h3
                        class="text-sm font-medium text-gray-900 dark:text-white"
                      >
                        切片保存路径
                      </h3>
                      <p class="text-sm text-gray-500 dark:text-gray-400">
                        {setting_model.output}
                      </p>
                    </div>
                    <button
                      class="mac-btn"
                      disabled={cacheChanging || outputChanging}
                      on:click={handleOutputChange}
                    >
                      {outputChanging ? "迁移中..." : "变更"}
                    </button>
                  </div>
                </div>
                <div class="p-4">
                  <div class="flex items-center justify-between">
                    <div>
                      <h3
                        class="text-sm font-medium text-gray-900 dark:text-white"
                      >
                        日志文件夹
                      </h3>
                      <p class="text-sm text-gray-500 dark:text-gray-400">
                        查看应用程序日志文件
                      </p>
                    </div>
                    <button
                      class="mac-btn"
                      on:click={handleLogFolder}
                    >
                      打开
                    </button>
                  </div>
                </div>
              </div>
            </div>
            <NasVideoStorageSettings settings={setting_model.nas_video_storage} />
            <KnowledgeVaultSettings />
          {/if}

          <!-- Notification Settings -->
          <div class="space-y-4">
            <h2
              class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
            >
              <Bell class="w-5 h-5 dark:icon-white" />
              <span>通知设置</span>
            </h2>
            <div
              class="mac-card divide-y divide-[color:var(--mac-separator)]"
            >
              <!-- Stream Start -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      直播开始通知
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      当直播间开始直播时，会收到通知
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      bind:checked={setting_model.live_start_notify}
                      on:change={update_notify}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      下播通知
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      当直播间结束直播时，会收到通知
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      bind:checked={setting_model.live_end_notify}
                      on:change={update_notify}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      切片完成通知
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      当切片完成时，会收到通知
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      bind:checked={setting_model.clip_notify}
                      on:change={update_notify}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      投稿完成通知
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      当投稿完成时，会收到通知
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      bind:checked={setting_model.post_notify}
                      on:change={update_notify}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
            </div>
          </div>

          <!-- Subtitle Generation Settings -->
          <div class="space-y-4">
            <h2
              class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
            >
              <Captions class="w-5 h-5 dark:icon-white" />
              <span>字幕生成</span>
            </h2>
            <div
              class="mac-card divide-y divide-[color:var(--mac-separator)]"
            >
              <!-- Auto Subtitle Generation -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      自动生成字幕
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      启用后，切片完成后会自动生成字幕
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      bind:checked={setting_model.auto_subtitle}
                      on:change={update_subtitle_setting}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
              <!-- Subtitle Generator Type -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      字幕生成器类型
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      选择字幕生成的方式：本地模型，OpenAI 服务或 <a
                        href="https://www.powerlive.io/"
                        class="text-[color:var(--mac-blue)] hover:underline"
                        target="_blank"
                        rel="noopener noreferrer">PowerLive</a
                      > 服务（按量付费）
                    </p>
                  </div>
                  <div class="flex items-center space-x-2">
                    {#if setting_model.admin_mode}
                    <select
                      class="mac-field"
                      bind:value={setting_model.subtitle_generator_type}
                      on:change={async () => {
                        try {
                          await invoke("update_subtitle_generator_type", {
                            subtitleGeneratorType:
                              setting_model.subtitle_generator_type,
                          });
                        } catch (error) {
                          console.error(error);
                        }
                      }}
                    >
                      <option value="funasr">本地 FunASR（推荐，Whisper 自动兜底）</option>
                      <option value="whisper">本地 Whisper（备用）</option>
                      <option value="volcengine">火山录音文件识别 2.0（推荐）</option>
                      <option value="whisper_online">在线 Whisper API</option>
                      <option value="powerlive">PowerLive</option>
                    </select>
                    {:else}
                      <span class="px-3 py-2 rounded-lg bg-green-50 text-green-700 dark:bg-green-900/20 dark:text-green-300 text-sm">
                        本地 FunASR 优先（Whisper 自动兜底）
                      </span>
                    {/if}
                  </div>
                </div>
              </div>
              <!-- Whisper Model Path -->
              {#if setting_model.admin_mode}
              {#if setting_model.subtitle_generator_type === "volcengine"}
                <div class="p-4 space-y-3 border-t border-gray-100 dark:border-gray-700">
                  <div>
                    <h3 class="text-sm font-medium text-gray-900 dark:text-white">火山录音文件识别 2.0</h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      已固定使用 Seed ASR 2.0 标准版。新控制台填写 API Key；旧控制台填写 App ID 和 Access Token。密钥留空表示保留已保存值。
                    </p>
                  </div>
                  <div class="grid grid-cols-1 gap-3 max-w-2xl">
                    <input type="password" class="mac-field" bind:value={setting_model.volcengine_api_key} placeholder="API Key（新控制台，可选）" />
                    <input type="text" class="mac-field" bind:value={setting_model.volcengine_app_id} placeholder="App ID（旧控制台）" />
                    <input type="password" class="mac-field" bind:value={setting_model.volcengine_access_token} placeholder="Access Token（旧控制台）" />
                    <div class="mac-field flex items-center text-sm text-[color:var(--mac-secondary)]">
                      模型版本：录音文件识别 2.0（volc.seedasr.auc）
                    </div>
                    <input type="text" class="mac-field" bind:value={setting_model.volcengine_boosting_table_id} placeholder="热词表 ID（boosting_table_id）" />
                    <input type="text" class="mac-field" bind:value={setting_model.volcengine_correct_table_id} placeholder="替换词表 ID（correct_table_id）" />
                    <button class="mac-btn mac-btn-primary w-fit"
                      on:click={async () => {
                        await invoke("update_volcengine_asr_config", {
                          apiKey: setting_model.volcengine_api_key,
                          appId: setting_model.volcengine_app_id,
                          accessToken: setting_model.volcengine_access_token,
                          resourceId: setting_model.volcengine_resource_id,
                          boostingTableId: setting_model.volcengine_boosting_table_id,
                          correctTableId: setting_model.volcengine_correct_table_id,
                        });
                        setting_model.volcengine_api_key = "";
                        setting_model.volcengine_access_token = "";
                        alert("火山录音文件识别 2.0 配置已保存");
                      }}>保存火山 ASR 配置</button>
                  </div>
                </div>
              {/if}
              {#if setting_model.subtitle_generator_type === "powerlive"}
                <div class="p-4">
                  <div class="flex items-center justify-between">
                    <div>
                      <h3
                        class="text-sm font-medium text-gray-900 dark:text-white"
                      >
                        PowerLive API 密钥
                      </h3>
                      <p class="text-sm text-gray-500 dark:text-gray-400">
                        设置 PowerLive API 的访问密钥
                      </p>
                    </div>
                    <div class="flex items-center space-x-2">
                      <input
                        type="password"
                        class="mac-field w-96"
                        bind:value={setting_model.powerlive_key}
                        on:change={async () => {
                          await invoke("update_powerlive_key", {
                            powerliveKey: setting_model.powerlive_key,
                          });
                        }}
                        placeholder="pk_..."
                      />
                    </div>
                  </div>
                </div>
              {:else}
                {#if TAURI_ENV && setting_model.subtitle_generator_type === "whisper"}
                  <div class="p-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <h3 class="text-sm font-medium text-gray-900 dark:text-white">Whisper 模型路径</h3>
                        <p class="text-sm text-gray-500 dark:text-gray-400">
                          {setting_model.whisper_model || "未设置"}
                          <span class="block mt-1 text-xs">可前往 <a href="https://huggingface.co/ggerganov/whisper.cpp/tree/main" class="text-[color:var(--mac-blue)] hover:underline" target="_blank" rel="noopener noreferrer">ggerganov/whisper.cpp</a> 下载模型文件</span>
                        </p>
                      </div>
                      <button class="mac-btn" on:click={handleWhisperModelPathChange}>变更</button>
                    </div>
                  </div>
                {/if}
                <!-- OpenAI API Settings -->
                {#if setting_model.subtitle_generator_type === "whisper_online"}
                  <div class="p-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <h3
                          class="text-sm font-medium text-gray-900 dark:text-white"
                        >
                          OpenAI API 端点
                        </h3>
                        <p class="text-sm text-gray-500 dark:text-gray-400">
                          设置 OpenAI API 的端点地址，默认为官方地址
                        </p>
                      </div>
                      <div class="flex items-center space-x-2">
                        <input
                          type="text"
                          class="mac-field w-96"
                          bind:value={setting_model.openai_api_endpoint}
                          on:change={async () => {
                            await invoke("update_openai_api_endpoint", {
                              openaiApiEndpoint:
                                setting_model.openai_api_endpoint,
                            });
                          }}
                          placeholder="https://api.openai.com/v1"
                        />
                      </div>
                    </div>
                  </div>
                  <div class="p-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <h3
                          class="text-sm font-medium text-gray-900 dark:text-white"
                        >
                          OpenAI API 密钥
                        </h3>
                        <p class="text-sm text-gray-500 dark:text-gray-400">
                          设置 OpenAI API 的访问密钥
                        </p>
                      </div>
                      <div class="flex items-center space-x-2">
                        <input
                          type="password"
                          class="mac-field w-96"
                          bind:value={setting_model.openai_api_key}
                          on:change={async () => {
                            await invoke("update_openai_api_key", {
                              openaiApiKey: setting_model.openai_api_key,
                            });
                          }}
                          placeholder="sk-..."
                        />
                      </div>
                    </div>
                  </div>
                {/if}
                {#if setting_model.subtitle_generator_type !== "powerlive"}
                  <div class="p-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <h3
                          class="text-sm font-medium text-gray-900 dark:text-white"
                        >
                          Whisper 提示词
                        </h3>
                        <p class="text-sm text-gray-500 dark:text-gray-400">
                          生成字幕时使用的提示词，尽量简洁明了，提示音频内容偏向的领域以及字幕的风格
                        </p>
                      </div>
                      <div class="flex items-center space-x-2">
                        <input
                          type="text"
                          class="mac-field w-96"
                          bind:value={setting_model.whisper_prompt}
                          on:change={async () => {
                            await invoke("update_whisper_prompt", {
                              whisperPrompt: setting_model.whisper_prompt,
                            });
                          }}
                        />
                      </div>
                    </div>
                  </div>
                {/if}
                <!-- Whisper Language -->
                <div class="p-4">
                  <div class="flex items-center justify-between">
                    <div>
                      <h3
                        class="text-sm font-medium text-gray-900 dark:text-white"
                      >
                        Whisper 语言
                      </h3>
                      <p class="text-sm text-gray-500 dark:text-gray-400">
                        生成字幕时使用的语言，默认自动识别
                      </p>
                    </div>
                    <div class="flex items-center space-x-2">
                      <input
                        type="text"
                        class="mac-field w-96"
                        bind:value={setting_model.whisper_language}
                        on:change={async () => {
                          await invoke("update_whisper_language", {
                            whisperLanguage: setting_model.whisper_language,
                          });
                        }}
                      />
                    </div>
                  </div>
                </div>
              {/if}
              {/if}
            </div>
          </div>

          <!-- Clip Name Format Settings -->
          <div class="space-y-4">
            <h2
              class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
            >
              <DiscAlbum class="w-5 h-5 dark:icon-white" />
              <span>切片文件名格式</span>
            </h2>
            <div
              class="mac-card divide-y divide-[color:var(--mac-separator)]"
            >
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      文件名格式
                    </h3>
                    <div class="flex items-center space-x-2">
                      <p class="text-sm text-gray-500 dark:text-gray-400">
                        可用标签：{"{title}"}
                        {"{platform}"}
                        {"{room_id}"}
                        {"{live_id}"}
                        {"{x}"}
                        {"{y}"}
                        {"{created_at}"}
                        {"{length}"}
                        {"{note}"}
                      </p>
                      <div
                        class="relative"
                        use:clickOutside={() => (show_clip_name_help = false)}
                        on:mouseenter={() => (show_clip_name_help = true)}
                        on:mouseleave={() => (show_clip_name_help = false)}
                      >
                        <button
                          type="button"
                          class="text-xs text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
                          on:click={() =>
                            (show_clip_name_help = !show_clip_name_help)}
                        >
                          详情
                        </button>
                        {#if show_clip_name_help}
                          <div
                            class="absolute left-0 top-6 z-20 w-96 rounded-lg border border-gray-200 bg-white p-3 text-xs text-gray-700 shadow-lg dark:border-gray-700 dark:bg-[#2c2c2e] dark:text-gray-200"
                          >
                            <div class="space-y-1">
                              <div>{"{title}"}: 直播标题</div>
                              <div>{"{platform}"}: 平台标识</div>
                              <div>{"{room_id}"}: 房间号</div>
                              <div>{"{live_id}"}: 录播 ID</div>
                              <div>{"{x}"}: 切片起始秒</div>
                              <div>{"{y}"}: 切片结束秒</div>
                              <div>{"{created_at}"}: 创建时间</div>
                              <div>{"{length}"}: 切片时长（秒）</div>
                              <div>{"{note}"}: 备注</div>
                            </div>
                          </div>
                        {/if}
                      </div>
                    </div>
                  </div>
                  <div class="flex items-center space-x-2">
                    <input
                      type="text"
                      class="mac-field w-96"
                      bind:value={setting_model.clip_name_format}
                      on:change={async () => {
                        await invoke("update_clip_name_format", {
                          clipNameFormat: setting_model.clip_name_format,
                        });
                      }}
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- Danmu Style Settings -->
          <div class="space-y-4">
            <h2
              class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
            >
              <Captions class="w-5 h-5 dark:icon-white" />
              <span>弹幕压制样式</span>
            </h2>
            <div
              class="mac-card divide-y divide-[color:var(--mac-separator)]"
            >
              <!-- Font Size -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      字体大小
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      设置弹幕字体大小
                    </p>
                  </div>
                  <div class="flex items-center space-x-2">
                    <input
                      type="number"
                      class="mac-field w-24"
                      bind:value={setting_model.danmu_ass_options.font_size}
                      on:blur={update_danmu_ass_options}
                      min="12"
                      max="72"
                      step="1"
                    />
                  </div>
                </div>
              </div>
              <!-- Opacity -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      不透明度
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      设置弹幕不透明度，范围
                      0.0-1.0，0.0为完全透明，1.0为完全不透明
                    </p>
                  </div>
                  <div class="flex items-center space-x-2">
                    <input
                      type="number"
                      class="mac-field w-24"
                      bind:value={setting_model.danmu_ass_options.opacity}
                      on:blur={update_danmu_ass_options}
                      min="0.0"
                      max="1.0"
                      step="0.1"
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- Auto Clip Settings -->
          <div class="space-y-4">
            <h2
              class="text-[15px] font-semibold tracking-tight text-[color:var(--mac-label)] flex items-center space-x-2"
            >
              <SquareBottomDashedScissors class="w-5 h-5 dark:icon-white" />
              <span>自动切片</span>
            </h2>
            <div
              class="mac-card divide-y divide-[color:var(--mac-separator)]"
            >
              <!-- Auto Clip Generation -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      整场录播生成
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      启用后，直播结束后会自动整场录播进入切片列表
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      bind:checked={setting_model.auto_generate.enabled}
                      on:change={async () => {
                        await invoke("update_auto_generate", {
                          enabled: setting_model.auto_generate.enabled,
                          encodeDanmu: setting_model.auto_generate.encode_danmu,
                        });
                      }}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
              <!-- Auto Clip Encode Danmu -->
              <div class="p-4">
                <div class="flex items-center justify-between">
                  <div>
                    <h3
                      class="text-sm font-medium text-gray-900 dark:text-white"
                    >
                      自动切片压制弹幕
                    </h3>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                      启用后，自动切片时会同时压制弹幕，会显著增加生成时间
                    </p>
                  </div>
                  <label class="relative inline-block w-11 h-6">
                    <input
                      type="checkbox"
                      class="peer opacity-0 w-0 h-0"
                      disabled
                      bind:checked={setting_model.auto_generate.encode_danmu}
                      on:change={async () => {
                        await invoke("update_auto_generate", {
                          enabled: setting_model.auto_generate.enabled,
                          encodeDanmu: setting_model.auto_generate.encode_danmu,
                        });
                      }}
                    />
                    <span
                      class="switch-slider absolute cursor-pointer top-0 left-0 right-0 bottom-0 bg-gray-300 dark:bg-gray-600 rounded-full transition-all duration-300 before:absolute before:h-4 before:w-4 before:left-1 before:bottom-1 before:bg-white before:rounded-full before:transition-all before:duration-300 peer-checked:bg-[color:var(--mac-green-solid)] peer-checked:before:translate-x-5"
                    ></span>
                  </label>
                </div>
              </div>
            </div>
          </div>
        {/if}
      </div>
</PageShell>

<!-- Modal -->
{#if showModal}
  <MacModal title="确认变更" panelClass="w-full max-w-md" closeOnBackdrop on:close={() => (showModal = false)}>
    <div class="flex items-start gap-3">
      <AlertTriangle class="w-6 h-6 text-[color:var(--mac-orange)] flex-shrink-0" />
      <div class="space-y-2 text-sm text-[color:var(--mac-secondary)]">
        <p>
          根据文件大小，可能需要耗时较长时间，迁移期间直播间会暂时移除，迁移完成后直播间会自动恢复。
        </p>
        <p class="font-semibold text-[color:var(--mac-label)]">
          迁移期间请不要关闭程序，且不要在迁移期间再次更改目录！
        </p>
        <p>确认要进行变更吗？</p>
        <p class="text-[color:var(--mac-orange)]">
          请选择 NAS 上的正常文件夹（例如 Z:\bsr-cache），不要选择 #recycle 回收站目录。
        </p>
      </div>
    </div>
    <svelte:fragment slot="actions">
      <button type="button" class="mac-btn" on:click={() => (showModal = false)}>
        取消
      </button>
      <button
        type="button"
        class="mac-btn mac-btn-primary"
        disabled={cacheChanging || outputChanging}
        on:click={confirmChange}
      >
        {cacheChanging ? "迁移中..." : "确认"}
      </button>
    </svelte:fragment>
  </MacModal>
{/if}
