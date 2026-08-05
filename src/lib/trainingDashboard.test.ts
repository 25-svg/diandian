import assert from "node:assert/strict";
import { buildTrainingDashboardRows } from "./trainingDashboard.js";

const rows = buildTrainingDashboardRows([
  {
    sourceKey: "video:1",
    sourceTitle: "罗雨欣晚场",
    updatedAt: "2026-07-26T10:00:00.000Z",
    candidates: [{ id: "SEG-1" }] as any,
    reviews: { "SEG-1": {} },
    masterComparisons: {
      "SEG-1": {
        comparison: {
          totalScore: 88,
          masterSectionId: 2,
          admission: "candidate_queue",
          risks: [],
        },
      },
    },
  },
  {
    sourceKey: "video:2",
    sourceTitle: "于千惠晚场",
    updatedAt: "2026-07-27T10:00:00.000Z",
    candidates: [],
    reviews: {},
    masterComparisons: {},
  },
]);

assert.equal(rows[0].title, "于千惠晚场");
assert.equal(rows[1].averageScore, 88);
assert.equal(rows[1].highScoreCount, 1);
assert.match(rows[1].headline, /已复盘 1 段/);

console.log("trainingDashboard tests passed");
