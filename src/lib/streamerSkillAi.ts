import {
  MIN_SKILL_SCORE_SOURCES,
  STREAMER_SKILL_DIMENSIONS,
  STREAMER_SKILL_DIMENSION_KEYS,
  type SkillEvidence,
  type StreamerSkillDimensionKey,
} from "./streamerProfile.js";

export type StreamerSkillAiRating = {
  dimension: StreamerSkillDimensionKey;
  score: number | null;
  basis: string;
  evidenceIds: string[];
};

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function isDimension(value: unknown): value is StreamerSkillDimensionKey {
  return STREAMER_SKILL_DIMENSION_KEYS.includes(value as StreamerSkillDimensionKey);
}

/**
 * This prompt deliberately makes an incomplete evidence set a first-class
 * outcome. The model is never asked to invent a score merely to fill the radar.
 */
export function streamerSkillProfileSystemPrompt(): string {
  const rubric = STREAMER_SKILL_DIMENSIONS.map(({ key, label }) => `- ${key}（${label}）`).join("\n");
  return `你是电商直播主播能力评估器。只根据用户消息中给出的“已完成分析证据”或“录播字幕原文”判断，不得补写直播原话、商品信息、客户意图、订单或合规事实。

请评估以下八个维度：
${rubric}

硬性规则：
1. 每个维度都必须返回一项；只有当该维度至少引用 ${MIN_SKILL_SCORE_SOURCES} 个不同 sourceId 的证据时，score 才能是 0–100 的整数。否则 score 必须为 null，evidenceIds 必须为空，并在 basis 明确写“数据不足”。
2. 只能引用输入 evidence 中真实存在的 id；不得引用未提供的 id。每个被评分维度至少列出两条证据，且来自不同 sourceId。
3. evidence 的 summary 与 basis 只代表已有分析结论或原始录播字幕；若它们无法直接支持某个维度，就返回数据不足。字幕原文只可按实际内容判断，不能因为主播场次多、表达流畅或某个维度得分高而推断其他维度。
4. 分数衡量“所给证据内的表现”，不是主播永久能力；basis 用不超过 90 个汉字说明共同证据支持的判断，并指出明显限制。
5. 风险合规只可依据明确的合规、风险提示或违规证据判断；没有此类明确证据即数据不足。
6. 只返回合法 JSON，不要 Markdown、解释或代码围栏。结构固定为：{"ratings":[{"dimension":"needs_confirmation","score":null,"basis":"数据不足：…","evidenceIds":[]}]}。`;
}

export function buildStreamerSkillProfileUserMessage(
  streamerName: string,
  evidence: readonly SkillEvidence[],
): string {
  return JSON.stringify({
    streamer: streamerName,
    evidence: evidence.map((item) => ({
      id: item.id,
      sourceId: item.sourceId,
      summary: item.summary,
      basis: item.basis,
    })),
  });
}

function parseJson(textValue: string): unknown {
  const trimmed = textValue.trim().replace(/^```(?:json)?\s*/iu, "").replace(/\s*```$/u, "");
  return JSON.parse(trimmed);
}

/** Rejects any model output that attempts to cite unavailable or insufficient evidence. */
export function parseStreamerSkillAiRatings(
  response: string,
  availableEvidence: readonly SkillEvidence[],
): StreamerSkillAiRating[] {
  let parsed: unknown;
  try {
    parsed = parseJson(response);
  } catch {
    throw new Error("AI 返回格式无效，请稍后重试。");
  }
  if (!isRecord(parsed) || !Array.isArray(parsed.ratings)) {
    throw new Error("AI 未返回能力评分列表，请稍后重试。");
  }
  const evidenceById = new Map(availableEvidence.map((item) => [item.id, item]));
  const results = new Map<StreamerSkillDimensionKey, StreamerSkillAiRating>();
  for (const value of parsed.ratings) {
    if (!isRecord(value) || !isDimension(value.dimension) || results.has(value.dimension)) continue;
    const basis = text(value.basis);
    const evidenceIds = Array.isArray(value.evidenceIds)
      ? [...new Set(value.evidenceIds.map(text).filter((id) => evidenceById.has(id)))]
      : [];
    const sourceCount = new Set(evidenceIds.map((id) => evidenceById.get(id)?.sourceId).filter(Boolean)).size;
    const score = typeof value.score === "number" && Number.isInteger(value.score)
      && value.score >= 0 && value.score <= 100 && evidenceIds.length >= MIN_SKILL_SCORE_SOURCES
      && sourceCount >= MIN_SKILL_SCORE_SOURCES
      ? value.score
      : null;
    results.set(value.dimension, {
      dimension: value.dimension,
      score,
      basis: basis || (score === null ? "数据不足：现有分析证据无法支持该维度评分。" : "AI 基于已完成分析证据给出的判断。"),
      evidenceIds: score === null ? [] : evidenceIds,
    });
  }
  return STREAMER_SKILL_DIMENSIONS.map(({ key }) => results.get(key) || ({
    dimension: key,
    score: null,
    basis: "数据不足：AI 未返回可用的本维度证据。",
    evidenceIds: [],
  }));
}
