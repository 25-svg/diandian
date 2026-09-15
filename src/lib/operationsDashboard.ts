export type OperationsSection = "运营总览" | "主播团队" | "改进任务" | "资产审核";

export type OperationsTaskStatus = "待领取" | "进行中" | "待复核" | "已完成";

export interface OperationsAnchorRow {
  id: string;
  name: string;
  team: string;
  liveCount: number;
  gmvYuan: number;
  conversionRate: number;
  averageOnline: number;
  score: number;
  scoreDelta: number;
  riskCount: number;
  focus: string;
}

export interface OperationsTaskRow {
  id: string;
  anchorId: string;
  anchorName: string;
  issue: string;
  evidence: string;
  action: string;
  dueDate: string;
  status: OperationsTaskStatus;
}

export interface OperationsAssetRow {
  id: string;
  anchorName: string;
  title: string;
  category: "成交话术" | "优秀切片" | "培训案例";
  evidence: string;
  submittedAt: string;
  risk: string;
}

export const OPERATIONS_SECTIONS: OperationsSection[] = [
  "运营总览",
  "主播团队",
  "改进任务",
  "资产审核",
];

export const operationsDemoAnchors: OperationsAnchorRow[] = [
  { id: "anchor-a", name: "主播 A", team: "相机一组", liveCount: 4, gmvYuan: 68420, conversionRate: 3.8, averageOnline: 426, score: 88, scoreDelta: 4, riskCount: 0, focus: "保持逼单节奏" },
  { id: "anchor-b", name: "主播 B", team: "相机一组", liveCount: 3, gmvYuan: 52680, conversionRate: 3.1, averageOnline: 389, score: 81, scoreDelta: 1, riskCount: 1, focus: "补强价格解释" },
  { id: "anchor-c", name: "主播 C", team: "相机二组", liveCount: 3, gmvYuan: 41320, conversionRate: 2.4, averageOnline: 344, score: 73, scoreDelta: -5, riskCount: 2, focus: "处理在线下跌" },
  { id: "anchor-d", name: "主播 D", team: "相机二组", liveCount: 2, gmvYuan: 24000, conversionRate: 1.9, averageOnline: 218, score: 68, scoreDelta: -2, riskCount: 2, focus: "完成异议训练" },
];

export const operationsDemoTasks: OperationsTaskRow[] = [
  { id: "task-01", anchorId: "anchor-c", anchorName: "主播 C", issue: "讲解 18 分钟后在线连续下降", evidence: "09-14 场次 · 00:18:20", action: "复练开价前 90 秒承接话术", dueDate: "今天 18:00", status: "进行中" },
  { id: "task-02", anchorId: "anchor-d", anchorName: "主播 D", issue: "三次价格异议未说明验机保障", evidence: "09-14 场次 · 00:42:16", action: "完成价格异议情景训练 3 题", dueDate: "明天 12:00", status: "待领取" },
  { id: "task-03", anchorId: "anchor-b", anchorName: "主播 B", issue: "优惠口径与商品参数库不一致", evidence: "09-13 场次 · 01:05:08", action: "提交修正版话术并标记事实来源", dueDate: "今天 16:00", status: "待复核" },
  { id: "task-04", anchorId: "anchor-a", anchorName: "主播 A", issue: "新品开场停留承接偏弱", evidence: "09-12 场次 · 00:03:42", action: "完成优秀案例跟练", dueDate: "已完成", status: "已完成" },
];

export const operationsDemoAssets: OperationsAssetRow[] = [
  { id: "asset-01", anchorName: "主播 A", title: "二手相机成色解释四步法", category: "成交话术", evidence: "原视频 + 逐字稿 + 订单窗口", submittedAt: "今天 10:24", risk: "价格数字需复核" },
  { id: "asset-02", anchorName: "主播 B", title: "高价机型验机保障回应", category: "优秀切片", evidence: "42 秒视频 + 人工标注", submittedAt: "昨天 17:40", risk: "无明显风险" },
  { id: "asset-03", anchorName: "主播 C", title: "观众比价异议训练案例", category: "培训案例", evidence: "真实评论 + 回答 + 评分", submittedAt: "昨天 15:12", risk: "需隐藏用户昵称" },
];

export function filterOperationsAnchors(
  anchors: OperationsAnchorRow[],
  team: string,
  query: string,
): OperationsAnchorRow[] {
  const normalized = query.trim().toLocaleLowerCase("zh-CN");
  return anchors.filter((anchor) => {
    if (team !== "全部团队" && anchor.team !== team) return false;
    if (!normalized) return true;
    return [anchor.name, anchor.team, anchor.focus].some((value) =>
      value.toLocaleLowerCase("zh-CN").includes(normalized),
    );
  });
}

export function operationsSummary(anchors: OperationsAnchorRow[], tasks: OperationsTaskRow[]) {
  const liveCount = anchors.reduce((sum, item) => sum + item.liveCount, 0);
  const gmvYuan = anchors.reduce((sum, item) => sum + item.gmvYuan, 0);
  const averageConversionRate = anchors.length
    ? anchors.reduce((sum, item) => sum + item.conversionRate, 0) / anchors.length
    : 0;
  const attentionAnchorCount = anchors.filter((item) => item.riskCount > 0).length;
  const openTaskCount = tasks.filter((item) => item.status !== "已完成").length;
  return { liveCount, gmvYuan, averageConversionRate, attentionAnchorCount, openTaskCount };
}
