import type { AnchorKnowledgeSource, AnchorKnowledgeStatus } from "./anchorKnowledge.js";

export type EvidenceLevel = "A" | "B" | "C" | "D";
export type LearningStage = "opening" | "traffic" | "needs" | "explanation" | "objection" | "conversion" | "after_sales" | "retention";
export type LearningDifficulty = "beginner" | "intermediate" | "advanced";
export type PracticeMode = "shadow" | "recall" | "scenario" | "risk_spotting";

export interface LearningCaseLine {
  lineId: string;
  startMs: number;
  endMs: number;
  originalText: string;
  function: string;
  timingReason: string;
  technique: string;
  trustMechanism: string;
  actionCue: string;
  riskNote: string;
  reusablePattern: string;
}

export interface LearningCaseSummary {
  caseId: string;
  assetId: string;
  anchorId: string;
  anchorName: string;
  title: string;
  stage: LearningStage;
  skill: string;
  productCategory: string;
  difficulty: LearningDifficulty;
  evidenceLevel: Exclude<EvidenceLevel, "D">;
  sceneContext: string;
  durationMs: number;
  publishedAt: string;
}

export interface LearningCaseSource extends Omit<AnchorKnowledgeSource, "sourceKind"> {
  sourceKind: Exclude<AnchorKnowledgeSource["sourceKind"], "external">;
}

export interface LearningCaseDetail extends Omit<LearningCaseSummary, "evidenceLevel" | "publishedAt"> {
  evidenceLevel: EvidenceLevel;
  evidenceSummary: string;
  operatorCommentary: string;
  aiAnalysis: string;
  audienceTrigger: string;
  trainingGoal: string;
  expressionReason: string;
  logicReason: string;
  trustReason: string;
  actionReason: string;
  reusableOutline: string;
  forbiddenCopy: string;
  traineeReference: string;
  factSlots: string[];
  applicableScope: string;
  expiryConditions: string;
  reviewDueAt: string;
  internalUseConfirmed: boolean;
  videoPath: string;
  videoStartMs: number;
  videoEndMs: number;
  reviewStatus: AnchorKnowledgeStatus | "retired";
  retiredAt: string | null;
  publishedAt: string | null;
  isCurrent: boolean;
  lines: LearningCaseLine[];
  sources: LearningCaseSource[];
  tags: string[];
}

export interface LearningCaseSearchRequest {
  query: string;
  anchorId: string | null;
  stage: LearningStage | null;
  skill: string | null;
  difficulty: LearningDifficulty | null;
  evidenceLevels: Exclude<EvidenceLevel, "D">[];
}

type UnknownRecord = Record<string, unknown>;

const evidenceLevels = new Set<EvidenceLevel>(["A", "B", "C", "D"]);
const publicEvidenceLevels = new Set<Exclude<EvidenceLevel, "D">>(["A", "B", "C"]);
const stages = new Set<LearningStage>(["opening", "traffic", "needs", "explanation", "objection", "conversion", "after_sales", "retention"]);
const difficulties = new Set<LearningDifficulty>(["beginner", "intermediate", "advanced"]);
const reviewStatuses = new Set<LearningCaseDetail["reviewStatus"]>(["candidate", "pending_review", "published", "rejected", "retired"]);
const sourceKinds = new Set<LearningCaseSource["sourceKind"]>(["video", "transcript", "product_fact", "analysis"]);

function isRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function isNonEmptyString(value: unknown): value is string {
  return isString(value) && value.trim().length > 0;
}

function isNonNegativeNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function hasStringFields(value: UnknownRecord, fields: string[]): boolean {
  return fields.every((field) => isString(value[field]));
}

function hasStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(isString);
}

function isLearningCaseLine(value: unknown): value is LearningCaseLine {
  if (!isRecord(value)) return false;
  return isNonEmptyString(value.lineId)
    && isNonNegativeNumber(value.startMs)
    && isNonNegativeNumber(value.endMs)
    && value.startMs < value.endMs
    && isNonEmptyString(value.originalText)
    && hasStringFields(value, ["function", "timingReason", "technique", "trustMechanism", "actionCue", "riskNote", "reusablePattern"]);
}

function isLearningCaseSource(value: unknown): value is LearningCaseSource {
  if (!isRecord(value) || !isNonEmptyString(value.sourceId) || !sourceKinds.has(value.sourceKind as LearningCaseSource["sourceKind"])) return false;
  if (!hasStringFields(value, ["sourceLocator", "transcriptVersion", "transcriptHash", "productFactId", "productFactVersion", "analysisVersion", "contentHash"])) return false;
  if (value.videoId !== null && !isNonNegativeNumber(value.videoId)) return false;
  const { startMs, endMs } = value;
  if ((startMs === null) !== (endMs === null)) return false;
  return startMs === null || (isNonNegativeNumber(startMs) && isNonNegativeNumber(endMs) && endMs > startMs);
}

function hasValidLineOrder(lines: LearningCaseLine[]): boolean {
  for (let index = 1; index < lines.length; index += 1) {
    const previous = lines[index - 1];
    const current = lines[index];
    if (current.startMs < previous.startMs || current.startMs < previous.endMs) return false;
  }
  return true;
}

export function readLearningCaseDetail(
  value: unknown,
  { scope }: { scope: "public" | "private" } = { scope: "public" },
): LearningCaseDetail | null {
  if (!isRecord(value) || (scope !== "public" && scope !== "private")) return null;
  if (!hasStringFields(value, [
    "caseId", "assetId", "anchorId", "anchorName", "title", "skill", "productCategory", "sceneContext",
    "evidenceSummary", "operatorCommentary", "aiAnalysis", "audienceTrigger", "trainingGoal", "expressionReason", "logicReason", "trustReason",
    "actionReason", "reusableOutline", "forbiddenCopy", "traineeReference", "applicableScope", "expiryConditions",
    "reviewDueAt", "videoPath",
  ])) return null;
  if (![value.caseId, value.assetId, value.anchorId].every(isNonEmptyString)) return null;
  if (!stages.has(value.stage as LearningStage) || !difficulties.has(value.difficulty as LearningDifficulty) || !evidenceLevels.has(value.evidenceLevel as EvidenceLevel)) return null;
  if (scope === "public" && value.evidenceLevel === "D") return null;
  if (!reviewStatuses.has(value.reviewStatus as LearningCaseDetail["reviewStatus"]) || typeof value.internalUseConfirmed !== "boolean" || typeof value.isCurrent !== "boolean") return null;
  if (!isNonNegativeNumber(value.durationMs) || !isNonNegativeNumber(value.videoStartMs) || !isNonNegativeNumber(value.videoEndMs) || value.videoStartMs >= value.videoEndMs) return null;
  if ((value.retiredAt !== null && !isString(value.retiredAt)) || (value.publishedAt !== null && !isString(value.publishedAt))) return null;
  if (scope === "public" && (value.reviewStatus !== "published" || !isNonEmptyString(value.publishedAt) || value.retiredAt !== null || !value.isCurrent)) return null;
  if (!hasStringArray(value.factSlots) || !hasStringArray(value.tags) || !Array.isArray(value.lines) || !Array.isArray(value.sources)) return null;
  if (!value.lines.every(isLearningCaseLine) || !value.sources.every(isLearningCaseSource) || !hasValidLineOrder(value.lines)) return null;
  return value as unknown as LearningCaseDetail;
}

export function buildPublicLearningCaseSearch(input: {
  query?: unknown;
  anchorId?: unknown;
  stage?: unknown;
  skill?: unknown;
  difficulty?: unknown;
  evidenceLevels?: unknown;
} = {}): LearningCaseSearchRequest {
  const requestedLevels = Array.isArray(input.evidenceLevels) ? input.evidenceLevels : [];
  const evidenceLevels = [...new Set(requestedLevels.filter((level): level is Exclude<EvidenceLevel, "D"> => publicEvidenceLevels.has(level as Exclude<EvidenceLevel, "D">)))];
  return {
    query: isString(input.query) ? input.query : "",
    anchorId: isNonEmptyString(input.anchorId) ? input.anchorId : null,
    stage: stages.has(input.stage as LearningStage) ? input.stage as LearningStage : null,
    skill: isNonEmptyString(input.skill) ? input.skill : null,
    difficulty: difficulties.has(input.difficulty as LearningDifficulty) ? input.difficulty as LearningDifficulty : null,
    evidenceLevels,
  };
}

export function activeLearningCaseLine(lines: readonly LearningCaseLine[], positionMs: number): LearningCaseLine | null {
  if (!isNonNegativeNumber(positionMs)) return null;
  return lines.find((line) => isLearningCaseLine(line) && line.startMs <= positionMs && positionMs < line.endMs) ?? null;
}
