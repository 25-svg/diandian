export type CompassSession = {
  shopName: string;
  title?: string;
  startedAt: string;
  endedAt: string;
  orderCount?: number | null;
  paymentAmountText?: string;
};

export const COMPASS_TARGET_SHOPS = [
  { value: "金典拍拍科创专卖店", label: "科创店" },
  { value: "金典拍拍相机专卖店", label: "相机店" },
] as const;

export type CompassQueueStatus =
  | "waiting"
  | "opening"
  | "downloading"
  | "validating"
  | "imported"
  | "skipped"
  | "failed";

export type CompassQueueItem = {
  sessionKey: string;
  session: CompassSession;
  status: CompassQueueStatus;
  message: string;
};

export type CompassProgress = {
  sessionKey: string;
  status: CompassQueueStatus;
  message?: string;
};

export function canStartCompassDownload(running: boolean): boolean {
  return !running;
}

export function normalizeCompassDate(value: string): string {
  const match = value.trim().match(/^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/);
  if (!match) throw new Error("请输入有效日期");
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(Date.UTC(year, month - 1, day));
  if (
    date.getUTCFullYear() !== year
    || date.getUTCMonth() !== month - 1
    || date.getUTCDate() !== day
  ) {
    throw new Error("请输入有效日期");
  }
  return `${year.toString().padStart(4, "0")}-${month.toString().padStart(2, "0")}-${day
    .toString()
    .padStart(2, "0")}`;
}

export function compassSessionKey(session: CompassSession): string {
  return `${session.shopName.trim()}|${session.startedAt}|${session.endedAt}`;
}

export function buildCompassQueue(
  sessions: readonly CompassSession[],
  importedSessionKeys: ReadonlySet<string> = new Set(),
  force = false,
): CompassQueueItem[] {
  const unique = new Map<string, CompassSession>();
  for (const session of sessions) unique.set(compassSessionKey(session), session);
  return [...unique.entries()]
    .sort((left, right) => left[1].startedAt.localeCompare(right[1].startedAt))
    .map(([sessionKey, session]) => ({
      sessionKey,
      session,
      status: !force && importedSessionKeys.has(sessionKey) ? "skipped" : "waiting",
      message: !force && importedSessionKeys.has(sessionKey) ? "该场次已导入" : "等待处理",
    }));
}

export function applyCompassProgress(
  queue: readonly CompassQueueItem[],
  progress: CompassProgress,
): CompassQueueItem[] {
  return queue.map((item) => item.sessionKey === progress.sessionKey
    ? { ...item, status: progress.status, message: progress.message ?? item.message }
    : item);
}

export function markCompassSessionImported(
  queue: readonly CompassQueueItem[],
  startedAt: string,
): CompassQueueItem[] {
  const importedMinute = startedAt.slice(0, 16);
  return queue.map((item) => item.session.startedAt.slice(0, 16) === importedMinute
    ? { ...item, status: "imported", message: "下载并导入成功" }
    : item);
}

export function compassStatusLabel(status: string): string {
  return ({
    waiting: "等待处理",
    opening: "正在打开详情",
    downloading: "正在下载",
    validating: "正在校验并导入",
    imported: "已导入",
    skipped: "已跳过",
    failed: "失败",
    filtering: "正在查询日期",
    "manual-filter-needed": "需要手动选择日期",
    "waiting-for-sessions": "等待场次列表",
    "sessions-found": "已找到场次",
    "waiting-for-download-button": "等待下载按钮",
    "login-required": "等待扫码登录",
    "switching-shop": "正在切换店铺",
    "shop-unavailable": "无店铺权限",
    "shop-mismatch": "店铺不一致",
    "batch-finished": "当天场次已处理完成",
  } as Record<string, string>)[status] ?? status;
}
