export type NasArchiveStatus =
  | "local_only"
  | "pending"
  | "uploading"
  | "archived"
  | "retry_wait"
  | "failed";

export interface VideoArchiveRow {
  id: number;
  videoId: number;
  sourceKind: string;
  status: NasArchiveStatus;
  localPath: string;
  nasPath: string;
  uploadedBytes: number;
  retryCount: number;
  lastError: string;
  nextRetryAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface NasArchiveView {
  label: string;
  detail: string;
  tone: "neutral" | "info" | "success" | "warning" | "danger";
  canRetry: boolean;
}

export function friendlyNasError(reason: unknown): string {
  return String(reason)
    .replace(/^Error:\s*/i, "")
    .replace(/^Failed to invoke [^:]+:\s*/i, "");
}

export function normalizeNasArchiveView(
  archive?: VideoArchiveRow,
): NasArchiveView {
  if (!archive || archive.status === "local_only") {
    return {
      label: "仅保存在本机",
      detail: "尚未加入 NAS 转存队列",
      tone: "neutral",
      canRetry: false,
    };
  }

  switch (archive.status) {
    case "pending":
      return {
        label: "等待存入 NAS",
        detail: "视频已安全保存在本机，将按顺序自动转存",
        tone: "info",
        canRetry: false,
      };
    case "uploading":
      return {
        label: "正在存入 NAS",
        detail: "转存完成并校验通过前不会删除本机文件",
        tone: "info",
        canRetry: false,
      };
    case "archived":
      return {
        label: "已存入 NAS",
        detail: "文件已完成大小和媒体时长校验",
        tone: "success",
        canRetry: false,
      };
    case "retry_wait":
      return {
        label: "NAS 暂时无法连接",
        detail: "视频仍安全保留在本机，连接恢复后会自动重试",
        tone: "warning",
        canRetry: true,
      };
    case "failed":
      return {
        label: "转存失败",
        detail: "视频仍安全保留在本机，请检查 NAS 设置后重试",
        tone: "danger",
        canRetry: true,
      };
  }
}
