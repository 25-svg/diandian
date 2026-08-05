#!/usr/bin/env python3
"""Match pay_time order pool to live dashboard product leaderboard (51 items).

Strategy (layered):
  1. Candidate pool: paid orders in pay_time window (e.g. 68 orders)
  2. Product targets: live dashboard product rows (件数 per product)
  3. Match SKU lines to leaderboard products by normalized name keywords
  4. Score extras: room_id=0, wrong room, unmatched product, GMV outlier

Usage:
  python scripts/doudian_match_orders_to_leaderboard.py ^
    --orders d:\\Desktop\\orders-pay-window-20260728-camera-paid.json ^
    --products-xlsx d:\\Desktop\\直播明细_全部账号_20260728_20260728.xlsx ^
    --main-room-id 7667365600607357706 ^
    --output d:\\Desktop\\orders-matched-to-51.json

If --products-xlsx is the official 整场 XLSX, uses sheet 商品分析-商品明细.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from doudian_order_utils import load_orders, pick_pay_time  # noqa: E402


def normalize_name(value: str) -> str:
    text = (value or "").lower()
    text = re.sub(r"99新\s*", "", text)
    text = re.sub(r"【[^】]*】", "", text)
    text = re.sub(r"[\s/\\|·\-_（）()【】\[\],，.+®]", "", text)
    return text


def product_keywords(name: str) -> set[str]:
    normalized = normalize_name(name)
    tokens = re.findall(r"[a-z0-9\u4e00-\u9fff]{2,}", normalized)
    stop = {"99新", "微单", "数码相机", "全画幅", "镜头", "相机", "高清", "专业", "二代", "二代2代"}
    return {token for token in tokens if token not in stop and len(token) >= 2}


def names_match(order_name: str, leaderboard_name: str) -> bool:
    a = product_keywords(order_name)
    b = product_keywords(leaderboard_name)
    if not a or not b:
        return False
    overlap = a & b
    if len(overlap) >= 2:
        return True
    if any(token in normalize_name(leaderboard_name) for token in a if len(token) >= 4):
        return True
    return normalize_name(order_name) in normalize_name(leaderboard_name) or normalize_name(
        leaderboard_name
    ) in normalize_name(order_name)


def load_leaderboard_from_xlsx(path: Path) -> list[dict[str, Any]]:
    try:
        from openpyxl import load_workbook
    except ImportError as error:
        raise RuntimeError("pip install openpyxl to read products XLSX") from error

    workbook = load_workbook(path, read_only=True, data_only=True)
    if "商品分析-商品明细" not in workbook.sheetnames:
        raise ValueError("Sheet 商品分析-商品明细 not found")
    sheet = workbook["商品分析-商品明细"]
    rows = list(sheet.iter_rows(values_only=True))
    if not rows:
        return []
    header = [str(cell or "").strip() for cell in rows[0]]
    products: list[dict[str, Any]] = []
    for raw in rows[1:]:
        row = {header[i]: raw[i] if i < len(raw) else None for i in range(len(header))}
        name = str(row.get("商品名称") or row.get("商品") or "").strip()
        if not name or name.startswith("-"):
            continue
        sold = row.get("成交件数") or row.get("件数") or 0
        try:
            sold_count = int(float(sold))
        except (TypeError, ValueError):
            sold_count = 0
        if sold_count <= 0:
            continue
        payment = row.get("用户支付金额") or row.get("支付金额") or 0
        payment_text = str(payment).replace("¥", "").replace(",", "").strip()
        try:
            payment_yuan = float(payment_text)
        except ValueError:
            payment_yuan = 0.0
        products.append(
            {
                "name": name,
                "sold_count": sold_count,
                "payment_yuan": payment_yuan,
                "buyer_count": row.get("成交人数") or row.get("人数"),
            }
        )
    return products


def load_leaderboard_from_json(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    products = payload.get("products") or payload.get("leaderboard") or []
    result: list[dict[str, Any]] = []
    for item in products:
        result.append(
            {
                "name": item.get("name") or item.get("productName") or "",
                "sold_count": int(item.get("soldCount") or item.get("sold_count") or 0),
                "payment_yuan": float(item.get("paymentAmountFen") or 0) / 100
                if item.get("paymentAmountFen")
                else float(item.get("payment_yuan") or 0),
                "buyer_count": item.get("buyerCount"),
            }
        )
    return [item for item in result if item["sold_count"] > 0]


def extract_sku_lines(full_orders_by_id: dict[str, dict]) -> list[dict[str, Any]]:
    lines: list[dict[str, Any]] = []
    for order_id, order in full_orders_by_id.items():
        skus = order.get("sku_order_list") or [order]
        for index, sku in enumerate(skus):
            product_name = (
                sku.get("product_name")
                or sku.get("product_name_str")
                or order.get("product_summary")
                or ""
            )
            room = str(sku.get("room_id_str") or sku.get("room_id") or order.get("room_id") or "0")
            pay_time = sku.get("pay_time") or order.get("pay_time")
            lines.append(
                {
                    "order_id": order_id,
                    "sku_index": index,
                    "product_name": str(product_name),
                    "room_id": room,
                    "pay_time": int(pay_time) if pay_time else None,
                    "pay_amount_yuan": round(int(sku.get("pay_amount") or order.get("pay_amount") or 0) / 100, 2),
                }
            )
    return lines


def match_lines_to_leaderboard(
    lines: list[dict[str, Any]],
    leaderboard: list[dict[str, Any]],
    *,
    main_room_id: str | None,
) -> dict[str, Any]:
    remaining = {idx: product["sold_count"] for idx, product in enumerate(leaderboard)}
    matched: list[dict[str, Any]] = []
    unmatched_lines: list[dict[str, Any]] = []

    for line in lines:
        best_idx = None
        for idx, product in enumerate(leaderboard):
            if remaining.get(idx, 0) <= 0:
                continue
            if names_match(line["product_name"], product["name"]):
                best_idx = idx
                break
        if best_idx is None:
            reason = "product_not_in_leaderboard"
            if main_room_id and line["room_id"] not in ("0", main_room_id):
                reason = "wrong_room_id"
            elif line["room_id"] == "0":
                reason = "room_id_zero"
            unmatched_lines.append({**line, "exclude_reason": reason})
            continue
        remaining[best_idx] -= 1
        matched.append(
            {
                **line,
                "leaderboard_product": leaderboard[best_idx]["name"],
                "leaderboard_payment_yuan": leaderboard[best_idx]["payment_yuan"],
            }
        )

    unfilled = [
        {
            "name": leaderboard[idx]["name"],
            "missing_count": count,
        }
        for idx, count in remaining.items()
        if count > 0
    ]

    matched_order_ids = sorted({line["order_id"] for line in matched})
    extra_order_ids = sorted({line["order_id"] for line in unmatched_lines})

    return {
        "leaderboard_target_items": sum(product["sold_count"] for product in leaderboard),
        "leaderboard_target_gmv": round(sum(product["payment_yuan"] for product in leaderboard), 2),
        "matched_sku_lines": len(matched),
        "matched_orders": len(matched_order_ids),
        "matched_gmv_yuan": round(sum(line["pay_amount_yuan"] for line in matched), 2),
        "unmatched_sku_lines": len(unmatched_lines),
        "extra_orders": len(extra_order_ids),
        "extra_gmv_yuan": round(sum(line["pay_amount_yuan"] for line in unmatched_lines), 2),
        "unfilled_leaderboard_products": unfilled,
        "matched_order_ids": matched_order_ids,
        "extra_order_ids": extra_order_ids,
        "matched_lines": matched,
        "extra_lines": unmatched_lines,
        "exclude_reason_breakdown": dict(
            Counter(line["exclude_reason"] for line in unmatched_lines)
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Match orders to product leaderboard")
    parser.add_argument("--orders", type=Path, required=True, help="Paid window export JSON")
    parser.add_argument("--source-orders", type=Path, help="Full order.searchList JSON for SKU detail")
    parser.add_argument("--products-xlsx", type=Path, help="Official live dashboard XLSX")
    parser.add_argument("--products-json", type=Path, help="Product leaderboard JSON")
    parser.add_argument("--main-room-id", default="7667365600607357706")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    paid = json.loads(args.orders.read_text(encoding="utf-8"))
    paid_ids = {row["order_id"] for row in paid.get("orders") or []}

    source_path = args.source_orders or Path(paid.get("source_file", ""))
    if not source_path.is_file():
        print(f"Need --source-orders or source_file in paid JSON: {source_path}", file=sys.stderr)
        return 1
    _, all_orders = load_orders(source_path)
    full_by_id = {
        str(order.get("order_id")): order
        for order in all_orders
        if str(order.get("order_id")) in paid_ids
    }

    if args.products_xlsx:
        leaderboard = load_leaderboard_from_xlsx(args.products_xlsx)
    elif args.products_json:
        leaderboard = load_leaderboard_from_json(args.products_json)
    else:
        print("Need --products-xlsx or --products-json", file=sys.stderr)
        return 1

    lines = extract_sku_lines(full_by_id)
    result = match_lines_to_leaderboard(
        lines,
        leaderboard,
        main_room_id=args.main_room_id.strip() or None,
    )
    result["input"] = {
        "paid_orders": len(paid_ids),
        "sku_lines": len(lines),
        "leaderboard_products": len(leaderboard),
    }

    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: result[k] for k in result if k not in {"matched_lines", "extra_lines"}}, ensure_ascii=False, indent=2))
    print(f"\nWrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
