<script lang="ts">
  import { invoke } from "../../invoker";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import {
    AlertTriangle,
    CheckCircle2,
    Database,
    FolderOpen,
    Loader2,
    RefreshCw,
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

  <div class="min-h-[148px] rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-[#3c3c3e]">
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
          {#if status.connected && status.errorCount > 0}
            <p class="mt-2 flex items-center gap-1.5 text-sm text-amber-700 dark:text-amber-300">
              <AlertTriangle class="h-4 w-4" />
              <span>查看需要处理的文件：共 {status.errorCount} 个</span>
            </p>
          {/if}
        </div>

        <div class="flex shrink-0 items-center gap-2">
          {#if status.connected}
            <button class="inline-flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:text-gray-100 dark:hover:bg-gray-700" on:click={syncVault} disabled={running}>
              <RefreshCw class={running ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
              <span>重新同步</span>
            </button>
            <button class="inline-flex h-9 w-9 items-center justify-center rounded-lg border border-gray-200 text-gray-700 transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:text-gray-100 dark:hover:bg-gray-700" on:click={openVault} disabled={running} title="打开文件夹" aria-label="打开文件夹">
              <FolderOpen class="h-4 w-4" />
            </button>
          {:else}
            <button class="inline-flex h-9 items-center gap-2 rounded-lg bg-blue-600 px-4 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50" on:click={chooseVault} disabled={running}>
              {#if running}<Loader2 class="h-4 w-4 animate-spin" />{:else}<FolderOpen class="h-4 w-4" />{/if}
              <span>选择知识库</span>
            </button>
          {/if}
        </div>
      </div>

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
