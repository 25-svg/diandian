# Enterprise Master Upgrade Comparison Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically compare only qualified prompt-two results with the active enterprise master, persist an auditable prompt-three decision, and present a beginner-readable human review workflow.

**Architecture:** Add a focused upgrade-review domain module to `master-comparison`, persist prompt-two snapshots and prompt-three results in a new idempotent database table, then orchestrate prompt three after prompt two without making prompt-three failure erase the score. Actionable decisions reuse the existing candidate queue; duplicate and rejected decisions remain audit records only.

**Tech Stack:** Rust 2021, serde/serde_json, sqlx/SQLite, Tauri commands, TypeScript, Svelte 3, existing MiniMax text client.

## Global Constraints

- Prompt three runs only for prompt-two decisions `support_candidate` or `golden_sentence`.
- Evidence grade `C` or risk `blocked` never invokes prompt three.
- `replace_existing` and `merge_with_existing` never alter the master without human approval.
- `duplicate` and `reject` never enter the upgrade candidate queue.
- Host words must remain verbatim and training suggestions must be visibly labeled as suggestions.
- Prompt-three failure keeps prompt-two results and can be retried without ASR, discovery, or prompt-two scoring.
- Existing unrelated dirty-worktree changes must not be reverted or included in destructive cleanup.
- Run focused tests per task; defer one complete desktop check to final integration.

---

### Task 1: Prompt-Three Domain Contract And Parser

**Files:**
- Create: `src-tauri/crates/master-comparison/src/upgrade_review.rs`
- Modify: `src-tauri/crates/master-comparison/src/lib.rs`
- Create: `src-tauri/crates/master-comparison/tests/upgrade_review_tests.rs`

**Interfaces:**
- Produces:
  - `UpgradeComparisonDecision`
  - `MasterUpgradeReview`
  - `parse_upgrade_review(raw, expected_section, candidate_original_text)`
  - `is_actionable_upgrade_decision(decision)`
  - `prompt_two_is_upgrade_eligible(review)`

- [ ] **Step 1: Write failing decision and parser tests**

Test all six decisions, fenced JSON, scalar-to-list normalization, unknown decisions, mismatched sections, and invented host words.

```rust
assert_eq!(
    parse_upgrade_review(
        raw,
        "异议处理/质量担忧",
        "我们每台机器都经过检测。"
    )?.comparison_decision,
    UpgradeComparisonDecision::AddAsSupport
);
assert!(parse_upgrade_review(wrong_section, expected, original).is_err());
assert!(parse_upgrade_review(invented_words, expected, original).is_err());
```

- [ ] **Step 2: Verify the new tests fail**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test -p master-comparison --test upgrade_review_tests
```

Expected: compile failure because the upgrade-review API does not exist.

- [ ] **Step 3: Implement the domain model and strict parser**

Use camelCase serialization for application output and snake_case input aliases matching the prompt. Normalize only list fields; do not normalize unknown decisions or mismatched sections.

```rust
pub enum UpgradeComparisonDecision {
    AddAsSupport,
    AddAsGoldenSentence,
    ReplaceExisting,
    MergeWithExisting,
    Duplicate,
    Reject,
}
```

Validate that `original_host_words` is a non-empty substring of the supplied candidate original text. Require non-empty `new_value`, `recommended_action`, and the expected matched section.

- [ ] **Step 4: Add deterministic eligibility and actionability rules**

```rust
pub fn prompt_two_is_upgrade_eligible(review: &SegmentQualityReview) -> bool {
    matches!(
        review.decision,
        SegmentDecision::SupportCandidate | SegmentDecision::GoldenSentence
    ) && review.evidence_grade != EvidenceGrade::C
      && review.risk_status != RiskStatus::Blocked
}
```

Only add, golden, replace and merge are actionable.

- [ ] **Step 5: Run focused tests**

Run:

```powershell
cargo test -p master-comparison --test upgrade_review_tests
cargo test -p master-comparison
```

Expected: all tests pass.

---

### Task 2: Persistent And Idempotent Upgrade Review Jobs

**Files:**
- Modify: `src-tauri/crates/master-script-store/src/lib.rs`
- Modify: `src-tauri/src/database/master_script.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/crates/master-script-store/src/lib.rs`

**Interfaces:**
- Consumes: serialized candidate, prompt-two comparison and master-section snapshot.
- Produces:
  - `MASTER_UPGRADE_REVIEW_MIGRATION_SQL`
  - `NewMasterUpgradeReview`
  - `MasterUpgradeReviewRow`
  - `create_or_get_master_upgrade_review`
  - `complete_master_upgrade_review`
  - `fail_master_upgrade_review`
  - `get_master_upgrade_review`

- [ ] **Step 1: Write failing store tests**

Cover:

1. Identical review identity returns the same row.
2. Different immutable payload for the same identity is rejected.
3. Pending can become complete.
4. Failed can be retried and become complete.
5. Complete result is immutable.

Identity fields are master version, section, source, clip range and transcript hash.

- [ ] **Step 2: Verify store tests fail**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test -p master-script-store master_upgrade_review
```

Expected: compile failure because the table and functions are absent.

- [ ] **Step 3: Add migration version 26**

Create `master_upgrade_reviews` with:

```sql
id, review_key, master_script_id, master_section_id,
source_key, source_start_ms, source_end_ms, transcript_hash,
candidate_json, prompt_two_json, master_section_json,
comparison_decision, review_json,
status CHECK(status IN ('pending','complete','failed')),
error, created_at, updated_at
```

Register migration version `26` in both GUI and headless migration paths through the existing migration list.

- [ ] **Step 4: Implement store transitions**

Use `INSERT ... ON CONFLICT(review_key) DO NOTHING`, then compare immutable payload fields. Completion stores the validated decision and JSON. Failure stores only an error and permits retry.

- [ ] **Step 5: Expose database wrapper methods**

Re-export the types and migration constant from `database/master_script.rs`, mapping `StoreError` through the existing adapter.

- [ ] **Step 6: Run focused store tests**

Run:

```powershell
cargo test -p master-script-store master_upgrade_review
cargo test -p master-script-store
```

Expected: all tests pass.

---

### Task 3: Automatic Prompt-Three Orchestration And Retry

**Files:**
- Modify: `src-tauri/src/master_script/comparison.rs`
- Modify: `src-tauri/src/handlers/master_script.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/master_script/comparison.rs`

**Interfaces:**
- Consumes: prompt-two `MasterComparison`, `CandidateSegmentInput`, matched `MasterSectionRow`.
- Produces:
  - `UpgradeReviewResult` on `CompareHighlightResult`
  - `upgrade_review_error`
  - `retry_master_upgrade_review(review_id)`

- [ ] **Step 1: Write failing orchestration tests**

Add pure helper tests proving:

```rust
assert!(should_run_upgrade_review(&support_candidate));
assert!(should_run_upgrade_review(&golden_sentence));
assert!(!should_run_upgrade_review(&score_84_training));
assert!(!should_run_upgrade_review(&evidence_c));
assert!(!should_run_upgrade_review(&blocked));
```

Add routing tests: duplicate/reject do not create support candidates; add/golden/replace/merge may create an actionable candidate.

- [ ] **Step 2: Verify tests fail**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test -p bili-shadowreplay master_script::comparison::tests
```

Expected: failure because prompt-three routing is missing.

- [ ] **Step 3: Add the exact prompt-three system and repair prompts**

The system prompt must contain:

- all seven comparison standards;
- all six decisions;
- the exact JSON shape;
- prohibition on invented facts;
- explicit host-original versus suggestion boundary.

The repair prompt may repair JSON shape only and must not change content or decision.

- [ ] **Step 4: Persist the pending review before the model call**

Serialize the prompt-two result, candidate and matched section. Create or retrieve the idempotent review job before invoking MiniMax.

If an identical complete job exists, reuse it without another model call.

- [ ] **Step 5: Validate and persist the prompt-three result**

Call `parse_upgrade_review` with the exact expected section and candidate original text. On first parse failure, make one repair call. Complete the review job only after validation.

On API or double-parse failure:

- mark the job failed;
- return prompt-two comparison normally;
- set `upgradeReviewError`;
- do not delete or downgrade prompt-two results.

- [ ] **Step 6: Route actionable decisions**

Store add/golden/replace/merge in `support_script_candidates`, with `comparison_json` containing:

```json
{
  "comparison": {},
  "upgradeReview": {}
}
```

Do not store duplicate/reject in the candidate queue. All six remain in `master_upgrade_reviews`.

- [ ] **Step 7: Add retry command**

`retry_master_upgrade_review` loads the saved candidate, prompt-two and section snapshots by review ID, reruns prompt three only, validates the result, and creates an actionable candidate idempotently when appropriate.

- [ ] **Step 8: Run command-layer tests**

Run:

```powershell
cargo test -p bili-shadowreplay master_script::comparison::tests
cargo test -p master-comparison
cargo test -p master-script-store
```

Expected: all focused tests pass.

---

### Task 4: Frontend Contract And Beginner Presentation

**Files:**
- Modify: `src/lib/masterScript.ts`
- Modify: `src/lib/masterScript.test.ts`
- Modify: `src/lib/components/analysis/MasterComparisonPanel.svelte`
- Modify: `src/page/ArchiveAnalysis.svelte`

**Interfaces:**
- Produces:
  - `UpgradeComparisonDecision`
  - `MasterUpgradeReview`
  - `upgradeDecisionLabel`
  - `upgradeDecisionNextAction`
  - `retryMasterUpgradeReview`

- [ ] **Step 1: Write failing frontend label tests**

Assert exact labels:

```ts
assert.equal(upgradeDecisionLabel("add_as_support"), "建议新增为辅稿");
assert.equal(upgradeDecisionLabel("replace_existing"), "建议人工审核后替换");
assert.equal(upgradeDecisionLabel("duplicate"), "与企业母稿重复");
```

Assert duplicate/reject are not actionable and suggestion text is labeled `建议稿`.

- [ ] **Step 2: Verify the frontend test fails**

Run:

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
```

Expected: failure because the prompt-three helpers do not exist.

- [ ] **Step 3: Add typed response contract and helpers**

Extend `MasterComparisonResult` with root-level:

```ts
upgradeReviewId: number | null;
upgradeReview: MasterUpgradeReview | null;
upgradeReviewError: string;
```

Expose the retry Tauri command.

- [ ] **Step 4: Render one upgrade-comparison section**

Below the prompt-two score show:

- Chinese decision;
- new value;
- why it is better;
- duplicate content;
- risks;
- host original words;
- visually separate `[建议稿]`;
- next action.

Do not show raw JSON. Replace/merge copy must say “提交人工审核”, never “已替换/已合并”.

- [ ] **Step 5: Add isolated retry behavior**

If `upgradeReviewError` exists, show “母稿比较暂未完成” and a retry button. The button calls only `retryMasterUpgradeReview(reviewId)` and updates the prompt-three section without clearing the prompt-two score.

- [ ] **Step 6: Run frontend focused tests and Svelte compilation**

Run:

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
```

Compile `MasterComparisonPanel.svelte` and `ArchiveAnalysis.svelte` using the existing `svelte/compiler` preprocessing check.

Expected: tests and both component compiles pass.

---

### Task 5: Candidate Queue Decisions And Safe Master Upgrade

**Files:**
- Modify: `src/lib/masterScript.ts`
- Modify: `src/lib/masterScript.test.ts`
- Modify: `src/lib/components/master/SupportCandidateQueue.svelte`
- Modify: `src-tauri/src/handlers/master_script.rs`
- Test: existing focused master-script handler tests

**Interfaces:**
- Consumes: prompt-three decision stored in candidate `comparisonJson`.
- Produces: grouped human-review UI and decision-aware upgrade preview.

- [ ] **Step 1: Write failing queue presentation tests**

Test grouping and next action for add, golden, replace and merge. Test that duplicate/reject cannot appear as selectable upgrade candidates.

- [ ] **Step 2: Verify frontend tests fail**

Run:

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
```

- [ ] **Step 3: Group the queue by prompt-three decision**

Show:

- 新增辅稿
- 金句话术
- 建议替换
- 建议合并

Each item retains the existing three human actions. Replace and merge display a stronger confirmation notice.

- [ ] **Step 4: Make upgrade preview decision-aware**

For approved candidates:

- `add_as_support`: append under `## 候选辅稿`.
- `add_as_golden_sentence`: append under `## 金句话术`.
- `merge_with_existing`: append under `## 待合并话术`, preserving current text.
- `replace_existing`: show proposed replacement in the diff, but require the existing diff confirmation before publication.

Reject missing or unknown prompt-three decisions. Continue rejecting candidates scored against an old master ID.

- [ ] **Step 5: Run focused queue and upgrade tests**

Run:

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
cargo test -p bili-shadowreplay master_script
```

Expected: all focused tests pass.

---

### Task 6: Final Focused Regression And Change Audit

**Files:**
- Verify only; no unrelated edits.

- [ ] **Step 1: Run all affected Rust suites**

```powershell
cargo test -p master-script --test scoring
cargo test -p master-comparison
cargo test -p master-script-store
cargo test -p bili-shadowreplay master_script::comparison::tests
```

- [ ] **Step 2: Run all affected frontend suites**

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
```

- [ ] **Step 3: Compile both affected Svelte entry points**

Preprocess and compile:

- `src/lib/components/analysis/MasterComparisonPanel.svelte`
- `src/lib/components/master/SupportCandidateQueue.svelte`
- `src/page/ArchiveAnalysis.svelte`

- [ ] **Step 4: Run formatting and diff checks**

Run targeted `rustfmt --check` and `git diff --check` only for files changed by this feature.

- [ ] **Step 5: Report the boundary**

State explicitly that the displayed score measures reusable talk value against the enterprise master, not real transaction performance. Report that no automatic master publication occurs.
