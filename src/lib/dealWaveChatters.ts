import type { DealWave } from "./orderDealTimeline.js";

/** Absolute-ms danmu row (as returned by get_danmu_record with live_start=0). */
export type DanmuSpeakerEntry = {
  ts: number;
  content: string;
  user_id?: string | null;
  user_name?: string | null;
};

export type WaveChatter = {
  userName: string;
  userId?: string;
  messageCount: number;
  sampleContent: string;
};

export type WaveChatterSummary = {
  speakers: WaveChatter[];
  anonymousCount: number;
  /** Short label for the wave card. */
  label: string;
  /** Longer hint for title / empty states. */
  hint: string;
};

export const DEFAULT_CHATTER_BEFORE_SEC = 180;
export const DEFAULT_CHATTER_AFTER_SEC = 30;
export const MAX_CHATTER_LABEL_NAMES = 3;

export function parseFlexibleDateToUnixMs(value: string | null | undefined): number | null {
  const trimmed = (value ?? "").trim();
  if (!trimmed) return null;
  if (/^\d{10,13}$/.test(trimmed)) {
    const numeric = Number(trimmed);
    if (!Number.isFinite(numeric)) return null;
    return trimmed.length >= 13 ? Math.trunc(numeric) : Math.trunc(numeric) * 1000;
  }
  const normalized = trimmed.includes("T") ? trimmed : trimmed.replace(" ", "T");
  const ms = Date.parse(normalized);
  return Number.isFinite(ms) ? ms : null;
}

export function danmuOffsetSec(entryTsMs: number, liveStartUnixMs: number): number {
  return entryTsMs / 1000 - liveStartUnixMs / 1000;
}

function speakerKey(entry: DanmuSpeakerEntry): string | null {
  const name = (entry.user_name ?? "").trim();
  const id = (entry.user_id ?? "").trim();
  if (name) return `name:${name}`;
  if (id && id !== "0") return `id:${id}`;
  return null;
}

function speakerDisplayName(entry: DanmuSpeakerEntry): string {
  const name = (entry.user_name ?? "").trim();
  if (name) return name;
  const id = (entry.user_id ?? "").trim();
  return id ? `uid:${id}` : "";
}

/**
 * Collect unique chat speakers in the pre-pay window of a deal wave.
 * This is a weak对照 aid — not automatic buyer matching.
 */
export function collectWaveChatters(
  wave: Pick<DealWave, "startOffsetSec" | "endOffsetSec">,
  danmu: readonly DanmuSpeakerEntry[],
  liveStartedAt: string | null | undefined,
  beforeSec = DEFAULT_CHATTER_BEFORE_SEC,
  afterSec = DEFAULT_CHATTER_AFTER_SEC,
): WaveChatterSummary {
  if (!danmu.length) {
    return {
      speakers: [],
      anonymousCount: 0,
      label: "",
      hint: "本场暂无弹幕文件",
    };
  }

  const liveStartMs = parseFlexibleDateToUnixMs(liveStartedAt);
  if (liveStartMs == null) {
    return {
      speakers: [],
      anonymousCount: 0,
      label: "",
      hint: "缺少开播时间，无法对齐弹幕",
    };
  }

  const windowStart = Math.max(0, wave.startOffsetSec - beforeSec);
  const windowEnd = wave.endOffsetSec + afterSec;
  const byKey = new Map<string, WaveChatter>();
  let anonymousCount = 0;
  let namedHits = 0;

  for (const entry of danmu) {
    if (!Number.isFinite(entry.ts)) continue;
    const offsetSec = danmuOffsetSec(entry.ts, liveStartMs);
    if (offsetSec < windowStart || offsetSec > windowEnd) continue;
    const key = speakerKey(entry);
    if (!key) {
      anonymousCount += 1;
      continue;
    }
    namedHits += 1;
    const existing = byKey.get(key);
    if (existing) {
      existing.messageCount += 1;
      if (!existing.sampleContent && entry.content.trim()) {
        existing.sampleContent = entry.content.trim();
      }
      continue;
    }
    byKey.set(key, {
      userName: speakerDisplayName(entry),
      userId: (entry.user_id ?? "").trim() || undefined,
      messageCount: 1,
      sampleContent: entry.content.trim(),
    });
  }

  const speakers = [...byKey.values()].sort((left, right) => {
    if (right.messageCount !== left.messageCount) {
      return right.messageCount - left.messageCount;
    }
    return left.userName.localeCompare(right.userName, "zh");
  });

  if (speakers.length === 0 && namedHits === 0 && anonymousCount === 0) {
    return {
      speakers: [],
      anonymousCount: 0,
      label: "",
      hint: "付款前窗口内无弹幕",
    };
  }

  if (speakers.length === 0) {
    return {
      speakers: [],
      anonymousCount,
      label: "",
      hint: "弹幕无昵称（需新录场次）",
    };
  }

  const names = speakers.map((speaker) => speaker.userName);
  const label = names.length <= MAX_CHATTER_LABEL_NAMES
    ? names.join("、")
    : `${names.slice(0, MAX_CHATTER_LABEL_NAMES).join("、")} 等 ${names.length} 人`;

  return {
    speakers,
    anonymousCount,
    label,
    hint: `付款前约 ${beforeSec / 60} 分钟互动人（弱对照，非自动匹配）`,
  };
}

export function summarizeWaveChattersMap(
  waves: readonly DealWave[],
  danmu: readonly DanmuSpeakerEntry[],
  liveStartedAt: string | null | undefined,
): Map<string, WaveChatterSummary> {
  const map = new Map<string, WaveChatterSummary>();
  for (const wave of waves) {
    map.set(wave.id, collectWaveChatters(wave, danmu, liveStartedAt));
  }
  return map;
}
