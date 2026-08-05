export type TimedText = {
  start: number;
  end: number;
  text: string;
};

export type TranscriptResultSignals = {
  inquiryCount: number;
  linkCount: number;
  conversionConfirmationCount: number;
  transitionCount: number;
  label: string;
};

export type PlaybackMetrics = {
  timestamp: number;
  onlineUsers?: number;
  comments?: number;
  productClicks?: number;
  addToCart?: number;
  paidOrders?: number;
};

const SIGNAL_TERMS = {
  inquiry: ["多少钱", "什么价", "价格", "到手", "预算", "怎么卖"],
  link: ["链接", "小黄车", "几号车", "几号链接", "置顶", "上车"],
  conversion: ["已经拍", "已拍", "下单了", "付款了", "成交", "给你备注", "锁单"],
  transition: ["下一个", "再看一款", "换一个", "转下一款", "再看看"],
} as const;

function countTerms(text: string, terms: readonly string[]): number {
  return terms.reduce((total, term) => total + (text.split(term).length - 1), 0);
}

export function analyzeTranscriptSignals(
  entries: readonly TimedText[],
  start: number,
  end: number,
): TranscriptResultSignals {
  const text = entries
    .filter((entry) => entry.end >= start && entry.start <= end)
    .map((entry) => entry.text)
    .join(" ");
  const inquiryCount = countTerms(text, SIGNAL_TERMS.inquiry);
  const linkCount = countTerms(text, SIGNAL_TERMS.link);
  const conversionConfirmationCount = countTerms(text, SIGNAL_TERMS.conversion);
  const transitionCount = countTerms(text, SIGNAL_TERMS.transition);
  const parts = [
    inquiryCount ? `问价×${inquiryCount}` : "",
    linkCount ? `链接×${linkCount}` : "",
    conversionConfirmationCount ? `成交确认×${conversionConfirmationCount}` : "无成交确认",
    transitionCount ? `转品×${transitionCount}` : "",
  ].filter(Boolean);
  return {
    inquiryCount,
    linkCount,
    conversionConfirmationCount,
    transitionCount,
    label: parts.join(" / "),
  };
}
