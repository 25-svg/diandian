import assert from "node:assert/strict";
import type { WorkspaceTranscriptEntry } from "./companyAnalysisWorkspace.js";
import type { PaymentEvent } from "./orderDealTimeline.js";
import {
  buildLiveProductCatalog,
  cleanLiveProductName,
  extractLiveTopProducts,
  liveProductCatalogSignature,
} from "./liveTopProducts.js";

const entries: WorkspaceTranscriptEntry[] = [
  { id: 1, start: 0, end: 10, text: "这一支索尼 70-200 GM 二代，成色非常好" },
  { id: 2, start: 10, end: 20, text: "再看一下 70-200，拍运动特别合适" },
  { id: 3, start: 20, end: 30, text: "佳能 RF35 1.8 这支镜头很轻" },
  { id: 4, start: 30, end: 40, text: "今天给大家介绍相机和镜头" },
];

const events: PaymentEvent[] = [
  { offsetSec: 20, payAmountFen: 100, productName: "99新 Sony/索尼 FE 70-200mm F/2.8 GM 二代" },
  { offsetSec: 30, payAmountFen: 200, productName: "99新 Sony/索尼 FE 70-200mm F/2.8 GM 二代" },
  { offsetSec: 40, payAmountFen: 300, productName: "98新 Canon/佳能 RF 35mm F/1.8 Macro IS STM" },
  { offsetSec: 50, payAmountFen: 400, productName: "99新 Nikon/尼康 Z 24-70mm F/4 S" },
];

const top = extractLiveTopProducts(entries, events);
assert.equal(top.length, 2);
assert.match(top[0]?.name ?? "", /70-200/i);
assert.equal(top[0]?.mentionCount, 2);
assert.equal(top[0]?.orderCount, 2);
assert.match(top[1]?.name ?? "", /35mm/i);
assert.equal(top[1]?.mentionCount, 1);
assert.equal(top[1]?.orderCount, 1);
assert.deepEqual(extractLiveTopProducts([], events), []);
assert.deepEqual(extractLiveTopProducts(entries, []), []);
assert.equal(extractLiveTopProducts(entries, events, 1).length, 1);
assert.equal(cleanLiveProductName("99新 Sony/索尼 FE 70-200mm"), "Sony/索尼 FE 70-200mm");
const catalog = buildLiveProductCatalog(events);
assert.equal(catalog.length, 3);
assert.ok(catalog.every((product) => product.aliases.length > 0));
assert.equal(
  liveProductCatalogSignature(catalog),
  liveProductCatalogSignature([...catalog].reverse()),
);

const ambiguousCatalog = buildLiveProductCatalog([
  { offsetSec: 1, payAmountFen: 1, productName: "99新 佳能 RF 35mm F1.8" },
  { offsetSec: 2, payAmountFen: 1, productName: "98新 索尼 FE 35mm F1.8" },
]);
assert.ok(ambiguousCatalog.every((product) => !product.aliases.includes("35")));

console.log("live top products tests passed");
