import type { ScriptIssueAnnotation } from "./scriptQuality";

export type SavedScriptQuality = {
  summary: string;
  annotations: ScriptIssueAnnotation[];
};

export function scriptQualityStorageKey(sourceKey: string): string {
  // v2: full-session review excludes deal-order ASR windows.
  return `bsr:script-quality:v2:${sourceKey}`;
}

export function parseSavedScriptQuality(raw: string | null): SavedScriptQuality | null {
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as Partial<SavedScriptQuality>;
    if (!Array.isArray(value.annotations)) return null;
    return {
      summary: typeof value.summary === "string" ? value.summary : "",
      annotations: value.annotations.filter((item): item is ScriptIssueAnnotation =>
        Boolean(item)
        && Number.isFinite(item.cueId)
        && Number.isFinite(item.startMs)
        && Number.isFinite(item.endMs)
        && typeof item.kind === "string"
        && typeof item.originalText === "string"
        && typeof item.reason === "string"
        && typeof item.suggestion === "string",
      ),
    };
  } catch {
    return null;
  }
}
