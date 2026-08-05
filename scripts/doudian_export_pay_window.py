#!/usr/bin/env python3
"""Export orders whose pay_time falls in a live window from order.searchList JSON.

Example (7/28 camera shop, from existing all-status pull):
  python scripts/doudian_export_pay_window.py ^
    --input d:\\Desktop\\order-searchList-all-20260729-152019.json ^
    --shop-id 212709966 ^
    --pay-time-start "2026/07/28 08:15:00" ^
    --pay-time-end "2026/07/28 15:44:39" ^
    --output d:\\Desktop\\orders-pay-window-20260728-camera.json
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from collections import Counter
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from doudian_order_utils import (
    CN_TZ,
    dedupe_orders,
    fmt_local,
    load_orders,
    order_status_label,
    pick_create_time,
    pick_pay_time,
    slim_order_row,
)


def filter_by_pay_window(
    orders: list[dict[str, Any]],
    *,
    pay_start: int,
    pay_end: int,
    shop_id: str | None,
    paid_only: bool,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    stats: dict[str, Any] = {
        "input_orders": len(orders),
        "no_pay_time": 0,
        "outside_window": 0,
        "shop_mismatch": 0,
        "unpaid_status": 0,
        "kept_orders": 0,
    }
    kept: list[dict[str, Any]] = []
    for order in orders:
        if shop_id and str(order.get("shop_id", "")) != shop_id:
            stats["shop_mismatch"] += 1
            continue
        pay_time = pick_pay_time(order)
        if pay_time is None:
            stats["no_pay_time"] += 1
            continue
        if pay_time < pay_start or pay_time > pay_end:
            stats["outside_window"] += 1
            continue
        if paid_only and str(order.get("order_status")) not in {"2", "3", "5"}:
            stats["unpaid_status"] += 1
            continue
        kept.append(order)
    stats["kept_orders"] = len(kept)
    stats["sku_rows"] = sum(len(order.get("sku_order_list") or []) for order in kept)
    stats["gmv_fen"] = sum(int(order.get("pay_amount") or 0) for order in kept)
    stats["status_breakdown"] = dict(
        Counter(order_status_label(order) for order in kept)
    )
    return kept, stats


def write_csv(rows: list[dict[str, Any]], path: Path) -> None:
    fields = [
        "order_id",
        "pay_time_local",
        "create_time_local",
        "order_status_label",
        "pay_amount_yuan",
        "sku_count",
        "room_id",
        "product_summary",
    ]
    with path.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            writer.writerow({field: row.get(field, "") for field in fields})


def main() -> int:
    parser = argparse.ArgumentParser(description="Export pay_time window orders")
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--shop-id", default="212709966")
    parser.add_argument("--pay-time-start", default="2026/07/28 08:15:00")
    parser.add_argument("--pay-time-end", default="2026/07/28 15:44:39")
    parser.add_argument(
        "--paid-only",
        action="store_true",
        help="Keep only order_status 2/3/5",
    )
    parser.add_argument("--include-full-order", action="store_true")
    args = parser.parse_args()

    if not args.input.is_file():
        print(f"Input not found: {args.input}", file=sys.stderr)
        return 1

    pay_start = int(
        datetime.strptime(args.pay_time_start, "%Y/%m/%d %H:%M:%S")
        .replace(tzinfo=CN_TZ)
        .timestamp()
    )
    pay_end = int(
        datetime.strptime(args.pay_time_end, "%Y/%m/%d %H:%M:%S")
        .replace(tzinfo=CN_TZ)
        .timestamp()
    )

    source_payload, orders = load_orders(args.input)
    orders = dedupe_orders(orders)
    kept, stats = filter_by_pay_window(
        orders,
        pay_start=pay_start,
        pay_end=pay_end,
        shop_id=args.shop_id.strip() or None,
        paid_only=args.paid_only,
    )
    kept.sort(key=lambda order: pick_pay_time(order) or 0)
    rows = [slim_order_row(order) for order in kept]

    payload: dict[str, Any] = {
        "exported_at": datetime.now(CN_TZ).isoformat(),
        "source_file": str(args.input),
        "shop_id": args.shop_id,
        "pay_time_start_local": fmt_local(pay_start),
        "pay_time_end_local": fmt_local(pay_end),
        "paid_only": args.paid_only,
        "stats": stats,
        "orders": rows,
    }
    if args.include_full_order:
        payload["full_orders"] = kept

    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    csv_path = args.output.with_suffix(".csv")
    write_csv(rows, csv_path)

    summary_path = args.output.with_name(args.output.stem + "-summary.json")
    summary_path.write_text(
        json.dumps(
            {
                "exported_at": payload["exported_at"],
                "source_file": payload["source_file"],
                "shop_id": args.shop_id,
                "window": {
                    "start": payload["pay_time_start_local"],
                    "end": payload["pay_time_end_local"],
                },
                "stats": stats,
                "order_ids": [row["order_id"] for row in rows],
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )

    print(
        f"Exported {stats['kept_orders']} orders / {stats['sku_rows']} SKU rows\n"
        f"  GMV: Y{stats['gmv_fen'] / 100:,.2f}\n"
        f"  status: {stats['status_breakdown']}\n"
        f"  -> {args.output}\n"
        f"  -> {csv_path}\n"
        f"  -> {summary_path}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
