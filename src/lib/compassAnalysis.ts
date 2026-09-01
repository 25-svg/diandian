export type CompassCaptureSummary = {
  captureId: string;
  targetDate: string;
  targetShopName: string;
  status: string;
  message: string;
  startedAt: string;
  finishedAt?: string | null;
  analysisResponseCount: number;
  metricLabels: string[];
  sessionKeys: string[];
};

export type CompassMetricPoint = {
  timeLabel: string;
  sortValue: number;
  value: number;
  valid?: boolean;
};

export type CompassProductAnalysis = {
  productId: string;
  productName: string;
  explainCount: number;
  clickCount: number;
  paymentAmount: number;
  soldCount: number;
  explainStartTime?: string;
  explainEndTime?: string;
};

export type CompassAudienceItem = {
  dimension: string;
  label: string;
  value: number;
  unit: string;
  sourceEndpoint: string;
};

export type CompassQianchuanMetric = {
  key: string;
  label: string;
  value: number;
  unit: string;
  sourceEndpoint: string;
};

export type CompassMetricAnalysis = {
  id?: string;
  label: string;
  unit: string;
  sourceKind?: "compass_native" | "legacy" | string;
  points: CompassMetricPoint[];
  minimum: number;
  maximum: number;
  average: number;
  peakTime: string;
  trendPercent: number;
  volatility: number;
  sourceEndpoint: string;
};

export type CompassMetricDimension = "count" | "currency" | "ratio" | "duration" | "number";

export type CompassTimeRange = {
  start: number;
  end: number;
};

export type CompassMetricChangePoint = {
  timeLabel: string;
  sortValue: number;
  previousValue: number;
  value: number;
  changePercent: number;
};

export type CompassMetricAnomaly = {
  id: string;
  direction: "high" | "low";
  startTime: string;
  endTime: string;
  startSortValue: number;
  endSortValue: number;
  value: number;
  baselineValue: number;
  deviationPercent: number | null;
};

export type CompassMetricWindowAnalysis = {
  metricId: string;
  label: string;
  unit: string;
  dimension: CompassMetricDimension;
  points: CompassMetricPoint[];
  validPointCount: number;
  missingPointCount: number;
  status: "sufficient" | "insufficient" | "flat" | "no_data";
  trendDirection: "rising" | "falling" | "stable" | "not_applicable";
  trendPercent: number | null;
  minimum: number | null;
  minimumTime: string;
  maximum: number | null;
  maximumTime: string;
  average: number | null;
  volatility: number | null;
  baselineValue: number | null;
  baselineSource: "explicit" | "statistical" | "none";
  changes: CompassMetricChangePoint[];
  anomalies: CompassMetricAnomaly[];
};

export type CompassMetricStyle = {
  color: string;
  dash: string;
  shape: "circle" | "square" | "triangle" | "diamond";
};

const METRIC_STYLES: CompassMetricStyle[] = [
  { color: "#1769d2", dash: "", shape: "circle" },
  { color: "#a04400", dash: "9 5", shape: "square" },
  { color: "#18794e", dash: "2 5", shape: "triangle" },
  { color: "#6e56cf", dash: "12 4 2 4", shape: "diamond" },
];

function metricPointValid(point: CompassMetricPoint): boolean {
  return point.valid !== false && Number.isFinite(point.value);
}

function median(values: readonly number[]): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2
    ? sorted[middle]
    : (sorted[middle - 1] + sorted[middle]) / 2;
}

export function compassMetricId(metric: Pick<CompassMetricAnalysis, "id" | "label" | "sourceEndpoint">): string {
  if (metric.id?.trim()) return metric.id.trim();
  return `${metric.sourceEndpoint}|${metric.label}`
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fff]+/g, "-")
    .replace(/^-|-$/g, "");
}

export function compassMetricDimension(metric: Pick<CompassMetricAnalysis, "label" | "unit">): CompassMetricDimension {
  const text = `${metric.label} ${metric.unit}`;
  if (/%|率|比例/.test(text)) return "ratio";
  if (/¥|￥|元|金额|gpm/i.test(text)) return "currency";
  if (/时长|秒|分钟|小时/.test(text)) return "duration";
  if (/人数|次数|件数|点击数|进入人数/.test(text)) return "count";
  return "number";
}

export function compassMetricStyle(index: number): CompassMetricStyle {
  return METRIC_STYLES[Math.abs(index) % METRIC_STYLES.length];
}

export function analyzeCompassMetricWindow(
  metric: CompassMetricAnalysis,
  range?: CompassTimeRange | null,
  explicitBaseline?: number | null,
): CompassMetricWindowAnalysis {
  const start = Math.min(range?.start ?? Number.NEGATIVE_INFINITY, range?.end ?? Number.POSITIVE_INFINITY);
  const end = Math.max(range?.start ?? Number.NEGATIVE_INFINITY, range?.end ?? Number.POSITIVE_INFINITY);
  const points = [...metric.points]
    .filter((point) => point.sortValue >= start && point.sortValue <= end)
    .sort((left, right) => left.sortValue - right.sortValue);
  const validPoints = points.filter(metricPointValid);
  const values = validPoints.map((point) => point.value);
  const missingPointCount = points.length - validPoints.length;
  const average = values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
  const volatility = average == null
    ? null
    : Math.sqrt(values.reduce((sum, value) => sum + (value - average) ** 2, 0) / values.length);

  let minimumPoint: CompassMetricPoint | undefined;
  let maximumPoint: CompassMetricPoint | undefined;
  for (const point of validPoints) {
    if (!minimumPoint || point.value < minimumPoint.value) minimumPoint = point;
    if (!maximumPoint || point.value > maximumPoint.value) maximumPoint = point;
  }

  const first = validPoints[0];
  const last = validPoints[validPoints.length - 1];
  const trendPercent = first && last && Math.abs(first.value) > Number.EPSILON
    ? (last.value - first.value) / Math.abs(first.value) * 100
    : null;
  const trendDirection = trendPercent == null
    ? "not_applicable"
    : trendPercent > 5
      ? "rising"
      : trendPercent < -5
        ? "falling"
        : "stable";
  const allEqual = values.length > 1 && values.every((value) => Math.abs(value - values[0]) <= Number.EPSILON);
  const status = values.length === 0
    ? "no_data"
    : allEqual
      ? "flat"
      : values.length < 3
        ? "insufficient"
        : "sufficient";

  const changes: CompassMetricChangePoint[] = [];
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1];
    const current = points[index];
    if (!metricPointValid(previous) || !metricPointValid(current) || Math.abs(previous.value) <= Number.EPSILON) continue;
    const changePercent = (current.value - previous.value) / Math.abs(previous.value) * 100;
    if (Math.abs(changePercent) >= 20) {
      changes.push({
        timeLabel: current.timeLabel,
        sortValue: current.sortValue,
        previousValue: previous.value,
        value: current.value,
        changePercent,
      });
    }
  }

  const statisticalBaseline = values.length ? median(values) : null;
  const baselineValue = Number.isFinite(explicitBaseline) ? Number(explicitBaseline) : statisticalBaseline;
  const baselineSource = Number.isFinite(explicitBaseline)
    ? "explicit"
    : statisticalBaseline == null
      ? "none"
      : "statistical";
  const mad = statisticalBaseline == null ? 0 : median(values.map((value) => Math.abs(value - statisticalBaseline)));
  const threshold = baselineValue == null
    ? Number.POSITIVE_INFINITY
    : Math.max(Math.abs(baselineValue) * 0.2, baselineSource === "explicit" ? 0 : mad * 3, Number.EPSILON);
  const anomalies: CompassMetricAnomaly[] = [];
  let active: { direction: "high" | "low"; points: CompassMetricPoint[] } | null = null;
  const flushAnomaly = () => {
    if (!active || baselineValue == null || !active.points.length) return;
    const extreme = active.points.reduce((best, point) => active!.direction === "high"
      ? (point.value > best.value ? point : best)
      : (point.value < best.value ? point : best));
    anomalies.push({
      id: `${compassMetricId(metric)}-anomaly-${anomalies.length + 1}`,
      direction: active.direction,
      startTime: active.points[0].timeLabel,
      endTime: active.points[active.points.length - 1].timeLabel,
      startSortValue: active.points[0].sortValue,
      endSortValue: active.points[active.points.length - 1].sortValue,
      value: extreme.value,
      baselineValue,
      deviationPercent: Math.abs(baselineValue) > Number.EPSILON
        ? (extreme.value - baselineValue) / Math.abs(baselineValue) * 100
        : null,
    });
    active = null;
  };
  for (const point of points) {
    if (!metricPointValid(point) || baselineValue == null) {
      flushAnomaly();
      continue;
    }
    const delta = point.value - baselineValue;
    if (Math.abs(delta) + Number.EPSILON < threshold) {
      flushAnomaly();
      continue;
    }
    const direction = delta >= 0 ? "high" : "low";
    if (active?.direction !== direction) {
      flushAnomaly();
      active = { direction, points: [] };
    }
    active.points.push(point);
  }
  flushAnomaly();

  return {
    metricId: compassMetricId(metric),
    label: metric.label,
    unit: metric.unit,
    dimension: compassMetricDimension(metric),
    points,
    validPointCount: validPoints.length,
    missingPointCount,
    status,
    trendDirection,
    trendPercent,
    minimum: minimumPoint?.value ?? null,
    minimumTime: minimumPoint?.timeLabel ?? "",
    maximum: maximumPoint?.value ?? null,
    maximumTime: maximumPoint?.timeLabel ?? "",
    average,
    volatility,
    baselineValue,
    baselineSource,
    changes,
    anomalies,
  };
}

export function normalizeCompassMetricValues(points: readonly CompassMetricPoint[]): Array<number | null> {
  const valid = points.filter(metricPointValid).map((point) => point.value);
  if (!valid.length) return points.map(() => null);
  const minimum = Math.min(...valid);
  const maximum = Math.max(...valid);
  if (Math.abs(maximum - minimum) <= Number.EPSILON) {
    return points.map((point) => metricPointValid(point) ? 50 : null);
  }
  return points.map((point) => metricPointValid(point)
    ? (point.value - minimum) / (maximum - minimum) * 100
    : null);
}

export function compassAnalysisRequestKey(metricIds: readonly string[], range: CompassTimeRange): string {
  return `${[...metricIds].sort().join(",")}@${Math.min(range.start, range.end)}-${Math.max(range.start, range.end)}`;
}

export function compassEvidenceStatus(event?: Pick<CompassDeclineEvent, "transcriptExcerpt"> | null): "linked" | "pending" {
  return event?.transcriptExcerpt?.trim() ? "linked" : "pending";
}

export function compassEvidenceExplanation(event?: Pick<CompassDeclineEvent, "transcriptExcerpt"> | null): string {
  return compassEvidenceStatus(event) === "linked"
    ? "已关联对应直播时间段和逐字稿；同期变化仅表示时间相关，不代表因果。"
    : "待核实：当前时间段没有逐字稿或节奏地图证据。";
}

function secondsOfDay(value: string): number | null {
  const match = value.match(/(?:^|[T\s])(\d{1,2}):(\d{2})(?::(\d{2}))?/);
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  const second = Number(match[3] ?? 0);
  return hour <= 23 && minute <= 59 && second <= 59 ? hour * 3600 + minute * 60 + second : null;
}

export function compassPointOffsetSeconds(
  point: Pick<CompassMetricPoint, "timeLabel" | "sortValue">,
  sessionStartedAt: string,
): number | null {
  const sessionEpoch = Date.parse(sessionStartedAt);
  const pointEpoch = Date.parse(point.timeLabel);
  if (Number.isFinite(sessionEpoch) && Number.isFinite(pointEpoch) && /\d{4}-\d{2}-\d{2}/.test(point.timeLabel)) {
    return Math.max(0, (pointEpoch - sessionEpoch) / 1000);
  }
  const startClock = secondsOfDay(sessionStartedAt);
  const pointClock = secondsOfDay(point.timeLabel)
    ?? (Number.isFinite(point.sortValue) && point.sortValue >= 0 && point.sortValue < 86400 ? point.sortValue : null);
  if (startClock == null || pointClock == null) return null;
  let offset = pointClock - startClock;
  if (offset < -43200) offset += 86400;
  return offset >= 0 ? offset : null;
}

export type CompassPointInsight = {
  previousValue: number | null;
  delta: number | null;
  changePercent: number | null;
  direction: "up" | "down" | "flat" | "first";
};

export function analyzeCompassPoint(metric: CompassMetricAnalysis, point: CompassMetricPoint): CompassPointInsight {
  const points = metric.points.filter(metricPointValid).sort((left, right) => left.sortValue - right.sortValue);
  const index = points.findIndex((candidate) => candidate === point
    || (candidate.sortValue === point.sortValue && candidate.timeLabel === point.timeLabel));
  const previous = index > 0 ? points[index - 1] : null;
  if (!previous) return { previousValue: null, delta: null, changePercent: null, direction: "first" };
  const delta = point.value - previous.value;
  return {
    previousValue: previous.value,
    delta,
    changePercent: Math.abs(previous.value) > Number.EPSILON ? delta / Math.abs(previous.value) * 100 : null,
    direction: Math.abs(delta) <= Number.EPSILON ? "flat" : delta > 0 ? "up" : "down",
  };
}

export type CompassChartTick = {
  label: string;
  percent: number;
};

export type CompassMetricChange = {
  label: string;
  changePercent: number;
};

export type CompassDeclineEvent = {
  id: string;
  metricLabel: string;
  startTime: string;
  endTime: string;
  startSortValue: number;
  endSortValue: number;
  baselineValue: number;
  lowestValue: number;
  dropPercent: number;
  seekSeconds?: number | null;
  transcriptStartSeconds?: number | null;
  transcriptEndSeconds?: number | null;
  transcriptExcerpt: string;
  correlatedChanges: CompassMetricChange[];
  possibleCauses: string[];
  confidence: string;
  explanation: string;
  limitations: string;
};

export type CompassCaptureAnalysis = {
  captureId: string;
  targetDate: string;
  targetShopName: string;
  captureStatus: string;
  quality: {
    analysisResponseCount: number;
    metricCount: number;
    complete: boolean;
    issues: string[];
  };
  metrics: CompassMetricAnalysis[];
  products: CompassProductAnalysis[];
  audience: CompassAudienceItem[];
  qianchuan: CompassQianchuanMetric[];
  coverage?: Array<{ key: string; label: string; status: string; message?: string }>;
  findings?: Array<{ level: string; title: string; summary: string; evidence: string }>;
  declineEvents: CompassDeclineEvent[];
  aiSummary?: string | null;
  generatedAt: string;
};

export type CompassSessionIdentity = {
  id: number;
  shopName: string;
  startedAt: string;
};

function normalizedSessionClock(value: string): string {
  return value.replace(/\D/g, "").slice(0, 12);
}

function sessionMinute(value: string): number | null {
  const normalized = normalizedSessionClock(value);
  if (normalized.length !== 12) return null;
  const minute = Number(normalized);
  return Number.isFinite(minute) ? minute : null;
}

export function normalizeCompassShopName(value: string): string {
  const compact = value.replace(/\s+/g, "").trim();
  if (compact.startsWith("金典拍拍") && compact.includes("科创")) return "金典拍拍科创专卖店";
  if (compact.startsWith("金典拍拍") && (compact.includes("相机") || compact.includes("微单"))) {
    return "金典拍拍相机专卖店";
  }
  return compact;
}

export function compassSessionKeyMatches(
  session: Pick<CompassSessionIdentity, "shopName" | "startedAt">,
  sessionKey: string,
): boolean {
  const [shopName = "", startedAt = "", endedAt = ""] = sessionKey.split("|");
  if (normalizeCompassShopName(shopName) !== normalizeCompassShopName(session.shopName)) return false;
  const sessionStart = sessionMinute(session.startedAt);
  const captureStart = sessionMinute(startedAt);
  const captureEnd = sessionMinute(endedAt);
  if (sessionStart == null || captureStart == null) return false;
  if (sessionStart === captureStart) return true;
  return captureEnd != null && captureStart <= sessionStart && sessionStart <= captureEnd;
}

export function selectCaptureForSession(
  captures: readonly CompassCaptureSummary[],
  session: CompassSessionIdentity,
): CompassCaptureSummary | null {
  const targetDate = session.startedAt.slice(0, 10);
  const sameDayAndShop = captures.filter((capture) =>
    capture.targetDate === targetDate
    && normalizeCompassShopName(capture.targetShopName) === normalizeCompassShopName(session.shopName)
    && capture.analysisResponseCount > 0
  );
  const matchesSession = (capture: CompassCaptureSummary) =>
    capture.sessionKeys.some((sessionKey) => compassSessionKeyMatches(session, sessionKey))
    || capture.sessionKeys.length === 0;
  return sameDayAndShop.find((capture) =>
    ["completed", "partial"].includes(capture.status) && matchesSession(capture)
  ) ?? sameDayAndShop.find(matchesSession) ?? null;
}

export function compassChartPath(metric: CompassMetricAnalysis): string {
  const validPoints = metric.points.filter(metricPointValid);
  if (validPoints.length < 2) return "";
  const width = 760;
  const height = 150;
  const left = 20;
  const top = 14;
  const minX = Math.min(...metric.points.map((point) => point.sortValue));
  const maxX = Math.max(...metric.points.map((point) => point.sortValue));
  const minY = Math.min(...validPoints.map((point) => point.value));
  const maxY = Math.max(...validPoints.map((point) => point.value));
  let penDown = false;
  return metric.points.map((point) => {
    if (!metricPointValid(point)) {
      penDown = false;
      return "";
    }
    const x = left + ((point.sortValue - minX) / Math.max(maxX - minX, 1)) * (width - left * 2);
    const y = top + (1 - ((point.value - minY) / Math.max(maxY - minY, 1))) * (height - top * 2);
    const command = penDown ? "L" : "M";
    penDown = true;
    return `${command}${x.toFixed(1)},${y.toFixed(1)}`;
  }).filter(Boolean).join(" ");
}

export function compassChartPercent(metric: CompassMetricAnalysis, sortValue: number): number {
  if (!metric.points.length) return 0;
  const minX = Math.min(...metric.points.map((point) => point.sortValue));
  const maxX = Math.max(...metric.points.map((point) => point.sortValue));
  return Math.max(0, Math.min(100, ((sortValue - minX) / Math.max(maxX - minX, 1)) * 100));
}

export function compassChartTicks(metric: CompassMetricAnalysis, desiredCount = 5): CompassChartTick[] {
  if (!metric.points.length) return [];
  const count = Math.max(2, Math.min(desiredCount, metric.points.length));
  const indexes = Array.from({ length: count }, (_, index) =>
    Math.round(index * (metric.points.length - 1) / (count - 1))
  );
  return [...new Set(indexes)].map((index) => ({
    label: metric.points[index]?.timeLabel ?? "",
    percent: compassChartPercent(metric, metric.points[index]?.sortValue ?? 0),
  }));
}

export function formatCompassOffset(seconds?: number | null): string {
  if (seconds == null || !Number.isFinite(seconds)) return "未绑定";
  const value = Math.max(0, Math.floor(seconds));
  return `${String(Math.floor(value / 3600)).padStart(2, "0")}:${String(Math.floor(value % 3600 / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
}
