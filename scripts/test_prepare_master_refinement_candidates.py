#!/usr/bin/env python3
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from join_srt_deal_segments import Cue
from prepare_master_refinement_candidates import build_candidates, classify_tier, search_product_speech


class PrepareMasterRefinementTests(unittest.TestCase):
    def test_osmo360_verified(self) -> None:
        cues = [
            Cue(560.0, 565.0, "这台大疆 Osmo 360 畅拍套装 99新 小黄车 1 号链接 2291"),
            Cue(900.0, 905.0, "尼康 D850 单反套机 今天特价"),
        ]
        cluster = search_product_speech(
            cues,
            product_name="99新 DJI/大疆 Osmo 360 【畅拍套装】",
            pay_offset_sec=589.0,
            pay_amount_yuan=2291.0,
        )
        assert cluster is not None
        tier = classify_tier(
            product_name="99新 DJI/大疆 Osmo 360 【畅拍套装】",
            speech_text=cluster.text,
            payment_window_text="",
            pay_amount_yuan=2291.0,
            cluster=cluster,
        )
        self.assertEqual(tier, "verified")

    def test_ricoh_mismatch_skipped(self) -> None:
        segments = [{
            "order_id": "1",
            "product_name": "99新 Ricoh/理光 GR2 GR3 GR3X GR4",
            "offset_sec": 47000,
            "pay_time_local": "2026-07-28 13:01:37",
            "pay_amount_yuan": 9219.0,
            "pitch_text": "尼康 D850 和 70-200 今天特价",
            "clip_start_sec": 46890,
            "clip_end_sec": 47130,
        }]
        cues = [Cue(46950.0, 46980.0, "尼康 D850 和 70-200 今天特价")]
        result = build_candidates(segments, cues, master_sections=[])
        self.assertEqual(result["summary"]["refinement_candidates"], 0)
        self.assertIn("unresolved", result["summary"]["skipped_breakdown"])

    def test_unique_by_product_dedupes(self) -> None:
        segments = [
            {
                "order_id": "a",
                "product_name": "99新 DJI/大疆 Osmo 360 【畅拍套装】",
                "offset_sec": 100.0,
                "pay_amount_yuan": 2291.0,
                "pitch_text": "大疆 360 畅拍 2291 小黄车",
            },
            {
                "order_id": "b",
                "product_name": "99新 DJI/大疆 Osmo 360 【畅拍套装】",
                "offset_sec": 500.0,
                "pay_amount_yuan": 2291.0,
                "pitch_text": "大疆 360 畅拍 2291 小黄车",
            },
        ]
        cues = [
            Cue(80.0, 90.0, "大疆 Osmo 360 畅拍 2291 小黄车"),
            Cue(480.0, 490.0, "大疆 Osmo 360 畅拍 2291 小黄车"),
        ]
        result = build_candidates(segments, cues, master_sections=[])
        self.assertEqual(len(result["unique_by_product"]), 1)


if __name__ == "__main__":
    unittest.main()
