export type TranscriptSource =
  | { kind: "archive"; platform: string; roomId: string; liveId: string }
  | { kind: "video"; videoId: number };

export type ReviewDecision = "pending" | "approved" | "kept_original";
export type ReviewAction = "approve" | "keep_original";

export interface TranscriptCorrection {
  id: string;
  startMs: number;
  endMs: number;
  original: string;
  proposed: string;
  category: string;
  evidence: string[];
  critical: boolean;
  decision: ReviewDecision;
  decidedText: string | null;
}

export interface TranscriptAuditBundle {
  source: TranscriptSource;
  rawSrt: string;
  correctedSrt: string;
  corrections: TranscriptCorrection[];
  pendingCriticalCount: number;
}

export interface CorrectionDecisionSummary {
  id: string;
  decision: ReviewDecision;
}

export interface TimedTranscriptEntry {
  id: number;
  start: number;
  end: number;
}

export interface ResolveTranscriptCorrectionRequest {
  source: TranscriptSource;
  correctionId: string;
  action: ReviewAction;
  decidedText: string | null;
  addToDictionaryCandidates: boolean;
}

export type DictionaryCandidateType = "hotword" | "replacement" | "local_rule";
export type DictionaryCandidateStatus =
  | "pending"
  | "approved"
  | "rejected"
  | "synced"
  | "sync_failed";
export type DictionaryCandidateExportFormat = "json" | "csv";

export interface TranscriptDictionaryCandidate {
  id: number;
  candidateType: DictionaryCandidateType;
  sourceText: string;
  targetText: string;
  evidenceJson: string;
  sourceJson: string;
  status: DictionaryCandidateStatus;
  createdAt: string;
  updatedAt: string;
}

// The current Rust database row has no serde rename rule, so its response is snake_case.
interface TranscriptDictionaryCandidateWireRow {
  id: number;
  candidate_type: DictionaryCandidateType;
  source_text: string;
  target_text: string;
  evidence_json: string;
  source_json: string;
  status: DictionaryCandidateStatus;
  created_at: string;
  updated_at: string;
}

export type InvokeFunction = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

export interface TranscriptReviewClient {
  getTranscriptAudit(source: TranscriptSource): Promise<TranscriptAuditBundle>;
  resolveTranscriptCorrection(
    request: ResolveTranscriptCorrectionRequest,
  ): Promise<TranscriptAuditBundle>;
  listDictionaryCandidates(
    status?: DictionaryCandidateStatus,
  ): Promise<TranscriptDictionaryCandidate[]>;
  setDictionaryCandidateStatus(
    id: number,
    status: DictionaryCandidateStatus,
  ): Promise<TranscriptDictionaryCandidate>;
  exportDictionaryCandidates(format: DictionaryCandidateExportFormat): Promise<string>;
}

export type AnalysisWorkflowStage =
  | "recognizing"
  | "proofreading"
  | "discovering_highlights";

export type TranscriptAuditState =
  | "idle"
  | "loading"
  | "loaded"
  | "legacy-empty"
  | "error";

export interface AnalysisRequestIdentity {
  sourceKey: string;
  sourceRevision: number;
  transcriptRevision: number;
}

export interface TranscriptReviewUpdateEvent {
  bundle: TranscriptAuditBundle;
  requestIdentity: AnalysisRequestIdentity;
  reviewRequestToken: number;
}

export type HighlightDiscoveryAction = "blocked" | "auto" | "continue" | "rerun";

export interface HighlightReviewGate {
  allowed: boolean;
  message: string;
}

export interface TranscriptReviewLock {
  disabled: boolean;
  reason: string;
}

export interface CandidateReviewIdentity {
  generation: number;
  candidateId: string;
}

export function analysisRequestIdentity(
  sourceKey: string,
  sourceRevision: number,
  transcriptRevision: number,
): AnalysisRequestIdentity {
  return { sourceKey, sourceRevision, transcriptRevision };
}

export function isCurrentAnalysisRequest(
  request: AnalysisRequestIdentity,
  active: AnalysisRequestIdentity,
  requestId: number,
  latestRequestId: number,
): boolean {
  return requestId === latestRequestId
    && request.sourceKey === active.sourceKey
    && request.sourceRevision === active.sourceRevision
    && request.transcriptRevision === active.transcriptRevision;
}

export function analysisWorkflowStage(input: {
  hasTranscript: boolean;
  isRecognizing: boolean;
  auditState: TranscriptAuditState;
  pendingCriticalCount: number;
}): AnalysisWorkflowStage {
  if (
    !input.hasTranscript
    || input.isRecognizing
    || input.auditState === "idle"
    || input.auditState === "loading"
  ) {
    return "recognizing";
  }
  if (input.auditState === "error") return "proofreading";
  if (input.auditState === "loaded" && input.pendingCriticalCount > 0) {
    return "proofreading";
  }
  return "discovering_highlights";
}

export function highlightDiscoveryAction(input: {
  stage: AnalysisWorkflowStage;
  sourceKey: string;
  requestedSourceKey: string;
  hadPendingCriticalReview: boolean;
  discoveryCompleted: boolean;
  isDiscovering: boolean;
}): HighlightDiscoveryAction {
  if (
    input.stage !== "discovering_highlights"
    || !input.sourceKey
    || input.sourceKey !== input.requestedSourceKey
    || input.isDiscovering
  ) {
    return "blocked";
  }
  if (input.discoveryCompleted) return "rerun";
  return input.hadPendingCriticalReview ? "continue" : "auto";
}

export function highlightReviewGate(
  stage: AnalysisWorkflowStage,
  discoveryCompleted: boolean,
  isDiscovering: boolean,
): HighlightReviewGate {
  if (stage === "recognizing") {
    return { allowed: false, message: "请先等待逐字稿和校稿读取完成。" };
  }
  if (stage === "proofreading") {
    return { allowed: false, message: "请先完成校稿确认，再开始片段复盘。" };
  }
  if (isDiscovering) {
    return { allowed: false, message: "正在重新识别高光片段，请稍等。" };
  }
  if (!discoveryCompleted) {
    return { allowed: false, message: "请先完成高光片段识别。" };
  }
  return { allowed: true, message: "" };
}

export function transcriptReviewLock(isTranscribing: boolean): TranscriptReviewLock {
  return isTranscribing
    ? { disabled: true, reason: "正在重新识别逐字稿，请完成后再确认校稿。" }
    : { disabled: false, reason: "" };
}

export function candidateReviewIdentity(
  generation: number,
  candidateId: string,
): CandidateReviewIdentity {
  return { generation, candidateId };
}

export function isCurrentCandidateReview(
  request: CandidateReviewIdentity,
  currentGeneration: number,
  currentCandidateIds: readonly string[],
  isDiscovering: boolean,
): boolean {
  return !isDiscovering
    && request.generation === currentGeneration
    && currentCandidateIds.includes(request.candidateId);
}

export function isCurrentCandidateGeneration(
  requestGeneration: number,
  currentGeneration: number,
): boolean {
  return requestGeneration === currentGeneration;
}

export function canContinueAnalysis(bundle: { pendingCriticalCount: number }): boolean {
  return bundle.pendingCriticalCount === 0;
}

export function reviewStatusLabel(decision: ReviewDecision): string {
  return {
    pending: "等你确认",
    approved: "已采用修改",
    kept_original: "已保留原文",
  }[decision];
}

export function isUnresolvedTranscriptPlaceholder(value: string): boolean {
  const normalized = value.trim();
  return normalized.startsWith("[")
    && normalized.endsWith("]")
    && ["待确认", "听不清", "疑似"].some((sentinel) => normalized.includes(sentinel));
}

export function correctionProgress(
  corrections: readonly CorrectionDecisionSummary[],
): { completed: number; total: number } {
  return {
    completed: corrections.filter((correction) => correction.decision !== "pending").length,
    total: corrections.length,
  };
}

export function firstPendingCorrectionId(
  corrections: readonly CorrectionDecisionSummary[],
): string | null {
  return corrections.find((correction) => correction.decision === "pending")?.id ?? null;
}

export function isLatestRequestResult(
  requestId: number,
  latestRequestId: number,
  requestKey: string,
  activeKey: string,
): boolean {
  return requestId === latestRequestId && requestKey === activeKey;
}

export function correctionEditIdentity(
  sourceKey: string,
  correction: Pick<
    TranscriptCorrection,
    "id" | "original" | "proposed" | "decidedText" | "decision"
  >,
): string {
  return JSON.stringify([
    sourceKey,
    correction.id,
    correction.original,
    correction.proposed,
    correction.decidedText,
    correction.decision,
  ]);
}

const CORRECTION_CATEGORY_LABELS: Record<string, string> = {
  product_name: "商品名称",
  product_model: "商品型号",
  model: "商品型号",
  price: "价格",
  discount: "优惠",
  stock: "库存",
  link: "商品链接",
  gift: "赠品",
  warranty: "售后承诺",
  formatting: "文字格式",
};

const CORRECTION_BUSINESS_REASONS: Record<string, string> = {
  product_name: "商品名称会影响客户理解和后续复盘，建议逐字确认。",
  product_model: "商品型号会影响客户判断和后续话术，建议逐字确认。",
  model: "商品型号会影响客户判断和后续话术，建议逐字确认。",
  price: "价格直接影响成交判断，必须以直播原话为准。",
  discount: "优惠信息关系客户权益，必须确认后再用于分析。",
  stock: "库存信息有时效性，必须确认主播当时的准确表达。",
  link: "商品链接决定客户购买对象，必须确认对应商品无误。",
  gift: "赠品属于成交承诺，必须确认内容和条件无误。",
  warranty: "售后承诺关系客户权益，必须确认原话和适用条件。",
  formatting: "调整文字边界能让逐字稿更易读，不改变主播原意。",
};

function normalizedCorrectionCategory(category: string): string {
  const normalized = category.trim().toLowerCase();
  if (normalized.includes("product_model") || normalized.includes("model") || normalized.includes("型号")) return "product_model";
  if (normalized.includes("product_name") || normalized.includes("商品名") || normalized.includes("品名")) return "product_name";
  if (normalized.includes("price") || normalized.includes("价格") || normalized.includes("报价")) return "price";
  if (normalized.includes("discount") || normalized.includes("优惠") || normalized.includes("折扣")) return "discount";
  if (normalized.includes("stock") || normalized.includes("库存")) return "stock";
  if (normalized.includes("link") || normalized.includes("链接")) return "link";
  if (normalized.includes("gift") || normalized.includes("赠品")) return "gift";
  if (normalized.includes("warranty") || normalized.includes("售后") || normalized.includes("保修")) return "warranty";
  if (["punctuation", "whitespace", "formatting", "标点", "标点符号", "空格", "空白", "格式"].some((value) => normalized.includes(value))) return "formatting";
  return normalized;
}

export function correctionCategoryLabel(category: string): string {
  return CORRECTION_CATEGORY_LABELS[normalizedCorrectionCategory(category)] ?? "关键信息";
}

export function correctionBusinessReason(category: string): string {
  return CORRECTION_BUSINESS_REASONS[normalizedCorrectionCategory(category)]
    ?? "这处内容会影响后续片段分析，建议结合视频原声确认。";
}

export function correctionSelectionTarget(
  correction: Pick<TranscriptCorrection, "startMs" | "endMs">,
  entries: readonly TimedTranscriptEntry[],
): { seekSeconds: number; transcriptEntryId: number | null } {
  const seekSeconds = Math.max(0, correction.startMs / 1000);
  const endSeconds = Math.max(seekSeconds, correction.endMs / 1000);
  const matchingEntry = entries.find((entry) => (
    entry.start <= seekSeconds && seekSeconds < entry.end
  )) ?? entries.find((entry) => (
    entry.end > seekSeconds && entry.start < endSeconds
  ));
  return {
    seekSeconds,
    transcriptEntryId: matchingEntry?.id ?? null,
  };
}

function normalizeDictionaryCandidate(
  row: TranscriptDictionaryCandidateWireRow,
): TranscriptDictionaryCandidate {
  return {
    id: row.id,
    candidateType: row.candidate_type,
    sourceText: row.source_text,
    targetText: row.target_text,
    evidenceJson: row.evidence_json,
    sourceJson: row.source_json,
    status: row.status,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
}

export function createTranscriptReviewClient(invoke: InvokeFunction): TranscriptReviewClient {
  return {
    async getTranscriptAudit(source) {
      return invoke("get_transcript_audit", { source }) as Promise<TranscriptAuditBundle>;
    },

    async resolveTranscriptCorrection(request) {
      return invoke("resolve_transcript_correction", { request }) as Promise<TranscriptAuditBundle>;
    },

    async listDictionaryCandidates(status) {
      const rows = await invoke("list_transcript_dictionary_candidates", {
        status: status ?? null,
      }) as TranscriptDictionaryCandidateWireRow[];
      return rows.map(normalizeDictionaryCandidate);
    },

    async setDictionaryCandidateStatus(id, status) {
      const row = await invoke("set_transcript_dictionary_candidate_status", { id, status }) as
        TranscriptDictionaryCandidateWireRow;
      return normalizeDictionaryCandidate(row);
    },

    async exportDictionaryCandidates(format) {
      return invoke("export_transcript_dictionary_candidates", { format }) as Promise<string>;
    },
  };
}

const defaultClient = createTranscriptReviewClient(async (command, args) => {
  const { invoke } = await import("./invoker.js");
  return invoke(command, args);
});

export const getTranscriptAudit = defaultClient.getTranscriptAudit;
export const resolveTranscriptCorrection = defaultClient.resolveTranscriptCorrection;
export const listDictionaryCandidates = defaultClient.listDictionaryCandidates;
export const setDictionaryCandidateStatus = defaultClient.setDictionaryCandidateStatus;
export const exportDictionaryCandidates = defaultClient.exportDictionaryCandidates;
