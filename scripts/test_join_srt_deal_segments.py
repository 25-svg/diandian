from join_srt_deal_segments import build_segments, parse_srt


def test_build_segments_joins_pitch_and_clamps_clip_bounds(tmp_path):
    srt_path = tmp_path / "subtitle.srt"
    srt_path.write_text(
        """1
00:00:12,000 --> 00:00:20,000
这台 Ricoh GR 现在什么价

2
00:01:30,000 --> 00:01:34,000
链接已经上了，喜欢直接拍

3
00:01:36,000 --> 00:01:42,000
好的已经拍下，给你备注
""",
        encoding="utf-8",
    )
    events = [
        {
            "order_id": "order-1",
            "product_name": "Ricoh GR",
            "pay_time_local": "2026-07-28 08:17:29",
            "pay_amount_yuan": 123.45,
            "offset_sec": 100,
            "offset_hms": "1:40",
        }
    ]

    segments = build_segments(events, parse_srt(srt_path), duration_sec=110)

    assert len(segments) == 1
    segment = segments[0]
    assert segment["pitch_start_sec"] == 10
    assert segment["pitch_end_sec"] == 95
    assert "Ricoh GR" in segment["pitch_text"]
    assert "链接已经上了" in segment["pitch_text"]
    assert "已经拍下" in segment["confirm_text"]
    assert segment["clip_start_sec"] == 0
    assert segment["clip_end_sec"] == 110
    assert segment["signals"]["inquiryCount"] == 1
    assert segment["signals"]["linkCount"] == 1
    # Mirrors transcriptSignals.ts: "已经拍下" contains both "已经拍" and "已拍".
    assert segment["signals"]["conversionConfirmationCount"] == 2
