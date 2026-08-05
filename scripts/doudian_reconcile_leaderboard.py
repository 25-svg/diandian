#!/usr/bin/env python3
"""Reconcile orders against live dashboard product leaderboard (件数/人数/金额)."""

from __future__ import annotations

import argparse
import json
import sys
import warnings
from datetime import datetime
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from doudian_match_orders_to_leaderboard import (  # noqa: E402
    extract_sku_lines,
    load_leaderboard_from_xlsx,
    names_match,
)
from doudian_order_utils import CN_TZ, load_orders, pick_pay_time  # noqa: E402

DEFAULT_AMOUNT_TOLERANCE = 80.0


def filter_orders_by_pay_window(
    orders: list[dict[str, Any]],
    *,
    shop_id: str,
    pay_start: int,
    pay_end: int,
    include_closed_paid: bool,
) -> list[dict[str, Any]]:
    allowed = {"2", "3", "5"}
    if include_closed_paid:
        allowed.add("4")
    kept: list[dict[str, Any]] = []
    for order in orders:
        if str(order.get("shop_id", "")) != shop_id:
            continue
        pay_time = pick_pay_time(order)
        if pay_time is None or pay_time < pay_start or pay_time > pay_end:
            continue
        if str(order.get("order_status", "")) not in allowed:
            continue
        kept.append(order)
    return kept


def amount_close(a: float, b: float, *, tolerance: float = 80.0) -> bool:
    """Per-line amount vs leaderboard row total / sold_count."""
    if a <= 0 or b <= 0:
        return True
    return abs(a - b) <= tolerance


def per_unit_yuan(product: dict[str, Any]) -> float | None:
    sold = int(product.get("sold_count") or 0)
    payment = float(product.get("payment_yuan") or 0)
    if sold <= 0:
        return None
    return round(payment / sold, 2)


def line_key(line: dict[str, Any]) -> str:
    return f"{line['order_id']}:{line['sku_index']}"


def amount_delta(line_amount: float, unit: float | None) -> float:
    if unit is None or unit <= 0:
        return 0.0
    return abs(line_amount - unit)


def match_by_leaderboard(
    lines: list[dict[str, Any]],
    leaderboard: list[dict[str, Any]],
    *,
    amount_tolerance: float = DEFAULT_AMOUNT_TOLERANCE,
) -> dict[str, Any]:
    """Product-first matching: for each leaderboard row pick best-N SKU lines by amount."""
    unit_yuan = {i: per_unit_yuan(p) for i, p in enumerate(leaderboard)}
    used: set[str] = set()
    matched: list[dict[str, Any]] = []

    # Match distinctive / multi-qty products first to avoid wrong early claims.
    product_order = sorted(
        range(len(leaderboard)),
        key=lambda idx: (
            -leaderboard[idx]["sold_count"],
            -(unit_yuan[idx] or 0),
        ),
    )

    for idx in product_order:
        product = leaderboard[idx]
        need = int(product["sold_count"])
        unit = unit_yuan[idx]
        candidates: list[tuple[float, dict[str, Any]]] = []
        for line in lines:
            key = line_key(line)
            if key in used:
                continue
            if not names_match(line["product_name"], product["name"]):
                continue
            delta = amount_delta(line["pay_amount_yuan"], unit)
            candidates.append((delta, line))

        candidates.sort(key=lambda item: (item[0], item[1].get("pay_time") or 0))
        picked = candidates[:need]
        if len(picked) < need:
            # Still assign best available name matches even if amount is off.
            pass

        for delta, line in picked:
            used.add(line_key(line))
            amount_ok = unit is None or delta <= amount_tolerance
            matched.append(
                {
                    **line,
                    "leaderboard_product": product["name"],
                    "leaderboard_unit_yuan": unit,
                    "leaderboard_payment_yuan": product["payment_yuan"],
                    "leaderboard_sold_count": product["sold_count"],
                    "leaderboard_buyer_count": product.get("buyer_count"),
                    "amount_delta_yuan": round(delta, 2),
                    "amount_match_ok": amount_ok,
                }
            )

    unmatched = [
        {**line, "exclude_reason": "product_not_in_leaderboard"}
        for line in lines
        if line_key(line) not in used
    ]

    by_product: list[dict[str, Any]] = []
    amount_mismatches: list[dict[str, Any]] = []
    for product in leaderboard:
        product_lines = [line for line in matched if line["leaderboard_product"] == product["name"]]
        matched_gmv = round(sum(line["pay_amount_yuan"] for line in product_lines), 2)
        target_gmv = float(product["payment_yuan"])
        gmv_delta = round(matched_gmv - target_gmv, 2)
        row = {
            "name": product["name"],
            "target_items": product["sold_count"],
            "target_people": int(product.get("buyer_count") or product["sold_count"]),
            "target_gmv_yuan": target_gmv,
            "matched_items": len(product_lines),
            "matched_gmv_yuan": matched_gmv,
            "gmv_delta_yuan": gmv_delta,
            "missing_items": max(0, product["sold_count"] - len(product_lines)),
            "order_ids": sorted({line["order_id"] for line in product_lines}),
            "amount_match_ok": all(line.get("amount_match_ok", True) for line in product_lines),
        }
        by_product.append(row)
        if row["missing_items"] == 0 and abs(gmv_delta) > amount_tolerance:
            amount_mismatches.append(row)

    matched_order_ids = sorted({line["order_id"] for line in matched})
    return {
        "leaderboard_target_items": sum(p["sold_count"] for p in leaderboard),
        "leaderboard_target_gmv": round(sum(p["payment_yuan"] for p in leaderboard), 2),
        "leaderboard_target_people_sum": sum(
            int(p.get("buyer_count") or p["sold_count"]) for p in leaderboard
        ),
        "matched_sku_lines": len(matched),
        "matched_orders": len(matched_order_ids),
        "matched_gmv_yuan": round(sum(line["pay_amount_yuan"] for line in matched), 2),
        "unmatched_sku_lines": len(unmatched),
        "extra_orders": len({line["order_id"] for line in unmatched}),
        "by_product": by_product,
        "amount_mismatches": amount_mismatches,
        "matched_order_ids": matched_order_ids,
        "unfilled": [row for row in by_product if row["missing_items"] > 0],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-orders", type=Path, required=True)
    parser.add_argument("--products-xlsx", type=Path, required=True)
    parser.add_argument("--shop-id", default="212709966")
    parser.add_argument("--pay-time-start", default="2026/07/28 08:15:00")
    parser.add_argument("--pay-time-end", default="2026/07/28 15:44:39")
    parser.add_argument("--include-closed-paid", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    pay_start = int(datetime.strptime(args.pay_time_start, "%Y/%m/%d %H:%M:%S").replace(tzinfo=CN_TZ).timestamp())
    pay_end = int(datetime.strptime(args.pay_time_end, "%Y/%m/%d %H:%M:%S").replace(tzinfo=CN_TZ).timestamp())

    _, all_orders = load_orders(args.source_orders)
    pool = filter_orders_by_pay_window(
        all_orders,
        shop_id=args.shop_id,
        pay_start=pay_start,
        pay_end=pay_end,
        include_closed_paid=args.include_closed_paid,
    )
    full_by_id = {str(order.get("order_id")): order for order in pool}
    with warnings.catch_warnings():
        warnings.filterwarnings("ignore", message="Workbook contains no default style")
        leaderboard = load_leaderboard_from_xlsx(args.products_xlsx)
    lines = extract_sku_lines(full_by_id)
    result = match_by_leaderboard(lines, leaderboard)
    result["input"] = {
        "orders_in_pool": len(pool),
        "sku_lines": len(lines),
        "leaderboard_products": len(leaderboard),
        "include_closed_paid": args.include_closed_paid,
    }

    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"Pool orders: {len(pool)} | SKU lines: {len(lines)}")
    print(
        f"Matched: {result['matched_sku_lines']}/{result['leaderboard_target_items']} items, "
        f"{result['matched_orders']} orders, GMV {result['matched_gmv_yuan']}"
    )
    print(f"Unfilled products: {len(result['unfilled'])}")
    for row in result["unfilled"]:
        print(f"  - {row['name'][:55]} missing {row['missing_items']}")
    if result.get("amount_mismatches"):
        print(f"Amount mismatches (>{DEFAULT_AMOUNT_TOLERANCE}): {len(result['amount_mismatches'])}")
        for row in result["amount_mismatches"]:
            print(
                f"  - target {row['target_gmv_yuan']:.0f} vs matched {row['matched_gmv_yuan']:.0f} "
                f"| {row['name'][:50]}"
            )

    print("\n--- per-product (target vs matched) ---")
    for row in result["by_product"]:
        if row["target_items"] <= 0:
            continue
        if row["missing_items"] == 0 and abs(row.get("gmv_delta_yuan") or 0) <= DEFAULT_AMOUNT_TOLERANCE:
            flag = "OK"
        elif row["missing_items"] == 0:
            flag = f"金额差{row['gmv_delta_yuan']:+.0f}"
        else:
            flag = f"缺{row['missing_items']}"
        print(
            f"[{flag}] {row['target_items']}pcs/{row['target_people']}ppl {row['target_gmv_yuan']:.0f} "
            f"-> {row['matched_items']}pcs {row['matched_gmv_yuan']:.0f} | {row['name'][:50]}"
        )

    print(f"\nWrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
