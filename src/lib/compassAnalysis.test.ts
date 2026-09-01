import assert from "node:assert/strict";
import {
  analyzeCompassMetricWindow,
  compassAnalysisRequestKey,
  compassChartPercent,
  compassChartTicks,
  compassEvidenceExplanation,
  compassPointOffsetSeconds,
  analyzeCompassPoint,
  normalizeCompassMetricValues,
  normalizeCompassShopName,
  compassSessionKeyMatches,
  selectCaptureForSession,
  type CompassCaptureSummary,
  type CompassMetricAnalysis,
} from "./compassAnalysis.js";

const captures: CompassCaptureSummary[] = [
  {
    captureId: "wrong-shop",
    targetDate: "2026-08-27",
    targetShopName: "科创店",
    status: "completed",
    message: "",
    startedAt: "2026-08-27T12:00:00Z",
    analysisResponseCount: 10,
    metricLabels: ["在线人数"],
    sessionKeys: ["科创店|2026-08-27T10:39:00+08:00|2026-08-27T12:00:00+08:00"],
  },
  {
    captureId: "matching",
    targetDate: "2026-08-27",
    targetShopName: "相机店",
    status: "completed",
    message: "",
    startedAt: "2026-08-27T13:00:00Z",
    analysisResponseCount: 20,
    metricLabels: ["在线人数"],
    sessionKeys: ["相机店|2026-08-27T10:39:12+08:00|2026-08-27T16:45:00+08:00"],
  },
];

const session = { id: 7, shopName: "相机店", startedAt: "2026-08-27T10:39:00+08:00" };
assert.equal(selectCaptureForSession(captures, session)?.captureId, "matching");
assert.equal(compassSessionKeyMatches(session, captures[1].sessionKeys[0]), true);
assert.equal(compassSessionKeyMatches(session, captures[0].sessionKeys[0]), false);
assert.equal(selectCaptureForSession([
  { ...captures[1], captureId: "empty", status: "completed", analysisResponseCount: 0 },
  { ...captures[1], captureId: "partial", status: "failed", analysisResponseCount: 4 },
], session)?.captureId, "partial");
assert.equal(normalizeCompassShopName(" 金典拍拍相机专卖店微单相机专场 "), "金典拍拍相机专卖店");
assert.equal(normalizeCompassShopName("金典拍拍科创专卖店直播场"), "金典拍拍科创专卖店");
assert.equal(compassSessionKeyMatches(
  { shopName: "金典拍拍相机专卖店微单相机专场", startedAt: "2026-08-27T10:39:00+08:00" },
  "金典拍拍相机专卖店|2026-08-27T10:39:12+08:00|2026-08-27T16:45:00+08:00",
), true);
assert.equal(compassSessionKeyMatches(
  { shopName: "金典拍拍相机专卖店", startedAt: "2026-08-29T10:24:15" },
  "金典拍拍相机专卖店|2026-08-29T08:37:00+08:00|2026-08-29T16:00:00+08:00",
), true);
assert.equal(compassSessionKeyMatches(
  { shopName: "金典拍拍相机专卖店", startedAt: "2026-08-29T16:01:00" },
  "金典拍拍相机专卖店|2026-08-29T08:37:00+08:00|2026-08-29T16:00:00+08:00",
), false);
assert.equal(selectCaptureForSession([
  {
    ...captures[1],
    captureId: "containing-official-session",
    targetDate: "2026-08-29",
    targetShopName: "金典拍拍相机专卖店",
    status: "failed",
    sessionKeys: ["金典拍拍相机专卖店|2026-08-29T08:37:00+08:00|2026-08-29T16:00:00+08:00"],
  },
], { id: 9646, shopName: "金典拍拍相机专卖店", startedAt: "2026-08-29T10:24:15" })?.captureId, "containing-official-session");

const metric: CompassMetricAnalysis = {
  label: "在线人数",
  unit: "",
  points: [
    { timeLabel: "10:00", sortValue: 0, value: 100 },
    { timeLabel: "10:01", sortValue: 60, value: 80 },
    { timeLabel: "10:02", sortValue: 120, value: 60 },
  ],
  minimum: 60,
  maximum: 100,
  average: 80,
  peakTime: "10:00",
  trendPercent: -40,
  volatility: 10,
  sourceEndpoint: "/trend",
};
assert.equal(compassChartPercent(metric, 60), 50);
assert.equal(compassChartPercent(metric, -60), 0);
assert.equal(compassChartPercent(metric, 180), 100);
assert.deepEqual(compassChartTicks(metric, 3), [
  { label: "10:00", percent: 0 },
  { label: "10:01", percent: 50 },
  { label: "10:02", percent: 100 },
]);

console.log("compassAnalysis tests passed");

const minute = (index: number) => 10 * 3600 + index * 60;
const fixtureMetric = (
  label: string,
  unit: string,
  values: Array<number | null>,
): CompassMetricAnalysis => ({
  label,
  unit,
  sourceKind: "compass_native",
  sourceEndpoint: "/compass_api/live/trend",
  points: values.map((value, index) => ({
    timeLabel: `10:${String(index).padStart(2, "0")}`,
    sortValue: minute(index),
    value: value ?? 0,
    valid: value != null,
  })),
  minimum: 0,
  maximum: 0,
  average: 0,
  peakTime: "",
  trendPercent: 0,
  volatility: 0,
});

const online = fixtureMetric("在线人数", "人", [100, 102, 98, 100, 150, 145, 70, 72, 100, null]);
const fullOnline = analyzeCompassMetricWindow(online, { start: minute(0), end: minute(9) }, 100);
assert.equal(fullOnline.validPointCount, 9);
assert.equal(fullOnline.missingPointCount, 1);
assert.equal(fullOnline.trendDirection, "stable");
assert.equal(fullOnline.trendPercent, 0);
assert.equal(fullOnline.maximum, 150);
assert.equal(fullOnline.maximumTime, "10:04");
assert.equal(fullOnline.minimum, 70);
assert.equal(fullOnline.minimumTime, "10:06");
assert.deepEqual(fullOnline.changes.map((item) => [item.timeLabel, Number(item.changePercent.toFixed(1))]), [
  ["10:04", 50],
  ["10:06", -51.7],
  ["10:08", 38.9],
]);
assert.deepEqual(fullOnline.anomalies.map((item) => [
  item.direction,
  item.startTime,
  item.endTime,
  Number((item.deviationPercent ?? 0).toFixed(1)),
]), [
  ["high", "10:04", "10:05", 50],
  ["low", "10:06", "10:07", -30],
]);

const narrowedOnline = analyzeCompassMetricWindow(online, { start: minute(5), end: minute(8) }, 100);
assert.equal(Number((narrowedOnline.trendPercent ?? 0).toFixed(1)), -31);
assert.equal(narrowedOnline.trendDirection, "falling");
assert.equal(narrowedOnline.maximum, 145);
assert.equal(narrowedOnline.maximumTime, "10:05");
assert.equal(narrowedOnline.minimum, 70);
assert.equal(narrowedOnline.minimumTime, "10:06");

const interaction = fixtureMetric("互动率", "%", [20, 21, 19, 20, 10, 11, 18, 19, 20, null]);
const interactionAnalysis = analyzeCompassMetricWindow(interaction, { start: minute(0), end: minute(9) }, 20);
assert.equal(interactionAnalysis.maximum, 21);
assert.equal(interactionAnalysis.maximumTime, "10:01");
assert.equal(interactionAnalysis.minimum, 10);
assert.equal(interactionAnalysis.minimumTime, "10:04");
assert.notEqual(interactionAnalysis.metricId, fullOnline.metricId);

const missing = analyzeCompassMetricWindow(fixtureMetric("缺失夹具", "", [10, null, 12]));
assert.equal(missing.status, "insufficient");
assert.equal(missing.validPointCount, 2);
assert.equal(missing.missingPointCount, 1);
assert.equal(missing.anomalies.length, 0);

const flat = analyzeCompassMetricWindow(fixtureMetric("平坦夹具", "", [5, 5, 5, 5, 5, 5]));
assert.equal(flat.status, "flat");
assert.equal(flat.trendPercent, 0);
assert.equal(flat.volatility, 0);
assert.equal(flat.maximumTime, "10:00");
assert.equal(flat.minimumTime, "10:00");
assert.equal(flat.changes.length, 0);
assert.equal(flat.anomalies.length, 0);

assert.deepEqual(normalizeCompassMetricValues(fixtureMetric("人数", "人", [10, 20, 30]).points), [0, 50, 100]);
assert.deepEqual(
  normalizeCompassMetricValues(fixtureMetric("金额", "元", [100, 1000, 10000]).points).map((value) => Number((value ?? 0).toFixed(4))),
  [0, 9.0909, 100],
);
assert.deepEqual(normalizeCompassMetricValues(fixtureMetric("平坦", "", [5, 5, 5]).points), [50, 50, 50]);
assert.equal(normalizeCompassMetricValues(fixtureMetric("缺失", "", [10, null, 12]).points)[1], null);

assert.notEqual(
  compassAnalysisRequestKey([fullOnline.metricId], { start: minute(0), end: minute(9) }),
  compassAnalysisRequestKey([fullOnline.metricId], { start: minute(5), end: minute(8) }),
);
assert.equal(compassEvidenceExplanation(null), "待核实：当前时间段没有逐字稿或节奏地图证据。");
assert.match(compassEvidenceExplanation({ transcriptExcerpt: "主播讲解赠品" }), /不代表因果/);

assert.equal(
  compassPointOffsetSeconds(
    { timeLabel: "10:26", sortValue: 10 * 3600 + 26 * 60 },
    "2026-08-31T08:35:00+08:00",
  ),
  6660,
);
assert.equal(
  compassPointOffsetSeconds(
    { timeLabel: "2026-08-31T10:26:00+08:00", sortValue: 0 },
    "2026-08-31T08:35:00+08:00",
  ),
  6660,
);
assert.equal(
  compassPointOffsetSeconds(
    { timeLabel: "00:05", sortValue: 300 },
    "2026-08-31T23:55:00+08:00",
  ),
  600,
);
assert.equal(compassPointOffsetSeconds({ timeLabel: "未知", sortValue: 999999 }, "未知"), null);
assert.deepEqual(analyzeCompassPoint(online, online.points[4]), {
  previousValue: 100,
  delta: 50,
  changePercent: 50,
  direction: "up",
});

console.log("compass curve acceptance fixtures passed");
