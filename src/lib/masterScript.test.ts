import assert from "node:assert/strict";
import {
  chunkProgress,
  masterPublishGate,
  masterStatusLabel,
  supportAdmissionPresentation,
  candidateDecisionStatus,
  upgradePublishGate,
} from "./masterScript.js";

assert.equal(masterStatusLabel("transcribing"), "正在生成母稿逐字稿");
assert.equal(masterStatusLabel("reviewing"), "逐字稿等待确认");
assert.deepEqual(
  masterPublishGate({ pendingCriticalCount: 1, blockingIssues: [] }),
  { allowed: false, reason: "请先确认逐字稿中的关键内容" },
);
assert.deepEqual(
  masterPublishGate({ pendingCriticalCount: 0, blockingIssues: [] }),
  { allowed: true, reason: "可以发布母稿 V1.0" },
);
assert.equal(chunkProgress(6, 10), 60);
assert.equal(chunkProgress(3, 0), 0);
assert.deepEqual(supportAdmissionPresentation("candidate_queue", 85), {
  tone: "success",
  label: "已进入候选辅稿",
  detail: "本地复核分 85，等待人工确认",
});
assert.deepEqual(supportAdmissionPresentation("blocked", 100, ["价格仍待确认"]), {
  tone: "warning",
  label: "暂不能进入候选辅稿",
  detail: "价格仍待确认",
});
assert.equal(candidateDecisionStatus("通过并加入新版本"), "approved");
assert.equal(candidateDecisionStatus("保留候选"), "held");
assert.equal(candidateDecisionStatus("退回修改"), "returned");
assert.equal(candidateDecisionStatus("不采用"), "rejected");
assert.deepEqual(upgradePublishGate(false, 1), { allowed: false, reason: "请先确认母稿差异" });
assert.deepEqual(upgradePublishGate(true, 0), { allowed: false, reason: "至少选择一条已通过的候选辅稿" });
assert.deepEqual(upgradePublishGate(true, 2), { allowed: true, reason: "可以发布母稿新版本" });

console.log("master script UI rules passed");
