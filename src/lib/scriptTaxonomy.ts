import type { CandidateType } from "./archiveAnalysis";

export type OperationalScriptTag =
  | "迎新留人"
  | "需求判断"
  | "选品推荐"
  | "塑品讲解"
  | "异议处理"
  | "信任保障"
  | "报价链接"
  | "逼单成交"
  | "金句示范"
  | "反面案例";

const TAG_BY_TYPE: Record<CandidateType, OperationalScriptTag> = {
  "完整成交链路": "逼单成交",
  "高质量金句": "金句示范",
  "需求判断": "需求判断",
  "产品推荐": "选品推荐",
  "产品讲解": "塑品讲解",
  "异议处理": "异议处理",
  "留人钩子": "迎新留人",
  "信任建立": "信任保障",
  "售后与风险消除": "信任保障",
  "价格、优惠或链接承接": "报价链接",
  "催单与成交确认": "逼单成交",
  "需要改进的反面案例": "反面案例",
  "关键话术片段": "金句示范",
  "成交收口": "逼单成交",
  "成交片段": "逼单成交",
  "疑似成交片段": "逼单成交",
  "问价未见成交信号": "需求判断",
  "转品/上链接片段": "报价链接",
  "讲得散片段": "反面案例",
  "无法判断": "反面案例",
};

export function operationalScriptTag(type: CandidateType): OperationalScriptTag {
  return TAG_BY_TYPE[type];
}

export function operationalScriptLabel(type: CandidateType): string {
  return `${operationalScriptTag(type)} · ${type}`;
}
