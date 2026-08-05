# Fast TS Playback Conversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make compatible TS files playable immediately through fast MP4 remuxing and make unavoidable re-encoding choose verified hardware with useful progress.

**Architecture:** The video handler probes once and classifies playback work before any ASR wait. Compatible H.264/browser-audio sources remux immediately; all other sources retain the exclusive media gate and call the FFmpeg hardware-selector-backed re-encoder. FFmpeg progress formatting receives the already-probed duration.

**Tech Stack:** Rust, Tauri, FFmpeg/ffprobe, existing `hwaccel` selector, recorder task queue.

## Global Constraints

- Do not lower resolution, crop, or delete original input files.
- H.264 plus AAC/MP3/MP2 must use `-c copy` and MP4 faststart before waiting for ASR.
- Re-encoding must remain serialized with ASR through `media_execution_gate`.
- Re-encoding must use a verified available encoder and fall back to `libx264` if the selected encoder fails.
- Output remains `.playable.mp4` and is exposed only after successful completion.

---

### Task 1: Classify fast playback conversion before queueing

**Files:**
- Modify: `src-tauri/src/handlers/video.rs`
- Test: existing `import_video_compatibility_tests` module in `src-tauri/src/handlers/video.rs`

**Interfaces:**
- Consumes: `ffmpeg::VideoMetadata`.
- Produces: `playback_conversion_kind(&VideoMetadata) -> PlaybackConversionKind`, where `FastRemux` applies only to browser-playable video and audio codecs.

- [ ] **Step 1: Write the failing test**

```rust
assert_eq!(
    playback_conversion_kind(&metadata("h264", "aac")),
    PlaybackConversionKind::FastRemux,
);
assert_eq!(
    playback_conversion_kind(&metadata("hevc", "aac")),
    PlaybackConversionKind::Reencode,
);
```

- [ ] **Step 2: Run RED**

Run: `cargo test import_video_compatibility_tests::h264_aac_playback_uses_fast_remux`

Expected: FAIL because the classifier does not exist.

- [ ] **Step 3: Implement and use the classifier**

Probe at the start of `prepare_video_playback`. Run `remux_mp4_faststart` immediately for `FastRemux`; only `Reencode` waits for an active subtitle task and acquires `media_execution_gate`. A remux failure falls through to the guarded re-encode path.

- [ ] **Step 4: Run GREEN**

Run: `cargo test import_video_compatibility_tests::h264_aac_playback_uses_fast_remux`

Expected: PASS.

### Task 2: Use verified hardware encoder and report real progress

**Files:**
- Modify: `src-tauri/src/ffmpeg/mod.rs`
- Modify: `src-tauri/src/ffmpeg/hwaccel.rs`
- Test: `src-tauri/src/ffmpeg/mod.rs` unit tests

**Interfaces:**
- Consumes: source duration, FFmpeg processed time, and `hwaccel::get_x264_encoder()`.
- Produces: `format_conversion_progress(processed_secs, total_secs, elapsed_secs, mode_name) -> String` and `try_browser_compatible_conversion(..., audio_copy: bool)`.

- [ ] **Step 1: Write failing progress test**

```rust
assert_eq!(
    format_conversion_progress(300.0, 600.0, 100.0, "硬件转换"),
    "转换 50% · 3.0x · 预计剩余 1分40秒（硬件转换）",
);
```

- [ ] **Step 2: Run RED**

Run: `cargo test ffmpeg::tests::conversion_progress_shows_percent_speed_and_eta`

Expected: FAIL because the formatter does not exist.

- [ ] **Step 3: Implement minimal encoder and progress changes**

Use `get_x264_encoder()` for browser conversion. Preserve AMF `-quality speed`; use existing helper quality arguments for other verified encoders; fall back to the current `libx264 veryfast CRF 20` command if hardware conversion fails. Copy compatible audio when `audio_copy` is true. Update the playback conversion FFmpeg runner with duration-based percent, media-time speed, and ETA.

- [ ] **Step 4: Run focused verification**

Run: `cargo test conversion_progress_shows_percent_speed_and_eta && cargo test import_video_compatibility_tests`

Expected: PASS.

### Task 3: Build and measure one local TS

**Files:** No source changes.

- [ ] **Step 1: Build desktop backend**

Run: `cargo build`

Expected: PASS.

- [ ] **Step 2: Verify a compatible TS path**

Use `Z:\bsr-clips\[imported] no desc20260722_121801.ts` when its codec is H.264/AAC. Expect task log to report fast remux and no ASR wait. If it is not compatible, record the selected hardware encoder and progress output instead.

- [ ] **Step 3: Restart the desktop app**

Restart `src-tauri\target\debug\bili-shadowreplay.exe` after the build completes.
