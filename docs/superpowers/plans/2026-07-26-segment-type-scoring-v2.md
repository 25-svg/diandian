# Segment Type Scoring V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the full-chain-only comparison score with one evidence-backed 100-point enterprise reusability score that evaluates all 12 discovered segment types and deterministically decides how each result may be used.

**Architecture:** Keep `compare_highlight_to_master` as the public command and the existing support-candidate table as the human-review queue. Upgrade the Rust scoring domain, model response parser, prompt, request payload, and Svelte result panel in place; retain compatibility fields only where old cached results still need to render.

**Tech Stack:** Rust, Serde, SQLite/sqlx, Tauri commands, TypeScript, Svelte 3, MiniMax structured JSON.

## Global Constraints

- Prompt one remains the only candidate discovery stage; its confidence score is never shown as the formal enterprise score.
- Prompt two produces the only formal “企业母稿可复用价值分”.
- All 12 segment types are scoreable; a single golden sentence must not be rejected for lacking a complete transaction chain.
- The backend validates score caps, recomputes the total, and recomputes the final decision.
- Only `support_candidate` is persisted to `support_script_candidates`.
- `golden_sentence` is displayed but is not persisted to the support-candidate table in this phase.
- No order, click, dwell-time, or other operating metric is inferred.
- Existing unrelated worktree changes must not be reverted or committed.
- Run focused tests per task; do not run the full desktop check until final integration.

---

## File Structure

- `src-tauri/crates/master-script/src/lib.rs`: canonical six-dimension score and admission boundary rules used by the database gate.
- `src-tauri/crates/master-script/tests/scoring.rs`: score caps and 69/70/84/85 boundary tests.
- `src-tauri/crates/master-comparison/src/lib.rs`: prompt-two response types, tolerant JSON parsing, evidence validation, deterministic decision, and comparison result.
- `src-tauri/crates/master-comparison/tests/master_comparison_tests.rs`: all-type scoring, evidence/risk gates, and master comparison behavior.
- `src-tauri/src/master_script/comparison.rs`: command request, MiniMax prompt, verified inputs, duplicate handling, and support-candidate persistence.
- `src/lib/masterScript.ts`: V2 request/response TypeScript contracts and presentation helpers.
- `src/lib/masterScript.test.ts`: frontend decision and compatibility presentation tests.
- `src/page/ArchiveAnalysis.svelte`: send the structured candidate and allow every V2 candidate to enter scoring.
- `src/lib/components/analysis/MasterComparisonPanel.svelte`: beginner-readable single-score result.

---

### Task 1: Canonical V2 Score And Admission Boundaries

**Files:**
- Modify: `src-tauri/crates/master-script/src/lib.rs`
- Modify: `src-tauri/crates/master-script/tests/scoring.rs`
- Modify: `src-tauri/crates/master-script-store/src/lib.rs`
- Test: `src-tauri/crates/master-script/tests/scoring.rs`

**Interfaces:**
- Produces: `ScoreBreakdown::new(scene_goal, persuasiveness, master_increment, reusability, factual_accuracy, natural_expression)`.
- Produces: accessors with the same six names and `total()`.
- Keeps: `evaluate_admission(&HardGateResult, &ScoreBreakdown) -> CandidateAdmission`.
- Consumed by: `master-comparison` validation and `master-script-store` queue gate.

- [ ] **Step 1: Write failing score-cap and boundary tests**

```rust
let maximum = ScoreBreakdown::new(30, 20, 20, 15, 10, 5).unwrap();
assert_eq!(maximum.total(), 100);
assert!(ScoreBreakdown::new(31, 20, 20, 15, 10, 5).is_err());
assert!(ScoreBreakdown::new(30, 21, 20, 15, 10, 5).is_err());
assert_eq!(
    evaluate_admission(&passing_gates(), &ScoreBreakdown::new(25, 17, 17, 12, 9, 5).unwrap()),
    CandidateAdmission::CandidateQueue,
);
```

- [ ] **Step 2: Run the focused domain tests and verify failure**

Run:

```powershell
cargo test -p master-script --test scoring
```

Expected: failures because the current field names and caps represent the old score.

- [ ] **Step 3: Replace the canonical score fields**

Use these serialized camelCase fields:

```rust
pub struct ScoreBreakdown {
    scene_goal: u8,
    persuasiveness: u8,
    master_increment: u8,
    reusability: u8,
    factual_accuracy: u8,
    natural_expression: u8,
    total: u8,
}
```

Validate exact caps `30, 20, 20, 15, 10, 5`, recompute `total`, and update store fixtures to produce valid 69/70/84/85 examples.

- [ ] **Step 4: Run the focused domain and store tests**

Run:

```powershell
cargo test -p master-script --test scoring
cargo test -p master-script-store
```

Expected: all score and queue-gate tests pass.

---

### Task 2: Prompt-Two Assessment Model And Deterministic Decision

**Files:**
- Modify: `src-tauri/crates/master-comparison/src/lib.rs`
- Modify: `src-tauri/crates/master-comparison/tests/master_comparison_tests.rs`
- Test: `src-tauri/crates/master-comparison/tests/master_comparison_tests.rs`

**Interfaces:**
- Produces enums: `EvidenceGrade { A, B, C }`, `RiskStatus { Passed, NeedsReview, Blocked }`, and `SegmentDecision`.
- Produces: `ScoreDetail`, `ScoreDetails`, and `SegmentQualityReview`.
- Produces: `evaluate_segment_decision(segment_type, total, evidence_grade, risk_status)`.
- Produces: `parse_model_assessment(response) -> Result<ModelAssessment, String>`.
- Consumed by: Tauri comparison handler and Svelte `MasterComparisonResult`.

- [ ] **Step 1: Write failing decision-matrix tests**

Cover:

```rust
assert_eq!(decision("异议处理", 85, A, Passed), SupportCandidate);
assert_eq!(decision("高质量金句", 85, B, NeedsReview), GoldenSentence);
assert_eq!(decision("产品讲解", 85, C, Passed), TrainingMaterial);
assert_eq!(decision("异议处理", 84, A, Passed), TrainingMaterial);
assert_eq!(decision("异议处理", 69, A, Passed), ReferenceOnly);
assert_eq!(decision("需要改进的反面案例", 96, A, Passed), ReferenceOnly);
assert_eq!(decision("产品推荐", 96, A, Blocked), Blocked);
```

- [ ] **Step 2: Write failing tolerant-parser tests**

Test a fenced JSON response with string scores such as `"26"` and assert it becomes numeric. Reject illegal evidence grade, risk status, score above its dimension cap, missing evidence for a score detail, and evidence timestamps outside the supplied clip.

- [ ] **Step 3: Run the comparison tests and verify failure**

Run:

```powershell
cargo test -p master-comparison
```

Expected: missing V2 assessment types and decisions.

- [ ] **Step 4: Implement V2 assessment types and parser**

The accepted model shape is:

```rust
pub struct ModelAssessment {
    pub segment_id: String,
    pub segment_type: String,
    pub total_score: u8,
    pub decision: SegmentDecision,
    pub evidence_grade: EvidenceGrade,
    pub risk_status: RiskStatus,
    pub score_details: ScoreDetails,
    pub what_is_good: Vec<String>,
    pub what_needs_improvement: Vec<String>,
    pub facts_to_confirm: Vec<String>,
    pub reusable_original_sentence: String,
    pub suggested_training_version: String,
    pub recommended_master_section: String,
    pub evidence_cue_ids: Vec<u64>,
}
```

Normalize numeric strings before deserialization. Build the canonical `ScoreBreakdown` from the six detail scores, reject a mismatched model total, then overwrite the model decision with `evaluate_segment_decision`.

- [ ] **Step 5: Extend `MasterComparison` without a database migration**

Return:

```rust
pub struct MasterComparison {
    // existing master identity fields
    pub score: Option<ScoreBreakdown>,
    pub total_score: Option<u8>,
    pub admission: Option<CandidateAdmission>,
    pub quality_review: Option<SegmentQualityReview>,
    // existing compatibility summary fields
}
```

Map decisions to existing admissions:

- `support_candidate -> candidate_queue`
- `training_material -> review_only`
- `reference_only -> analysis_only`
- `blocked -> blocked`
- `golden_sentence -> analysis_only`

- [ ] **Step 6: Run focused comparison tests**

Run:

```powershell
cargo test -p master-comparison
```

Expected: parser, evidence, decision, score, unmatched-master, and stale-master tests pass.

---

### Task 3: Replace Full-Chain-Only Handler With Prompt Two

**Files:**
- Modify: `src-tauri/src/master_script/comparison.rs`
- Modify: `src-tauri/src/handlers/master_script.rs` only if command serialization requires it
- Test: inline `comparison.rs` tests

**Interfaces:**
- Extends `CompareHighlightRequest` with `candidate_segment`.
- Consumes the matched master section, reviewed clip cues, and verified parameter-card facts.
- Calls `request_assessment` for every structured V2 segment type.
- Persists only `SegmentDecision::SupportCandidate`.

- [ ] **Step 1: Write failing request and eligibility tests**

Construct requests for `高质量金句`, `异议处理`, `产品讲解`, and `完整成交链路`. Assert all 12 known types are scoreable and unknown types are rejected. Assert no complete-chain gate remains.

- [ ] **Step 2: Run the handler unit tests and verify failure**

Run:

```powershell
cargo test -p bili-shadowreplay master_script::comparison::tests
```

Expected: non-full-chain types still take the old analysis-only return.

- [ ] **Step 3: Extend the request payload**

Accept this candidate data:

```rust
pub struct CandidateSegmentInput {
    pub segment_id: String,
    pub segment_type: String,
    pub scene: String,
    pub customer_need: String,
    pub original_text: String,
    pub key_sentence: String,
    pub outcome: String,
    pub interrupted: bool,
    pub why_selected: String,
}
```

The backend still obtains authoritative clip text from the reviewed transcript; `original_text` is context, not trusted evidence.

- [ ] **Step 4: Replace the MiniMax system prompt**

Use the user-approved prompt-two rules plus explicit targets for:

- 需求判断
- 产品推荐
- 售后与风险消除
- 需要改进的反面案例

Require fixed JSON, all six score details, evidence cue IDs, evidence grade, risk status, and the five decisions. Include the matched master section and only the confirmed parameter card associated with that section.

- [ ] **Step 5: Update hard gates**

Set:

- `master_section_matched` from actual match.
- `host_speech_backed` only when valid cue evidence exists.
- `facts_resolved=false` when required facts remain unresolved.
- `transaction_evidence_valid=false` when the model claims confirmed conversion without transcript evidence.
- `context_complete=true` when the context is sufficient for the current segment type, not only when seven transaction stages exist.

If `pending_critical_count > 0`, force risk to at least `needs_review` and prevent queue insertion.

- [ ] **Step 6: Restrict persistence by V2 decision**

Persist only when:

```rust
comparison.quality_review.as_ref().map(|review| review.decision)
    == Some(SegmentDecision::SupportCandidate)
```

Keep existing duplicate detection and `candidate_queue` store validation.

- [ ] **Step 7: Run focused handler and database tests**

Run:

```powershell
cargo test -p bili-shadowreplay master_script::comparison::tests
cargo test -p master-script-store
```

Expected: specialized segment types score, only support candidates persist, golden sentences do not persist, and duplicates do not duplicate rows.

---

### Task 4: Frontend Contract And All-Type Scoring Trigger

**Files:**
- Modify: `src/lib/masterScript.ts`
- Modify: `src/lib/masterScript.test.ts`
- Modify: `src/page/ArchiveAnalysis.svelte`
- Test: `src/lib/masterScript.test.ts`

**Interfaces:**
- Sends `candidateSegment` from the structured prompt-one result.
- Receives `qualityReview`.
- Produces beginner labels for the five decisions, three evidence grades, and three risk statuses.

- [ ] **Step 1: Write failing frontend presentation tests**

Test exact labels:

```ts
segmentDecisionPresentation("support_candidate") === "候选辅稿"
segmentDecisionPresentation("golden_sentence") === "金句话术"
segmentDecisionPresentation("training_material") === "训练素材"
segmentDecisionPresentation("reference_only") === "仅作复盘参考"
segmentDecisionPresentation("blocked") === "禁止使用"
```

Also test A/B/C and risk labels.

- [ ] **Step 2: Run the frontend domain test and verify failure**

Run:

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
```

Expected: V2 types and helpers are missing.

- [ ] **Step 3: Add TypeScript V2 contracts**

Add `SegmentDecision`, `EvidenceGrade`, `RiskStatus`, `ScoreDetail`, `ScoreDetails`, and `SegmentQualityReview` to `MasterComparisonResult`. Keep old optional fields for cached-result rendering.

- [ ] **Step 4: Send the structured candidate**

In `compareSelectedToMaster`, send:

```ts
candidateSegment: {
  segmentId: candidate.id,
  segmentType: candidate.type,
  scene: candidate.scene,
  customerNeed: candidate.customerNeed,
  originalText: candidate.originalText,
  keySentence: candidate.keySentence,
  outcome: candidate.outcome,
  interrupted: candidate.interrupted,
  whySelected: candidate.whySelected,
}
```

- [ ] **Step 5: Remove the complete-chain-only frontend gate**

Replace `isMasterScoreEligible(candidate)` as the scoring prerequisite with a structured V2 candidate check. Keep failed transcript context verification and unmatched/ambiguous master scenes as explicit stopping states.

- [ ] **Step 6: Run frontend domain tests**

Run:

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
```

Expected: both pass.

---

### Task 5: Beginner-Readable Single Score Panel

**Files:**
- Modify: `src/lib/components/analysis/MasterComparisonPanel.svelte`
- Modify: `src/page/ArchiveAnalysis.svelte`

**Interfaces:**
- Consumes: `result.comparison.qualityReview`.
- Displays: one total score, decision, evidence grade, risk status, good points, improvements, facts to confirm, six score details, reusable original sentence, training suggestion, and master insertion section.

- [ ] **Step 1: Replace the current summary with the V2 hierarchy**

Render in this order:

1. Decision and one formal total.
2. Evidence grade and risk status.
3. “为什么好”.
4. “哪里需要改”.
5. “还要确认”.
6. Collapsible six-dimension details.
7. “主播原话”.
8. “培训建议稿” with an explicit suggestion label.
9. Recommended master section.

- [ ] **Step 2: Remove ambiguous score copy**

Use only “企业母稿可复用价值分”. Do not render the discovery confidence score or the old “本地复核分”.

- [ ] **Step 3: Preserve old cached-result fallback**

When `qualityReview` is absent, render the existing `verdict`, `improvements`, `risks`, and total without crashing.

- [ ] **Step 4: Compile the changed Svelte files**

Run the focused Svelte preprocess/compile check for:

- `src/page/ArchiveAnalysis.svelte`
- `src/lib/components/analysis/MasterComparisonPanel.svelte`

Expected: both compile.

---

### Task 6: Final Focused Verification

**Files:**
- Verify all files listed above

- [ ] **Step 1: Run all focused Rust tests**

```powershell
cargo test -p master-script --test scoring
cargo test -p master-comparison
cargo test -p master-script-store
cargo test -p bili-shadowreplay master_script::comparison::tests
```

- [ ] **Step 2: Run focused TypeScript tests**

```powershell
node --loader ts-node/esm src/lib/masterScript.test.ts
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
```

- [ ] **Step 3: Run changed-page Svelte compilation**

Preprocess and compile both changed Svelte files with `svelte/compiler` and `svelte-preprocess`.

- [ ] **Step 4: Run diff validation**

```powershell
git diff --check -- `
  src-tauri/crates/master-script/src/lib.rs `
  src-tauri/crates/master-script/tests/scoring.rs `
  src-tauri/crates/master-comparison/src/lib.rs `
  src-tauri/crates/master-comparison/tests/master_comparison_tests.rs `
  src-tauri/src/master_script/comparison.rs `
  src/lib/masterScript.ts `
  src/lib/masterScript.test.ts `
  src/page/ArchiveAnalysis.svelte `
  src/lib/components/analysis/MasterComparisonPanel.svelte
```

Expected: no whitespace errors.

- [ ] **Step 5: Review behavior against the specification**

Confirm:

- All 12 types receive type-specific scoring.
- Only one formal score is visible.
- 85/A-or-B/nonblocked specialized segments can become support candidates.
- Golden sentences are displayed but not inserted into the support queue.
- Counterexamples never enter the support queue.
- Every score detail contains actual evidence.
- The system does not claim actual business conversion effectiveness.

