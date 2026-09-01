import { PENDING_STREAMER_KEY, normalizeStreamerKey } from "./streamerProfile.js";

export const STREAMER_VIRTUAL_AVATAR_STORAGE_KEY = "bsr:streamer-virtual-avatar:v1";

export type StreamerAvatarStyle = "pixel" | "virtual";

export type StreamerVirtualAvatarId =
  | "pixel-nova"
  | "pixel-mint"
  | "pixel-ember"
  | "pixel-ink"
  | "virtual-luna"
  | "virtual-coral"
  | "virtual-azure"
  | "virtual-violet";

export type StreamerVirtualAvatar = {
  id: StreamerVirtualAvatarId;
  name: string;
  style: StreamerAvatarStyle;
  background: string;
  primary: string;
  secondary: string;
  skin: string;
};

export type StreamerVirtualAvatarSelection = {
  streamerKey: string;
  avatarId: StreamerVirtualAvatarId;
  updatedAt: number;
};

export const STREAMER_VIRTUAL_AVATARS: readonly StreamerVirtualAvatar[] = [
  { id: "pixel-nova", name: "像素·星旅者", style: "pixel", background: "#e8f0ff", primary: "#3d5afe", secondary: "#1c2f80", skin: "#ffd2b2" },
  { id: "pixel-mint", name: "像素·薄荷", style: "pixel", background: "#e5f8f0", primary: "#0f9f6e", secondary: "#07573e", skin: "#ffd6b8" },
  { id: "pixel-ember", name: "像素·赤焰", style: "pixel", background: "#fff0e5", primary: "#e6632b", secondary: "#93340d", skin: "#f4c29f" },
  { id: "pixel-ink", name: "像素·墨影", style: "pixel", background: "#f0ecff", primary: "#7450d6", secondary: "#39226f", skin: "#f6cbb3" },
  { id: "virtual-luna", name: "虚拟·月白", style: "virtual", background: "#edf3ff", primary: "#3267d6", secondary: "#102a66", skin: "#ffd4bc" },
  { id: "virtual-coral", name: "虚拟·珊瑚", style: "virtual", background: "#fff0f2", primary: "#d94c79", secondary: "#7b1538", skin: "#f6c9ad" },
  { id: "virtual-azure", name: "虚拟·青空", style: "virtual", background: "#e6f8fb", primary: "#1686ad", secondary: "#07506b", skin: "#f7cfb5" },
  { id: "virtual-violet", name: "虚拟·紫藤", style: "virtual", background: "#f5ecff", primary: "#9860d4", secondary: "#502281", skin: "#f5c8ae" },
] as const;

const avatarIds = new Set<string>(STREAMER_VIRTUAL_AVATARS.map((avatar) => avatar.id));

export function getStreamerVirtualAvatar(
  avatarId: string | null | undefined,
): StreamerVirtualAvatar {
  return STREAMER_VIRTUAL_AVATARS.find((avatar) => avatar.id === avatarId)
    || STREAMER_VIRTUAL_AVATARS[0];
}

export function parseStreamerVirtualAvatarSelections(
  value: unknown,
): StreamerVirtualAvatarSelection[] {
  if (!Array.isArray(value)) return [];
  const byStreamer = new Map<string, StreamerVirtualAvatarSelection>();
  value.forEach((item) => {
    if (!item || typeof item !== "object") return;
    const candidate = item as Partial<StreamerVirtualAvatarSelection>;
    const streamerKey = normalizeStreamerKey(candidate.streamerKey || "");
    const avatarId = String(candidate.avatarId || "");
    const updatedAt = Number(candidate.updatedAt);
    if (
      !streamerKey
      || streamerKey === PENDING_STREAMER_KEY
      || !avatarIds.has(avatarId)
      || !Number.isFinite(updatedAt)
    ) return;
    const selection: StreamerVirtualAvatarSelection = {
      streamerKey,
      avatarId: avatarId as StreamerVirtualAvatarId,
      updatedAt,
    };
    const previous = byStreamer.get(streamerKey);
    if (!previous || selection.updatedAt >= previous.updatedAt) byStreamer.set(streamerKey, selection);
  });
  return [...byStreamer.values()].sort((left, right) => left.streamerKey.localeCompare(right.streamerKey, "zh-CN"));
}

export function avatarSelectionForStreamer(
  selections: readonly StreamerVirtualAvatarSelection[],
  streamerNameOrKey: string,
  fallbackIndex = 0,
): StreamerVirtualAvatarSelection {
  const streamerKey = normalizeStreamerKey(streamerNameOrKey);
  const stored = selections.find((selection) => selection.streamerKey === streamerKey);
  if (stored) return stored;
  const index = Math.abs(Math.floor(Number(fallbackIndex) || 0)) % STREAMER_VIRTUAL_AVATARS.length;
  return {
    streamerKey,
    avatarId: STREAMER_VIRTUAL_AVATARS[index].id,
    updatedAt: 0,
  };
}

export function upsertStreamerVirtualAvatarSelection(
  selections: readonly StreamerVirtualAvatarSelection[],
  streamerNameOrKey: string,
  avatarId: string,
  updatedAt = Date.now(),
): StreamerVirtualAvatarSelection[] {
  const streamerKey = normalizeStreamerKey(streamerNameOrKey);
  if (!streamerKey || streamerKey === PENDING_STREAMER_KEY) {
    throw new Error("待确认主播不能选择虚拟角色");
  }
  if (!avatarIds.has(avatarId)) throw new Error("未知的虚拟角色");
  const next = parseStreamerVirtualAvatarSelections(selections)
    .filter((selection) => selection.streamerKey !== streamerKey);
  next.push({
    streamerKey,
    avatarId: avatarId as StreamerVirtualAvatarId,
    updatedAt: Number.isFinite(updatedAt) ? updatedAt : Date.now(),
  });
  return next.sort((left, right) => left.streamerKey.localeCompare(right.streamerKey, "zh-CN"));
}
