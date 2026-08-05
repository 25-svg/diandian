import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src/page/Archive.svelte", import.meta.url), "utf8");
const headerStart = source.indexOf("<thead");
const headerEnd = source.indexOf("</thead>", headerStart);
const rowStart = source.indexOf("{#each filteredArchives as archive", headerEnd);
const rowEnd = source.indexOf("{/each}", rowStart);

assert.ok(headerStart >= 0 && headerEnd > headerStart, "archive table header must exist");
assert.ok(rowStart >= 0 && rowEnd > rowStart, "archive rows must exist");

const header = source.slice(headerStart, headerEnd);
const row = source.slice(rowStart, rowEnd);

assert.ok(
  header.indexOf(">直播时间</th") < header.indexOf(">主播</th"),
  "the live-time header must appear before the anchor header",
);
assert.ok(
  row.indexOf("formatDate(archive.created_at)") < row.indexOf("archive.anchor_name || anchorStatusLabel(archive)"),
  "each row must render live time before the anchor name",
);

console.log("archive table column-order test passed");
