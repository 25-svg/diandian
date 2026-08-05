#!/usr/bin/env python3
"""Compare payment events / orders against XLSX session KPI."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load_events(path: Path) -> tuple[dict, list[dict]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if "events" in payload:
        return payload.get("summary", {}), payload["events"]
    # raw order.searchList
    events = []
    for shop in payload.get("shops", []):
        for page in shop.get("pages", []):
            inner = page.get("payload") or page
            for order in inner.get("shop_order_list", []):
                for sku in order.get("sku_order_list") or []:
                    pt = sku.get("pay_time")
                    if not pt:
                        continue
                    amt = sku.get("pay_amount") or (
                        (sku.get("actual_receive_amount_info") or {}).get("actual_receive_amount")
                    )
                    events.append(
                        {
                            "pay_time_local": pt,
                            "pay_amount_yuan": round(int(amt or 0) / 100, 2),
                            "product_name": sku.get("product_name", ""),
                        }
                    )
    total = round(sum(e["pay_amount_yuan"] for e in events), 2)
    return {"event_count": len(events), "total_pay_amount_yuan": total}, events


def main() -> int:
    parser = argparse.ArgumentParser(description="Reconcile orders/events vs XLSX KPI")
    parser.add_argument("--input", type=Path, required=True, help="payment-events.json or order JSON")
    parser.add_argument("--xlsx-payment-yuan", type=float, required=True)
    parser.add_argument("--xlsx-deal-items", type=int, default=None)
    args = parser.parse_args()

    summary, events = load_events(args.input)
    total = summary.get("total_pay_amount_yuan")
    if total is None:
        total = round(sum(e.get("pay_amount_yuan") or 0 for e in events), 2)
    count = summary.get("event_count", len(events))

    print("=== 对账结果 ===")
    print(f"输入: {args.input}")
    print(f"SKU/事件数: {count}")
    print(f"订单 API 合计: {total:,.2f} 元")
    print(f"XLSX 用户支付金额: {args.xlsx_payment_yuan:,.2f} 元")
    gap = args.xlsx_payment_yuan - total
    pct = (total / args.xlsx_payment_yuan * 100) if args.xlsx_payment_yuan else 0
    print(f"差距: {gap:,.2f} 元 ({pct:.1f}% 覆盖率)")
    if args.xlsx_deal_items is not None:
        print(f"XLSX 成交件数: {args.xlsx_deal_items} (事件数 {count} 仅作参考，口径可能不同)")
    if gap > 1000:
        print("\n可能原因:")
        print("  1. 拉单 create_time 窗口太窄 — 加大 --create-buffer-before")
        print("  2. token 店铺与 XLSX 场次不一致")
        print("  3. 当前 JSON 不是按 pay_time 窗口重新拉的")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
