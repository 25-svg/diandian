#!/usr/bin/env python3
"""Aggregate payment events into per-minute deal buckets.

Usage:
  python scripts/doudian_aggregate_deal_minutes.py ^
    --events "d:\\Desktop\\payment-events-20260728-v4.json" ^
    --output "d:\\Desktop\\deal-minutes-20260728.json"
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def aggregate(events: list[dict[str, Any]]) -> list[dict[str, Any]]:
    buckets: dict[int, dict[str, Any]] = {}
    for event in events:
        offset_sec = int(event.get("offset_sec", -1))
        if offset_sec < 0:
            continue
        minute_index = offset_sec // 60
        bucket = buckets.setdefault(
            minute_index,
            {
                "minute_index": minute_index,
                "offset_sec": minute_index * 60,
                "order_count": 0,
                "total_pay_amount_fen": 0,
            },
        )
        bucket["order_count"] += 1
        amount = event.get("pay_amount_fen")
        if isinstance(amount, (int, float)) and amount > 0:
            bucket["total_pay_amount_fen"] += int(amount)
    return [buckets[key] for key in sorted(buckets)]


def main() -> int:
    parser = argparse.ArgumentParser(description="Aggregate payment events by minute")
    parser.add_argument("--events", type=Path, required=True, help="payment-events JSON from doudian_order_events.py")
    parser.add_argument("--output", type=Path, help="Write minute buckets JSON (default: stdout)")
    args = parser.parse_args()

    if not args.events.is_file():
        print(f"Events file not found: {args.events}", file=sys.stderr)
        return 1

    payload = json.loads(args.events.read_text(encoding="utf-8"))
    events = payload.get("events") if isinstance(payload, dict) else payload
    if not isinstance(events, list):
        print("Input must contain an events[] array", file=sys.stderr)
        return 1

    minutes = aggregate(events)
    result = {
        "summary": payload.get("summary") if isinstance(payload, dict) else None,
        "minute_buckets": minutes,
    }
    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")
        print(f"Wrote {len(minutes)} minute buckets to {args.output}")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
