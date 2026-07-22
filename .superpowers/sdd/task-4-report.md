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

## Review Fix RED Evidence

1. `cargo test -p master-ingest --test master_ingest_tests`
   - Failed with three intended regressions: `R5` ranked as an exact match for `R50`, legitimate
     overlapping cues were shifted or removed, and malformed complete checkpoint SRT was accepted.
2. `cargo test -p master-ingest --test master_ingest_tests selected_cards_render_deterministic_volcengine_context`
   - Failed with `E0432` because the pure parameter-card context adapter did not exist.

## Review Fix GREEN Evidence

1. `cargo test -p master-ingest`
   - Passed: 10 tests, 0 failed.
2. `cargo clippy -p master-ingest --tests -- -D warnings`
   - Passed with no warnings.
3. `npm run test:canonical-video-subtitles`
   - Passed: `Canonical video subtitle source check passed`.
4. Root compilation remains intentionally deferred. The concrete FFmpeg, Volcengine, State,
   database, artifact, and command wiring was statically reviewed but no root/Tauri/V8/Whisper
   build, real API request, or real video processing was run.

## Direct Follow-up Fixes

- Resume now loads the immutable master source by ID and rejects a different video identity.
- Mixed CJK/model names enforce ASCII boundaries, so `佳能R5` does not match `佳能R50`.
- FFmpeg writes each MP3 chunk to a unique same-directory temporary file and publishes it only
  after successful, non-empty extraction; failed publication removes the temporary file.
- Focused verification: `master-ingest` 13 passed, `master-script-store` 17 passed, and clippy
  passed for both crates with warnings denied.
