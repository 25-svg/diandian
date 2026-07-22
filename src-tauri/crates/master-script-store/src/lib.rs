use master_script::{
    evaluate_admission, CandidateAdmission, HardGateResult, MasterSectionKind, ScoreBreakdown,
};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;

pub const MASTER_SCRIPT_MIGRATION_SQL: &str = r#"
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
  transcript_hash TEXT NOT NULL,
  host_text TEXT NOT NULL,
  comparison_json TEXT NOT NULL,
  gates_json TEXT NOT NULL,
  score_json TEXT NOT NULL,
  total_score INTEGER NOT NULL CHECK(total_score BETWEEN 0 AND 100),
  admission TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending_review','approved','held','returned','rejected','merged')),
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  decided_at TEXT,
  UNIQUE(master_script_id, master_section_id, source_key, source_start_ms, source_end_ms, transcript_hash)
);
"#;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid master script state: {0}")]
    InvalidMasterScriptState(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMasterSource {
    pub source_kind: String,
    pub source_key: String,
    pub media_path: String,
    pub media_hash: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterChunkInput {
    pub source_id: i64,
    pub chunk_index: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub status: String,
    pub input_hash: String,
    pub raw_srt: String,
    pub reviewed_srt: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMasterSection {
    pub section_key: String,
    pub position: i64,
    pub section_kind: MasterSectionKind,
    pub product_card_id: Option<String>,
    pub title: String,
    pub source_start_ms: i64,
    pub source_end_ms: i64,
    pub host_text: String,
    pub master_text: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMasterVersion {
    pub script_key: String,
    pub version: String,
    pub source_id: i64,
    pub title: String,
    pub index_relative_path: String,
    pub content_hash: String,
    pub sections: Vec<NewMasterSection>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSupportCandidate {
    pub candidate_key: String,
    pub master_script_id: i64,
    pub master_section_id: i64,
    pub source_key: String,
    pub source_start_ms: i64,
    pub source_end_ms: i64,
    pub transcript_hash: String,
    pub host_text: String,
    pub comparison_json: String,
    pub gates_json: String,
    pub score_json: String,
    pub admission: CandidateAdmission,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterSourceRow {
    pub id: i64,
    pub source_kind: String,
    pub source_key: String,
    pub media_path: String,
    pub media_hash: String,
    pub duration_ms: i64,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterChunkRow {
    pub id: i64,
    pub source_id: i64,
    pub chunk_index: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub status: String,
    pub input_hash: String,
    pub raw_srt: String,
    pub reviewed_srt: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterScriptRow {
    pub id: i64,
    pub script_key: String,
    pub version: String,
    pub source_id: i64,
    pub title: String,
    pub status: String,
    pub index_relative_path: String,
    pub content_hash: String,
    pub created_at: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterSectionRow {
    pub id: i64,
    pub master_script_id: i64,
    pub section_key: String,
    pub position: i64,
    pub section_kind: String,
    pub product_card_id: Option<String>,
    pub title: String,
    pub source_start_ms: i64,
    pub source_end_ms: i64,
    pub host_text: String,
    pub master_text: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SupportCandidateRow {
    pub id: i64,
    pub candidate_key: String,
    pub master_script_id: i64,
    pub master_section_id: i64,
    pub source_key: String,
    pub source_start_ms: i64,
    pub source_end_ms: i64,
    pub transcript_hash: String,
    pub host_text: String,
    pub comparison_json: String,
    pub gates_json: String,
    pub score_json: String,
    pub total_score: i64,
    pub admission: String,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
}

pub async fn create_master_source(
    pool: &SqlitePool,
    input: &NewMasterSource,
) -> Result<MasterSourceRow, StoreError> {
    let inserted = sqlx::query_as::<_, MasterSourceRow>(
        "INSERT INTO master_sources (source_kind, source_key, media_path, media_hash, duration_ms, status) \
         VALUES ($1, $2, $3, $4, $5, 'queued') \
         ON CONFLICT(source_key) DO NOTHING RETURNING *",
    )
    .bind(&input.source_kind)
    .bind(&input.source_key)
    .bind(&input.media_path)
    .bind(&input.media_hash)
    .bind(input.duration_ms)
    .fetch_optional(pool)
    .await?;
    if let Some(inserted) = inserted {
        return Ok(inserted);
    }

    let existing =
        sqlx::query_as::<_, MasterSourceRow>("SELECT * FROM master_sources WHERE source_key = $1")
            .bind(&input.source_key)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| {
                invalid_state(format!(
                    "source key conflict for {} could not be resolved",
                    input.source_key
                ))
            })?;
    if same_master_source_input(&existing, input) {
        Ok(existing)
    } else {
        Err(invalid_state(format!(
            "source key {} is already bound to different immutable input",
            input.source_key
        )))
    }
}

pub async fn get_master_source(
    pool: &SqlitePool,
    source_id: i64,
) -> Result<MasterSourceRow, StoreError> {
    sqlx::query_as::<_, MasterSourceRow>("SELECT * FROM master_sources WHERE id = $1")
        .bind(source_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| invalid_state(format!("master source {source_id} does not exist")))
}

pub async fn upsert_master_chunk(
    pool: &SqlitePool,
    input: &MasterChunkInput,
) -> Result<MasterChunkRow, StoreError> {
    Ok(sqlx::query_as::<_, MasterChunkRow>(
        "INSERT INTO master_transcript_chunks (source_id, chunk_index, start_ms, end_ms, status, input_hash, raw_srt, reviewed_srt, error) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT(source_id, chunk_index) DO UPDATE SET \
           start_ms=excluded.start_ms, end_ms=excluded.end_ms, status=excluded.status, \
           input_hash=excluded.input_hash, raw_srt=excluded.raw_srt, \
           reviewed_srt=excluded.reviewed_srt, error=excluded.error \
         RETURNING *",
    )
    .bind(input.source_id)
    .bind(input.chunk_index)
    .bind(input.start_ms)
    .bind(input.end_ms)
    .bind(&input.status)
    .bind(&input.input_hash)
    .bind(&input.raw_srt)
    .bind(&input.reviewed_srt)
    .bind(&input.error)
    .fetch_one(pool)
    .await?)
}

pub async fn list_master_chunks(
    pool: &SqlitePool,
    source_id: i64,
) -> Result<Vec<MasterChunkRow>, StoreError> {
    Ok(sqlx::query_as::<_, MasterChunkRow>(
        "SELECT * FROM master_transcript_chunks WHERE source_id = $1 ORDER BY chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?)
}

pub async fn publish_master_version(
    pool: &SqlitePool,
    input: &NewMasterVersion,
) -> Result<MasterScriptRow, StoreError> {
    let mut transaction = pool.begin().await?;
    let duplicate = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM master_scripts WHERE script_key = $1 AND version = $2",
    )
    .bind(&input.script_key)
    .bind(&input.version)
    .fetch_optional(&mut *transaction)
    .await?;
    if duplicate.is_some() {
        return Err(invalid_state(format!(
            "master script {} version {} is already published",
            input.script_key, input.version
        )));
    }

    let script = sqlx::query_as::<_, MasterScriptRow>(
        "INSERT INTO master_scripts (script_key, version, source_id, title, status, index_relative_path, content_hash, published_at) \
         VALUES ($1, $2, $3, $4, 'published', $5, $6, datetime('now')) RETURNING *",
    )
    .bind(&input.script_key)
    .bind(&input.version)
    .bind(input.source_id)
    .bind(&input.title)
    .bind(&input.index_relative_path)
    .bind(&input.content_hash)
    .fetch_one(&mut *transaction)
    .await?;

    for section in &input.sections {
        sqlx::query(
            "INSERT INTO master_sections (master_script_id, section_key, position, section_kind, product_card_id, title, source_start_ms, source_end_ms, host_text, master_text, metadata_json) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(script.id)
        .bind(&section.section_key)
        .bind(section.position)
        .bind(section.section_kind.as_str())
        .bind(&section.product_card_id)
        .bind(&section.title)
        .bind(section.source_start_ms)
        .bind(section.source_end_ms)
        .bind(&section.host_text)
        .bind(&section.master_text)
        .bind(&section.metadata_json)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(script)
}

pub async fn get_master_version(pool: &SqlitePool, id: i64) -> Result<MasterScriptRow, StoreError> {
    sqlx::query_as::<_, MasterScriptRow>("SELECT * FROM master_scripts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| invalid_state(format!("master script {id} does not exist")))
}

pub async fn get_latest_published_master(
    pool: &SqlitePool,
    script_key: &str,
) -> Result<MasterScriptRow, StoreError> {
    sqlx::query_as::<_, MasterScriptRow>(
        "SELECT * FROM master_scripts \
         WHERE script_key = $1 AND status = 'published' \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(script_key)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        invalid_state(format!(
            "published master script {script_key} does not exist"
        ))
    })
}

pub async fn list_master_sections(
    pool: &SqlitePool,
    master_script_id: i64,
) -> Result<Vec<MasterSectionRow>, StoreError> {
    Ok(sqlx::query_as::<_, MasterSectionRow>(
        "SELECT * FROM master_sections WHERE master_script_id = $1 ORDER BY position, id",
    )
    .bind(master_script_id)
    .fetch_all(pool)
    .await?)
}

pub async fn insert_support_candidate(
    pool: &SqlitePool,
    input: &NewSupportCandidate,
) -> Result<SupportCandidateRow, StoreError> {
    let gates: HardGateResult = serde_json::from_str(&input.gates_json)
        .map_err(|error| invalid_state(format!("invalid gates JSON: {error}")))?;
    let score: ScoreBreakdown = serde_json::from_str(&input.score_json)
        .map_err(|error| invalid_state(format!("invalid score JSON: {error}")))?;
    let computed_admission = evaluate_admission(&gates, &score);
    if input.admission != computed_admission {
        return Err(invalid_state(format!(
            "requested admission {:?} differs from computed admission {:?}",
            input.admission, computed_admission
        )));
    }
    if computed_admission != CandidateAdmission::CandidateQueue {
        return Err(invalid_state(format!(
            "support candidates require candidate_queue admission, got {:?}",
            computed_admission
        )));
    }

    let section_belongs_to_master = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM master_sections WHERE id = $1 AND master_script_id = $2",
    )
    .bind(input.master_section_id)
    .bind(input.master_script_id)
    .fetch_optional(pool)
    .await?;
    if section_belongs_to_master.is_none() {
        return Err(invalid_state(
            "candidate section does not belong to the requested master script",
        ));
    }

    let inserted = sqlx::query_as::<_, SupportCandidateRow>(
        "INSERT INTO support_script_candidates (candidate_key, master_script_id, master_section_id, source_key, source_start_ms, source_end_ms, transcript_hash, host_text, comparison_json, gates_json, score_json, total_score, admission, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'pending_review') \
         ON CONFLICT DO NOTHING RETURNING *",
    )
    .bind(&input.candidate_key)
    .bind(input.master_script_id)
    .bind(input.master_section_id)
    .bind(&input.source_key)
    .bind(input.source_start_ms)
    .bind(input.source_end_ms)
    .bind(&input.transcript_hash)
    .bind(&input.host_text)
    .bind(&input.comparison_json)
    .bind(&input.gates_json)
    .bind(&input.score_json)
    .bind(i64::from(score.total()))
    .bind(admission_as_str(computed_admission))
    .fetch_optional(pool)
    .await?;
    if let Some(inserted) = inserted {
        return Ok(inserted);
    }

    let by_key = sqlx::query_as::<_, SupportCandidateRow>(
        "SELECT * FROM support_script_candidates WHERE candidate_key = $1",
    )
    .bind(&input.candidate_key)
    .fetch_optional(pool)
    .await?;
    let by_identity = sqlx::query_as::<_, SupportCandidateRow>(
        "SELECT * FROM support_script_candidates \
         WHERE master_script_id = $1 AND master_section_id = $2 AND source_key = $3 \
           AND source_start_ms = $4 AND source_end_ms = $5 AND transcript_hash = $6",
    )
    .bind(input.master_script_id)
    .bind(input.master_section_id)
    .bind(&input.source_key)
    .bind(input.source_start_ms)
    .bind(input.source_end_ms)
    .bind(&input.transcript_hash)
    .fetch_optional(pool)
    .await?;

    let existing = match (by_key, by_identity) {
        (Some(by_key), Some(by_identity)) if by_key.id != by_identity.id => {
            return Err(invalid_state(
                "candidate key and immutable identity conflict with different rows",
            ));
        }
        (Some(existing), _) | (_, Some(existing)) => existing,
        (None, None) => {
            return Err(invalid_state(
                "candidate conflict could not be resolved to an existing row",
            ));
        }
    };
    if same_candidate_payload(&existing, input, score.total(), computed_admission) {
        Ok(existing)
    } else {
        Err(invalid_state(format!(
            "candidate identity {} has a different immutable payload",
            input.candidate_key
        )))
    }
}

pub async fn list_support_candidates(
    pool: &SqlitePool,
    status: Option<&str>,
) -> Result<Vec<SupportCandidateRow>, StoreError> {
    let candidates = match status {
        Some(status) => {
            sqlx::query_as::<_, SupportCandidateRow>(
                "SELECT * FROM support_script_candidates WHERE status = $1 ORDER BY id",
            )
            .bind(status)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, SupportCandidateRow>(
                "SELECT * FROM support_script_candidates ORDER BY id",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(candidates)
}

pub async fn decide_support_candidate(
    pool: &SqlitePool,
    id: i64,
    next_status: &str,
) -> Result<SupportCandidateRow, StoreError> {
    let required_status = if next_status == "merged" {
        "approved"
    } else if matches!(next_status, "approved" | "held" | "returned" | "rejected") {
        "pending_review"
    } else {
        return Err(invalid_state(format!(
            "support candidate cannot be directly changed to {next_status}"
        )));
    };

    sqlx::query_as::<_, SupportCandidateRow>(
        "UPDATE support_script_candidates SET status = $1, decided_at = datetime('now') \
         WHERE id = $2 AND status = $3 RETURNING *",
    )
    .bind(next_status)
    .bind(id)
    .bind(required_status)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| invalid_state(format!("candidate {id} is not {required_status}")))
}

fn same_candidate_payload(
    existing: &SupportCandidateRow,
    input: &NewSupportCandidate,
    total_score: u8,
    admission: CandidateAdmission,
) -> bool {
    existing.master_script_id == input.master_script_id
        && existing.master_section_id == input.master_section_id
        && existing.source_key == input.source_key
        && existing.source_start_ms == input.source_start_ms
        && existing.source_end_ms == input.source_end_ms
        && existing.transcript_hash == input.transcript_hash
        && existing.host_text == input.host_text
        && existing.comparison_json == input.comparison_json
        && existing.gates_json == input.gates_json
        && existing.score_json == input.score_json
        && existing.total_score == i64::from(total_score)
        && existing.admission == admission_as_str(admission)
}

fn same_master_source_input(existing: &MasterSourceRow, input: &NewMasterSource) -> bool {
    existing.source_kind == input.source_kind
        && existing.source_key == input.source_key
        && existing.media_path == input.media_path
        && existing.media_hash == input.media_hash
        && existing.duration_ms == input.duration_ms
}

fn admission_as_str(admission: CandidateAdmission) -> &'static str {
    match admission {
        CandidateAdmission::Blocked => "blocked",
        CandidateAdmission::AnalysisOnly => "analysis_only",
        CandidateAdmission::ReviewOnly => "review_only",
        CandidateAdmission::CandidateQueue => "candidate_queue",
    }
}

fn invalid_state(message: impl Into<String>) -> StoreError {
    StoreError::InvalidMasterScriptState(message.into())
}

#[cfg(test)]
mod master_script_store_tests {
    use super::*;
    use master_script::{CandidateAdmission, HardGateResult, MasterSectionKind, ScoreBreakdown};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Executor;
    use std::str::FromStr;
    use std::time::Duration;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(MASTER_SCRIPT_MIGRATION_SQL).await.unwrap();
        pool
    }

    async fn concurrent_test_pool() -> sqlx::SqlitePool {
        let database_name = format!("master-script-store-{}", uuid::Uuid::new_v4());
        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite:file:{database_name}?mode=memory&cache=shared"
        ))
        .unwrap()
        .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .unwrap();
        pool.execute(MASTER_SCRIPT_MIGRATION_SQL).await.unwrap();
        pool
    }

    fn source_input() -> NewMasterSource {
        NewMasterSource {
            source_kind: "video".into(),
            source_key: "video:7".into(),
            media_path: r"C:\fixtures\master.ts".into(),
            media_hash: "media-hash".into(),
            duration_ms: 3_600_000,
        }
    }

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

    async fn seeded_master() -> (sqlx::SqlitePool, MasterScriptRow, MasterSectionRow) {
        let pool = test_pool().await;
        let source = create_master_source(&pool, &source_input()).await.unwrap();
        let master = publish_master_version(
            &pool,
            &NewMasterVersion {
                script_key: "MS-001".into(),
                version: "1.0.0".into(),
                source_id: source.id,
                title: "master script".into(),
                index_relative_path: "10-master-scripts/MS-001/V1.0.md".into(),
                content_hash: "v1-hash".into(),
                sections: vec![NewMasterSection {
                    section_key: "product-1".into(),
                    position: 1,
                    section_kind: MasterSectionKind::Product,
                    product_card_id: Some("product-card-1".into()),
                    title: "Product".into(),
                    source_start_ms: 0,
                    source_end_ms: 30_000,
                    host_text: "host text".into(),
                    master_text: "master text".into(),
                    metadata_json: "{}".into(),
                }],
            },
        )
        .await
        .unwrap();
        let section = list_master_sections(&pool, master.id)
            .await
            .unwrap()
            .remove(0);
        (pool, master, section)
    }

    fn queued_candidate(
        master_script_id: i64,
        master_section_id: i64,
        score_total: u8,
    ) -> NewSupportCandidate {
        let score = if score_total == 84 {
            ScoreBreakdown::new(20, 20, 17, 13, 9, 5).unwrap()
        } else {
            ScoreBreakdown::new(20, 20, 18, 13, 9, 5).unwrap()
        };
        NewSupportCandidate {
            candidate_key: format!("candidate-{score_total}"),
            master_script_id,
            master_section_id,
            source_key: "video:7".into(),
            source_start_ms: 500,
            source_end_ms: 4_000,
            transcript_hash: "transcript-hash".into(),
            host_text: "supporting host text".into(),
            comparison_json: "{\"matches\":[\"product\"]}".into(),
            gates_json: serde_json::to_string(&passing_gates()).unwrap(),
            score_json: serde_json::to_string(&score).unwrap(),
            admission: CandidateAdmission::CandidateQueue,
        }
    }

    #[tokio::test]
    async fn chunk_checkpoint_is_idempotent_and_resumable() {
        let pool = test_pool().await;
        let source = create_master_source(&pool, &source_input()).await.unwrap();
        let mut chunk = MasterChunkInput {
            source_id: source.id,
            chunk_index: 0,
            start_ms: 0,
            end_ms: 600_000,
            status: "complete".into(),
            input_hash: "chunk-hash".into(),
            raw_srt: "raw-v1".into(),
            reviewed_srt: "reviewed-v1".into(),
            error: None,
        };
        let first = upsert_master_chunk(&pool, &chunk).await.unwrap();
        chunk.reviewed_srt = "reviewed-v2".into();
        let second = upsert_master_chunk(&pool, &chunk).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.reviewed_srt, "reviewed-v2");
        assert_eq!(list_master_chunks(&pool, source.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn master_source_is_idempotent_only_for_identical_immutable_input() {
        let pool = test_pool().await;
        let first = create_master_source(&pool, &source_input()).await.unwrap();
        let second = create_master_source(&pool, &source_input()).await.unwrap();
        assert_eq!(first.id, second.id);

        let mut conflicts = Vec::new();
        let mut source_kind = source_input();
        source_kind.source_kind = "archive".into();
        conflicts.push(source_kind);
        let mut media_path = source_input();
        media_path.media_path = r"C:\fixtures\other-master.ts".into();
        conflicts.push(media_path);
        let mut media_hash = source_input();
        media_hash.media_hash = "other-media".into();
        conflicts.push(media_hash);
        let mut duration = source_input();
        duration.duration_ms += 1;
        conflicts.push(duration);

        for conflict in conflicts {
            assert!(matches!(
                create_master_source(&pool, &conflict).await,
                Err(StoreError::InvalidMasterScriptState(_))
            ));
        }
    }

    #[tokio::test]
    async fn master_source_can_be_loaded_by_id_for_resume_validation() {
        let pool = test_pool().await;
        let created = create_master_source(&pool, &source_input()).await.unwrap();

        let loaded = get_master_source(&pool, created.id).await.unwrap();

        assert_eq!(loaded, created);
    }

    #[tokio::test]
    async fn concurrent_identical_master_sources_return_the_same_row() {
        let pool = concurrent_test_pool().await;
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(8));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let pool = pool.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                create_master_source(&pool, &source_input()).await
            }));
        }

        let mut ids = Vec::new();
        for task in tasks {
            ids.push(task.await.unwrap().unwrap().id);
        }
        assert!(ids.iter().all(|id| *id == ids[0]));
    }

    #[tokio::test]
    async fn published_master_versions_are_immutable() {
        let pool = test_pool().await;
        let source = create_master_source(&pool, &source_input()).await.unwrap();
        let v1 = publish_master_version(
            &pool,
            &NewMasterVersion {
                script_key: "MS-001".into(),
                version: "1.0.0".into(),
                source_id: source.id,
                title: "master script".into(),
                index_relative_path: "10-master-scripts/MS-001/V1.0.md".into(),
                content_hash: "v1-hash".into(),
                sections: vec![],
            },
        )
        .await
        .unwrap();
        publish_master_version(
            &pool,
            &NewMasterVersion {
                script_key: "MS-001".into(),
                version: "1.0.1".into(),
                source_id: source.id,
                title: "master script".into(),
                index_relative_path: "10-master-scripts/MS-001/V1.0.1.md".into(),
                content_hash: "v2-hash".into(),
                sections: vec![],
            },
        )
        .await
        .unwrap();
        let persisted_v1 = get_master_version(&pool, v1.id).await.unwrap();
        assert_eq!(persisted_v1.version, "1.0.0");
        assert_eq!(persisted_v1.content_hash, "v1-hash");
        assert!(publish_master_version(
            &pool,
            &NewMasterVersion {
                script_key: "MS-001".into(),
                version: "1.0.0".into(),
                source_id: source.id,
                title: "changed".into(),
                index_relative_path: "changed.md".into(),
                content_hash: "changed-hash".into(),
                sections: vec![],
            },
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn latest_published_master_is_resolved_by_script_key() {
        let (pool, master, _) = seeded_master().await;
        let source = get_master_source(&pool, master.source_id).await.unwrap();
        let latest = publish_master_version(
            &pool,
            &NewMasterVersion {
                script_key: master.script_key.clone(),
                version: "1.0.1".into(),
                source_id: source.id,
                title: "new master".into(),
                index_relative_path: "10-master-scripts/MS-001/V1.0.1.md".into(),
                content_hash: "v2-hash".into(),
                sections: vec![],
            },
        )
        .await
        .unwrap();

        let resolved = get_latest_published_master(&pool, &master.script_key)
            .await
            .unwrap();

        assert_eq!(resolved.id, latest.id);
        assert_eq!(resolved.version, "1.0.1");
    }

    #[tokio::test]
    async fn support_candidate_dedupes_identical_payload() {
        let (pool, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        let first = insert_support_candidate(&pool, &input).await.unwrap();
        let second = insert_support_candidate(&pool, &input).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            list_support_candidates(&pool, Some("pending_review"))
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn support_candidate_rejects_conflicting_duplicate_payload() {
        let (pool, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        insert_support_candidate(&pool, &input).await.unwrap();
        let mut conflict = input;
        conflict.host_text = "changed host text".into();
        assert!(matches!(
            insert_support_candidate(&pool, &conflict).await,
            Err(StoreError::InvalidMasterScriptState(_))
        ));
    }

    #[tokio::test]
    async fn same_candidate_identity_with_different_key_returns_original_row() {
        let (pool, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        let first = insert_support_candidate(&pool, &input).await.unwrap();
        let mut retry = input;
        retry.candidate_key = "different-caller-key".into();

        let second = insert_support_candidate(&pool, &retry).await.unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.candidate_key, second.candidate_key);
        assert_eq!(list_support_candidates(&pool, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn same_candidate_identity_with_conflicting_payload_is_rejected() {
        let (pool, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        insert_support_candidate(&pool, &input).await.unwrap();
        let mut conflict = input;
        conflict.candidate_key = "different-caller-key".into();
        conflict.host_text = "conflicting host text".into();

        let error = insert_support_candidate(&pool, &conflict)
            .await
            .unwrap_err();

        assert!(matches!(error, StoreError::InvalidMasterScriptState(_)));
        assert_eq!(list_support_candidates(&pool, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn different_transcript_hash_creates_a_distinct_candidate_identity() {
        let (pool, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        let first = insert_support_candidate(&pool, &input).await.unwrap();
        let mut distinct = input;
        distinct.candidate_key = "different-caller-key".into();
        distinct.transcript_hash = "different-transcript-hash".into();

        let second = insert_support_candidate(&pool, &distinct).await.unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(list_support_candidates(&pool, None).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn score_84_cannot_be_persisted_as_queued() {
        let (pool, master, section) = seeded_master().await;
        let error = insert_support_candidate(&pool, &queued_candidate(master.id, section.id, 84))
            .await
            .unwrap_err();
        assert!(matches!(error, StoreError::InvalidMasterScriptState(_)));
    }

    #[tokio::test]
    async fn review_only_candidate_is_not_inserted() {
        let (pool, master, section) = seeded_master().await;
        let mut input = queued_candidate(master.id, section.id, 84);
        input.admission = CandidateAdmission::ReviewOnly;

        let error = insert_support_candidate(&pool, &input).await.unwrap_err();

        assert!(matches!(error, StoreError::InvalidMasterScriptState(_)));
        assert!(list_support_candidates(&pool, Some("pending_review"))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn analysis_only_candidate_is_not_inserted() {
        let (pool, master, section) = seeded_master().await;
        let score = ScoreBreakdown::new(20, 20, 15, 10, 4, 0).unwrap();
        let mut input = queued_candidate(master.id, section.id, 85);
        input.candidate_key = "candidate-analysis-only".into();
        input.score_json = serde_json::to_string(&score).unwrap();
        input.admission = CandidateAdmission::AnalysisOnly;

        let error = insert_support_candidate(&pool, &input).await.unwrap_err();

        assert!(matches!(error, StoreError::InvalidMasterScriptState(_)));
        assert!(list_support_candidates(&pool, Some("pending_review"))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn blocked_candidate_is_not_inserted() {
        let (pool, master, section) = seeded_master().await;
        let mut gates = passing_gates();
        gates.facts_resolved = false;
        let mut input = queued_candidate(master.id, section.id, 85);
        input.candidate_key = "candidate-blocked".into();
        input.gates_json = serde_json::to_string(&gates).unwrap();
        input.admission = CandidateAdmission::Blocked;

        let error = insert_support_candidate(&pool, &input).await.unwrap_err();

        assert!(matches!(error, StoreError::InvalidMasterScriptState(_)));
        assert!(list_support_candidates(&pool, Some("pending_review"))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn candidate_decisions_allow_only_pending_review_transitions() {
        let (pool, master, section) = seeded_master().await;
        let candidate =
            insert_support_candidate(&pool, &queued_candidate(master.id, section.id, 85))
                .await
                .unwrap();
        let approved = decide_support_candidate(&pool, candidate.id, "approved")
            .await
            .unwrap();
        assert_eq!(approved.status, "approved");
        let merged = decide_support_candidate(&pool, candidate.id, "merged")
            .await
            .unwrap();
        assert_eq!(merged.status, "merged");
        assert!(matches!(
            decide_support_candidate(&pool, candidate.id, "rejected").await,
            Err(StoreError::InvalidMasterScriptState(_))
        ));
    }

    #[tokio::test]
    async fn exact_candidate_retry_after_decision_returns_the_decided_row() {
        let (pool, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        let inserted = insert_support_candidate(&pool, &input).await.unwrap();
        let approved = decide_support_candidate(&pool, inserted.id, "approved")
            .await
            .unwrap();

        let retried = insert_support_candidate(&pool, &input).await.unwrap();

        assert_eq!(retried.id, approved.id);
        assert_eq!(retried.status, "approved");
        assert_eq!(list_support_candidates(&pool, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn concurrent_identical_candidate_identities_return_the_same_row() {
        let pool = concurrent_test_pool().await;
        let source = create_master_source(&pool, &source_input()).await.unwrap();
        let master = publish_master_version(
            &pool,
            &NewMasterVersion {
                script_key: "MS-CONCURRENT".into(),
                version: "1.0.0".into(),
                source_id: source.id,
                title: "concurrent master".into(),
                index_relative_path: "10-master-scripts/MS-CONCURRENT/V1.0.md".into(),
                content_hash: "concurrent-master-hash".into(),
                sections: vec![NewMasterSection {
                    section_key: "product-1".into(),
                    position: 1,
                    section_kind: MasterSectionKind::Product,
                    product_card_id: Some("product-card-1".into()),
                    title: "Product".into(),
                    source_start_ms: 0,
                    source_end_ms: 30_000,
                    host_text: "host text".into(),
                    master_text: "master text".into(),
                    metadata_json: "{}".into(),
                }],
            },
        )
        .await
        .unwrap();
        let section = list_master_sections(&pool, master.id)
            .await
            .unwrap()
            .remove(0);
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(8));
        let mut tasks = Vec::new();
        for index in 0..8 {
            let pool = pool.clone();
            let barrier = barrier.clone();
            let mut input = queued_candidate(master.id, section.id, 85);
            input.candidate_key = format!("concurrent-candidate-{index}");
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                insert_support_candidate(&pool, &input).await
            }));
        }

        let mut ids = Vec::new();
        for task in tasks {
            ids.push(task.await.unwrap().unwrap().id);
        }
        assert!(ids.iter().all(|id| *id == ids[0]));
        assert_eq!(list_support_candidates(&pool, None).await.unwrap().len(), 1);
    }
}
