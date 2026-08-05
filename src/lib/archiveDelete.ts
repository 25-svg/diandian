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

/** Keep batches small to avoid long IPC calls and HTTP schema limits (max 50). */
export const ARCHIVE_DELETE_BATCH_SIZE = 20;

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

export function chunkArchiveDeleteGroups(
  groups: ArchiveDeleteGroup[],
  batchSize = ARCHIVE_DELETE_BATCH_SIZE
): ArchiveDeleteGroup[] {
  const chunks: ArchiveDeleteGroup[] = [];
  for (const group of groups) {
    if (group.liveIds.length === 0) continue;
    for (let offset = 0; offset < group.liveIds.length; offset += batchSize) {
      chunks.push({
        platform: group.platform,
        roomId: group.roomId,
        liveIds: group.liveIds.slice(offset, offset + batchSize),
      });
    }
  }
  return chunks;
}

export function countArchiveDeleteTargets(
  groups: ArchiveDeleteGroup[]
): number {
  return groups.reduce((sum, group) => sum + group.liveIds.length, 0);
}
