import assert from "node:assert/strict";
import {
  friendlyKnowledgeError,
  knowledgeStatusPresentation,
  knowledgeSyncSummaryText,
} from "./knowledge.js";

assert.deepEqual(knowledgeStatusPresentation({
  connected: false, vaultPath: "", status: "disconnected", lastSyncedAt: null,
  activeCount: 0, eligibleCount: 0, errorCount: 0,
}), { tone: "neutral", label: "未连接", detail: "选择 Obsidian 知识库文件夹后即可同步" });

assert.deepEqual(knowledgeStatusPresentation({
  connected: true, vaultPath: "C:/Vault", status: "degraded", lastSyncedAt: "2026-07-22 10:00:00",
  activeCount: 12, eligibleCount: 8, errorCount: 2,
}), { tone: "warning", label: "有 2 个文件需要处理", detail: "已同步 12 张卡片，8 张可用于正式检索" });

assert.equal(knowledgeSyncSummaryText({
  vaultPath: "C:/Vault", inserted: 3, updated: 2, unchanged: 5,
  deactivated: 1, eligibleCount: 7, errorCount: 0, syncedAt: "2026-07-22 10:00:00",
}), "同步完成：新增 3，更新 2，未变化 5，停用 1");

assert.equal(friendlyKnowledgeError({ message: "permission denied" }), "知识库无法读取，请检查文件夹权限后重试");
console.log("knowledge frontend tests passed");
