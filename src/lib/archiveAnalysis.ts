export type CandidateType =
  | "成交片段"
  | "疑似成交片段"
  | "问价未见成交信号"
  | "转品/上链接片段"
  | "讲得散片段"
  | "无法判断";

export type CandidateConfidence = "高" | "中" | "低";
export type HighlightTier = "核心高光" | "候选高光";

export type CandidateInput = {
  id?: string;
  start?: number | string;
  end?: number | string;
  type?: CandidateType | string;
  confidence?: CandidateConfidence | string;
  tier?: HighlightTier | string;
  score?: number | string;
  product?: string;
  evidence?: string;
  reason?: string;
  verify?: string;
  signals?: unknown;
  hook?: string;
  takeaway?: string;
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
  hook: string;
  takeaway: string;
};

export type BeginnerReview = {
  verdict: string;
  summary: string;
  goodPoints: string[];
  improvements: string[];
  checks: string[];
};

export type MasterComparisonAdmission = "blocked" | "analysis_only" | "review_only" | "candidate_queue";

type MatchableMasterSection = {
  id: number;
  sectionKind: string;
  productCardId: string | null;
  title: string;
};

export type MasterSectionMatch = {
  sectionId: number | null;
  status: "matched" | "ambiguous" | "unmatched";
  strategy: "product-card" | "title" | "single-product" | null;
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

export function masterComparisonPresentation(input: {
  admission: MasterComparisonAdmission | null;
  totalScore: number | null;
  reasons: string[];
}): { tone: "neutral" | "success" | "warning"; label: string; detail: string } {
  if (input.admission === null || input.totalScore === null) {
    return { tone: "warning", label: "还没有匹配到母稿章节", detail: "请选择母稿位置后重新评分" };
  }
  if (input.admission === "candidate_queue") {
    return { tone: "success", label: "已进入候选辅稿", detail: `本地复核分 ${input.totalScore}，等待人工确认` };
  }
  if (input.admission === "blocked") {
    return { tone: "warning", label: "暂不能进入候选辅稿", detail: input.reasons[0] || "存在未通过的关键检查" };
  }
  if (input.admission === "review_only") {
    return { tone: "neutral", label: "仅保留复盘", detail: `本地复核分 ${input.totalScore}，未达到 85 分` };
  }
  return { tone: "neutral", label: "仅作分析参考", detail: `本地复核分 ${input.totalScore}` };
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

export type ArchiveTranscriptAction = "use-existing" | "refresh";

export function selectArchiveTranscriptAction(
  forceRefresh: boolean,
  existingSubtitle: string,
): ArchiveTranscriptAction {
  return !forceRefresh && existingSubtitle.trim() ? "use-existing" : "refresh";
}

export function friendlyArchiveTranscriptError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error || "");
  if (/playlist file not found|系统找不到指定的文件|no such file/i.test(message)) {
    return "程序找不到这条录播的源文件，暂时无法重新识别。录播记录仍在，请检查缓存目录或重新导入原视频。";
  }
  return message;
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
  "成交片段",
  "疑似成交片段",
  "问价未见成交信号",
  "转品/上链接片段",
  "讲得散片段",
  "无法判断",
]);

const strongTransactionTypes = new Set<CandidateType>(["成交片段", "疑似成交片段"]);

const signalByType: Record<CandidateType, string> = {
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

export function normalizeBeginnerReview(input: any, fallbackText = ""): BeginnerReview {
  const fallbackSummary = String(fallbackText || "")
    .split(/\r?\n/)
    .map((line) => line.replace(/^#+\s*/, "").trim())
    .find(Boolean) || "暂时没有足够信息，请人工查看逐字稿。";
  const goodPoints = normalizeReviewList(input?.good_points ?? input?.goodPoints);
  const improvements = normalizeReviewList(input?.improvements);
  const checks = normalizeReviewList(input?.checks);
  return {
    verdict: String(input?.verdict || "建议人工判断").trim(),
    summary: String(input?.summary || fallbackSummary).trim(),
    goodPoints: goodPoints.length ? goodPoints : ["暂未识别到明确优点，可结合视频人工判断"],
    improvements: improvements.length ? improvements : ["先确认商品和用户需求，再给出明确下一步动作"],
    checks: checks.length ? checks : ["暂无，按逐字稿结论使用"],
  };
}

export function normalizeCandidate(input: CandidateInput, index: number): HighlightCandidate | null {
  const start = Number(input?.start);
  const end = Number(input?.end);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return null;
  if (!candidateTypes.has(input?.type as CandidateType)) return null;

  const type = input.type as CandidateType;
  const confidence: CandidateConfidence = ["高", "中", "低"].includes(String(input.confidence))
    ? input.confidence as CandidateConfidence
    : "低";
  const explicitTier = input.tier === "核心高光" || input.tier === "候选高光"
    ? input.tier
    : null;
  const tier = explicitTier || fallbackTier(type, confidence);
  const score = clampScore(input.score) ?? fallbackScore(tier, confidence);
  const safeStart = Math.max(0, start);
  const safeEnd = Math.max(0, end);

  return {
    id: String(input.id || `C${String(index + 1).padStart(2, "0")}-${Math.round(safeStart)}-${Math.round(safeEnd)}`),
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
    hook: String(input.hook || ""),
    takeaway: String(input.takeaway || ""),
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
