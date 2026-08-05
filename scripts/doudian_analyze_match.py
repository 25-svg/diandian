#!/usr/bin/env python3
"""Deep-dive analysis of orders-matched-to-51 output."""

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from doudian_order_utils import iter_orders, load_orders, pick_pay_time, fmt_local

ORDER_JSON = Path(r"d:\Desktop\order-searchList-20260728-0815-1544-20260729-161739.json")
MATCH_JSON = Path(r"d:\Desktop\orders-matched-to-51-v2.json")

MISSING_KEYWORDS = [
    ("Canon R8/R5/R6", ["r8", "r5c", "r62", "r6"]),
    ("DJI Osmo 360", ["osmo", "360"]),
    ("Canon RF 45mm", ["rf45", "45mm"]),
    ("Canon RF 35mm macro", ["rf35", "35mm"]),
]


def product_text(order: dict) -> str:
    parts = []
    for sku in order.get("sku_order_list") or []:
        parts.append(str(sku.get("product_name") or sku.get("product_name_str") or ""))
    return " | ".join(parts).lower()


def room_ids(order: dict) -> str:
    rooms = set()
    for sku in order.get("sku_order_list") or []:
        rooms.add(str(sku.get("room_id_str") or sku.get("room_id") or "0"))
    return ",".join(sorted(rooms))


def main() -> None:
    match = json.loads(MATCH_JSON.read_text(encoding="utf-8"))
    _, orders = load_orders(ORDER_JSON)
    cam = [o for o in orders if str(o.get("shop_id")) == "212709966"]
    by_id = {str(o.get("order_id")): o for o in cam}

    matched = set(match.get("matched_order_ids") or [])
    extra = set(match.get("extra_order_ids") or [])
    overlap = matched & extra
    print("=== ID LIST CHECK ===")
    print("matched:", len(matched), "extra:", len(extra), "overlap:", overlap)

    print("\n=== EXTRA ORDERS (23) ===")
    for oid in sorted(extra):
        o = by_id.get(oid)
        if not o:
            print(oid, "NOT IN JSON")
            continue
        pt = pick_pay_time(o)
        print(
            f"{oid} status={o.get('order_status')} pay={fmt_local(pt) if pt else '?'} "
            f"room={room_ids(o)} amt={int(o.get('pay_amount') or 0)/100:.2f}"
        )
        print(f"  {product_text(o)[:100]}")

    print("\n=== SEARCH 4 MISSING PRODUCTS IN ALL 146 ===")
    for label, keys in MISSING_KEYWORDS:
        hits = []
        for o in cam:
            text = product_text(o)
            if any(k in text for k in keys):
                hits.append(o)
        print(f"\n{label}: {len(hits)} hit(s)")
        for o in hits[:5]:
            pt = pick_pay_time(o)
            oid = str(o.get("order_id"))
            flag = []
            if oid in matched:
                flag.append("MATCHED")
            if oid in extra:
                flag.append("EXTRA")
            print(
                f"  {oid} [{','.join(flag) or 'NOT IN 68 PAID POOL'}] "
                f"status={o.get('order_status')} pay={fmt_local(pt) if pt else 'none'} "
                f"create={fmt_local(int(o.get('create_time')))} room={room_ids(o)}"
            )
            print(f"    {product_text(o)[:120]}")

    print("\n=== GMV RECON ===")
    print("matched_gmv:", match.get("matched_gmv_yuan"))
    print("extra_gmv:", match.get("extra_gmv_yuan"))
    print("sum:", (match.get("matched_gmv_yuan") or 0) + (match.get("extra_gmv_yuan") or 0))


if __name__ == "__main__":
    main()
