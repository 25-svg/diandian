#!/usr/bin/env python3
"""Prepare master-script refinement candidates from order deal segments + SRT.

Only segments with product-matched speech (verified / weak) should feed master
upgrade.  Payment-window text alone is kept for debug as payment_window_text.

Usage:
  python scripts/prepare_master_refinement_candidates.py ^
    --deal-segments "d:\\Desktop\\deal-segments-20260728-v4.json" ^
    --srt "D:\\cache\\...\\subtitle.srt" ^
    --master-baseline "d:\\Desktop\\master-baseline-export.json" ^
    --output "d:\\Desktop\\master-refinement-candidates.json"
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from join_srt_deal_segments import Cue, analyze_signals, cues_in_window, parse_srt, seconds_to_hms


BRAND_ALIASES = {
    "canon": ("佳能", "canon"),
    "sony": ("索尼", "sony"),
    "nikon": ("尼康", "nikon"),
    "fujifilm": ("富士", "fujifilm", "富士胶片"),
    "dji": ("大疆", "dji"),
    "ricoh": ("理光", "ricoh"),
    "sigma": ("适马", "sigma"),
    "insta": ("影石", "insta", "360"),
    "panasonic": ("松下", "panasonic"),
    "tamron": ("腾龙", "tamron"),
}


def normalize_text(value: str) -> str:
    return re.sub(r"[\s/\\|·\-_（）()【】\[\],，.+®]", "", (value or "").lower())


def product_tokens(product_name: str) -> set[str]:
    text = normalize_text(product_name)
    tokens = set(re.findall(r"[a-z0-9\u4e00-\u9fff]{2,}", text))
    stop = {"99新", "准新品", "微单", "数码相机", "镜头", "相机", "高清", "专业", "二代", "标准套装", "畅拍套装"}
    return {token for token in tokens if token not in stop and len(token) >= 2}


def detect_brand(product_name: str) -> str | None:
    lowered = product_name.lower()
    for brand, aliases in BRAND_ALIASES.items():
        if any(alias in lowered or alias in product_name for alias in aliases):
            return brand
    return None


def dominant_brand_in_text(text: str) -> str | None:
    lowered = text.lower()
    hits = []
    for brand, aliases in BRAND_ALIASES.items():
        if any(alias in lowered or alias in text for alias in aliases):
            hits.append(brand)
    if len(hits) == 1:
        return hits[0]
    return None


def price_close(text: str, target_yuan: float | None, *, tolerance: float = 80.0) -> bool:
    if target_yuan is None or target_yuan <= 0:
        return False
    for match in re.finditer(r"(\d{3,5})", text.replace(",", "")):
        value = float(match.group(1))
        if abs(value - target_yuan) <= tolerance:
            return True
    return False


def model_hit(text: str, tokens: set[str]) -> bool:
    normalized = normalize_text(text)
    hits = [token for token in tokens if len(token) >= 2 and token in normalized]
    if not hits:
        return False
    if len(tokens) <= 4:
        return len(hits) >= 1
    return len(hits) >= 2


@dataclass
class SpeechCluster:
    start_sec: float
    end_sec: float
    text: str
    score: float
    timing_relation: str


def search_product_speech(
    cues: list[Cue],
    *,
    product_name: str,
    pay_offset_sec: float,
    pay_amount_yuan: float | None,
    pre_pay_window_sec: float = 1800,
    post_pay_window_sec: float = 120,
) -> SpeechCluster | None:
    tokens = product_tokens(product_name)
    brand = detect_brand(product_name)
    if not tokens:
        return None

    window_start = max(0.0, pay_offset_sec - pre_pay_window_sec)
    window_end = pay_offset_sec + post_pay_window_sec
    candidates: list[SpeechCluster] = []

    for cue in cues:
        if cue.end < window_start or cue.start > window_end:
            continue
        text = cue.text
        score = 0.0
        if model_hit(text, tokens):
            score += 3.0
        if brand_mentioned(text, brand):
            score += 2.0
        elif brand and dominant_brand_in_text(text) not in (None, brand):
            score -= 2.0
        if price_close(text, pay_amount_yuan):
            score += 2.0
        if any(term in text for term in ("小黄车", "链接", "号链接", "上车")):
            score += 1.0
        if score <= 0:
            continue
        distance = abs(pay_offset_sec - cue.end)
        if cue.end <= pay_offset_sec:
            timing_relation = "pre_pay"
            score += max(0.0, 2.0 - distance / 600.0)
        else:
            timing_relation = "post_pay"
            score += max(0.0, 1.0 - distance / 300.0)
        cluster_end = cue.end
        cluster_start = cue.start
        cluster_text = text
        candidates.append(
            SpeechCluster(cluster_start, cluster_end, cluster_text, score, timing_relation)
        )

    if not candidates:
        return None
    best = max(candidates, key=lambda item: item.score)
    pad = 15.0
    return SpeechCluster(
        max(0.0, best.start_sec - pad),
        best.end_sec + pad,
        best.text,
        best.score,
        best.timing_relation,
    )


def brand_mentioned(text: str, brand: str | None) -> bool:
    if not brand:
        return False
    lowered = text.lower()
    return any(alias in lowered or alias in text for alias in BRAND_ALIASES[brand])


def classify_tier(
    *,
    product_name: str,
    speech_text: str,
    payment_window_text: str,
    pay_amount_yuan: float | None,
    cluster: SpeechCluster | None,
) -> str:
    if cluster is None or not cluster.text.strip():
        return "unresolved"
    tokens = product_tokens(product_name)
    brand = detect_brand(product_name)
    has_model = model_hit(speech_text, tokens)
    has_brand = brand_mentioned(speech_text, brand)
    has_price = price_close(speech_text, pay_amount_yuan)
    has_link = any(term in speech_text for term in ("小黄车", "链接", "号链接"))
    if has_brand and has_model and (has_price or has_link):
        return "verified"
    if has_brand and has_price and has_link:
        return "verified"
    if has_model or has_brand:
        return "weak"
    payment_brand = dominant_brand_in_text(payment_window_text)
    if brand and payment_brand and payment_brand != brand:
        return "mismatch"
    if any(term in speech_text for term in ("下单", "备注", "发货", "链接", "小黄车")):
        return "generic"
    return "unresolved"


def match_master_section(
    product_name: str,
    sections: list[dict[str, Any]],
) -> dict[str, Any] | None:
    tokens = product_tokens(product_name)
    best: tuple[float, dict[str, Any]] | None = None
    for section in sections:
        if section.get("kind") != "product":
            continue
        hay = normalize_text(
            " ".join(
                str(section.get(field) or "")
                for field in ("title", "productCardId", "hostText", "masterText")
            )
        )
        overlap = sum(1 for token in tokens if len(token) >= 3 and token in hay)
        if overlap <= 0:
            continue
        score = float(overlap)
        if best is None or score > best[0]:
            best = (score, section)
    return best[1] if best else None


def load_master_sections(path: Path | None) -> list[dict[str, Any]]:
    if path is None or not path.is_file():
        return []
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload.get("sections") or []


def build_candidates(
    segments: list[dict[str, Any]],
    cues: list[Cue],
    master_sections: list[dict[str, Any]],
) -> dict[str, Any]:
    refined: list[dict[str, Any]] = []
    skipped: dict[str, int] = {}

    for segment in segments:
        pay_offset = float(segment.get("offset_sec") or 0)
        payment_window_text = str(segment.get("pitch_text") or "")
        cluster = search_product_speech(
            cues,
            product_name=str(segment.get("product_name") or ""),
            pay_offset_sec=pay_offset,
            pay_amount_yuan=segment.get("pay_amount_yuan"),
        )
        speech_text = cluster.text if cluster else ""
        tier = classify_tier(
            product_name=str(segment.get("product_name") or ""),
            speech_text=speech_text,
            payment_window_text=payment_window_text,
            pay_amount_yuan=segment.get("pay_amount_yuan"),
            cluster=cluster,
        )
        if tier in {"mismatch", "unresolved", "generic"}:
            skipped[tier] = skipped.get(tier, 0) + 1
            continue
        if tier == "weak":
            skipped["weak_pending_review"] = skipped.get("weak_pending_review", 0) + 1

        master_section = match_master_section(str(segment.get("product_name") or ""), master_sections)
        clip_start = cluster.start_sec if cluster else segment.get("clip_start_sec", 0)
        clip_end = cluster.end_sec if cluster else segment.get("clip_end_sec", 0)
        refined.append(
            {
                "orderId": segment.get("order_id"),
                "productName": segment.get("product_name"),
                "payTimeLocal": segment.get("pay_time_local"),
                "payAmountYuan": segment.get("pay_amount_yuan"),
                "verificationTier": tier,
                "timingRelation": cluster.timing_relation if cluster else None,
                "speechStartSec": round(clip_start, 3),
                "speechEndSec": round(clip_end, 3),
                "speechStartHms": seconds_to_hms(clip_start),
                "speechEndHms": seconds_to_hms(clip_end),
                "speechText": speech_text,
                "paymentWindowText": payment_window_text,
                "signals": analyze_signals(cues_in_window(cues, clip_start, clip_end)),
                "masterSectionId": master_section.get("id") if master_section else None,
                "masterSectionTitle": master_section.get("title") if master_section else None,
                "masterProductCardId": master_section.get("productCardId") if master_section else None,
                "recommendedAction": "compare_to_master" if tier == "verified" else "manual_review",
            }
        )

    by_product: dict[str, dict[str, Any]] = {}
    for item in refined:
        key = str(item.get("masterProductCardId") or item.get("productName"))
        existing = by_product.get(key)
        if existing is None or item["verificationTier"] == "verified":
            by_product[key] = item

    return {
        "summary": {
            "input_segments": len(segments),
            "refinement_candidates": len(refined),
            "unique_products": len(by_product),
            "skipped_breakdown": skipped,
            "verified_only_for_auto_master_upgrade": True,
        },
        "candidates": refined,
        "unique_by_product": sorted(by_product.values(), key=lambda row: row.get("speechStartSec") or 0),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Prepare master refinement candidates from deal segments")
    parser.add_argument("--deal-segments", type=Path, required=True)
    parser.add_argument("--srt", type=Path, required=True)
    parser.add_argument("--master-baseline", type=Path, help="JSON export from get_master_baseline")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    payload = json.loads(args.deal_segments.read_text(encoding="utf-8"))
    segments = payload.get("segments") or payload
    cues = parse_srt(args.srt)
    master_sections = load_master_sections(args.master_baseline)
    result = build_candidates(segments, cues, master_sections)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    summary = result["summary"]
    print(
        f"Candidates: {summary['refinement_candidates']} "
        f"(unique products {summary['unique_products']}) "
        f"from {summary['input_segments']} deal segments"
    )
    print(f"Skipped: {summary['skipped_breakdown']}")
    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
