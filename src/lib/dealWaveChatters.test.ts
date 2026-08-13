import assert from "node:assert/strict";
import {
  collectWaveChatters,
  danmuOffsetSec,
  parseFlexibleDateToUnixMs,
  summarizeWaveChattersMap,
} from "./dealWaveChatters.js";
import { buildDealWaves } from "./orderDealTimeline.js";

assert.equal(parseFlexibleDateToUnixMs("2026-07-28 08:15:49"), Date.parse("2026-07-28T08:15:49"));
assert.equal(parseFlexibleDateToUnixMs("1710000000"), 1710000000_000);
assert.equal(danmuOffsetSec(1_000_000, 0), 1000);

const liveStartedAt = "2026-07-28T08:00:00+08:00";
const liveStartMs = parseFlexibleDateToUnixMs(liveStartedAt)!;
const payOffsetSec = 600;
const payTs = liveStartMs + payOffsetSec * 1000;

const waves = buildDealWaves([
  { offsetSec: payOffsetSec, payAmountFen: 10000, productName: "索尼 A7M4", buyerLabel: "张*" },
]);
assert.equal(waves.length, 1);

const summary = collectWaveChatters(
  waves[0]!,
  [
    { ts: payTs - 120_000, content: "多少钱", user_name: "小明", user_id: "1" },
    { ts: payTs - 90_000, content: "还有吗", user_name: "小明", user_id: "1" },
    { ts: payTs - 60_000, content: "求链接", user_name: "小红", user_id: "2" },
    { ts: payTs - 30_000, content: "匿名提问" },
    { ts: payTs - 400_000, content: "窗外", user_name: "路人", user_id: "9" },
  ],
  liveStartedAt,
);

assert.equal(summary.speakers.length, 2);
assert.equal(summary.speakers[0]?.userName, "小明");
assert.equal(summary.speakers[0]?.messageCount, 2);
assert.equal(summary.anonymousCount, 1);
assert.match(summary.label, /小明/);
assert.match(summary.label, /小红/);

const noNick = collectWaveChatters(
  waves[0]!,
  [{ ts: payTs - 10_000, content: "只有内容" }],
  liveStartedAt,
);
assert.equal(noNick.speakers.length, 0);
assert.match(noNick.hint, /无昵称/);

const mapped = summarizeWaveChattersMap(waves, [
  { ts: payTs - 20_000, content: "在", user_name: "甲" },
], liveStartedAt);
assert.equal(mapped.get(waves[0]!.id)?.label, "甲");

console.log("dealWaveChatters.test.ts passed");
