import assert from "node:assert/strict";
import {
  filterOperationsAnchors,
  operationsDemoAnchors,
  operationsDemoTasks,
  operationsSummary,
} from "./operationsDashboard.js";

const summary = operationsSummary(operationsDemoAnchors, operationsDemoTasks);
assert.equal(summary.liveCount, 12);
assert.equal(summary.gmvYuan, 186420);
assert.equal(summary.attentionAnchorCount, 3);
assert.equal(summary.openTaskCount, 3);

assert.deepEqual(
  filterOperationsAnchors(operationsDemoAnchors, "相机二组", "").map((item) => item.id),
  ["anchor-c", "anchor-d"],
);
assert.deepEqual(
  filterOperationsAnchors(operationsDemoAnchors, "全部团队", "价格").map((item) => item.id),
  ["anchor-b"],
);
assert.deepEqual(filterOperationsAnchors(operationsDemoAnchors, "不存在", ""), []);

console.log("operations dashboard model tests passed");
