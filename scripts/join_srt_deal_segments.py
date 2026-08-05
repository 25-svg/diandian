#!/usr/bin/env python3
"""Join matched pay-time events with SRT evidence windows for review/training.

The output is order-anchored training reference.  It is not a script for a
host to read verbatim.  Payment time is the evidence anchor; transcript text
may have normal ASR timing drift.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


SIGNAL_TERMS = {
    "inquiry": ("多少钱", "什么价", "价格", "到手", "预算", "怎么卖"),
    "link": ("链接", "小黄车", "几号车", "几号链接", "置顶", "上车"),
    "conversion": ("已经拍", "已拍", "下单了", "付款了", "成交", "给你备注", "锁单"),
    "transition": ("下一个", "再看一款", "换一个", "转下一款", "再看看"),
}
TIMESTAMP_RE = re.compile(
    r"(?P<start>\d{2}:\d{2}:\d{2}[,.]\d{3})\s+-->\s+"
    r"(?P<end>\d{2}:\d{2}:\d{2}[,.]\d{3})"
)


@dataclass(frozen=True)
class Cue:
    start: float
    end: float
    text: str


def timestamp_to_seconds(value: str) -> float:
    hours, minutes, seconds_and_millis = value.replace(",", ".").split(":")
    seconds, millis = seconds_and_millis.split(".", 1)
    return int(hours) * 3600 + int(minutes) * 60 + int(seconds) + int(millis) / 1000


def parse_srt(path: Path) -> list[Cue]:
    """Parse normal SRT blocks.  Keep multi-line cue text as one string."""
    cues: list[Cue] = []
    for block in re.split(r"\r?\n\s*\r?\n", path.read_text(encoding="utf-8-sig")):
        lines = [line.strip() for line in block.splitlines() if line.strip()]
        timestamp_index = next(
            (index for index, line in enumerate(lines) if TIMESTAMP_RE.fullmatch(line)), None
        )
        if timestamp_index is None:
            continue
        match = TIMESTAMP_RE.fullmatch(lines[timestamp_index])
        assert match is not None
        text = " ".join(lines[timestamp_index + 1 :]).strip()
        if text:
            cues.append(
                Cue(
                    start=timestamp_to_seconds(match.group("start")),
                    end=timestamp_to_seconds(match.group("end")),
                    text=text,
                )
            )
    return cues


def cues_in_window(cues: Iterable[Cue], start_sec: float, end_sec: float) -> list[Cue]:
    return [cue for cue in cues if cue.end >= start_sec and cue.start <= end_sec]


def seconds_to_hms(value: float) -> str:
    total = max(0, round(value))
    hours, remainder = divmod(total, 3600)
    minutes, seconds = divmod(remainder, 60)
    return f"{hours}:{minutes:02d}:{seconds:02d}" if hours else f"{minutes}:{seconds:02d}"


def analyze_signals(cues: Iterable[Cue]) -> dict[str, Any]:
    text = " ".join(cue.text for cue in cues)
    inquiry_count = sum(text.count(term) for term in SIGNAL_TERMS["inquiry"])
    link_count = sum(text.count(term) for term in SIGNAL_TERMS["link"])
    conversion_count = sum(text.count(term) for term in SIGNAL_TERMS["conversion"])
    transition_count = sum(text.count(term) for term in SIGNAL_TERMS["transition"])
    label_parts = [
        f"问价×{inquiry_count}" if inquiry_count else "",
        f"链接×{link_count}" if link_count else "",
        f"成交确认×{conversion_count}" if conversion_count else "无成交确认",
        f"转品×{transition_count}" if transition_count else "",
    ]
    return {
        "inquiryCount": inquiry_count,
        "linkCount": link_count,
        "conversionConfirmationCount": conversion_count,
        "transitionCount": transition_count,
        "label": " / ".join(part for part in label_parts if part),
    }


def build_segments(
    events: Iterable[dict[str, Any]],
    cues: list[Cue],
    *,
    duration_sec: float,
    pre_sec: float = 90,
    post_sec: float = 30,
    clip_pre_sec: float = 120,
    clip_post_sec: float = 30,
) -> list[dict[str, Any]]:
    """Build one evidence segment for each SKU-level event."""
    segments: list[dict[str, Any]] = []
    for event in events:
        offset = float(event["offset_sec"])
        pitch_start = max(0.0, offset - pre_sec)
        pitch_end = max(pitch_start, offset - 5)
        confirm_start = max(0.0, offset - 5)
        confirm_end = min(duration_sec, offset + post_sec)
        clip_start = max(0.0, offset - clip_pre_sec)
        clip_end = min(duration_sec, offset + clip_post_sec)
        pitch_cues = cues_in_window(cues, pitch_start, pitch_end)
        confirm_cues = cues_in_window(cues, confirm_start, confirm_end)
        signal_cues = cues_in_window(cues, pitch_start, confirm_end)

        segment = {
            "order_id": event.get("order_id", ""),
            "product_name": event.get("product_name", ""),
            "pay_time_local": event.get("pay_time_local"),
            "pay_time_unix": event.get("pay_time"),
            "offset_sec": event.get("offset_sec"),
            "offset_hms": event.get("offset_hms") or seconds_to_hms(offset),
            "pay_amount_yuan": event.get("pay_amount_yuan"),
            "buyer_id": event.get("buyer_id", ""),
            "sku_index": event.get("sku_index"),
            "clip_start_sec": round(clip_start, 3),
            "clip_end_sec": round(clip_end, 3),
            "clip_start_hms": seconds_to_hms(clip_start),
            "clip_end_hms": seconds_to_hms(clip_end),
            "pitch_start_sec": round(pitch_start, 3),
            "pitch_end_sec": round(pitch_end, 3),
            "pitch_text": " ".join(cue.text for cue in pitch_cues),
            "confirm_text": " ".join(cue.text for cue in confirm_cues),
            "pitch_cue_count": len(pitch_cues),
            "confirm_cue_count": len(confirm_cues),
            "srt_missing": not pitch_cues and not confirm_cues,
            "signals": analyze_signals(signal_cues),
        }
        segments.append(segment)
    return segments


def load_events(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, list):
        return {}, payload
    return payload.get("summary") or {}, payload.get("events") or []


def write_csv(path: Path, segments: Iterable[dict[str, Any]]) -> None:
    fieldnames = [
        "order_id", "product_name", "pay_time_local", "offset_sec", "offset_hms",
        "pay_amount_yuan", "clip_start_sec", "clip_end_sec", "clip_start_hms",
        "clip_end_hms", "pitch_start_sec", "pitch_end_sec", "pitch_text", "confirm_text",
        "signals_label", "srt_missing",
    ]
    with path.open("w", newline="", encoding="utf-8-sig") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for segment in segments:
            writer.writerow({
                **{field: segment.get(field, "") for field in fieldnames},
                "signals_label": segment["signals"]["label"],
            })


def main() -> int:
    parser = argparse.ArgumentParser(description="Join payment events with SRT deal-evidence windows")
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--srt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--csv", type=Path, required=True)
    parser.add_argument("--pre-sec", type=float, default=90)
    parser.add_argument("--post-sec", type=float, default=30)
    parser.add_argument("--clip-pre-sec", type=float, default=120)
    parser.add_argument("--clip-post-sec", type=float, default=30)
    parser.add_argument("--duration-sec", type=float, help="Video duration; defaults to final SRT cue end")
    args = parser.parse_args()
    if not args.events.is_file() or not args.srt.is_file():
        parser.error("--events and --srt must both be existing files")

    summary, events = load_events(args.events)
    cues = parse_srt(args.srt)
    duration = args.duration_sec if args.duration_sec is not None else max((cue.end for cue in cues), default=0.0)
    segments = build_segments(
        events, cues, duration_sec=duration, pre_sec=args.pre_sec, post_sec=args.post_sec,
        clip_pre_sec=args.clip_pre_sec, clip_post_sec=args.clip_post_sec,
    )
    result = {
        "summary": {
            "live_started_at": summary.get("live_started_at"),
            "segment_count": len(segments),
            "srt_path": str(args.srt),
            "duration_sec": duration,
            "windows": {
                "pitch_pre_sec": args.pre_sec,
                "pitch_end_before_payment_sec": 5,
                "confirm_post_sec": args.post_sec,
                "clip_pre_sec": args.clip_pre_sec,
                "clip_post_sec": args.clip_post_sec,
            },
            "training_reference_only": True,
        },
        "segments": segments,
    }
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    write_csv(args.csv, segments)
    nonempty = sum(bool(segment["pitch_text"]) for segment in segments)
    print(f"Wrote {len(segments)} segments; pitch text present for {nonempty}/{len(segments)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
