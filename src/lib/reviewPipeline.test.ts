import assert from "node:assert/strict";
import { buildReviewPipeline } from "./reviewPipeline.js";

const record: any = {
  platform: "douyin",
  room_id: "room-1",
  live_id: "live-1",
  parent_id: "parent-1",
  size: 4096,
};

const processing: any = {
  id: "asr-1",
  task_type: "generate_archive_subtitle",
  status: "processing",
  message: "正在转写 2/6",
  metadata: JSON.stringify({ platform: "douyin", room_id: "room-1", live_id: "live-1" }),
  created_at: "2026-08-27T10:00:00Z",
};

let pipeline = buildReviewPipeline({
  record,
  tasks: [processing],
  hasTranscript: false,
  hasOrders: false,
  hasGeneratedClips: false,
});
assert.equal(pipeline.stages.find((stage) => stage.id === "transcript")?.status, "active");
assert.equal(pipeline.currentLabel, "转写文稿");

pipeline = buildReviewPipeline({
  record,
  tasks: [processing],
  hasTranscript: true,
  hasOrders: true,
  hasGeneratedClips: false,
});
assert.equal(pipeline.stages.find((stage) => stage.id === "analysis")?.status, "active");
assert.equal(pipeline.completed, 3);

pipeline = buildReviewPipeline({
  record,
  tasks: [],
  hasTranscript: true,
  hasOrders: true,
  hasGeneratedClips: true,
});
assert.equal(pipeline.stages.find((stage) => stage.id === "clip")?.status, "done");
assert.equal(pipeline.stages.find((stage) => stage.id === "review")?.status, "active");

const durablePipelineTask: any = {
  id: "pipeline-1",
  task_type: "auto_review_pipeline",
  status: "success",
  message: "已生成 2 条学习切片，等待主播审核",
  metadata: JSON.stringify({
    platform: "douyin",
    room_id: "room-1",
    live_id: "live-1",
    stage: "review",
    payment_events: { events: [{ offsetSec: 90 }, { offsetSec: 120 }] },
    generated_video_ids: [88, 89],
  }),
  created_at: "2026-08-27T11:00:00Z",
};
pipeline = buildReviewPipeline({
  record,
  tasks: [durablePipelineTask],
  hasTranscript: true,
  hasOrders: false,
  hasGeneratedClips: false,
});
assert.equal(pipeline.stages.find((stage) => stage.id === "orders")?.status, "done");
assert.equal(pipeline.stages.find((stage) => stage.id === "clip")?.status, "done");
assert.equal(pipeline.stages.find((stage) => stage.id === "review")?.status, "active");

const metricPipelineTask: any = {
  ...durablePipelineTask,
  id: "pipeline-metrics",
  metadata: JSON.stringify({
    platform: "douyin",
    room_id: "room-1",
    live_id: "live-1",
    pipeline_version: 2,
    stage: "review",
    payment_events: { events: [{ offsetSec: 90 }] },
    metric_analysis: {
      status: "done",
      message: "已从05直播大屏分析 8 条曲线、3 个关键下降区间",
      sessionId: 27,
      captureId: "capture-1",
      analysisId: "capture-1::session:27",
      metricCount: 8,
      declineEventCount: 3,
    },
    generated_video_ids: [88],
  }),
  created_at: "2026-08-27T12:00:00Z",
};
pipeline = buildReviewPipeline({
  record,
  tasks: [metricPipelineTask],
  hasTranscript: true,
  hasOrders: false,
  hasGeneratedClips: false,
});
assert.equal(pipeline.stages.length, 8);
assert.equal(pipeline.stages.find((stage) => stage.id === "dashboard")?.status, "done");
assert.equal(pipeline.stages.find((stage) => stage.id === "metrics")?.status, "done");
assert.match(pipeline.stages.find((stage) => stage.id === "metrics")?.detail || "", /8 条曲线/);

const missingMetricTask: any = {
  ...metricPipelineTask,
  id: "pipeline-metrics-missing",
  status: "success",
  metadata: JSON.stringify({
    platform: "douyin",
    room_id: "room-1",
    live_id: "live-1",
    pipeline_version: 2,
    stage: "review",
    metric_analysis: {
      status: "attention",
      message: "05直播大屏尚未采集本场分钟曲线",
      sessionId: 27,
    },
    generated_video_ids: [88],
  }),
  created_at: "2026-08-27T13:00:00Z",
};
pipeline = buildReviewPipeline({
  record,
  tasks: [missingMetricTask],
  hasTranscript: true,
  hasOrders: true,
  hasGeneratedClips: true,
});
assert.equal(pipeline.stages.find((stage) => stage.id === "dashboard")?.status, "done");
assert.equal(pipeline.stages.find((stage) => stage.id === "metrics")?.status, "attention");
assert.equal(pipeline.needsAttention, true);

console.log("review pipeline tests passed");
