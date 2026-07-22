# Master Script Baseline Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a recoverable multi-product master-script pipeline, compare later transaction clips against the published master, and route only gate-passing scores of at least 85 into a human-reviewed support-script queue.

**Architecture:** Add a focused Rust domain crate for immutable score and lifecycle rules, persist master jobs/versions/sections/support candidates in SQLite, and add controlled atomic Markdown writes beside the existing read-only Obsidian snapshot. Reuse the current transcript artifacts, Volcengine ten-minute ASR chunks, MiniMax client, and three-column analysis page; keep source transcript, faithful master text, and AI rewrite candidates as distinct assets.

**Tech Stack:** Rust 2021, Tokio, SQLx/SQLite, serde, Tauri 2, Svelte 3, TypeScript, Vitest-style Node assertions already used by the project, FFmpeg/FFprobe, Obsidian Markdown with YAML frontmatter.

## Global Constraints

- The master is one whole multi-product live session, internally organized as ordered opening, product, transition, scenario, and closing sections.
- The model may correct, segment, classify, and structure source speech; it may not label rewritten wording as host speech.
- Raw ASR, reviewed transcript, structured master, support candidate, and AI rewrite candidate remain separate.
- Dynamic price, inventory, discount, condition, link number, and transaction status must never be inferred from parameter cards or context.
- A score is valid only after all hard gates pass and must be recomputed locally from six bounded integer dimensions totaling 100.
- Total score `85-100` enters the support-candidate queue; `70-84` is review-only and `0-69` is analysis-only.
- Queue admission never modifies the published master. Only an explicit human approval creates a new immutable master version.
- Every master section and support candidate retains source identity, absolute time range, transcript evidence, model version, and prompt version.
- Database migrations are immutable after release. New schema changes always receive a new migration number after version 18.
- Obsidian writes use same-directory temporary files, atomic rename, read-back hash verification, and an audit record. Existing Vault documents are never silently overwritten.
- User-facing text is Chinese and beginner-readable. Raw JSON and internal English statuses stay behind diagnostic controls.

---

### Task 1: Master-script domain rules

**Files:**
- Create: `src-tauri/crates/master-script/Cargo.toml`
- Create: `src-tauri/crates/master-script/src/lib.rs`
- Create: `src-tauri/crates/master-script/tests/scoring.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: `ScoreBreakdown`, `HardGateResult`, `CandidateAdmission`, `MasterSectionKind`, `SupportCandidateStatus`.
- Produces: `evaluate_admission(gates: &HardGateResult, score: &ScoreBreakdown) -> CandidateAdmission`.
- Produces: `next_patch_version(current: &str) -> Result<String, MasterScriptError>`.

- [ ] **Step 1: Write failing domain tests**

```rust
use master_script::{evaluate_admission, next_patch_version, CandidateAdmission, HardGateResult, ScoreBreakdown};

fn passing_gates() -> HardGateResult {
    HardGateResult {
        transcript_reviewed: true,
        master_section_matched: true,
        context_complete: true,
        facts_resolved: true,
        transaction_evidence_valid: true,
        host_speech_backed: true,
        not_duplicate: true,
        reasons: vec![],
    }
}

#[test]
fn admits_scores_at_85_after_all_gates_pass() {
    let score_84 = ScoreBreakdown::new(20, 20, 17, 13, 9, 5).unwrap();
    assert_eq!(evaluate_admission(&passing_gates(), &score_84), CandidateAdmission::ReviewOnly);
    let score_85 = ScoreBreakdown::new(20, 20, 18, 13, 9, 5).unwrap();
    assert_eq!(evaluate_admission(&passing_gates(), &score_85), CandidateAdmission::CandidateQueue);
}

#[test]
fn a_failed_gate_blocks_a_perfect_score() {
    let mut gates = passing_gates();
    gates.facts_resolved = false;
    gates.reasons.push("价格仍待确认".into());
    let score = ScoreBreakdown::new(25, 25, 20, 15, 10, 5).unwrap();
    assert_eq!(evaluate_admission(&gates, &score), CandidateAdmission::Blocked);
}

#[test]
fn validates_dimension_caps_and_versions() {
    assert!(ScoreBreakdown::new(26, 25, 20, 15, 10, 5).is_err());
    assert_eq!(next_patch_version("1.0.9").unwrap(), "1.0.10");
}
```

- [ ] **Step 2: Run the tests and verify failure**

Run: `cargo test -p master-script --test scoring`

Expected: FAIL because the crate and public contracts do not exist.

- [ ] **Step 3: Implement the domain crate**

Add `master-script` to `workspace.members`, then implement bounded score construction and deterministic admission:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBreakdown {
    pub transaction_evidence: u8,
    pub improvement_over_master: u8,
    pub reusability: u8,
    pub completeness: u8,
    pub factual_accuracy: u8,
    pub scenario_clarity: u8,
    pub total: u8,
}

impl ScoreBreakdown {
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Result<Self, MasterScriptError> {
        if a > 25 || b > 25 || c > 20 || d > 15 || e > 10 || f > 5 {
            return Err(MasterScriptError::InvalidScore);
        }
        Ok(Self { transaction_evidence: a, improvement_over_master: b, reusability: c,
            completeness: d, factual_accuracy: e, scenario_clarity: f,
            total: a + b + c + d + e + f })
    }
}

pub fn evaluate_admission(gates: &HardGateResult, score: &ScoreBreakdown) -> CandidateAdmission {
    if !gates.all_pass() { CandidateAdmission::Blocked }
    else if score.total() >= 85 { CandidateAdmission::CandidateQueue }
    else if score.total() >= 70 { CandidateAdmission::ReviewOnly }
    else { CandidateAdmission::AnalysisOnly }
}
```

- [ ] **Step 4: Run domain tests**

Run: `cargo test -p master-script`

Expected: all tests PASS, including exact boundary checks for 69, 70, 84, and 85.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/crates/master-script
git commit -m "feat: define master script admission rules"
```

### Task 2: Immutable SQLite master assets and jobs

**Files:**
- Create: `src-tauri/src/database/master_script.rs`
- Modify: `src-tauri/src/database/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Produces: migration version 19, `MASTER_SCRIPT_MIGRATION_SQL`.
- Produces: `Database::create_master_source`, `upsert_master_chunk`, `publish_master_version`, `insert_support_candidate`, `decide_support_candidate`.
- Produces inputs `NewMasterSource`, `MasterChunkInput`, `NewMasterVersion`, and `NewSupportCandidate`; row DTOs expose generated `id` plus persisted fields.
- Produces `DatabaseError::InvalidMasterScriptState(String)` for lifecycle or admission inconsistencies.
- Consumes: domain statuses and score JSON from Task 1.

- [ ] **Step 1: Write failing in-memory database tests**

Use a one-connection in-memory pool, execute `MASTER_SCRIPT_MIGRATION_SQL`, and attach it with `Database::set`. Implement these tests with the exact public inputs from this task:

```rust
async fn test_db() -> Database {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    sqlx::Executor::execute(&pool, MASTER_SCRIPT_MIGRATION_SQL).await.unwrap();
    let db = Database::new();
    db.set(pool).await;
    db
}

fn source_input() -> NewMasterSource {
    NewMasterSource {
        source_kind: "video".into(), source_key: "video:7".into(),
        media_path: r"C:\fixtures\master.ts".into(), media_hash: "media-hash".into(),
        duration_ms: 3_600_000,
    }
}

#[tokio::test]
async fn chunk_checkpoint_is_idempotent_and_resumable() {
    let db = test_db().await;
    let source = db.create_master_source(source_input()).await.unwrap();
    let mut chunk = MasterChunkInput {
        source_id: source.id, chunk_index: 0, start_ms: 0, end_ms: 600_000,
        status: "complete".into(), input_hash: "chunk-hash".into(),
        raw_srt: "raw-v1".into(), reviewed_srt: "reviewed-v1".into(), error: None,
    };
    let first = db.upsert_master_chunk(chunk.clone()).await.unwrap();
    chunk.reviewed_srt = "reviewed-v2".into();
    let second = db.upsert_master_chunk(chunk).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.reviewed_srt, "reviewed-v2");
    assert_eq!(db.list_master_chunks(source.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn published_master_versions_are_immutable() {
    let db = test_db().await;
    let source = db.create_master_source(source_input()).await.unwrap();
    let v1 = db.publish_master_version(NewMasterVersion {
        script_key: "MS-001".into(), version: "1.0.0".into(), source_id: source.id,
        title: "整场母稿".into(), index_relative_path: "10-企业母稿/MS-001/V1.0.md".into(),
        content_hash: "v1-hash".into(), sections: vec![],
    }).await.unwrap();
    db.publish_master_version(NewMasterVersion {
        script_key: "MS-001".into(), version: "1.0.1".into(), source_id: source.id,
        title: "整场母稿".into(), index_relative_path: "10-企业母稿/MS-001/V1.0.1.md".into(),
        content_hash: "v2-hash".into(), sections: vec![],
    }).await.unwrap();
    let persisted_v1 = db.get_master_version(v1.id).await.unwrap();
    assert_eq!(persisted_v1.version, "1.0.0");
    assert_eq!(persisted_v1.content_hash, "v1-hash");
}

#[tokio::test]
async fn support_candidate_dedupes_by_source_time_and_master_version() {
    let (db, master, section) = seeded_master().await;
    let input = queued_candidate(master.id, section.id, 85);
    let first = db.insert_support_candidate(input.clone()).await.unwrap();
    let second = db.insert_support_candidate(input).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(db.list_support_candidates(Some("pending_review")).await.unwrap().len(), 1);
}

#[tokio::test]
async fn score_84_cannot_be_persisted_as_queued() {
    let (db, master, section) = seeded_master().await;
    let error = db.insert_support_candidate(queued_candidate(master.id, section.id, 84))
        .await.unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidMasterScriptState(_)));
}
```

`seeded_master()` creates an in-memory database, one source, published V1.0 and one product section. `queued_candidate()` creates passing gates and a valid bounded `ScoreBreakdown`; for total 84 it deliberately requests `CandidateQueue` so the persistence boundary must reject it.

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test master_script_database_tests`

Expected: FAIL because the migration and methods are absent.

- [ ] **Step 3: Add migration 19 without modifying migrations 16-18**

Create tables with foreign keys and unique constraints:

```sql
CREATE TABLE master_sources (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_kind TEXT NOT NULL CHECK(source_kind IN ('archive','video')),
  source_key TEXT NOT NULL UNIQUE,
  media_path TEXT NOT NULL,
  media_hash TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('queued','transcribing','reviewing','structuring','ready','failed')),
  error TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE master_transcript_chunks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_id INTEGER NOT NULL REFERENCES master_sources(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending','running','complete','failed')),
  input_hash TEXT NOT NULL,
  raw_srt TEXT NOT NULL DEFAULT '',
  reviewed_srt TEXT NOT NULL DEFAULT '',
  error TEXT,
  UNIQUE(source_id, chunk_index)
);
CREATE TABLE master_scripts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  script_key TEXT NOT NULL,
  version TEXT NOT NULL,
  source_id INTEGER NOT NULL REFERENCES master_sources(id),
  title TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('draft','pending_review','published','retired')),
  index_relative_path TEXT NOT NULL DEFAULT '',
  content_hash TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  published_at TEXT,
  UNIQUE(script_key, version)
);
CREATE TABLE master_sections (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  master_script_id INTEGER NOT NULL REFERENCES master_scripts(id) ON DELETE CASCADE,
  section_key TEXT NOT NULL,
  position INTEGER NOT NULL,
  section_kind TEXT NOT NULL,
  product_card_id TEXT,
  title TEXT NOT NULL,
  source_start_ms INTEGER NOT NULL,
  source_end_ms INTEGER NOT NULL,
  host_text TEXT NOT NULL,
  master_text TEXT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  UNIQUE(master_script_id, section_key)
);
CREATE TABLE support_script_candidates (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  candidate_key TEXT NOT NULL UNIQUE,
  master_script_id INTEGER NOT NULL REFERENCES master_scripts(id),
  master_section_id INTEGER NOT NULL REFERENCES master_sections(id),
  source_key TEXT NOT NULL,
  source_start_ms INTEGER NOT NULL,
  source_end_ms INTEGER NOT NULL,
  host_text TEXT NOT NULL,
  comparison_json TEXT NOT NULL,
  gates_json TEXT NOT NULL,
  score_json TEXT NOT NULL,
  total_score INTEGER NOT NULL CHECK(total_score BETWEEN 0 AND 100),
  admission TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending_review','approved','held','returned','rejected','merged')),
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  decided_at TEXT
);
```

- [ ] **Step 4: Implement transactional methods and consistency checks**

`insert_support_candidate` must deserialize `gates_json` and `score_json`, call `evaluate_admission`, and reject a requested persisted admission that differs from the locally computed value.

- [ ] **Step 5: Run database and migration tests**

Run: `cargo test master_script_database_tests`

Expected: all tests PASS and existing knowledge migration tests remain green.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/database/master_script.rs src-tauri/src/database/mod.rs src-tauri/src/main.rs
git commit -m "feat: persist master scripts and support candidates"
```

### Task 3: Knowledge retrieval and controlled Markdown writer

**Files:**
- Modify: `src-tauri/crates/knowledge/src/lib.rs`
- Modify: `src-tauri/crates/knowledge/tests/vault_scan.rs`
- Modify: `src-tauri/crates/knowledge-store/src/lib.rs`
- Create: `src-tauri/src/knowledge_writer.rs`
- Modify: `src-tauri/src/database/knowledge.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Produces: `KnowledgeDocumentRecord` and `list_eligible_documents(card_types: &[String])`.
- Produces migration version 20 with `knowledge_documents.asr_eligible INTEGER NOT NULL DEFAULT 0` and index `idx_knowledge_documents_asr_eligible`.
- Produces: `list_asr_parameter_cards() -> Vec<KnowledgeDocumentRecord>`; this is independent of conversational-knowledge eligibility.
- Produces: `write_verified_markdown(vault, relative_path, expected_absent, content) -> WriteReceipt`.
- Adds recognized directories `10-企业母稿` and `11-场景应对`.

- [ ] **Step 1: Write failing query and atomic-write tests**

Cover eligible filtering, ASR parameter-card filtering, no path traversal, refusal to overwrite, UTF-8 read-back, hash equality, temporary-file cleanup, and unchanged external sample Vault fingerprint for read-only sync. A structurally valid company card under `08-产品参数库` must be `asr_eligible=1` even when its general knowledge status is `pending_review`; malformed or restricted cards must remain ineligible for ASR.

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p knowledge && cargo test -p knowledge-store && cargo test knowledge_writer_tests`

Expected: writer and query tests fail because interfaces are absent; existing tests pass.

- [ ] **Step 3: Implement eligible body queries**

General retrieval returns only `active=1 AND eligible=1`. ASR retrieval returns only `active=1 AND asr_eligible=1`. Set `asr_eligible` only for structurally valid, unrestricted documents under `08-产品参数库` or `02-别名与ASR纠错`; do not promote them to general `eligible`. Filter card types with bound parameters, never SQL string interpolation. Returned records include `card_id`, `title`, `card_type`, `version`, `relative_path`, `metadata_json`, and `body`.

- [ ] **Step 4: Implement controlled atomic writer**

```rust
pub async fn write_verified_markdown(
    vault: &Path,
    relative_path: &Path,
    expected_absent: bool,
    content: &str,
) -> Result<WriteReceipt, String> {
    let destination = checked_child(vault, relative_path)?;
    if expected_absent && destination.exists() { return Err("目标母稿版本已经存在".into()); }
    let parent = destination.parent().ok_or("目标路径无效")?;
    tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, content.as_bytes()).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&temporary, &destination).await.map_err(|e| e.to_string())?;
    let read_back = tokio::fs::read(&destination).await.map_err(|e| e.to_string())?;
    if read_back != content.as_bytes() { return Err("母稿写入校验失败".into()); }
    Ok(WriteReceipt::from_bytes(relative_path, &read_back))
}
```

- [ ] **Step 5: Run all knowledge tests**

Run: `cargo test -p knowledge && cargo test -p knowledge-store && cargo test knowledge_writer_tests`

Expected: PASS; external sample Vault test confirms read-only synchronization still changes no files.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/crates/knowledge src-tauri/crates/knowledge-store src-tauri/src/knowledge_writer.rs src-tauri/src/database/knowledge.rs src-tauri/src/main.rs
git commit -m "feat: add controlled master script knowledge writes"
```

### Task 4: Recoverable six-hour master transcription

**Files:**
- Create: `src-tauri/src/master_script/mod.rs`
- Create: `src-tauri/src/master_script/ingest.rs`
- Modify: `src-tauri/src/ffmpeg/mod.rs`
- Modify: `src-tauri/src/subtitle_generator/volcengine.rs`
- Modify: `src-tauri/src/handlers/video.rs`

**Interfaces:**
- Produces: `start_master_ingest(source: AnalysisSource) -> MasterIngestStatus`.
- Produces: `resume_master_ingest(source_id: i64) -> MasterIngestStatus`.
- Produces: `select_parameter_cards(title, recognized_terms, manually_selected_ids, available_cards, limit=30) -> SelectedParameterCards`.
- Consumes: existing ten-minute Volcengine chunks and transcript artifact rules.

- [ ] **Step 1: Write failing chunk-resume tests**

Use a fake transcriber and three chunk fixtures. Assert that after chunk 2 fails, retry processes only chunks 2 and 3; chunk 1 content/hash remain unchanged. Assert absolute timestamps add `chunk_index * 600_000` and overlapping cues are deduplicated. Add parameter-card fixtures proving manual selections rank first, exact standard names/aliases outrank token overlap, duplicates collapse by card ID, malformed cards never enter selection, and no more than 30 cards are sent to ASR/model context.

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test master_ingest_tests`

Expected: FAIL because the orchestration module does not exist.

- [ ] **Step 3: Extract reusable chunk planning from the current Volcengine path**

Create pure `plan_asr_chunks(duration_ms, 600_000)` and `offset_chunk_result(result, start_ms)`. Keep ordinary subtitle generation behavior unchanged.

- [ ] **Step 4: Implement checkpointed orchestration**

Load ASR-eligible cards from Task 3. Rank manual selections first, then exact canonical-name/alias matches in the title and recognized terms, then descending normalized token overlap; break ties by card ID for deterministic output. For each chunk: compute input hash including selected card IDs/versions, skip matching `complete` rows, set `running`, transcribe with selected parameter-card context, persist raw/reviewed SRT, then set `complete`. On error set only that chunk to `failed`, preserve completed chunks, and surface a resumable status.

- [ ] **Step 5: Preserve canonical transcript artifacts**

After all chunks complete, merge into `subtitle.raw.srt`, `subtitle.corrected.srt`, and `transcript.review.json` using `TranscriptArtifactStore`; do not overwrite raw artifacts on retry.

- [ ] **Step 6: Run transcription regression tests**

Run: `cargo test master_ingest_tests && cargo test transcript_artifacts && npm run test:canonical-video-subtitles`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/master_script src-tauri/src/ffmpeg/mod.rs src-tauri/src/subtitle_generator/volcengine.rs src-tauri/src/handlers/video.rs
git commit -m "feat: add resumable master transcript ingestion"
```

### Task 5: Faithful multi-product master builder

**Files:**
- Create: `src-tauri/src/master_script/builder.rs`
- Create: `src-tauri/src/master_script/model.rs`
- Modify: `src-tauri/src/handlers/ai.rs`
- Create: `src-tauri/src/handlers/master_script.rs`
- Modify: `src-tauri/src/handlers/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Produces commands: `preview_master_script`, `publish_master_script`, `get_master_script_status`.
- Produces model JSON: ordered `MasterSectionDraft[]` with source cue IDs, section kind, product card ID, host text, faithful master text, conditions, and dynamic fields.

- [ ] **Step 1: Write failing builder validation tests**

Tests reject sections with missing cue IDs, source ranges outside the transcript, `master_text` facts absent from host text/parameter cards, overlapping positions, and AI rewrite labels presented as host speech. Tests accept ordered opening/product/transition/closing sections covering multiple products.

- [ ] **Step 2: Run and verify failure**

Run: `cargo test master_builder_tests`

Expected: FAIL because builder contracts do not exist.

- [ ] **Step 3: Expose the shared MiniMax request internally**

Change `request_minimax_text` visibility to `pub(crate)` and retain its existing timeout/retry behavior. Do not add a second HTTP client implementation.

- [ ] **Step 4: Implement bounded model requests**

Process reviewed transcript in ordered blocks with neighboring context. Require JSON only. The system prompt must explicitly state: preserve host wording, do not polish, cite cue IDs, use parameter cards only for static terminology, and emit a separate `scenario` section for random interactions.

- [ ] **Step 5: Validate and preview without writing**

`preview_master_script` returns sections plus blocking issues. Publishing is disabled until all key transcript corrections and builder issues are resolved.

- [ ] **Step 6: Publish V1.0 transactionally**

Render index and section Markdown, write every file with Task 3's verified writer, then insert `master_scripts`/`master_sections` in one database transaction. On any file failure, delete only newly created files listed in this publish attempt and leave no published database row.

- [ ] **Step 7: Run builder, handler, and knowledge tests**

Run: `cargo test master_builder_tests && cargo test master_script_handler_tests && cargo test -p knowledge`

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/src/master_script src-tauri/src/handlers src-tauri/src/main.rs
git commit -m "feat: build faithful multi-product master scripts"
```

### Task 6: Mother-baseline comparison and deterministic score gate

**Files:**
- Create: `src-tauri/src/master_script/comparison.rs`
- Modify: `src-tauri/src/handlers/master_script.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Produces command: `compare_highlight_to_master(source, candidate, transcript_revision) -> MasterComparison`.
- Consumes latest published master, matching product card, reviewed clip transcript, and Task 1 scoring rules.

- [ ] **Step 1: Write failing comparison tests**

Cover exact product/section match, no-match response without a score, stale master version detection, score dimension cap rejection, hard-gate block despite 100 model points, and 85-point queue admission.

- [ ] **Step 2: Run and verify failure**

Run: `cargo test master_comparison_tests`

Expected: FAIL because comparison service is absent.

- [ ] **Step 3: Implement two-stage comparison**

Stage one resolves `master_script_id` and `master_section_id` using product card ID plus section kind. Stage two asks the model for evidence-backed gates, six raw dimensions, improvements, risks, and suggested insertion point. The server validates citations, constructs `ScoreBreakdown`, calls `evaluate_admission`, and ignores any model-provided total/admission.

- [ ] **Step 4: Persist admitted candidates only through the database contract**

`CandidateQueue` inserts `pending_review`; `ReviewOnly` and `AnalysisOnly` return analysis without insertion; `Blocked` returns reasons. Generate `candidate_key` from source identity, absolute time range, transcript hash, master script ID, and master section ID.

- [ ] **Step 5: Run comparison and database tests**

Run: `cargo test master_comparison_tests && cargo test master_script_database_tests`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/master_script/comparison.rs src-tauri/src/handlers/master_script.rs src-tauri/src/main.rs
git commit -m "feat: score highlights against the published master"
```

### Task 7: Frontend master models and import workflow

**Files:**
- Create: `src/lib/masterScript.ts`
- Create: `src/lib/masterScript.test.ts`
- Create: `src/lib/components/master/MasterSourceDialog.svelte`
- Create: `src/lib/components/master/MasterBuildProgress.svelte`
- Create: `src/lib/components/master/MasterPreview.svelte`
- Modify: `src/lib/components/ImportVideoDialog.svelte`
- Modify: `src/lib/components/settings/KnowledgeVaultSettings.svelte`
- Modify: `src/lib/knowledge.ts`
- Modify: `src/lib/knowledge.test.ts`
- Modify: `src/page/AI.svelte`
- Modify: `package.json`

**Interfaces:**
- Produces beginner-facing state labels, progress calculation, publish gating, and Tauri command wrappers.
- Consumes Task 4 and Task 5 commands.

- [ ] **Step 1: Write failing frontend state tests**

```ts
assert.equal(masterStatusLabel("transcribing"), "正在生成母稿逐字稿");
assert.deepEqual(masterPublishGate({ pendingCriticalCount: 1, blockingIssues: [] }), {
  allowed: false,
  reason: "请先确认逐字稿中的关键内容",
});
assert.equal(chunkProgress(6, 10), 60);
```

- [ ] **Step 2: Run and verify failure**

Run: `npm run test:master-script`

Expected: FAIL because `src/lib/masterScript.ts` does not exist.

- [ ] **Step 3: Implement typed command client and pure UI rules**

Add `"test:master-script": "node --loader ts-node/esm src/lib/masterScript.test.ts"` and define DTOs matching Rust camelCase serialization exactly. Extend knowledge status DTOs with `asrEligibleCount` and present it as `可用于 ASR 纠错` instead of counting these company cards as waiting for general knowledge approval.

- [ ] **Step 4: Build the import and preview components**

Add a `导入后作为整场母稿` checkbox to `ImportVideoDialog.svelte`; after import, `AI.svelte` opens `MasterSourceDialog` instead of immediately opening ordinary three-column analysis. Allow automatic parameter-card selection with a visible count and a manual add/remove list. Show stable progress, failed-chunk retry, reviewed-transcript requirement, ordered section preview, source time links, and one `发布母稿 V1.0` command. Do not expose YAML or raw model JSON.

- [ ] **Step 5: Run frontend tests and build**

Run: `npm run test:master-script && npm run build`

Expected: PASS; Vite resolves all three components.

- [ ] **Step 6: Commit**

```powershell
git add package.json src/lib/masterScript.ts src/lib/masterScript.test.ts src/lib/knowledge.ts src/lib/knowledge.test.ts src/lib/components/master src/lib/components/ImportVideoDialog.svelte src/lib/components/settings/KnowledgeVaultSettings.svelte src/page/AI.svelte
git commit -m "feat: add master script import and preview workflow"
```

### Task 8: Three-column mother comparison in recording analysis

**Files:**
- Modify: `src/lib/archiveAnalysis.ts`
- Modify: `src/lib/archiveAnalysis.test.ts`
- Create: `src/lib/components/analysis/MasterComparisonPanel.svelte`
- Modify: `src/page/ArchiveAnalysis.svelte`
- Modify: `scripts/check-analysis-reactivity.mjs`

**Interfaces:**
- Produces: `MasterComparisonView`, admission labels, dimension rows, and stale-response guards.
- Consumes: `compare_highlight_to_master`.

- [ ] **Step 1: Write failing analysis-state tests**

Test that score 84 displays `仅保留复盘`, score 85 plus passing gates displays `已进入候选辅稿`, blocked 100 displays the blocking reason, unmatched sections display `请选择母稿位置后重新评分`, and stale responses cannot replace a newly selected candidate.

- [ ] **Step 2: Run and verify failure**

Run: `node --loader ts-node/esm src/lib/archiveAnalysis.test.ts && npm run test:analysis-reactivity`

Expected: new assertions FAIL.

- [ ] **Step 3: Replace generic recommendation score with baseline comparison**

Keep initial highlight discovery score as a discovery signal only. After selecting/reviewing a clip, call the baseline comparison command and display the authoritative six-dimension mother score separately. Never relabel the discovery score as the mother score.

- [ ] **Step 4: Implement the beginner panel**

Right-column order: gate result, matched mother location, total and six dimensions, what is better, what needs confirmation, host speech versus current mother, candidate-queue status. Preserve left video and middle timestamp transcript behavior.

- [ ] **Step 5: Run analysis tests and production build**

Run: `node --loader ts-node/esm src/lib/archiveAnalysis.test.ts && npm run test:analysis-reactivity && npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/lib/archiveAnalysis.ts src/lib/archiveAnalysis.test.ts src/lib/components/analysis/MasterComparisonPanel.svelte src/page/ArchiveAnalysis.svelte scripts/check-analysis-reactivity.mjs
git commit -m "feat: compare analyzed clips with the master script"
```

### Task 9: Candidate review and immutable master version upgrade

**Files:**
- Create: `src/lib/components/master/SupportCandidateQueue.svelte`
- Create: `src/lib/components/master/MasterVersionDiff.svelte`
- Modify: `src/lib/masterScript.ts`
- Modify: `src/lib/masterScript.test.ts`
- Modify: `src-tauri/src/handlers/master_script.rs`
- Modify: `src-tauri/src/master_script/builder.rs`

**Interfaces:**
- Produces commands: `list_support_candidates`, `decide_support_candidate`, `preview_master_upgrade`, `publish_master_upgrade`.
- Produces V1.0 to V1.1 append/replace decision with immutable audit metadata.

- [ ] **Step 1: Write failing lifecycle tests**

Backend tests prove only `approved` candidates can upgrade, stale-base candidates require re-score, publish creates the next patch version, source V1.0 remains byte-identical, and a failed Markdown write leaves the candidate `approved` rather than `merged`.

Frontend tests prove `通过并加入新版本`, `保留候选`, `退回修改`, and `不采用` map to the correct statuses and that publish is disabled until the diff is explicitly confirmed.

- [ ] **Step 2: Run and verify failure**

Run: `cargo test master_upgrade_tests && npm run test:master-script`

Expected: FAIL because lifecycle commands and components are absent.

- [ ] **Step 3: Implement queue decisions and diff preview**

The reviewer sees source video/time, host speech, current mother text, proposed change, score evidence, conditions, and unresolved facts. `approved` does not write files; it only enables upgrade preview.

- [ ] **Step 4: Implement version publication**

Create a complete new master directory/version, verify all files, insert new database version/sections, then mark included candidates `merged`. Never edit V1.0 files in place.

- [ ] **Step 5: Run lifecycle tests**

Run: `cargo test master_upgrade_tests && npm run test:master-script && npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/lib/components/master src/lib/masterScript.ts src/lib/masterScript.test.ts src-tauri/src/handlers/master_script.rs src-tauri/src/master_script/builder.rs
git commit -m "feat: review support scripts and version the master"
```

### Task 10: End-to-end verification and project records

**Files:**
- Create: `src-tauri/tests/master_script_workflow.rs`
- Create: `docs/asr-enterprise-pipeline-v1.md`
- Create: `docs/master-script-operator-guide.md`
- Update: `C:/Users/10230/Documents/Codex/Workspace/01-Projects/Project-003-直播切片分析系统/05-测试与验收/2026-07-22-整场母稿基准分析验收记录.md`
- Update: `C:/Users/10230/Documents/Codex/Workspace/01-Projects/Project-003-直播切片分析系统/00-项目总览.md`

**Interfaces:**
- Verifies the complete boundary from source import to immutable V1.1.

- [ ] **Step 1: Add an end-to-end fixture test**

Use a short synthetic multi-product SRT and fake model responses. Assert: V1.0 has ordered sections, a gate-passing score 85 creates one pending candidate, score 84 creates none, approval plus publish creates V1.1, V1.0 hash remains unchanged, and every candidate links to source/time/master section.

- [ ] **Step 2: Run the complete automated suite**

Run:

```powershell
cargo test -p master-script
cargo test -p knowledge
cargo test -p knowledge-store
cargo test master_script
npm run test:master-script
npm run test:knowledge
npm run test:knowledge-settings
npm run test:transcript-review
npm run test:analysis-reactivity
npm run test:canonical-video-subtitles
npm run build
cargo check --features gui
```

Expected: every command exits 0. Record any pre-existing compiler warnings separately; do not report them as new failures.

- [ ] **Step 3: Run manual desktop acceptance**

Use the provided `C:/Users/10230/Downloads/Video/直播大屏·专业版_4.ts` only after making a safety copy or recording its SHA-256. Verify: import, resumable progress, parameter-card terminology, transcript review, multi-product section preview, V1.0 publish, later clip comparison, 84/85 boundary, queue review, and V1.1 publication. Confirm source video and previous master hashes are unchanged.

- [ ] **Step 4: Verify desktop layout**

Capture desktop screenshots at 1920x1080 and 1366x768. Confirm the three columns do not overlap, long product names wrap, score details remain readable, and buttons have stable dimensions. Verify loading, empty, blocked, error, and success states.

- [ ] **Step 5: Update operator and governance records**

Document the beginner workflow, score meaning, hard gates, recovery procedure, Obsidian locations, and the rule that candidate admission is not master publication. Record exact test commands and observed results in Project-003.

- [ ] **Step 6: Final commit**

```powershell
git add src-tauri/tests/master_script_workflow.rs docs/asr-enterprise-pipeline-v1.md docs/master-script-operator-guide.md
git commit -m "test: verify master script baseline workflow"
```

## Execution Checkpoints

- After Task 3: review domain, schema, and controlled-write security before processing real media.
- After Task 6: review a synthetic master and the exact 84/85 scoring boundary before frontend integration.
- After Task 9: review the complete workflow with fixture media before using the six-hour company recording.
- After Task 10: copy reviewed commits back to the user's dirty main worktree using blob-level conflict checks; never reset or overwrite unrelated user changes.
