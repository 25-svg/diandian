export type KnowledgeStatus = {
  connected: boolean;
  vaultPath: string;
  status: "disconnected" | "ready" | "degraded";
  lastSyncedAt: string | null;
  activeCount: number;
  eligibleCount: number;
  errorCount: number;
};

export type KnowledgeSyncSummary = {
  vaultPath: string;
  inserted: number;
  updated: number;
  unchanged: number;
  deactivated: number;
  eligibleCount: number;
  errorCount: number;
  syncedAt: string;
};

export type VaultInspection = {
  path: string;
  valid: boolean;
  hasObsidianConfig: boolean;
  markdownCount: number;
  missingDirectories: string[];
};

export type KnowledgeStatusPresentation = {
  tone: "neutral" | "success" | "warning";
  label: string;
  detail: string;
};

export function knowledgeStatusPresentation(status: KnowledgeStatus): KnowledgeStatusPresentation {
  if (!status.connected) {
    return {
      tone: "neutral",
      label: "未连接",
      detail: "选择 Obsidian 知识库文件夹后即可同步",
    };
  }
  if (status.status === "degraded" || status.errorCount > 0) {
    return {
      tone: "warning",
      label: `有 ${status.errorCount} 个文件需要处理`,
      detail: `已同步 ${status.activeCount} 张卡片，${status.eligibleCount} 张可用于正式检索`,
    };
  }
  return {
    tone: "success",
    label: "知识库已就绪",
    detail: `已同步 ${status.activeCount} 张卡片，${status.eligibleCount} 张可用于正式检索`,
  };
}

export function knowledgeSyncSummaryText(summary: KnowledgeSyncSummary): string {
  return `同步完成：新增 ${summary.inserted}，更新 ${summary.updated}，未变化 ${summary.unchanged}，停用 ${summary.deactivated}`;
}

export function friendlyKnowledgeError(error: unknown): string {
  let message = "";
  if (typeof error === "string") {
    message = error;
  } else if (error instanceof Error) {
    message = error.message;
  } else if (error && typeof error === "object" && "message" in error) {
    const value = (error as { message?: unknown }).message;
    message = typeof value === "string" ? value : "";
  }

  const normalized = message.toLowerCase();
  if (normalized.includes("permission") || normalized.includes("denied") || message.includes("权限")) {
    return "知识库无法读取，请检查文件夹权限后重试";
  }
  if (normalized.includes("not found") || normalized.includes("does not exist") || message.includes("找不到")) {
    return "找不到知识库文件夹，请重新选择";
  }
  if (message && message !== "[object Object]") {
    return message;
  }
  return "知识库操作失败，请稍后重试";
}
