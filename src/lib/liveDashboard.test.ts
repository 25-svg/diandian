import assert from "node:assert/strict";
import { dashboardMetricCards } from "./liveDashboard.js";

assert.deepEqual(
  dashboardMetricCards({
    paymentAmountFen: 23655200,
    viewerConversionRate: 0.0068,
    averageWatchSeconds: 65,
  }),
  [
    { label: "直播间用户支付金额", value: "¥236,552" },
    { label: "直播间观看-成交率", value: "0.68%" },
    { label: "人均观看时长", value: "1分5秒" },
  ],
);
