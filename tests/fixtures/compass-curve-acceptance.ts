import type { CompassCaptureAnalysis, CompassMetricAnalysis } from "../../src/lib/compassAnalysis";

const minute = (index: number) => 10 * 3600 + index * 60;

function metric(
  label: string,
  unit: string,
  values: Array<number | null>,
): CompassMetricAnalysis {
  const valid = values.filter((value): value is number => value != null);
  return {
    label,
    unit,
    sourceKind: "compass_native",
    sourceEndpoint: "/compass_api/shop/live/live_screen/product_overall_trend",
    points: values.map((value, index) => ({
      timeLabel: `10:${String(index).padStart(2, "0")}`,
      sortValue: minute(index),
      value: value ?? 0,
      valid: value != null,
    })),
    minimum: Math.min(...valid),
    maximum: Math.max(...valid),
    average: valid.reduce((sum, value) => sum + value, 0) / valid.length,
    peakTime: `10:${String(values.indexOf(Math.max(...valid))).padStart(2, "0")}`,
    trendPercent: 0,
    volatility: 0,
  };
}

export const COMPASS_CURVE_ACCEPTANCE: CompassCaptureAnalysis = {
  captureId: "compass-curve-acceptance-v1",
  targetDate: "2026-08-12",
  targetShopName: "验收店铺",
  captureStatus: "completed",
  quality: {
    analysisResponseCount: 12,
    metricCount: 3,
    complete: true,
    issues: [],
  },
  metrics: [
    metric("在线人数", "人", [100, 102, 98, 100, 150, 145, 70, 72, 100, 200]),
    metric("投放消耗", "元", [10, 12, 11, 13, 35, 32, 15, 16, 18, 20]),
    metric("缺失样本", "次", [10, null, 12, null, 11, 10, 9, 8, 9, 10]),
  ],
  products: [{
    productId: "camera-1",
    productName: "佳能相机套装",
    explainCount: 3,
    clickCount: 28,
    paymentAmount: 12999,
    soldCount: 2,
    explainStartTime: "10:04",
    explainEndTime: "10:06",
  }],
  audience: [
    { dimension: "性别", label: "女", value: 68, unit: "%", sourceEndpoint: "/audience" },
    { dimension: "性别", label: "男", value: 32, unit: "%", sourceEndpoint: "/audience" },
    { dimension: "年龄", label: "25-34岁", value: 42, unit: "%", sourceEndpoint: "/audience" },
  ],
  qianchuan: [
    { key: "spend", label: "投放消耗", value: 24.01, unit: "元", sourceEndpoint: "/qianchuan" },
    { key: "roi", label: "ROI", value: 1.8, unit: "", sourceEndpoint: "/qianchuan" },
  ],
  declineEvents: [
    {
      id: "online-low-evidence",
      metricLabel: "在线人数",
      startTime: "10:06",
      endTime: "10:07",
      startSortValue: minute(6),
      endSortValue: minute(7),
      baselineValue: 100,
      lowestValue: 70,
      dropPercent: 30,
      seekSeconds: 360,
      transcriptStartSeconds: 330,
      transcriptEndSeconds: 450,
      transcriptExcerpt: "主播正在切换商品并重新介绍赠品。",
      correlatedChanges: [{ label: "投放消耗", changePercent: -53.1 }],
      possibleCauses: ["同期出现商品切换，仅表示时间相关，仍需人工核实。"],
      confidence: "中",
      explanation: "在线人数下降与商品切换处于同一时间段，属于时间相关，不代表因果。",
      limitations: "需要结合录播、逐字稿和节奏地图复核。",
    },
  ],
  aiSummary: null,
  generatedAt: "2026-08-28T10:00:00+08:00",
};
