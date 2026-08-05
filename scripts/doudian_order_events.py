#!/usr/bin/env python3
"""Extract payment events from order.searchList JSON for live/transcript alignment.

Usage:
  python scripts/doudian_order_events.py ^
    --orders "d:\\Desktop\\order-searchList-20260729-135146.json" ^
    --started-at "2026-07-29 08:15:49" ^
    --output events.json

Each event:
  pay_time, pay_time_local, pay_amount_fen, pay_amount_yuan,
  product_name, product_id, order_id, offset_sec
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any, Iterator

from doudian_order_utils import fmt_local as shared_fmt_local

CN_TZ = timezone(timedelta(hours=8))


def parse_started_at(value: str) -> int:
    value = value.strip()
    for fmt in (
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
    ):
        try:
            return int(datetime.strptime(value, fmt).replace(tzinfo=CN_TZ).timestamp())
        except ValueError:
            continue
    # XLSX range: 2026/07/28 08:15:49-2026/07/28 15:44:39
    head = value.split("-", 1)[0].strip()
    return parse_started_at(head)


def fmt_local(ts: int) -> str:
    return shared_fmt_local(ts)


def load_matched_order_ids(path: Path) -> set[str]:
    """Load the v4 reconcile JSON's matched_order_ids without copying order data."""
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, list):
        return {str(value) for value in payload}
    values = payload.get("matched_order_ids") or []
    if not isinstance(values, list):
        raise ValueError("order IDs file must be a JSON list or contain matched_order_ids")
    return {str(value) for value in values}


def shop_matches(
    shop_id: str,
    shop_name: str,
    *,
    filter_shop_id: str | None,
    filter_shop_name: str | None,
) -> bool:
    if filter_shop_id and shop_id != filter_shop_id:
        return False
    if filter_shop_name and filter_shop_name not in shop_name:
        return False
    return True


def iter_shop_orders(
    payload: dict[str, Any],
    *,
    shop_id: str | None = None,
    shop_name: str | None = None,
) -> Iterator[tuple[dict[str, str], dict[str, Any]]]:
    if "shop_order_list" in payload:
        meta = {
            "shop_id": str(payload.get("shop_id", "")),
            "shop_name": str(payload.get("shop_name", "")),
        }
        if shop_matches(meta["shop_id"], meta["shop_name"], filter_shop_id=shop_id, filter_shop_name=shop_name):
            for order in payload["shop_order_list"]:
                yield meta, order
        return

    for shop in payload.get("shops", []):
        meta = {
            "shop_id": str(shop.get("shop_id", "")),
            "shop_name": str(shop.get("shop_name", "")),
        }
        if not shop_matches(
            meta["shop_id"],
            meta["shop_name"],
            filter_shop_id=shop_id,
            filter_shop_name=shop_name,
        ):
            continue
        for order in shop.get("orders", []):
            yield meta, order
        for page in shop.get("pages", []):
            inner = page.get("payload") or page
            for order in inner.get("shop_order_list", []):
                yield meta, order


def pick_pay_time(order: dict[str, Any], sku: dict[str, Any] | None) -> int | None:
    for source in (sku, order):
        if not source:
            continue
        ts = source.get("pay_time") or source.get("create_time")
        if isinstance(ts, (int, float)) and ts > 0:
            return int(ts)
    return None


def pick_pay_amount_fen(order: dict[str, Any], sku: dict[str, Any] | None) -> int | None:
    for source in (sku, order):
        if not source:
            continue
        for key in ("pay_amount", "actual_receive_amount"):
            amount = source.get(key)
            if isinstance(amount, (int, float)) and amount > 0:
                return int(amount)
        info = source.get("actual_receive_amount_info") or {}
        amount = info.get("actual_receive_amount")
        if isinstance(amount, (int, float)) and amount > 0:
            return int(amount)
    return None


def extract_events(
    orders_path: Path,
    started_at_unix: int,
    *,
    shop_id: str | None = None,
    shop_name: str | None = None,
    order_ids: set[str] | None = None,
) -> tuple[list[dict[str, Any]], dict[str, str] | None]:
    payload = json.loads(orders_path.read_text(encoding="utf-8"))
    events: list[dict[str, Any]] = []
    shop_meta: dict[str, str] | None = None

    for meta, order in iter_shop_orders(payload, shop_id=shop_id, shop_name=shop_name):
        shop_meta = meta
        shop_order_id = str(order.get("order_id", ""))
        if order_ids is not None and shop_order_id not in order_ids:
            continue
        sku_list = order.get("sku_order_list") or [None]

        for sku_index, sku in enumerate(sku_list, start=1):
            pay_time = pick_pay_time(order, sku)
            if pay_time is None:
                continue

            create_time = order.get("create_time")
            if isinstance(create_time, (int, float)) and create_time > 0:
                create_time = int(create_time)
            else:
                create_time = None

            product_name = ""
            product_id = ""
            if sku:
                product_name = str(sku.get("product_name") or "").strip()
                product_id = str(sku.get("product_id") or sku.get("sku_id") or "")

            pay_amount_fen = pick_pay_amount_fen(order, sku)
            offset_sec = pay_time - started_at_unix

            events.append(
                {
                    "shop_id": meta["shop_id"],
                    "shop_name": meta["shop_name"],
                    "pay_time": pay_time,
                    "pay_time_local": fmt_local(pay_time),
                    "create_time": create_time,
                    "create_time_local": fmt_local(create_time) if create_time else None,
                    "pay_amount_fen": pay_amount_fen,
                    "pay_amount_yuan": round(pay_amount_fen / 100, 2)
                    if pay_amount_fen is not None
                    else None,
                    "product_name": product_name,
                    "product_id": product_id,
                    "order_id": shop_order_id,
                    "buyer_id": str(order.get("doudian_open_id") or order.get("open_id") or ""),
                    "sku_index": sku_index,
                    "order_status": order.get("order_status"),
                    "offset_sec": offset_sec,
                    "offset_hms": _sec_to_hms(max(offset_sec, 0)),
                }
            )

    events.sort(key=lambda item: item["pay_time"])
    return events, shop_meta


def _sec_to_hms(total: int) -> str:
    hours, rem = divmod(total, 3600)
    minutes, seconds = divmod(rem, 60)
    if hours:
        return f"{hours}:{minutes:02d}:{seconds:02d}"
    return f"{minutes}:{seconds:02d}"


def summarize(
    events: list[dict[str, Any]],
    started_at_unix: int,
    shop_meta: dict[str, str] | None,
) -> dict[str, Any]:
    total_fen = sum(event["pay_amount_fen"] or 0 for event in events)
    summary: dict[str, Any] = {
        "live_started_at": fmt_local(started_at_unix),
        "live_started_at_unix": started_at_unix,
        "event_count": len(events),
        "total_pay_amount_yuan": round(total_fen / 100, 2),
        "first_pay_local": events[0]["pay_time_local"] if events else None,
        "last_pay_local": events[-1]["pay_time_local"] if events else None,
    }
    if shop_meta:
        summary["shop_id"] = shop_meta["shop_id"]
        summary["shop_name"] = shop_meta["shop_name"]
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description="Extract pay-time events from order.searchList JSON")
    parser.add_argument("--orders", type=Path, required=True, help="order.searchList JSON path")
    parser.add_argument(
        "--started-at",
        required=True,
        help='Live start time, e.g. "2026-07-29 08:15:49" or XLSX "2026/07/28 08:15:49-..."',
    )
    parser.add_argument("--output", type=Path, help="Write JSON result here (default: stdout)")
    parser.add_argument(
        "--in-window",
        type=int,
        default=None,
        help="Only keep events with 0 <= offset_sec <= N seconds (optional)",
    )
    parser.add_argument("--shop-id", help="Only include orders from this shop_id")
    parser.add_argument(
        "--shop-name",
        help='Only include orders whose shop_name contains this text, e.g. "金典拍拍相机专卖店"',
    )
    parser.add_argument(
        "--pay-time-start",
        help='Absolute pay_time lower bound, e.g. "2026/07/28 08:15:49"',
    )
    parser.add_argument(
        "--pay-time-end",
        help='Absolute pay_time upper bound, e.g. "2026/07/28 15:44:39"',
    )
    parser.add_argument(
        "--order-ids-file",
        type=Path,
        help="v4 reconcile JSON (matched_order_ids) or a JSON order ID list; filters before SKU events",
    )
    args = parser.parse_args()

    if not args.orders.is_file():
        print(f"Orders file not found: {args.orders}", file=sys.stderr)
        return 1
    if args.order_ids_file and not args.order_ids_file.is_file():
        print(f"Order IDs file not found: {args.order_ids_file}", file=sys.stderr)
        return 1

    started_at_unix = parse_started_at(args.started_at)
    try:
        order_ids = load_matched_order_ids(args.order_ids_file) if args.order_ids_file else None
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Could not load order IDs file: {error}", file=sys.stderr)
        return 1
    events, shop_meta = extract_events(
        args.orders,
        started_at_unix,
        shop_id=args.shop_id,
        shop_name=args.shop_name,
        order_ids=order_ids,
    )

    if args.in_window is not None:
        events = [e for e in events if 0 <= e["offset_sec"] <= args.in_window]

    if args.pay_time_start:
        pay_start = parse_started_at(args.pay_time_start)
        events = [e for e in events if e["pay_time"] >= pay_start]
    if args.pay_time_end:
        pay_end = parse_started_at(args.pay_time_end)
        events = [e for e in events if e["pay_time"] <= pay_end]

    if (args.shop_id or args.shop_name) and not events and shop_meta is None:
        print("No orders matched the shop filter.", file=sys.stderr)
        return 1

    result = {
        "summary": summarize(events, started_at_unix, shop_meta),
        "events": events,
    }

    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")
        print(f"Wrote {len(events)} events to {args.output}")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
