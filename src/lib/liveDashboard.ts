export type LiveDashboardMetrics = {
  paymentAmountFen: number;
  perThousandPaymentAmountFen?: number | null;
  viewerCount?: number | null;
  averageOnline?: number | null;
  averageWatchSeconds?: number | null;
  viewerConversionRate?: number | null;
  dealBuyerCount?: number | null;
  dealItemCount?: number | null;
  productClickConversionRate?: number | null;
  exposureViewerRate?: number | null;
  qianchuanSpendFen?: number | null;
};

export type LiveDashboardBindingSummary = {
  liveId: string;
  sessionId: number;
  matchMethod: string;
  accountKey: string;
  shopName: string;
  startedAt: string;
  paymentAmountFen: number;
  dealItemCount?: number | null;
};

export type ArchiveIdentitySource = {
  title?: string;
  anchorName?: string;
};

export type ArchiveIdentityDisplay = {
  primary: string;
  secondary: string;
  hasDashboard: boolean;
};

const unavailable = "— / 官方导出未提供";

function money(value?: number | null) {
  return value == null
    ? unavailable
    : `¥${(value / 100).toLocaleString("zh-CN", { maximumFractionDigits: 2 })}`;
}

function count(value?: number | null) {
  return value == null ? unavailable : value.toLocaleString("zh-CN");
}

function ratio(value?: number | null) {
  return value == null ? unavailable : `${(value * 100).toFixed(2)}%`;
}

function duration(value?: number | null) {
  return value == null
    ? unavailable
    : `${Math.floor(value / 60)}分${value % 60}秒`;
}

export function formatDashboardSessionTime(startedAt: string): string {
  const date = new Date(startedAt);
  if (Number.isNaN(date.getTime())) return startedAt;
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export function formatDashboardSessionLength(startedAt: string, endedAt?: string | null): string {
  if (!endedAt) return "";
  const start = new Date(startedAt).getTime();
  const end = new Date(endedAt).getTime();
  if (Number.isNaN(start) || Number.isNaN(end) || end <= start) return "";
  const totalMin = Math.round((end - start) / 60000);
  const hours = Math.floor(totalMin / 60);
  const minutes = totalMin % 60;
  return hours > 0 ? `${hours}小时${minutes}分` : `${minutes}分钟`;
}

export function dashboardCandidateLabel(candidate: {
  session: { startedAt: string; endedAt?: string; shopName?: string; accountKey?: string; sourceFile?: string };
}): string {
  const session = candidate.session;
  const length = formatDashboardSessionLength(session.startedAt, session.endedAt);
  const shop = session.shopName || session.accountKey || "";
  const file = session.sourceFile ? session.sourceFile.split(/[/\\]/).pop() : "";
  return [formatDashboardSessionTime(session.startedAt), length, shop, file]
    .filter(Boolean)
    .join(" · ");
}

export function formatArchiveDashboardIdentity(
  archive: ArchiveIdentitySource,
  binding?: LiveDashboardBindingSummary | null
): ArchiveIdentityDisplay {
  const anchorName = archive.anchorName?.trim();
  if (binding) {
    const primary = binding.shopName.trim() || anchorName || archive.title?.trim() || "未知账号";
    const secondaryParts = [
      binding.accountKey,
      formatDashboardSessionTime(binding.startedAt),
      money(binding.paymentAmountFen),
      binding.dealItemCount != null ? `${binding.dealItemCount} 件成交` : null,
      anchorName && anchorName !== binding.shopName ? `主播 ${anchorName}` : null,
    ].filter(Boolean);
    return {
      primary,
      secondary: secondaryParts.join(" · "),
      hasDashboard: true,
    };
  }

  return {
    primary: anchorName || "未识别账号",
    secondary: anchorName ? "未绑定直播经营数据" : "可在录播分析页绑定罗盘数据",
    hasDashboard: false,
  };
}

export function dashboardMetricCards(metrics: LiveDashboardMetrics) {
  return [
    { label: "直播间用户支付金额", value: money(metrics.paymentAmountFen) },
    { label: "千次观看用户支付金额", value: money(metrics.perThousandPaymentAmountFen) },
    { label: "直播间观看人数", value: count(metrics.viewerCount) },
    { label: "平均在线人数", value: count(metrics.averageOnline) },
    { label: "直播间观看-成交率", value: ratio(metrics.viewerConversionRate) },
    { label: "人均观看时长", value: duration(metrics.averageWatchSeconds) },
    { label: "成交人数", value: count(metrics.dealBuyerCount) },
    { label: "成交件数", value: count(metrics.dealItemCount) },
    { label: "直播间商品点击-成交率", value: ratio(metrics.productClickConversionRate) },
    { label: "直播间曝光-观看率(人数)", value: ratio(metrics.exposureViewerRate) },
    { label: "千川消耗", value: money(metrics.qianchuanSpendFen) },
  ];
}

export type DashboardVisualHero = {
  key: string;
  label: string;
  value: string;
  hint: string;
};

export type DashboardVisualRate = {
  key: string;
  label: string;
  value: string;
  /** 0–100 for bar width; null when official export missing. */
  percent: number | null;
};

/** Compact visual groups for the company analysis data board. */
export function dashboardVisualGroups(metrics: LiveDashboardMetrics): {
  heroes: DashboardVisualHero[];
  rates: DashboardVisualRate[];
  secondary: Array<{ label: string; value: string }>;
} {
  const rateOrNull = (value?: number | null): number | null => {
    if (value == null || !Number.isFinite(value)) return null;
    return Math.round(Math.max(0, Math.min(100, value * 100)) * 100) / 100;
  };

  return {
    heroes: [
      {
        key: "gmv",
        label: "支付金额",
        value: money(metrics.paymentAmountFen),
        hint: "直播间用户支付",
      },
      {
        key: "viewers",
        label: "观看人数",
        value: count(metrics.viewerCount),
        hint: "进房人数",
      },
      {
        key: "buyers",
        label: "成交人数",
        value: count(metrics.dealBuyerCount),
        hint: `${count(metrics.dealItemCount)} 件`,
      },
      {
        key: "ad",
        label: "千川消耗",
        value: money(metrics.qianchuanSpendFen),
        hint: "投放花费",
      },
    ],
    rates: [
      {
        key: "viewerConv",
        label: "观看→成交",
        value: ratio(metrics.viewerConversionRate),
        percent: rateOrNull(metrics.viewerConversionRate),
      },
      {
        key: "clickConv",
        label: "点击→成交",
        value: ratio(metrics.productClickConversionRate),
        percent: rateOrNull(metrics.productClickConversionRate),
      },
      {
        key: "exposure",
        label: "曝光→观看",
        value: ratio(metrics.exposureViewerRate),
        percent: rateOrNull(metrics.exposureViewerRate),
      },
    ],
    secondary: [
      { label: "千次观看支付", value: money(metrics.perThousandPaymentAmountFen) },
      { label: "平均在线", value: count(metrics.averageOnline) },
      { label: "人均观看", value: duration(metrics.averageWatchSeconds) },
    ],
  };
}

export type LiveDataBoardSession = LiveDashboardMetrics & {
  id: number;
  accountKey: string;
  shopName: string;
  startedAt: string;
  endedAt?: string;
  sourceFile: string;
};

export type LiveDataBoardCandidate = {
  session: LiveDataBoardSession;
  timeDeltaSeconds: number;
  accountMatch: boolean;
  shopNameMatch: boolean;
};

export type LiveDataBoardOrderSummary = {
  eventCount: number;
  totalPayAmountYuan: number;
  peakMinuteLabel: string;
  sourceLabel: string;
};
