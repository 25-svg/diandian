import assert from "node:assert/strict";
import type { WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";
import {
  extractLiveHighFrequencyHits,
  filterHighFrequencyHits,
  paginateHighFrequencyHits,
  summarizeHighFrequencyCategories,
} from "./liveHighFrequencyWords.js";

const entries: WorkspaceTranscriptEntry[] = [
  { id: 1, start: 10, end: 20, text: "怎么选哪个比较好" },
  { id: 2, start: 30, end: 40, text: "小黄车置顶上链接，拍下就给你备注" },
  { id: 3, start: 50, end: 60, text: "这台99新成色过得去，今天发货包邮" },
  { id: 4, start: 70, end: 80, text: "没关注的点个关注" },
];

const hits = extractLiveHighFrequencyHits(entries);
assert.ok(hits.length >= 6);
assert.ok(hits.some((hit) => hit.word === "小黄车" && hit.category === "促单/线索"));
assert.ok(hits.some((hit) => hit.word === "99新" && hit.category === "塑品"));
assert.ok(hits.some((hit) => hit.word === "点个关注" && hit.category === "关注力"));
assert.equal(hits.find((hit) => hit.word === "上链接")?.sampleStartSec, 30);

const summaries = summarizeHighFrequencyCategories(hits);
assert.equal(summaries[0]?.category, "全部");
assert.equal(summaries[0]?.count, hits.length);
assert.ok(summaries.some((item) => item.category === "促单/线索" && item.count > 0));

const onlyAfterSales = filterHighFrequencyHits(hits, "售后");
assert.ok(onlyAfterSales.every((hit) => hit.category === "售后"));
assert.ok(onlyAfterSales.some((hit) => hit.word === "包邮" || hit.word === "发货"));

const page = paginateHighFrequencyHits(hits, 1, 3);
assert.equal(page.pageItems.length, Math.min(3, hits.length));
assert.equal(page.page, 1);
assert.ok(page.pageCount >= 1);

assert.deepEqual(extractLiveHighFrequencyHits([]), []);

console.log("live high frequency words tests passed");
