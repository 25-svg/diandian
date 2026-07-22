# Task 1 Report: Master-script domain rules

## Implementation summary

- Added the `master-script` workspace crate with `serde` and `thiserror` dependencies.
- Implemented bounded `ScoreBreakdown` construction with caps of `25, 25, 20, 15, 10, 5` and a computed total.
- Implemented `HardGateResult::all_pass()` as the conjunction of exactly seven booleans; `reasons` remains UI evidence only.
- Implemented deterministic admission thresholds: `Blocked` for any failed gate, `AnalysisOnly` below 70, `ReviewOnly` from 70 through 85, and `CandidateQueue` above 85.
- Implemented the exact `MasterSectionKind` and `SupportCandidateStatus` variants with snake_case serde and `as_str()` values.
- Implemented strict three-component ASCII decimal version parsing with `u64` components and checked patch increment.

## RED evidence

Command, run from `src-tauri`:

```powershell
cargo test -p master-script --test scoring
```

Output:

```text
Compiling master-script v0.1.0 (...\src-tauri\crates\master-script)
error[E0432]: unresolved import `master_script`
 --> crates\master-script\tests\scoring.rs:1:5
  |
1 | use master_script::{
  |     ^^^^^^^^^^^^^ use of unresolved module or unlinked crate `master_script`
error[E0433]: cannot find module or crate `master_script` in this scope
error: could not compile `master-script` (test "scoring") due to 3 previous errors
```

This was the expected RED state: the integration test existed, but the library and public contracts did not.

## GREEN evidence

The first implementation run exposed a test-only assertion compilation issue because `serde_json::Error` does not implement `PartialEq`. The test was corrected to unwrap deserialization before comparing values, then the focused suite was rerun.

Command:

```powershell
cargo test -p master-script
```

Final output:

```text
running 0 tests
test result: ok. 0 passed; 0 failed

running 7 tests
test a_failed_gate_blocks_a_perfect_score ... ok
test admits_only_scores_above_85_after_all_gates_pass ... ok
test gate_reasons_do_not_change_a_passing_gate_result ... ok
test increments_patch_and_reports_patch_overflow ... ok
test rejects_non_strict_patch_versions ... ok
test serializes_master_section_kinds_and_support_statuses_as_snake_case ... ok
test validates_dimension_caps ... ok

test result: ok. 7 passed; 0 failed

Finished `test` profile
```

Focused formatting verification also passed:

```powershell
rustfmt --edition 2021 --check 'crates/master-script/src/lib.rs' 'crates/master-script/tests/scoring.rs'
```

Output: exit code `0`, no output.

## Regression evidence

The relevant non-native package tests passed:

```powershell
cargo test -p danmu_stream --lib
```

Output: `6 passed; 0 failed`.

```powershell
cargo test -p knowledge
```

Output: `9 passed; 1 ignored; 0 failed`.

```powershell
cargo test -p knowledge-store
```

Output: `5 passed; 0 failed`.

The focused `master-script` command above passed `7 passed; 0 failed`.

## Files changed

- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/crates/master-script/Cargo.toml`
- `src-tauri/crates/master-script/src/lib.rs`
- `src-tauri/crates/master-script/tests/scoring.rs`
- `.superpowers/sdd/task-1-report.md`

## Self-review

- TDD order was preserved: the focused test target was run and failed before `lib.rs` was created.
- All clarified enum variants and serialized values are explicit and covered by round-trip tests.
- Gate reasons cannot override the seven boolean conjunction.
- Score totals cannot exceed 100 because each dimension is capped before construction.
- Version parsing rejects missing, extra, signed, whitespace-containing, prerelease, build-suffixed, and non-ASCII/non-decimal components. Numeric parse overflow is invalid input; patch increment overflow is `VersionOverflow`.
- `git diff --check` reported no whitespace errors.
- No unrelated source or plan/spec documents were modified.

## Concerns

- `cargo test --workspace --exclude whisper-cpp-rs --no-default-features` could not complete because the existing `whisper-cpp-rs` CMake build directory is cached for Ninja while its build script requests Visual Studio 17 2022 (`CMake Error: generator ... does not match the generator used previously`).
- The broader package regression command also encountered an existing network-dependent Huya test failure in `recorder` (`RelativeUrlWithoutBase`) after fetching live HTML.
- A package regression run including `danmu_stream` failed only its existing doctest, which uses `await` outside an async function and unresolved example symbols. Its library tests passed when run with `--lib`.
- Workspace-wide `cargo fmt --all -- --check` reports pre-existing formatting differences in `crates/knowledge` and `src/subtitle_generator`; only the new files were formatted and verified.

## Review fix: derived score total

### Fix summary

- Made `ScoreBreakdown.total` private and exposed `ScoreBreakdown::total()` for admission and later consumers.
- Replaced derived score deserialization with a helper containing only the six bounded dimensions. An incoming `total` field is ignored by serde and recomputed through `ScoreBreakdown::new`.
- Kept serialization derived, so UI payloads include the corrected recomputed `total`.
- Added `tampered_serialized_total_cannot_admit_a_low_score`, which supplies low dimensions with `total: 86`, verifies the derived total is `0`, verifies admission is not `CandidateQueue`, and verifies serialized output contains `total: 0`.

### Review-fix RED evidence

Command, run from `src-tauri` after adding the regression test and before changing production code:

```powershell
cargo test -p master-script
```

Output:

```text
running 8 tests
test tampered_serialized_total_cannot_admit_a_low_score ... FAILED

thread 'tampered_serialized_total_cannot_admit_a_low_score' panicked ...
assertion `left != right` failed
left: CandidateQueue
right: CandidateQueue

test result: FAILED. 7 passed; 1 failed
error: test failed, to rerun pass `-p master-script --test scoring`
```

### Review-fix GREEN evidence

Command:

```powershell
cargo test -p master-script
```

Output:

```text
running 8 tests
test a_failed_gate_blocks_a_perfect_score ... ok
test admits_only_scores_above_85_after_all_gates_pass ... ok
test gate_reasons_do_not_change_a_passing_gate_result ... ok
test increments_patch_and_reports_patch_overflow ... ok
test rejects_non_strict_patch_versions ... ok
test tampered_serialized_total_cannot_admit_a_low_score ... ok
test serializes_master_section_kinds_and_support_statuses_as_snake_case ... ok
test validates_dimension_caps ... ok

test result: ok. 8 passed; 0 failed
```

Focused formatting verification:

```powershell
rustfmt --edition 2021 --check 'crates/master-script/src/lib.rs' 'crates/master-script/tests/scoring.rs'
```

Output: exit code `0`, no output.
