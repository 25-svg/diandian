export type PaymentEvent = {
  offsetSec: number;
  payAmountFen: number | null;
  productName?: string;
  orderId?: string;
};

export type PaymentEventsSummary = {
  eventCount: number;
  totalPayAmountYuan: number;
  liveStartedAt?: string;
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
    orderId: readString(record.order_id ?? record.orderId) || undefined,
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

/** Stable list row key: orderId alone is not unique in real order.searchList exports. */
export function paymentEventRowKey(event: PaymentEvent, index: number): string {
  const orderId = event.orderId?.trim();
  return orderId ? `${orderId}#${index}` : `${event.offsetSec}#${index}`;
}
