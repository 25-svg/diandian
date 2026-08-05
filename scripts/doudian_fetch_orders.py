#!/usr/bin/env python3
"""Fetch Doudian order.searchList JSON aligned to a live session.

Official API has NO pay_time_start/end. Two modes:

  --placed-in-live (recommended for 罗盘「成交/下单」对齐)
    create_time = live start .. live end (+ optional after buffer)
    order_status = 2,3,5 (paid / shipped / completed)
    keep all rows; optional client filter create_time in live window

  default (pay_time alignment for SRT)
    widen create_time around live window, then filter by pay_time locally

Usage (7/28 camera shop — 按下单时间重拉):
  python scripts/doudian_fetch_orders.py ^
    --token-file d:\\Desktop\\doudian_token.env ^
    --live-started-at "2026/07/28 08:15:49" ^
    --live-ended-at "2026/07/28 15:44:39" ^
    --placed-in-live ^
    --shop-id 212709966 ^
    --output d:\\Desktop\\order-searchList-camera-20260728-placed.json

See docs/integrations/douyin/order-searchList-pay-time-guide.md
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any

CN_TZ = timezone(timedelta(hours=8))
DEFAULT_ENV = Path(r"d:\Desktop\.env")
DEFAULT_TOKEN = Path(r"d:\Desktop\doudian_token.env")
DEFAULT_SDK = Path(r"d:\Desktop\doudian-sdk-python-1.1.0-20260724091610\sdk-python")


def load_kv_file(path: Path) -> dict[str, str]:
    env: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        value = value.strip().strip('"').rstrip('"')
        env[key.strip()] = value
    return env


def parse_time(value: str) -> int:
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
    if value.isdigit():
        return int(value)
    head = value.split("-", 1)[0].strip()
    if head != value:
        return parse_time(head)
    raise ValueError(f"Unrecognized time: {value}")


def fmt_local(ts: int) -> str:
    return datetime.fromtimestamp(ts, CN_TZ).strftime("%Y-%m-%d %H:%M:%S")


def normalize_order_status(value: str | None) -> str | None:
    normalized = (value or "").strip()
    return normalized or None


def pick_create_time(order: dict[str, Any]) -> int | None:
    ts = order.get("create_time")
    if isinstance(ts, (int, float)) and ts > 0:
        return int(ts)
    return None


def pick_pay_time(order: dict[str, Any]) -> int | None:
    for sku in order.get("sku_order_list") or []:
        ts = sku.get("pay_time")
        if isinstance(ts, (int, float)) and ts > 0:
            return int(ts)
    ts = order.get("pay_time")
    if isinstance(ts, (int, float)) and ts > 0:
        return int(ts)
    return None


def filter_orders_by_pay_time(
    orders: list[dict[str, Any]],
    pay_start: int | None,
    pay_end: int | None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    kept: list[dict[str, Any]] = []
    stats = {"input_orders": len(orders), "no_pay_time": 0, "outside_window": 0, "kept_orders": 0}
    for order in orders:
        pay_time = pick_pay_time(order)
        if pay_time is None:
            stats["no_pay_time"] += 1
            continue
        if pay_start is not None and pay_time < pay_start:
            stats["outside_window"] += 1
            continue
        if pay_end is not None and pay_time > pay_end:
            stats["outside_window"] += 1
            continue
        kept.append(order)
    stats["kept_orders"] = len(kept)
    return kept, stats


def filter_orders_by_create_time(
    orders: list[dict[str, Any]],
    create_start: int | None,
    create_end: int | None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    kept: list[dict[str, Any]] = []
    stats = {"input_orders": len(orders), "no_create_time": 0, "outside_window": 0, "kept_orders": 0}
    for order in orders:
        create_time = pick_create_time(order)
        if create_time is None:
            stats["no_create_time"] += 1
            continue
        if create_start is not None and create_time < create_start:
            stats["outside_window"] += 1
            continue
        if create_end is not None and create_time > create_end:
            stats["outside_window"] += 1
            continue
        kept.append(order)
    stats["kept_orders"] = len(kept)
    return kept, stats


def filter_orders_by_shop_id(
    orders: list[dict[str, Any]],
    shop_id: str,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    target = shop_id.strip()
    kept = [order for order in orders if str(order.get("shop_id", "")) == target]
    stats = {
        "input_orders": len(orders),
        "shop_id": target,
        "kept_orders": len(kept),
        "dropped_orders": len(orders) - len(kept),
    }
    return kept, stats


def build_summary(shop_meta: dict[str, Any], orders: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "shop_id": shop_meta.get("shop_id"),
        "shop_name": shop_meta.get("shop_name"),
        "order_count": len(orders),
        "total": len(orders),
        "error": None,
        "order_ids": [str(order.get("order_id", "")) for order in orders],
        "sample": orders[0] if orders else None,
    }


class StaticAccessToken:
    def __init__(self, token: str) -> None:
        self._token = token

    def getAccessToken(self) -> str:
        return self._token


def fetch_orders(
    *,
    access_token: str,
    sdk_path: Path,
    create_time_start: int,
    create_time_end: int,
    order_status: str | None,
    page_size: int,
) -> list[dict[str, Any]]:
    sys.path.insert(0, str(sdk_path))
    from doudian.api.order_searchList.OrderSearchListRequest import OrderSearchListRequest
    from doudian.api.order_searchList.param.CombineStatusItem import CombineStatusItem

    orders: list[dict[str, Any]] = []
    page = 0
    total = None

    while True:
        request = OrderSearchListRequest()
        request.params.create_time_start = create_time_start
        request.params.create_time_end = create_time_end
        request.params.page = page
        request.params.size = page_size
        request.params.order_by = "create_time"
        request.params.order_asc = False

        if order_status:
            status = CombineStatusItem()
            status.order_status = order_status
            request.params.combine_status = [status]

        response = request.execute(StaticAccessToken(access_token))
        if not response.isSuccess():
            raise RuntimeError(
                f"order.searchList failed: code={response.code} msg={response.msg} "
                f"sub_code={response.sub_code} sub_msg={response.sub_msg}"
            )

        data = response.data or {}
        batch = data.get("shop_order_list") or []
        total = data.get("total", total)
        orders.extend(batch)

        if not batch:
            break
        if total is not None and len(orders) >= int(total):
            break
        if len(batch) < page_size:
            break
        page += 1

    return orders


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch order.searchList for a live pay_time window")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV)
    parser.add_argument("--token-file", type=Path, default=DEFAULT_TOKEN)
    parser.add_argument("--sdk-path", type=Path, default=DEFAULT_SDK)
    parser.add_argument("--output", type=Path, required=True)

    parser.add_argument("--create-time-start", help="Unix sec or local datetime")
    parser.add_argument("--create-time-end", help="Unix sec or local datetime")
    parser.add_argument("--pay-time-start", help="Target pay window start (for client-side filter)")
    parser.add_argument("--pay-time-end", help="Target pay window end (for client-side filter)")
    parser.add_argument("--live-started-at", help='XLSX/record start, e.g. "2026/07/28 08:15:49"')
    parser.add_argument("--live-ended-at", help='XLSX end, e.g. "2026/07/28 15:44:39"')
    parser.add_argument(
        "--create-buffer-before",
        type=int,
        default=259200,
        help="Seconds before pay/live start for create_time_start (default 3 days)",
    )
    parser.add_argument(
        "--create-buffer-after",
        type=int,
        default=86400,
        help="Seconds after pay/live end for create_time_end (default 1 day)",
    )
    parser.add_argument(
        "--order-status",
        default="",
        help='combine_status.order_status, e.g. "2" or "2,3,5". Default: none; with --placed-in-live: "2,3,5".',
    )
    parser.add_argument("--page-size", type=int, default=50)
    parser.add_argument(
        "--keep-unpaid",
        action="store_true",
        help="Do not filter by pay_time; keep all rows returned by create_time query",
    )
    parser.add_argument(
        "--placed-in-live",
        action="store_true",
        help="Pull orders placed during live: create_time = live window, status 2,3,5, no pay_time filter",
    )
    parser.add_argument(
        "--shop-id",
        default="",
        help="Keep only orders for this shop_id (client-side filter after fetch)",
    )
    parser.add_argument(
        "--no-create-window-filter",
        action="store_true",
        help="With --placed-in-live: skip client-side create_time in [live_start, live_end] filter",
    )
    parser.add_argument("--summary-output", type=Path, help="Optional summary JSON path")
    args = parser.parse_args()

    if args.placed_in_live:
        if args.create_buffer_before == 259200:
            args.create_buffer_before = 0
        if not args.order_status:
            args.order_status = "2,3,5"
        args.keep_unpaid = True

    if not args.sdk_path.is_dir():
        print(f"SDK not found: {args.sdk_path}", file=sys.stderr)
        return 1
    if not args.token_file.is_file():
        print(f"Token file not found: {args.token_file}", file=sys.stderr)
        print("Run scripts/doudian_get_token.py first.", file=sys.stderr)
        return 1

    token_env = load_kv_file(args.token_file)
    access_token = token_env.get("DOUYIN_OPEN_ACCESS_TOKEN", "")
    if not access_token:
        print("Missing DOUYIN_OPEN_ACCESS_TOKEN in token file", file=sys.stderr)
        return 1

    pay_start = parse_time(args.pay_time_start) if args.pay_time_start else None
    pay_end = parse_time(args.pay_time_end) if args.pay_time_end else None
    if args.live_started_at:
        pay_start = parse_time(args.live_started_at)
    if args.live_ended_at:
        pay_end = parse_time(args.live_ended_at)

    if args.create_time_start:
        create_start = parse_time(args.create_time_start)
    elif pay_start is not None:
        create_start = pay_start - args.create_buffer_before
    else:
        print("Need --create-time-start or --pay-time-start/--live-started-at", file=sys.stderr)
        return 1

    if args.create_time_end:
        create_end = parse_time(args.create_time_end)
    elif pay_end is not None:
        create_end = pay_end + args.create_buffer_after
    else:
        print("Need --create-time-end or --pay-time-end/--live-ended-at", file=sys.stderr)
        return 1

    if create_start >= create_end:
        print("create_time_start must be before create_time_end", file=sys.stderr)
        return 1

    order_status = normalize_order_status(args.order_status)

    app_env = load_kv_file(args.env_file) if args.env_file.is_file() else {}
    sys.path.insert(0, str(args.sdk_path))
    from doudian.core.DoudianOpConfig import GlobalConfig

    app_key = app_env.get("DOUYIN_OPEN_APP_KEY") or app_env.get("APP_KEY")
    app_secret = app_env.get("DOUYIN_OPEN_APP_SECRET") or app_env.get("APP_SECRET")
    if not app_key or not app_secret:
        print("Missing DOUYIN_OPEN_APP_KEY/SECRET in --env-file", file=sys.stderr)
        return 1
    GlobalConfig.appKey = app_key
    GlobalConfig.appSecret = app_secret

    print(
        "Fetching order.searchList\n"
        f"  create_time: {fmt_local(create_start)} .. {fmt_local(create_end)}\n"
        f"  pay filter:  "
        f"{fmt_local(pay_start) if pay_start else '(none)'} .. "
        f"{fmt_local(pay_end) if pay_end else '(none)'}"
    )

    raw_orders = fetch_orders(
        access_token=access_token,
        sdk_path=args.sdk_path,
        create_time_start=create_start,
        create_time_end=create_end,
        order_status=order_status,
        page_size=args.page_size,
    )

    pay_filter_stats = None
    create_filter_stats = None
    shop_filter_stats = None
    orders = raw_orders
    if not args.keep_unpaid and pay_start is not None and pay_end is not None:
        orders, pay_filter_stats = filter_orders_by_pay_time(raw_orders, pay_start, pay_end)
    elif (
        args.placed_in_live
        and not args.no_create_window_filter
        and pay_start is not None
        and pay_end is not None
    ):
        orders, create_filter_stats = filter_orders_by_create_time(orders, pay_start, pay_end)

    if args.shop_id.strip():
        orders, shop_filter_stats = filter_orders_by_shop_id(orders, args.shop_id)

    shop_meta = {
        "shop_id": token_env.get("DOUYIN_OPEN_SHOP_ID", ""),
        "shop_name": token_env.get("DOUYIN_OPEN_SHOP_NAME", ""),
        "erp_account": token_env.get("DOUYIN_OPEN_SHOP_NAME", ""),
    }

    payload: dict[str, Any] = {
        "fetched_at": datetime.now(CN_TZ).isoformat(),
        "api": "/order/searchList",
        "query": {
            "create_time_start": create_start,
            "create_time_end": create_end,
            "create_time_start_local": fmt_local(create_start),
            "create_time_end_local": fmt_local(create_end),
            "pay_time_start": pay_start,
            "pay_time_end": pay_end,
            "pay_time_start_local": fmt_local(pay_start) if pay_start else None,
            "pay_time_end_local": fmt_local(pay_end) if pay_end else None,
            "create_buffer_before_sec": args.create_buffer_before,
            "create_buffer_after_sec": args.create_buffer_after,
            "combine_status": {"order_status": order_status} if order_status else None,
            "keep_unpaid": args.keep_unpaid,
            "placed_in_live": args.placed_in_live,
            "shop_id_filter": args.shop_id.strip() or None,
            "create_window_filter": (
                args.placed_in_live
                and not args.no_create_window_filter
                and pay_start is not None
                and pay_end is not None
            ),
        },
        "stats": {
            "raw_order_count": len(raw_orders),
            "kept_order_count": len(orders),
            "pay_filter": pay_filter_stats,
            "create_filter": create_filter_stats,
            "shop_filter": shop_filter_stats,
        },
        "shops": [
            {
                **shop_meta,
                "pages": [
                    {
                        "page": 0,
                        "raw_keys": ["page", "total", "size", "shop_order_list"],
                        "payload": {
                            "page": 0,
                            "total": len(orders),
                            "size": len(orders),
                            "shop_order_list": orders,
                        },
                    }
                ],
            }
        ],
    }

    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(orders)} orders (raw={len(raw_orders)}) -> {args.output}")

    summary_path = args.summary_output or args.output.with_name(args.output.stem + "-summary.json")
    summary_payload = {
        "fetched_at": payload["fetched_at"],
        "api": payload["api"],
        "query": payload["query"],
        "stats": payload["stats"],
        "shops": [build_summary(shop_meta, orders)],
    }
    summary_path.write_text(json.dumps(summary_payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote summary -> {summary_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
