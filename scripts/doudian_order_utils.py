"""Shared helpers for Doudian order.searchList JSON files."""

from __future__ import annotations

import json
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any

CN_TZ = timezone(timedelta(hours=8))

ORDER_STATUS_LABELS = {
    "1": "待支付",
    "2": "待发货",
    "3": "已发货",
    "4": "已关闭",
    "5": "已完成",
}


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


def fmt_local(ts: int) -> str:
    return datetime.fromtimestamp(ts, CN_TZ).strftime("%Y-%m-%d %H:%M:%S")


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


def iter_orders(payload: dict[str, Any]) -> list[dict[str, Any]]:
    orders: list[dict[str, Any]] = []
    for shop in payload.get("shops") or []:
        orders.extend(shop.get("orders") or [])
        for page in shop.get("pages") or []:
            page_payload = page.get("payload") or {}
            orders.extend(page_payload.get("shop_order_list") or [])
    if payload.get("shop_order_list"):
        orders.extend(payload["shop_order_list"])
    return orders


def load_orders(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload, iter_orders(payload)


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


def pick_product_summary(order: dict[str, Any]) -> str:
    names: list[str] = []
    for sku in order.get("sku_order_list") or []:
        name = (sku.get("product_name") or sku.get("product_name_str") or "").strip()
        if name:
            names.append(name)
    return " | ".join(names[:3])


def pick_room_id(order: dict[str, Any]) -> str:
    for sku in order.get("sku_order_list") or []:
        room = sku.get("room_id_str") or sku.get("room_id")
        if room not in (None, "", "0", 0):
            return str(room)
    return str(order.get("room_id_str") or order.get("room_id") or "")


def order_status_label(order: dict[str, Any]) -> str:
    code = str(order.get("order_status", ""))
    return ORDER_STATUS_LABELS.get(code, code or "未知")


def slim_order_row(order: dict[str, Any]) -> dict[str, Any]:
    pay_time = pick_pay_time(order)
    create_time = pick_create_time(order)
    return {
        "order_id": str(order.get("order_id", "")),
        "shop_id": str(order.get("shop_id", "")),
        "order_status": order.get("order_status"),
        "order_status_label": order_status_label(order),
        "create_time": create_time,
        "create_time_local": fmt_local(create_time) if create_time else None,
        "pay_time": pay_time,
        "pay_time_local": fmt_local(pay_time) if pay_time else None,
        "pay_amount_fen": int(order.get("pay_amount") or 0),
        "pay_amount_yuan": round(int(order.get("pay_amount") or 0) / 100, 2),
        "sku_count": len(order.get("sku_order_list") or []),
        "product_summary": pick_product_summary(order),
        "room_id": pick_room_id(order),
    }


def dedupe_orders(orders: list[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: set[str] = set()
    kept: list[dict[str, Any]] = []
    for order in orders:
        order_id = str(order.get("order_id", ""))
        if not order_id or order_id in seen:
            continue
        seen.add(order_id)
        kept.append(order)
    return kept
