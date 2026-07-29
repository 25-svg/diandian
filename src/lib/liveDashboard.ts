export type LiveDashboardMetrics = {
  paymentAmountFen: number;
  viewerConversionRate?: number | null;
  averageWatchSeconds?: number | null;
};

export function dashboardMetricCards(metrics: LiveDashboardMetrics) {
  return [
    {
      label: "直播间用户支付金额",
      value: `¥${(metrics.paymentAmountFen / 100).toLocaleString("zh-CN", {
        maximumFractionDigits: 2,
      })}`,
    },
    {
      label: "直播间观看-成交率",
      value:
        metrics.viewerConversionRate == null
          ? "— / 官方导出未提供"
          : `${(metrics.viewerConversionRate * 100).toFixed(2)}%`,
    },
    {
      label: "人均观看时长",
      value:
        metrics.averageWatchSeconds == null
          ? "— / 官方导出未提供"
          : `${Math.floor(metrics.averageWatchSeconds / 60)}分${metrics.averageWatchSeconds % 60}秒`,
    },
  ];
}
