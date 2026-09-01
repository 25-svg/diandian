import assert from "node:assert/strict";
import {
  anchorKnowledgeCitationText,
  anchorKnowledgeStatusLabel,
  buildAnchorKnowledgeSearchRequest,
  canReviewAnchorKnowledgeAsset,
  type AnchorKnowledgeAsset,
} from "./anchorKnowledge.js";

assert.throws(
  () => buildAnchorKnowledgeSearchRequest({ requesterAnchorId: " ", scope: "private" }),
  /必须选择当前主播/,
);
assert.throws(
  () => buildAnchorKnowledgeSearchRequest({
    requesterAnchorId: "anchor-a",
    scope: "private",
    ownerAnchorId: "anchor-b",
  }),
  /不得查询其他主播/,
);
assert.deepEqual(
  buildAnchorKnowledgeSearchRequest({
    requesterAnchorId: " anchor-a ",
    scope: "team",
    ownerAnchorId: "anchor-b",
    reviewStatus: "rejected",
  }),
  {
    requesterAnchorId: "anchor-a",
    scope: "team",
    ownerAnchorId: "anchor-b",
    query: "",
    assetType: null,
    reviewStatus: "published",
  },
);

const pending = {
  reviewStatus: "pending_review",
} as AnchorKnowledgeAsset;
assert.equal(canReviewAnchorKnowledgeAsset(pending), true);
assert.equal(anchorKnowledgeStatusLabel("published"), "已发布");
assert.equal(
  anchorKnowledgeCitationText({
    sourceId: "source-1",
    sourceKind: "transcript",
    sourceLocator: "video:1",
    videoId: 1,
    startMs: 1000,
    endMs: 3000,
    transcriptVersion: "v1",
    transcriptHash: "hash",
    productFactId: "fact-1",
    productFactVersion: "2",
    analysisVersion: "",
    contentHash: "source-hash",
  }),
  "video:1 · 1000–3000 ms · 逐字稿 v1 · 商品事实 fact-1@2",
);

console.log("anchorKnowledge tests passed");
