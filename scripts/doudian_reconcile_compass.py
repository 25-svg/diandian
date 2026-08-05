#!/usr/bin/env python3
"""Compare pay_time window export vs compass KPI (51 items / 46 buyers / GMV)."""

from __future__ import annotations

import csv
import json
import sys
from collections import Counter
from datetime import datetime, timezone, timedelta
from pathlib import Path

CN_TZ = timezone(timedelta(hours=8))
COMPASS = {"items": 51, "buyers": 46, "gmv_yuan": 236762}


def sku_room_ids(order: dict) -> set[str]:
    rooms: set[str] = set()
    for sku in order.get("sku_order_list") or []:
        rooms.add(str(sku.get("room_id_str") or sku.get("room_id") or "0"))
    if not rooms:
        rooms.add(str(order.get("room_id_str") or order.get("room_id") or "0"))
    return rooms


def main() -> int:
    paid_path = Path(r"d:\Desktop\orders-pay-window-20260728-camera-paid.json")
    full_path = Path(r"d:\Desktop\order-searchList-all-20260729-152019.json")
    if not paid_path.is_file():
        print(f"Missing {paid_path}", file=sys.stderr)
        return 1

    paid = json.loads(paid_path.read_text(encoding="utf-8"))
    orders = paid["orders"]
    stats = paid["stats"]

    by_id: dict[str, dict] = {}
    if full_path.is_file():
        full = json.loads(full_path.read_text(encoding="utf-8"))
        for shop in full.get("shops") or []:
            for order in shop.get("orders") or []:
                by_id[str(order.get("order_id", ""))] = order

    room_order = Counter(row.get("room_id") or "0" for row in orders)
    room_sku: Counter[str] = Counter()
    enriched = []
    all_room_zero = []

    for row in sorted(orders, key=lambda item: item.get("pay_time") or 0):
        full_order = by_id.get(row["order_id"], {})
        rooms = sku_room_ids(full_order) if full_order else {row.get("room_id") or "0"}
        for room in rooms:
            room_sku[room] += 1
        rooms_text = ",".join(sorted(rooms))
        only_zero = rooms <= {"0"}
        if only_zero:
            all_room_zero.append(row)
        attributed = "unknown (room_id=0)" if only_zero else "maybe (has room_id)"
        item = {
            **row,
            "sku_room_ids": rooms_text,
            "likely_live_attributed": attributed,
        }
        enriched.append(item)

    hours: Counter[str] = Counter()
    for row in orders:
        pay_time = row.get("pay_time")
        if pay_time:
            hour = datetime.fromtimestamp(pay_time, CN_TZ).strftime("%H")
            hours[f"{hour}:00-{hour}:59"] += 1

    gmv = stats["gmv_fen"] / 100
    report = {
        "compass": COMPASS,
        "api_paid_window": {
            "orders": len(orders),
            "sku_rows": stats["sku_rows"],
            "gmv_yuan": gmv,
            "window": {
                "start": paid.get("pay_time_start_local"),
                "end": paid.get("pay_time_end_local"),
            },
        },
        "delta_vs_compass": {
            "orders_minus_items": len(orders) - COMPASS["items"],
            "sku_rows_minus_items": stats["sku_rows"] - COMPASS["items"],
            "gmv_minus_yuan": round(gmv - COMPASS["gmv_yuan"], 2),
        },
        "room_id_at_order_level": dict(room_order),
        "room_id_at_sku_level": dict(room_sku),
        "orders_with_only_room_zero": {
            "count": len(all_room_zero),
            "gmv_yuan": round(sum(row["pay_amount_yuan"] for row in all_room_zero), 2),
        },
        "orders_with_nonzero_room": {
            "count": len(orders) - len(all_room_zero),
            "gmv_yuan": round(
                sum(row["pay_amount_yuan"] for row in orders if row not in all_room_zero),
                2,
            ),
        },
        "pay_time_hourly": dict(sorted(hours.items())),
        "interpretation": [
            "Compass 51 = session-attributed deal items, not shop-wide pay_time count.",
            "API 68 orders = all paid orders in pay_time window for shop 212709966.",
            f"Gap of {len(orders) - COMPASS['items']} orders cannot be closed without live-session attribution API.",
        ],
    }

    out_json = Path(r"d:\Desktop\orders-reconcile-vs-compass-20260728.json")
    out_json.write_text(
        json.dumps({"summary": report, "orders": enriched}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    out_csv = Path(r"d:\Desktop\orders-pay-window-20260728-camera-paid-reconcile.csv")
    fields = [
        "order_id",
        "pay_time_local",
        "pay_amount_yuan",
        "sku_count",
        "order_status_label",
        "room_id",
        "sku_room_ids",
        "likely_live_attributed",
        "product_summary",
    ]
    with out_csv.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in enriched:
            writer.writerow({field: row.get(field, "") for field in fields})

    print(json.dumps(report, ensure_ascii=False, indent=2))
    print(f"\nWrote {out_json}")
    print(f"Wrote {out_csv}")

    by_room: dict[str, dict] = {}
    for row in orders:
        full_order = by_id.get(row["order_id"], {})
        rooms = sku_room_ids(full_order) if full_order else {row.get("room_id") or "0"}
        nonzero = [room for room in rooms if room != "0"]
        room = nonzero[0] if nonzero else "0"
        bucket = by_room.setdefault(
            room, {"orders": 0, "skus": 0, "gmv_yuan": 0.0, "order_ids": []}
        )
        bucket["orders"] += 1
        bucket["skus"] += row["sku_count"]
        bucket["gmv_yuan"] += row["pay_amount_yuan"]
        bucket["order_ids"].append(row["order_id"])

    print("\nPer room_id:")
    for room, bucket in sorted(by_room.items(), key=lambda item: -item[1]["gmv_yuan"]):
        print(
            f"  {room}: orders={bucket['orders']} skus={bucket['skus']} "
            f"gmv=Y{bucket['gmv_yuan']:,.2f}"
        )
        if room == "7667365600607357706":
            print(
                f"    vs compass 51 items / Y236,762 -> "
                f"delta skus={bucket['skus'] - COMPASS['items']} "
                f"delta gmv=Y{bucket['gmv_yuan'] - COMPASS['gmv_yuan']:,.2f}"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
