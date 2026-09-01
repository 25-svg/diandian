import type { SessionDiagnosis } from "./sessionDiagnosis";

export const COACH_ANALYSIS_CACHE_PREFIX = "bsr:content-analysis:v3:";
export const COACH_STORAGE_PREFIX = "bsr:streamer-coach:v1:";
export const UNASSIGNED_COACH_HOST_ID = "__unassigned__";

export type CoachEvidenceStatus =
  | "comment_present"
  | "speech_video_only"
  | "needs_confirmation";

export type CoachHost = {
  id: string;
  displayName: string;
};

export type CoachEvidenceItem = {
  id: string;
  label: string;
  startSec: number | null;
  endSec: number | null;
  evidence: string;
  suggestion: string;
  checks: string[];
};

export type CoachSession = {
  sourceKey: string;
  title: string;
  updatedAt: string;
  hostId: string;
  hostName: string;
  identityConfirmed: boolean;
  evidenceStatus: CoachEvidenceStatus;
  commentCount: number | null;
  diagnosis: SessionDiagnosis | null;
  evidenceItems: CoachEvidenceItem[];
};

export type CoachMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  createdAt: string;
  evidenceSourceKeys: string[];
};

export type CoachHostSummary = {
  host: CoachHost;
  sessions: CoachSession[];
  sessionCount: number;
  reviewedEvidenceCount: number;
  pendingConfirmationCount: number;
  commentSessionCount: number;
  averageScore: number | null;
  recurringIssues: Array<{ label: string; count: number }>;
  nextActions: string[];
};

export const DEFAULT_COACH_HOSTS: readonly CoachHost[] = [
  { id: "ROLE-LUO", displayName: "罗雨欣" },
  { id: "ROLE-HOU", displayName: "侯梦娜" },
  { id: "ROLE-XIAOE", displayName: "小鹅" },
  { id: "ROLE-YU", displayName: "于千惠" },
];

type StoredCoachAnalysis = {
  sourceTitle?: unknown;
  updatedAt?: unknown;
  anchorName?: unknown;
  anchor_name?: unknown;
  streamerName?: unknown;
  streamer_name?: unknown;
  anchorIdentityConfirmed?: unknown;
  anchor_identity_confirmed?: unknown;
  commentEvidenceStatus?: unknown;
  comment_evidence_status?: unknown;
  commentCount?: unknown;
  comment_count?: unknown;
  candidates?: unknown;
  reviews?: unknown;
  diagnosis?: unknown;
};

function object(value: unknown): Record<string, any> {
  return value && typeof value === "object" ? value as Record<string, any> : {};
}

function text(value: unknown): string {
  return typeof value === "string" || typeof value === "number"
    ? String(value).trim().replace(/\s+/gu, " ")
    : "";
}

function finiteNumber(value: unknown): number | null {
  const result = Number(value);
  return Number.isFinite(result) ? result : null;
}

function stringList(value: unknown, limit = 6): string[] {
  if (!Array.isArray(value)) return [];
  return [...new Set(value.map(text).filter(Boolean))].slice(0, limit);
}

function normalizeDiagnosis(value: unknown): SessionDiagnosis | null {
  const source = object(value);
  if (!text(source.headline)) return null;
  const stats = object(source.stats);
  return {
    headline: text(source.headline),
    highlights: stringList(source.highlights),
    issues: stringList(source.issues),
    confirmations: stringList(source.confirmations),
    nextActions: stringList(source.nextActions),
    stats: {
      totalCandidates: finiteNumber(stats.totalCandidates) ?? 0,
      reviewedCount: finiteNumber(stats.reviewedCount) ?? 0,
      pendingCount: finiteNumber(stats.pendingCount) ?? 0,
      matchedCount: finiteNumber(stats.matchedCount) ?? 0,
      unmatchedCount: finiteNumber(stats.unmatchedCount) ?? 0,
      highScoreCount: finiteNumber(stats.highScoreCount) ?? 0,
      riskCount: finiteNumber(stats.riskCount) ?? 0,
      averageScore: finiteNumber(stats.averageScore),
    },
  };
}

function resolveHost(
  source: StoredCoachAnalysis,
  hosts: readonly CoachHost[],
): { hostId: string; hostName: string; identityConfirmed: boolean } {
  const name = text(
    source.anchorName ?? source.anchor_name ?? source.streamerName ?? source.streamer_name,
  );
  const explicitlyConfirmed = source.anchorIdentityConfirmed === true
    || source.anchor_identity_confirmed === true;
  if (!name || !explicitlyConfirmed) {
    return {
      hostId: UNASSIGNED_COACH_HOST_ID,
      hostName: "待确认主播",
      identityConfirmed: false,
    };
  }
  const matched = hosts.find((host) => host.displayName === name);
  return {
    hostId: matched?.id || `HOST-${encodeURIComponent(name)}`,
    hostName: name,
    identityConfirmed: true,
  };
}

function resolveEvidenceStatus(source: StoredCoachAnalysis): CoachEvidenceStatus {
  const explicit = text(source.commentEvidenceStatus ?? source.comment_evidence_status);
  if (explicit === "available" || explicit === "comment_present") return "comment_present";
  if (explicit === "missing" || explicit === "speech_video_only") return "speech_video_only";
  return "needs_confirmation";
}

function evidenceItems(source: StoredCoachAnalysis): CoachEvidenceItem[] {
  const candidates = Array.isArray(source.candidates) ? source.candidates : [];
  const reviews = object(source.reviews);
  return candidates.flatMap((candidateValue, index) => {
    const candidate = object(candidateValue);
    const id = text(candidate.id) || `candidate-${index + 1}`;
    const review = object(reviews[id]);
    const beginner = object(review.beginner);
    const evidence = text(
      candidate.evidence ?? candidate.originalText ?? candidate.original_text ?? candidate.keySentence,
    );
    const suggestion = text(review.spokenScript ?? beginner.summary);
    if (!evidence && !suggestion) return [];
    return [{
      id,
      label: text(candidate.scene ?? candidate.product ?? candidate.type) || `片段 ${index + 1}`,
      startSec: finiteNumber(candidate.start),
      endSec: finiteNumber(candidate.end),
      evidence,
      suggestion,
      checks: stringList(beginner.checks, 4),
    }];
  }).slice(0, 12);
}

export function parseCoachAnalysisEntry(
  storageKey: string,
  rawValue: unknown,
  hosts: readonly CoachHost[] = DEFAULT_COACH_HOSTS,
): CoachSession | null {
  if (!storageKey.startsWith(COACH_ANALYSIS_CACHE_PREFIX)) return null;
  const sourceKey = storageKey.slice(COACH_ANALYSIS_CACHE_PREFIX.length);
  let parsed = rawValue;
  if (typeof rawValue === "string") {
    try {
      parsed = JSON.parse(rawValue);
    } catch {
      return null;
    }
  }
  const source = object(parsed) as StoredCoachAnalysis;
  const resolvedHost = resolveHost(source, hosts);
  const count = finiteNumber(source.commentCount ?? source.comment_count);
  return {
    sourceKey,
    title: text(source.sourceTitle) || sourceKey,
    updatedAt: text(source.updatedAt) || new Date(0).toISOString(),
    ...resolvedHost,
    evidenceStatus: resolveEvidenceStatus(source),
    commentCount: count === null ? null : Math.max(0, Math.floor(count)),
    diagnosis: normalizeDiagnosis(source.diagnosis),
    evidenceItems: evidenceItems(source),
  };
}

export function parseCoachAnalysisEntries(
  entries: Iterable<readonly [string, unknown]>,
  hosts: readonly CoachHost[] = DEFAULT_COACH_HOSTS,
): CoachSession[] {
  const sessions: CoachSession[] = [];
  for (const [key, value] of entries) {
    const parsed = parseCoachAnalysisEntry(key, value, hosts);
    if (parsed) sessions.push(parsed);
  }
  return sessions.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
}

function topRecurring(values: readonly string[], limit = 3): Array<{ label: string; count: number }> {
  const counts = new Map<string, number>();
  values.map(text).filter(Boolean).forEach((item) => counts.set(item, (counts.get(item) || 0) + 1));
  return [...counts.entries()]
    .map(([label, count]) => ({ label, count }))
    .sort((left, right) => right.count - left.count || left.label.localeCompare(right.label, "zh-CN"))
    .slice(0, limit);
}

export function buildCoachHostSummaries(
  hosts: readonly CoachHost[],
  sessions: readonly CoachSession[],
): CoachHostSummary[] {
  return hosts.map((host) => {
    const owned = sessions.filter((session) => session.hostId === host.id);
    const scores = owned
      .map((session) => session.diagnosis?.stats.averageScore)
      .filter((score): score is number => typeof score === "number" && Number.isFinite(score));
    const nextActions = [...new Set(owned.flatMap((session) => session.diagnosis?.nextActions || []))]
      .slice(0, 4);
    return {
      host,
      sessions: owned,
      sessionCount: owned.length,
      reviewedEvidenceCount: owned.reduce((total, session) => total + session.evidenceItems.length, 0),
      pendingConfirmationCount: owned.reduce(
        (total, session) => total + (session.diagnosis?.confirmations.length || 0),
        0,
      ),
      commentSessionCount: owned.filter((session) => session.evidenceStatus === "comment_present").length,
      averageScore: scores.length
        ? Math.round(scores.reduce((total, score) => total + score, 0) / scores.length)
        : null,
      recurringIssues: topRecurring(owned.flatMap((session) => session.diagnosis?.issues || [])),
      nextActions,
    };
  });
}

export function coachMessageStorageKey(hostId: string): string {
  return `${COACH_STORAGE_PREFIX}messages:${hostId}`;
}

export function parseCoachMessages(value: unknown): CoachMessage[] {
  let parsed = value;
  if (typeof value === "string") {
    try {
      parsed = JSON.parse(value);
    } catch {
      return [];
    }
  }
  if (!Array.isArray(parsed)) return [];
  return parsed.flatMap((item, index) => {
    const source = object(item);
    const role = source.role === "assistant" ? "assistant" : source.role === "user" ? "user" : null;
    const content = text(source.content);
    if (!role || !content) return [];
    return [{
      id: text(source.id) || `message-${index + 1}`,
      role,
      content,
      createdAt: text(source.createdAt) || new Date(0).toISOString(),
      evidenceSourceKeys: stringList(source.evidenceSourceKeys, 12),
    } as CoachMessage];
  }).slice(-80);
}

function safeEvidenceLine(value: string): string {
  return value.replace(/[<>]/gu, "").slice(0, 360);
}

export function buildCoachSystemPrompt(host: CoachHost, sessions: readonly CoachSession[]): string {
  const evidence = sessions.slice(0, 8).map((session) => {
    const diagnosis = session.diagnosis;
    const items = session.evidenceItems.slice(0, 3).map((item) =>
      `片段 ${item.startSec ?? "?"}-${item.endSec ?? "?"} 秒：${safeEvidenceLine(item.evidence || "无逐字证据")}`
    );
    return [
      `场次：${safeEvidenceLine(session.title)}（${session.sourceKey}）`,
      `评论状态：${session.evidenceStatus}`,
      `诊断：${safeEvidenceLine(diagnosis?.headline || "尚无诊断")}`,
      `问题：${(diagnosis?.issues || []).map(safeEvidenceLine).join("；") || "尚无经审核结论"}`,
      `待确认：${(diagnosis?.confirmations || []).map(safeEvidenceLine).join("；") || "无"}`,
      ...items,
    ].join("\n");
  }).join("\n\n");

  return `你是二手3C数码相机直播电商主播教练，当前只辅导主播“${host.displayName}”。
你的职责覆盖开播前准备、直播中问题梳理、下播复盘、专项训练和日常问答。

硬性规则：
1. 只把下方场次摘要当作本主播证据；绝不把其他主播资料混入。
2. 价格、库存、成色、型号参数、赠品、链接和售后等商品事实，证据未明确时必须写“待确认”并建议核实，不得补写。
3. comment_present 只表示本场存在评论记录，不表示已形成“评论+主播回答+视频片段”三联证据；只有明确给出的片段内容才可引用。
4. speech_video_only 表示本场无可用评论，只能分析主播口播或视频；禁止生成一条不存在的真实评论。
5. 你的回答和参考话术都标为“AI教练建议，需人工审核”，不得冒充主播真实原话或企业标准稿。
6. 先回答当前问题，再给最多3条可执行建议。证据不足时明确说明缺什么。
7. 不虚构履历、场次数、收益率、平台规则或稀缺库存。

当前可用场次证据：
${evidence || "暂无本主播已确认场次证据。只能提供通用方法，并明确标注无场次依据。"}`;
}
