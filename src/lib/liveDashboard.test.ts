import assert from "node:assert/strict";
import {
  dashboardMetricCards,
  dashboardVisualGroups,
  formatArchiveDashboardIdentity,
} from "./liveDashboard.js";

assert.deepEqual(
  dashboardMetricCards({
    paymentAmountFen: 23655200,
    perThousandPaymentAmountFen: 2643925,
    viewerCount: 6782,
    averageOnline: 22,
    viewerConversionRate: 0.0068,
    averageWatchSeconds: 65,
    dealBuyerCount: 46,
    dealItemCount: 51,
    productClickConversionRate: 0.0308,
    exposureViewerRate: 0.1012,
    qianchuanSpendFen: 220151,
  }),
  [
    { label: "直播间用户支付金额", value: "¥236,552" },
    { label: "千次观看用户支付金额", value: "¥26,439.25" },
    { label: "直播间观看人数", value: "6,782" },
    { label: "平均在线人数", value: "22" },
    { label: "直播间观看-成交率", value: "0.68%" },
    { label: "人均观看时长", value: "1分5秒" },
    { label: "成交人数", value: "46" },
    { label: "成交件数", value: "51" },
    { label: "直播间商品点击-成交率", value: "3.08%" },
    { label: "直播间曝光-观看率(人数)", value: "10.12%" },
    { label: "千川消耗", value: "¥2,201.51" },
  ],
);

const identity = formatArchiveDashboardIdentity(
  { title: "直播录像", anchorName: "罗雨欣" },
  {
    liveId: "1",
    sessionId: 9,
    matchMethod: "auto_account",
    accountKey: "20296833869",
    shopName: "金典拍拍相机专卖店",
    startedAt: "2026-07-28T08:15:49+08:00",
    paymentAmountFen: 23655200,
    dealItemCount: 51,
  }
);
assert.equal(identity.primary, "金典拍拍相机专卖店");
assert.match(identity.secondary, /20296833869/);
assert.match(identity.secondary, /罗雨欣/);
assert.equal(identity.hasDashboard, true);

const visual = dashboardVisualGroups({
  paymentAmountFen: 23655200,
  viewerCount: 6782,
  dealBuyerCount: 46,
  dealItemCount: 51,
  viewerConversionRate: 0.0068,
  productClickConversionRate: 0.0308,
  exposureViewerRate: null,
  qianchuanSpendFen: 220151,
});
assert.equal(visual.heroes[0]?.value, "¥236,552");
assert.equal(visual.rates[0]?.percent, 0.68);
assert.equal(visual.rates[2]?.percent, null);

console.log("live dashboard tests passed");
