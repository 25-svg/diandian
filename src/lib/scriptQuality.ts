import { buildScriptQualityPrompt } from "./scriptQualityPrompts.js";

export type ScriptIssueKind =
  | "structure_gap"
  | "unclear_expression"
  | "factual_risk"
  | "compliance_risk";

export type ScriptIssueAnnotation = {
  cueId: number;
  startMs: number;
  endMs: number;
  kind: ScriptIssueKind;
  originalText: string;
  reason: string;
  suggestion: string;
};

export type ScriptQualityChunk = {
  index: number;
  total: number;
  cueIds: number[];
  text: string;
};

export type TranscriptCue = {
  id: number;
  start: number;
  end: number;
  text: string;
};

const ISSUE_KINDS = new Set<ScriptIssueKind>([
  "structure_gap",
  "unclear_expression",
  "factual_risk",
  "compliance_risk",
]);

const LEGACY_ISSUE_KINDS: Record<string, ScriptIssueKind> = {
  master_structure_gap: "structure_gap",
};

export function scriptIssueKindLabel(kind: ScriptIssueKind): string {
  switch (kind) {
    case "structure_gap":
      return "讲法顺序";
    case "unclear_expression":
      return "表达可优化";
    case "factual_risk":
      return "信息不准确";
    case "compliance_risk":
      return "承诺过重";
  }
}

export function splitTranscriptForScriptQuality(
  entries: TranscriptCue[],
  chunkSeconds = 480,
  overlapSeconds = 30,
): ScriptQualityChunk[] {
  if (!entries.length) return [];
  const duration = Math.max(...entries.map((entry) => entry.end));
  const total = Math.max(1, Math.ceil(duration / chunkSeconds));
  const chunks: ScriptQualityChunk[] = [];
  for (let index = 0, start = 0; start < duration; index += 1, start += chunkSeconds) {
    const end = start + chunkSeconds;
    const slice = entries.filter(
      (entry) => entry.end >= Math.max(0, start - overlapSeconds) && entry.start <= end + overlapSeconds,
    );
    if (!slice.length) continue;
    chunks.push({
      index,
      total,
      cueIds: slice.map((entry) => entry.id),
      text: slice
        .map((entry) => `[cue:${entry.id}] ${formatClock(entry.start)} ${entry.text}`)
        .join("\n"),
    });
  }
  return chunks.map((chunk, index, list) => ({ ...chunk, total: list.length, index }));
}

function formatClock(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;
  return hours > 0
    ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function repairJsonText(input: string): string {
  const withoutTrailingCommas = input.replace(/,\s*([}\]])/g, "$1");
  return escapeControlCharsInJsonStrings(
    withoutTrailingCommas
      .replace(/[\u201c\u201d]/g, '"')
      .replace(/[\u2018\u2019]/g, "'"),
  );
}

function escapeControlCharsInJsonStrings(input: string): string {
  let out = "";
  let inString = false;
  let escaped = false;
  for (let index = 0; index < input.length; index += 1) {
    const character = input[index];
    if (inString) {
      if (escaped) {
        out += character;
        escaped = false;
        continue;
      }
      if (character === "\\") {
        out += character;
        escaped = true;
        continue;
      }
      if (character === '"') {
        inString = false;
        out += character;
        continue;
      }
      if (character === "\n") {
        out += "\\n";
        continue;
      }
      if (character === "\r") continue;
      if (character === "\t") {
        out += "\\t";
        continue;
      }
      out += character;
      continue;
    }
    if (character === '"') inString = true;
    out += character;
  }
  return out;
}

function decodeLooseJsonString(value: string): string {
  try {
    return JSON.parse(`"${value.replace(/\r?\n/g, "\\n")}"`);
  } catch {
    return value.replace(/\\"/g, '"').replace(/\\n/g, "\n").trim();
  }
}

function jsonCandidates(raw: string): string[] {
  const source = String(raw || "").trim();
  const candidates: string[] = [];
  const push = (value: string) => {
    const trimmed = value.trim();
    if (trimmed && !candidates.includes(trimmed)) candidates.push(trimmed);
  };
  push(source.replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/i, ""));
  const objectStart = source.indexOf("{");
  const objectEnd = source.lastIndexOf("}");
  if (objectStart >= 0 && objectEnd > objectStart) {
    push(source.slice(objectStart, objectEnd + 1));
  }
  const arrayStart = source.indexOf("[");
  const arrayEnd = source.lastIndexOf("]");
  if (arrayStart >= 0 && arrayEnd > arrayStart) {
    push(source.slice(arrayStart, arrayEnd + 1));
  }
  return candidates;
}

function tryParseJsonPayload(raw: string): unknown | null {
  for (const candidate of jsonCandidates(raw)) {
    for (const variant of [candidate, repairJsonText(candidate)]) {
      try {
        return JSON.parse(variant);
      } catch {
        // Try the next recovered/repaired candidate.
      }
    }
  }
  return null;
}

function extractJsonArraySection(raw: string, key: string): unknown[] {
  const marker = `"${key}"`;
  const markerIndex = raw.indexOf(marker);
  if (markerIndex < 0) return [];
  const arrayStart = raw.indexOf("[", markerIndex + marker.length);
  if (arrayStart < 0) return [];
  let depth = 0;
  let escaped = false;
  let inString = false;
  for (let index = arrayStart; index < raw.length; index += 1) {
    const character = raw[index];
    if (inString) {
      if (character === "\\" && !escaped) {
        escaped = true;
        continue;
      }
      if (character === '"' && !escaped) inString = false;
      escaped = false;
      continue;
    }
    if (character === '"') {
      inString = true;
      continue;
    }
    if (character === "[") depth += 1;
    if (character === "]") {
      depth -= 1;
      if (depth === 0) {
        const slice = raw.slice(arrayStart, index + 1);
        for (const variant of [slice, repairJsonText(slice)]) {
          try {
            const parsed = JSON.parse(variant);
            return Array.isArray(parsed) ? parsed : [];
          } catch {
            // Fall through to object scanning below.
          }
        }
        const objects: unknown[] = [];
        const objectPattern = /\{[^{}]*\}/g;
        for (const match of slice.matchAll(objectPattern)) {
          for (const variant of [match[0], repairJsonText(match[0])]) {
            try {
              objects.push(JSON.parse(variant));
              break;
            } catch {
              // Try the next object fragment.
            }
          }
        }
        return objects;
      }
    }
  }
  return [];
}

function extractJsonPayload(raw: string): unknown {
  const parsed = tryParseJsonPayload(raw);
  if (parsed != null) return parsed;
  const summaryMatch = /"summary"\s*:\s*"((?:\\.|[^"\\])*)"/.exec(raw);
  const annotations = extractJsonArraySection(raw, "annotations");
  if (summaryMatch || annotations.length) {
    return {
      summary: summaryMatch ? decodeLooseJsonString(summaryMatch[1]) : "",
      annotations,
    };
  }
  throw new Error("模型返回不是合法 JSON，且无法从响应中恢复 annotations");
}

function normalizeKind(value: unknown): ScriptIssueKind | null {
  const raw = String(value || "").trim();
  if (ISSUE_KINDS.has(raw as ScriptIssueKind)) return raw as ScriptIssueKind;
  return LEGACY_ISSUE_KINDS[raw] ?? null;
}

export function parseScriptQualityResponse(
  raw: string,
  entriesById: Map<number, TranscriptCue>,
): ScriptIssueAnnotation[] {
  return parseScriptQualityBundle(raw, entriesById).annotations;
}

export function parseScriptQualityBundle(
  raw: string,
  entriesById: Map<number, TranscriptCue>,
): { summary: string; annotations: ScriptIssueAnnotation[] } {
  const payload = extractJsonPayload(raw);
  const record = payload && typeof payload === "object" && !Array.isArray(payload)
    ? payload as Record<string, unknown>
    : null;
  const list = Array.isArray(payload)
    ? payload
    : Array.isArray(record?.annotations)
      ? record.annotations
      : Array.isArray(record?.issues)
        ? record.issues
        : [];
  const annotations: ScriptIssueAnnotation[] = [];
  for (const item of list) {
    if (!item || typeof item !== "object") continue;
    const row = item as Record<string, unknown>;
    const cueId = Number(row.cueId ?? row.cue_id ?? row.id);
    if (!Number.isFinite(cueId)) continue;
    const entry = entriesById.get(Math.trunc(cueId));
    if (!entry) continue;
    const kind = normalizeKind(row.kind ?? row.issue_kind ?? row.type);
    if (!kind) continue;
    const reason = String(row.reason ?? row.issue ?? "").trim();
    const suggestion = String(row.suggestion ?? row.fix ?? "").trim();
    if (!reason && !suggestion) continue;
    annotations.push({
      cueId: entry.id,
      startMs: Math.round(entry.start * 1000),
      endMs: Math.round(entry.end * 1000),
      kind,
      originalText: String(row.originalText ?? row.original_text ?? entry.text).trim() || entry.text,
      reason,
      suggestion: suggestion.startsWith("[建议稿]") ? suggestion : `[建议稿] ${suggestion}`.trim(),
    });
  }
  const summary = String(record?.summary ?? "").trim();
  return { summary, annotations };
}

export function mergeScriptQualityAnnotations(
  batches: ScriptIssueAnnotation[][],
): ScriptIssueAnnotation[] {
  const byCue = new Map<number, ScriptIssueAnnotation>();
  const priority: Record<ScriptIssueKind, number> = {
    compliance_risk: 4,
    factual_risk: 3,
    structure_gap: 2,
    unclear_expression: 1,
  };
  for (const batch of batches) {
    for (const annotation of batch) {
      const existing = byCue.get(annotation.cueId);
      if (!existing || priority[annotation.kind] > priority[existing.kind]) {
        byCue.set(annotation.cueId, annotation);
      }
    }
  }
  return [...byCue.values()].sort((left, right) => left.cueId - right.cueId);
}

export function buildScriptQualityUserMessage(chunk: ScriptQualityChunk): string {
  return [
    `话术复盘 ${chunk.index + 1}/${chunk.total}`,
    `本块 cueId 范围：${chunk.cueIds[0]}–${chunk.cueIds[chunk.cueIds.length - 1]}（共 ${chunk.cueIds.length} 句）`,
    "以下逐字稿每行以 [cue:编号] 开头。请像复盘教练一样，标出「说得不够好、可以改得更好」的句子，并给出下次可以怎么说的建议。",
    "若本块以互动/暖场为主，annotations 可返回空数组；不要为了凑数标注。",
    chunk.text,
  ].join("\n\n");
}

export function scriptQualitySystemPrompt(): string {
  return buildScriptQualityPrompt();
}

export function annotationsByCueId(
  annotations: ScriptIssueAnnotation[],
): Map<number, ScriptIssueAnnotation> {
  return new Map(annotations.map((annotation) => [annotation.cueId, annotation]));
}
