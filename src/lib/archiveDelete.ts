type ArchiveDeleteTarget = {
  platform: string;
  room_id: string;
  live_id: string;
};

export type ArchiveDeleteGroup = {
  platform: string;
  roomId: string;
  liveIds: string[];
};

export function groupArchivesForDeletion(
  archives: ArchiveDeleteTarget[],
  selectedLiveIds: Set<string>
): ArchiveDeleteGroup[] {
  const groups = new Map<string, ArchiveDeleteGroup>();
  for (const archive of archives) {
    if (!selectedLiveIds.has(archive.live_id)) continue;
    const key = `${archive.platform}\u0000${archive.room_id}`;
    const group = groups.get(key) || {
      platform: archive.platform,
      roomId: archive.room_id,
      liveIds: [],
    };
    group.liveIds.push(archive.live_id);
    groups.set(key, group);
  }
  return [...groups.values()];
}
