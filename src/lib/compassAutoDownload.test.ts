import assert from "node:assert/strict";
import {
  applyCompassProgress,
  buildCompassQueue,
  canStartCompassDownload,
  COMPASS_TARGET_SHOPS,
  compassStatusLabel,
  compassSessionKey,
  inferCompassDateFromVideo,
  inferCompassShopFromTexts,
  markCompassSessionImported,
  normalizeCompassDate,
  type CompassSession,
} from "./compassAutoDownload.js";

assert.equal(canStartCompassDownload(false), true, "one-click download must work before querying");
assert.equal(canStartCompassDownload(true), false, "a running batch must not be started twice");
assert.equal(inferCompassShopFromTexts(["金典拍拍科创专卖店_2026-08-01"]), "金典拍拍科创专卖店");
assert.equal(inferCompassShopFromTexts(["金典拍拍相机专卖店直播"]), "金典拍拍相机专卖店");
assert.equal(
  inferCompassDateFromVideo({
    createdAt: "2026-08-12T10:02:00",
    texts: ["金典拍拍相机专卖店_2026-08-12_09-00-00.ts"],
  }),
  "2026-08-12",
);
assert.deepEqual(COMPASS_TARGET_SHOPS.map((shop) => shop.value), [
  "金典拍拍科创专卖店",
  "金典拍拍相机专卖店",
]);

assert.equal(normalizeCompassDate("2026-7-28"), "2026-07-28");
assert.equal(normalizeCompassDate("2026/07/28"), "2026-07-28");
assert.throws(() => normalizeCompassDate("2026-02-30"), /有效日期/);

const morning: CompassSession = {
  shopName: "金典拍拍相机专卖店",
  startedAt: "2026-07-28T08:15:00+08:00",
  endedAt: "2026-07-28T15:44:00+08:00",
  orderCount: 51,
  paymentAmountText: "¥23.66万",
};
const evening: CompassSession = {
  shopName: "金典拍拍相机专卖店",
  startedAt: "2026-07-28T19:30:00+08:00",
  endedAt: "2026-07-28T22:10:00+08:00",
  orderCount: 18,
  paymentAmountText: "¥6.28万",
};

assert.equal(
  compassSessionKey(morning),
  "金典拍拍相机专卖店|2026-07-28T08:15:00+08:00|2026-07-28T15:44:00+08:00",
);

const queue = buildCompassQueue(
  [evening, morning, morning],
  new Set([compassSessionKey(evening)]),
);
assert.deepEqual(queue.map((item) => item.session.startedAt), [morning.startedAt, evening.startedAt]);
assert.deepEqual(queue.map((item) => item.status), ["waiting", "skipped"]);

const forced = buildCompassQueue([evening], new Set([compassSessionKey(evening)]), true);
assert.equal(forced[0]?.status, "waiting");

const downloading = applyCompassProgress(queue, {
  sessionKey: compassSessionKey(morning),
  status: "downloading",
  message: "正在下载整场数据",
});
assert.equal(downloading[0]?.status, "downloading");
assert.equal(downloading[0]?.message, "正在下载整场数据");
assert.equal(queue[0]?.status, "waiting", "progress updates must be immutable");

const imported = markCompassSessionImported(queue, "2026-07-28T08:15:49+08:00");
assert.equal(imported[0]?.status, "imported");
assert.equal(imported[1]?.status, "skipped");
assert.equal(compassStatusLabel("waiting"), "等待处理");
assert.equal(compassStatusLabel("manual-filter-needed"), "需要手动选择日期");
assert.equal(compassStatusLabel("shop-mismatch"), "店铺不一致");

console.log("compass auto-download tests passed");
