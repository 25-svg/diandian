import type {
  HighlightCandidate,
  SessionComparisonLike,
  SessionReviewSummary,
} from "./archiveAnalysis";
import type { ComplianceFinding } from "./complianceRules";
import type { SegmentDealSignals } from "./orderDealTimeline";
import type { TranscriptResultSignals } from "./transcriptSignals";

export type SessionDiagnosis = {
  headline: string;
  highlights: string[];
  issues: string[];
  confirmations: string[];
  nextActions: string[];
  stats: SessionReviewSummary;
};

type AuditSummary = {
  pendingCriticalCount: number;
} | null;

type BuildSessionDiagnosisInput = {
  candidates: readonly HighlightCandidate[];
  masterComparisons: Readonly<Record<string, SessionComparisonLike | undefined>>;
  auditBundle: AuditSummary;
  sessionReviewSummary: SessionReviewSummary;
  transcriptSignals?: Readonly<Record<string, TranscriptResultSignals | undefined>>;
  dealSignals?: Readonly<Record<string, SegmentDealSignals | undefined>>;
  complianceFindings?: readonly ComplianceFinding[];
  peakDealMinuteLabel?: string | null;
  paymentEventsLoaded?: boolean;
};

function uniqueLimited(values: readonly string[], limit = 3): string[] {
  return [...new Set(values.map((item) => item.trim()).filter(Boolean))].slice(0, limit);
}

function candidateName(candidate: HighlightCandidate): string {
  return candidate.scene || candidate.product || candidate.id;
}

export function buildSessionDiagnosis(input: BuildSessionDiagnosisInput): SessionDiagnosis {
  const {
    candidates,
    masterComparisons,
    auditBundle,
    sessionReviewSummary: stats,
    transcriptSignals = {},
    dealSignals = {},
    complianceFindings = [],
    peakDealMinuteLabel = null,
    paymentEventsLoaded = false,
  } = input;
  const scored = candidates
    .map((candidate) => ({
      candidate,
      comparison: masterComparisons[candidate.id],
    }))
    .filter((item): item is {
      candidate: HighlightCandidate;
      comparison: SessionComparisonLike;
    } => Boolean(item.comparison))
    .sort((left, right) =>
      (right.comparison.comparison.totalScore ?? -1) - (left.comparison.comparison.totalScore ?? -1)
    );

  let headline = "候选片段已整理，可先看重点，再决定是否逐段复盘";
  if (!candidates.length) {
    headline = "暂未发现有训练价值的片段，建议检查逐字稿后重新发现";
  } else if (stats.highScoreCount > 0) {
    headline = `发现 ${stats.highScoreCount} 段高价值候选，须人工审核后沉淀`;
  } else if (stats.reviewedCount > 0) {
    headline = `已复盘 ${stats.reviewedCount} 段，当前先补证据再筛选高价值话术`;
  }

  const highlights = uniqueLimited([
    ...scored
      .filter((item) => (item.comparison.comparison.totalScore ?? 0) >= 85)
      .map((item) =>
        `${candidateName(item.candidate)}：母稿对照 ${item.comparison.comparison.totalScore} 分`
      ),
    ...candidates
      .filter((candidate) => candidate.type === "高质量金句" && candidate.keySentence)
      .map((candidate) => `${candidateName(candidate)}：“${candidate.keySentence}”`),
    ...candidates
      .filter((candidate) => candidate.outcome === "confirmed_conversion")
      .map((candidate) => `${candidateName(candidate)}：逐字稿出现明确成交确认`),
    ...(peakDealMinuteLabel ? [`成交高峰：${peakDealMinuteLabel}`] : []),
    ...candidates
      .filter((candidate) => (dealSignals[candidate.id]?.afterWindowOrderCount || 0) > 0)
      .map((candidate) =>
        `${candidateName(candidate)}：段后 ${Math.round((dealSignals[candidate.id]?.afterWindowSec || 120) / 60)} 分钟有 ${dealSignals[candidate.id]?.afterWindowOrderCount} 单`
      ),
  ]);

  const issues = uniqueLimited([
    ...(stats.pendingCount > 0 ? [`还有 ${stats.pendingCount} 段未完成母稿对照`] : []),
    ...(stats.unmatchedCount > 0 ? [`有 ${stats.unmatchedCount} 段未匹配到母稿场景`] : []),
    ...(stats.riskCount > 0 ? [`有 ${stats.riskCount} 段存在事实或表达风险`] : []),
    ...(candidates.some((candidate) => candidate.interrupted)
      ? ["部分有效话术受到场内沟通打断，训练时应弱化无关插话"]
      : []),
    ...(paymentEventsLoaded && candidates.some((candidate) => {
      const transcript = transcriptSignals[candidate.id];
      const deal = dealSignals[candidate.id];
      const hasInquiry = (transcript?.inquiryCount || 0) > 0;
      const hasTextConversion = (transcript?.conversionConfirmationCount || 0) > 0;
      const hasOrderAfter = (deal?.afterWindowOrderCount || 0) > 0 || (deal?.inSegmentOrderCount || 0) > 0;
      return hasInquiry && !hasTextConversion && !hasOrderAfter;
    }) ? ["部分片段出现问价，但逐字稿和订单时间轴都未见成交"] : []),
    ...(paymentEventsLoaded && candidates.some((candidate) =>
      (transcriptSignals[candidate.id]?.inquiryCount || 0) > 0
      && (transcriptSignals[candidate.id]?.conversionConfirmationCount || 0) === 0
      && ((dealSignals[candidate.id]?.afterWindowOrderCount || 0) > 0
        || (dealSignals[candidate.id]?.inSegmentOrderCount || 0) > 0)
    ) ? ["部分片段逐字稿无成交确认，但订单显示段内/段后有成交，建议复听对齐"] : []),
    ...(!paymentEventsLoaded && candidates.some((candidate) =>
      (transcriptSignals[candidate.id]?.inquiryCount || 0) > 0
      && (transcriptSignals[candidate.id]?.conversionConfirmationCount || 0) === 0
    ) ? ["部分片段出现问价，但逐字稿窗口内没有成交确认"] : []),
  ]);

  const pendingCriticalCount = auditBundle?.pendingCriticalCount ?? 0;
  const confirmations = uniqueLimited([
    ...(pendingCriticalCount > 0 ? [`逐字稿仍有 ${pendingCriticalCount} 条关键内容待人工确认`] : []),
    ...candidates
      .filter((candidate) => candidate.verify && candidate.verify !== "无")
      .map((candidate) => `${candidateName(candidate)}：${candidate.verify}`),
    ...scored.flatMap((item) => item.comparison.comparison.risks || []),
    ...complianceFindings.map((finding) => `${finding.label}：${finding.quote}`),
  ]);

  const nextActions = uniqueLimited([
    ...(pendingCriticalCount > 0 ? ["先确认影响型号、价格、链接或成交判断的逐字稿"] : []),
    ...(stats.pendingCount > 0 ? ["后台完成剩余片段复盘，期间可继续查看当前片段"] : []),
    ...(stats.highScoreCount > 0 ? ["优先审核 85 分及以上候选，确认后再进入辅稿队列"] : []),
    ...(stats.riskCount > 0 ? ["逐条核对风险项，不把待确认信息写入企业母稿"] : []),
    ...(complianceFindings.length ? ["复听合规命中原话，确认后删除或改写风险表达"] : []),
    ...(!stats.pendingCount && !stats.highScoreCount ? ["选择最有训练价值的片段，补充证据后再次评分"] : []),
  ]);

  return {
    headline: headline.slice(0, 45),
    highlights,
    issues,
    confirmations,
    nextActions,
    stats,
  };
}
