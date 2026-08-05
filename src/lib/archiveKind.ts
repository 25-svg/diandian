export type ArchiveKind = "company" | "competitor";
export type ArchiveClassificationSource = "auto_rule" | "manual";

export function classifyArchiveKind(input: {
  title?: string | null;
  anchorName?: string | null;
  accountName?: string | null;
}): {
  archiveKind: ArchiveKind;
  classificationSource: ArchiveClassificationSource;
} {
  const text = `${input.title || ""} ${input.anchorName || ""} ${input.accountName || ""}`;
  return {
    archiveKind: text.includes("金典拍拍") ? "company" : "competitor",
    classificationSource: "auto_rule",
  };
}

/** Keep locally loaded records if optional server-side reclassification fails. */
export function preferAutoClassifiedArchives<T>(existing: T[], updated: T[] | null): T[] {
  return updated && updated.length === existing.length ? updated : existing;
}

export function applyLoadedArchiveClassification<T extends {
  archive_kind: ArchiveKind;
  classification_source: ArchiveClassificationSource;
  title: string;
  anchor_name: string;
}>(archive: T, accountName: string): T {
  if (archive.classification_source === "manual") return archive;
  return {
    ...archive,
    archive_kind: classifyArchiveKind({ title: archive.title, anchorName: archive.anchor_name, accountName }).archiveKind,
  };
}
