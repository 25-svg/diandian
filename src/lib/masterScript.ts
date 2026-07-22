export type MasterSourceStatus =
  | "queued"
  | "transcribing"
  | "reviewing"
  | "structuring"
  | "ready"
  | "failed";

export type MasterSectionKind = "opening" | "product" | "transition" | "scenario" | "closing";
export type CandidateAdmission = "blocked" | "analysis_only" | "review_only" | "candidate_queue";

export type MasterChunk = {
  id: number;
  sourceId: number;
  chunkIndex: number;
  startMs: number;
  endMs: number;
  status: "pending" | "running" | "complete" | "failed";
  error: string | null;
};

export type MasterSource = {
  id: number;
  sourceKind: "video" | "archive";
  sourceKey: string;
  durationMs: number;
  status: MasterSourceStatus;
  error: string | null;
};

export type MasterSectionDraft = {
  position: number;
  sectionKey: string;
  kind: MasterSectionKind;
  productCardId: string | null;
  sourceCueIds: number[];
  sourceStartMs: number;
  sourceEndMs: number;
  hostText: string;
  masterText: string;
  textOrigin: "host_speech" | "ai_rewrite_candidate";
  conditions: string[];
  dynamicFields: Array<{
    name: string;
    value: string;
    sourceCueIds: number[];
    confirmed: boolean;
  }>;
};

export type MasterDraft = { title: string; sections: MasterSectionDraft[] };
export type MasterBlockingIssue = { code: string; message: string; sectionKey: string | null };
export type MasterPreview = {
  sourceId: number;
  scriptKey: string;
  draft: MasterDraft;
  validation: { publishable: boolean; blockingIssues: MasterBlockingIssue[] };
};

export type MasterIngestResponse = {
  sourceId: number;
  status: { status: "complete" | "failed"; completed: number; total: number; failedChunk?: number; error?: string };
};

export function masterStatusLabel(status: MasterSourceStatus): string {
  return ({
    queued: "等待生成母稿",
    transcribing: "正在生成母稿逐字稿",
    reviewing: "逐字稿等待确认",
    structuring: "正在按商品整理母稿",
    ready: "母稿可以预览",
    failed: "母稿生成中断",
  } satisfies Record<MasterSourceStatus, string>)[status];
}

export function chunkProgress(completed: number, total: number): number {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((completed / total) * 100)));
}

export function masterPublishGate(input: {
  pendingCriticalCount: number;
  blockingIssues: MasterBlockingIssue[];
}): { allowed: boolean; reason: string } {
  if (input.pendingCriticalCount > 0) {
    return { allowed: false, reason: "请先确认逐字稿中的关键内容" };
  }
  if (input.blockingIssues.length > 0) {
    return { allowed: false, reason: input.blockingIssues[0].message };
  }
  return { allowed: true, reason: "可以发布母稿 V1.0" };
}

export function supportAdmissionPresentation(
  admission: CandidateAdmission,
  total: number,
  reasons: string[] = [],
): { tone: "neutral" | "success" | "warning"; label: string; detail: string } {
  if (admission === "candidate_queue") {
    return { tone: "success", label: "已进入候选辅稿", detail: `本地复核分 ${total}，等待人工确认` };
  }
  if (admission === "blocked") {
    return { tone: "warning", label: "暂不能进入候选辅稿", detail: reasons[0] || "存在未通过的关键检查" };
  }
  if (admission === "review_only") {
    return { tone: "neutral", label: "仅保留复盘", detail: `本地复核分 ${total}，未达到 85 分` };
  }
  return { tone: "neutral", label: "仅作分析参考", detail: `本地复核分 ${total}` };
}

export function startMasterIngest(request: {
  videoId: number;
  title: string;
  recognizedTerms?: string[];
  manuallySelectedCardIds?: string[];
}) {
  return invokeCommand<MasterIngestResponse>(
    "start_master_ingest",
    { request },
  );
}

export function resumeMasterIngest(sourceId: number, request: {
  videoId: number;
  title: string;
  recognizedTerms?: string[];
  manuallySelectedCardIds?: string[];
}) {
  return invokeCommand("resume_master_ingest", { sourceId, request });
}

export function getMasterScriptStatus(sourceId: number) {
  return invokeCommand<{ source: MasterSource; chunks: MasterChunk[] }>("get_master_script_status", { sourceId });
}

export function previewMasterScript(sourceId: number, scriptKey: string, title: string) {
  return invokeCommand<MasterPreview>("preview_master_script", { request: { sourceId, scriptKey, title } });
}

export function publishMasterScript(sourceId: number, scriptKey: string, draft: MasterDraft) {
  return invokeCommand("publish_master_script", { request: { sourceId, scriptKey, draft } });
}

async function invokeCommand<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("./invoker.js");
  return invoke<T>(command, args);
}
