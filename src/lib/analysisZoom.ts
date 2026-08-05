export const ANALYSIS_ZOOM_MIN = 80;
export const ANALYSIS_ZOOM_MAX = 150;
export const ANALYSIS_ZOOM_STEP = 10;
export const ANALYSIS_ZOOM_DEFAULT = 100;

export function normalizeAnalysisZoom(value: unknown): number {
  if (value === null || value === undefined || value === "") return ANALYSIS_ZOOM_DEFAULT;
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(parsed)) return ANALYSIS_ZOOM_DEFAULT;
  const rounded = Math.round(parsed / ANALYSIS_ZOOM_STEP) * ANALYSIS_ZOOM_STEP;
  return Math.min(ANALYSIS_ZOOM_MAX, Math.max(ANALYSIS_ZOOM_MIN, rounded));
}

export function stepAnalysisZoom(current: number, direction: 1 | -1): number {
  return normalizeAnalysisZoom(current + direction * ANALYSIS_ZOOM_STEP);
}
