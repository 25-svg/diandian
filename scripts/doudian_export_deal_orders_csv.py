#!/usr/bin/env python3
"""Export SKU-level matched deal orders from reconcile v4 and order.searchList."""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
from typing import Any

from doudian_order_utils import (
    dedupe_orders,
    fmt_local,
    load_orders,
    order_status_label,
    pick_pay_time,
    pick_room_id,
)


CSV_FIELDS = [
    "order_id", "pay_time_local", "pay_time_unix", "pay_amount_yuan", "order_status",
    "buyer_id", "product_name", "leaderboard_product", "sku_index", "room_id",
]


def pick_amount_fen(order: dict[str, Any], sku: dict[str, Any]) -> int:
    for source in (sku, order):
        for key in ("pay_amount", "actual_receive_amount"):
            value = source.get(key)
            if isinstance(value, (int, float)):
                return int(value)
        amount_info = source.get("actual_receive_amount_info") or {}
        value = amount_info.get("actual_receive_amount")
        if isinstance(value, (int, float)):
            return int(value)
    return 0


def normalized(value: str) -> str:
    return "".join(value.lower().split())


def leaderboard_names_by_order(reconcile: dict[str, Any]) -> dict[str, list[str]]:
    rows = reconcile.get("by_product") or []
    if isinstance(rows, dict):
        rows = [dict(value, name=name) for name, value in rows.items()]
    result: dict[str, list[str]] = {}
    for row in rows:
        name = str(row.get("name") or "").strip()
        for order_id in row.get("order_ids") or []:
            result.setdefault(str(order_id), []).append(name)
    return result


def choose_leaderboard_product(product_name: str, candidates: list[str]) -> str:
    if len(candidates) == 1:
        return candidates[0]
    product_key = normalized(product_name)
    for candidate in candidates:
        candidate_key = normalized(candidate)
        if candidate_key == product_key or candidate_key in product_key or product_key in candidate_key:
            return candidate
    return " | ".join(candidates)


def export_rows(reconcile: dict[str, Any], orders: list[dict[str, Any]]) -> list[dict[str, Any]]:
    matched_order_ids = {str(value) for value in reconcile.get("matched_order_ids") or []}
    leaderboard_by_order = leaderboard_names_by_order(reconcile)
    rows: list[dict[str, Any]] = []
    for order in dedupe_orders(orders):
        order_id = str(order.get("order_id") or "")
        if order_id not in matched_order_ids:
            continue
        buyer_id = str(order.get("doudian_open_id") or order.get("open_id") or "")
        for sku_index, sku in enumerate(order.get("sku_order_list") or [], start=1):
            pay_time = sku.get("pay_time")
            if not isinstance(pay_time, (int, float)) or pay_time <= 0:
                pay_time = pick_pay_time(order)
            if pay_time is None:
                continue
            product_name = str(sku.get("product_name") or sku.get("product_name_str") or "").strip()
            rows.append({
                "order_id": order_id,
                "pay_time_local": fmt_local(int(pay_time)),
                "pay_time_unix": int(pay_time),
                "pay_amount_yuan": f"{pick_amount_fen(order, sku) / 100:.2f}",
                "order_status": str(order.get("order_status_desc") or order_status_label(order)),
                "buyer_id": buyer_id,
                "product_name": product_name,
                "leaderboard_product": choose_leaderboard_product(
                    product_name, leaderboard_by_order.get(order_id, [])
                ),
                "sku_index": sku_index,
                "room_id": str(sku.get("room_id_str") or sku.get("room_id") or pick_room_id(order)),
            })
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description="Export matched SKU deal orders to CSV")
    parser.add_argument("--reconcile", type=Path, required=True)
    parser.add_argument("--source-orders", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not args.reconcile.is_file() or not args.source_orders.is_file():
        parser.error("--reconcile and --source-orders must both be existing files")

    reconcile = json.loads(args.reconcile.read_text(encoding="utf-8"))
    _, orders = load_orders(args.source_orders)
    rows = export_rows(reconcile, orders)
    with args.output.open("w", newline="", encoding="utf-8-sig") as handle:
        writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS)
        writer.writeheader()
        writer.writerows(rows)
    print(
        f"Wrote {len(rows)} SKU rows / {len({row['order_id'] for row in rows})} orders / "
        f"{len({row['buyer_id'] for row in rows if row['buyer_id']})} buyers to {args.output}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
