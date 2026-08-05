#!/usr/bin/env python3
"""Resume order.searchList fetch for one shop/day with retries (系统繁忙).

Typical use after a partial 90-day pull:
  python scripts/doudian_resume_shop_fetch.py ^
    --merge-input d:\\Desktop\\order-searchList-all-20260729-152019.json ^
    --shop-id 212709966 ^
    --create-time-start "2026/07/28 00:00:00" ^
    --create-time-end "2026/07/29 00:00:00" ^
    --start-page 0 ^
    --output d:\\Desktop\\order-searchList-camera-20260728-resumed.json

Then export pay window:
  python scripts/doudian_export_pay_window.py --input ... --output ...
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from doudian_order_utils import (
    CN_TZ,
    dedupe_orders,
    fmt_local,
    iter_orders,
    load_kv_file,
)

DEFAULT_ENV = Path(r"d:\Desktop\.env")
DEFAULT_TOKEN = Path(r"d:\Desktop\doudian_token.env")
DEFAULT_SDK = Path(r"d:\Desktop\doudian-sdk-python-1.1.0-20260724091610\sdk-python")


class StaticAccessToken:
    def __init__(self, token: str) -> None:
        self._token = token

    def getAccessToken(self) -> str:
        return self._token


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
    raise ValueError(f"Unrecognized time: {value}")


def fetch_page_with_retry(
    *,
    access_token: str,
    sdk_path: Path,
    create_time_start: int,
    create_time_end: int,
    page: int,
    page_size: int,
    max_retries: int,
    retry_base_ms: int,
) -> dict[str, Any]:
    sys.path.insert(0, str(sdk_path))
    from doudian.api.order_searchList.OrderSearchListRequest import OrderSearchListRequest

    last_error = "unknown"
    for attempt in range(max_retries):
        request = OrderSearchListRequest()
        request.params.create_time_start = create_time_start
        request.params.create_time_end = create_time_end
        request.params.page = page
        request.params.size = page_size
        request.params.order_by = "create_time"
        request.params.order_asc = False

        response = request.execute(StaticAccessToken(access_token))
        if response.isSuccess():
            return response.data or {}

        last_error = (
            f"code={response.code} msg={response.msg} "
            f"sub_code={response.sub_code} sub_msg={response.sub_msg}"
        )
        if attempt + 1 < max_retries:
            delay = retry_base_ms * (attempt + 1) / 1000.0
            print(f"page {page} failed ({last_error}); retry in {delay:.1f}s", file=sys.stderr)
            time.sleep(delay)

    raise RuntimeError(f"order.searchList page {page} failed after retries: {last_error}")


def fetch_shop_orders(
    *,
    access_token: str,
    sdk_path: Path,
    create_time_start: int,
    create_time_end: int,
    page_size: int,
    start_page: int,
    max_pages: int | None,
    max_retries: int,
    retry_base_ms: int,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    orders: list[dict[str, Any]] = []
    page = start_page
    pages_fetched = 0
    total = None
    last_error = None

    while True:
        if max_pages is not None and pages_fetched >= max_pages:
            break
        try:
            data = fetch_page_with_retry(
                access_token=access_token,
                sdk_path=sdk_path,
                create_time_start=create_time_start,
                create_time_end=create_time_end,
                page=page,
                page_size=page_size,
                max_retries=max_retries,
                retry_base_ms=retry_base_ms,
            )
        except RuntimeError as error:
            last_error = str(error)
            break

        batch = data.get("shop_order_list") or []
        total = data.get("total", total)
        orders.extend(batch)
        pages_fetched += 1
        print(f"page {page}: +{len(batch)} orders (total={total}, collected={len(orders)})")

        if not batch:
            break
        if total is not None and len(orders) + start_page * page_size >= int(total):
            # We only know per-page progress; stop when batch smaller or total reached heuristically
            if len(orders) >= int(total):
                break
        if len(batch) < page_size:
            break
        page += 1
        time.sleep(0.25)

    meta = {
        "api_total": total,
        "pages_fetched": pages_fetched,
        "start_page": start_page,
        "last_page": page,
        "error": last_error,
    }
    return orders, meta


def main() -> int:
    parser = argparse.ArgumentParser(description="Resume Doudian order.searchList for one shop")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV)
    parser.add_argument("--token-file", type=Path, default=DEFAULT_TOKEN)
    parser.add_argument("--sdk-path", type=Path, default=DEFAULT_SDK)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--merge-input", type=Path, help="Existing JSON to merge/dedupe")
    parser.add_argument("--shop-id", default="212709966")
    parser.add_argument("--shop-name", default="金典拍拍相机专卖店")
    parser.add_argument("--create-time-start", default="2026/07/28 00:00:00")
    parser.add_argument("--create-time-end", default="2026/07/29 00:00:00")
    parser.add_argument("--start-page", type=int, default=0)
    parser.add_argument("--page-size", type=int, default=100)
    parser.add_argument("--max-pages", type=int, default=0, help="0 = no limit")
    parser.add_argument("--max-retries", type=int, default=6)
    parser.add_argument("--retry-base-ms", type=int, default=800)
    args = parser.parse_args()

    if not args.sdk_path.is_dir():
        print(f"SDK not found: {args.sdk_path}", file=sys.stderr)
        return 1
    if not args.token_file.is_file():
        print(f"Token file not found: {args.token_file}", file=sys.stderr)
        return 1

    token_env = load_kv_file(args.token_file)
    access_token = token_env.get("DOUYIN_OPEN_ACCESS_TOKEN", "")
    if not access_token:
        print("Missing DOUYIN_OPEN_ACCESS_TOKEN", file=sys.stderr)
        return 1

    app_env = load_kv_file(args.env_file) if args.env_file.is_file() else {}
    sys.path.insert(0, str(args.sdk_path))
    from doudian.core.DoudianOpConfig import GlobalConfig

    app_key = app_env.get("DOUYIN_OPEN_APP_KEY") or app_env.get("APP_KEY")
    app_secret = app_env.get("DOUYIN_OPEN_APP_SECRET") or app_env.get("APP_SECRET")
    if not app_key or not app_secret:
        print("Missing DOUYIN_OPEN_APP_KEY/SECRET", file=sys.stderr)
        return 1
    GlobalConfig.appKey = app_key
    GlobalConfig.appSecret = app_secret

    create_start = parse_time(args.create_time_start)
    create_end = parse_time(args.create_time_end)
    max_pages = None if args.max_pages <= 0 else args.max_pages

    print(
        "Resuming order.searchList\n"
        f"  shop_id: {args.shop_id}\n"
        f"  create_time: {fmt_local(create_start)} .. {fmt_local(create_end)}\n"
        f"  start_page: {args.start_page}  page_size: {args.page_size}"
    )

    fetched, fetch_meta = fetch_shop_orders(
        access_token=access_token,
        sdk_path=args.sdk_path,
        create_time_start=create_start,
        create_time_end=create_end,
        page_size=args.page_size,
        start_page=args.start_page,
        max_pages=max_pages,
        max_retries=args.max_retries,
        retry_base_ms=args.retry_base_ms,
    )

    merged = fetched
    if args.merge_input and args.merge_input.is_file():
        prior_payload = json.loads(args.merge_input.read_text(encoding="utf-8"))
        prior = [
            order
            for order in iter_orders(prior_payload)
            if str(order.get("shop_id", "")) == args.shop_id
        ]
        merged = dedupe_orders(prior + fetched)
        print(f"Merged with {len(prior)} prior shop orders -> {len(merged)} unique")

    shop_orders = [order for order in merged if str(order.get("shop_id", "")) == args.shop_id]
    if not shop_orders and merged:
        shop_orders = merged

    payload: dict[str, Any] = {
        "fetched_at": datetime.now(CN_TZ).isoformat(),
        "api": "/order/searchList",
        "mode": "resume_shop_day",
        "create_time_start": create_start,
        "create_time_end": create_end,
        "create_time_start_local": fmt_local(create_start),
        "create_time_end_local": fmt_local(create_end),
        "combine_status": None,
        "shops": [
            {
                "shop_id": args.shop_id,
                "shop_name": args.shop_name,
                "api_total": fetch_meta.get("api_total"),
                "order_count": len(shop_orders),
                "pages_fetched": fetch_meta.get("pages_fetched"),
                "start_page": fetch_meta.get("start_page"),
                "error": fetch_meta.get("error"),
                "orders": shop_orders,
            }
        ],
    }

    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    summary_path = args.output.with_name(args.output.stem + "-summary.json")
    summary_path.write_text(
        json.dumps(
            {
                "fetched_at": payload["fetched_at"],
                "shop_id": args.shop_id,
                "shop_name": args.shop_name,
                "order_count": len(shop_orders),
                **fetch_meta,
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"Wrote {len(shop_orders)} orders -> {args.output}")
    if fetch_meta.get("error"):
        print(f"WARNING: fetch ended early: {fetch_meta['error']}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
