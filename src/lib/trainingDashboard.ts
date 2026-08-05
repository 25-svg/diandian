import {
  summarizeSessionReview,
  type HighlightCandidate,
  type SessionComparisonLike,
} from "./archiveAnalysis.js";
import type { SessionDiagnosis } from "./sessionDiagnosis";

export type StoredSessionAnalysis = {
  sourceKey: string;
  sourceTitle?: string;
  updatedAt: string;
  candidates: HighlightCandidate[];
  reviews: Record<string, unknown>;
  masterComparisons?: Record<string, SessionComparisonLike>;
  diagnosis?: SessionDiagnosis;
};

export type TrainingDashboardRow = {
  sourceKey: string;
  title: string;
  updatedAt: string;
  headline: string;
  averageScore: number | null;
  pendingCount: number;
  confirmationCount: number;
  highScoreCount: number;
};

export function buildTrainingDashboardRows(
  records: readonly StoredSessionAnalysis[],
): TrainingDashboardRow[] {
  return records
    .map((record) => {
      const summary = record.diagnosis?.stats || summarizeSessionReview(
        record.candidates || [],
        record.masterComparisons || {},
        record.reviews || {},
      );
      return {
        sourceKey: record.sourceKey,
        title: record.sourceTitle || record.sourceKey,
        updatedAt: record.updatedAt,
        headline: record.diagnosis?.headline || (
          summary.reviewedCount
            ? `已复盘 ${summary.reviewedCount} 段，待复盘 ${summary.pendingCount} 段`
            : "已发现片段，尚未开始整场复盘"
        ),
        averageScore: summary.averageScore,
        pendingCount: summary.pendingCount,
        confirmationCount: record.diagnosis?.confirmations.length || 0,
        highScoreCount: summary.highScoreCount,
      };
    })
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
}
