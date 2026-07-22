import assert from "node:assert/strict";
import {
  chunkProgress,
  masterPublishGate,
  masterStatusLabel,
  supportAdmissionPresentation,
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

console.log("master script UI rules passed");
