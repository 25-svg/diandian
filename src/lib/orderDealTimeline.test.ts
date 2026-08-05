import assert from "node:assert/strict";
import {
  analyzeSegmentDealSignals,
  buildDealMinuteBuckets,
  buildPeakDealMinuteLabel,
  dealReviewSeekOffset,
  parsePaymentEventsPayload,
  paymentEventRowKey,
} from "./orderDealTimeline.js";

const payload = parsePaymentEventsPayload({
  summary: {
    event_count: 4,
    total_pay_amount_yuan: 23655.2,
    live_started_at: "2026-07-28 08:15:49",
  },
  events: [
    { offset_sec: 45, pay_amount_fen: 320000, product_name: "佳能 R7" },
    { offset_sec: 75, pay_amount_fen: 711500, product_name: "佳能 R62" },
    { offset_sec: 135, pay_amount_fen: 580000, product_name: "尼康 Z5" },
    { offset_sec: 140, pay_amount_fen: 450000, product_name: "索尼 A7C" },
  ],
});

assert.ok(payload);
assert.equal(payload!.events.length, 4);
assert.equal(payload!.summary?.eventCount, 4);

const buckets = buildDealMinuteBuckets(payload!.events);
assert.equal(buckets.length, 3);
assert.equal(buckets[0]?.orderCount, 1);
assert.equal(buckets[0]?.totalPayAmountFen, 320000);
assert.equal(buckets[2]?.orderCount, 2);

const segment = analyzeSegmentDealSignals(payload!.events, 58, 120, 120);
assert.equal(segment.inSegmentOrderCount, 1);
assert.equal(segment.afterWindowOrderCount, 2);
assert.match(segment.label, /段内 1 单/);
assert.match(segment.label, /段后2分钟 2 单/);

const hotSegment = analyzeSegmentDealSignals(payload!.events, 0, 58, 120);
assert.equal(hotSegment.inSegmentOrderCount, 1);
assert.equal(hotSegment.afterWindowOrderCount, 3);
assert.match(hotSegment.label, /段内 1 单/);
assert.match(hotSegment.label, /段后2分钟 3 单/);

const peak = buildPeakDealMinuteLabel(buckets);
assert.match(peak || "", /2分 成交 2 单/);

const empty = analyzeSegmentDealSignals([], 10, 20);
assert.match(empty.label, /段内 0 单/);

assert.equal(dealReviewSeekOffset(75), 0);
assert.equal(dealReviewSeekOffset(180), 60);

const duplicateOrderEvents = [
  { offsetSec: 10, payAmountFen: 100, orderId: "same-id" },
  { offsetSec: 20, payAmountFen: 200, orderId: "same-id" },
];
assert.notEqual(
  paymentEventRowKey(duplicateOrderEvents[0], 0),
  paymentEventRowKey(duplicateOrderEvents[1], 1),
);

console.log("orderDealTimeline tests passed");
