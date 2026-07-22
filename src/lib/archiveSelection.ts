export function selectArchiveRange(
  orderedIds: string[],
  current: Set<string>,
  targetId: string,
  anchorId: string | null,
  additive: boolean
): Set<string> {
  const targetIndex = orderedIds.indexOf(targetId);
  const anchorIndex = anchorId ? orderedIds.indexOf(anchorId) : -1;

  if (targetIndex < 0 || anchorIndex < 0) {
    if (additive) {
      const next = new Set(current);
      next.has(targetId) ? next.delete(targetId) : next.add(targetId);
      return next;
    }
    if (current.size === 1 && current.has(targetId)) return new Set();
    return new Set([targetId]);
  }

  const next = additive ? new Set(current) : new Set<string>();
  const start = Math.min(targetIndex, anchorIndex);
  const end = Math.max(targetIndex, anchorIndex);
  orderedIds.slice(start, end + 1).forEach((id) => next.add(id));
  return next;
}

export function pruneArchiveSelection(
  current: Set<string>,
  visibleIds: string[]
): Set<string> {
  const visible = new Set(visibleIds);
  return new Set([...current].filter((id) => visible.has(id)));
}
