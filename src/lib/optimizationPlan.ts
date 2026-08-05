import type { HighlightCandidate } from "./archiveAnalysis";
import type { MasterComparisonResult } from "./masterScript";
import type { SessionDiagnosis } from "./sessionDiagnosis";

export type OptimizationPlan = {
  sessionTitle: string;
  diagnosedAt: string;
  masterScriptVersion: string;
  summary: SessionDiagnosis;
  masterSectionUpdates: Array<{
    sectionTitle: string;
    reason: string;
    candidateSegmentId?: string;
    score?: number;
  }>;
  practiceSegments: Array<{
    id: string;
    label: string;
    scene: string;
    discussionPoints: string[];
  }>;
  factConfirmations: string[];
};

type ReviewLike = {
  trainingChecklist?: string;
};

type BuildOptimizationPlanInput = {
  sessionTitle: string;
  diagnosedAt: string;
  masterScriptVersion: string;
  diagnosis: SessionDiagnosis;
  candidates: readonly HighlightCandidate[];
  comparisons: Readonly<Record<string, MasterComparisonResult | undefined>>;
  reviews: Readonly<Record<string, ReviewLike | undefined>>;
};

function lines(value: string | undefined): string[] {
  return (value || "")
    .split(/\r?\n/)
    .map((item) => item.replace(/^[\s\-*•\d.、]+/, "").trim())
    .filter(Boolean)
    .slice(0, 3);
}

export function buildOptimizationPlan(input: BuildOptimizationPlanInput): OptimizationPlan {
  const candidateById = new Map(input.candidates.map((candidate) => [candidate.id, candidate]));
  const ranked = Object.entries(input.comparisons)
    .filter((entry): entry is [string, MasterComparisonResult] => Boolean(entry[1]))
    .sort((left, right) =>
      (right[1].comparison.totalScore ?? -1) - (left[1].comparison.totalScore ?? -1)
    );

  const masterSectionUpdates = ranked
    .filter(([, result]) => (result.comparison.totalScore ?? 0) >= 85)
    .slice(0, 5)
    .map(([candidateId, result]) => ({
      sectionTitle: result.comparison.suggestedInsertionPoint || "待人工指定母稿章节",
      reason: result.comparison.improvements[0]
        || result.comparison.verdict
        || "候选片段达到复用门槛，须人工审核原话和事实",
      candidateSegmentId: candidateId,
      score: result.comparison.totalScore ?? undefined,
    }));

  const practiceSegments = ranked
    .filter(([, result]) => (result.comparison.totalScore ?? 0) >= 70)
    .slice(0, 3)
    .map(([candidateId]) => {
      const candidate = candidateById.get(candidateId);
      return {
        id: candidateId,
        label: candidate?.type || "训练片段",
        scene: candidate?.scene || candidate?.product || "场景待确认",
        discussionPoints: lines(input.reviews[candidateId]?.trainingChecklist).length
          ? lines(input.reviews[candidateId]?.trainingChecklist)
          : ["核对客户需求是否明确", "讨论这段话术如何更自然地推动下一步"],
      };
    });

  return {
    sessionTitle: input.sessionTitle,
    diagnosedAt: input.diagnosedAt,
    masterScriptVersion: input.masterScriptVersion,
    summary: input.diagnosis,
    masterSectionUpdates,
    practiceSegments,
    factConfirmations: input.diagnosis.confirmations,
  };
}

function list(items: readonly string[], emptyText: string): string {
  return items.length ? items.map((item) => `- ${item}`).join("\n") : `- ${emptyText}`;
}

export function formatOptimizationPlanMarkdown(plan: OptimizationPlan): string {
  const updates = plan.masterSectionUpdates.length
    ? plan.masterSectionUpdates.map((item) =>
      `- ${item.candidateSegmentId || "候选片段"}｜${item.score ?? "-"} 分｜${item.sectionTitle}\n  - 原因：${item.reason}`
    ).join("\n")
    : "- 暂无达到 85 分且可提交人工审核的候选";
  const practice = plan.practiceSegments.length
    ? plan.practiceSegments.map((item) =>
      `### ${item.id}｜${item.label}｜${item.scene}\n${list(item.discussionPoints, "待培训负责人补充")}`
    ).join("\n\n")
    : "暂无建议练习片段。";

  return [
    `# 下场优化计划｜${plan.sessionTitle}`,
    "",
    "> 本文档由系统根据本场证据生成，仅供复盘和培训讨论；不是企业标准稿，所有事实与母稿变更须人工审定。",
    "",
    `- 诊断时间：${plan.diagnosedAt}`,
    `- 企业母稿版本：${plan.masterScriptVersion || "尚未启用"}`,
    `- 已完成复盘：${plan.summary.stats.reviewedCount}/${plan.summary.stats.totalCandidates}`,
    "",
    "## 一句话结论",
    plan.summary.headline,
    "",
    "## 本场亮点",
    list(plan.summary.highlights, "暂无已确认亮点"),
    "",
    "## 主要问题",
    list(plan.summary.issues, "暂无已确认问题"),
    "",
    "## 下场三件事",
    list(plan.summary.nextActions, "先补齐证据，再继续评分"),
    "",
    "## 待确认事实",
    list(plan.factConfirmations, "暂无"),
    "",
    "## 建议提交母稿审核",
    updates,
    "",
    "## 建议练习片段",
    practice,
    "",
  ].join("\n");
}

export function optimizationPlanFileName(sessionTitle: string): string {
  const safe = sessionTitle.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").trim() || "直播场次";
  return `${safe}-下场优化计划.md`;
}
