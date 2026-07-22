# Task 1 Report: Master-script domain rules

## Implementation summary

- Added the `master-script` workspace crate with `serde` and `thiserror` dependencies.
- Implemented bounded `ScoreBreakdown` construction with caps of `25, 25, 20, 15, 10, 5` and a computed total.
- Implemented `HardGateResult::all_pass()` as the conjunction of exactly seven booleans; `reasons` remains UI evidence only.
- Implemented deterministic admission thresholds: `Blocked` for any failed gate, `AnalysisOnly` from 0 through 69, `ReviewOnly` from 70 through 84, and `CandidateQueue` from 85 through 100.
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

## Business threshold change: score 85 admission

This section supersedes the historical 85/86 boundary evidence elsewhere in this report. The binding rule is now: after all hard gates pass, `85-100` is `CandidateQueue`, `70-84` is `ReviewOnly`, and `0-69` is `AnalysisOnly`.

### Threshold fix summary

- Changed the boundary test first from 85/86 to 84/85 while leaving production unchanged.
- Changed `evaluate_admission` from `score.total() > 85` to `score.total() >= 85`.
- Preserved private score dimensions, read-only accessors, bounded construction, recomputing deserialization, and derived serialization.
- Updated the design specification and implementation plan, including Task 2 persistence, Task 6 comparison, Task 8 UI, Task 10 E2E, manual acceptance, and execution checkpoints.

### Threshold RED evidence

Command, run from `src-tauri` before the production change:

```powershell
cargo test -p master-script
```

Output:

```text
running 9 tests
test admits_scores_at_85_after_all_gates_pass ... FAILED

assertion `left == right` failed
left: ReviewOnly
right: CandidateQueue

test result: FAILED. 8 passed; 1 failed; 0 ignored
error: test failed, to rerun pass `-p master-script --test scoring`
```

### Threshold GREEN evidence

Command:

```powershell
cargo test -p master-script
```

Output:

```text
running 9 tests
test a_failed_gate_blocks_a_perfect_score ... ok
test admits_scores_at_85_after_all_gates_pass ... ok
test gate_reasons_do_not_change_a_passing_gate_result ... ok
test increments_patch_and_reports_patch_overflow ... ok
test rejects_non_strict_patch_versions ... ok
test score_round_trip_exposes_read_only_dimensions_and_derived_total ... ok
test serializes_master_section_kinds_and_support_statuses_as_snake_case ... ok
test tampered_serialized_total_cannot_admit_a_low_score ... ok
test validates_dimension_caps ... ok

test result: ok. 9 passed; 0 failed; 0 ignored
```

Focused formatting command:

```powershell
rustfmt --edition 2021 --check 'crates/master-script/src/lib.rs' 'crates/master-script/tests/scoring.rs'
```

Output: exit code `0`, no output.

### Document consistency evidence

Forbidden-old-semantics scan:

```powershell
rg -n -i '高于\s*85|above\s+85|>\s*85|86-100|70-85|85\s*(不|does not|creates none)|score_85_cannot|85/86|86-point|score 86|总分 86|\b86\b' docs/superpowers/specs/2026-07-22-master-script-baseline-analysis-design.md docs/superpowers/plans/2026-07-22-master-script-baseline-analysis.md
```

Output: no matches (`NO_FORBIDDEN_OLD_SEMANTICS`).

Positive boundary scan:

```powershell
rg -n '85-100|70-84|0-69|score_84|score_85|84/85|总分 84|总分 85|at least 85|达到 85|85 分及以上|score 84|score 85' docs/superpowers/specs/2026-07-22-master-script-baseline-analysis-design.md docs/superpowers/plans/2026-07-22-master-script-baseline-analysis.md
```

Output confirmed:

```text
plan: Total score `85-100` enters the support-candidate queue; `70-84` is review-only and `0-69` is analysis-only.
plan: score_84 => ReviewOnly; score_85 => CandidateQueue.
plan Task 2: score_84_cannot_be_persisted_as_queued.
plan Task 8: score 84 displays review-only; score 85 displays entered-candidate.
plan Task 10: score 85 creates one pending candidate; score 84 creates none.
spec: `85-100` candidate queue; `70-84` review; `0-69` analysis.
spec acceptance: total 84 does not enter; total 85 with all gates enters.
```

Final diff validation:

```powershell
git diff --check
```

Output: exit code `0`; only existing Git LF-to-CRLF conversion warnings were printed.

## Second review fix: immutable score dimensions

### Fix summary

- Made all six score dimensions private alongside the already-private cached total.
- Added read-only accessors named `transaction_evidence()`, `improvement_over_master()`, `reusability()`, `completeness()`, `factual_accuracy()`, `scenario_clarity()`, and `total()`.
- Kept `ScoreBreakdown::new` and custom serde deserialization as the only construction paths enforcing caps and recomputing total.
- Added a serde round-trip regression covering all accessors and asserting the exact camelCase six-dimension plus `total` JSON payload.

### Second review RED evidence

Command, run from `src-tauri` after adding the round-trip regression and before adding the accessors or privatizing fields:

```powershell
cargo test -p master-script
```

Output:

```text
error[E0599]: no method named `transaction_evidence` found for struct `ScoreBreakdown`
error[E0599]: no method named `improvement_over_master` found for struct `ScoreBreakdown`
error[E0599]: no method named `reusability` found for struct `ScoreBreakdown`
error[E0599]: no method named `completeness` found for struct `ScoreBreakdown`
error[E0599]: no method named `factual_accuracy` found for struct `ScoreBreakdown`
error[E0599]: no method named `scenario_clarity` found for struct `ScoreBreakdown`
error: could not compile `master-script` (test "scoring") due to 6 previous errors
```

### Second review GREEN evidence

Command:

```powershell
cargo test -p master-script
```

Output:

```text
running 9 tests
test a_failed_gate_blocks_a_perfect_score ... ok
test score_round_trip_exposes_read_only_dimensions_and_derived_total ... ok
test serializes_master_section_kinds_and_support_statuses_as_snake_case ... ok
test increments_patch_and_reports_patch_overflow ... ok
test rejects_non_strict_patch_versions ... ok
test gate_reasons_do_not_change_a_passing_gate_result ... ok
test admits_only_scores_above_85_after_all_gates_pass ... ok
test tampered_serialized_total_cannot_admit_a_low_score ... ok
test validates_dimension_caps ... ok

test result: ok. 9 passed; 0 failed
```

Focused formatting verification:

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
