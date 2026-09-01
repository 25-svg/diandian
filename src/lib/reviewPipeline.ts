import type { RecordItem, TaskRow } from "./db";

export type ReviewPipelineStageStatus = "done" | "active" | "waiting" | "attention";

export type ReviewPipelineStage = {
  id: "record" | "orders" | "dashboard" | "transcript" | "metrics" | "analysis" | "clip" | "review";
  label: string;
  detail: string;
  status: ReviewPipelineStageStatus;
};

export type ReviewPipelineSnapshot = {
  sourceKey: string;
  stages: ReviewPipelineStage[];
  completed: number;
  currentLabel: string;
  needsAttention: boolean;
};

export function archiveReviewSourceKey(record: Pick<RecordItem, "platform" | "room_id" | "live_id">): string {
  return `archive:${record.platform}:${record.room_id}:${record.live_id}`;
}

function parseMetadata(task: Pick<TaskRow, "metadata">): Record<string, unknown> {
  try {
    const value = JSON.parse(task.metadata || "{}");
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}

function taskBelongsToRecord(task: TaskRow, record: RecordItem): boolean {
  const metadata = parseMetadata(task);
  const liveId = String(metadata.live_id ?? metadata.liveId ?? "");
  const roomId = String(metadata.room_id ?? metadata.roomId ?? "");
  const parentId = String(metadata.parent_id ?? metadata.parentId ?? "");
  if (liveId && liveId === String(record.live_id)) {
    return !roomId || roomId === String(record.room_id);
  }
  return Boolean(parentId && parentId === String(record.parent_id));
}

function latestTask(tasks: readonly TaskRow[], types: readonly string[]): TaskRow | null {
  return [...tasks]
    .filter((task) => types.includes(task.task_type))
    .sort((left, right) => Date.parse(right.created_at) - Date.parse(left.created_at))[0] || null;
}

function statusOfTask(task: TaskRow | null): ReviewPipelineStageStatus {
  if (!task) return "waiting";
  const status = task.status.toLowerCase();
  if (["success", "completed"].includes(status)) return "done";
  if (["pending", "processing", "running", "waiting"].includes(status)) return "active";
  if (["failed", "error", "interrupted"].includes(status)) return "attention";
  return "waiting";
}

export function buildReviewPipeline(input: {
  record: RecordItem;
  tasks: readonly TaskRow[];
  hasTranscript: boolean;
  hasOrders: boolean;
  hasGeneratedClips: boolean;
}): ReviewPipelineSnapshot {
  const relatedTasks = input.tasks.filter((task) => taskBelongsToRecord(task, input.record));
  const pipelineTask = latestTask(relatedTasks, ["auto_review_pipeline"]);
  const pipelineMetadata = pipelineTask ? parseMetadata(pipelineTask) : {};
  const persistedPayments = pipelineMetadata.payment_events as Record<string, unknown> | undefined;
  const persistedEvents = persistedPayments && Array.isArray(persistedPayments.events)
    ? persistedPayments.events
    : [];
  const generatedVideoIds = Array.isArray(pipelineMetadata.generated_video_ids)
    ? pipelineMetadata.generated_video_ids
    : [];
  const metricAnalysis = pipelineMetadata.metric_analysis && typeof pipelineMetadata.metric_analysis === "object"
    ? pipelineMetadata.metric_analysis as Record<string, unknown>
    : {};
  const metricAnalysisStatus = String(metricAnalysis.status || "");
  const metricSessionId = Number(metricAnalysis.sessionId ?? metricAnalysis.session_id ?? 0);
  const metricCount = Number(metricAnalysis.metricCount ?? metricAnalysis.metric_count ?? 0);
  const declineEventCount = Number(metricAnalysis.declineEventCount ?? metricAnalysis.decline_event_count ?? 0);
  const metricMessage = String(metricAnalysis.message || "");
  const pipelineStage = String(pipelineMetadata.stage || "");
  const hasOrders = input.hasOrders || persistedEvents.length > 0;
  const hasGeneratedClips = input.hasGeneratedClips || generatedVideoIds.length > 0;
  const pipelineAttention = pipelineTask ? statusOfTask(pipelineTask) === "attention" : false;
  const transcriptTask = latestTask(relatedTasks, ["generate_archive_subtitle"]);
  const clipTask = latestTask(relatedTasks, ["deal_auto_clip_batch", "clip_video"]);
  const transcriptStatus = input.hasTranscript
    ? "done"
    : pipelineAttention && pipelineStage === "transcript"
      ? "attention"
      : pipelineStage === "transcript"
        ? "active"
        : statusOfTask(transcriptTask);
  const clipStatus = hasGeneratedClips
    ? "done"
    : pipelineAttention && pipelineStage === "clip"
      ? "attention"
      : pipelineStage === "clip"
        ? "active"
        : statusOfTask(clipTask);
  const canAnalyze = input.hasTranscript && hasOrders;
  const dashboardStatus: ReviewPipelineStageStatus = metricSessionId > 0
    ? "done"
    : metricAnalysisStatus === "attention"
      ? "attention"
      : pipelineStage === "metrics" || metricAnalysisStatus === "active"
        ? "active"
        : "waiting";
  const metricStatus: ReviewPipelineStageStatus = metricAnalysisStatus === "done"
    ? "done"
    : metricAnalysisStatus === "attention"
      ? "attention"
      : pipelineStage === "metrics" || metricAnalysisStatus === "active"
        ? "active"
        : "waiting";

  const stages: ReviewPipelineStage[] = [
    {
      id: "record",
      label: "录制完成",
      detail: input.record.size > 0 ? "录像文件已保存" : "录像文件为空，需要检查录制",
      status: input.record.size > 0 ? "done" : "attention",
    },
    {
      id: "orders",
      label: "匹配订单",
      detail: hasOrders
        ? `本场订单时间轴已保存${persistedEvents.length ? ` · ${persistedEvents.length} 笔` : ""}`
        : pipelineTask?.message || "下播后自动匹配本场订单",
      status: hasOrders
        ? "done"
        : pipelineAttention
          ? "attention"
          : pipelineTask && (!pipelineStage || pipelineStage === "orders")
            ? "active"
            : "waiting",
    },
    {
      id: "dashboard",
      label: "匹配直播指标",
      detail: metricSessionId > 0
        ? "已匹配05直播大屏对应场次"
        : metricMessage || "等待自动匹配05直播大屏场次",
      status: dashboardStatus,
    },
    {
      id: "transcript",
      label: "转写文稿",
      detail: input.hasTranscript
        ? "整场逐字稿可用"
        : transcriptTask?.message || "等待本地转写",
      status: transcriptStatus,
    },
    {
      id: "metrics",
      label: "指标诊断",
      detail: metricStatus === "done"
        ? `已分析 ${metricCount} 条曲线${declineEventCount ? ` · ${declineEventCount} 个关键下降区间` : ""}`
        : metricMessage || "等待05直播大屏曲线和逐字稿",
      status: metricStatus,
    },
    {
      id: "analysis",
      label: "识别成交链路",
      detail: hasGeneratedClips
        ? "成交链路分析已生成切片"
        : canAnalyze
          ? pipelineTask?.message || "订单和文稿已齐，后台自动分析"
          : "等待订单和文稿",
      status: hasGeneratedClips
        ? "done"
        : pipelineAttention && pipelineStage === "analysis"
          ? "attention"
          : canAnalyze || pipelineStage === "analysis"
            ? "active"
            : "waiting",
    },
    {
      id: "clip",
      label: "生成学习切片",
      detail: hasGeneratedClips ? `已生成 ${generatedVideoIds.length || ""} 条学习切片`.trim() : pipelineTask?.message || clipTask?.message || "等待成交链路分析",
      status: clipStatus,
    },
    {
      id: "review",
      label: "主播审核",
      detail: hasGeneratedClips ? "等待主播确认后进入知识库" : "切片生成后可审核",
      status: hasGeneratedClips ? "active" : "waiting",
    },
  ];
  const completed = stages.filter((stage) => stage.status === "done").length;
  const current = stages.find((stage) => stage.status === "attention")
    || stages.find((stage) => stage.status === "active")
    || stages.find((stage) => stage.status === "waiting")
    || stages[stages.length - 1];
  return {
    sourceKey: archiveReviewSourceKey(input.record),
    stages,
    completed,
    currentLabel: current.label,
    needsAttention: stages.some((stage) => stage.status === "attention"),
  };
}
