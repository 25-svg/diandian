import assert from "node:assert/strict";
import {
  friendlyKnowledgeError,
  knowledgeStatusPresentation,
  knowledgeSyncSummaryText,
} from "./knowledge.js";

assert.deepEqual(knowledgeStatusPresentation({
  connected: false, vaultPath: "", status: "disconnected", lastSyncedAt: null,
  activeCount: 0, eligibleCount: 0, asrEligibleCount: 0, pendingReviewCount: 0, ignoredCount: 0,
  restrictedCount: 0, errorCount: 0,
}), { tone: "neutral", label: "未连接", detail: "选择 Obsidian 知识库文件夹后即可同步" });

assert.deepEqual(knowledgeStatusPresentation({
  connected: true, vaultPath: "C:/Vault", status: "ready", lastSyncedAt: "2026-07-22 10:00:00",
  activeCount: 547, eligibleCount: 2, asrEligibleCount: 2, pendingReviewCount: 533, ignoredCount: 12,
  restrictedCount: 0, errorCount: 0,
}), {
  tone: "warning",
  label: "知识库已同步，可用于 ASR 纠错",
  detail: "知识卡 535 张：可用 2，ASR 纠错 2，待复盘检索审核 533；另忽略 12 篇说明文档",
});

assert.deepEqual(knowledgeStatusPresentation({
  connected: true, vaultPath: "C:/Vault", status: "degraded", lastSyncedAt: "2026-07-22 10:00:00",
  activeCount: 12, eligibleCount: 8, asrEligibleCount: 2, pendingReviewCount: 1, ignoredCount: 0,
  restrictedCount: 1, errorCount: 2,
}), {
  tone: "warning",
  label: "有 2 个格式问题需要处理",
  detail: "知识卡 12 张：可用 8，ASR 纠错 2，待复盘检索审核 1，受限 1，异常 2",
});

assert.equal(knowledgeSyncSummaryText({
  vaultPath: "C:/Vault", inserted: 3, updated: 2, unchanged: 5,
  deactivated: 1, eligibleCount: 7, asrEligibleCount: 2, pendingReviewCount: 1, ignoredCount: 1,
  restrictedCount: 0, errorCount: 0, syncedAt: "2026-07-22 10:00:00",
}), "同步完成：新增 3，更新 2，未变化 5，停用 1");

assert.equal(friendlyKnowledgeError({ message: "permission denied" }), "知识库无法读取，请检查文件夹权限后重试");
console.log("knowledge frontend tests passed");
