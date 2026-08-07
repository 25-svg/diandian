<script lang="ts">
  import { invoke } from "../../invoker";
  import type { NasVideoStorageConfig } from "../../interface";
  import { friendlyNasError } from "../../nasStorage";
  import {
    AlertTriangle,
    CheckCircle2,
    Database,
    Loader2,
    Save,
    Wifi,
  } from "lucide-svelte";

  export let settings: NasVideoStorageConfig;

  let testing = false;
  let saving = false;
  let message = "";
  let error = "";

  async function testConnection(): Promise<void> {
    testing = true;
    message = "";
    error = "";
    try {
      await invoke("test_nas_video_storage", { rootPath: settings.root_path });
      message = "连接成功，可以写入该共享文件夹。";
    } catch (reason) {
      error = friendlyNasError(reason);
    } finally {
      testing = false;
    }
  }

  async function saveSettings(): Promise<void> {
    saving = true;
    message = "";
    error = "";
    try {
      await invoke("update_nas_video_storage", { settings });
      message = settings.enabled
        ? "NAS 视频存储已启用。新完成的视频会自动转存。"
        : "NAS 视频存储已停用。视频将继续保存在本机。";
    } catch (reason) {
      error = friendlyNasError(reason);
    } finally {
      saving = false;
    }
  }
</script>

<section class="space-y-4">
  <div class="flex items-center gap-2">
    <Database class="h-5 w-5 text-gray-700" />
    <div>
      <h2 class="text-lg font-medium text-gray-900">NAS 视频存储</h2>
      <p class="text-sm text-gray-500">录制和导入先保存在本机，完整校验后再清理本机视频。</p>
    </div>
  </div>

  <div class="mac-card divide-y divide-[color:var(--mac-separator)]">
    <div class="flex items-center justify-between gap-6 p-4">
      <div>
        <strong class="text-sm font-medium text-gray-900">自动存入 NAS</strong>
        <p class="mt-1 text-sm text-gray-500">NAS 断开时保留本机文件，恢复连接后自动重试。</p>
      </div>
      <input
        aria-label="自动存入 NAS"
        type="checkbox"
        class="h-5 w-5 rounded border-gray-300"
        bind:checked={settings.enabled}
      />
    </div>

    <div class="space-y-3 p-4">
      <label class="block text-sm font-medium text-gray-900" for="nas-root">
        NAS 共享文件夹
      </label>
      <div class="flex gap-2">
        <input
          id="nas-root"
          type="text"
          class="mac-field min-w-0 flex-1"
          placeholder="\\绿联设备名\直播录像"
          bind:value={settings.root_path}
        />
        <button
          type="button"
          class="mac-btn"
          disabled={testing || !settings.root_path.trim()}
          on:click={testConnection}
        >
          {#if testing}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Wifi class="h-4 w-4" />{/if}
          测试连接
        </button>
      </div>
      <p class="text-xs text-gray-500">使用 Windows 共享路径，例如：\\UGREEN-NAS\直播录像。账号登录由 Windows 管理。</p>
    </div>

    <div class="grid gap-3 p-4 md:grid-cols-3">
      <label class="flex items-start gap-3 text-sm text-gray-700">
        <input type="checkbox" class="mt-0.5 h-4 w-4 rounded border-gray-300" bind:checked={settings.archive_recordings} />
        <span><strong class="block font-medium text-gray-900">自动录播</strong>录制结束后自动转存</span>
      </label>
      <label class="flex items-start gap-3 text-sm text-gray-700">
        <input type="checkbox" class="mt-0.5 h-4 w-4 rounded border-gray-300" bind:checked={settings.archive_imports} />
        <span><strong class="block font-medium text-gray-900">导入视频</strong>导入完成后自动转存</span>
      </label>
      <label class="flex items-start gap-3 text-sm text-gray-700">
        <input type="checkbox" class="mt-0.5 h-4 w-4 rounded border-gray-300" bind:checked={settings.delete_local_after_archive} />
        <span><strong class="block font-medium text-gray-900">释放本机空间</strong>NAS 校验通过后删除本机视频</span>
      </label>
    </div>

    {#if message}
      <div class="flex items-center gap-2 bg-emerald-50 p-4 text-sm text-emerald-800">
        <CheckCircle2 class="h-4 w-4 shrink-0" />{message}
      </div>
    {/if}
    {#if error}
      <div class="flex items-start gap-2 bg-red-50 p-4 text-sm text-red-700">
        <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0" />
        <span>{error}</span>
      </div>
    {/if}

    <div class="flex justify-end p-4">
      <button
        type="button"
        class="mac-btn mac-btn-primary"
        disabled={saving}
        on:click={saveSettings}
      >
        {#if saving}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Save class="h-4 w-4" />{/if}
        保存 NAS 设置
      </button>
    </div>
  </div>
</section>
