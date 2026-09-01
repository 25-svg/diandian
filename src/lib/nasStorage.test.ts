import assert from "node:assert/strict";
import {
  friendlyNasError,
  normalizeNasArchiveView,
  type VideoArchiveRow,
} from "./nasStorage.js";

function archive(status: VideoArchiveRow["status"], lastError = ""): VideoArchiveRow {
  return {
    id: 1,
    videoId: 7,
    sourceKind: "recording",
    status,
    localPath: "D:\\videos\\demo.mp4",
    nasPath: status === "archived" ? "\\\\nas\\videos\\demo.mp4" : "",
    uploadedBytes: 0,
    retryCount: 0,
    lastError,
    nextRetryAt: null,
    createdAt: "2026-07-26T00:00:00Z",
    updatedAt: "2026-07-26T00:00:00Z",
  };
}

assert.deepEqual(normalizeNasArchiveView(undefined), {
  label: "仅保存在本机",
  detail: "尚未加入 NAS 转存队列",
  tone: "neutral",
  canRetry: false,
});

assert.equal(normalizeNasArchiveView(archive("pending")).label, "等待存入 NAS");
assert.equal(normalizeNasArchiveView(archive("uploading")).label, "正在存入 NAS");
assert.equal(normalizeNasArchiveView(archive("archived")).label, "已存入 NAS");

const retryView = normalizeNasArchiveView(archive("retry_wait", "network error"));
assert.equal(retryView.label, "NAS 暂时无法连接");
assert.equal(retryView.canRetry, true);
assert.match(retryView.detail, /本机/);
assert.doesNotMatch(retryView.detail, /network error/);

const failedView = normalizeNasArchiveView(archive("failed", "permission denied"));
assert.equal(failedView.label, "转存失败");
assert.equal(failedView.canRetry, true);
assert.match(failedView.detail, /本机/);

assert.equal(
  friendlyNasError(
    "Error: Failed to invoke test_nas_video_storage: NAS 共享目录不存在或当前无法访问",
  ),
  "NAS 共享目录不存在或当前无法访问",
);

console.log("nasStorage tests passed");
