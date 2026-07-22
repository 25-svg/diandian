# Task 4 SDD Report

## Scope

Focused implementation of recoverable master transcript ingestion in the lightweight
`master-ingest` crate. Root desktop adapters remain thin and no real media is processed.

## RED Evidence

1. `cargo test -p master-ingest --test master_ingest_tests`
   - Failed as expected with `E0432` because chunk planning, parameter-card selection,
     checkpoint orchestration, artifact persistence, and related public contracts did not exist.
   - The command used `%TEMP%\bili-shadowreplay-task4-target` as `CARGO_TARGET_DIR` and did not
     compile the root desktop, Tauri, V8, or Whisper targets.

## GREEN Evidence

1. `cargo test -p master-ingest`
   - Passed: 6 tests, 0 failed.
   - Covers ten-minute planning, absolute SRT offsets and overlap dedupe, deterministic
     parameter-card ranking and cap, input hashing, failed chunk resume, checkpoint transitions,
     canonical artifact creation, and immutable raw retry behavior.
2. `cargo clippy -p master-ingest --tests -- -D warnings`
   - Passed with no warnings.
3. `npm run test:canonical-video-subtitles`
   - Passed: `Canonical video subtitle source check passed`.
4. Root `cargo test transcript_artifacts` was intentionally not run because the root package has
   direct Whisper/V8/native dependencies and the task explicitly prohibits those builds. The
   lightweight crate's `canonical_artifacts_never_overwrite_raw_on_retry` filesystem test provides
   the Task 4 artifact regression coverage without starting a desktop/native build.

## RED/GREEN Notes

- RED used the missing public orchestration contracts, not a syntax or fixture failure.
- GREEN reused `%TEMP%\bili-shadowreplay-task4-target`; no repository target directory was created.
- No real video or the 8.24 GiB source was opened or processed.
