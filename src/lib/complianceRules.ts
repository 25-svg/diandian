import type { TimedText } from "./transcriptSignals";

export type ComplianceSeverity = "review" | "blocked";

export type ComplianceFinding = {
  ruleId: string;
  severity: ComplianceSeverity;
  label: string;
  time: number;
  quote: string;
  guidance: string;
};

type ComplianceRule = {
  id: string;
  terms: readonly string[];
  severity: ComplianceSeverity;
  label: string;
  guidance: string;
};

const RULES: readonly ComplianceRule[] = [
  {
    id: "absolute-claim",
    terms: ["全网最低", "最便宜", "百分百没问题", "绝对没问题"],
    severity: "blocked",
    label: "绝对化或无法证明的承诺",
    guidance: "删除绝对化表述，改为展示可核验的检测、成色或价格依据。",
  },
  {
    id: "permanent-promise",
    terms: ["永久保修", "终身保修", "永远不会坏"],
    severity: "blocked",
    label: "超出已确认范围的售后承诺",
    guidance: "只说明公司已确认的售后范围和期限。",
  },
  {
    id: "condition-needs-proof",
    terms: ["全新未拆", "零使用", "官方在保", "原封未拆"],
    severity: "review",
    label: "成色或保修事实待核验",
    guidance: "核对商品参数卡、检测记录或官方凭证后再使用。",
  },
];

export function scanComplianceRisks(entries: readonly TimedText[]): ComplianceFinding[] {
  const findings: ComplianceFinding[] = [];
  for (const entry of entries) {
    for (const rule of RULES) {
      if (!rule.terms.some((term) => entry.text.includes(term))) continue;
      findings.push({
        ruleId: rule.id,
        severity: rule.severity,
        label: rule.label,
        time: entry.start,
        quote: entry.text,
        guidance: rule.guidance,
      });
    }
  }
  return findings;
}
