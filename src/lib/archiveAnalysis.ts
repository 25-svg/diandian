export type DiscoverySegmentType =
  | "完整成交链路"
  | "高质量金句"
  | "需求判断"
  | "产品推荐"
  | "产品讲解"
  | "异议处理"
  | "留人钩子"
  | "信任建立"
  | "售后与风险消除"
  | "价格、优惠或链接承接"
  | "催单与成交确认"
  | "需要改进的反面案例";

export type LegacyCandidateType =
  | "完整成交链路"
  | "关键话术片段"
  | "成交收口"
  | "成交片段"
  | "疑似成交片段"
  | "问价未见成交信号"
  | "转品/上链接片段"
  | "讲得散片段"
  | "无法判断";

export type CandidateType = DiscoverySegmentType | LegacyCandidateType;
export type DiscoveryOutcome =
  | "confirmed_conversion"
  | "conversion_signal"
  | "unconfirmed"
  | "no_conversion";

export type AnalysisPurpose = "enterprise_review" | "competitor_benchmark";

export type AnalysisProfile = {
  analysisPurpose: AnalysisPurpose;
  competitorName: string;
  masterScriptKey: string;
};

/** Metadata is stored in videos.note so imported recordings need no schema fork. */
export function parseAnalysisProfile(note: string | null | undefined): AnalysisProfile {
  const fallback: AnalysisProfile = {
    analysisPurpose: "enterprise_review",
    competitorName: "",
    masterScriptKey: "",
  };
  if (!note?.trim()) return fallback;
  try {
    const value = JSON.parse(note) as Partial<AnalysisProfile>;
    return {
      analysisPurpose: value.analysisPurpose === "competitor_benchmark"
        ? "competitor_benchmark"
        : "enterprise_review",
      competitorName: typeof value.competitorName === "string" ? value.competitorName.trim() : "",
      masterScriptKey: typeof value.masterScriptKey === "string" ? value.masterScriptKey.trim() : "",
    };
  } catch {
    return fallback;
  }
}

/** Competitor recordings never have order evidence, even if an LLM guesses a sale. */
export function competitorDiscoveryOutcome(outcome: DiscoveryOutcome | string): Exclude<DiscoveryOutcome, "confirmed_conversion"> {
  return outcome === "confirmed_conversion" ? "conversion_signal" : outcome === "no_conversion"
    ? "no_conversion"
    : outcome === "conversion_signal"
      ? "conversion_signal"
      : "unconfirmed";
}

export type DiscoveryEvidence = {
  time: string;
  quote: string;
};

export type CandidateConfidence = "高" | "中" | "低";
export type HighlightTier = "核心高光" | "候选高光";
export type CandidateVerificationStatus =
  | "unverified"
  | "checking"
  | "verified_complete"
  | "partial"
  | "closing"
  | "failed";
export type SalesChainStage =
  | "客户需求/疑问"
  | "产品匹配"
  | "卖点或价值说明"
  | "风险消除/售后承诺"
  | "价格/链接/优惠"
  | "引导下单"
  | "确认成交";

export const COMPLETE_SALES_CHAIN_STAGES: readonly SalesChainStage[] = [
  "客户需求/疑问",
  "产品匹配",
  "卖点或价值说明",
  "风险消除/售后承诺",
  "价格/链接/优惠",
  "引导下单",
  "确认成交",
];

export const INTERRUPTED_CHAIN_MAX_GAP_SECONDS = 300;

export function interruptedChainContextWindow(start: number, end: number): { start: number; end: number } {
  return {
    start: Math.max(0, start - INTERRUPTED_CHAIN_MAX_GAP_SECONDS),
    end: end + INTERRUPTED_CHAIN_MAX_GAP_SECONDS,
  };
}

export type CandidateInput = {
  id?: string;
  segment_id?: string;
  start?: number | string;
  end?: number | string;
  start_time?: number | string;
  end_time?: number | string;
  type?: CandidateType | string;
  confidence?: CandidateConfidence | string;
  tier?: HighlightTier | string;
  score?: number | string;
  product?: string;
  evidence?: string;
  reason?: string;
  verify?: string;
  signals?: unknown;
  chainStages?: unknown;
  hook?: string;
  takeaway?: string;
  verificationStatus?: CandidateVerificationStatus | string;
  scene?: string;
  customer_need?: string;
  customerNeed?: string;
  original_text?: string;
  originalText?: string;
  key_sentence?: string;
  keySentence?: string;
  outcome?: DiscoveryOutcome | string;
  interrupted?: boolean;
  why_selected?: string;
  whySelected?: string;
  evidence_items?: unknown;
  evidenceItems?: unknown;
};

export type HighlightCandidate = {
  id: string;
  start: number;
  end: number;
  type: CandidateType;
  confidence: CandidateConfidence;
  tier: HighlightTier;
  score: number;
  product: string;
  evidence: string;
  reason: string;
  verify: string;
  signals: string[];
  chainStages: SalesChainStage[];
  hook: string;
  takeaway: string;
  verificationStatus: CandidateVerificationStatus;
  scene: string;
  customerNeed: string;
  originalText: string;
  keySentence: string;
  outcome: DiscoveryOutcome;
  interrupted: boolean;
  whySelected: string;
  evidenceItems: DiscoveryEvidence[];
};

export type BeginnerReview = {
  verdict: string;
  summary: string;
  goodPoints: string[];
  improvements: string[];
  checks: string[];
};

export type MasterComparisonAdmission = "blocked" | "analysis_only" | "review_only" | "candidate_queue";

export type SessionComparisonLike = {
  comparison: {
    totalScore: number | null;
    masterSectionId: number | null;
    admission: MasterComparisonAdmission | null;
    risks: string[];
  };
};

export type SessionReviewSummary = {
  totalCandidates: number;
  reviewedCount: number;
  pendingCount: number;
  matchedCount: number;
  unmatchedCount: number;
  highScoreCount: number;
  riskCount: number;
  averageScore: number | null;
};

export type SessionCandidateWork = "novice_review" | "master_comparison";

export function sessionCandidateWorkPlan(
  hasNoviceReview: boolean,
  hasMasterComparison: boolean,
): SessionCandidateWork[] {
  const work: SessionCandidateWork[] = [];
  if (!hasNoviceReview) work.push("novice_review");
  if (!hasMasterComparison) work.push("master_comparison");
  return work;
}

/**
 * Summarize only the candidates currently displayed for this recording.
 * Persisted comparisons from an older discovery pass must not affect the review.
 */
export function summarizeSessionReview(
  candidates: readonly Pick<HighlightCandidate, "id">[],
  comparisons: Readonly<Record<string, SessionComparisonLike | undefined>>,
  noviceReviews: Readonly<Record<string, unknown>>,
): SessionReviewSummary {
  const reviewed = candidates
    .filter((candidate) => Boolean(noviceReviews[candidate.id]))
    .map((candidate) => comparisons[candidate.id])
    .filter((comparison): comparison is SessionComparisonLike => Boolean(comparison));
  const scored = reviewed
    .map((item) => item.comparison.totalScore)
    .filter((score): score is number => typeof score === "number");
  const matchedCount = reviewed.filter((item) => item.comparison.masterSectionId !== null).length;

  return {
    totalCandidates: candidates.length,
    reviewedCount: reviewed.length,
    pendingCount: Math.max(0, candidates.length - reviewed.length),
    matchedCount,
    unmatchedCount: reviewed.length - matchedCount,
    highScoreCount: reviewed.filter((item) =>
      (item.comparison.totalScore || 0) >= 85 && item.comparison.admission === "candidate_queue",
    ).length,
    riskCount: reviewed.filter((item) =>
      (item.comparison.risks || []).length > 0 || item.comparison.admission === "blocked",
    ).length,
    averageScore: scored.length
      ? Math.round(scored.reduce((total, score) => total + score, 0) / scored.length)
      : null,
  };
}

type MatchableMasterSection = {
  id: number;
  sectionKind: string;
  productCardId: string | null;
  title: string;
};

export type MasterSectionMatch = {
  sectionId: number | null;
  status: "matched" | "ambiguous" | "unmatched";
  strategy: "product-card" | "title" | "single-product" | "scene" | null;
};

function normalizeProductText(value: string): string {
  return String(value || "")
    .toUpperCase()
    .replace(/(?:99|95|9)\s*新/g, "")
    .replace(/[^A-Z0-9\u4e00-\u9fff]/g, "");
}

function longestCommonAsciiRun(left: string, right: string): number {
  const a = left.replace(/[^A-Z0-9]/g, "");
  const b = right.replace(/[^A-Z0-9]/g, "");
  let best = 0;
  const row = new Array(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i += 1) {
    for (let j = b.length; j >= 1; j -= 1) {
      row[j] = a[i - 1] === b[j - 1] ? row[j - 1] + 1 : 0;
      best = Math.max(best, row[j]);
    }
  }
  return best;
}

function titleMatchesProduct(product: string, title: string): boolean {
  const normalizedProduct = normalizeProductText(product);
  const normalizedTitle = normalizeProductText(title);
  if (!normalizedProduct || !normalizedTitle) return false;
  if (
    (normalizedProduct.length >= 4 && normalizedTitle.includes(normalizedProduct))
    || (normalizedTitle.length >= 4 && normalizedProduct.includes(normalizedTitle))
  ) return true;
  return longestCommonAsciiRun(normalizedProduct, normalizedTitle) >= 4;
}

export function autoMatchMasterSection(
  product: string,
  sections: readonly MatchableMasterSection[],
): MasterSectionMatch {
  const candidates = sections.filter((item) => item.sectionKind === "product" || item.sectionKind === "scenario");
  const normalizedProduct = normalizeProductText(product);
  const productCardMatches = candidates.filter((item) => {
    const card = normalizeProductText(item.productCardId || "");
    return Boolean(card && normalizedProduct && (
      card === normalizedProduct
      || (card.length >= 4 && normalizedProduct.includes(card))
      || (normalizedProduct.length >= 4 && card.includes(normalizedProduct))
    ));
  });
  if (productCardMatches.length === 1) {
    return { sectionId: productCardMatches[0].id, status: "matched", strategy: "product-card" };
  }
  if (productCardMatches.length > 1) {
    return { sectionId: null, status: "ambiguous", strategy: null };
  }

  const titleMatches = candidates.filter((item) => titleMatchesProduct(product, item.title));
  if (titleMatches.length === 1) {
    return { sectionId: titleMatches[0].id, status: "matched", strategy: "title" };
  }
  if (titleMatches.length > 1) {
    return { sectionId: null, status: "ambiguous", strategy: null };
  }

  const productSections = candidates.filter((item) => item.sectionKind === "product");
  if (productSections.length === 1) {
    return { sectionId: productSections[0].id, status: "matched", strategy: "single-product" };
  }
  return { sectionId: null, status: "unmatched", strategy: null };
}

/**
 * Cross-session masters organise reusable selling scenes instead of one section
 * per product. Prefer a concrete product match, then select a clear selling
 * scene from the candidate's evidence. This keeps a missing model code from
 * preventing a legitimate sales highlight from being scored.
 */
export function autoMatchMasterScene(
  candidate: Pick<HighlightCandidate, "type" | "product" | "reason" | "evidence" | "signals" | "hook" | "takeaway">,
  sections: readonly MatchableMasterSection[],
): MasterSectionMatch {
  const productMatch = autoMatchMasterSection(candidate.product, sections);
  if (productMatch.status !== "unmatched") return productMatch;

  const text = [
    candidate.type,
    candidate.reason,
    candidate.evidence,
    candidate.hook,
    candidate.takeaway,
    ...candidate.signals,
  ].join(" ");
  const sceneRules: Array<{ signals: string[]; titles: string[] }> = [
    { signals: ["售后", "保修", "顺丰", "发货", "退换", "到货"], titles: ["售后", "承诺"] },
    { signals: ["异议", "砍价", "质量", "国行", "港版", "过保", "放心"], titles: ["异议", "议价"] },
    { signals: ["链接", "小黄车", "到手价", "优惠券", "报价", "价格"], titles: ["链接", "到手价", "报价"] },
    { signals: ["下单", "成交", "付款", "锁货", "库存", "最后", "稀缺"], titles: ["稀缺", "成交", "逼单"] },
    { signals: ["预算", "新手", "需求", "推荐", "怎么选"], titles: ["需求", "预算"] },
    { signals: ["转品", "上链接", "互动", "公屏", "停留"], titles: ["转场", "互动", "停留"] },
  ];
  for (const rule of sceneRules) {
    if (!rule.signals.some((signal) => text.includes(signal))) continue;
    const matches = sections.filter((section) =>
      rule.titles.some((title) => section.title.includes(title)),
    );
    if (matches.length === 1) {
      return { sectionId: matches[0].id, status: "matched", strategy: "scene" };
    }
  }
  const defaultTitle = candidate.type.includes("问价") ? ["需求", "预算", "链接"] : ["成交", "稀缺", "产品讲解"];
  const defaultMatch = sections.find((section) =>
    defaultTitle.some((title) => section.title.includes(title)),
  );
  return defaultMatch
    ? { sectionId: defaultMatch.id, status: "matched", strategy: "scene" }
    : { sectionId: null, status: "unmatched", strategy: null };
}

export function masterComparisonPresentation(input: {
  admission: MasterComparisonAdmission | null;
  totalScore: number | null;
  reasons: string[];
}): { tone: "neutral" | "success" | "warning"; label: string; detail: string } {
  if (input.admission === null || input.totalScore === null) {
    return { tone: "warning", label: "还没有匹配到母稿章节", detail: "请选择母稿位置后重新评分" };
  }
  if (input.admission === "candidate_queue") {
    return { tone: "success", label: "已进入候选辅稿", detail: `母稿匹配与可复用分 ${input.totalScore}，等待人工审核` };
  }
  if (input.admission === "blocked") {
    return { tone: "warning", label: "暂不能进入候选辅稿", detail: input.reasons[0] || "存在未通过的关键检查" };
  }
  if (input.admission === "review_only") {
    return { tone: "neutral", label: "仅保留复盘", detail: `母稿匹配与可复用分 ${input.totalScore}，未达到 85 分` };
  }
  if (input.admission === "analysis_only") {
    return { tone: "neutral", label: "仅作局部复盘", detail: input.reasons[0] || "该片段不进入母稿评分或辅稿候选" };
  }
  return { tone: "neutral", label: "仅作分析参考", detail: `母稿匹配与可复用分 ${input.totalScore}` };
}

export function isCurrentMasterComparison(
  requestedCandidateId: string,
  requestedGeneration: number,
  selectedCandidateId: string,
  currentGeneration: number,
): boolean {
  return requestedCandidateId === selectedCandidateId && requestedGeneration === currentGeneration;
}

export type AnalysisSourceIdentity =
  | { kind: "archive"; platform: string; roomId: string; liveId: string }
  | { kind: "video"; videoId: number };

export function analysisSourceKey(source: AnalysisSourceIdentity): string {
  if (source.kind === "video") return `video:${source.videoId}`;
  return `archive:${source.platform}:${source.roomId}:${source.liveId}`;
}

export type BackgroundTaskLike = {
  id: string;
  task_type?: string;
  taskType?: string;
  status: string;
  metadata: string;
  message?: string;
};

export function findActiveVideoSubtitleTask(
  tasks: readonly BackgroundTaskLike[],
  videoId: number,
): { id: string; message: string } | null {
  for (const task of tasks) {
    if ((task.task_type || task.taskType) !== "generate_video_subtitle") continue;
    if (task.status !== "pending" && task.status !== "processing") continue;
    try {
      if (Number(JSON.parse(task.metadata || "{}").video_id) === videoId) {
        return { id: task.id, message: String(task.message || "") };
      }
    } catch {
      // Legacy task metadata should not prevent normal transcript work.
    }
  }
  return null;
}

export function findActiveArchiveSubtitleTask(
  tasks: readonly BackgroundTaskLike[],
  platform: string,
  roomId: string,
  liveId: string,
): { id: string; message: string } | null {
  for (const task of tasks) {
    if ((task.task_type || task.taskType) !== "generate_archive_subtitle") continue;
    if (task.status !== "pending" && task.status !== "processing") continue;
    try {
      const metadata = JSON.parse(task.metadata || "{}") as Record<string, unknown>;
      const taskPlatform = String(metadata.platform ?? "");
      const taskRoomId = String(metadata.room_id ?? "");
      const taskLiveId = String(metadata.live_id ?? "");
      if (
        taskPlatform === platform
        && taskRoomId === roomId
        && taskLiveId === liveId
      ) {
        return { id: task.id, message: String(task.message || "") };
      }
    } catch {
      // Legacy task metadata should not prevent normal transcript work.
    }
  }
  return null;
}

export type ArchiveTranscriptAction = "use-existing" | "refresh";

export function selectArchiveTranscriptAction(
  forceRefresh: boolean,
  existingSubtitle: string,
): ArchiveTranscriptAction {
  return !forceRefresh && existingSubtitle.trim() ? "use-existing" : "refresh";
}

export function friendlyArchiveTranscriptError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error || "");
  const message = raw.replace(/^Failed to invoke [^:]+:\s*/i, "").trim();
  if (/playlist file not found|系统找不到指定的文件|no such file/i.test(message)) {
    return "程序找不到这条录播的源文件，暂时无法重新识别。录播记录仍在，请检查缓存目录或重新导入原视频。";
  }
  if (/已有逐字稿任务/.test(message)) {
    return "该录播正在转写逐字稿，请等待当前任务完成后再试；也可在「任务」页查看进度。";
  }
  return message || raw;
}

export function transcriptRefreshFailureState(hasTranscript: boolean): {
  notice: string;
  stage: string;
} {
  return hasTranscript
    ? { notice: "刷新失败 · 继续使用旧稿", stage: "逐字稿刷新失败，已保留原有分析" }
    : { notice: "源文件不可用", stage: "找不到可分析的录播源文件" };
}

const candidateTypes = new Set<CandidateType>([
  "完整成交链路",
  "高质量金句",
  "需求判断",
  "产品推荐",
  "产品讲解",
  "异议处理",
  "留人钩子",
  "信任建立",
  "售后与风险消除",
  "价格、优惠或链接承接",
  "催单与成交确认",
  "需要改进的反面案例",
  "关键话术片段",
  "成交收口",
  "成交片段",
  "疑似成交片段",
  "问价未见成交信号",
  "转品/上链接片段",
  "讲得散片段",
  "无法判断",
]);

const strongTransactionTypes = new Set<CandidateType>(["完整成交链路"]);

const signalByType: Record<CandidateType, string> = {
  完整成交链路: "完整成交链路",
  高质量金句: "高质量金句",
  需求判断: "需求判断",
  产品推荐: "产品推荐",
  产品讲解: "产品讲解",
  异议处理: "异议处理",
  留人钩子: "留人钩子",
  信任建立: "信任建立",
  "售后与风险消除": "售后与风险消除",
  "价格、优惠或链接承接": "价格与链接承接",
  "催单与成交确认": "催单与成交确认",
  "需要改进的反面案例": "反面案例",
  关键话术片段: "关键话术",
  成交收口: "成交收口",
  成交片段: "明确成交",
  疑似成交片段: "成交意向",
  问价未见成交信号: "问价回应",
  "转品/上链接片段": "链接动作",
  讲得散片段: "主线分散",
  无法判断: "证据不足",
};

function clampScore(value: unknown): number | null {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return null;
  return Math.max(0, Math.min(100, Math.round(parsed)));
}

function fallbackTier(type: CandidateType, confidence: CandidateConfidence): HighlightTier {
  return confidence === "高" && strongTransactionTypes.has(type) ? "核心高光" : "候选高光";
}

function fallbackScore(tier: HighlightTier, confidence: CandidateConfidence): number {
  if (tier === "核心高光") return confidence === "高" ? 88 : 80;
  if (confidence === "高") return 74;
  if (confidence === "中") return 62;
  return 48;
}

function normalizeSignals(value: unknown, type: CandidateType, confidence: CandidateConfidence): string[] {
  if (Array.isArray(value)) {
    const signals = value
      .map((item) => String(item || "").trim())
      .filter(Boolean)
      .slice(0, 4);
    if (signals.length) return signals;
  }
  const signals = [signalByType[type]];
  if (confidence === "高") signals.push("高置信度");
  return signals;
}

function normalizeChainStages(value: unknown): SalesChainStage[] {
  const aliases: Record<string, SalesChainStage> = {
    "客户需求/疑问": "客户需求/疑问",
    "客户需求": "客户需求/疑问",
    "客户疑问": "客户需求/疑问",
    "需求确认": "客户需求/疑问",
    "产品推荐": "产品匹配",
    "产品匹配": "产品匹配",
    "产品说明": "卖点或价值说明",
    "卖点或价值说明": "卖点或价值说明",
    "卖点说明": "卖点或价值说明",
    "价值说明": "卖点或价值说明",
    "风险消除": "风险消除/售后承诺",
    "风险消除/售后承诺": "风险消除/售后承诺",
    "售后承诺": "风险消除/售后承诺",
    "价格链接优惠": "价格/链接/优惠",
    "价格/链接/优惠": "价格/链接/优惠",
    "报价": "价格/链接/优惠",
    "链接": "价格/链接/优惠",
    "引导下单": "引导下单",
    "购买指令": "引导下单",
    "确认成交": "确认成交",
    "成交确认": "确认成交",
  };
  const raw = Array.isArray(value) ? value : [];
  const stages = raw
    .map((item) => aliases[String(item || "").replace(/\s+/g, "").trim()] || null)
    .filter((item): item is SalesChainStage => Boolean(item));
  return COMPLETE_SALES_CHAIN_STAGES.filter((stage) => stages.includes(stage));
}

const discoverySegmentTypes = new Set<DiscoverySegmentType>([
  "完整成交链路",
  "高质量金句",
  "需求判断",
  "产品推荐",
  "产品讲解",
  "异议处理",
  "留人钩子",
  "信任建立",
  "售后与风险消除",
  "价格、优惠或链接承接",
  "催单与成交确认",
  "需要改进的反面案例",
]);

export function isStructuredDiscoveryCandidate(
  candidate: Pick<
    HighlightCandidate,
    "type" | "originalText" | "whySelected" | "evidenceItems"
  >,
): boolean {
  return discoverySegmentTypes.has(candidate.type as DiscoverySegmentType)
    && candidate.originalText.trim().length > 0
    && candidate.whySelected.trim().length > 0
    && candidate.evidenceItems.length > 0;
}

export function candidateNeedsLegacyContextVerification(
  candidate: Pick<
    HighlightCandidate,
    "type" | "originalText" | "whySelected" | "evidenceItems"
  >,
): boolean {
  if (!isStructuredDiscoveryCandidate(candidate)) return true;
  return candidate.evidenceItems.some(
    (item) => !item.time.trim() || !item.quote.trim(),
  );
}

const discoveryOutcomes = new Set<DiscoveryOutcome>([
  "confirmed_conversion",
  "conversion_signal",
  "unconfirmed",
  "no_conversion",
]);

function normalizeTimestampSeconds(value: unknown): number | null {
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  const text = String(value ?? "").trim();
  if (!text) return null;
  if (/^\d+(?:\.\d+)?$/.test(text)) {
    const parsed = Number(text);
    return Number.isFinite(parsed) ? parsed : null;
  }
  const parts = text.split(":");
  if (parts.length !== 2 && parts.length !== 3) return null;
  const values = parts.map(Number);
  if (values.some((part) => !Number.isFinite(part) || part < 0)) return null;
  if (parts.length === 2) {
    const [minutes, seconds] = values;
    if (seconds >= 60) return null;
    return minutes * 60 + seconds;
  }
  const [hours, minutes, seconds] = values;
  if (minutes >= 60 || seconds >= 60) return null;
  return hours * 3600 + minutes * 60 + seconds;
}

function normalizeDiscoveryEvidence(value: unknown): DiscoveryEvidence[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => ({
      time: String(item?.time ?? "").trim(),
      quote: String(item?.quote ?? "").trim(),
    }))
    .filter((item) => item.time && item.quote && normalizeTimestampSeconds(item.time) !== null)
    .slice(0, 8);
}

function compatibilityChainStages(
  type: DiscoverySegmentType,
  outcome: DiscoveryOutcome,
): SalesChainStage[] {
  // Discovery labels are hypotheses. Only verified evidence may populate all seven stages.
  if (type === "完整成交链路") return [];
  if (type === "需求判断") return ["客户需求/疑问"];
  if (type === "产品推荐") return ["产品匹配"];
  if (type === "产品讲解") return ["卖点或价值说明"];
  if (type === "售后与风险消除" || type === "异议处理") return ["风险消除/售后承诺"];
  if (type === "价格、优惠或链接承接") return ["价格/链接/优惠"];
  if (type === "催单与成交确认") {
    return outcome === "confirmed_conversion" ? ["引导下单", "确认成交"] : ["引导下单"];
  }
  return [];
}

export function candidateOutcomeLabel(outcome: DiscoveryOutcome): string {
  if (outcome === "confirmed_conversion") return "已发现成交确认，等待复核";
  if (outcome === "conversion_signal") return "出现成交信号";
  if (outcome === "no_conversion") return "未形成成交";
  return "尚未确认结果";
}

export function buildCandidateDiscoveryPrompt(productDictionary: string): string {
  const dictionary = String(productDictionary || "").trim()
    || "当前未提供企业商品词典。型号不确定时标记为待确认，不得猜测。";
  return `你是“金典拍拍直播复盘分析官”。

你的任务是从直播逐字稿中发现有训练价值的片段，而不是只寻找已经确认成交的片段。

【企业商品词典】
${dictionary}

【候选片段类型】
1. 完整成交链路
2. 高质量金句
3. 需求判断
4. 产品推荐
5. 产品讲解
6. 异议处理
7. 留人钩子
8. 信任建立
9. 售后与风险消除
10. 价格、优惠或链接承接
11. 催单与成交确认
12. 需要改进的反面案例

【发现规则】
- 完整成交链路通常包含：客户需求/疑问 → 产品匹配 → 卖点说明 → 风险消除 → 价格/优惠/链接 → 引导下单 → 确认成交。
- 链路不完整，但某一句表达特别好时，也必须保留，类型标记为“高质量金句”。
- 不得把普通聊天、机械报型号、无依据催单识别为高光片段。
- 片段必须保留足够上下文，能够看懂客户问题、主播回答和结果。
- 优先选择30秒至3分钟的完整语义片段。短于30秒的高质量金句只有在上下文仍然完整时才可保留。
- 对被他人沟通打断的内容，应保留有效话术，但标记“受到打断”，即 interrupted=true。
- 不得根据常识补写逐字稿中没有出现的成交、价格、库存、链接或售后信息。
- 每条结论必须引用真实时间和原话。
- outcome 只能为 confirmed_conversion、conversion_signal、unconfirmed、no_conversion。
- 无合格候选时返回 {"segments":[]}。

【输出要求】
只返回 JSON 对象，不要 Markdown，不要解释文字：
{
  "segments": [
    {
      "segment_id": "SEG-001",
      "start_time": "00:00:00",
      "end_time": "00:01:30",
      "type": "异议处理",
      "scene": "客户担心二手相机质量",
      "customer_need": "客户担心机器存在暗病",
      "original_text": "完整原话",
      "key_sentence": "最值得保留的一句话",
      "outcome": "unconfirmed",
      "interrupted": false,
      "why_selected": "为什么值得进入下一步评分",
      "evidence": [
        {
          "time": "00:00:25",
          "quote": "对应原话"
        }
      ]
    }
  ]
}`;
}

export function buildCompetitorDiscoveryPrompt(productDictionary: string): string {
  return `${buildCandidateDiscoveryPrompt(productDictionary)}

【竞品对照模式】
- 没有订单、成交峰或 GMV 数据；不得断言“已确认成交”。
- outcome 只能是 conversion_signal、unconfirmed 或 no_conversion。
- 恭喜、备注、链接、报价均只表示疑似成交信号。
- 输出仅供与企业母稿对照，竞品原话不得作为企业标准稿。`;
}

export function parseDiscoverySegments(value: unknown): HighlightCandidate[] {
  const items = Array.isArray(value)
    ? value
    : Array.isArray((value as any)?.segments)
      ? (value as any).segments
      : [];
  return items
    .map((item: any, index: number) => {
      const type = String(item?.type || "").trim() as DiscoverySegmentType;
      const outcome = String(item?.outcome || "").trim() as DiscoveryOutcome;
      const start = normalizeTimestampSeconds(item?.start_time ?? item?.start);
      const end = normalizeTimestampSeconds(item?.end_time ?? item?.end);
      const originalText = String(item?.original_text ?? item?.originalText ?? "").trim();
      const evidenceItems = normalizeDiscoveryEvidence(
        item?.evidence_items ?? item?.evidenceItems ?? item?.evidence,
      );
      if (
        !discoverySegmentTypes.has(type)
        || !discoveryOutcomes.has(outcome)
        || start === null
        || end === null
        || end <= start
        || !originalText
        || evidenceItems.length === 0
      ) return null;
      const keySentence = String(item?.key_sentence ?? item?.keySentence ?? "").trim();
      const whySelected = String(item?.why_selected ?? item?.whySelected ?? "").trim();
      const compatibilityInput: CandidateInput = {
        id: String(item?.segment_id || item?.id || `SEG-${String(index + 1).padStart(3, "0")}`),
        start,
        end,
        type,
        confidence: outcome === "confirmed_conversion" ? "高" : "中",
        tier: "候选高光",
        product: String(item?.product || "商品待确认").trim(),
        evidence: evidenceItems.map((entry) => `${entry.time} ${entry.quote}`).join("\n"),
        reason: whySelected,
        verify: outcome === "confirmed_conversion" ? "核验订单或明确成交证据" : "",
        signals: [
          type,
          candidateOutcomeLabel(outcome),
          ...(item?.interrupted ? ["受到打断"] : []),
        ],
        chainStages: compatibilityChainStages(type, outcome),
        hook: keySentence,
        takeaway: whySelected,
        verificationStatus: "unverified",
        scene: String(item?.scene || "").trim(),
        customerNeed: String(item?.customer_need ?? item?.customerNeed ?? "").trim(),
        originalText,
        keySentence,
        outcome,
        interrupted: Boolean(item?.interrupted),
        whySelected,
        evidenceItems,
      };
      return normalizeCandidate(compatibilityInput, index);
    })
    .filter((item): item is HighlightCandidate => Boolean(item));
}

export function parseCandidateDiscoveryResponse(content: string): HighlightCandidate[] {
  const source = String(content || "").trim();
  if (!source) return [];
  const candidates = [
    source.replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/i, ""),
  ];
  const objectStart = source.indexOf("{");
  const objectEnd = source.lastIndexOf("}");
  if (objectStart >= 0 && objectEnd > objectStart) {
    candidates.push(source.slice(objectStart, objectEnd + 1));
  }
  const arrayStart = source.indexOf("[");
  const arrayEnd = source.lastIndexOf("]");
  if (arrayStart >= 0 && arrayEnd > arrayStart) {
    candidates.push(source.slice(arrayStart, arrayEnd + 1));
  }
  for (const candidate of candidates) {
    try {
      const parsed = JSON.parse(candidate);
      const result = parseDiscoverySegments(parsed);
      if (result.length || Array.isArray(parsed) || Array.isArray(parsed?.segments)) return result;
    } catch {
      // Try the next JSON boundary recovered from the model response.
    }
  }
  return [];
}

export function isCompleteSalesChain(candidate: Pick<HighlightCandidate, "type" | "start" | "end" | "chainStages">): boolean {
  return candidate.type === "完整成交链路"
    && candidate.end - candidate.start >= 45
    && COMPLETE_SALES_CHAIN_STAGES.every((stage) => candidate.chainStages.includes(stage));
}

export function missingSalesChainStages(stages: readonly SalesChainStage[]): SalesChainStage[] {
  return COMPLETE_SALES_CHAIN_STAGES.filter((stage) => !stages.includes(stage));
}

export function isMasterScoreEligible(candidate: Pick<
  HighlightCandidate,
  "type" | "start" | "end" | "chainStages" | "verificationStatus"
>): boolean {
  return candidate.verificationStatus === "verified_complete" && isCompleteSalesChain(candidate);
}

export function applyCandidateContextVerification(
  candidate: HighlightCandidate,
  verification: Pick<CandidateInput, "start" | "end" | "type" | "product" | "evidence" | "reason" | "verify" | "signals" | "chainStages" | "hook" | "takeaway">,
): HighlightCandidate {
  const preserveDiscoveryFields = discoverySegmentTypes.has(candidate.type as DiscoverySegmentType)
    && (candidate.evidenceItems.length > 0 || Boolean(candidate.originalText));
  const normalized = normalizeCandidate({
    ...candidate,
    ...verification,
    id: candidate.id,
    verificationStatus: "unverified",
  }, 0);
  if (!normalized) return { ...candidate, verificationStatus: "failed" };
  const verificationStatus: CandidateVerificationStatus = isCompleteSalesChain(normalized)
    ? "verified_complete"
    : normalized.type === "成交收口"
      ? "closing"
      : "partial";
  return {
    ...normalized,
    ...(preserveDiscoveryFields ? {
      type: candidate.type,
      scene: candidate.scene,
      customerNeed: candidate.customerNeed,
      originalText: candidate.originalText,
      keySentence: candidate.keySentence,
      outcome: candidate.outcome,
      interrupted: candidate.interrupted,
      whySelected: candidate.whySelected,
      evidenceItems: candidate.evidenceItems,
    } : {}),
    id: candidate.id,
    verificationStatus,
  };
}

function normalizeReviewList(value: unknown): string[] {
  const source = Array.isArray(value)
    ? value
    : typeof value === "string"
      ? value.split(/\r?\n|；/)
      : [];
  return source
    .map((item) => String(item || "").replace(/^[-*\d.、\s]+/, "").trim())
    .filter(Boolean)
    .slice(0, 4);
}

function decodeRecoveredJsonString(value: string): string {
  try {
    return JSON.parse(`"${value.replace(/\r?\n/g, "\\n")}"`);
  } catch {
    return value.replace(/\\"/g, '"').replace(/\\n/g, "\n").trim();
  }
}

function extractRecoveredStringField(source: string, key: string): string {
  const match = new RegExp(`"${key}"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`).exec(source);
  return match ? decodeRecoveredJsonString(match[1]) : "";
}

function extractRecoveredListField(source: string, key: string): string[] {
  const marker = `"${key}"`;
  const markerIndex = source.indexOf(marker);
  if (markerIndex < 0) return [];
  const arrayStart = source.indexOf("[", markerIndex + marker.length);
  if (arrayStart < 0) return [];
  let depth = 0;
  let escaped = false;
  let inString = false;
  for (let index = arrayStart; index < source.length; index += 1) {
    const character = source[index];
    if (character === '"' && !escaped) inString = !inString;
    if (!inString) {
      if (character === "[") depth += 1;
      if (character === "]") {
        depth -= 1;
        if (depth === 0) {
          const raw = source.slice(arrayStart, index + 1);
          try {
            return normalizeReviewList(JSON.parse(raw));
          } catch {
            return normalizeReviewList([...raw.matchAll(/"((?:\\.|[^"\\])*)"/g)].map((item) => decodeRecoveredJsonString(item[1])));
          }
        }
      }
    }
    if (character === "\\" && !escaped) escaped = true;
    else escaped = false;
  }
  return [];
}

export function normalizeBeginnerReview(input: any, fallbackText = ""): BeginnerReview {
  const fallbackSummary = String(fallbackText || "")
    .split(/\r?\n/)
    .map((line) => line.replace(/^#+\s*/, "").trim())
    .find(Boolean) || "暂时没有足够信息，请人工查看逐字稿。";
  const recoverySource = `${typeof input?.summary === "string" ? input.summary : ""}\n${String(fallbackText || "")}`;
  const goodPoints = normalizeReviewList(input?.good_points ?? input?.goodPoints);
  const improvements = normalizeReviewList(input?.improvements);
  const checks = normalizeReviewList(input?.checks);
  const recoveredGoodPoints = extractRecoveredListField(recoverySource, "good_points");
  const recoveredImprovements = extractRecoveredListField(recoverySource, "improvements");
  const recoveredChecks = extractRecoveredListField(recoverySource, "checks");
  const recoveredVerdict = extractRecoveredStringField(recoverySource, "verdict");
  const recoveredSummary = extractRecoveredStringField(recoverySource, "summary");
  const directSummary = String(input?.summary || "").trim();
  const directSummaryIsRawEnvelope = /^\s*\{\s*"(?:verdict|summary)"\s*:/.test(directSummary);
  return {
    verdict: String(input?.verdict || recoveredVerdict || "建议人工判断").trim(),
    summary: String(directSummary && !directSummaryIsRawEnvelope ? directSummary : recoveredSummary || fallbackSummary).trim(),
    goodPoints: goodPoints.length ? goodPoints : recoveredGoodPoints.length ? recoveredGoodPoints : ["暂未识别到明确优点，可结合视频人工判断"],
    improvements: improvements.length ? improvements : recoveredImprovements.length ? recoveredImprovements : ["先确认商品和用户需求，再给出明确下一步动作"],
    checks: checks.length ? checks : recoveredChecks.length ? recoveredChecks : ["暂无，按逐字稿结论使用"],
  };
}

export function normalizeCandidate(input: CandidateInput, index: number): HighlightCandidate | null {
  const start = normalizeTimestampSeconds(input?.start ?? input?.start_time);
  const end = normalizeTimestampSeconds(input?.end ?? input?.end_time);
  if (start === null || end === null || end <= start) return null;
  if (!candidateTypes.has(input?.type as CandidateType)) return null;

  const requestedType = input.type as CandidateType;
  const confidence: CandidateConfidence = ["高", "中", "低"].includes(String(input.confidence))
    ? input.confidence as CandidateConfidence
    : "低";
  const safeStart = Math.max(0, start);
  const safeEnd = Math.max(0, end);
  const chainStages = normalizeChainStages(input.chainStages);
  const completeChain = safeEnd - safeStart >= 45 && missingSalesChainStages(chainStages).length === 0;
  const closingOnly = chainStages.length > 0 && chainStages.every((stage) => stage === "确认成交");
  const evidenceItems = normalizeDiscoveryEvidence(input.evidenceItems ?? input.evidence_items);
  const originalText = String(input.originalText ?? input.original_text ?? "").trim();
  const whySelected = String(input.whySelected ?? input.why_selected ?? input.reason ?? "").trim();
  const structuredDiscoveryInput = discoverySegmentTypes.has(requestedType as DiscoverySegmentType)
    && originalText.length > 0
    && whySelected.length > 0
    && evidenceItems.length > 0;
  const type = requestedType === "完整成交链路" && !completeChain && !structuredDiscoveryInput
    ? closingOnly ? "成交收口" : "关键话术片段"
    : requestedType;
  const requestedVerificationStatus = String(input.verificationStatus || "");
  const acceptedVerificationStatus: CandidateVerificationStatus | null = [
    "checking",
    "failed",
    "partial",
    "closing",
    "verified_complete",
  ].includes(requestedVerificationStatus)
    ? requestedVerificationStatus as CandidateVerificationStatus
    : null;
  // Persisted status is never trusted over the seven-step evidence. This also
  // repairs legacy cache records that were incorrectly marked as complete.
  const verificationStatus: CandidateVerificationStatus = type === "完整成交链路" && completeChain
    ? acceptedVerificationStatus === "verified_complete" ? "verified_complete" : "unverified"
    : type === "成交收口"
      ? "closing"
      : acceptedVerificationStatus === "checking" || acceptedVerificationStatus === "failed" || acceptedVerificationStatus === "partial"
        ? acceptedVerificationStatus
        : acceptedVerificationStatus === "verified_complete"
          ? "partial"
          : "unverified";
  const explicitTier = input.tier === "核心高光" || input.tier === "候选高光"
    ? input.tier
    : null;
  const tier = explicitTier || fallbackTier(type, confidence);
  const score = clampScore(input.score) ?? fallbackScore(tier, confidence);
  const requestedOutcome = String(input.outcome || "unconfirmed");
  const outcome: DiscoveryOutcome = discoveryOutcomes.has(requestedOutcome as DiscoveryOutcome)
    ? requestedOutcome as DiscoveryOutcome
    : "unconfirmed";
  const keySentence = String(input.keySentence ?? input.key_sentence ?? input.hook ?? "").trim();

  return {
    id: String(input.id || input.segment_id || `C${String(index + 1).padStart(2, "0")}-${Math.round(safeStart)}-${Math.round(safeEnd)}`),
    start: safeStart,
    end: safeEnd,
    type,
    confidence,
    tier,
    score,
    product: String(input.product || "主商品待确认"),
    evidence: String(input.evidence || ""),
    reason: String(input.reason || ""),
    verify: String(input.verify || ""),
    signals: normalizeSignals(input.signals, type, confidence),
    chainStages,
    hook: String(input.hook || ""),
    takeaway: String(input.takeaway || ""),
    verificationStatus,
    scene: String(input.scene || "").trim(),
    customerNeed: String(input.customerNeed ?? input.customer_need ?? "").trim(),
    originalText,
    keySentence,
    outcome,
    interrupted: Boolean(input.interrupted),
    whySelected,
    evidenceItems,
  };
}

export function normalizeCandidates(inputs: CandidateInput[]): HighlightCandidate[] {
  return inputs
    .map((input, index) => normalizeCandidate(input, index))
    .filter((candidate): candidate is HighlightCandidate => Boolean(candidate))
    .sort((left, right) => left.start - right.start || right.score - left.score)
    .map((candidate, index) => ({
      ...candidate,
      id: `C${String(index + 1).padStart(2, "0")}-${Math.round(candidate.start)}-${Math.round(candidate.end)}`,
    }));
}

/**
 * Keep every distinct evidence-backed discovery from the full recording.
 * Discovery ranking affects only display order; it must not silently discard
 * later, independent sales moments because they share a candidate type.
 */
export function selectDiscoveryCandidates(
  items: readonly HighlightCandidate[],
): HighlightCandidate[] {
  const confidenceRank: Record<CandidateConfidence, number> = { 高: 3, 中: 2, 低: 1 };
  const deduped: HighlightCandidate[] = [];
  for (const item of [...items].sort((left, right) =>
    left.start - right.start || confidenceRank[right.confidence] - confidenceRank[left.confidence],
  )) {
    const duplicate = deduped.some((saved) =>
      saved.type === item.type && Math.min(saved.end, item.end) - Math.max(saved.start, item.start) > 10,
    );
    if (!duplicate && item.type !== "无法判断") deduped.push(item);
  }
  return normalizeCandidates(deduped);
}

function isSameSalesMoment(left: HighlightCandidate, right: HighlightCandidate): boolean {
  const leftProduct = normalizeProductText(left.product);
  const rightProduct = normalizeProductText(right.product);
  return left.type === right.type
    && Boolean(leftProduct)
    && leftProduct === rightProduct
    && right.start <= left.end + 15;
}

function joinDistinctText(left?: string, right?: string): string {
  return [...new Set([left, right].map((value) => String(value || "").trim()).filter(Boolean))].join("；");
}

function strongerDiscoveryOutcome(left: DiscoveryOutcome, right: DiscoveryOutcome): DiscoveryOutcome {
  const rank: Record<DiscoveryOutcome, number> = {
    confirmed_conversion: 4,
    conversion_signal: 3,
    unconfirmed: 2,
    no_conversion: 1,
  };
  return rank[right] > rank[left] ? right : left;
}

function mergeDiscoveryEvidence(
  left: readonly DiscoveryEvidence[],
  right: readonly DiscoveryEvidence[],
): DiscoveryEvidence[] {
  const unique = new Map<string, DiscoveryEvidence>();
  for (const item of [...left, ...right]) {
    unique.set(`${item.time}\u0000${item.quote}`, item);
  }
  return [...unique.values()];
}

/**
 * Collapse the same selling action emitted from overlapping transcript chunks.
 * This score remains discovery confidence, never the master-script review score.
 */
export function mergeSalesCandidates(candidates: readonly HighlightCandidate[]): HighlightCandidate[] {
  const merged: HighlightCandidate[] = [];
  for (const candidate of [...candidates].sort((left, right) => left.start - right.start || right.score - left.score)) {
    const previous = merged[merged.length - 1];
    if (!previous || !isSameSalesMoment(previous, candidate)) {
      merged.push(candidate);
      continue;
    }
    const preferred = candidate.score > previous.score ? candidate : previous;
    merged[merged.length - 1] = {
      ...previous,
      end: Math.max(previous.end, candidate.end),
      score: Math.max(previous.score, candidate.score),
      confidence: preferred.confidence,
      evidence: joinDistinctText(previous.evidence, candidate.evidence),
      reason: joinDistinctText(previous.reason, candidate.reason),
      verify: joinDistinctText(previous.verify, candidate.verify),
      signals: [...new Set([...(previous.signals || []), ...(candidate.signals || [])])].slice(0, 4),
      hook: joinDistinctText(previous.hook, candidate.hook),
      takeaway: joinDistinctText(previous.takeaway, candidate.takeaway),
      scene: joinDistinctText(previous.scene, candidate.scene),
      customerNeed: joinDistinctText(previous.customerNeed, candidate.customerNeed),
      originalText: joinDistinctText(previous.originalText, candidate.originalText),
      keySentence: joinDistinctText(previous.keySentence, candidate.keySentence),
      outcome: strongerDiscoveryOutcome(previous.outcome, candidate.outcome),
      interrupted: previous.interrupted || candidate.interrupted,
      whySelected: joinDistinctText(previous.whySelected, candidate.whySelected),
      evidenceItems: mergeDiscoveryEvidence(previous.evidenceItems, candidate.evidenceItems),
    };
  }
  return normalizeCandidates(merged);
}
