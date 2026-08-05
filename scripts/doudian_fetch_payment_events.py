#!/usr/bin/env python3
"""Fetch order.searchList and emit payment-events JSON in one step.

Usage:
  python scripts/doudian_fetch_payment_events.py ^
    --live-started-at "2026/07/28 08:15:49" ^
    --live-ended-at "2026/07/28 15:44:39" ^
    --output payment-events.json

Stdout receives only JSON when --output is omitted.
Progress logs go to stderr.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from doudian_fetch_orders import (  # noqa: E402
    DEFAULT_ENV,
    DEFAULT_SDK,
    DEFAULT_TOKEN,
    fetch_orders,
    filter_orders_by_pay_time,
    fmt_local,
    load_kv_file,
    normalize_order_status,
    parse_time,
)
from doudian_order_events import extract_events, summarize  # noqa: E402

CN_TZ = timezone(timedelta(hours=8))


def configure_sdk(env_file: Path, sdk_path: Path) -> None:
    app_env = load_kv_file(env_file) if env_file.is_file() else {}
    sys.path.insert(0, str(sdk_path))
    from doudian.core.DoudianOpConfig import GlobalConfig

    app_key = app_env.get("DOUYIN_OPEN_APP_KEY") or app_env.get("APP_KEY")
    app_secret = app_env.get("DOUYIN_OPEN_APP_SECRET") or app_env.get("APP_SECRET")
    if not app_key or not app_secret:
        raise RuntimeError(f"Missing DOUYIN_OPEN_APP_KEY/SECRET in {env_file}")
    GlobalConfig.appKey = app_key
    GlobalConfig.appSecret = app_secret


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch Doudian orders and build payment-events JSON")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV)
    parser.add_argument("--token-file", type=Path, default=DEFAULT_TOKEN)
    parser.add_argument("--sdk-path", type=Path, default=DEFAULT_SDK)
    parser.add_argument("--live-started-at", required=True)
    parser.add_argument("--live-ended-at", required=True)
    parser.add_argument("--shop-id", default="")
    parser.add_argument("--shop-name", default="")
    parser.add_argument("--create-buffer-before", type=int, default=259200)
    parser.add_argument("--create-buffer-after", type=int, default=86400)
    parser.add_argument("--page-size", type=int, default=50)
    parser.add_argument("--output", type=Path, help="Write JSON here; default stdout")
    args = parser.parse_args()

    if not args.sdk_path.is_dir():
        print(f"SDK not found: {args.sdk_path}", file=sys.stderr)
        return 1
    if not args.token_file.is_file():
        print(f"Token file not found: {args.token_file}", file=sys.stderr)
        print("Run scripts/doudian_get_token.py first.", file=sys.stderr)
        return 1
    if not args.env_file.is_file():
        print(f"Env file not found: {args.env_file}", file=sys.stderr)
        return 1

    pay_start = parse_time(args.live_started_at)
    pay_end = parse_time(args.live_ended_at)
    if pay_start >= pay_end:
        print("live-started-at must be before live-ended-at", file=sys.stderr)
        return 1

    create_start = pay_start - args.create_buffer_before
    create_end = pay_end + args.create_buffer_after

    token_env = load_kv_file(args.token_file)
    access_token = token_env.get("DOUYIN_OPEN_ACCESS_TOKEN", "")
    if not access_token:
        print("Missing DOUYIN_OPEN_ACCESS_TOKEN in token file", file=sys.stderr)
        return 1

    configure_sdk(args.env_file, args.sdk_path)

    print(
        "Fetching order.searchList\n"
        f"  live window: {fmt_local(pay_start)} .. {fmt_local(pay_end)}\n"
        f"  create_time: {fmt_local(create_start)} .. {fmt_local(create_end)}",
        file=sys.stderr,
    )

    raw_orders = fetch_orders(
        access_token=access_token,
        sdk_path=args.sdk_path,
        create_time_start=create_start,
        create_time_end=create_end,
        order_status=normalize_order_status("2,3,5"),
        page_size=args.page_size,
    )
    orders, pay_filter_stats = filter_orders_by_pay_time(raw_orders, pay_start, pay_end)

    shop_id = (args.shop_id or token_env.get("DOUYIN_OPEN_SHOP_ID", "")).strip()
    shop_name = (args.shop_name or token_env.get("DOUYIN_OPEN_SHOP_NAME", "")).strip()
    if shop_id:
        orders = [order for order in orders if str(order.get("shop_id", "")) == shop_id]

    import tempfile

    with tempfile.NamedTemporaryFile(
        mode="w",
        suffix=".json",
        delete=False,
        encoding="utf-8",
    ) as handle:
        json.dump(
            {
                "shops": [
                    {
                        "shop_id": shop_id,
                        "shop_name": shop_name,
                        "orders": orders,
                    }
                ]
            },
            handle,
            ensure_ascii=False,
        )
        orders_path = Path(handle.name)

    try:
        events, shop_meta = extract_events(
            orders_path,
            pay_start,
            shop_id=shop_id or None,
            shop_name=shop_name or None,
        )
    finally:
        orders_path.unlink(missing_ok=True)

    result: dict[str, Any] = {
        "fetched_at": datetime.now(CN_TZ).isoformat(),
        "source_label": "order.searchList",
        "summary": summarize(events, pay_start, shop_meta),
        "events": events,
        "stats": {
            "raw_order_count": len(raw_orders),
            "kept_order_count": len(orders),
            "event_count": len(events),
            "pay_filter": pay_filter_stats,
            "query": {
                "live_started_at": fmt_local(pay_start),
                "live_ended_at": fmt_local(pay_end),
                "create_time_start_local": fmt_local(create_start),
                "create_time_end_local": fmt_local(create_end),
                "shop_id": shop_id or None,
            },
        },
    }

    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")
        print(f"Wrote {len(events)} payment events -> {args.output}", file=sys.stderr)
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
