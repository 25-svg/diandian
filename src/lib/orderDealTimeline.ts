export type PaymentEvent = {
  offsetSec: number;
  payAmountFen: number | null;
  productName?: string;
  productId?: string;
  orderId?: string;
  orderStatus?: string;
  /** Masked receiver / nick for streamer review, e.g. "张*". */
  buyerLabel?: string;
};

export type PaymentEventsSummary = {
  eventCount: number;
  totalPayAmountYuan: number;
  liveStartedAt?: string;
  shopId?: string;
  shopName?: string;
  expectedEventCount?: number;
  expectedPayAmountYuan?: number;
  candidateEventCount?: number;
  candidateTotalPayAmountYuan?: number;
  currentValidEventCount?: number;
  currentValidAmountYuan?: number;
  closedOrRefundedEventCount?: number;
  closedOrRefundedAmountYuan?: number;
  attributedClosedOrRefundedEventCount?: number;
  attributedClosedOrRefundedAmountYuan?: number;
  unmatchedEventCount?: number;
  unmatchedAmountYuan?: number;
  confidence?: string;
};

export type PaymentEventsBundle = {
  events: PaymentEvent[];
  summary?: PaymentEventsSummary;
  sourceLabel?: string;
};

export type DealMinuteBucket = {
  minuteIndex: number;
  offsetSec: number;
  orderCount: number;
  totalPayAmountFen: number;
};

export type DealWave = {
  id: string;
  startOffsetSec: number;
  endOffsetSec: number;
  anchorOffsetSec: number;
  eventCount: number;
  totalPayAmountFen: number;
  productName: string;
  /** One or more masked buyer labels for this wave. */
  buyerLabel: string;
  events: PaymentEvent[];
};

export type SegmentDealSignals = {
  inSegmentOrderCount: number;
  inSegmentPayAmountFen: number;
  afterWindowOrderCount: number;
  afterWindowPayAmountFen: number;
  afterWindowSec: number;
  label: string;
};

export const DEFAULT_DEAL_AFTER_WINDOW_SEC = 120;

export function dealReviewSeekOffset(offsetSec: number): number {
  return Math.max(0, Math.floor(offsetSec) - 120);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function readNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function readString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

export function formatDealMoneyYuan(fen: number): string {
  return `¥${(fen / 100).toLocaleString("zh-CN", { maximumFractionDigits: 2 })}`;
}

function normalizePaymentEvent(raw: unknown): PaymentEvent | null {
  const record = asRecord(raw);
  if (!record) return null;
  const offsetSec = readNumber(record.offset_sec ?? record.offsetSec);
  if (offsetSec == null) return null;
  const payAmountFen = readNumber(record.pay_amount_fen ?? record.payAmountFen);
  return {
    offsetSec,
    payAmountFen: payAmountFen == null ? null : Math.max(0, Math.trunc(payAmountFen)),
    productName: readString(record.product_name ?? record.productName) || undefined,
    productId: readString(record.product_id ?? record.productId) || undefined,
    orderId: readString(record.order_id ?? record.orderId) || undefined,
    orderStatus: readString(record.order_status ?? record.orderStatus) || undefined,
    buyerLabel: readString(
      record.buyer_label
        ?? record.buyerLabel
        ?? record.mask_post_receiver
        ?? record.maskPostReceiver
        ?? record.user_nick_name
        ?? record.userNickName,
    ) || undefined,
  };
}

export function parsePaymentEventsPayload(raw: unknown): PaymentEventsBundle | null {
  if (Array.isArray(raw)) {
    const events = raw
      .map(normalizePaymentEvent)
      .filter((event): event is PaymentEvent => Boolean(event))
      .sort((left, right) => left.offsetSec - right.offsetSec);
    return { events, summary: summarizePaymentEvents(events) };
  }

  const record = asRecord(raw);
  if (!record) return null;

  const eventList = Array.isArray(record.events) ? record.events : [];
  const events = eventList
    .map(normalizePaymentEvent)
    .filter((event): event is PaymentEvent => Boolean(event))
    .sort((left, right) => left.offsetSec - right.offsetSec);

  const summaryRecord = asRecord(record.summary);
  const summary = summaryRecord
    ? {
        eventCount: readNumber(summaryRecord.event_count ?? summaryRecord.eventCount) ?? events.length,
        totalPayAmountYuan: readNumber(
          summaryRecord.total_pay_amount_yuan ?? summaryRecord.totalPayAmountYuan,
        ) ?? roundYuan(events.reduce((total, event) => total + (event.payAmountFen ?? 0), 0)),
        liveStartedAt: readString(summaryRecord.live_started_at ?? summaryRecord.liveStartedAt) || undefined,
        shopId: readString(summaryRecord.shop_id ?? summaryRecord.shopId) || undefined,
        shopName: readString(summaryRecord.shop_name ?? summaryRecord.shopName) || undefined,
        expectedEventCount: readNumber(
          summaryRecord.expected_event_count ?? summaryRecord.expectedEventCount,
        ) ?? undefined,
        expectedPayAmountYuan: readNumber(
          summaryRecord.expected_pay_amount_yuan ?? summaryRecord.expectedPayAmountYuan,
        ) ?? undefined,
        candidateEventCount: readNumber(
          summaryRecord.candidate_event_count ?? summaryRecord.candidateEventCount,
        ) ?? undefined,
        candidateTotalPayAmountYuan: readNumber(
          summaryRecord.candidate_total_pay_amount_yuan ?? summaryRecord.candidateTotalPayAmountYuan,
        ) ?? undefined,
        currentValidEventCount: readNumber(
          summaryRecord.current_valid_event_count ?? summaryRecord.currentValidEventCount,
        ) ?? undefined,
        currentValidAmountYuan: readNumber(
          summaryRecord.current_valid_amount_yuan ?? summaryRecord.currentValidAmountYuan,
        ) ?? undefined,
        closedOrRefundedEventCount: readNumber(
          summaryRecord.closed_or_refunded_event_count ?? summaryRecord.closedOrRefundedEventCount,
        ) ?? undefined,
        closedOrRefundedAmountYuan: readNumber(
          summaryRecord.closed_or_refunded_amount_yuan ?? summaryRecord.closedOrRefundedAmountYuan,
        ) ?? undefined,
        attributedClosedOrRefundedEventCount: readNumber(
          summaryRecord.attributed_closed_or_refunded_event_count
            ?? summaryRecord.attributedClosedOrRefundedEventCount,
        ) ?? undefined,
        attributedClosedOrRefundedAmountYuan: readNumber(
          summaryRecord.attributed_closed_or_refunded_amount_yuan
            ?? summaryRecord.attributedClosedOrRefundedAmountYuan,
        ) ?? undefined,
        unmatchedEventCount: readNumber(
          summaryRecord.unmatched_event_count ?? summaryRecord.unmatchedEventCount,
        ) ?? undefined,
        unmatchedAmountYuan: readNumber(
          summaryRecord.unmatched_amount_yuan ?? summaryRecord.unmatchedAmountYuan,
        ) ?? undefined,
        confidence: readString(summaryRecord.confidence) || undefined,
      }
    : summarizePaymentEvents(events);

  return {
    events,
    summary,
    sourceLabel: readString(record.source_label ?? record.sourceLabel) || undefined,
  };
}

function roundYuan(fen: number): number {
  return Math.round((fen / 100) * 100) / 100;
}

export function summarizePaymentEvents(events: readonly PaymentEvent[]): PaymentEventsSummary {
  const totalFen = events.reduce((total, event) => total + (event.payAmountFen ?? 0), 0);
  return {
    eventCount: events.length,
    totalPayAmountYuan: roundYuan(totalFen),
  };
}

export function buildDealMinuteBuckets(events: readonly PaymentEvent[]): DealMinuteBucket[] {
  const buckets = new Map<number, DealMinuteBucket>();
  for (const event of events) {
    if (event.offsetSec < 0) continue;
    const minuteIndex = Math.floor(event.offsetSec / 60);
    const existing = buckets.get(minuteIndex) ?? {
      minuteIndex,
      offsetSec: minuteIndex * 60,
      orderCount: 0,
      totalPayAmountFen: 0,
    };
    existing.orderCount += 1;
    existing.totalPayAmountFen += event.payAmountFen ?? 0;
    buckets.set(minuteIndex, existing);
  }
  return [...buckets.values()].sort((left, right) => left.minuteIndex - right.minuteIndex);
}

function normalizedProductName(event: PaymentEvent): string {
  return (event.productName || "").replace(/\s+/g, "").toLowerCase();
}

/**
 * Turn scattered SKU payment rows into beginner-friendly deal waves.
 * Nearby payments belong together; repeat purchases of the same product may
 * span a longer explanation window.
 */
export function buildDealWaves(
  events: readonly PaymentEvent[],
  nearbyGapSec = 120,
  sameProductGapSec = 600,
): DealWave[] {
  const sorted = [...events].sort((left, right) => left.offsetSec - right.offsetSec);
  const groups: PaymentEvent[][] = [];
  for (const event of sorted) {
    const current = groups[groups.length - 1];
    const previous = current?.[current.length - 1];
    if (!current || !previous) {
      groups.push([event]);
      continue;
    }
    const gap = event.offsetSec - previous.offsetSec;
    const sameProduct = Boolean(
      normalizedProductName(event)
        && normalizedProductName(event) === normalizedProductName(previous),
    );
    if (gap <= nearbyGapSec || (sameProduct && gap <= sameProductGapSec)) {
      current.push(event);
    } else {
      groups.push([event]);
    }
  }

  return groups.map((group, index) => {
    const productCounts = new Map<string, { label: string; count: number }>();
    for (const event of group) {
      const label = event.productName?.trim() || "未命名商品";
      const key = label.replace(/\s+/g, "").toLowerCase();
      const current = productCounts.get(key) ?? { label, count: 0 };
      current.count += 1;
      productCounts.set(key, current);
    }
    const products = [...productCounts.values()].sort((left, right) => right.count - left.count);
    const productName = products.length > 1
      ? `${products[0]?.label || "多个商品"} 等 ${products.length} 个商品`
      : products[0]?.label || "未命名商品";
    const buyerLabels: string[] = [];
    const seenBuyers = new Set<string>();
    for (const event of group) {
      const label = event.buyerLabel?.trim();
      if (!label || seenBuyers.has(label)) continue;
      seenBuyers.add(label);
      buyerLabels.push(label);
    }
    const buyerLabel = buyerLabels.length === 0
      ? ""
      : buyerLabels.length === 1
        ? buyerLabels[0]
        : `${buyerLabels[0]} 等 ${buyerLabels.length} 人`;
    const startOffsetSec = group[0]?.offsetSec ?? 0;
    const endOffsetSec = group[group.length - 1]?.offsetSec ?? startOffsetSec;
    return {
      id: `${Math.round(startOffsetSec)}:${index}`,
      startOffsetSec,
      endOffsetSec,
      anchorOffsetSec: startOffsetSec,
      eventCount: group.length,
      totalPayAmountFen: group.reduce((total, event) => total + (event.payAmountFen ?? 0), 0),
      productName,
      buyerLabel,
      events: group,
    };
  });
}

function eventsInWindow(
  events: readonly PaymentEvent[],
  startSec: number,
  endSec: number,
): PaymentEvent[] {
  return events.filter((event) => event.offsetSec >= startSec && event.offsetSec <= endSec);
}

function sumPayAmountFen(events: readonly PaymentEvent[]): number {
  return events.reduce((total, event) => total + (event.payAmountFen ?? 0), 0);
}

function formatOrderPart(count: number, fen: number): string {
  if (count <= 0) return "0 单";
  return fen > 0 ? `${count} 单 · ${formatDealMoneyYuan(fen)}` : `${count} 单`;
}

export function analyzeSegmentDealSignals(
  events: readonly PaymentEvent[],
  start: number,
  end: number,
  afterWindowSec = DEFAULT_DEAL_AFTER_WINDOW_SEC,
): SegmentDealSignals {
  const inSegment = eventsInWindow(events, start, end);
  const afterSegment = eventsInWindow(events, end, end + afterWindowSec);
  const inSegmentOrderCount = inSegment.length;
  const inSegmentPayAmountFen = sumPayAmountFen(inSegment);
  const afterWindowOrderCount = afterSegment.length;
  const afterWindowPayAmountFen = sumPayAmountFen(afterSegment);
  const afterMinutes = Math.max(1, Math.round(afterWindowSec / 60));

  const parts = [
    `段内 ${formatOrderPart(inSegmentOrderCount, inSegmentPayAmountFen)}`,
    `段后${afterMinutes}分钟 ${formatOrderPart(afterWindowOrderCount, afterWindowPayAmountFen)}`,
  ];

  return {
    inSegmentOrderCount,
    inSegmentPayAmountFen,
    afterWindowOrderCount,
    afterWindowPayAmountFen,
    afterWindowSec,
    label: parts.join(" / "),
  };
}

export function buildPeakDealMinuteLabel(buckets: readonly DealMinuteBucket[]): string | null {
  if (!buckets.length) return null;
  const peak = [...buckets].sort((left, right) => {
    if (right.orderCount !== left.orderCount) return right.orderCount - left.orderCount;
    return right.totalPayAmountFen - left.totalPayAmountFen;
  })[0];
  if (!peak || peak.orderCount <= 0) return null;
  const minuteLabel = `${Math.floor(peak.offsetSec / 60)}分`;
  return peak.totalPayAmountFen > 0
    ? `${minuteLabel} 成交 ${peak.orderCount} 单 · ${formatDealMoneyYuan(peak.totalPayAmountFen)}`
    : `${minuteLabel} 成交 ${peak.orderCount} 单`;
}

export function paymentEventsStorageKey(sourceKey: string): string {
  return `bsr:payment-events:v1:${sourceKey}`;
}

export function paymentEventsHistoryStorageKey(sourceKey: string): string {
  return `bsr:payment-events-history:v1:${sourceKey}`;
}

/** Stable list row key: orderId alone is not unique in real order.searchList exports. */
export function paymentEventRowKey(event: PaymentEvent, index: number): string {
  const orderId = event.orderId?.trim();
  return orderId ? `${orderId}#${index}` : `${event.offsetSec}#${index}`;
}
