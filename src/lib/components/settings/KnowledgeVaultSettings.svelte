<script lang="ts">
  import { invoke } from "../../invoker";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import {
    AlertTriangle,
    CheckCircle2,
    Database,
    FileText,
    FolderOpen,
    Loader2,
    RefreshCw,
    ShieldAlert,
  } from "lucide-svelte";
  import {
    friendlyKnowledgeError,
    knowledgeStatusPresentation,
    knowledgeSyncSummaryText,
    type KnowledgeStatus,
    type KnowledgeSyncSummary,
    type VaultInspection,
  } from "../../knowledge";

  let loading = true;
  let running = false;
  let errorText = "";
  let status: KnowledgeStatus = {
    connected: false,
    vaultPath: "",
    status: "disconnected",
    lastSyncedAt: null,
    activeCount: 0,
    eligibleCount: 0,
    asrEligibleCount: 0,
    pendingReviewCount: 0,
    ignoredCount: 0,
    restrictedCount: 0,
    errorCount: 0,
  };
  let inspection: VaultInspection | null = null;
  let lastSummary: KnowledgeSyncSummary | null = null;
  $: presentation = knowledgeStatusPresentation(status);

  async function loadStatus(): Promise<void> {
    try {
      status = await invoke<KnowledgeStatus>("get_knowledge_status");
    } catch (error) {
      errorText = friendlyKnowledgeError(error);
    } finally {
      loading = false;
    }
  }

  async function chooseVault(): Promise<void> {
    const selected = await open({ directory: true, multiple: false });
    const vaultPath = Array.isArray(selected) ? selected[0] : selected;
    if (!vaultPath) return;
    running = true;
    errorText = "";
    lastSummary = null;
    try {
      inspection = await invoke<VaultInspection>("inspect_knowledge_vault", { vaultPath });
      if (!inspection.valid) throw new Error("所选文件夹不是可用的知识库");
      lastSummary = await invoke<KnowledgeSyncSummary>("connect_knowledge_vault", { vaultPath });
      status = await invoke<KnowledgeStatus>("get_knowledge_status");
    } catch (error) {
      errorText = friendlyKnowledgeError(error);
    } finally {
      running = false;
    }
  }

  async function syncVault(): Promise<void> {
    running = true;
    errorText = "";
    try {
      lastSummary = await invoke<KnowledgeSyncSummary>("sync_knowledge_vault");
      status = await invoke<KnowledgeStatus>("get_knowledge_status");
    } catch (error) {
      errorText = friendlyKnowledgeError(error);
    } finally {
      running = false;
    }
  }

  async function openVault(): Promise<void> {
    running = true;
    errorText = "";
    try {
      await invoke("open_knowledge_vault");
    } catch (error) {
      errorText = friendlyKnowledgeError(error);
    } finally {
      running = false;
    }
  }

  onMount(loadStatus);
</script>

<section class="space-y-4" aria-labelledby="knowledge-vault-heading">
  <h2 id="knowledge-vault-heading" class="flex items-center space-x-2 text-lg font-medium text-gray-900 dark:text-white">
    <Database class="h-5 w-5 dark:icon-white" />
    <span>Obsidian 知识库</span>
  </h2>

  <div class="mac-card min-h-[148px] p-4">
    {#if loading}
      <div class="flex min-h-[116px] items-center gap-3 text-sm text-gray-500 dark:text-gray-400">
        <Loader2 class="h-5 w-5 animate-spin" />
        <span>正在读取知识库状态</span>
      </div>
    {:else}
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            {#if presentation.tone === "success"}
              <span class="h-2.5 w-2.5 rounded-full bg-green-500"></span>
            {:else if presentation.tone === "warning"}
              <span class="h-2.5 w-2.5 rounded-full bg-amber-500"></span>
            {:else}
              <FolderOpen class="h-5 w-5 text-gray-400" />
            {/if}
            <h3 class="text-sm font-semibold text-gray-900 dark:text-white">{presentation.label}</h3>
          </div>
          <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{presentation.detail}</p>
          {#if status.connected}
            <p class="mt-2 break-all text-xs text-gray-500 dark:text-gray-400">{status.vaultPath}</p>
          {/if}
        </div>

        <div class="flex shrink-0 items-center gap-2">
          {#if status.connected}
            <button type="button" class="mac-btn" on:click={syncVault} disabled={running}>
              <RefreshCw class={running ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
              <span>重新同步</span>
            </button>
            <button type="button" class="mac-btn mac-btn-icon" on:click={openVault} disabled={running} title="打开文件夹" aria-label="打开文件夹">
              <FolderOpen class="h-4 w-4" />
            </button>
          {:else}
            <button type="button" class="mac-btn mac-btn-primary" on:click={chooseVault} disabled={running}>
              {#if running}<Loader2 class="h-4 w-4 animate-spin" />{:else}<FolderOpen class="h-4 w-4" />{/if}
              <span>选择知识库</span>
            </button>
          {/if}
        </div>
      </div>

      {#if status.connected}
        <div class="mt-4 grid gap-2 border-t border-gray-100 pt-4 sm:grid-cols-2 dark:border-gray-700">
          <div class="flex items-start gap-2 text-sm">
            <CheckCircle2 class="mt-0.5 h-4 w-4 shrink-0 text-blue-600" />
            <div>
              <p class="font-medium text-gray-800 dark:text-gray-100">可用于 ASR 纠错：{status.asrEligibleCount} 张</p>
              <p class="text-xs text-gray-500 dark:text-gray-400">公司确认过的产品名、型号和替换词会自动参与转写纠错</p>
            </div>
          </div>
          <div class="flex items-start gap-2 text-sm">
            <CheckCircle2 class="mt-0.5 h-4 w-4 shrink-0 text-green-600" />
            <div>
              <p class="font-medium text-gray-800 dark:text-gray-100">可正式检索：{status.eligibleCount} 张</p>
              <p class="text-xs text-gray-500 dark:text-gray-400">已审核，可用于直播复盘和辅稿生成</p>
            </div>
          </div>
          <div class="flex items-start gap-2 text-sm">
            <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
            <div>
              <p class="font-medium text-gray-800 dark:text-gray-100">待复盘检索审核：{status.pendingReviewCount} 张</p>
              <p class="text-xs text-gray-500 dark:text-gray-400">仅影响复盘模型检索，不影响已确认参数卡用于 ASR 纠错</p>
            </div>
          </div>
          {#if status.ignoredCount > 0}
            <div class="flex items-start gap-2 text-sm">
              <FileText class="mt-0.5 h-4 w-4 shrink-0 text-gray-400" />
              <div>
                <p class="font-medium text-gray-800 dark:text-gray-100">说明文档：{status.ignoredCount} 篇</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">README 和目录索引，无需处理</p>
              </div>
            </div>
          {/if}
          {#if status.restrictedCount > 0}
            <div class="flex items-start gap-2 text-sm">
              <ShieldAlert class="mt-0.5 h-4 w-4 shrink-0 text-orange-500" />
              <div>
                <p class="font-medium text-gray-800 dark:text-gray-100">受限内容：{status.restrictedCount} 张</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">含个人或受限信息，正文不会进入索引</p>
              </div>
            </div>
          {/if}
          {#if status.errorCount > 0}
            <div class="flex items-start gap-2 text-sm text-red-700 dark:text-red-300">
              <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0" />
              <div>
                <p class="font-medium">格式问题：{status.errorCount} 个</p>
                <p class="text-xs">需要补全字段、修复 YAML 或处理重复 ID</p>
              </div>
            </div>
          {/if}
        </div>
      {/if}

      {#if inspection && inspection.missingDirectories.length > 0}
        <div class="mt-4 border-t border-gray-100 pt-3 text-sm text-gray-600 dark:border-gray-700 dark:text-gray-300">
          <p>缺少目录：{inspection.missingDirectories.join("、")}</p>
          <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">当前阶段只读取，不会自动创建目录</p>
        </div>
      {/if}
      {#if lastSummary}
        <p class="mt-4 flex items-center gap-2 text-sm text-green-700 dark:text-green-300">
          <CheckCircle2 class="h-4 w-4" />
          <span>{knowledgeSyncSummaryText(lastSummary)}</span>
        </p>
      {/if}
      {#if errorText}
        <p class="mt-4 flex items-center gap-2 text-sm text-red-600 dark:text-red-300">
          <AlertTriangle class="h-4 w-4" />
          <span>{errorText}</span>
        </p>
      {/if}
    {/if}
  </div>
</section>
