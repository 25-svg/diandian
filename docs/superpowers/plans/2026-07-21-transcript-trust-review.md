# Transcript Trust Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build one novice-friendly, auditable transcript proofreading workflow for both archived livestream recordings and imported videos, so product names, model numbers, prices, stock, discounts, and links are never silently changed before highlight analysis.

**Architecture:** Keep ASR output as immutable evidence, then produce a corrected working transcript through deterministic rules, evidence-constrained model proposals, and explicit human decisions. A source-agnostic artifact service owns files and review state; thin Tauri commands expose typed data to a shared Svelte review panel used by archive and video analysis. Approved terminology becomes a local dictionary candidate and can be exported, but this phase does not call undocumented remote dictionary-management APIs.

**Tech Stack:** Rust, Tokio, Serde, SQLx/SQLite, Tauri 2 commands, Svelte 3.54, TypeScript 5, Node test runner, existing Volcengine/FunASR integrations.

## Global Constraints

- `subtitle.raw.srt` is immutable evidence after first successful recognition; re-recognition must preserve the prior raw artifact as a timestamped version.
- `subtitle.corrected.srt` may contain deterministic formatting fixes and human-approved critical-fact corrections only.
- Product names, model numbers, prices, discounts, stock, links, gifts, and warranty terms are critical facts and always require human confirmation when changed.
- The model proposes local edits only; it must not rewrite the full transcript, infer missing commercial facts, or replace a value without evidence.
- Archive and imported-video sources use the same review contract and the same user interface.
- Existing `transcript.*` artifacts remain readable, and existing `subtitle.srt` or sibling `.srt` files remain compatibility outputs.
- Phase 1 ends at a trusted transcript and approved local dictionary candidates. A/B baseline comparison, TOP3 deep analysis, operational funnel analysis, auxiliary-script generation, and mother-script management remain later phases.
- No API key, access token, full signed URL, or unredacted commercial fact card may be written to application logs.
- All novice-facing actions use business language: `保留原文`, `采用修改`, `加入词库候选`, `继续分析`; implementation terms stay inside the collapsed evidence details.

---

## File Structure

- `src-tauri/src/subtitle_generator/asr_text.rs`: pure deterministic boundary normalization.
- `src-tauri/src/subtitle_generator/transcript_artifacts.rs`: source identity, typed audit contract, canonical/legacy file reads, atomic writes, and review resolution.
- `src-tauri/src/database/transcript_dictionary_candidate.rs`: local terminology candidate persistence and export queries.
- `src-tauri/src/handlers/transcript_review.rs`: source-agnostic Tauri commands.
- `src/lib/transcriptReview.ts`: frontend types, invoke adapter, and novice status helpers.
- `src/lib/components/analysis/TranscriptReviewPanel.svelte`: review queue and completion gate.
- `src/lib/components/analysis/TranscriptCorrectionCard.svelte`: one correction decision with collapsed evidence.
- `src/lib/components/analysis/DictionaryCandidateDrawer.svelte`: approved candidate list and JSON/CSV export.
- `src/page/ArchiveAnalysis.svelte`: shared archive/video orchestration and right-column mode switching.

### Task 1: Harden Deterministic Commerce-Text Boundaries

**Files:**
- Modify: `src-tauri/src/subtitle_generator/asr_text.rs`
- Modify: `src-tauri/src/subtitle_generator/mod.rs`
- Modify: `src-tauri/src/subtitle_generator/volcengine.rs`
- Test: `src-tauri/src/subtitle_generator/asr_text.rs`

**Interfaces:**
- Consumes: each Volcengine utterance text before SRT serialization.
- Produces: `pub fn normalize_commerce_text_boundaries(input: &str) -> String`.

- [ ] **Step 1: Add failing boundary and preservation cases**

```rust
#[cfg(test)]
mod tests {
    use super::normalize_commerce_text_boundaries;

    #[test]
    fn separates_model_from_condition_phrase() {
        assert_eq!(normalize_commerce_text_boundaries("影石A4PRO299新"), "影石A4PRO2 99新");
        assert_eq!(normalize_commerce_text_boundaries("A4 Pro 2九十九新"), "A4 Pro 2 九十九新");
    }

    #[test]
    fn preserves_model_and_unrelated_numbers() {
        assert_eq!(normalize_commerce_text_boundaries("影石A4PRO2"), "影石A4PRO2");
        assert_eq!(normalize_commerce_text_boundaries("到手价199新币"), "到手价199新币");
        assert_eq!(normalize_commerce_text_boundaries("R50白色"), "R50白色");
    }
}
```

- [ ] **Step 2: Run the focused test and confirm the new Chinese-number case fails**

Run: `rustc --edition=2021 --test src-tauri/src/subtitle_generator/asr_text.rs -o "$env:TEMP\asr_text_tests.exe"; & "$env:TEMP\asr_text_tests.exe"`

Expected: `separates_model_from_condition_phrase` fails for `九十九新` before implementation.

- [ ] **Step 3: Implement conservative token-boundary rules and apply once per utterance**

```rust
pub fn normalize_commerce_text_boundaries(input: &str) -> String {
    let mut value = input.to_string();
    for marker in ["99新", "九十九新"] {
        let mut search_from = 0;
        while let Some(offset) = value[search_from..].find(marker) {
            let at = search_from + offset;
            let previous = value[..at].chars().next_back();
            if previous.is_some_and(|ch| ch.is_ascii_alphanumeric())
                && !value[..at].ends_with(' ')
            {
                value.insert(at, ' ');
                search_from = at + marker.len() + 1;
            } else {
                search_from = at + marker.len();
            }
        }
    }
    value
}
```

In `volcengine.rs`, normalize every returned text segment before constructing `GenerateResult`; do not normalize the final SRT string because that would lose segment-level provenance.

- [ ] **Step 4: Run focused tests**

Run: `rustc --edition=2021 --test src-tauri/src/subtitle_generator/asr_text.rs -o "$env:TEMP\asr_text_tests.exe"; & "$env:TEMP\asr_text_tests.exe"`

Expected: all tests pass.

- [ ] **Step 5: Commit the boundary normalizer**

```powershell
git add src-tauri/src/subtitle_generator/asr_text.rs src-tauri/src/subtitle_generator/mod.rs src-tauri/src/subtitle_generator/volcengine.rs
git -c user.name="Codex" -c user.email="codex@local" commit -m "fix: preserve commerce transcript boundaries"
```

### Task 2: Add a Typed Canonical Transcript Artifact Store

**Files:**
- Create: `src-tauri/src/subtitle_generator/transcript_artifacts.rs`
- Modify: `src-tauri/src/subtitle_generator/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Test: `src-tauri/src/subtitle_generator/transcript_artifacts.rs`

**Interfaces:**
- Consumes: a resolved source directory plus raw/corrected SRT and model correction metadata.
- Produces: `TranscriptSource`, `TranscriptCorrection`, `TranscriptAuditBundle`, `TranscriptArtifactStore::load_from_dir`, `initialize`, and `resolve`.

- [ ] **Step 1: Define the serializable contract and failing legacy-read test**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum TranscriptSource {
    Archive { platform: String, room_id: String, live_id: String },
    Video { video_id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision { Pending, Approved, KeptOriginal }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCorrection {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub original: String,
    pub proposed: String,
    pub category: String,
    pub evidence: Vec<String>,
    pub critical: bool,
    pub decision: ReviewDecision,
    pub decided_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAuditBundle {
    pub source: TranscriptSource,
    pub raw_srt: String,
    pub corrected_srt: String,
    pub corrections: Vec<TranscriptCorrection>,
    pub pending_critical_count: usize,
}

#[tokio::test]
async fn loads_legacy_transcript_files_when_canonical_files_are_absent() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("transcript.raw.srt"), "legacy raw").await.unwrap();
    tokio::fs::write(dir.path().join("transcript.corrected.srt"), "legacy corrected").await.unwrap();
    let bundle = TranscriptArtifactStore::load_from_dir(
        TranscriptSource::Video { video_id: 7 }, dir.path()
    ).await.unwrap();
    assert_eq!(bundle.raw_srt, "legacy raw");
    assert_eq!(bundle.corrected_srt, "legacy corrected");
}
```

Add the test-only dependency:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run the module test and verify it fails to compile**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_artifacts --lib`

Expected: failure because `TranscriptArtifactStore` is not defined. If the known CMake generator cache blocks Cargo, record that exact infrastructure error and continue with Task 2 Step 4 after adding pure tests that can run under the repository's repaired Rust build environment.

- [ ] **Step 3: Implement canonical paths, legacy fallback, and atomic writes**

```rust
impl TranscriptArtifactStore {
    const RAW: &'static str = "subtitle.raw.srt";
    const CORRECTED: &'static str = "subtitle.corrected.srt";
    const CORRECTIONS: &'static str = "subtitle.corrections.json";

    async fn read_with_fallback(dir: &Path, canonical: &str, legacy: &str) -> io::Result<String> {
        match tokio::fs::read_to_string(dir.join(canonical)).await {
            Ok(value) => Ok(value),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                tokio::fs::read_to_string(dir.join(legacy)).await.or_else(|legacy_error| {
                    if legacy_error.kind() == io::ErrorKind::NotFound { Ok(String::new()) } else { Err(legacy_error) }
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn recoverable_write(path: &Path, value: &[u8]) -> io::Result<()> {
        let temporary = path.with_extension("tmp");
        let backup = path.with_extension("bak");
        tokio::fs::write(&temporary, value).await?;
        if tokio::fs::try_exists(&backup).await? { tokio::fs::remove_file(&backup).await?; }
        if tokio::fs::try_exists(path).await? { tokio::fs::rename(path, &backup).await?; }
        if let Err(error) = tokio::fs::rename(&temporary, path).await {
            if tokio::fs::try_exists(&backup).await? { tokio::fs::rename(&backup, path).await?; }
            return Err(error);
        }
        if tokio::fs::try_exists(&backup).await? { tokio::fs::remove_file(backup).await?; }
        Ok(())
    }
}
```

`initialize` writes canonical raw only when absent, writes corrected/corrections with the recoverable temp/backup sequence, and mirrors corrected SRT to `subtitle.srt`. `load_from_dir` restores a `.bak` file when the canonical target is absent, and returns an error for invalid correction arrays rather than silently discarding audit history.

- [ ] **Step 4: Run artifact-store tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_artifacts --lib`

Expected: legacy fallback, canonical precedence, immutable raw, and atomic corrected-write tests pass.

- [ ] **Step 5: Commit the artifact store**

```powershell
git add src-tauri/src/subtitle_generator/transcript_artifacts.rs src-tauri/src/subtitle_generator/mod.rs src-tauri/Cargo.toml
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: add transcript artifact store"
```

### Task 3: Use the Artifact Store for Archive and Video ASR

**Files:**
- Modify: `src-tauri/src/recorder_manager.rs`
- Modify: `src-tauri/src/handlers/video.rs`
- Test: `src-tauri/src/subtitle_generator/transcript_artifacts.rs`

**Interfaces:**
- Consumes: `TranscriptArtifactStore::initialize(source, dir, raw_srt, corrected_srt, corrections)`.
- Produces: identical canonical artifacts for archive and video sources while preserving existing public subtitle commands.

- [ ] **Step 1: Add a failing parity test**

```rust
#[tokio::test]
async fn archive_and_video_initialization_write_the_same_artifact_set() {
    for source in [
        TranscriptSource::Archive { platform: "bilibili".into(), room_id: "1".into(), live_id: "2".into() },
        TranscriptSource::Video { video_id: 7 },
    ] {
        let dir = tempfile::tempdir().unwrap();
        TranscriptArtifactStore::initialize(source, dir.path(), "raw", "corrected", vec![]).await.unwrap();
        for name in ["subtitle.raw.srt", "subtitle.corrected.srt", "subtitle.corrections.json", "subtitle.srt"] {
            assert!(dir.path().join(name).exists(), "missing {name}");
        }
    }
}
```

- [ ] **Step 2: Run the parity test and verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml archive_and_video_initialization_write_the_same_artifact_set --lib`

Expected: failure until `initialize` writes all four files.

- [ ] **Step 3: Integrate archive and video generation**

After either Volcengine or FunASR returns, run the existing `minimax_correct_transcript` only with `raw_text`, deterministic text, and the source fact card. Map each returned local edit into `TranscriptCorrection`; copy the provider/model evidence into `evidence`, mark commercial-fact categories as critical, and resolve the source directory before calling:

```rust
TranscriptArtifactStore::initialize(
    TranscriptSource::Video { video_id: id },
    artifact_dir.as_path(),
    &raw_srt,
    &safe_corrected_srt,
    corrections,
).await?;
```

For archive, use the existing archive directory and `TranscriptSource::Archive`. `safe_corrected_srt` must exclude all `critical == true && decision == Pending` proposals. A proposal whose replacement is absent from the fact card must remain pending with `[待确认]` rather than being applied. Keep current return values and progress events unchanged so existing callers do not regress.

- [ ] **Step 4: Verify focused Rust tests and frontend build**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_artifacts --lib`

Run: `npm run build`

Expected: Rust artifact tests pass and Vite production build exits successfully.

- [ ] **Step 5: Commit pipeline parity**

```powershell
git add src-tauri/src/recorder_manager.rs src-tauri/src/handlers/video.rs src-tauri/src/subtitle_generator/transcript_artifacts.rs
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: persist auditable transcripts for all sources"
```

### Task 4: Expose Generic Review Decisions

**Files:**
- Create: `src-tauri/src/handlers/transcript_review.rs`
- Modify: `src-tauri/src/handlers/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/handlers/recorder.rs`
- Modify: `src-tauri/src/recorder_manager.rs`
- Test: `src-tauri/src/subtitle_generator/transcript_artifacts.rs`

**Interfaces:**
- Consumes: `TranscriptSource`, correction ID, `ReviewAction`, optional edited text.
- Produces: `get_transcript_audit(source) -> TranscriptAuditBundle` and `resolve_transcript_correction(request) -> TranscriptAuditBundle`.

- [ ] **Step 1: Add failing decision-state tests**

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction { Approve, KeepOriginal }

fn pending_critical() -> TranscriptCorrection {
    TranscriptCorrection {
        id: "correction-1".into(), start_ms: 0, end_ms: 1_000,
        original: "A4PRO299新".into(), proposed: "A4PRO2 99新".into(),
        category: "型号成色".into(), evidence: vec!["商品事实卡:A4PRO2".into()],
        critical: true, decision: ReviewDecision::Pending, decided_text: None,
    }
}

#[tokio::test]
async fn keep_original_resolves_without_changing_corrected_text() {
    let dir = tempfile::tempdir().unwrap();
    let source = TranscriptSource::Video { video_id: 7 };
    TranscriptArtifactStore::initialize(source, dir.path(), "A4PRO299新", "A4PRO299新", vec![pending_critical()]).await.unwrap();
    let updated = TranscriptArtifactStore::at(dir.path()).resolve("correction-1", ReviewAction::KeepOriginal, None).await.unwrap();
    assert_eq!(updated.corrections[0].decision, ReviewDecision::KeptOriginal);
    assert_eq!(updated.corrected_srt, updated.raw_srt);
    assert_eq!(updated.pending_critical_count, 0);
}

#[tokio::test]
async fn approve_requires_non_empty_decided_text() {
    let dir = tempfile::tempdir().unwrap();
    let source = TranscriptSource::Video { video_id: 7 };
    TranscriptArtifactStore::initialize(source, dir.path(), "A4PRO299新", "A4PRO299新", vec![pending_critical()]).await.unwrap();
    let error = TranscriptArtifactStore::at(dir.path()).resolve("correction-1", ReviewAction::Approve, Some("  ".into())).await.unwrap_err();
    assert!(error.to_string().contains("采用修改时必须提供校对文本"));
}
```

- [ ] **Step 2: Run decision tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_artifacts::tests --lib`

Expected: compile failure until resolution is implemented.

- [ ] **Step 3: Implement commands and compatibility wrappers**

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveTranscriptCorrectionRequest {
    pub source: TranscriptSource,
    pub correction_id: String,
    pub action: ReviewAction,
    pub decided_text: Option<String>,
    pub add_to_dictionary_candidates: bool,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resolve_transcript_correction(
    state: state_type!(),
    request: ResolveTranscriptCorrectionRequest,
) -> Result<TranscriptAuditBundle, String> {
    let dir = resolve_transcript_source_dir(&state, &request.source).await?;
    Ok(TranscriptArtifactStore::at(dir)
        .resolve(&request.correction_id, request.action, request.decided_text)
        .await?)
}
```

Register both generic commands in `main.rs`. Keep `get_archive_transcript_audit` and `resolve_archive_review_item` as wrappers that translate legacy archive arguments/index into the new source and correction ID.

- [ ] **Step 4: Run tests and build**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_artifacts::tests --lib`

Run: `npm run build`

Expected: decision tests and production build pass.

- [ ] **Step 5: Commit the generic review API**

```powershell
git add src-tauri/src/handlers/transcript_review.rs src-tauri/src/handlers/mod.rs src-tauri/src/main.rs src-tauri/src/handlers/recorder.rs src-tauri/src/recorder_manager.rs src-tauri/src/subtitle_generator/transcript_artifacts.rs
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: unify transcript review decisions"
```

### Task 5: Persist Approved Dictionary Candidates

**Files:**
- Create: `src-tauri/src/database/transcript_dictionary_candidate.rs`
- Modify: `src-tauri/src/database/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/handlers/transcript_review.rs`
- Test: `src-tauri/src/database/transcript_dictionary_candidate.rs`

**Interfaces:**
- Consumes: an approved correction whose user selected `add_to_dictionary_candidates`.
- Produces: deduplicated candidates with statuses `pending`, `approved`, `rejected`, `synced`, `sync_failed`; JSON/CSV export payloads.

- [ ] **Step 1: Add migration 15 and a failing deduplication test**

```sql
CREATE TABLE transcript_dictionary_candidates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    candidate_type TEXT NOT NULL CHECK(candidate_type IN ('hotword','replacement','local_rule')),
    source_text TEXT NOT NULL,
    target_text TEXT NOT NULL DEFAULT '',
    evidence_json TEXT NOT NULL DEFAULT '[]',
    source_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','synced','sync_failed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(candidate_type, source_text, target_text)
);
CREATE INDEX idx_transcript_dictionary_candidates_status
ON transcript_dictionary_candidates(status, updated_at DESC);
```

```rust
#[sqlx::test]
async fn upsert_candidate_deduplicates_same_replacement(pool: SqlitePool) {
    let db = Database::from_pool(pool);
    let first = db.upsert_transcript_dictionary_candidate(new_replacement("A4PRO299新", "A4PRO2 99新")).await.unwrap();
    let second = db.upsert_transcript_dictionary_candidate(new_replacement("A4PRO299新", "A4PRO2 99新")).await.unwrap();
    assert_eq!(first.id, second.id);
}
```

- [ ] **Step 2: Run the database test and verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_dictionary_candidate --lib`

Expected: failure until migration and repository methods exist.

- [ ] **Step 3: Implement repository methods and command integration**

```rust
pub async fn upsert_transcript_dictionary_candidate(
    &self,
    input: NewTranscriptDictionaryCandidate,
) -> Result<TranscriptDictionaryCandidateRow, DatabaseError>;

pub async fn list_transcript_dictionary_candidates(
    &self,
    status: Option<&str>,
) -> Result<Vec<TranscriptDictionaryCandidateRow>, DatabaseError>;

pub async fn set_transcript_dictionary_candidate_status(
    &self,
    id: i64,
    status: &str,
) -> Result<TranscriptDictionaryCandidateRow, DatabaseError>;
```

When `Approve` and `add_to_dictionary_candidates == true`, create a `replacement` candidate from original to decided text in the same command. Export JSON as the typed row array and CSV with columns `类型,原文,目标文本,状态,创建时间`; escape embedded quotes by doubling them.

- [ ] **Step 4: Run database tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcript_dictionary_candidate --lib`

Expected: insert, deduplication, status transition, JSON export, and CSV escaping tests pass.

- [ ] **Step 5: Commit candidate persistence**

```powershell
git add src-tauri/src/database/transcript_dictionary_candidate.rs src-tauri/src/database/mod.rs src-tauri/src/main.rs src-tauri/src/handlers/transcript_review.rs
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: queue approved transcript dictionary candidates"
```

### Task 6: Add the Frontend Review Domain Adapter

**Files:**
- Create: `src/lib/transcriptReview.ts`
- Create: `src/lib/transcriptReview.test.ts`
- Modify: `package.json`

**Interfaces:**
- Consumes: generic Tauri commands from Task 4.
- Produces: typed API functions and derived novice-facing review state.

- [ ] **Step 1: Write failing status and gate tests**

```ts
import test from "node:test";
import assert from "node:assert/strict";
import { canContinueAnalysis, reviewStatusLabel } from "./transcriptReview.js";

test("critical pending corrections block analysis", () => {
  assert.equal(canContinueAnalysis({ pendingCriticalCount: 1 }), false);
  assert.equal(canContinueAnalysis({ pendingCriticalCount: 0 }), true);
});

test("review labels use novice language", () => {
  assert.equal(reviewStatusLabel("pending"), "等你确认");
  assert.equal(reviewStatusLabel("approved"), "已采用修改");
  assert.equal(reviewStatusLabel("kept_original"), "已保留原文");
});
```

- [ ] **Step 2: Add and run the focused frontend test script**

Add to `package.json`:

```json
"test:transcript-review": "node --loader ts-node/esm src/lib/transcriptReview.test.ts"
```

Run: `npm run test:transcript-review`

Expected: failure because `transcriptReview.ts` does not exist.

- [ ] **Step 3: Implement types, helpers, and invoke adapters**

```ts
export type TranscriptSource =
  | { kind: "archive"; platform: string; roomId: string; liveId: string }
  | { kind: "video"; videoId: number };

export type ReviewDecision = "pending" | "approved" | "kept_original";

export interface TranscriptCorrection {
  id: string;
  startMs: number;
  endMs: number;
  original: string;
  proposed: string;
  category: string;
  evidence: string[];
  critical: boolean;
  decision: ReviewDecision;
  decidedText: string | null;
}

export interface TranscriptAuditBundle {
  source: TranscriptSource;
  rawSrt: string;
  correctedSrt: string;
  corrections: TranscriptCorrection[];
  pendingCriticalCount: number;
}

export function canContinueAnalysis(bundle: { pendingCriticalCount: number }): boolean {
  return bundle.pendingCriticalCount === 0;
}

export function reviewStatusLabel(decision: ReviewDecision): string {
  return {
    pending: "等你确认",
    approved: "已采用修改",
    kept_original: "已保留原文",
  }[decision];
}
```

Export `getTranscriptAudit(source)`, `resolveTranscriptCorrection(request)`, `listDictionaryCandidates(status)`, `setDictionaryCandidateStatus(id, status)`, and `exportDictionaryCandidates(format)` using `@tauri-apps/api/core` `invoke`.

- [ ] **Step 4: Run domain tests and production build**

Run: `npm run test:transcript-review`

Run: `npm run build`

Expected: tests pass and Vite build succeeds.

- [ ] **Step 5: Commit the frontend domain layer**

```powershell
git add src/lib/transcriptReview.ts src/lib/transcriptReview.test.ts package.json
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: add transcript review client"
```

### Task 7: Build the Novice Proofreading Panel

**Files:**
- Create: `src/lib/components/analysis/TranscriptCorrectionCard.svelte`
- Create: `src/lib/components/analysis/TranscriptReviewPanel.svelte`
- Create: `src/lib/components/analysis/DictionaryCandidateDrawer.svelte`
- Modify: `src/page/ArchiveAnalysis.svelte`
- Test: `src/lib/transcriptReview.test.ts`

**Interfaces:**
- Consumes: `TranscriptAuditBundle`, current correction ID, review and export functions from Task 6.
- Produces: shared middle-column highlighting, right-column review decisions, dictionary drawer, and a completion event.

- [ ] **Step 1: Add failing view-model tests**

```ts
import { correctionProgress, firstPendingCorrectionId } from "./transcriptReview.js";

test("progress counts every explicit decision", () => {
  const corrections = [
    { id: "a", decision: "approved" },
    { id: "b", decision: "kept_original" },
    { id: "c", decision: "pending" },
  ] as const;
  assert.deepEqual(correctionProgress(corrections), { completed: 2, total: 3 });
  assert.equal(firstPendingCorrectionId(corrections), "c");
});
```

- [ ] **Step 2: Run tests and verify the missing helpers fail**

Run: `npm run test:transcript-review`

Expected: import failure for `correctionProgress` and `firstPendingCorrectionId`.

- [ ] **Step 3: Implement the panel and card interactions**

`TranscriptCorrectionCard.svelte` renders, in this order: time range, `听到的内容`, `建议改成`, one-sentence business reason, `保留原文`, `采用修改`, an `加入词库候选` checkbox, and a collapsed `查看识别依据`. Disable both decision buttons while an invoke is running and show the returned error inline.

```svelte
<button class="secondary" disabled={submitting} on:click={() => decide("keep_original")}>保留原文</button>
<button class="primary" disabled={submitting || !editedText.trim()} on:click={() => decide("approve")}>采用修改</button>
<label><input type="checkbox" bind:checked={addToDictionary} /> 加入词库候选</label>
<details><summary>查看识别依据</summary>{#each correction.evidence as item}<p>{item}</p>{/each}</details>
```

`TranscriptReviewPanel.svelte` shows one pending item at a time, `已处理 X/Y`, previous/next icon buttons with tooltips, and emits `complete` only when `pendingCriticalCount === 0`. `DictionaryCandidateDrawer.svelte` lists pending/approved/rejected rows and uses icon buttons with tooltips for JSON and CSV export.

- [ ] **Step 4: Integrate with the three-column analysis page**

Add right-column tabs `校稿审核` and `片段分析`. After source selection, load the audit bundle; if it contains critical pending items, select `校稿审核`. Clicking a correction seeks the left video to `start_ms / 1000` and scrolls/highlights the matching middle transcript line. Keep the existing archive/video source identity reactivity fix and existing analysis result component.

- [ ] **Step 5: Run tests and build**

Run: `npm run test:transcript-review`

Run: `npm run test:analysis-reactivity`

Run: `npm run build`

Expected: both focused test suites pass and production build succeeds.

- [ ] **Step 6: Commit the novice review panel**

```powershell
git add src/lib/components/analysis/TranscriptCorrectionCard.svelte src/lib/components/analysis/TranscriptReviewPanel.svelte src/lib/components/analysis/DictionaryCandidateDrawer.svelte src/page/ArchiveAnalysis.svelte src/lib/transcriptReview.ts src/lib/transcriptReview.test.ts
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: add novice transcript review workspace"
```

### Task 8: Gate Highlight Discovery on Critical-Fact Review

**Files:**
- Modify: `src/page/ArchiveAnalysis.svelte`
- Modify: `src/lib/transcriptReview.ts`
- Modify: `src/lib/transcriptReview.test.ts`

**Interfaces:**
- Consumes: `canContinueAnalysis(bundle)` and panel `complete` event.
- Produces: one explicit, identical archive/video transition from trusted transcript to highlight discovery.

- [ ] **Step 1: Add a failing workflow-state test**

```ts
import { nextAnalysisStage } from "./transcriptReview.js";

test("workflow enters proofreading before highlight analysis", () => {
  assert.equal(nextAnalysisStage({ hasSubtitle: false, pendingCriticalCount: 0 }), "recognizing");
  assert.equal(nextAnalysisStage({ hasSubtitle: true, pendingCriticalCount: 2 }), "proofreading");
  assert.equal(nextAnalysisStage({ hasSubtitle: true, pendingCriticalCount: 0 }), "discovering_highlights");
});
```

- [ ] **Step 2: Run the workflow test and verify failure**

Run: `npm run test:transcript-review`

Expected: import failure for `nextAnalysisStage`.

- [ ] **Step 3: Implement the finite workflow decision**

```ts
export type AnalysisStage = "recognizing" | "proofreading" | "discovering_highlights";

export function nextAnalysisStage(input: {
  hasSubtitle: boolean;
  pendingCriticalCount: number;
}): AnalysisStage {
  if (!input.hasSubtitle) return "recognizing";
  if (input.pendingCriticalCount > 0) return "proofreading";
  return "discovering_highlights";
}
```

In `ArchiveAnalysis.svelte`, replace the direct ASR-to-discovery call with this state decision. Show `全部处理完成，继续分析` after the last critical decision; only that click invokes existing candidate discovery. Non-critical deterministic formatting does not block the button.

- [ ] **Step 4: Verify archive and video workflows**

Run: `npm run test:transcript-review`

Run: `npm run test:analysis-reactivity`

Run: `npm run build`

Expected: all commands pass. Manually verify one archive and one imported video: source selection loads video, transcript, and audit; a pending model/price correction blocks discovery; `保留原文` and `采用修改` both resolve it; the continue button starts existing highlight discovery.

- [ ] **Step 5: Commit the workflow gate**

```powershell
git add src/page/ArchiveAnalysis.svelte src/lib/transcriptReview.ts src/lib/transcriptReview.test.ts
git -c user.name="Codex" -c user.email="codex@local" commit -m "feat: require transcript review before highlight analysis"
```

### Task 9: Final Regression and Audit Verification

**Files:**
- Modify only files required to fix failures found by this task.

**Interfaces:**
- Consumes: complete Phase 1 implementation.
- Produces: evidence that existing analysis still builds and the new audit chain is usable.

- [ ] **Step 1: Run all focused automated checks**

```powershell
npm run test:analysis-reactivity
npm run test:transcript-review
npm run build
rustc --edition=2021 --test src-tauri/src/subtitle_generator/asr_text.rs -o "$env:TEMP\asr_text_tests.exe"
& "$env:TEMP\asr_text_tests.exe"
cargo test --manifest-path src-tauri/Cargo.toml transcript_artifacts --lib
cargo test --manifest-path src-tauri/Cargo.toml transcript_dictionary_candidate --lib
```

Expected: every command passes. If Cargo is blocked by the existing CMake generator mismatch, repair the build environment without deleting user data, rerun both Cargo commands, and record the exact successful output before completion.

- [ ] **Step 2: Verify canonical files and immutability manually**

For one archive and one imported video, confirm these files exist beside the source artifact: `subtitle.raw.srt`, `subtitle.corrected.srt`, `subtitle.corrections.json`, and compatibility `subtitle.srt`. Record the SHA-256 of `subtitle.raw.srt`, resolve one correction, and confirm the raw hash is unchanged while corrected/corrections files change.

- [ ] **Step 3: Verify novice UI at desktop and narrow widths**

Run the desktop app, capture screenshots at approximately `1920x1080` and `1280x720`, and verify: no text overlaps; video, transcript, and right review remain usable; decision buttons fit; evidence is collapsed initially; long product models wrap without changing toolbar dimensions.

- [ ] **Step 4: Verify audit and security behavior**

Confirm an approved critical correction records original text, decided text, evidence, decision, source identity, and timestamp. Search logs for configured API key/token values and confirm there are zero matches. Confirm no remote hotword/replacement API is called when adding a local candidate.

- [ ] **Step 5: Commit only verification-driven fixes**

```powershell
git status --short
git add src-tauri/src/subtitle_generator/asr_text.rs src-tauri/src/subtitle_generator/transcript_artifacts.rs src-tauri/src/subtitle_generator/mod.rs src-tauri/src/subtitle_generator/volcengine.rs src-tauri/src/recorder_manager.rs src-tauri/src/handlers/video.rs src-tauri/src/handlers/transcript_review.rs src-tauri/src/handlers/mod.rs src-tauri/src/handlers/recorder.rs src-tauri/src/database/transcript_dictionary_candidate.rs src-tauri/src/database/mod.rs src-tauri/src/main.rs src/lib/transcriptReview.ts src/lib/transcriptReview.test.ts src/lib/components/analysis/TranscriptCorrectionCard.svelte src/lib/components/analysis/TranscriptReviewPanel.svelte src/lib/components/analysis/DictionaryCandidateDrawer.svelte src/page/ArchiveAnalysis.svelte package.json
git -c user.name="Codex" -c user.email="codex@local" commit -m "fix: close transcript review regressions"
```

Skip the commit when verification requires no code change.

## Phase Boundary

After this plan is complete, the next implementation plan starts from `subtitle.corrected.srt` and builds the company analysis framework: global merchandise and transition diagnosis, operational funnel (`互动 -> 停留 -> 加购 -> 下单`), module classification, B-baseline comparison where A only patches B, TOP3 deep analysis, and reusable auxiliary-script output. That later plan must consume only trusted transcript facts and must preserve evidence links back to timestamped video segments.
