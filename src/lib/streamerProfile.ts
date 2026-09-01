export const PENDING_STREAMER_KEY = "__pending__" as const;
export const MIN_SKILL_SCORE_SOURCES = 2;
export const DEFAULT_STREAMER_AVATAR_COUNT = 8;
export const CONTENT_ANALYSIS_CACHE_PREFIX = "bsr:content-analysis:v3:";

export const STREAMER_SKILL_DIMENSIONS = [
  { key: "needs_confirmation", label: "需求确认" },
  { key: "product_expertise", label: "产品专业" },
  { key: "trust_building", label: "信任建立" },
  { key: "expression_structure", label: "表达结构" },
  { key: "pacing_control", label: "节奏控场" },
  { key: "objection_handling", label: "异议处理" },
  { key: "conversion_advancement", label: "成交推进" },
  { key: "risk_compliance", label: "风险合规" },
] as const;

export type StreamerSkillDimensionKey = typeof STREAMER_SKILL_DIMENSIONS[number]["key"];
export type SkillRatingPeriod = "current" | "previous";

export const STREAMER_SKILL_DIMENSION_KEYS: readonly StreamerSkillDimensionKey[] =
  STREAMER_SKILL_DIMENSIONS.map((dimension) => dimension.key);

export const SKILL_RATING_PERIOD_LABELS: Readonly<Record<SkillRatingPeriod, string>> = {
  current: "当前周期",
  previous: "上一周期",
};

export type PendingStreamerReason =
  | "missing_name"
  | "detection_unresolved"
  | "detection_conflict"
  | "low_confidence"
  | "unverified_detection";

export interface StreamerArchiveLike {
  live_id: string | number;
  platform?: string | null;
  room_id?: string | number | null;
  title?: string | null;
  created_at?: string | null;
  length?: number | string | null;
  cover?: string | null;
  archive_kind?: string | null;
  anchor_name?: string | null;
  anchor_source?: string | null;
  anchor_confidence?: string | number | null;
  anchor_detection_status?: string | null;
  anchor_detection_error?: string | null;
  imported_video_id?: number | null;
  product?: unknown;
  products?: unknown;
  product_names?: unknown;
  analysisCompletion?: number | null;
  analysis_completion?: number | null;
}

export interface RuntimeRecordingLike {
  recording?: boolean | null;
  live_id?: string | number | null;
  room_info?: {
    platform?: string | null;
    room_id?: string | number | null;
  } | null;
}

export interface StreamerArchiveAssignment<T extends StreamerArchiveLike = StreamerArchiveLike> {
  archive: T;
  streamerKey: string;
  streamerName: string;
  isPending: boolean;
  pendingReason: PendingStreamerReason | null;
  manuallyConfirmed: boolean;
}

export interface StreamerProfileDataRange {
  from: string;
  to: string;
}

export interface StreamerArchiveGroup<T extends StreamerArchiveLike = StreamerArchiveLike> {
  streamerKey: string;
  streamerName: string;
  isPending: boolean;
  archives: T[];
  sessionCount: number;
  totalDurationSeconds: number;
  latestLiveAt: string | null;
  latestArchive: T | null;
  dataRange: StreamerProfileDataRange | null;
  avatarIndex: number;
  pendingReasonCounts: Partial<Record<PendingStreamerReason, number>>;
}

export type StreamerArchiveLiveStatus = "all" | "recording" | "completed";
export type StreamerArchiveAnalysisStatus = "all" | "not_started" | "in_progress" | "complete";

export interface StreamerArchiveFilters {
  dateFrom?: string | null;
  dateTo?: string | null;
  sessionQuery?: string | null;
  productQuery?: string | null;
  liveStatus?: StreamerArchiveLiveStatus;
  analysisStatus?: StreamerArchiveAnalysisStatus;
}

export interface StreamerArchiveFilterContext {
  runtimeRecorders?: readonly RuntimeRecordingLike[];
  analysisCompletionBySource?:
    | ReadonlyMap<string, number>
    | Readonly<Record<string, number>>;
}

function normalizeWhitespace(value: unknown): string {
  return String(value ?? "").trim().replace(/\s+/gu, " ");
}

function normalizeIdentifier(value: unknown): string {
  return String(value ?? "").trim();
}

function normalizeStatus(value: unknown): string {
  return normalizeWhitespace(value).toLowerCase();
}

const CONFIRMED_STREAMER_NAME_ALIASES: Readonly<Record<string, string>> = {
  "小鸦": "小鹅",
};

export function normalizeStreamerName(value: unknown): string {
  const normalized = normalizeWhitespace(value);
  return CONFIRMED_STREAMER_NAME_ALIASES[normalized] || normalized;
}

/** Grouping applies explicit human-confirmed aliases only; it never fuzzy-guesses names. */
export function normalizeStreamerKey(value: unknown): string {
  return normalizeStreamerName(value).toLowerCase();
}

function isManualConfirmation(archive: StreamerArchiveLike): boolean {
  return normalizeStatus(archive.anchor_source) === "manual"
    && normalizeStatus(archive.anchor_detection_status) === "confirmed";
}

function hasConflictSignal(archive: StreamerArchiveLike): boolean {
  const status = normalizeStatus(archive.anchor_detection_status);
  if (status === "conflict" || status === "ambiguous") return true;
  const error = normalizeWhitespace(archive.anchor_detection_error).toLowerCase();
  return /冲突|不一致|歧义|conflict|inconsisten|ambiguous/u.test(error);
}

function isLowAutomaticConfidence(value: StreamerArchiveLike["anchor_confidence"]): boolean {
  if (typeof value === "number") return Number.isFinite(value) && value < 0.5;
  const confidence = normalizeStatus(value);
  if (!confidence) return false;
  if (["low", "低", "weak", "unreliable"].includes(confidence)) return true;
  const numeric = Number(confidence);
  return Number.isFinite(numeric) && numeric < 0.5;
}

function pendingAssignment<T extends StreamerArchiveLike>(
  archive: T,
  reason: PendingStreamerReason,
): StreamerArchiveAssignment<T> {
  return {
    archive,
    streamerKey: PENDING_STREAMER_KEY,
    streamerName: "待确认",
    isPending: true,
    pendingReason: reason,
    manuallyConfirmed: false,
  };
}

/**
 * Competitor rows return null. Company rows only enter a named profile after a
 * resolved identity; manual confirmed identity wins over stale automatic data.
 */
export function assignCompanyArchiveToStreamer<T extends StreamerArchiveLike>(
  archive: T,
): StreamerArchiveAssignment<T> | null {
  if (normalizeStatus(archive.archive_kind) !== "company") return null;

  const streamerName = normalizeStreamerName(archive.anchor_name);
  if (streamerName && isManualConfirmation(archive)) {
    return {
      archive,
      streamerKey: normalizeStreamerKey(streamerName),
      streamerName,
      isPending: false,
      pendingReason: null,
      manuallyConfirmed: true,
    };
  }

  if (hasConflictSignal(archive)) {
    return pendingAssignment(archive, "detection_conflict");
  }
  if (!streamerName) return pendingAssignment(archive, "missing_name");

  const status = normalizeStatus(archive.anchor_detection_status);
  if (["failed", "running", "pending", "not_requested"].includes(status)) {
    return pendingAssignment(archive, "detection_unresolved");
  }
  if (status === "conflict" || status === "ambiguous") {
    return pendingAssignment(archive, "detection_conflict");
  }
  if (isLowAutomaticConfidence(archive.anchor_confidence)) {
    return pendingAssignment(archive, "low_confidence");
  }
  if (status !== "confirmed") {
    return pendingAssignment(archive, "unverified_detection");
  }

  return {
    archive,
    streamerKey: normalizeStreamerKey(streamerName),
    streamerName,
    isPending: false,
    pendingReason: null,
    manuallyConfirmed: false,
  };
}

function validTimestamp(value: unknown): number | null {
  const text = normalizeIdentifier(value);
  if (!text) return null;
  const timestamp = Date.parse(text);
  return Number.isFinite(timestamp) ? timestamp : null;
}

function normalizedDurationSeconds(value: unknown): number {
  const duration = Number(value);
  return Number.isFinite(duration) && duration > 0 ? duration : 0;
}

export function stableStreamerAvatarIndex(
  streamerKey: string,
  avatarCount = DEFAULT_STREAMER_AVATAR_COUNT,
): number {
  const count = Math.max(1, Math.floor(Number(avatarCount) || DEFAULT_STREAMER_AVATAR_COUNT));
  const key = normalizeStreamerKey(streamerKey) || PENDING_STREAMER_KEY;
  let hash = 0x811c9dc5;
  for (let index = 0; index < key.length; index += 1) {
    hash ^= key.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0) % count;
}

function preferredStreamerName<T extends StreamerArchiveLike>(
  assignments: readonly StreamerArchiveAssignment<T>[],
): string {
  if (assignments[0]?.isPending) return "待确认";
  return [...assignments]
    .sort((left, right) => {
      if (left.manuallyConfirmed !== right.manuallyConfirmed) {
        return left.manuallyConfirmed ? -1 : 1;
      }
      const leftTime = validTimestamp(left.archive.created_at) ?? Number.NEGATIVE_INFINITY;
      const rightTime = validTimestamp(right.archive.created_at) ?? Number.NEGATIVE_INFINITY;
      if (leftTime !== rightTime) return rightTime - leftTime;
      return left.streamerName.localeCompare(right.streamerName, "zh-CN");
    })[0]?.streamerName || "待确认";
}

export function groupCompanyArchivesByStreamer<T extends StreamerArchiveLike>(
  archives: readonly T[],
): StreamerArchiveGroup<T>[] {
  const grouped = new Map<string, StreamerArchiveAssignment<T>[]>();
  archives.forEach((archive) => {
    const assignment = assignCompanyArchiveToStreamer(archive);
    if (!assignment) return;
    const current = grouped.get(assignment.streamerKey) || [];
    current.push(assignment);
    grouped.set(assignment.streamerKey, current);
  });

  return [...grouped.entries()]
    .map(([streamerKey, assignments]) => {
      const dated = assignments
        .map((assignment, inputIndex) => ({
          assignment,
          inputIndex,
          timestamp: validTimestamp(assignment.archive.created_at),
        }))
        .sort((left, right) => {
          const leftTime = left.timestamp ?? Number.NEGATIVE_INFINITY;
          const rightTime = right.timestamp ?? Number.NEGATIVE_INFINITY;
          return rightTime - leftTime || left.inputIndex - right.inputIndex;
        });
      const validDates = dated.filter((item) => item.timestamp !== null);
      const sortedArchives = dated.map((item) => item.assignment.archive);
      const pendingReasonCounts: Partial<Record<PendingStreamerReason, number>> = {};
      assignments.forEach((assignment) => {
        if (!assignment.pendingReason) return;
        pendingReasonCounts[assignment.pendingReason] =
          (pendingReasonCounts[assignment.pendingReason] || 0) + 1;
      });
      const latest = validDates[0] || null;
      const earliest = validDates[validDates.length - 1] || null;

      return {
        streamerKey,
        streamerName: preferredStreamerName(assignments),
        isPending: streamerKey === PENDING_STREAMER_KEY,
        archives: sortedArchives,
        sessionCount: sortedArchives.length,
        totalDurationSeconds: assignments.reduce(
          (total, assignment) => total + normalizedDurationSeconds(assignment.archive.length),
          0,
        ),
        latestLiveAt: latest ? normalizeIdentifier(latest.assignment.archive.created_at) : null,
        latestArchive: dated[0]?.assignment.archive || null,
        dataRange: latest && earliest
          ? {
            from: normalizeIdentifier(earliest.assignment.archive.created_at),
            to: normalizeIdentifier(latest.assignment.archive.created_at),
          }
          : null,
        avatarIndex: stableStreamerAvatarIndex(streamerKey),
        pendingReasonCounts,
      } satisfies StreamerArchiveGroup<T>;
    })
    .sort((left, right) => {
      if (left.isPending !== right.isPending) return left.isPending ? 1 : -1;
      const leftTime = validTimestamp(left.latestLiveAt) ?? Number.NEGATIVE_INFINITY;
      const rightTime = validTimestamp(right.latestLiveAt) ?? Number.NEGATIVE_INFINITY;
      return rightTime - leftTime || left.streamerName.localeCompare(right.streamerName, "zh-CN");
    });
}

export function findStreamerArchiveGroup<T extends StreamerArchiveLike>(
  groups: readonly StreamerArchiveGroup<T>[],
  streamerNameOrKey: string,
): StreamerArchiveGroup<T> | null {
  const requested = streamerNameOrKey === PENDING_STREAMER_KEY
    ? PENDING_STREAMER_KEY
    : normalizeStreamerKey(streamerNameOrKey);
  return groups.find((group) => group.streamerKey === requested) || null;
}

function exactArchiveRuntimeIdentity(archive: StreamerArchiveLike): string | null {
  const platform = normalizeStatus(archive.platform);
  const roomId = normalizeIdentifier(archive.room_id);
  const liveId = normalizeIdentifier(archive.live_id);
  return platform && roomId && liveId ? `${platform}\u0000${roomId}\u0000${liveId}` : null;
}

function exactRecorderRuntimeIdentity(recorder: RuntimeRecordingLike): string | null {
  const platform = normalizeStatus(recorder.room_info?.platform);
  const roomId = normalizeIdentifier(recorder.room_info?.room_id);
  const liveId = normalizeIdentifier(recorder.live_id);
  return platform && roomId && liveId ? `${platform}\u0000${roomId}\u0000${liveId}` : null;
}

/** Never infers "直播中" from dates, duration, names, or stale archive fields. */
export function isArchiveActivelyRecording(
  archive: StreamerArchiveLike,
  runtimeRecorders: readonly RuntimeRecordingLike[],
): boolean {
  const archiveIdentity = exactArchiveRuntimeIdentity(archive);
  if (!archiveIdentity) return false;
  return runtimeRecorders.some((recorder) =>
    recorder.recording === true
    && exactRecorderRuntimeIdentity(recorder) === archiveIdentity
  );
}

export const isReliablyLiveArchive = isArchiveActivelyRecording;

export function streamerArchiveSourceKey(archive: StreamerArchiveLike): string {
  const importedId = Number(archive.imported_video_id);
  if (Number.isInteger(importedId) && importedId > 0) return `video:${importedId}`;
  const liveId = normalizeIdentifier(archive.live_id);
  if (liveId.startsWith("import:")) {
    const parsed = Number.parseInt(liveId.slice("import:".length), 10);
    if (Number.isInteger(parsed) && parsed > 0) return `video:${parsed}`;
  }
  return `archive:${normalizeIdentifier(archive.platform)}:${normalizeIdentifier(archive.room_id)}:${liveId}`;
}

function filterBoundaryTimestamp(value: unknown, endOfDay: boolean): number | null {
  const text = normalizeIdentifier(value);
  if (!text) return null;
  const candidate = /^\d{4}-\d{2}-\d{2}$/u.test(text)
    ? `${text}T${endOfDay ? "23:59:59.999" : "00:00:00.000"}`
    : text;
  return validTimestamp(candidate);
}

function searchableProductText(archive: StreamerArchiveLike): string {
  // RecordItem currently has no structured product field. Its title remains a
  // first-class fallback until analysis data enriches the row.
  const values: string[] = [normalizeWhitespace(archive.title)];
  const collect = (value: unknown): void => {
    if (typeof value === "string" || typeof value === "number") {
      values.push(String(value));
      return;
    }
    if (Array.isArray(value)) {
      value.forEach(collect);
      return;
    }
    if (value && typeof value === "object") {
      const item = value as Record<string, unknown>;
      collect(item.name ?? item.title ?? item.product ?? item.label);
    }
  };
  collect(archive.product);
  collect(archive.products);
  collect(archive.product_names);
  return normalizeWhitespace(values.join(" ")).toLowerCase();
}

function completionFromIndex(
  archive: StreamerArchiveLike,
  index: StreamerArchiveFilterContext["analysisCompletionBySource"],
): number | null {
  const own = archive.analysisCompletion ?? archive.analysis_completion;
  if (typeof own === "number" && Number.isFinite(own)) {
    return Math.max(0, Math.min(100, own));
  }
  if (!index) return null;
  const key = streamerArchiveSourceKey(archive);
  let value: unknown;
  if (typeof (index as ReadonlyMap<string, number>).get === "function") {
    value = (index as ReadonlyMap<string, number>).get(key);
  } else {
    value = (index as Readonly<Record<string, number>>)[key];
  }
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.min(100, value))
    : null;
}

export function filterStreamerArchives<T extends StreamerArchiveLike>(
  archives: readonly T[],
  filters: StreamerArchiveFilters = {},
  context: StreamerArchiveFilterContext = {},
): T[] {
  const from = filterBoundaryTimestamp(filters.dateFrom, false);
  const to = filterBoundaryTimestamp(filters.dateTo, true);
  const sessionQuery = normalizeWhitespace(filters.sessionQuery).toLowerCase();
  const productQuery = normalizeWhitespace(filters.productQuery).toLowerCase();
  const runtimeRecorders = context.runtimeRecorders || [];
  const liveStatus = filters.liveStatus || "all";
  const analysisStatus = filters.analysisStatus || "all";

  return archives.filter((archive) => {
    const createdAt = validTimestamp(archive.created_at);
    if (from !== null && (createdAt === null || createdAt < from)) return false;
    if (to !== null && (createdAt === null || createdAt > to)) return false;

    if (sessionQuery) {
      const sessionText = normalizeWhitespace([
        archive.title,
        archive.live_id,
        archive.room_id,
        archive.created_at,
      ].join(" ")).toLowerCase();
      if (!sessionText.includes(sessionQuery)) return false;
    }
    if (productQuery && !searchableProductText(archive).includes(productQuery)) return false;

    const recording = isArchiveActivelyRecording(archive, runtimeRecorders);
    if (liveStatus === "recording" && !recording) return false;
    if (liveStatus === "completed" && recording) return false;

    if (analysisStatus !== "all") {
      const completion = completionFromIndex(archive, context.analysisCompletionBySource);
      if (analysisStatus === "not_started" && completion !== null && completion > 0) return false;
      if (analysisStatus === "in_progress" && !(completion !== null && completion > 0 && completion < 100)) {
        return false;
      }
      if (analysisStatus === "complete" && !(completion !== null && completion >= 100)) return false;
    }
    return true;
  });
}

export type SkillEvidenceReviewStatus = "pending_review" | "human_confirmed" | "ai_assessed";

export interface SkillEvidence {
  id: string;
  sourceId: string;
  candidateId: string;
  dimension: StreamerSkillDimensionKey | null;
  summary: string;
  basis: string;
  reviewStatus: SkillEvidenceReviewStatus;
  humanConfirmed: boolean;
  reviewedBy: string | null;
  reviewedAt: string | null;
  sourceUpdatedAt: string | null;
}

export interface ConfirmSkillEvidenceInput {
  dimension: StreamerSkillDimensionKey;
  reviewedBy: string;
  reviewedAt: string;
  summary?: string;
  basis?: string;
}

export interface SkillRatingVersion {
  id: string;
  streamerKey: string;
  period: SkillRatingPeriod;
  dimension: StreamerSkillDimensionKey;
  score: number;
  basis: string;
  evidence: SkillEvidence[];
  version: number;
  supersedesId: string | null;
  reviewedBy: string;
  createdAt: string;
  source: "manual" | "ai";
}

export interface SkillRatingInput {
  id?: string;
  streamerKey: string;
  period: SkillRatingPeriod;
  dimension: StreamerSkillDimensionKey;
  score: number;
  basis: string;
  evidence: readonly SkillEvidence[];
  reviewedBy: string;
  createdAt: string;
}

export interface AppendSkillRatingResult {
  history: SkillRatingVersion[];
  added: SkillRatingVersion;
}

export type SkillRatingSourceCollection = ReadonlySet<string> | readonly string[];

export interface SkillScoreRow {
  dimension: StreamerSkillDimensionKey;
  label: string;
  period: SkillRatingPeriod;
  score: number | null;
  displayScore: string;
  sampleCount: number;
  basis: string | null;
  version: number | null;
  ratingId: string | null;
  evidence: SkillEvidence[];
  status: "unrated" | "insufficient_data" | "scored";
}

export interface StreamerRadarSeries {
  period: SkillRatingPeriod;
  label: string;
  values: Array<number | null>;
  rows: SkillScoreRow[];
}

export interface SkillTrendPoint {
  ratingId: string;
  period: SkillRatingPeriod;
  dimension: StreamerSkillDimensionKey;
  label: string;
  at: string;
  score: number | null;
  displayScore: string;
  sampleCount: number;
  basis: string;
  version: number;
  evidence: SkillEvidence[];
}

export interface RatingConfidence {
  period: SkillRatingPeriod;
  qualifiedDimensionCount: number;
  totalDimensionCount: number;
  sampleCount: number;
  dimensionSampleCount: number;
  minimumSamplesPerDimension: number;
  coveragePercent: number;
  label: string;
}

function isSkillDimension(value: unknown): value is StreamerSkillDimensionKey {
  return STREAMER_SKILL_DIMENSION_KEYS.includes(value as StreamerSkillDimensionKey);
}

function strictRequiredString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = normalizeWhitespace(value);
  return text || null;
}

function parseConfirmedSkillEvidence(
  value: unknown,
  dimension: StreamerSkillDimensionKey,
): SkillEvidence | null {
  if (!isRecord(value)) return null;
  try {
    const id = strictRequiredString(value.id);
    const sourceId = strictRequiredString(value.sourceId);
    const candidateId = strictRequiredString(value.candidateId);
    const summary = strictRequiredString(value.summary);
    const basis = strictRequiredString(value.basis);
    const reviewedBy = strictRequiredString(value.reviewedBy);
    const reviewedAt = strictRequiredString(value.reviewedAt);
    if (
      !id
      || !sourceId
      || !candidateId
      || !summary
      || !basis
      || value.dimension !== dimension
      || value.reviewStatus !== "human_confirmed"
      || value.humanConfirmed !== true
      || !reviewedBy
      || !reviewedAt
      || validTimestamp(reviewedAt) === null
    ) return null;

    let sourceUpdatedAt: string | null = null;
    if (value.sourceUpdatedAt !== null) {
      sourceUpdatedAt = strictRequiredString(value.sourceUpdatedAt);
      if (!sourceUpdatedAt || validTimestamp(sourceUpdatedAt) === null) return null;
    }
    return {
      id,
      sourceId,
      candidateId,
      dimension,
      summary,
      basis,
      reviewStatus: "human_confirmed",
      humanConfirmed: true,
      reviewedBy,
      reviewedAt,
      sourceUpdatedAt,
    };
  } catch {
    return null;
  }
}

function parseAiSkillEvidence(
  value: unknown,
  dimension: StreamerSkillDimensionKey,
): SkillEvidence | null {
  if (!isRecord(value)) return null;
  const id = strictRequiredString(value.id);
  const sourceId = strictRequiredString(value.sourceId);
  const candidateId = strictRequiredString(value.candidateId);
  const summary = strictRequiredString(value.summary);
  const basis = strictRequiredString(value.basis);
  const reviewedBy = strictRequiredString(value.reviewedBy);
  const reviewedAt = strictRequiredString(value.reviewedAt);
  if (
    !id || !sourceId || !candidateId || !summary || !basis || !reviewedBy || !reviewedAt
    || value.dimension !== dimension || value.reviewStatus !== "ai_assessed"
    || value.humanConfirmed !== false || validTimestamp(reviewedAt) === null
  ) return null;
  const sourceUpdatedAt = value.sourceUpdatedAt === null
    ? null
    : strictRequiredString(value.sourceUpdatedAt);
  if (sourceUpdatedAt !== null && validTimestamp(sourceUpdatedAt) === null) return null;
  return {
    id,
    sourceId,
    candidateId,
    dimension,
    summary,
    basis,
    reviewStatus: "ai_assessed",
    humanConfirmed: false,
    reviewedBy,
    reviewedAt,
    sourceUpdatedAt,
  };
}

function parseSkillRatingVersion(value: unknown): SkillRatingVersion | null {
  if (!isRecord(value)) return null;
  try {
    const id = strictRequiredString(value.id);
    const rawStreamerKey = strictRequiredString(value.streamerKey);
    const streamerKey = rawStreamerKey ? normalizeStreamerKey(rawStreamerKey) : "";
    const basis = strictRequiredString(value.basis);
    const reviewedBy = strictRequiredString(value.reviewedBy);
    const createdAt = strictRequiredString(value.createdAt);
    const version = value.version;
    if (
      (value.source !== "manual" && value.source !== "ai")
      || !id
      || !streamerKey
      || streamerKey === PENDING_STREAMER_KEY
      || (value.period !== "current" && value.period !== "previous")
      || !isSkillDimension(value.dimension)
      || typeof value.score !== "number"
      || !Number.isFinite(value.score)
      || value.score < 0
      || value.score > 100
      || !basis
      || !reviewedBy
      || !createdAt
      || validTimestamp(createdAt) === null
      || typeof version !== "number"
      || !Number.isInteger(version)
      || version < 1
      || !Array.isArray(value.evidence)
      || value.evidence.length === 0
    ) return null;

    let supersedesId: string | null = null;
    if (value.supersedesId !== null) {
      supersedesId = strictRequiredString(value.supersedesId);
      if (!supersedesId || supersedesId === id) return null;
    }

    const evidence: SkillEvidence[] = [];
    const evidenceIds = new Set<string>();
    for (const candidate of value.evidence) {
      const parsed = value.source === "manual"
        ? parseConfirmedSkillEvidence(candidate, value.dimension)
        : parseAiSkillEvidence(candidate, value.dimension);
      if (!parsed || evidenceIds.has(parsed.id)) return null;
      evidenceIds.add(parsed.id);
      evidence.push(parsed);
    }

    return {
      id,
      streamerKey,
      period: value.period,
      dimension: value.dimension,
      score: value.score,
      basis,
      evidence,
      version,
      supersedesId,
      reviewedBy,
      createdAt,
      source: value.source,
    };
  } catch {
    return null;
  }
}

/**
 * Strictly isolates untrusted local history. One corrupt or forged entry never
 * prevents another complete, manual, human-evidence-backed version from loading.
 */
export function parseSkillRatingHistory(value: unknown): SkillRatingVersion[] {
  let parsed: unknown;
  try {
    parsed = typeof value === "string" ? JSON.parse(value) : value;
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];

  const history: SkillRatingVersion[] = [];
  const ratingIds = new Set<string>();
  try {
    for (let index = 0; index < parsed.length; index += 1) {
      const rating = parseSkillRatingVersion(parsed[index]);
      if (!rating || ratingIds.has(rating.id)) continue;
      ratingIds.add(rating.id);
      history.push(rating);
    }
  } catch {
    // A hostile array/proxy still returns every valid entry parsed before it.
  }
  return history;
}

function assertIsoLikeTimestamp(value: unknown, field: string): string {
  const text = normalizeIdentifier(value);
  if (!text || validTimestamp(text) === null) throw new Error(`${field} 必须是有效时间`);
  return text;
}

function evidenceSourceCount(
  evidence: readonly SkillEvidence[],
  dimension: StreamerSkillDimensionKey,
): number {
  return new Set(evidence
    .filter((item) =>
      ((item.humanConfirmed === true && item.reviewStatus === "human_confirmed")
        || (item.humanConfirmed === false && item.reviewStatus === "ai_assessed"))
      && item.dimension === dimension
      && normalizeIdentifier(item.reviewedBy)
      && validTimestamp(item.reviewedAt) !== null
      && normalizeIdentifier(item.sourceId)
    )
    .map((item) => normalizeIdentifier(item.sourceId).toLowerCase()))
    .size;
}

export function confirmSkillEvidence(
  evidence: SkillEvidence,
  input: ConfirmSkillEvidenceInput,
): SkillEvidence {
  if (!isSkillDimension(input.dimension)) throw new Error("能力维度无效");
  const reviewedBy = normalizeWhitespace(input.reviewedBy);
  if (!reviewedBy) throw new Error("人工审核人不能为空");
  const reviewedAt = assertIsoLikeTimestamp(input.reviewedAt, "人工审核时间");
  const sourceId = normalizeIdentifier(evidence.sourceId);
  if (!sourceId) throw new Error("证据来源不能为空");
  return {
    ...evidence,
    sourceId,
    dimension: input.dimension,
    summary: normalizeWhitespace(input.summary ?? evidence.summary),
    basis: normalizeWhitespace(input.basis ?? evidence.basis ?? evidence.summary),
    reviewStatus: "human_confirmed",
    humanConfirmed: true,
    reviewedBy,
    reviewedAt,
  };
}

/** Converts an already completed analysis candidate into an AI-attributed skill citation. */
export function assessSkillEvidenceWithAi(
  evidence: SkillEvidence,
  dimension: StreamerSkillDimensionKey,
  assessedAt: string,
): SkillEvidence {
  if (!isSkillDimension(dimension)) throw new Error("能力维度无效");
  const reviewedAt = assertIsoLikeTimestamp(assessedAt, "AI 评估时间");
  const sourceId = normalizeIdentifier(evidence.sourceId);
  if (!sourceId) throw new Error("证据来源不能为空");
  return {
    ...evidence,
    sourceId,
    dimension,
    reviewStatus: "ai_assessed",
    humanConfirmed: false,
    reviewedBy: "AI 能力评估",
    reviewedAt,
  };
}

function compareRatingRecency(left: SkillRatingVersion, right: SkillRatingVersion): number {
  if (left.version !== right.version) return left.version - right.version;
  return (validTimestamp(left.createdAt) ?? Number.NEGATIVE_INFINITY)
    - (validTimestamp(right.createdAt) ?? Number.NEGATIVE_INFINITY);
}

export function latestSkillRating(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
  period: SkillRatingPeriod,
  dimension: StreamerSkillDimensionKey,
): SkillRatingVersion | null {
  const streamerKey = normalizeStreamerKey(streamerNameOrKey);
  return history
    .filter((rating) =>
      normalizeStreamerKey(rating.streamerKey) === streamerKey
      && rating.period === period
      && rating.dimension === dimension
    )
    .reduce<SkillRatingVersion | null>((latest, rating) =>
      !latest || compareRatingRecency(rating, latest) > 0 ? rating : latest,
    null);
}

function normalizedSkillRatingSourceSet(sourceIds: SkillRatingSourceCollection): Set<string> {
  return new Set(
    Array.from(sourceIds)
      .map((sourceId) => normalizeIdentifier(sourceId).toLowerCase())
      .filter(Boolean),
  );
}

/**
 * A recorded rating remains immutable, but it is only active while every
 * evidence source still belongs to the streamer's current company archive.
 * Invalidating the whole version avoids silently reusing a score whose basis
 * was reviewed against a different evidence set.
 */
export function isSkillRatingVersionWithinSources(
  rating: SkillRatingVersion,
  sourceIds: SkillRatingSourceCollection,
): boolean {
  const allowedSources = normalizedSkillRatingSourceSet(sourceIds);
  return rating.evidence.length > 0 && rating.evidence.every((evidence) => {
    const sourceId = normalizeIdentifier(evidence.sourceId).toLowerCase();
    return Boolean(sourceId) && allowedSources.has(sourceId);
  });
}

/** Keep only versions whose streamer and complete evidence set are in scope. */
export function scopeSkillRatingHistoryToSources(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
  sourceIds: SkillRatingSourceCollection,
): SkillRatingVersion[] {
  const streamerKey = normalizeStreamerKey(streamerNameOrKey);
  return history.filter((rating) =>
    normalizeStreamerKey(rating.streamerKey) === streamerKey
    && isSkillRatingVersionWithinSources(rating, sourceIds)
  );
}

function validateRatingEvidence(
  evidence: readonly SkillEvidence[],
  dimension: StreamerSkillDimensionKey,
): SkillEvidence[] {
  if (!evidence.length) throw new Error("评分至少需要 1 条人工审核证据");
  return evidence.map((item) => {
    if (
      item.humanConfirmed !== true
      || item.reviewStatus !== "human_confirmed"
      || item.dimension !== dimension
      || !normalizeIdentifier(item.sourceId)
      || !normalizeIdentifier(item.reviewedBy)
      || validTimestamp(item.reviewedAt) === null
    ) {
      throw new Error("评分证据必须经过人工确认并绑定同一能力维度");
    }
    return { ...item };
  });
}

function validateAiRatingEvidence(
  evidence: readonly SkillEvidence[],
  dimension: StreamerSkillDimensionKey,
): SkillEvidence[] {
  if (!evidence.length) throw new Error("AI 评分至少需要 1 条已完成分析证据");
  return evidence.map((item) => {
    if (
      item.humanConfirmed !== false
      || item.reviewStatus !== "ai_assessed"
      || item.dimension !== dimension
      || !normalizeIdentifier(item.sourceId)
      || !normalizeIdentifier(item.reviewedBy)
      || validTimestamp(item.reviewedAt) === null
    ) throw new Error("AI 评分证据无效");
    return { ...item };
  });
}

export function appendSkillRatingVersion(
  history: readonly SkillRatingVersion[],
  input: SkillRatingInput,
): AppendSkillRatingResult {
  const streamerKey = normalizeStreamerKey(input.streamerKey);
  if (!streamerKey || streamerKey === PENDING_STREAMER_KEY) {
    throw new Error("待确认主播不能写入能力评分");
  }
  if (input.period !== "current" && input.period !== "previous") {
    throw new Error("评分周期无效");
  }
  if (!isSkillDimension(input.dimension)) throw new Error("能力维度无效");
  if (!Number.isFinite(input.score) || input.score < 0 || input.score > 100) {
    throw new Error("评分必须是 0–100 的数字");
  }
  const basis = normalizeWhitespace(input.basis);
  if (!basis) throw new Error("评分依据不能为空");
  const reviewedBy = normalizeWhitespace(input.reviewedBy);
  if (!reviewedBy) throw new Error("评分审核人不能为空");
  const createdAt = assertIsoLikeTimestamp(input.createdAt, "评分时间");
  const evidence = validateRatingEvidence(input.evidence, input.dimension);
  const superseded = latestSkillRating(history, streamerKey, input.period, input.dimension);
  const version = Math.max(
    0,
    ...history
      .filter((rating) =>
        normalizeStreamerKey(rating.streamerKey) === streamerKey
        && rating.period === input.period
        && rating.dimension === input.dimension
      )
      .map((rating) => Number.isInteger(rating.version) ? rating.version : 0),
  ) + 1;
  const id = normalizeIdentifier(input.id)
    || `rating:${encodeURIComponent(streamerKey)}:${input.period}:${input.dimension}:v${version}`;
  if (history.some((rating) => rating.id === id)) throw new Error("评分版本 ID 已存在");

  const added: SkillRatingVersion = {
    id,
    streamerKey,
    period: input.period,
    dimension: input.dimension,
    score: input.score,
    basis,
    evidence,
    version,
    supersedesId: superseded?.id || null,
    reviewedBy,
    createdAt,
    source: "manual",
  };
  return { history: [...history, added], added };
}

/** Persists an immutable, evidence-cited AI score without mixing it with manual versions. */
export function appendAiSkillRatingVersion(
  history: readonly SkillRatingVersion[],
  input: SkillRatingInput,
): AppendSkillRatingResult {
  const streamerKey = normalizeStreamerKey(input.streamerKey);
  if (!streamerKey || streamerKey === PENDING_STREAMER_KEY) throw new Error("待确认主播不能写入能力评分");
  if (input.period !== "current" && input.period !== "previous") throw new Error("评分周期无效");
  if (!isSkillDimension(input.dimension)) throw new Error("能力维度无效");
  if (!Number.isFinite(input.score) || input.score < 0 || input.score > 100) throw new Error("评分必须是 0–100 的数字");
  const basis = normalizeWhitespace(input.basis);
  if (!basis) throw new Error("评分依据不能为空");
  const createdAt = assertIsoLikeTimestamp(input.createdAt, "评分时间");
  const evidence = validateAiRatingEvidence(input.evidence, input.dimension);
  const superseded = latestSkillRating(history, streamerKey, input.period, input.dimension);
  const version = Math.max(0, ...history
    .filter((rating) => normalizeStreamerKey(rating.streamerKey) === streamerKey
      && rating.period === input.period && rating.dimension === input.dimension)
    .map((rating) => Number.isInteger(rating.version) ? rating.version : 0)) + 1;
  const id = normalizeIdentifier(input.id)
    || `rating:${encodeURIComponent(streamerKey)}:${input.period}:${input.dimension}:v${version}`;
  if (history.some((rating) => rating.id === id)) throw new Error("评分版本 ID 已存在");
  const added: SkillRatingVersion = {
    id,
    streamerKey,
    period: input.period,
    dimension: input.dimension,
    score: input.score,
    basis,
    evidence,
    version,
    supersedesId: superseded?.id || null,
    reviewedBy: "AI 能力评估",
    createdAt,
    source: "ai",
  };
  return { history: [...history, added], added };
}

function skillScoreRow(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
  period: SkillRatingPeriod,
  dimension: typeof STREAMER_SKILL_DIMENSIONS[number],
): SkillScoreRow {
  const rating = latestSkillRating(history, streamerNameOrKey, period, dimension.key);
  if (!rating) {
    return {
      dimension: dimension.key,
      label: dimension.label,
      period,
      score: null,
      displayScore: "数据不足",
      sampleCount: 0,
      basis: null,
      version: null,
      ratingId: null,
      evidence: [],
      status: "unrated",
    };
  }
  const sampleCount = evidenceSourceCount(rating.evidence, dimension.key);
  const score = sampleCount >= MIN_SKILL_SCORE_SOURCES ? rating.score : null;
  return {
    dimension: dimension.key,
    label: dimension.label,
    period,
    score,
    displayScore: score === null ? "数据不足" : String(score),
    sampleCount,
    basis: rating.basis,
    version: rating.version,
    ratingId: rating.id,
    evidence: rating.evidence.map((item) => ({ ...item })),
    status: score === null ? "insufficient_data" : "scored",
  };
}

export function buildSkillScoreTable(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
  period: SkillRatingPeriod = "current",
): SkillScoreRow[] {
  return STREAMER_SKILL_DIMENSIONS.map((dimension) =>
    skillScoreRow(history, streamerNameOrKey, period, dimension)
  );
}

/** Returns only plottable current/previous series, never an all-null radar line. */
export function buildStreamerRadarSeries(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
): StreamerRadarSeries[] {
  return (["current", "previous"] as const)
    .map((period) => {
      const rows = buildSkillScoreTable(history, streamerNameOrKey, period);
      return {
        period,
        label: SKILL_RATING_PERIOD_LABELS[period],
        values: rows.map((row) => row.score),
        rows,
      };
    })
    .filter((series) => series.values.some((score) => score !== null));
}

export function buildSkillTrend(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
  dimension?: StreamerSkillDimensionKey,
): SkillTrendPoint[] {
  const streamerKey = normalizeStreamerKey(streamerNameOrKey);
  const dimensionIndex = new Map(
    STREAMER_SKILL_DIMENSIONS.map((item, index) => [item.key, index]),
  );
  return history
    .filter((rating) =>
      normalizeStreamerKey(rating.streamerKey) === streamerKey
      && (!dimension || rating.dimension === dimension)
      && isSkillDimension(rating.dimension)
    )
    .map((rating) => {
      const sampleCount = evidenceSourceCount(rating.evidence, rating.dimension);
      const score = sampleCount >= MIN_SKILL_SCORE_SOURCES ? rating.score : null;
      return {
        ratingId: rating.id,
        period: rating.period,
        dimension: rating.dimension,
        label: STREAMER_SKILL_DIMENSIONS.find((item) => item.key === rating.dimension)?.label || rating.dimension,
        at: rating.createdAt,
        score,
        displayScore: score === null ? "数据不足" : String(score),
        sampleCount,
        basis: rating.basis,
        version: rating.version,
        evidence: rating.evidence.map((item) => ({ ...item })),
      };
    })
    .sort((left, right) => {
      const timeDifference = (validTimestamp(left.at) ?? Number.NEGATIVE_INFINITY)
        - (validTimestamp(right.at) ?? Number.NEGATIVE_INFINITY);
      return timeDifference
        || (dimensionIndex.get(left.dimension) ?? 0) - (dimensionIndex.get(right.dimension) ?? 0)
        || left.version - right.version;
    });
}

export function buildRatingConfidence(
  history: readonly SkillRatingVersion[],
  streamerNameOrKey: string,
  period: SkillRatingPeriod = "current",
): RatingConfidence {
  const rows = buildSkillScoreTable(history, streamerNameOrKey, period);
  const qualified = rows.filter((row) => row.score !== null);
  const sources = new Set<string>();
  qualified.forEach((row) => {
    row.evidence.forEach((item) => {
      if (
        item.humanConfirmed === true
        && item.reviewStatus === "human_confirmed"
        && item.dimension === row.dimension
      ) {
        const sourceId = normalizeIdentifier(item.sourceId).toLowerCase();
        if (sourceId) sources.add(sourceId);
      }
    });
  });
  const qualifiedDimensionCount = qualified.length;
  const totalDimensionCount = STREAMER_SKILL_DIMENSIONS.length;
  const dimensionSampleCount = qualified.reduce((total, row) => total + row.sampleCount, 0);
  const coveragePercent = Math.round((qualifiedDimensionCount / totalDimensionCount) * 100);
  return {
    period,
    qualifiedDimensionCount,
    totalDimensionCount,
    sampleCount: sources.size,
    dimensionSampleCount,
    minimumSamplesPerDimension: MIN_SKILL_SCORE_SOURCES,
    coveragePercent,
    label: `${qualifiedDimensionCount}/${totalDimensionCount} 维达到每维 ${MIN_SKILL_SCORE_SOURCES} 个来源门槛 · ${sources.size} 个不重复分析来源`,
  };
}

export type ContentAnalysisCacheStatus = "ok" | "partial" | "invalid";

export interface ContentAnalysisSourceSummary {
  storageKey: string;
  sourceKey: string;
  sourceTitle: string;
  updatedAt: string | null;
  discoveryCompleted: boolean;
  status: ContentAnalysisCacheStatus;
  candidateCount: number;
  reviewCount: number;
  masterComparisonCount: number;
  fullyAnalyzedCandidateCount: number;
  completionPercent: number;
  error: string | null;
}

export interface ContentAnalysisCompletionSummary {
  sourceCount: number;
  degradedSourceCount: number;
  damagedSourceCount: number;
  candidateCount: number;
  evidenceCandidateCount: number;
  completionPercent: number;
}

export interface ContentAnalysisCacheParseResult {
  sources: ContentAnalysisSourceSummary[];
  evidenceCandidates: SkillEvidence[];
  summary: ContentAnalysisCompletionSummary;
}

export interface KeyValueStorageLike {
  readonly length: number;
  key(index: number): string | null;
  getItem(key: string): string | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function recordMap(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

function recordText(value: unknown): string {
  return typeof value === "string" || typeof value === "number"
    ? normalizeWhitespace(value)
    : "";
}

function firstText(...values: unknown[]): string {
  for (const value of values) {
    const text = recordText(value);
    if (text) return text;
  }
  return "";
}

function joinedText(value: unknown): string {
  if (Array.isArray(value)) return normalizeWhitespace(value.map(recordText).filter(Boolean).join("；"));
  return recordText(value);
}

function parsedCandidateSummary(candidate: Record<string, unknown>): string {
  return firstText(
    candidate.keySentence,
    candidate.key_sentence,
    candidate.evidence,
    candidate.reason,
    candidate.originalText,
    candidate.original_text,
    candidate.type,
    candidate.id,
  );
}

function parsedEvidenceBasis(
  candidate: Record<string, unknown>,
  review: Record<string, unknown>,
  comparison: Record<string, unknown>,
): string {
  const beginner = recordMap(review.beginner);
  const comparisonBody = recordMap(comparison.comparison);
  const reviewText = firstText(beginner.summary, review.review, review.spokenScript, review.trainingChecklist);
  const comparisonReasons = firstText(joinedText(comparisonBody.reasons), joinedText(comparison.reasons));
  const comparisonScore = typeof comparisonBody.totalScore === "number"
    && Number.isFinite(comparisonBody.totalScore)
    ? `对照分 ${comparisonBody.totalScore}`
    : "";
  const comparisonText = normalizeWhitespace([comparisonReasons, comparisonScore].filter(Boolean).join("；"));
  return normalizeWhitespace([
    reviewText ? `复盘：${reviewText}` : "",
    comparisonText ? `母稿对照：${comparisonText}` : "",
    parsedCandidateSummary(candidate),
  ].filter(Boolean).join("；"));
}

function invalidContentAnalysisSource(storageKey: string, sourceKey: string): ContentAnalysisSourceSummary {
  return {
    storageKey,
    sourceKey,
    sourceTitle: "",
    updatedAt: null,
    discoveryCompleted: false,
    status: "invalid",
    candidateCount: 0,
    reviewCount: 0,
    masterComparisonCount: 0,
    fullyAnalyzedCandidateCount: 0,
    completionPercent: 0,
    error: "缓存损坏，已跳过该来源",
  };
}

/**
 * Converts v3 analysis caches to pending evidence. Existing review/comparison
 * output is never treated as a human-confirmed skill score.
 */
export function parseContentAnalysisCacheEntries(
  entries: Iterable<readonly [string, unknown]>,
): ContentAnalysisCacheParseResult {
  const sources: ContentAnalysisSourceSummary[] = [];
  const evidenceCandidates: SkillEvidence[] = [];

  for (const [rawKey, rawValue] of entries) {
    const storageKey = String(rawKey || "");
    if (!storageKey.startsWith(CONTENT_ANALYSIS_CACHE_PREFIX)) continue;
    const sourceKey = storageKey.slice(CONTENT_ANALYSIS_CACHE_PREFIX.length);
    let saved: unknown;
    try {
      saved = typeof rawValue === "string" ? JSON.parse(rawValue) : rawValue;
    } catch {
      sources.push(invalidContentAnalysisSource(storageKey, sourceKey));
      continue;
    }
    if (!isRecord(saved)) {
      sources.push(invalidContentAnalysisSource(storageKey, sourceKey));
      continue;
    }

    const hasCandidates = Array.isArray(saved.candidates);
    const hasReviews = isRecord(saved.reviews);
    const hasComparisons = isRecord(saved.masterComparisons);
    const candidates = hasCandidates
      ? (saved.candidates as unknown[]).filter(isRecord)
      : [];
    const reviews = recordMap(saved.reviews);
    const comparisons = recordMap(saved.masterComparisons);
    const identified = candidates
      .map((candidate) => ({ candidate, id: recordText(candidate.id ?? candidate.segment_id) }))
      .filter((item) => Boolean(item.id));
    let reviewCount = 0;
    let masterComparisonCount = 0;
    let fullyAnalyzedCandidateCount = 0;

    identified.forEach(({ candidate, id: candidateId }) => {
      const review = reviews[candidateId];
      const comparison = comparisons[candidateId];
      const hasReview = isRecord(review);
      const hasComparison = isRecord(comparison);
      if (hasReview) reviewCount += 1;
      if (hasComparison) masterComparisonCount += 1;
      if (!hasReview || !hasComparison) return;
      fullyAnalyzedCandidateCount += 1;
      const summary = parsedCandidateSummary(candidate) || `候选片段 ${candidateId}`;
      evidenceCandidates.push({
        id: `${sourceKey}#${candidateId}`,
        sourceId: sourceKey,
        candidateId,
        dimension: null,
        summary,
        basis: parsedEvidenceBasis(candidate, review, comparison) || summary,
        reviewStatus: "pending_review",
        humanConfirmed: false,
        reviewedBy: null,
        reviewedAt: null,
        sourceUpdatedAt: validTimestamp(saved.updatedAt) === null ? null : recordText(saved.updatedAt),
      });
    });

    const candidateCount = identified.length;
    const discoveryCompleted = saved.discoveryCompleted === true;
    sources.push({
      storageKey,
      sourceKey,
      sourceTitle: recordText(saved.sourceTitle),
      updatedAt: validTimestamp(saved.updatedAt) === null ? null : recordText(saved.updatedAt),
      discoveryCompleted,
      status: hasCandidates && hasReviews && hasComparisons ? "ok" : "partial",
      candidateCount,
      reviewCount,
      masterComparisonCount,
      fullyAnalyzedCandidateCount,
      completionPercent: candidateCount > 0
        ? Math.round((fullyAnalyzedCandidateCount / candidateCount) * 100)
        : discoveryCompleted ? 100 : 0,
      error: hasCandidates && hasReviews && hasComparisons
        ? null
        : "缓存字段不完整，已保留可读取部分",
    });
  }

  sources.sort((left, right) => left.sourceKey.localeCompare(right.sourceKey));
  evidenceCandidates.sort((left, right) =>
    left.sourceId.localeCompare(right.sourceId) || left.candidateId.localeCompare(right.candidateId)
  );
  const candidateCount = sources.reduce((total, source) => total + source.candidateCount, 0);
  const evidenceCandidateCount = sources.reduce(
    (total, source) => total + source.fullyAnalyzedCandidateCount,
    0,
  );
  const completedEmptySources = sources.filter((source) =>
    source.candidateCount === 0 && source.discoveryCompleted && source.status !== "invalid"
  ).length;
  const completionPercent = candidateCount > 0
    ? Math.round((evidenceCandidateCount / candidateCount) * 100)
    : sources.length > 0
      ? Math.round((completedEmptySources / sources.length) * 100)
      : 0;

  return {
    sources,
    evidenceCandidates,
    summary: {
      sourceCount: sources.length,
      degradedSourceCount: sources.filter((source) => source.status !== "ok").length,
      damagedSourceCount: sources.filter((source) => source.status === "invalid").length,
      candidateCount,
      evidenceCandidateCount,
      completionPercent,
    },
  };
}

export function readContentAnalysisStorage(
  storage: KeyValueStorageLike,
): ContentAnalysisCacheParseResult {
  const entries: Array<readonly [string, unknown]> = [];
  let length = 0;
  try {
    length = Math.max(0, Number(storage.length) || 0);
  } catch {
    return parseContentAnalysisCacheEntries(entries);
  }
  for (let index = 0; index < length; index += 1) {
    let key: string | null = null;
    try {
      key = storage.key(index);
    } catch {
      continue;
    }
    if (!key || !key.startsWith(CONTENT_ANALYSIS_CACHE_PREFIX)) continue;
    try {
      entries.push([key, storage.getItem(key)]);
    } catch {
      entries.push([key, undefined]);
    }
  }
  return parseContentAnalysisCacheEntries(entries);
}

export interface StreamerTrainingTask {
  threadId: string;
  title: string;
  url: string;
}

const STREAMER_TRAINING_TASKS: Readonly<Record<string, Omit<StreamerTrainingTask, "url">>> = {
  [normalizeStreamerKey("罗雨欣")]: {
    threadId: "01a0479c-9d06-7ca2-9ab1-8806b33272ec",
    title: "Project-003｜07 罗雨欣直播情景训练",
  },
};

export function getStreamerTrainingTask(streamerName: string): StreamerTrainingTask | null {
  const task = STREAMER_TRAINING_TASKS[normalizeStreamerKey(streamerName)];
  return task
    ? { ...task, url: `codex://threads/${task.threadId}` }
    : null;
}
