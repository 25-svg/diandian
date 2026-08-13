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

export type PublishedMaster = { id: number; scriptKey: string; version: string; title: string };
export type MasterSection = {
  id: number;
  masterScriptId: number;
  sectionKey: string;
  position: number;
  sectionKind: MasterSectionKind;
  productCardId: string | null;
  title: string;
  masterText: string;
};
export type MasterBaseline = { master: PublishedMaster; sections: MasterSection[] };

export type TrainingGuide = {
  title: string;
  version: string;
  generatedOn: string;
  html: string;
  rtf: string;
};

const TRAINING_FACT_NOTE = "价格、库存、优惠、链接、成色等以当场事实为准；未确认时不要承诺。";

function trainingPurpose(kind: MasterSectionKind): string {
  return ({
    opening: "建立信任并说明本场能为顾客解决什么问题。",
    product: "先了解顾客需求，再给出对应产品和价值说明。",
    transition: "自然承接上一个话题，带顾客进入下一步。",
    scenario: "处理顾客疑问、异议或购买犹豫。",
    closing: "明确下一步动作，并完成下单后的服务承接。",
  } satisfies Record<MasterSectionKind, string>)[kind];
}

function trainingPractice(kind: MasterSectionKind): string {
  return ({
    opening: "先用自己的话复述这一段，再练习在 30 秒内说清本场价值。",
    product: "两人模拟顾客提问，练习先问需求、再给推荐，不直接报型号。",
    transition: "练习用上一句自然带出下一件商品或下一步动作。",
    scenario: "两人模拟顾客犹豫，练习先回应顾虑，再给出明确选择。",
    closing: "练习说完后停顿，等待顾客确认，不催促或虚构成交。",
  } satisfies Record<MasterSectionKind, string>)[kind];
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\"/g, "&quot;").replace(/'/g, "&#39;");
}

function escapeRtf(value: string): string {
  let result = "";
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    const character = value[index];
    if (character === "\\") result += "\\\\";
    else if (character === "{") result += "\\{";
    else if (character === "}") result += "\\}";
    else if (character === "\n") result += "\\line ";
    else if (code <= 127) result += character;
    else result += `\\u${code > 32767 ? code - 65536 : code}?`;
  }
  return result;
}

export function buildTrainingGuide(baseline: MasterBaseline, generatedOn: string): TrainingGuide {
  const title = baseline.master.title || "企业标准话术";
  const sections = [...baseline.sections].sort((left, right) => left.position - right.position);
  const htmlSections = sections.map((section) => `<section class="script-section">
    <h2>${section.position}. ${escapeHtml(section.title || section.sectionKey)}</h2>
    <div><h3>这一段要做什么</h3><p>${escapeHtml(trainingPurpose(section.sectionKind))}</p></div>
    <div class="speech"><h3>标准话术</h3><p>${escapeHtml(section.masterText).replace(/\n/g, "<br>")}</p></div>
    <div><h3>现场要确认</h3><p>${TRAINING_FACT_NOTE}</p></div>
    <div><h3>练习方法</h3><p>${escapeHtml(trainingPractice(section.sectionKind))}</p></div>
  </section>`).join("\n");
  const rtfSections = sections.map((section) => [
    `${section.position}. ${section.title || section.sectionKey}`,
    `这一段要做什么：${trainingPurpose(section.sectionKind)}`,
    `标准话术：${section.masterText}`,
    `现场要确认：${TRAINING_FACT_NOTE}`,
    `练习方法：${trainingPractice(section.sectionKind)}`,
  ].map(escapeRtf).join("\\par ")).join("\\par \\par ");
  const html = `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>${escapeHtml(title)} 培训版</title><style>body{max-width:860px;margin:0 auto;padding:40px;font-family:"Microsoft YaHei",Arial,sans-serif;color:#1f2937;line-height:1.7}h1{margin-bottom:4px}h2{font-size:19px;border-bottom:1px solid #dbe3ee;padding-bottom:8px}.meta{color:#667085}.script-section{margin-top:30px;break-inside:avoid}.script-section div{margin-top:12px;padding:12px 14px;background:#f8fafc;border-radius:6px}.script-section h3{margin:0 0 4px;font-size:14px}.script-section p{margin:0}.speech{border-left:3px solid #1677ff;background:#f0f7ff!important}@media print{body{padding:18mm}.script-section{page-break-inside:avoid}}</style></head><body><h1>${escapeHtml(title)} 培训版</h1><p class="meta">企业标准话术 V${escapeHtml(baseline.master.version)} · 生成日期 ${escapeHtml(generatedOn)}</p><section class="script-section"><h2>使用原则</h2><div><h3>只学习已发布企业标准</h3><p>本资料不包含候选话术、局部素材或待审核内容。实际直播中，价格、库存、优惠、链接、成色等以当场事实为准；未确认时不要承诺。</p></div></section>${htmlSections}</body></html>`;
  const rtf = `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Microsoft YaHei;}}\\viewkind4\\uc1\\pard\\sa180\\f0\\fs32\\b ${escapeRtf(title)} 培训版\\b0\\par\\fs22 企业标准话术 V${escapeRtf(baseline.master.version)} · 生成日期 ${escapeRtf(generatedOn)}\\par\\par\\b 使用原则\\b0\\par 本资料不包含候选话术、局部素材或待审核内容。${escapeRtf(TRAINING_FACT_NOTE)}\\par\\par ${rtfSections}\\par}`;
  return { title, version: baseline.master.version, generatedOn, html, rtf };
}

export function trainingGuideFileName(guide: Pick<TrainingGuide, "title" | "version">, format: "rtf" | "html"): string {
  const safeTitle = guide.title.replace(/[\\/:*?"<>|]/g, "_").trim() || "企业标准话术";
  return `${safeTitle}_V${guide.version}_培训版.${format}`;
}

export type FriendlyMasterError = { title: string; detail: string; nextAction: string };

export function friendlyMasterError(reason: unknown): FriendlyMasterError {
  const raw = typeof reason === "string"
    ? reason
    : reason && typeof reason === "object" && "message" in reason
      ? String((reason as { message?: unknown }).message || "")
      : String(reason || "");
  const normalized = raw.toLowerCase();
  if (normalized.includes("os error 2") || normalized.includes("file not found") || raw.includes("系统找不到指定的文件")) {
    return {
      title: "找不到原视频文件",
      detail: "这场录播的原文件已移动或删除，系统没有改动已有逐字稿和母稿。",
      nextAction: "请重新导入原视频后，再继续处理该场。",
    };
  }
  if (raw.includes("母稿已更新") || normalized.includes("stale master")) {
    return {
      title: "企业标准已更新",
      detail: "这条结果基于旧版企业标准，不能直接加入新版。",
      nextAction: "回到对应录播，按当前企业标准重新分析该片段。",
    };
  }
  if (raw.includes("格式错误") || normalized.includes("expected `") || normalized.includes("invalid type") || normalized.includes("unknown variant")) {
    return {
      title: "本次整理结果没有生成成功",
      detail: "模型返回内容格式异常，已有逐字稿和已发布企业标准均保持不变。",
      nextAction: "点击“重新生成”即可，不需要重新转写视频。",
    };
  }
  if (raw.includes("正在后台处理") || normalized.includes("already bound") || normalized.includes("already processing")) {
    return {
      title: "任务正在处理中",
      detail: "系统正在继续处理已有任务，关闭页面不会删除已完成内容。",
      nextAction: "请稍后刷新进度；如状态停止，再点击“继续未完成场次”。",
    };
  }
  if (raw.includes("主播名") || raw.includes("样本来源") || raw.includes("主播知识库")) {
    return {
      title: "缺少主播名",
      detail: raw.includes("请先") ? raw : "发布到主播知识库前必须填写主播名。",
      nextAction: "在批次中填写样本来源/主播名（如：于千惠）后重试发布。",
    };
  }
  return {
    title: "这一步暂时没有完成",
    detail: "系统没有删除视频、逐字稿或已发布企业标准。",
    nextAction: "请重试当前操作；若仍失败，请保留页面提示后联系管理员。",
  };
}
export type MasterSampleBatchStatus =
  | "collecting"
  | "processing"
  | "ready_for_synthesis"
  | "draft_ready"
  | "published"
  | "failed";
export type MasterSampleBatchPurpose = "sample" | "enterprise";
export type MasterSampleBatch = {
  id: number;
  batchKey: string;
  title: string;
  hostLabel: string;
  purpose: MasterSampleBatchPurpose;
  targetSampleCount: number;
  status: MasterSampleBatchStatus;
  createdAt: string;
  updatedAt: string;
};
export type MasterSampleBatchSummary = {
  batch: MasterSampleBatch;
  sampleCount: number;
  readyCount: number;
  failedCount: number;
};
export type MasterSampleBatchItemStatus = "pending" | "transcribing" | "reviewing" | "ready" | "failed";
export type MasterSampleBatchItem = {
  id: number;
  batchId: number;
  videoId: number;
  sourceId: number | null;
  processingStatus: MasterSampleBatchItemStatus;
  error: string | null;
  displayOrder: number;
  createdAt: string;
  updatedAt: string;
  videoTitle: string;
  videoLength: number;
};
export type MasterSampleBatchDetail = {
  batch: MasterSampleBatch;
  items: MasterSampleBatchItem[];
  synthesis: MasterSampleBatchSynthesis | null;
  correctableModelFormatIssueCount: number | null;
};
export type MasterSampleBatchSynthesisStatus = "generating" | "ready" | "failed" | "published";
export type MasterSampleBatchSynthesis = {
  batchId: number;
  status: MasterSampleBatchSynthesisStatus;
  draftJson: string | null;
  error: string | null;
  createdAt: string;
  updatedAt: string;
  generatedAt: string | null;
  publishedAt: string | null;
};

export function masterBatchPurposeLabel(purpose: MasterSampleBatchPurpose): string {
  return purpose === "enterprise" ? "公司统一话术" : "主播话术";
}

export function canActivateEnterpriseMaster(purpose: MasterSampleBatchPurpose): boolean {
  return purpose === "enterprise";
}

export function hostScriptAutomationAction(
  status: MasterSampleBatchStatus,
): "process" | "generate" | "review" | "ready" | "wait" {
  if (status === "collecting" || status === "failed") return "process";
  if (status === "ready_for_synthesis") return "generate";
  if (status === "draft_ready") return "review";
  if (status === "published") return "ready";
  return "wait";
}

export function enterpriseMasterResumeAction(
  purpose: MasterSampleBatchPurpose,
  status: MasterSampleBatchStatus,
  synthesisStatus: MasterSampleBatchSynthesisStatus | null,
): "generate" | "review" | "ready" | "wait" | "none" {
  if (purpose !== "enterprise") return "none";
  if (status === "draft_ready") return "review";
  if (status === "published") return "ready";
  if (status === "ready_for_synthesis" && (synthesisStatus === null || synthesisStatus === "failed")) {
    return "generate";
  }
  if (status === "processing" || synthesisStatus === "generating") return "wait";
  return "none";
}

export function readyVideoIdsForEnterpriseMaster(
  batches: Array<{
    batch: Pick<MasterSampleBatch, "purpose">;
    items: Array<Pick<MasterSampleBatchItem, "videoId" | "processingStatus">>;
  }>,
): number[] {
  return Array.from(new Set(
    batches.flatMap((detail) => detail.batch.purpose === "sample"
      ? detail.items
        .filter((item) => item.processingStatus === "ready")
        .map((item) => item.videoId)
      : []),
  ));
}

export function shouldAutoGenerateEnterpriseDraft(
  purpose: MasterSampleBatchPurpose,
  status: MasterSampleBatchStatus,
  synthesisStatus: MasterSampleBatchSynthesisStatus | null,
): boolean {
  return purpose === "enterprise"
    && status === "ready_for_synthesis"
    && synthesisStatus === null;
}
export type BatchMasterEvidence = {
  evidenceId: string;
  videoId: number;
  videoTitle: string;
  startMs: number;
  endMs: number;
  quote: string;
};
export type BatchMasterDraft = {
  title: string;
  evidence: BatchMasterEvidence[];
  patterns: Array<{ name: string; observation: string; evidenceIds: string[] }>;
  sections: Array<{
    title: string;
    purpose: string;
    evidenceIds: string[];
    fixedSpeech: string;
    conditions: string[];
    dynamicFields: string[];
  }>;
  operatingRules: string[];
};

export type SegmentDecision =
  | "support_candidate"
  | "golden_sentence"
  | "training_material"
  | "reference_only"
  | "blocked";
export type EvidenceGrade = "A" | "B" | "C";
export type RiskStatus = "passed" | "needs_review" | "blocked";
export type UpgradeComparisonDecision =
  | "add_as_support"
  | "add_as_golden_sentence"
  | "replace_existing"
  | "merge_with_existing"
  | "duplicate"
  | "reject";
export type MasterUpgradeReview = {
  comparisonDecision: UpgradeComparisonDecision;
  matchedMasterSection: string;
  sameScene: boolean;
  newValue: string;
  whyBetter: string[];
  duplicateContent: string[];
  riskOrUncertainty: string[];
  originalHostWords: string;
  trainingSuggestion: string;
  recommendedAction: string;
};
export type SegmentScoreDetail = {
  score: number;
  maxScore: number;
  reason: string;
  evidence: string[];
};
export type SegmentQualityReview = {
  segmentId: string;
  segmentType: string;
  totalScore: number;
  decision: SegmentDecision;
  evidenceGrade: EvidenceGrade;
  riskStatus: RiskStatus;
  scoreDetails: {
    sceneGoal: SegmentScoreDetail;
    persuasiveness: SegmentScoreDetail;
    masterIncrement: SegmentScoreDetail;
    reusability: SegmentScoreDetail;
    factualAccuracy: SegmentScoreDetail;
    naturalExpression: SegmentScoreDetail;
  };
  whatIsGood: string[];
  whatNeedsImprovement: string[];
  factsToConfirm: string[];
  reusableOriginalSentence: string;
  suggestedTrainingVersion: string;
  recommendedMasterSection: string;
};

export type PresentedSegmentScoreDetail = {
  label: string;
  detail: SegmentScoreDetail;
};

export type MasterComparisonResult = {
  comparison: {
    masterScriptId: number;
    masterVersion: string;
    masterSectionId: number | null;
    isMasterSource: boolean;
    score: {
      sceneGoal: number;
      persuasiveness: number;
      masterIncrement: number;
      reusability: number;
      factualAccuracy: number;
      naturalExpression: number;
      total: number;
    } | null;
    totalScore: number | null;
    admission: CandidateAdmission | null;
    gates: { reasons: string[] } | null;
    verdict: string;
    moduleTags: string[];
    improvements: string[];
    risks: string[];
    suggestedInsertionPoint: string | null;
    qualityReview?: SegmentQualityReview | null;
  };
  candidate: { id: number; status: string } | null;
  upgradeReviewId: number | null;
  upgradeReview: MasterUpgradeReview | null;
  upgradeReviewError: string | null;
};
export type SupportCandidate = {
  id: number;
  candidateKey: string;
  masterScriptId: number;
  masterSectionId: number;
  sourceKey: string;
  sourceStartMs: number;
  sourceEndMs: number;
  hostText: string;
  comparisonJson: string;
  totalScore: number;
  admission: CandidateAdmission;
  status: SupportCandidateStatus;
  createdAt: string;
};

export type TransactionMasterView = {
  rules: Array<{ title: string; detail: string; evidenceIds: string[] }>;
  completeExamples: Array<{
    id: number;
    text: string;
    score: number;
    sourceKey: string;
    startMs: number;
    endMs: number;
  }>;
  trainingMaterials: Array<{
    id: number;
    text: string;
    sourceKey: string;
    startMs: number;
    endMs: number;
  }>;
};

export function buildTransactionMasterView(input: {
  draft: BatchMasterDraft;
  candidates: SupportCandidate[];
}): TransactionMasterView {
  const patternRules = input.draft.patterns.map((pattern) => ({
    title: pattern.name,
    detail: pattern.observation,
    evidenceIds: pattern.evidenceIds,
  }));
  const operatingRules = input.draft.operatingRules.map((rule, index) => ({
    title: `执行规则 ${index + 1}`,
    detail: rule,
    evidenceIds: [],
  }));
  const toSource = (candidate: SupportCandidate) => ({
    id: candidate.id,
    text: candidate.hostText,
    sourceKey: candidate.sourceKey,
    startMs: candidate.sourceStartMs,
    endMs: candidate.sourceEndMs,
  });
  return {
    rules: [...patternRules, ...operatingRules],
    completeExamples: input.candidates
      .filter((candidate) => ["approved", "merged"].includes(candidate.status) && candidate.totalScore >= 85)
      .map((candidate) => ({ ...toSource(candidate), score: candidate.totalScore })),
    trainingMaterials: input.candidates
      .filter((candidate) => candidate.status === "held")
      .map(toSource),
  };
}

export type SupportCandidatePresentation = {
  scenario: string;
  reasons: string[];
  checks: string[];
  insertion: string;
};

export function supportCandidateUpgradeReview(
  candidate: Pick<SupportCandidate, "comparisonJson">,
): MasterUpgradeReview | null {
  try {
    const parsed = JSON.parse(candidate.comparisonJson) as {
      upgradeReview?: MasterUpgradeReview;
    };
    const review = parsed.upgradeReview;
    if (!review || ![
      "add_as_support",
      "add_as_golden_sentence",
      "replace_existing",
      "merge_with_existing",
      "duplicate",
      "reject",
    ].includes(review.comparisonDecision)) return null;
    return review;
  } catch {
    return null;
  }
}

export function supportCandidateCanUpgradeMaster(
  candidate: Pick<SupportCandidate, "comparisonJson">,
): boolean {
  const review = supportCandidateUpgradeReview(candidate);
  return review !== null && upgradeDecisionIsActionable(review.comparisonDecision);
}

function comparisonList(value: unknown): string[] {
  if (Array.isArray(value)) return value.filter((item): item is string => typeof item === "string" && Boolean(item.trim())).map((item) => item.trim());
  return typeof value === "string" && value.trim() ? [value.trim()] : [];
}

function removeModulePrefix(value: string): string {
  return value.replace(/^[^：:]{1,12}[：:]\s*/, "").trim();
}

export function supportCandidatePresentation(
  candidate: Pick<SupportCandidate, "comparisonJson">,
): SupportCandidatePresentation {
  try {
    const parsed = JSON.parse(candidate.comparisonJson) as Record<string, unknown>;
    const comparison = parsed.comparison && typeof parsed.comparison === "object"
      ? parsed.comparison as Record<string, unknown>
      : parsed;
    const modules = comparisonList(comparison.moduleTags);
    const reasons = comparisonList(comparison.improvements).map(removeModulePrefix);
    const checks = comparisonList(comparison.risks);
    const insertion = typeof comparison.suggestedInsertionPoint === "string"
      ? comparison.suggestedInsertionPoint.trim()
      : "";
    return {
      scenario: modules.length ? modules.join("、") : "完整成交场景",
      reasons: reasons.length ? reasons : ["保留主播原话，作为完整成交过程的可复用示范。"],
      checks: checks.length ? checks : ["未发现需要额外确认的事项。"],
      insertion: insertion || "放入位置由审核人确认；使用时保留主播原话。",
    };
  } catch {
    return {
      scenario: "完整成交场景",
      reasons: ["保留主播原话，作为完整成交过程的可复用示范。"],
      checks: ["未发现需要额外确认的事项。"],
      insertion: "放入位置由审核人确认；使用时保留主播原话。",
    };
  }
}

export function supportCandidateVersionPresentation(
  candidateMasterScriptId: number,
  activeMasterScriptId: number,
): { current: boolean; label: string; detail: string } {
  if (candidateMasterScriptId === activeMasterScriptId) {
    return {
      current: true,
      label: "当前企业标准",
      detail: "这条话术按当前企业标准评分，可以继续人工审核。",
    };
  }
  return {
    current: false,
    label: "需要按新版复核",
    detail: "这条话术按旧版企业标准评分，不能直接加入新版企业话术。",
  };
}
export type MasterUpgradePreview = {
  baseMasterId: number;
  scriptKey: string;
  currentVersion: string;
  nextVersion: string;
  sections: Array<{ sectionId: number; sectionKey: string; currentText: string; nextText: string; candidateIds: number[] }>;
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
    return { tone: "success", label: "已进入候选辅稿", detail: `母稿匹配与可复用分 ${total}，等待人工审核` };
  }
  if (admission === "blocked") {
    return { tone: "warning", label: "暂不能进入候选辅稿", detail: reasons[0] || "存在未通过的关键检查" };
  }
  if (admission === "review_only") {
    return { tone: "neutral", label: "仅保留复盘", detail: `母稿匹配与可复用分 ${total}，未达到 85 分` };
  }
  return { tone: "neutral", label: "仅作分析参考", detail: `母稿匹配与可复用分 ${total}` };
}

export function segmentDecisionLabel(decision: SegmentDecision): string {
  return {
    support_candidate: "候选辅稿",
    golden_sentence: "金句话术",
    training_material: "训练素材",
    reference_only: "仅作复盘参考",
    blocked: "禁止使用",
  }[decision];
}

export function presentSegmentScoreDetails(
  scoreDetails:
    | Partial<Record<keyof SegmentQualityReview["scoreDetails"], SegmentScoreDetail | undefined>>
    | null
    | undefined,
): PresentedSegmentScoreDetail[] {
  if (!scoreDetails) return [];
  const definitions: Array<[
    keyof SegmentQualityReview["scoreDetails"],
    string,
  ]> = [
    ["sceneGoal", "场景目标完成度"],
    ["persuasiveness", "话术说服力"],
    ["masterIncrement", "比企业母稿新增的价值"],
    ["reusability", "新人参考价值"],
    ["factualAccuracy", "事实准确性"],
    ["naturalExpression", "表达简洁自然度"],
  ];
  return definitions.flatMap(([key, label]) => {
    const detail = scoreDetails[key];
    if (
      !detail
      || !Number.isFinite(detail.score)
      || !Number.isFinite(detail.maxScore)
      || typeof detail.reason !== "string"
      || !Array.isArray(detail.evidence)
    ) {
      return [];
    }
    return [{ label, detail }];
  });
}

export function evidenceGradeLabel(grade: EvidenceGrade): string {
  return {
    A: "A级 · 证据充分",
    B: "B级 · 少量事实待确认",
    C: "C级 · 仅作观察素材",
  }[grade];
}

export function riskStatusLabel(status: RiskStatus): string {
  return {
    passed: "无关键风险",
    needs_review: "需要人工确认",
    blocked: "存在阻断风险",
  }[status];
}

export function upgradeDecisionLabel(decision: UpgradeComparisonDecision): string {
  return ({
    add_as_support: "建议新增为辅稿",
    add_as_golden_sentence: "建议加入金句话术库",
    replace_existing: "建议人工审核后替换",
    merge_with_existing: "建议与现有章节合并",
    duplicate: "与企业母稿重复",
    reject: "不建议采用",
  } satisfies Record<UpgradeComparisonDecision, string>)[decision];
}

export function upgradeDecisionNextAction(decision: UpgradeComparisonDecision): string {
  return ({
    add_as_support: "进入候选辅稿，等待人工审核。",
    add_as_golden_sentence: "进入金句话术分组，等待人工审核。",
    replace_existing: "提交人工审核；确认差异后才能替换企业母稿。",
    merge_with_existing: "提交人工审核；确认互补内容后才能合并企业母稿。",
    duplicate: "保留比较记录，不进入母稿升级队列。",
    reject: "保留复盘记录，不进入母稿升级队列。",
  } satisfies Record<UpgradeComparisonDecision, string>)[decision];
}

export function upgradeDecisionIsActionable(decision: UpgradeComparisonDecision): boolean {
  return !["duplicate", "reject"].includes(decision);
}

export type CandidateDecisionLabel = "加入下一版企业话术" | "保留为局部素材" | "不采用";
export type SupportCandidateStatus = "pending_review" | "approved" | "held" | "returned" | "rejected" | "merged";

export function candidateDecisionStatus(label: CandidateDecisionLabel): SupportCandidateStatus {
  return ({
    "加入下一版企业话术": "approved",
    "保留为局部素材": "held",
    "不采用": "rejected",
  } satisfies Record<CandidateDecisionLabel, SupportCandidateStatus>)[label];
}

export function upgradePublishGate(confirmed: boolean, approvedCount: number): { allowed: boolean; reason: string } {
  if (!confirmed) return { allowed: false, reason: "请先确认母稿差异" };
  if (approvedCount < 1) return { allowed: false, reason: "至少选择一条已通过的候选辅稿" };
  return { allowed: true, reason: "可以发布母稿新版本" };
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
  return invokeCommand<PublishedMaster>("publish_master_script", { request: { sourceId, scriptKey, draft } });
}

export function getMasterBaseline(scriptKey: string) {
  return invokeCommand<MasterBaseline>("get_master_baseline", { scriptKey });
}

export function createMasterSampleBatch(input: {
  title: string;
  hostLabel?: string;
  purpose?: MasterSampleBatchPurpose;
  targetSampleCount: number;
  videoIds: number[];
}) {
  return invokeCommand<MasterSampleBatchDetail>("create_master_sample_batch", { request: input });
}

export function listMasterSampleBatches() {
  return invokeCommand<MasterSampleBatchSummary[]>("list_master_sample_batches", {});
}

export function getMasterSampleBatch(batchId: number) {
  return invokeCommand<MasterSampleBatchDetail>("get_master_sample_batch", { batchId });
}

export function startMasterSampleBatchProcessing(batchId: number) {
  return invokeCommand<MasterSampleBatchDetail>("start_master_sample_batch_processing", { batchId });
}

export function generateMasterSampleBatchDraft(batchId: number) {
  return invokeCommand<MasterSampleBatchDetail>("generate_master_sample_batch_draft", { batchId });
}

export function publishMasterSampleBatchDraft(batchId: number) {
  return invokeCommand<PublishedMaster>("publish_master_sample_batch_draft", { request: { batchId } });
}

export function publishCorrectedMasterSampleBatchVersion(batchId: number) {
  return invokeCommand<PublishedMaster>("publish_corrected_master_sample_batch_version", { request: { batchId } });
}

export function listUnbatchedMasterSourceVideoIds() {
  return invokeCommand<number[]>("list_unbatched_master_source_video_ids", {});
}

export function compareHighlightToMaster(request: {
  scriptKey: string;
  expectedMasterScriptId: number;
  masterSectionId?: number | null;
  source: { kind: "video"; videoId: number } | { kind: "archive"; platform: string; roomId: string; liveId: string };
  sourceStartMs: number;
  sourceEndMs: number;
  candidateType: string;
  chainStages: string[];
  candidateSegment: {
    segmentId: string;
    segmentType: string;
    scene: string;
    customerNeed: string;
    originalText: string;
    keySentence: string;
    outcome: string;
    interrupted: boolean;
    whySelected: string;
  };
  productCardId: string | null;
  sectionKind: MasterSectionKind;
  /** Competitor recordings are reference-only and never enter enterprise upgrade queues. */
  comparisonMode?: "enterprise_upgrade" | "competitor_benchmark";
  competitorName?: string | null;
}) {
  return invokeCommand<MasterComparisonResult>("compare_highlight_to_master", { request });
}

export function retryMasterUpgradeReview(reviewId: number) {
  return invokeCommand<{
    reviewId: number;
    upgradeReview: MasterUpgradeReview;
    candidate: SupportCandidate | null;
  }>("retry_master_upgrade_review", { reviewId });
}

export function listSupportCandidates(status?: SupportCandidateStatus) {
  return invokeCommand<SupportCandidate[]>("list_support_candidates", { status: status || null });
}

export function decideSupportCandidate(id: number, nextStatus: SupportCandidateStatus) {
  return invokeCommand<SupportCandidate>("decide_support_candidate", { id, nextStatus });
}

export type CompetitorReferenceStatus = "pending" | "approved_reference" | "rejected" | "merged_to_case_study";
export type CompetitorReferenceCandidate = {
  id: number; competitorName: string; sourceKey: string; masterSectionId: number;
  sourceStartMs: number; sourceEndMs: number; hostText: string; comparisonJson: string;
  migrationDecision: "migratable_structure" | "better_phrasing" | "reference_only" | "not_applicable";
  totalScore: number; status: CompetitorReferenceStatus; createdAt: string;
};

export function listCompetitorReferenceCandidates(status?: CompetitorReferenceStatus) {
  return invokeCommand<CompetitorReferenceCandidate[]>("list_competitor_reference_candidates", { status: status || null });
}

export function decideCompetitorReferenceCandidate(id: number, nextStatus: CompetitorReferenceStatus) {
  return invokeCommand<CompetitorReferenceCandidate>("decide_competitor_reference_candidate", { id, nextStatus });
}

export function publishCompetitorReference(candidateId: number, title = "") {
  return invokeCommand<string>("publish_competitor_reference", { request: { candidateId, title } });
}

export function previewMasterUpgrade(scriptKey: string, candidateIds: number[]) {
  return invokeCommand<MasterUpgradePreview>("preview_master_upgrade", { request: { scriptKey, candidateIds } });
}

export function publishMasterUpgrade(scriptKey: string, candidateIds: number[], diffConfirmed: boolean) {
  return invokeCommand<PublishedMaster>("publish_master_upgrade", { request: { scriptKey, candidateIds, diffConfirmed } });
}

async function invokeCommand<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("./invoker.js");
  return invoke<T>(command, args);
}
