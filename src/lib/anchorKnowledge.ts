export type AnchorKnowledgeAssetType = "speech" | "deal_clip" | "analysis_advice";
export type AnchorKnowledgeStatus = "candidate" | "pending_review" | "published" | "rejected";
export type AnchorKnowledgeScope = "private" | "team";

export interface AnchorKnowledgeProfile {
  anchorId: string;
  displayName: string;
  vaultRelativeRoot: string;
  isActive: boolean;
  candidateCount: number;
  pendingReviewCount: number;
  publishedCount: number;
}

export interface AnchorKnowledgeSource {
  sourceId: string;
  sourceKind: "video" | "transcript" | "product_fact" | "analysis" | "external";
  sourceLocator: string;
  videoId: number | null;
  startMs: number | null;
  endMs: number | null;
  transcriptVersion: string;
  transcriptHash: string;
  productFactId: string;
  productFactVersion: string;
  analysisVersion: string;
  contentHash: string;
}

export interface AnchorKnowledgeAsset {
  assetId: string;
  anchorId: string;
  anchorName: string;
  assetType: AnchorKnowledgeAssetType;
  title: string;
  body: string;
  productId: string;
  reviewStatus: AnchorKnowledgeStatus;
  version: number;
  supersedesAssetId: string | null;
  contentHash: string;
  isCurrent: boolean;
  publishedRelativePath: string;
  publishedFileHash: string;
  createdAt: string;
  updatedAt: string;
  reviewedAt: string | null;
  reviewedBy: string;
  reviewReason: string;
  publishedAt: string | null;
}

export interface AnchorKnowledgeAssetDetail {
  asset: AnchorKnowledgeAsset;
  sources: AnchorKnowledgeSource[];
}

export interface AnchorKnowledgeSearchRequest {
  requesterAnchorId: string;
  scope: AnchorKnowledgeScope;
  ownerAnchorId: string | null;
  query: string;
  assetType: AnchorKnowledgeAssetType | null;
  reviewStatus: AnchorKnowledgeStatus | null;
}

export function buildAnchorKnowledgeSearchRequest(input: {
  requesterAnchorId: string;
  scope: AnchorKnowledgeScope;
  ownerAnchorId?: string | null;
  query?: string;
  assetType?: AnchorKnowledgeAssetType | null;
  reviewStatus?: AnchorKnowledgeStatus | null;
}): AnchorKnowledgeSearchRequest {
  const requesterAnchorId = input.requesterAnchorId.trim();
  if (!requesterAnchorId) {
    throw new Error("检索必须选择当前主播");
  }
  if (input.scope === "private") {
    const requestedOwner = input.ownerAnchorId?.trim();
    if (requestedOwner && requestedOwner !== requesterAnchorId) {
      throw new Error("私有检索不得查询其他主播");
    }
    return {
      requesterAnchorId,
      scope: "private",
      ownerAnchorId: requesterAnchorId,
      query: input.query?.trim() ?? "",
      assetType: input.assetType ?? null,
      reviewStatus: input.reviewStatus ?? null,
    };
  }
  return {
    requesterAnchorId,
    scope: "team",
    ownerAnchorId: input.ownerAnchorId?.trim() || null,
    query: input.query?.trim() ?? "",
    assetType: input.assetType ?? null,
    reviewStatus: "published",
  };
}

export function anchorKnowledgeAssetTypeLabel(type: AnchorKnowledgeAssetType): string {
  return {
    speech: "主播话术",
    deal_clip: "成交切片",
    analysis_advice: "分析建议",
  }[type];
}

export function anchorKnowledgeStatusLabel(status: AnchorKnowledgeStatus): string {
  return {
    candidate: "候选",
    pending_review: "待审核",
    published: "已发布",
    rejected: "已驳回",
  }[status];
}

export function canSubmitAnchorKnowledgeAsset(asset: AnchorKnowledgeAsset): boolean {
  return asset.reviewStatus === "candidate";
}

export function canReviewAnchorKnowledgeAsset(asset: AnchorKnowledgeAsset): boolean {
  return asset.reviewStatus === "pending_review";
}

export function anchorKnowledgeCitationText(source: AnchorKnowledgeSource): string {
  const parts = [source.sourceLocator];
  if (source.startMs !== null && source.endMs !== null) {
    parts.push(`${source.startMs}–${source.endMs} ms`);
  }
  if (source.transcriptVersion) {
    parts.push(`逐字稿 ${source.transcriptVersion}`);
  }
  if (source.productFactId) {
    parts.push(`商品事实 ${source.productFactId}@${source.productFactVersion}`);
  }
  return parts.join(" · ");
}

export function friendlyAnchorKnowledgeError(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return "主播知识库操作失败，请稍后重试";
}
