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
  | "capture-started"
  | "capturing-metric"
  | "capturing-products"
  | "capturing-product-detail"
  | "capture-session-finished"
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

export function inferCompassShopFromTexts(values: readonly (string | null | undefined)[]): string {
  return resolveCompassShopFromAccount(values) || "金典拍拍相机专卖店";
}

/** Shop identity from account/shop fields only. Ignores live titles like「富士相机专场」. */
export function resolveCompassShopFromAccount(
  values: readonly (string | null | undefined)[],
): string | null {
  const identity = values.filter(Boolean).join(" ");
  if (identity.includes("科创")) return "金典拍拍科创专卖店";
  if (identity.includes("金典拍拍相机专卖店") || identity.includes("相机专卖店")) {
    return "金典拍拍相机专卖店";
  }
  return null;
}

const NAMED_VIDEO_IDENTITY =
  /(金典拍拍科创专卖店|金典拍拍相机专卖店)_(\d{4}-\d{2}-\d{2})_(\d{2})-(\d{2})-(\d{2})/;

/** IDM / 导入规范名：店铺_开播时间，不把「富士相机专场」这类标题当店名。 */
export function parseCompassIdentityFromName(
  value?: string | null,
): { shopName: string; startedAt: string } | null {
  if (!value) return null;
  const match = value.match(NAMED_VIDEO_IDENTITY);
  if (!match) return null;
  return {
    shopName: match[1],
    startedAt: `${match[2]}T${match[3]}:${match[4]}:${match[5]}+08:00`,
  };
}

export function parseCompassIdentityFromTexts(
  values: readonly (string | null | undefined)[],
): { shopName: string; startedAt: string } | null {
  for (const value of values) {
    const parsed = parseCompassIdentityFromName(value);
    if (parsed) return parsed;
  }
  return null;
}

const padClockPart = (value: number): string => value.toString().padStart(2, "0");

/**
 * Convert an archive clock to the China-local clock used by Douyin Compass.
 * Zoned values represent an absolute instant; zone-less values are treated as China local time.
 */
export function normalizeCompassStartedAt(value?: string | null): string | null {
  const clock = value?.trim();
  if (!clock) return null;

  const local = clock.match(
    /^(\d{4})-(\d{2})-(\d{2})[T ](\d{1,2}):(\d{2})(?::(\d{2}))?(?:\.\d+)?$/,
  );
  if (local) {
    const year = Number(local[1]);
    const month = Number(local[2]);
    const day = Number(local[3]);
    const hour = Number(local[4]);
    const minute = Number(local[5]);
    const second = Number(local[6] ?? "0");
    const checked = new Date(Date.UTC(year, month - 1, day, hour, minute, second));
    if (
      checked.getUTCFullYear() !== year
      || checked.getUTCMonth() !== month - 1
      || checked.getUTCDate() !== day
      || checked.getUTCHours() !== hour
      || checked.getUTCMinutes() !== minute
      || checked.getUTCSeconds() !== second
    ) {
      return null;
    }
    return `${year.toString().padStart(4, "0")}-${padClockPart(month)}-${padClockPart(day)}`
      + `T${padClockPart(hour)}:${padClockPart(minute)}:${padClockPart(second)}+08:00`;
  }

  if (!/(?:Z|[+-]\d{2}:?\d{2})$/i.test(clock)) return null;
  const timestamp = Date.parse(clock);
  if (!Number.isFinite(timestamp)) return null;
  const chinaClock = new Date(timestamp + 8 * 60 * 60 * 1000);
  return `${chinaClock.getUTCFullYear().toString().padStart(4, "0")}`
    + `-${padClockPart(chinaClock.getUTCMonth() + 1)}-${padClockPart(chinaClock.getUTCDate())}`
    + `T${padClockPart(chinaClock.getUTCHours())}:${padClockPart(chinaClock.getUTCMinutes())}`
    + `:${padClockPart(chinaClock.getUTCSeconds())}+08:00`;
}

export function inferCompassDateFromVideo(options: {
  createdAt?: string | null;
  texts?: readonly (string | null | undefined)[];
}): string | null {
  const texts = options.texts ?? [];
  for (const value of texts) {
    if (!value) continue;
    const separated = value.match(/(20\d{2})[-_/年.](\d{1,2})[-_/月.](\d{1,2})/);
    if (separated) {
      try {
        return normalizeCompassDate(`${separated[1]}-${separated[2]}-${separated[3]}`);
      } catch {
        continue;
      }
    }
    const compact = value.match(/(20\d{2})(\d{2})(\d{2})/);
    if (compact) {
      try {
        return normalizeCompassDate(`${compact[1]}-${compact[2]}-${compact[3]}`);
      } catch {
        continue;
      }
    }
  }
  const clock = normalizeCompassStartedAt(options.createdAt);
  if (clock) return clock.slice(0, 10);
  return null;
}

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
    "opening-live-screen": "正在进入直播大屏",
    "capture-started": "开始完整采集",
    "capturing-metric": "正在采集指标曲线",
    "capturing-section": "正在采集页面模块",
    "capturing-products": "正在采集商品列表",
    "capturing-product-detail": "正在采集讲解商品",
    "capture-session-finished": "当前场次采集完成",
    "login-required": "等待扫码登录",
    "switching-shop": "正在切换店铺",
    "shop-unavailable": "无店铺权限",
    "shop-mismatch": "店铺不一致",
    "batch-finished": "当天场次已处理完成",
  } as Record<string, string>)[status] ?? status;
}
