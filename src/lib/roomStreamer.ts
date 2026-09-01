export const ROOM_STREAMER_MAX_LENGTH = 12;

const INVALID_STREAMER_NAME = /[{},，。:：\n\r\[\]]/u;

export function normalizeRoomStreamerName(value: string): string {
  return value
    .trim()
    .replace(/^主播\s*/u, "")
    .replace(/^[:：]\s*/u, "")
    .trim();
}

export function validateRoomStreamerName(value: string): string {
  const normalized = normalizeRoomStreamerName(value);
  const length = Array.from(normalized).length;
  if (
    length < 1 ||
    length > ROOM_STREAMER_MAX_LENGTH ||
    INVALID_STREAMER_NAME.test(normalized)
  ) {
    return "主播姓名应为 1 至 12 个字符，不能包含标点或换行";
  }
  return "";
}

export function roomStreamerLabel(name: string): string {
  const normalized = normalizeRoomStreamerName(name);
  return normalized || "待指定";
}
