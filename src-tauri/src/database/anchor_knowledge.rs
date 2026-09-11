use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row, Sqlite, Transaction};

use super::{Database, DatabaseError};

pub const ANCHOR_KNOWLEDGE_MIGRATION_SQL: &str = r#"
CREATE TABLE anchor_knowledge_profiles (
  anchor_id TEXT PRIMARY KEY CHECK(trim(anchor_id) <> ''),
  display_name TEXT NOT NULL CHECK(trim(display_name) <> ''),
  normalized_name TEXT NOT NULL UNIQUE CHECK(trim(normalized_name) <> ''),
  vault_relative_root TEXT NOT NULL UNIQUE CHECK(trim(vault_relative_root) <> ''),
  is_active INTEGER NOT NULL DEFAULT 1 CHECK(is_active IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE anchor_knowledge_aliases (
  normalized_alias TEXT PRIMARY KEY CHECK(trim(normalized_alias) <> ''),
  alias TEXT NOT NULL CHECK(trim(alias) <> ''),
  anchor_id TEXT NOT NULL REFERENCES anchor_knowledge_profiles(anchor_id) ON DELETE CASCADE,
  created_at TEXT NOT NULL
);

CREATE TABLE anchor_knowledge_assets (
  asset_id TEXT PRIMARY KEY CHECK(trim(asset_id) <> ''),
  anchor_id TEXT NOT NULL REFERENCES anchor_knowledge_profiles(anchor_id) ON DELETE RESTRICT,
  asset_type TEXT NOT NULL CHECK(asset_type IN ('speech', 'deal_clip', 'analysis_advice')),
  title TEXT NOT NULL CHECK(trim(title) <> ''),
  body TEXT NOT NULL CHECK(trim(body) <> ''),
  product_id TEXT NOT NULL DEFAULT '',
  review_status TEXT NOT NULL DEFAULT 'candidate'
    CHECK(review_status IN ('candidate', 'pending_review', 'published', 'rejected')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version >= 1),
  supersedes_asset_id TEXT REFERENCES anchor_knowledge_assets(asset_id) ON DELETE RESTRICT,
  created_by_kind TEXT NOT NULL CHECK(created_by_kind IN ('model', 'human', 'system')),
  created_by_id TEXT NOT NULL CHECK(trim(created_by_id) <> ''),
  content_hash TEXT NOT NULL CHECK(trim(content_hash) <> ''),
  is_current INTEGER NOT NULL DEFAULT 0 CHECK(is_current IN (0, 1)),
  published_relative_path TEXT NOT NULL DEFAULT '',
  published_file_hash TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  submitted_at TEXT,
  reviewed_at TEXT,
  reviewed_by TEXT NOT NULL DEFAULT '',
  review_reason TEXT NOT NULL DEFAULT '',
  published_at TEXT,
  CHECK(review_status <> 'published' OR (
    reviewed_at IS NOT NULL AND trim(reviewed_by) <> '' AND published_at IS NOT NULL
    AND trim(published_relative_path) <> '' AND trim(published_file_hash) <> ''
  ))
);

CREATE TABLE anchor_knowledge_sources (
  source_id TEXT PRIMARY KEY CHECK(trim(source_id) <> ''),
  asset_id TEXT NOT NULL REFERENCES anchor_knowledge_assets(asset_id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL
    CHECK(source_kind IN ('video', 'transcript', 'product_fact', 'analysis', 'external')),
  source_locator TEXT NOT NULL CHECK(trim(source_locator) <> ''),
  video_id INTEGER,
  start_ms INTEGER,
  end_ms INTEGER,
  transcript_version TEXT NOT NULL DEFAULT '',
  transcript_hash TEXT NOT NULL DEFAULT '',
  product_fact_id TEXT NOT NULL DEFAULT '',
  product_fact_version TEXT NOT NULL DEFAULT '',
  analysis_version TEXT NOT NULL DEFAULT '',
  content_hash TEXT NOT NULL CHECK(trim(content_hash) <> ''),
  created_at TEXT NOT NULL,
  CHECK(
    (start_ms IS NULL AND end_ms IS NULL) OR
    (start_ms IS NOT NULL AND end_ms IS NOT NULL AND start_ms >= 0 AND end_ms > start_ms)
  ),
  CHECK(source_kind <> 'transcript' OR (
    trim(transcript_version) <> '' AND trim(transcript_hash) <> ''
  )),
  CHECK(source_kind <> 'product_fact' OR (
    trim(product_fact_id) <> '' AND trim(product_fact_version) <> ''
  ))
);

CREATE TABLE anchor_knowledge_review_events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  asset_id TEXT NOT NULL REFERENCES anchor_knowledge_assets(asset_id) ON DELETE CASCADE,
  previous_status TEXT NOT NULL,
  next_status TEXT NOT NULL,
  actor_kind TEXT NOT NULL CHECK(actor_kind IN ('model', 'human', 'system')),
  actor_id TEXT NOT NULL CHECK(trim(actor_id) <> ''),
  reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE TABLE anchor_knowledge_search_index (
  asset_id TEXT PRIMARY KEY REFERENCES anchor_knowledge_assets(asset_id) ON DELETE CASCADE,
  anchor_id TEXT NOT NULL REFERENCES anchor_knowledge_profiles(anchor_id) ON DELETE CASCADE,
  anchor_name TEXT NOT NULL,
  asset_type TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  product_id TEXT NOT NULL DEFAULT '',
  version INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  published_at TEXT NOT NULL
);

CREATE INDEX idx_anchor_knowledge_alias_anchor
  ON anchor_knowledge_aliases(anchor_id, normalized_alias);
CREATE INDEX idx_anchor_knowledge_assets_scope
  ON anchor_knowledge_assets(anchor_id, review_status, is_current, updated_at DESC);
CREATE INDEX idx_anchor_knowledge_assets_product
  ON anchor_knowledge_assets(anchor_id, product_id, review_status);
CREATE INDEX idx_anchor_knowledge_sources_asset
  ON anchor_knowledge_sources(asset_id, source_kind);
CREATE INDEX idx_anchor_knowledge_search_scope
  ON anchor_knowledge_search_index(anchor_id, asset_type, published_at DESC);
"#;

pub const ANCHOR_LEARNING_CASE_MIGRATION_SQL: &str = r#"
CREATE TABLE anchor_learning_cases (
  case_id TEXT PRIMARY KEY CHECK(trim(case_id) <> ''),
  asset_id TEXT NOT NULL UNIQUE REFERENCES anchor_knowledge_assets(asset_id) ON DELETE CASCADE,
  stage TEXT NOT NULL CHECK(stage IN ('opening','traffic','needs','explanation','objection','conversion','after_sales','retention')),
  skill TEXT NOT NULL CHECK(trim(skill) <> ''),
  product_category TEXT NOT NULL DEFAULT '',
  difficulty TEXT NOT NULL CHECK(difficulty IN ('beginner','intermediate','advanced')),
  evidence_level TEXT NOT NULL CHECK(evidence_level IN ('A','B','C','D')),
  evidence_summary TEXT NOT NULL DEFAULT '',
  operator_commentary TEXT NOT NULL DEFAULT '',
  ai_analysis TEXT NOT NULL DEFAULT '',
  scene_context TEXT NOT NULL CHECK(trim(scene_context) <> ''),
  audience_trigger TEXT NOT NULL DEFAULT '',
  training_goal TEXT NOT NULL CHECK(trim(training_goal) <> ''),
  applicable_scope TEXT NOT NULL CHECK(trim(applicable_scope) <> ''),
  expiry_conditions TEXT NOT NULL CHECK(trim(expiry_conditions) <> ''),
  review_due_at TEXT NOT NULL CHECK(trim(review_due_at) <> ''),
  expression_reason TEXT NOT NULL DEFAULT '',
  logic_reason TEXT NOT NULL DEFAULT '',
  trust_reason TEXT NOT NULL DEFAULT '',
  action_reason TEXT NOT NULL DEFAULT '',
  reusable_outline TEXT NOT NULL DEFAULT '',
  forbidden_copy TEXT NOT NULL DEFAULT '',
  trainee_reference TEXT NOT NULL DEFAULT '',
  fact_slots_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(fact_slots_json)),
  internal_use_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(internal_use_confirmed IN (0,1)),
  retired_at TEXT,
  retired_by TEXT NOT NULL DEFAULT '',
  retire_reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TRIGGER trg_anchor_learning_cases_asset_speech_insert
BEFORE INSERT ON anchor_learning_cases
WHEN NOT EXISTS (
  SELECT 1 FROM anchor_knowledge_assets
  WHERE asset_id = NEW.asset_id AND asset_type = 'speech'
)
BEGIN
  SELECT RAISE(ABORT, 'learning case asset must be speech');
END;

CREATE TRIGGER trg_anchor_learning_cases_asset_speech_update
BEFORE UPDATE OF asset_id ON anchor_learning_cases
WHEN NOT EXISTS (
  SELECT 1 FROM anchor_knowledge_assets
  WHERE asset_id = NEW.asset_id AND asset_type = 'speech'
)
BEGIN
  SELECT RAISE(ABORT, 'learning case asset must be speech');
END;

CREATE TABLE anchor_learning_case_lines (
  line_id TEXT PRIMARY KEY,
  case_id TEXT NOT NULL REFERENCES anchor_learning_cases(case_id) ON DELETE CASCADE,
  line_order INTEGER NOT NULL CHECK(line_order >= 0),
  start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
  end_ms INTEGER NOT NULL CHECK(end_ms > start_ms),
  original_text TEXT NOT NULL CHECK(trim(original_text) <> ''),
  function_text TEXT NOT NULL DEFAULT '',
  timing_reason TEXT NOT NULL DEFAULT '',
  technique TEXT NOT NULL DEFAULT '',
  trust_mechanism TEXT NOT NULL DEFAULT '',
  action_cue TEXT NOT NULL DEFAULT '',
  risk_note TEXT NOT NULL DEFAULT '',
  reusable_pattern TEXT NOT NULL DEFAULT '',
  UNIQUE(case_id, line_order)
);

CREATE TABLE anchor_learning_case_tags (
  case_id TEXT NOT NULL REFERENCES anchor_learning_cases(case_id) ON DELETE CASCADE,
  tag_kind TEXT NOT NULL CHECK(tag_kind IN ('stage','skill','product','audience','risk')),
  tag_value TEXT NOT NULL CHECK(trim(tag_value) <> ''),
  PRIMARY KEY(case_id, tag_kind, tag_value)
);

CREATE TABLE anchor_learning_case_attempts (
  attempt_id TEXT PRIMARY KEY,
  case_id TEXT NOT NULL REFERENCES anchor_learning_cases(case_id) ON DELETE RESTRICT,
  trainee_anchor_id TEXT NOT NULL REFERENCES anchor_knowledge_profiles(anchor_id) ON DELETE RESTRICT,
  practice_mode TEXT NOT NULL CHECK(practice_mode IN ('shadow','recall','scenario','risk_spotting')),
  answer_text TEXT NOT NULL DEFAULT '',
  evaluation_status TEXT NOT NULL CHECK(evaluation_status IN ('scored','needs_review')),
  scores_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(scores_json)),
  feedback_text TEXT NOT NULL DEFAULT '',
  case_snapshot_json TEXT NOT NULL CHECK(json_valid(case_snapshot_json)),
  created_at TEXT NOT NULL
);

CREATE INDEX idx_learning_cases_public
  ON anchor_learning_cases(evidence_level, retired_at, updated_at DESC);
CREATE INDEX idx_learning_case_lines_order
  ON anchor_learning_case_lines(case_id, line_order);
CREATE INDEX idx_learning_case_attempts_trainee
  ON anchor_learning_case_attempts(trainee_anchor_id, created_at DESC);
"#;

const ASSET_TYPES: [&str; 3] = ["speech", "deal_clip", "analysis_advice"];
const SOURCE_KINDS: [&str; 5] = [
    "video",
    "transcript",
    "product_fact",
    "analysis",
    "external",
];
const PRIVATE_STATUSES: [&str; 4] = ["candidate", "pending_review", "published", "rejected"];

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AnchorKnowledgeProfile {
    pub anchor_id: String,
    pub display_name: String,
    pub vault_relative_root: String,
    pub is_active: bool,
    pub candidate_count: i64,
    pub pending_review_count: i64,
    pub published_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnchorKnowledgeProfileRequest {
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorKnowledgeSourceInput {
    pub source_kind: String,
    pub source_locator: String,
    pub video_id: Option<i64>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    #[serde(default)]
    pub transcript_version: String,
    #[serde(default)]
    pub transcript_hash: String,
    #[serde(default)]
    pub product_fact_id: String,
    #[serde(default)]
    pub product_fact_version: String,
    #[serde(default)]
    pub analysis_version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorKnowledgeSource {
    pub source_id: String,
    pub source_kind: String,
    pub source_locator: String,
    pub video_id: Option<i64>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub transcript_version: String,
    pub transcript_hash: String,
    pub product_fact_id: String,
    pub product_fact_version: String,
    pub analysis_version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnchorKnowledgeCandidateRequest {
    pub anchor_id: String,
    pub asset_type: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub product_id: String,
    pub created_by_kind: String,
    pub created_by_id: String,
    pub supersedes_asset_id: Option<String>,
    pub sources: Vec<AnchorKnowledgeSourceInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnchorLearningCaseRequest {
    pub anchor_id: String,
    pub title: String,
    pub body: String,
    pub product_id: String,
    pub stage: String,
    pub skill: String,
    pub product_category: String,
    pub difficulty: String,
    pub evidence_level: String,
    pub evidence_summary: String,
    pub operator_commentary: String,
    pub ai_analysis: String,
    pub scene_context: String,
    pub audience_trigger: String,
    pub training_goal: String,
    pub applicable_scope: String,
    pub expiry_conditions: String,
    pub review_due_at: String,
    pub expression_reason: String,
    pub logic_reason: String,
    pub trust_reason: String,
    pub action_reason: String,
    pub reusable_outline: String,
    pub forbidden_copy: String,
    pub trainee_reference: String,
    pub fact_slots: Vec<String>,
    pub internal_use_confirmed: bool,
    pub created_by_kind: String,
    pub created_by_id: String,
    pub supersedes_asset_id: Option<String>,
    pub sources: Vec<AnchorKnowledgeSourceInput>,
    pub lines: Vec<LearningCaseLineInput>,
    pub tags: Vec<LearningCaseTagInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningCaseLineInput {
    pub start_ms: i64,
    pub end_ms: i64,
    pub original_text: String,
    pub function_text: String,
    pub timing_reason: String,
    pub technique: String,
    pub trust_mechanism: String,
    pub action_cue: String,
    pub risk_note: String,
    pub reusable_pattern: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningCaseTagInput {
    pub tag_kind: String,
    pub tag_value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchPublicLearningCasesRequest {
    #[serde(default)]
    pub query: String,
    pub anchor_id: Option<String>,
    pub stage: Option<String>,
    pub skill: Option<String>,
    pub difficulty: Option<String>,
    #[serde(default)]
    pub evidence_levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCaseRow {
    pub case_id: String,
    pub asset_id: String,
    pub stage: String,
    pub skill: String,
    pub product_category: String,
    pub difficulty: String,
    pub evidence_level: String,
    pub evidence_summary: String,
    pub operator_commentary: String,
    pub ai_analysis: String,
    pub scene_context: String,
    pub audience_trigger: String,
    pub training_goal: String,
    pub applicable_scope: String,
    pub expiry_conditions: String,
    pub review_due_at: String,
    pub expression_reason: String,
    pub logic_reason: String,
    pub trust_reason: String,
    pub action_reason: String,
    pub reusable_outline: String,
    pub forbidden_copy: String,
    pub trainee_reference: String,
    pub fact_slots_json: String,
    pub internal_use_confirmed: bool,
    pub retired_at: Option<String>,
    pub retired_by: String,
    pub retire_reason: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCaseLineRow {
    pub line_id: String,
    pub case_id: String,
    pub line_order: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub original_text: String,
    pub function_text: String,
    pub timing_reason: String,
    pub technique: String,
    pub trust_mechanism: String,
    pub action_cue: String,
    pub risk_note: String,
    pub reusable_pattern: String,
}

#[derive(Debug, Clone, Serialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCaseAttemptRow {
    pub attempt_id: String,
    pub case_id: String,
    pub trainee_anchor_id: String,
    pub practice_mode: String,
    pub answer_text: String,
    pub evaluation_status: String,
    pub scores_json: String,
    pub feedback_text: String,
    pub case_snapshot_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitAnchorKnowledgeAssetRequest {
    pub anchor_id: String,
    pub asset_id: String,
    pub submitted_by: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewAnchorKnowledgeAssetRequest {
    pub anchor_id: String,
    pub asset_id: String,
    pub decision: String,
    pub reviewer_id: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchAnchorKnowledgeRequest {
    pub requester_anchor_id: String,
    pub scope: String,
    pub owner_anchor_id: Option<String>,
    #[serde(default)]
    pub query: String,
    pub asset_type: Option<String>,
    pub review_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAnchorKnowledgeAssetRequest {
    pub requester_anchor_id: String,
    pub scope: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorKnowledgeAssetSummary {
    pub asset_id: String,
    pub anchor_id: String,
    pub anchor_name: String,
    pub asset_type: String,
    pub title: String,
    pub body: String,
    pub product_id: String,
    pub review_status: String,
    pub version: i64,
    pub supersedes_asset_id: Option<String>,
    pub content_hash: String,
    pub is_current: bool,
    pub published_relative_path: String,
    pub published_file_hash: String,
    pub created_at: String,
    pub updated_at: String,
    pub reviewed_at: Option<String>,
    pub reviewed_by: String,
    pub review_reason: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorKnowledgeAssetDetail {
    #[serde(flatten)]
    pub asset: AnchorKnowledgeAssetSummary,
    pub sources: Vec<AnchorKnowledgeSource>,
}

#[derive(Debug, Clone)]
pub struct PublishedAnchorKnowledgeAssetInput {
    pub asset_id: String,
    pub anchor_id: String,
    pub asset_type: String,
    pub title: String,
    pub body: String,
    pub product_id: String,
    pub version: i64,
    pub supersedes_asset_id: Option<String>,
    pub reviewer_id: String,
    pub review_reason: String,
    pub published_relative_path: String,
    pub published_file_hash: String,
    pub sources: Vec<AnchorKnowledgeSourceInput>,
}

fn invalid(message: impl Into<String>) -> DatabaseError {
    DatabaseError::InvalidAnchorKnowledgeState(message.into())
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn normalized_label(value: &str) -> Result<String, DatabaseError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("主播名称或别名不能为空"));
    }
    if value.chars().count() > 80 {
        return Err(invalid("主播名称或别名不能超过 80 个字符"));
    }
    Ok(value.to_lowercase())
}

fn validate_asset_type(value: &str) -> Result<&str, DatabaseError> {
    let value = value.trim();
    if !ASSET_TYPES.contains(&value) {
        return Err(invalid(
            "知识资产类型仅允许 speech、deal_clip、analysis_advice；产品事实只能引用，不能写入主播库",
        ));
    }
    Ok(value)
}

fn validate_actor_kind(value: &str) -> Result<&str, DatabaseError> {
    let value = value.trim();
    if !matches!(value, "model" | "human" | "system") {
        return Err(invalid("创建者类型必须是 model、human 或 system"));
    }
    Ok(value)
}

fn validate_sources(sources: &[AnchorKnowledgeSourceInput]) -> Result<(), DatabaseError> {
    if sources.is_empty() {
        return Err(invalid("知识资产必须至少保留一条来源引用"));
    }
    for source in sources {
        let kind = source.source_kind.trim();
        if !SOURCE_KINDS.contains(&kind) {
            return Err(invalid("来源类型不受支持"));
        }
        if source.source_locator.trim().is_empty() || source.content_hash.trim().is_empty() {
            return Err(invalid("来源位置和内容哈希不能为空"));
        }
        match (source.start_ms, source.end_ms) {
            (None, None) => {}
            (Some(start), Some(end)) if start >= 0 && end > start => {}
            _ => return Err(invalid("来源时间范围必须同时填写，且结束时间大于开始时间")),
        }
        if kind == "transcript"
            && (source.transcript_version.trim().is_empty()
                || source.transcript_hash.trim().is_empty())
        {
            return Err(invalid("逐字稿来源必须包含版本和哈希"));
        }
        if kind == "product_fact"
            && (source.product_fact_id.trim().is_empty()
                || source.product_fact_version.trim().is_empty())
        {
            return Err(invalid("商品事实引用必须包含事实编号和版本"));
        }
    }
    Ok(())
}

fn asset_content_hash(
    anchor_id: &str,
    asset_type: &str,
    title: &str,
    body: &str,
    product_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    for value in [anchor_id, asset_type, title, body, product_id] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

async fn insert_sources(
    transaction: &mut Transaction<'_, Sqlite>,
    asset_id: &str,
    sources: &[AnchorKnowledgeSourceInput],
    created_at: &str,
) -> Result<(), DatabaseError> {
    for source in sources {
        let source_id = format!("source-{}", uuid::Uuid::new_v4().simple());
        sqlx::query(
            r#"INSERT INTO anchor_knowledge_sources (
                source_id, asset_id, source_kind, source_locator, video_id, start_ms, end_ms,
                transcript_version, transcript_hash, product_fact_id, product_fact_version,
                analysis_version, content_hash, created_at
              ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(source_id)
        .bind(asset_id)
        .bind(source.source_kind.trim())
        .bind(source.source_locator.trim())
        .bind(source.video_id)
        .bind(source.start_ms)
        .bind(source.end_ms)
        .bind(source.transcript_version.trim())
        .bind(source.transcript_hash.trim())
        .bind(source.product_fact_id.trim())
        .bind(source.product_fact_version.trim())
        .bind(source.analysis_version.trim())
        .bind(source.content_hash.trim())
        .bind(created_at)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn insert_review_event(
    transaction: &mut Transaction<'_, Sqlite>,
    asset_id: &str,
    previous_status: &str,
    next_status: &str,
    actor_kind: &str,
    actor_id: &str,
    reason: &str,
    created_at: &str,
) -> Result<(), DatabaseError> {
    sqlx::query(
        r#"INSERT INTO anchor_knowledge_review_events (
            asset_id, previous_status, next_status, actor_kind, actor_id, reason, created_at
          ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(asset_id)
    .bind(previous_status)
    .bind(next_status)
    .bind(actor_kind)
    .bind(actor_id)
    .bind(reason)
    .bind(created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

impl Database {
    pub async fn create_anchor_knowledge_profile(
        &self,
        request: CreateAnchorKnowledgeProfileRequest,
    ) -> Result<AnchorKnowledgeProfile, DatabaseError> {
        let display_name = request.display_name.trim();
        let normalized_name = normalized_label(display_name)?;
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM anchor_knowledge_profiles WHERE normalized_name = ?",
        )
        .bind(&normalized_name)
        .fetch_one(&mut *transaction)
        .await?;
        if exists > 0 {
            return Err(invalid("该主播知识库已经存在"));
        }
        let anchor_id = format!("anchor-{}", uuid::Uuid::new_v4().simple());
        let vault_relative_root = format!("主播知识库/{anchor_id}");
        let timestamp = now();
        sqlx::query(
            r#"INSERT INTO anchor_knowledge_profiles (
                anchor_id, display_name, normalized_name, vault_relative_root,
                is_active, created_at, updated_at
              ) VALUES (?, ?, ?, ?, 1, ?, ?)"#,
        )
        .bind(&anchor_id)
        .bind(display_name)
        .bind(&normalized_name)
        .bind(&vault_relative_root)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;

        let mut aliases = request.aliases;
        aliases.push(display_name.to_string());
        aliases.sort_by_key(|value| value.trim().to_lowercase());
        aliases.dedup_by_key(|value| value.trim().to_lowercase());
        for alias in aliases {
            let alias = alias.trim();
            let normalized_alias = normalized_label(alias)?;
            let result = sqlx::query(
                r#"INSERT INTO anchor_knowledge_aliases (
                    normalized_alias, alias, anchor_id, created_at
                  ) VALUES (?, ?, ?, ?)"#,
            )
            .bind(&normalized_alias)
            .bind(alias)
            .bind(&anchor_id)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await;
            if let Err(error) = result {
                if matches!(&error, sqlx::Error::Database(database) if database.is_unique_violation())
                {
                    return Err(invalid(format!("主播别名“{alias}”已属于其他主播")));
                }
                return Err(error.into());
            }
        }
        transaction.commit().await?;
        self.get_anchor_knowledge_profile(&anchor_id).await
    }

    pub async fn ensure_anchor_knowledge_profile(
        &self,
        display_name: &str,
    ) -> Result<AnchorKnowledgeProfile, DatabaseError> {
        let normalized = normalized_label(display_name)?;
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let existing = sqlx::query_scalar::<_, String>(
            r#"SELECT anchor_id FROM anchor_knowledge_aliases WHERE normalized_alias = ?
               UNION ALL
               SELECT anchor_id FROM anchor_knowledge_profiles WHERE normalized_name = ?
               LIMIT 1"#,
        )
        .bind(&normalized)
        .bind(&normalized)
        .fetch_optional(&pool)
        .await?;
        if let Some(anchor_id) = existing {
            return self.get_anchor_knowledge_profile(&anchor_id).await;
        }
        self.create_anchor_knowledge_profile(CreateAnchorKnowledgeProfileRequest {
            display_name: display_name.trim().to_string(),
            aliases: Vec::new(),
        })
        .await
    }

    pub async fn list_anchor_knowledge_profiles(
        &self,
    ) -> Result<Vec<AnchorKnowledgeProfile>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        Ok(sqlx::query_as::<_, AnchorKnowledgeProfile>(
            r#"SELECT
                profiles.anchor_id,
                profiles.display_name,
                profiles.vault_relative_root,
                profiles.is_active,
                SUM(CASE WHEN assets.review_status = 'candidate' THEN 1 ELSE 0 END) AS candidate_count,
                SUM(CASE WHEN assets.review_status = 'pending_review' THEN 1 ELSE 0 END) AS pending_review_count,
                SUM(CASE WHEN assets.review_status = 'published' AND assets.is_current = 1 THEN 1 ELSE 0 END) AS published_count
              FROM anchor_knowledge_profiles profiles
              LEFT JOIN anchor_knowledge_assets assets ON assets.anchor_id = profiles.anchor_id
              WHERE profiles.is_active = 1
              GROUP BY profiles.anchor_id
              ORDER BY profiles.display_name COLLATE NOCASE"#,
        )
        .fetch_all(&pool)
        .await?)
    }

    pub async fn get_anchor_knowledge_profile(
        &self,
        anchor_id: &str,
    ) -> Result<AnchorKnowledgeProfile, DatabaseError> {
        let anchor_id = anchor_id.trim();
        if anchor_id.is_empty() {
            return Err(invalid("anchor_id 不能为空"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query_as::<_, AnchorKnowledgeProfile>(
            r#"SELECT
                profiles.anchor_id,
                profiles.display_name,
                profiles.vault_relative_root,
                profiles.is_active,
                SUM(CASE WHEN assets.review_status = 'candidate' THEN 1 ELSE 0 END) AS candidate_count,
                SUM(CASE WHEN assets.review_status = 'pending_review' THEN 1 ELSE 0 END) AS pending_review_count,
                SUM(CASE WHEN assets.review_status = 'published' AND assets.is_current = 1 THEN 1 ELSE 0 END) AS published_count
              FROM anchor_knowledge_profiles profiles
              LEFT JOIN anchor_knowledge_assets assets ON assets.anchor_id = profiles.anchor_id
              WHERE profiles.anchor_id = ? AND profiles.is_active = 1
              GROUP BY profiles.anchor_id"#,
        )
        .bind(anchor_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| invalid("主播知识库不存在或已停用"))
    }

    pub async fn create_anchor_knowledge_candidate(
        &self,
        request: CreateAnchorKnowledgeCandidateRequest,
    ) -> Result<AnchorKnowledgeAssetDetail, DatabaseError> {
        let anchor_id = request.anchor_id.trim();
        if anchor_id.is_empty() {
            return Err(invalid("anchor_id 不能为空"));
        }
        let asset_type = validate_asset_type(&request.asset_type)?;
        let actor_kind = validate_actor_kind(&request.created_by_kind)?;
        let actor_id = request.created_by_id.trim();
        let title = request.title.trim();
        let body = request.body.trim();
        if actor_id.is_empty() || title.is_empty() || body.is_empty() {
            return Err(invalid("创建者、标题和正文不能为空"));
        }
        validate_sources(&request.sources)?;
        self.get_anchor_knowledge_profile(anchor_id).await?;

        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let version = if let Some(supersedes) = request
            .supersedes_asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let row = sqlx::query(
                r#"SELECT anchor_id, asset_type, version, review_status
                   FROM anchor_knowledge_assets WHERE asset_id = ?"#,
            )
            .bind(supersedes)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| invalid("被替代的知识资产不存在"))?;
            if row.get::<String, _>("anchor_id") != anchor_id
                || row.get::<String, _>("asset_type") != asset_type
                || row.get::<String, _>("review_status") != "published"
            {
                return Err(invalid("只能为同一主播、同一类型的已发布资产创建新版本"));
            }
            row.get::<i64, _>("version") + 1
        } else {
            1
        };
        let asset_id = format!("asset-{}", uuid::Uuid::new_v4().simple());
        let product_id = request.product_id.trim();
        let content_hash = asset_content_hash(anchor_id, asset_type, title, body, product_id);
        let timestamp = now();
        sqlx::query(
            r#"INSERT INTO anchor_knowledge_assets (
                asset_id, anchor_id, asset_type, title, body, product_id, review_status,
                version, supersedes_asset_id, created_by_kind, created_by_id, content_hash,
                is_current, created_at, updated_at
              ) VALUES (?, ?, ?, ?, ?, ?, 'candidate', ?, ?, ?, ?, ?, 0, ?, ?)"#,
        )
        .bind(&asset_id)
        .bind(anchor_id)
        .bind(asset_type)
        .bind(title)
        .bind(body)
        .bind(product_id)
        .bind(version)
        .bind(request.supersedes_asset_id.as_deref().map(str::trim))
        .bind(actor_kind)
        .bind(actor_id)
        .bind(&content_hash)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;
        insert_sources(&mut transaction, &asset_id, &request.sources, &timestamp).await?;
        insert_review_event(
            &mut transaction,
            &asset_id,
            "",
            "candidate",
            actor_kind,
            actor_id,
            "创建知识候选",
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        self.get_anchor_knowledge_asset_unscoped(&asset_id).await
    }

    pub async fn create_anchor_learning_case(
        &self,
        request: CreateAnchorLearningCaseRequest,
    ) -> Result<LearningCaseRow, DatabaseError> {
        let anchor_id = request.anchor_id.trim();
        let actor_kind = validate_actor_kind(&request.created_by_kind)?;
        let actor_id = request.created_by_id.trim();
        let title = request.title.trim();
        let body = request.body.trim();
        if anchor_id.is_empty() || actor_id.is_empty() || title.is_empty() || body.is_empty() {
            return Err(invalid("主播、创建者、标题和正文不能为空"));
        }
        validate_sources(&request.sources)?;
        self.get_anchor_knowledge_profile(anchor_id).await?;

        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let supersedes_asset_id = request
            .supersedes_asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let version = if let Some(supersedes) = supersedes_asset_id {
            let row = sqlx::query(
                r#"SELECT assets.anchor_id, assets.asset_type, assets.version,
                          assets.review_status, assets.is_current
                   FROM anchor_knowledge_assets assets
                   JOIN anchor_learning_cases cases ON cases.asset_id = assets.asset_id
                   WHERE assets.asset_id = ?"#,
            )
            .bind(supersedes)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| invalid("被替代的学习案例不存在"))?;
            if row.get::<String, _>("anchor_id") != anchor_id
                || row.get::<String, _>("asset_type") != "speech"
                || row.get::<String, _>("review_status") != "published"
                || row.get::<i64, _>("is_current") != 1
            {
                return Err(invalid("只能为同一主播当前已发布的学习案例创建新版本"));
            }
            row.get::<i64, _>("version") + 1
        } else {
            1
        };

        let timestamp = now();
        let asset_id = format!("asset-{}", uuid::Uuid::new_v4().simple());
        let case_id = format!("case-{}", uuid::Uuid::new_v4().simple());
        let product_id = request.product_id.trim();
        let content_hash = asset_content_hash(anchor_id, "speech", title, body, product_id);
        let fact_slots_json = serde_json::to_string(&request.fact_slots)
            .map_err(|_| invalid("事实槽位无法序列化"))?;

        sqlx::query(
            r#"INSERT INTO anchor_knowledge_assets (
                asset_id, anchor_id, asset_type, title, body, product_id, review_status,
                version, supersedes_asset_id, created_by_kind, created_by_id, content_hash,
                is_current, created_at, updated_at
              ) VALUES (?, ?, 'speech', ?, ?, ?, 'candidate', ?, ?, ?, ?, ?, 0, ?, ?)"#,
        )
        .bind(&asset_id)
        .bind(anchor_id)
        .bind(title)
        .bind(body)
        .bind(product_id)
        .bind(version)
        .bind(supersedes_asset_id)
        .bind(actor_kind)
        .bind(actor_id)
        .bind(&content_hash)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;
        insert_sources(&mut transaction, &asset_id, &request.sources, &timestamp).await?;
        sqlx::query(
            r#"INSERT INTO anchor_learning_cases (
                case_id, asset_id, stage, skill, product_category, difficulty, evidence_level,
                evidence_summary, operator_commentary, ai_analysis, scene_context, audience_trigger,
                training_goal, applicable_scope, expiry_conditions, review_due_at,
                expression_reason, logic_reason, trust_reason, action_reason, reusable_outline,
                forbidden_copy, trainee_reference, fact_slots_json, internal_use_confirmed,
                created_at, updated_at
              ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&case_id)
        .bind(&asset_id)
        .bind(request.stage.trim())
        .bind(request.skill.trim())
        .bind(request.product_category.trim())
        .bind(request.difficulty.trim())
        .bind(request.evidence_level.trim())
        .bind(request.evidence_summary.trim())
        .bind(request.operator_commentary.trim())
        .bind(request.ai_analysis.trim())
        .bind(request.scene_context.trim())
        .bind(request.audience_trigger.trim())
        .bind(request.training_goal.trim())
        .bind(request.applicable_scope.trim())
        .bind(request.expiry_conditions.trim())
        .bind(request.review_due_at.trim())
        .bind(request.expression_reason.trim())
        .bind(request.logic_reason.trim())
        .bind(request.trust_reason.trim())
        .bind(request.action_reason.trim())
        .bind(request.reusable_outline.trim())
        .bind(request.forbidden_copy.trim())
        .bind(request.trainee_reference.trim())
        .bind(&fact_slots_json)
        .bind(request.internal_use_confirmed)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;

        for (line_order, line) in request.lines.iter().enumerate() {
            let line_order =
                i64::try_from(line_order).map_err(|_| DatabaseError::NumberExceedI64Range)?;
            sqlx::query(
                r#"INSERT INTO anchor_learning_case_lines (
                    line_id, case_id, line_order, start_ms, end_ms, original_text,
                    function_text, timing_reason, technique, trust_mechanism, action_cue,
                    risk_note, reusable_pattern
                  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(format!("line-{}", uuid::Uuid::new_v4().simple()))
            .bind(&case_id)
            .bind(line_order)
            .bind(line.start_ms)
            .bind(line.end_ms)
            .bind(line.original_text.trim())
            .bind(line.function_text.trim())
            .bind(line.timing_reason.trim())
            .bind(line.technique.trim())
            .bind(line.trust_mechanism.trim())
            .bind(line.action_cue.trim())
            .bind(line.risk_note.trim())
            .bind(line.reusable_pattern.trim())
            .execute(&mut *transaction)
            .await?;
        }
        for tag in &request.tags {
            sqlx::query(
                "INSERT INTO anchor_learning_case_tags (case_id, tag_kind, tag_value) VALUES (?, ?, ?)",
            )
            .bind(&case_id)
            .bind(tag.tag_kind.trim())
            .bind(tag.tag_value.trim())
            .execute(&mut *transaction)
            .await?;
        }
        insert_review_event(
            &mut transaction,
            &asset_id,
            "",
            "candidate",
            actor_kind,
            actor_id,
            "创建学习案例候选",
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        self.get_anchor_learning_case_unscoped(&case_id).await
    }

    pub async fn submit_anchor_learning_case(
        &self,
        case_id: &str,
        anchor_id: &str,
        submitted_by: &str,
    ) -> Result<LearningCaseRow, DatabaseError> {
        let case_id = case_id.trim();
        let anchor_id = anchor_id.trim();
        let submitted_by = submitted_by.trim();
        if case_id.is_empty() || anchor_id.is_empty() || submitted_by.is_empty() {
            return Err(invalid("案例、主播和提交人不能为空"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let row = sqlx::query(
            r#"SELECT assets.asset_id, assets.anchor_id, assets.review_status,
                      cases.evidence_level, cases.internal_use_confirmed,
                      cases.expression_reason, cases.logic_reason, cases.trust_reason,
                      cases.action_reason, cases.forbidden_copy, cases.applicable_scope,
                      cases.expiry_conditions, cases.review_due_at
               FROM anchor_learning_cases cases
               JOIN anchor_knowledge_assets assets ON assets.asset_id = cases.asset_id
               WHERE cases.case_id = ?"#,
        )
        .bind(case_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| invalid("学习案例不存在"))?;
        if row.get::<String, _>("anchor_id") != anchor_id
            || row.get::<String, _>("review_status") != "candidate"
        {
            return Err(invalid("仅所属主播的候选学习案例可以提交审核"));
        }
        if row.get::<String, _>("evidence_level") == "D" {
            return Err(invalid("D 级证据仅可保留为私有候选，不得提交发布审核"));
        }
        if row.get::<i64, _>("internal_use_confirmed") != 1 {
            return Err(invalid("尚未确认仅限内部学习使用"));
        }
        for field in [
            "expression_reason",
            "logic_reason",
            "trust_reason",
            "action_reason",
        ] {
            if row.get::<String, _>(field).trim().is_empty() {
                return Err(invalid("表达、逻辑、信任和行动四层原因必须完整"));
            }
        }
        if row.get::<String, _>("forbidden_copy").trim().is_empty() {
            return Err(invalid("必须明确禁止照搬的动态信息"));
        }
        if row.get::<String, _>("applicable_scope").trim().is_empty()
            || row.get::<String, _>("expiry_conditions").trim().is_empty()
        {
            return Err(invalid("适用范围和失效条件不能为空"));
        }
        let review_due_at = row.get::<String, _>("review_due_at");
        NaiveDate::parse_from_str(review_due_at.trim(), "%Y-%m-%d")
            .map_err(|_| invalid("复审日期必须是有效的 YYYY-MM-DD"))?;

        let asset_id = row.get::<String, _>("asset_id");
        let video_ranges = sqlx::query_as::<_, (i64, i64)>(
            r#"SELECT start_ms, end_ms FROM anchor_knowledge_sources
               WHERE asset_id = ? AND source_kind = 'video'
                 AND start_ms IS NOT NULL AND end_ms IS NOT NULL"#,
        )
        .bind(&asset_id)
        .fetch_all(&mut *transaction)
        .await?;
        if video_ranges.is_empty() {
            return Err(invalid("学习案例必须引用带时间范围的视频来源"));
        }
        let transcript_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM anchor_knowledge_sources WHERE asset_id = ? AND source_kind = 'transcript'",
        )
        .bind(&asset_id)
        .fetch_one(&mut *transaction)
        .await?;
        if transcript_count < 1 {
            return Err(invalid("学习案例必须引用逐字稿来源"));
        }
        let lines = sqlx::query_as::<_, (i64, i64)>(
            "SELECT start_ms, end_ms FROM anchor_learning_case_lines WHERE case_id = ? ORDER BY line_order",
        )
        .bind(case_id)
        .fetch_all(&mut *transaction)
        .await?;
        if lines.is_empty() {
            return Err(invalid("学习案例至少需要一行真实话术"));
        }
        if lines.iter().any(|(line_start, line_end)| {
            !video_ranges
                .iter()
                .any(|(video_start, video_end)| line_start >= video_start && line_end <= video_end)
        }) {
            return Err(invalid("话术时间范围必须位于引用视频片段内"));
        }

        let timestamp = now();
        let result = sqlx::query(
            r#"UPDATE anchor_knowledge_assets
               SET review_status = 'pending_review', submitted_at = ?, updated_at = ?
               WHERE asset_id = ? AND anchor_id = ? AND review_status = 'candidate'"#,
        )
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(&asset_id)
        .bind(anchor_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(invalid("学习案例状态已变化，请刷新后重试"));
        }
        insert_review_event(
            &mut transaction,
            &asset_id,
            "candidate",
            "pending_review",
            "human",
            submitted_by,
            "提交学习案例审核",
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        self.get_anchor_learning_case_unscoped(case_id).await
    }

    pub async fn review_anchor_learning_case(
        &self,
        case_id: &str,
        anchor_id: &str,
        reviewer_id: &str,
        reason: &str,
        published_relative_path: &str,
        published_file_hash: &str,
    ) -> Result<LearningCaseRow, DatabaseError> {
        let case = self
            .get_anchor_learning_case_unscoped(case_id.trim())
            .await?;
        self.review_anchor_knowledge_asset(
            ReviewAnchorKnowledgeAssetRequest {
                anchor_id: anchor_id.trim().to_string(),
                asset_id: case.asset_id,
                decision: "publish".into(),
                reviewer_id: reviewer_id.trim().to_string(),
                reason: reason.trim().to_string(),
            },
            Some(published_relative_path),
            Some(published_file_hash),
        )
        .await?;
        self.get_anchor_learning_case_unscoped(case_id.trim()).await
    }

    pub async fn reject_anchor_learning_case(
        &self,
        case_id: &str,
        anchor_id: &str,
        reviewer_id: &str,
        reason: &str,
    ) -> Result<LearningCaseRow, DatabaseError> {
        let case = self
            .get_anchor_learning_case_unscoped(case_id.trim())
            .await?;
        self.review_anchor_knowledge_asset(
            ReviewAnchorKnowledgeAssetRequest {
                anchor_id: anchor_id.trim().to_string(),
                asset_id: case.asset_id,
                decision: "reject".into(),
                reviewer_id: reviewer_id.trim().to_string(),
                reason: reason.trim().to_string(),
            },
            None,
            None,
        )
        .await?;
        self.get_anchor_learning_case_unscoped(case_id.trim()).await
    }

    pub async fn search_public_learning_cases(
        &self,
        request: SearchPublicLearningCasesRequest,
    ) -> Result<Vec<LearningCaseRow>, DatabaseError> {
        let anchor_id = request.anchor_id.as_deref().unwrap_or_default().trim();
        let stage = request.stage.as_deref().unwrap_or_default().trim();
        let skill = request.skill.as_deref().unwrap_or_default().trim();
        let difficulty = request.difficulty.as_deref().unwrap_or_default().trim();
        if !stage.is_empty()
            && !matches!(
                stage,
                "opening"
                    | "traffic"
                    | "needs"
                    | "explanation"
                    | "objection"
                    | "conversion"
                    | "after_sales"
                    | "retention"
            )
        {
            return Err(invalid("公开案例阶段筛选无效"));
        }
        if !difficulty.is_empty() && !matches!(difficulty, "beginner" | "intermediate" | "advanced")
        {
            return Err(invalid("公开案例难度筛选无效"));
        }
        let mut evidence_levels = request
            .evidence_levels
            .iter()
            .map(|level| level.trim())
            .filter(|level| !level.is_empty())
            .collect::<Vec<_>>();
        evidence_levels.sort_unstable();
        evidence_levels.dedup();
        if evidence_levels
            .iter()
            .any(|level| !matches!(*level, "A" | "B" | "C"))
        {
            return Err(invalid("公开案例证据等级仅允许 A、B、C"));
        }
        let evidence_filter = evidence_levels.join(",");
        let query = request.query.trim();
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        Ok(sqlx::query_as::<_, LearningCaseRow>(
            r#"SELECT cases.case_id, cases.asset_id, cases.stage, cases.skill,
                      cases.product_category, cases.difficulty, cases.evidence_level,
                      cases.evidence_summary, cases.operator_commentary, cases.ai_analysis,
                      cases.scene_context, cases.audience_trigger, cases.training_goal,
                      cases.applicable_scope, cases.expiry_conditions, cases.review_due_at,
                      cases.expression_reason, cases.logic_reason, cases.trust_reason,
                      cases.action_reason, cases.reusable_outline, cases.forbidden_copy,
                      cases.trainee_reference, cases.fact_slots_json,
                      cases.internal_use_confirmed, cases.retired_at, cases.retired_by,
                      cases.retire_reason, cases.created_at, cases.updated_at
               FROM anchor_learning_cases cases
               JOIN anchor_knowledge_assets assets ON assets.asset_id = cases.asset_id
               WHERE assets.review_status = 'published'
                 AND assets.is_current = 1
                 AND cases.retired_at IS NULL
                 AND cases.evidence_level IN ('A','B','C')
                 AND (? = '' OR assets.anchor_id = ?)
                 AND (? = '' OR cases.stage = ?)
                 AND (? = '' OR cases.skill = ?)
                 AND (? = '' OR cases.difficulty = ?)
                 AND (? = 1 OR instr(',' || ? || ',', ',' || cases.evidence_level || ',') > 0)
                 AND (? = '' OR assets.title LIKE '%' || ? || '%'
                              OR assets.body LIKE '%' || ? || '%'
                              OR cases.skill LIKE '%' || ? || '%'
                              OR cases.scene_context LIKE '%' || ? || '%')
               ORDER BY assets.published_at DESC, cases.case_id
               LIMIT 200"#,
        )
        .bind(anchor_id)
        .bind(anchor_id)
        .bind(stage)
        .bind(stage)
        .bind(skill)
        .bind(skill)
        .bind(difficulty)
        .bind(difficulty)
        .bind(if evidence_levels.is_empty() {
            1_i64
        } else {
            0_i64
        })
        .bind(&evidence_filter)
        .bind(query)
        .bind(query)
        .bind(query)
        .bind(query)
        .bind(query)
        .fetch_all(&pool)
        .await?)
    }

    pub async fn get_anchor_learning_case(
        &self,
        requester_anchor_id: &str,
        scope: &str,
        case_id: &str,
    ) -> Result<LearningCaseRow, DatabaseError> {
        let case_id = case_id.trim();
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let access = sqlx::query(
            r#"SELECT assets.anchor_id, assets.review_status, assets.is_current,
                      cases.retired_at, cases.evidence_level
               FROM anchor_learning_cases cases
               JOIN anchor_knowledge_assets assets ON assets.asset_id = cases.asset_id
               WHERE cases.case_id = ?"#,
        )
        .bind(case_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| invalid("学习案例不存在"))?;
        match scope.trim() {
            "private"
                if !requester_anchor_id.trim().is_empty()
                    && access.get::<String, _>("anchor_id") == requester_anchor_id.trim() => {}
            "public" | "team"
                if access.get::<String, _>("review_status") == "published"
                    && access.get::<i64, _>("is_current") == 1
                    && access.get::<Option<String>, _>("retired_at").is_none()
                    && matches!(
                        access.get::<String, _>("evidence_level").as_str(),
                        "A" | "B" | "C"
                    ) => {}
            "private" | "public" | "team" => return Err(invalid("无权读取该学习案例")),
            _ => return Err(invalid("案例读取 scope 必须是 private 或 public")),
        }
        self.get_anchor_learning_case_unscoped(case_id).await
    }

    pub async fn retire_anchor_learning_case(
        &self,
        case_id: &str,
        retired_by: &str,
        reason: &str,
    ) -> Result<LearningCaseRow, DatabaseError> {
        let case_id = case_id.trim();
        let retired_by = retired_by.trim();
        let reason = reason.trim();
        if case_id.is_empty() || retired_by.is_empty() || reason.is_empty() {
            return Err(invalid("案例、下架人和下架原因不能为空"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let row = sqlx::query(
            r#"SELECT assets.asset_id, assets.review_status, assets.is_current, cases.retired_at
               FROM anchor_learning_cases cases
               JOIN anchor_knowledge_assets assets ON assets.asset_id = cases.asset_id
               WHERE cases.case_id = ?"#,
        )
        .bind(case_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| invalid("学习案例不存在"))?;
        if row.get::<String, _>("review_status") != "published"
            || row.get::<i64, _>("is_current") != 1
            || row.get::<Option<String>, _>("retired_at").is_some()
        {
            return Err(invalid("仅当前已发布且未下架的学习案例可以下架"));
        }
        let asset_id = row.get::<String, _>("asset_id");
        let timestamp = now();
        sqlx::query(
            r#"UPDATE anchor_learning_cases
               SET retired_at = ?, retired_by = ?, retire_reason = ?, updated_at = ?
               WHERE case_id = ? AND retired_at IS NULL"#,
        )
        .bind(&timestamp)
        .bind(retired_by)
        .bind(reason)
        .bind(&timestamp)
        .bind(case_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE anchor_knowledge_assets SET is_current = 0, updated_at = ? WHERE asset_id = ?",
        )
        .bind(&timestamp)
        .bind(&asset_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM anchor_knowledge_search_index WHERE asset_id = ?")
            .bind(&asset_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.get_anchor_learning_case_unscoped(case_id).await
    }

    pub async fn submit_anchor_knowledge_asset(
        &self,
        request: SubmitAnchorKnowledgeAssetRequest,
    ) -> Result<AnchorKnowledgeAssetDetail, DatabaseError> {
        let anchor_id = request.anchor_id.trim();
        let asset_id = request.asset_id.trim();
        let actor_id = request.submitted_by.trim();
        if anchor_id.is_empty() || asset_id.is_empty() || actor_id.is_empty() {
            return Err(invalid("anchor_id、asset_id 和提交人不能为空"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let timestamp = now();
        let result = sqlx::query(
            r#"UPDATE anchor_knowledge_assets
               SET review_status = 'pending_review', submitted_at = ?, updated_at = ?
               WHERE asset_id = ? AND anchor_id = ? AND review_status = 'candidate'"#,
        )
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(asset_id)
        .bind(anchor_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(invalid("仅候选状态资产可以提交审核，且不得跨主播操作"));
        }
        insert_review_event(
            &mut transaction,
            asset_id,
            "candidate",
            "pending_review",
            "human",
            actor_id,
            "提交人工审核",
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        self.get_anchor_knowledge_asset_unscoped(asset_id).await
    }

    pub async fn review_anchor_knowledge_asset(
        &self,
        request: ReviewAnchorKnowledgeAssetRequest,
        published_relative_path: Option<&str>,
        published_file_hash: Option<&str>,
    ) -> Result<AnchorKnowledgeAssetDetail, DatabaseError> {
        let anchor_id = request.anchor_id.trim();
        let asset_id = request.asset_id.trim();
        let reviewer_id = request.reviewer_id.trim();
        if anchor_id.is_empty() || asset_id.is_empty() || reviewer_id.is_empty() {
            return Err(invalid("anchor_id、asset_id 和审核人不能为空"));
        }
        if !matches!(request.decision.trim(), "publish" | "reject") {
            return Err(invalid("审核决定必须是 publish 或 reject"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let row = sqlx::query(
            r#"SELECT assets.anchor_id, assets.review_status, assets.supersedes_asset_id,
                      (SELECT COUNT(*) FROM anchor_knowledge_sources sources WHERE sources.asset_id = assets.asset_id) AS source_count
               FROM anchor_knowledge_assets assets WHERE assets.asset_id = ?"#,
        )
        .bind(asset_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| invalid("知识资产不存在"))?;
        if row.get::<String, _>("anchor_id") != anchor_id
            || row.get::<String, _>("review_status") != "pending_review"
        {
            return Err(invalid("仅所属主播的待审核资产可以审核"));
        }
        let timestamp = now();
        if request.decision.trim() == "reject" {
            sqlx::query(
                r#"UPDATE anchor_knowledge_assets
                   SET review_status = 'rejected', reviewed_at = ?, reviewed_by = ?,
                       review_reason = ?, updated_at = ?, is_current = 0
                   WHERE asset_id = ?"#,
            )
            .bind(&timestamp)
            .bind(reviewer_id)
            .bind(request.reason.trim())
            .bind(&timestamp)
            .bind(asset_id)
            .execute(&mut *transaction)
            .await?;
            insert_review_event(
                &mut transaction,
                asset_id,
                "pending_review",
                "rejected",
                "human",
                reviewer_id,
                request.reason.trim(),
                &timestamp,
            )
            .await?;
        } else {
            if row.get::<i64, _>("source_count") < 1 {
                return Err(invalid("缺少来源引用，禁止发布"));
            }
            let relative_path = published_relative_path.unwrap_or_default().trim();
            let file_hash = published_file_hash.unwrap_or_default().trim();
            if relative_path.is_empty() || file_hash.is_empty() {
                return Err(invalid("发布文件路径和校验哈希不能为空"));
            }
            if let Some(previous_asset_id) = row.get::<Option<String>, _>("supersedes_asset_id") {
                sqlx::query(
                    "UPDATE anchor_knowledge_assets SET is_current = 0, updated_at = ? WHERE asset_id = ?",
                )
                .bind(&timestamp)
                .bind(&previous_asset_id)
                .execute(&mut *transaction)
                .await?;
                sqlx::query("DELETE FROM anchor_knowledge_search_index WHERE asset_id = ?")
                    .bind(previous_asset_id)
                    .execute(&mut *transaction)
                    .await?;
            }
            sqlx::query(
                r#"UPDATE anchor_knowledge_assets
                   SET review_status = 'published', reviewed_at = ?, reviewed_by = ?,
                       review_reason = ?, published_at = ?, updated_at = ?, is_current = 1,
                       published_relative_path = ?, published_file_hash = ?
                   WHERE asset_id = ?"#,
            )
            .bind(&timestamp)
            .bind(reviewer_id)
            .bind(request.reason.trim())
            .bind(&timestamp)
            .bind(&timestamp)
            .bind(relative_path)
            .bind(file_hash)
            .bind(asset_id)
            .execute(&mut *transaction)
            .await?;
            Self::index_published_anchor_asset(&mut transaction, asset_id).await?;
            insert_review_event(
                &mut transaction,
                asset_id,
                "pending_review",
                "published",
                "human",
                reviewer_id,
                request.reason.trim(),
                &timestamp,
            )
            .await?;
        }
        transaction.commit().await?;
        self.get_anchor_knowledge_asset_unscoped(asset_id).await
    }

    pub async fn import_published_anchor_knowledge_asset(
        &self,
        input: PublishedAnchorKnowledgeAssetInput,
    ) -> Result<AnchorKnowledgeAssetDetail, DatabaseError> {
        let asset_type = validate_asset_type(&input.asset_type)?;
        validate_sources(&input.sources)?;
        if input.asset_id.trim().is_empty()
            || input.anchor_id.trim().is_empty()
            || input.title.trim().is_empty()
            || input.body.trim().is_empty()
            || input.reviewer_id.trim().is_empty()
            || input.published_relative_path.trim().is_empty()
            || input.published_file_hash.trim().is_empty()
            || input.version < 1
        {
            return Err(invalid("已发布资产导入字段不完整"));
        }
        self.get_anchor_knowledge_profile(&input.anchor_id).await?;
        let content_hash = asset_content_hash(
            input.anchor_id.trim(),
            asset_type,
            input.title.trim(),
            input.body.trim(),
            input.product_id.trim(),
        );
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        if let Some(existing) = sqlx::query_as::<_, AssetIdentityRow>(
            "SELECT asset_id, content_hash, review_status FROM anchor_knowledge_assets WHERE asset_id = ?",
        )
        .bind(input.asset_id.trim())
        .fetch_optional(&pool)
        .await?
        {
            if existing.content_hash == content_hash && existing.review_status == "published" {
                return self.get_anchor_knowledge_asset_unscoped(&existing.asset_id).await;
            }
            return Err(invalid("相同资产编号已存在不同内容，禁止覆盖"));
        }
        let mut transaction = pool.begin().await?;
        let timestamp = now();
        if let Some(previous) = input
            .supersedes_asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let previous_row = sqlx::query(
                "SELECT anchor_id, asset_type, review_status FROM anchor_knowledge_assets WHERE asset_id = ?",
            )
            .bind(previous)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| invalid("被替代的已发布资产不存在"))?;
            if previous_row.get::<String, _>("anchor_id") != input.anchor_id.trim()
                || previous_row.get::<String, _>("asset_type") != asset_type
                || previous_row.get::<String, _>("review_status") != "published"
            {
                return Err(invalid("被替代资产不属于同一主播或类型"));
            }
            sqlx::query(
                "UPDATE anchor_knowledge_assets SET is_current = 0, updated_at = ? WHERE asset_id = ?",
            )
            .bind(&timestamp)
            .bind(previous)
            .execute(&mut *transaction)
            .await?;
            sqlx::query("DELETE FROM anchor_knowledge_search_index WHERE asset_id = ?")
                .bind(previous)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(
            r#"INSERT INTO anchor_knowledge_assets (
                asset_id, anchor_id, asset_type, title, body, product_id, review_status,
                version, supersedes_asset_id, created_by_kind, created_by_id, content_hash,
                is_current, published_relative_path, published_file_hash, created_at, updated_at,
                submitted_at, reviewed_at, reviewed_by, review_reason, published_at
              ) VALUES (?, ?, ?, ?, ?, ?, 'published', ?, ?, 'system', 'approved-master-import', ?,
                        1, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(input.asset_id.trim())
        .bind(input.anchor_id.trim())
        .bind(asset_type)
        .bind(input.title.trim())
        .bind(input.body.trim())
        .bind(input.product_id.trim())
        .bind(input.version)
        .bind(input.supersedes_asset_id.as_deref().map(str::trim))
        .bind(&content_hash)
        .bind(input.published_relative_path.trim())
        .bind(input.published_file_hash.trim())
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(input.reviewer_id.trim())
        .bind(input.review_reason.trim())
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;
        insert_sources(
            &mut transaction,
            input.asset_id.trim(),
            &input.sources,
            &timestamp,
        )
        .await?;
        Self::index_published_anchor_asset(&mut transaction, input.asset_id.trim()).await?;
        insert_review_event(
            &mut transaction,
            input.asset_id.trim(),
            "pending_review",
            "published",
            "human",
            input.reviewer_id.trim(),
            input.review_reason.trim(),
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        self.get_anchor_knowledge_asset_unscoped(input.asset_id.trim())
            .await
    }

    async fn index_published_anchor_asset(
        transaction: &mut Transaction<'_, Sqlite>,
        asset_id: &str,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"INSERT INTO anchor_knowledge_search_index (
                asset_id, anchor_id, anchor_name, asset_type, title, body, product_id,
                version, content_hash, published_at
              )
              SELECT assets.asset_id, assets.anchor_id, profiles.display_name, assets.asset_type,
                     assets.title, assets.body, assets.product_id, assets.version,
                     assets.content_hash, assets.published_at
              FROM anchor_knowledge_assets assets
              JOIN anchor_knowledge_profiles profiles ON profiles.anchor_id = assets.anchor_id
              WHERE assets.asset_id = ? AND assets.review_status = 'published' AND assets.is_current = 1
              ON CONFLICT(asset_id) DO UPDATE SET
                anchor_id = excluded.anchor_id,
                anchor_name = excluded.anchor_name,
                asset_type = excluded.asset_type,
                title = excluded.title,
                body = excluded.body,
                product_id = excluded.product_id,
                version = excluded.version,
                content_hash = excluded.content_hash,
                published_at = excluded.published_at"#,
        )
        .bind(asset_id)
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    pub async fn search_anchor_knowledge(
        &self,
        request: SearchAnchorKnowledgeRequest,
    ) -> Result<Vec<AnchorKnowledgeAssetSummary>, DatabaseError> {
        let requester = request.requester_anchor_id.trim();
        if requester.is_empty() {
            return Err(invalid("检索必须提供 requester_anchor_id"));
        }
        self.get_anchor_knowledge_profile(requester).await?;
        let asset_type = request.asset_type.as_deref().unwrap_or_default().trim();
        if !asset_type.is_empty() {
            validate_asset_type(asset_type)?;
        }
        let query = request.query.trim();
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        match request.scope.trim() {
            "private" => {
                if request
                    .owner_anchor_id
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|owner| !owner.is_empty() && owner != requester)
                {
                    return Err(invalid("私有检索不得查询其他主播"));
                }
                let status = request.review_status.as_deref().unwrap_or_default().trim();
                if !status.is_empty() && !PRIVATE_STATUSES.contains(&status) {
                    return Err(invalid("私有检索状态不受支持"));
                }
                Ok(sqlx::query_as::<_, AnchorKnowledgeAssetSummary>(
                    r#"SELECT assets.asset_id, assets.anchor_id, profiles.display_name AS anchor_name,
                              assets.asset_type, assets.title, assets.body, assets.product_id,
                              assets.review_status, assets.version, assets.supersedes_asset_id,
                              assets.content_hash, assets.is_current, assets.published_relative_path,
                              assets.published_file_hash, assets.created_at, assets.updated_at,
                              assets.reviewed_at, assets.reviewed_by, assets.review_reason, assets.published_at
                       FROM anchor_knowledge_assets assets
                       JOIN anchor_knowledge_profiles profiles ON profiles.anchor_id = assets.anchor_id
                       WHERE assets.anchor_id = ?
                         AND (? = '' OR assets.review_status = ?)
                         AND (? = '' OR assets.asset_type = ?)
                         AND (? = '' OR assets.title LIKE '%' || ? || '%'
                                      OR assets.body LIKE '%' || ? || '%'
                                      OR assets.product_id LIKE '%' || ? || '%')
                       ORDER BY assets.updated_at DESC, assets.asset_id
                       LIMIT 200"#,
                )
                .bind(requester)
                .bind(status)
                .bind(status)
                .bind(asset_type)
                .bind(asset_type)
                .bind(query)
                .bind(query)
                .bind(query)
                .bind(query)
                .fetch_all(&pool)
                .await?)
            }
            "team" => {
                if request
                    .review_status
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|status| !status.is_empty() && status != "published")
                {
                    return Err(invalid("团队检索只允许 published 状态"));
                }
                let owner = request
                    .owner_anchor_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim();
                Ok(sqlx::query_as::<_, AnchorKnowledgeAssetSummary>(
                    r#"SELECT assets.asset_id, assets.anchor_id, search.anchor_name,
                              assets.asset_type, assets.title, assets.body, assets.product_id,
                              assets.review_status, assets.version, assets.supersedes_asset_id,
                              assets.content_hash, assets.is_current, assets.published_relative_path,
                              assets.published_file_hash, assets.created_at, assets.updated_at,
                              assets.reviewed_at, assets.reviewed_by, assets.review_reason, assets.published_at
                       FROM anchor_knowledge_search_index search
                       JOIN anchor_knowledge_assets assets ON assets.asset_id = search.asset_id
                       WHERE (? = '' OR search.anchor_id = ?)
                         AND (? = '' OR search.asset_type = ?)
                         AND (? = '' OR search.title LIKE '%' || ? || '%'
                                      OR search.body LIKE '%' || ? || '%'
                                      OR search.product_id LIKE '%' || ? || '%')
                       ORDER BY search.published_at DESC, search.asset_id
                       LIMIT 200"#,
                )
                .bind(owner)
                .bind(owner)
                .bind(asset_type)
                .bind(asset_type)
                .bind(query)
                .bind(query)
                .bind(query)
                .bind(query)
                .fetch_all(&pool)
                .await?)
            }
            _ => Err(invalid("检索 scope 必须是 private 或 team")),
        }
    }

    pub async fn get_anchor_knowledge_asset(
        &self,
        request: GetAnchorKnowledgeAssetRequest,
    ) -> Result<AnchorKnowledgeAssetDetail, DatabaseError> {
        let requester = request.requester_anchor_id.trim();
        if requester.is_empty() {
            return Err(invalid("读取知识资产必须提供 requester_anchor_id"));
        }
        self.get_anchor_knowledge_profile(requester).await?;
        let detail = self
            .get_anchor_knowledge_asset_unscoped(request.asset_id.trim())
            .await?;
        match request.scope.trim() {
            "private" if detail.asset.anchor_id == requester => Ok(detail),
            "team" if detail.asset.review_status == "published" && detail.asset.is_current => {
                Ok(detail)
            }
            "private" | "team" => Err(invalid("无权读取该主播知识资产")),
            _ => Err(invalid("读取 scope 必须是 private 或 team")),
        }
    }

    pub async fn rebuild_anchor_knowledge_search_index(
        &self,
        requester_anchor_id: &str,
        owner_anchor_id: &str,
    ) -> Result<i64, DatabaseError> {
        let requester = requester_anchor_id.trim();
        let owner = owner_anchor_id.trim();
        if requester.is_empty() || owner.is_empty() {
            return Err(invalid(
                "重建索引必须提供 requester_anchor_id 和 owner_anchor_id",
            ));
        }
        self.get_anchor_knowledge_profile(requester).await?;
        self.get_anchor_knowledge_profile(owner).await?;
        if requester != owner {
            return Err(invalid("当前阶段仅允许主播重建自己的派生索引"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        sqlx::query("DELETE FROM anchor_knowledge_search_index WHERE anchor_id = ?")
            .bind(owner)
            .execute(&mut *transaction)
            .await?;
        let result = sqlx::query(
            r#"INSERT INTO anchor_knowledge_search_index (
                asset_id, anchor_id, anchor_name, asset_type, title, body, product_id,
                version, content_hash, published_at
              )
              SELECT assets.asset_id, assets.anchor_id, profiles.display_name, assets.asset_type,
                     assets.title, assets.body, assets.product_id, assets.version,
                     assets.content_hash, assets.published_at
              FROM anchor_knowledge_assets assets
              JOIN anchor_knowledge_profiles profiles ON profiles.anchor_id = assets.anchor_id
              WHERE assets.anchor_id = ? AND assets.review_status = 'published' AND assets.is_current = 1"#,
        )
        .bind(owner)
        .execute(&mut *transaction)
        .await?;
        let count = i64::try_from(result.rows_affected())
            .map_err(|_| DatabaseError::NumberExceedI64Range)?;
        transaction.commit().await?;
        Ok(count)
    }

    async fn get_anchor_learning_case_unscoped(
        &self,
        case_id: &str,
    ) -> Result<LearningCaseRow, DatabaseError> {
        if case_id.trim().is_empty() {
            return Err(invalid("case_id 不能为空"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query_as::<_, LearningCaseRow>(
            r#"SELECT case_id, asset_id, stage, skill, product_category, difficulty,
                      evidence_level, evidence_summary, operator_commentary, ai_analysis,
                      scene_context, audience_trigger, training_goal, applicable_scope,
                      expiry_conditions, review_due_at, expression_reason, logic_reason,
                      trust_reason, action_reason, reusable_outline, forbidden_copy,
                      trainee_reference, fact_slots_json, internal_use_confirmed, retired_at,
                      retired_by, retire_reason, created_at, updated_at
               FROM anchor_learning_cases WHERE case_id = ?"#,
        )
        .bind(case_id.trim())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| invalid("学习案例不存在"))
    }

    async fn get_anchor_knowledge_asset_unscoped(
        &self,
        asset_id: &str,
    ) -> Result<AnchorKnowledgeAssetDetail, DatabaseError> {
        if asset_id.trim().is_empty() {
            return Err(invalid("asset_id 不能为空"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let asset = sqlx::query_as::<_, AnchorKnowledgeAssetSummary>(
            r#"SELECT assets.asset_id, assets.anchor_id, profiles.display_name AS anchor_name,
                      assets.asset_type, assets.title, assets.body, assets.product_id,
                      assets.review_status, assets.version, assets.supersedes_asset_id,
                      assets.content_hash, assets.is_current, assets.published_relative_path,
                      assets.published_file_hash, assets.created_at, assets.updated_at,
                      assets.reviewed_at, assets.reviewed_by, assets.review_reason, assets.published_at
               FROM anchor_knowledge_assets assets
               JOIN anchor_knowledge_profiles profiles ON profiles.anchor_id = assets.anchor_id
               WHERE assets.asset_id = ?"#,
        )
        .bind(asset_id.trim())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| invalid("知识资产不存在"))?;
        let sources = sqlx::query_as::<_, AnchorKnowledgeSource>(
            r#"SELECT source_id, source_kind, source_locator, video_id, start_ms, end_ms,
                      transcript_version, transcript_hash, product_fact_id, product_fact_version,
                      analysis_version, content_hash
               FROM anchor_knowledge_sources WHERE asset_id = ? ORDER BY source_id"#,
        )
        .bind(asset_id.trim())
        .fetch_all(&pool)
        .await?;
        Ok(AnchorKnowledgeAssetDetail { asset, sources })
    }
}

#[derive(Debug, FromRow)]
struct AssetIdentityRow {
    asset_id: String,
    content_hash: String,
    review_status: String,
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;

    use super::*;

    async fn database() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        pool.execute(ANCHOR_KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool.execute(ANCHOR_LEARNING_CASE_MIGRATION_SQL)
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    #[tokio::test]
    async fn learning_case_migration_enforces_publication_and_timeline_boundaries() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        pool.execute(ANCHOR_KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool.execute(ANCHOR_LEARNING_CASE_MIGRATION_SQL)
            .await
            .unwrap();

        let tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('anchor_learning_cases','anchor_learning_case_lines','anchor_learning_case_tags','anchor_learning_case_attempts')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tables, 4);

        pool.execute(
            "INSERT INTO anchor_knowledge_profiles (anchor_id, display_name, normalized_name, vault_relative_root, created_at, updated_at) VALUES ('anchor-a','主播A','主播a','主播知识库/anchor-a',datetime('now'),datetime('now'))",
        )
        .await
        .unwrap();
        pool.execute(
            "INSERT INTO anchor_knowledge_assets (asset_id, anchor_id, asset_type, title, body, created_by_kind, created_by_id, content_hash, is_current, created_at, updated_at) VALUES ('a1','anchor-a','speech','价格异议处理','先确认需求再提供证据','human','operator','asset-hash',1,datetime('now'),datetime('now'))",
        )
        .await
        .unwrap();

        let missing_asset = sqlx::query(
            "INSERT INTO anchor_learning_cases (case_id, asset_id, stage, skill, product_category, difficulty, evidence_level, evidence_summary, operator_commentary, ai_analysis, scene_context, audience_trigger, training_goal, applicable_scope, expiry_conditions, review_due_at, expression_reason, logic_reason, trust_reason, action_reason, reusable_outline, forbidden_copy, trainee_reference, fact_slots_json, internal_use_confirmed, created_at, updated_at) VALUES ('missing-case','missing','needs','需求确认','相机','beginner','D','待审核','运营说明','AI分析','场景','评论','目标','二手相机','规则变化','2026-12-31','表达','逻辑','信任','行动','骨架','禁用','参考','[]',1,datetime('now'),datetime('now'))",
        )
        .execute(&pool)
        .await;
        assert!(missing_asset.is_err());

        pool.execute(
            "INSERT INTO anchor_learning_cases (case_id, asset_id, stage, skill, product_category, difficulty, evidence_level, evidence_summary, operator_commentary, ai_analysis, scene_context, audience_trigger, training_goal, applicable_scope, expiry_conditions, review_due_at, expression_reason, logic_reason, trust_reason, action_reason, reusable_outline, forbidden_copy, trainee_reference, fact_slots_json, internal_use_confirmed, created_at, updated_at) VALUES ('c1','a1','needs','需求确认','相机','beginner','D','待审核','运营说明','AI分析','场景','评论','目标','二手相机','规则变化','2026-12-31','表达','逻辑','信任','行动','骨架','禁用','参考','[\"用途\"]',1,datetime('now'),datetime('now'))",
        )
        .await
        .unwrap();
        let persisted: (String, String, String, String, String, i64) = sqlx::query_as(
            "SELECT cases.operator_commentary, cases.ai_analysis, cases.applicable_scope, cases.expiry_conditions, cases.review_due_at, assets.is_current FROM anchor_learning_cases cases JOIN anchor_knowledge_assets assets ON assets.asset_id = cases.asset_id WHERE cases.case_id = 'c1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            persisted,
            (
                "运营说明".into(),
                "AI分析".into(),
                "二手相机".into(),
                "规则变化".into(),
                "2026-12-31".into(),
                1,
            )
        );

        let duplicate_extension = sqlx::query(
            "INSERT INTO anchor_learning_cases (case_id, asset_id, stage, skill, difficulty, evidence_level, scene_context, training_goal, applicable_scope, expiry_conditions, review_due_at, fact_slots_json, created_at, updated_at) VALUES ('c2','a1','needs','需求确认','beginner','A','场景','目标','范围','条件','2026-12-31','[]',datetime('now'),datetime('now'))",
        )
        .execute(&pool)
        .await;
        assert!(duplicate_extension.is_err());

        let invalid_line = sqlx::query(
            "INSERT INTO anchor_learning_case_lines (line_id, case_id, line_order, start_ms, end_ms, original_text) VALUES ('l1','c1',0,1000,1000,'先问用途')",
        )
        .execute(&pool)
        .await;
        assert!(invalid_line.is_err());
        pool.execute(
            "INSERT INTO anchor_learning_case_lines (line_id, case_id, line_order, start_ms, end_ms, original_text) VALUES ('l1','c1',0,1000,2500,'先问用途')",
        )
        .await
        .unwrap();

        let invalid_snapshot = sqlx::query(
            "INSERT INTO anchor_learning_case_attempts (attempt_id, case_id, trainee_anchor_id, practice_mode, evaluation_status, case_snapshot_json, created_at) VALUES ('try-1','c1','anchor-a','shadow','scored','not-json',datetime('now'))",
        )
        .execute(&pool)
        .await;
        assert!(invalid_snapshot.is_err());
        pool.execute(
            "INSERT INTO anchor_learning_case_attempts (attempt_id, case_id, trainee_anchor_id, practice_mode, evaluation_status, case_snapshot_json, created_at) VALUES ('try-1','c1','anchor-a','shadow','scored','{\"caseId\":\"c1\"}',datetime('now'))",
        )
        .await
        .unwrap();
        assert!(
            sqlx::query("DELETE FROM anchor_learning_cases WHERE case_id = 'c1'")
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn learning_cases_only_extend_speech_assets() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        pool.execute(ANCHOR_KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool.execute(ANCHOR_LEARNING_CASE_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(
            "INSERT INTO anchor_knowledge_profiles (anchor_id, display_name, normalized_name, vault_relative_root, created_at, updated_at) VALUES ('anchor-a','主播A','主播a','主播知识库/anchor-a',datetime('now'),datetime('now'))",
        )
        .await
        .unwrap();
        for (asset_id, asset_type) in [
            ("speech-1", "speech"),
            ("deal-1", "deal_clip"),
            ("deal-2", "deal_clip"),
        ] {
            sqlx::query(
                "INSERT INTO anchor_knowledge_assets (asset_id, anchor_id, asset_type, title, body, created_by_kind, created_by_id, content_hash, created_at, updated_at) VALUES (?,?,?,?,?,'human','operator',?,datetime('now'),datetime('now'))",
            )
            .bind(asset_id)
            .bind("anchor-a")
            .bind(asset_type)
            .bind(format!("title-{asset_id}"))
            .bind(format!("body-{asset_id}"))
            .bind(format!("hash-{asset_id}"))
            .execute(&pool)
            .await
            .unwrap();
        }

        pool.execute(
            "INSERT INTO anchor_learning_cases (case_id, asset_id, stage, skill, difficulty, evidence_level, scene_context, training_goal, applicable_scope, expiry_conditions, review_due_at, created_at, updated_at) VALUES ('speech-case','speech-1','needs','需求确认','beginner','A','场景','目标','范围','条件','2026-12-31',datetime('now'),datetime('now'))",
        )
        .await
        .unwrap();
        let insert_error = sqlx::query(
            "INSERT INTO anchor_learning_cases (case_id, asset_id, stage, skill, difficulty, evidence_level, scene_context, training_goal, applicable_scope, expiry_conditions, review_due_at, created_at, updated_at) VALUES ('deal-case','deal-1','needs','需求确认','beginner','A','场景','目标','范围','条件','2026-12-31',datetime('now'),datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(insert_error
            .to_string()
            .contains("learning case asset must be speech"));

        let update_error = sqlx::query(
            "UPDATE anchor_learning_cases SET asset_id = 'deal-2' WHERE case_id = 'speech-case'",
        )
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(update_error
            .to_string()
            .contains("learning case asset must be speech"));
    }

    fn valid_learning_case_request(
        anchor_id: &str,
        evidence_level: &str,
    ) -> CreateAnchorLearningCaseRequest {
        CreateAnchorLearningCaseRequest {
            anchor_id: anchor_id.into(),
            title: "价格异议处理".into(),
            body: "先确认需求再提供证据".into(),
            product_id: "camera-1".into(),
            stage: "objection".into(),
            skill: "价格异议处理".into(),
            product_category: "相机".into(),
            difficulty: "beginner".into(),
            evidence_level: evidence_level.into(),
            evidence_summary: "运营已核对评论和现场表现".into(),
            operator_commentary: "运营点评：先确认需求再回应价格".into(),
            ai_analysis: "AI分析：追问可降低直接比价风险".into(),
            scene_context: "观众认为价格偏高".into(),
            audience_trigger: "为什么比别家贵".into(),
            training_goal: "先确认用途再展示证据".into(),
            applicable_scope: "二手相机价格异议".into(),
            expiry_conditions: "价格、库存或售后规则变化时复审".into(),
            review_due_at: "2026-12-31".into(),
            expression_reason: "短句承接".into(),
            logic_reason: "先需求后证据".into(),
            trust_reason: "只引用已核验事实".into(),
            action_reason: "引导继续看实物".into(),
            reusable_outline: "承接→追问→证据→行动".into(),
            forbidden_copy: "价格、库存、赠品、成色、售后和链接不得照搬".into(),
            trainee_reference: "先确认您的[用途]，再看[已核验事实]".into(),
            fact_slots: vec!["用途".into(), "已核验事实".into()],
            internal_use_confirmed: true,
            created_by_kind: "human".into(),
            created_by_id: "operator".into(),
            supersedes_asset_id: None,
            sources: vec![
                AnchorKnowledgeSourceInput {
                    source_kind: "video".into(),
                    source_locator: "C:/fixtures/case.mp4".into(),
                    video_id: Some(1),
                    start_ms: Some(1000),
                    end_ms: Some(4000),
                    transcript_version: String::new(),
                    transcript_hash: String::new(),
                    product_fact_id: String::new(),
                    product_fact_version: String::new(),
                    analysis_version: String::new(),
                    content_hash: "video-hash".into(),
                },
                AnchorKnowledgeSourceInput {
                    source_kind: "transcript".into(),
                    source_locator: "video:1".into(),
                    video_id: Some(1),
                    start_ms: Some(1000),
                    end_ms: Some(4000),
                    transcript_version: "v1".into(),
                    transcript_hash: "transcript-hash".into(),
                    product_fact_id: String::new(),
                    product_fact_version: String::new(),
                    analysis_version: String::new(),
                    content_hash: "transcript-source-hash".into(),
                },
            ],
            lines: vec![LearningCaseLineInput {
                start_ms: 1000,
                end_ms: 2500,
                original_text: "先问用途".into(),
                function_text: "需求确认".into(),
                timing_reason: "承接评论".into(),
                technique: "追问".into(),
                trust_mechanism: String::new(),
                action_cue: "停顿".into(),
                risk_note: String::new(),
                reusable_pattern: "先问[用途]".into(),
            }],
            tags: vec![LearningCaseTagInput {
                tag_kind: "skill".into(),
                tag_value: "价格异议".into(),
            }],
        }
    }

    async fn publish_learning_case(
        database: &Database,
        anchor_id: &str,
        case_id: &str,
    ) -> LearningCaseRow {
        database
            .submit_anchor_learning_case(case_id, anchor_id, "operator")
            .await
            .unwrap();
        database
            .review_anchor_learning_case(
                case_id,
                anchor_id,
                "reviewer",
                "可用于培训",
                &format!("主播知识库/{anchor_id}/精品案例/{case_id}.md"),
                "file-hash",
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn learning_case_lifecycle_is_private_until_published_and_retirement_is_immediate() {
        let database = database().await;
        let anchor = database
            .create_anchor_knowledge_profile(CreateAnchorKnowledgeProfileRequest {
                display_name: "主播A".into(),
                aliases: vec![],
            })
            .await
            .unwrap();
        let other_anchor = profile(&database, "主播B").await;

        let case_d = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "D"))
            .await
            .unwrap();
        assert!(database
            .submit_anchor_learning_case(&case_d.case_id, &anchor.anchor_id, "operator")
            .await
            .is_err());
        assert_eq!(
            database
                .search_public_learning_cases(Default::default())
                .await
                .unwrap()
                .len(),
            0
        );

        let case_a = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "A"))
            .await
            .unwrap();
        assert!(database
            .get_anchor_learning_case(&other_anchor.anchor_id, "private", &case_a.case_id)
            .await
            .is_err());
        database
            .submit_anchor_learning_case(&case_a.case_id, &anchor.anchor_id, "operator")
            .await
            .unwrap();
        let published = database
            .review_anchor_learning_case(
                &case_a.case_id,
                &anchor.anchor_id,
                "reviewer",
                "可用于培训",
                "主播知识库/a/精品案例/c-v1.md",
                "file-hash",
            )
            .await
            .unwrap();
        assert_eq!(
            database
                .search_public_learning_cases(Default::default())
                .await
                .unwrap()[0]
                .case_id,
            published.case_id
        );
        database
            .retire_anchor_learning_case(&published.case_id, "reviewer", "售后政策已变化")
            .await
            .unwrap();
        assert!(database
            .search_public_learning_cases(Default::default())
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn learning_case_submission_rejects_each_publication_gate() {
        let database = database().await;
        let anchor = profile(&database, "主播A").await;

        for (marker, mutate) in [
            ("internal-use", 0),
            ("video-source", 1),
            ("transcript-source", 2),
            ("four-reasons", 3),
            ("lines", 4),
            ("line-range", 5),
            ("forbidden-copy", 6),
            ("scope", 7),
            ("expiry", 8),
            ("review-date", 9),
        ] {
            let mut request = valid_learning_case_request(&anchor.anchor_id, "A");
            request.title = marker.into();
            match mutate {
                0 => request.internal_use_confirmed = false,
                1 => request
                    .sources
                    .retain(|source| source.source_kind != "video"),
                2 => request
                    .sources
                    .retain(|source| source.source_kind != "transcript"),
                3 => request.trust_reason.clear(),
                4 => request.lines.clear(),
                5 => request.lines[0].end_ms = 5000,
                6 => request.forbidden_copy = "  ".into(),
                7 | 8 => {}
                9 => request.review_due_at = "2026-02-30".into(),
                _ => unreachable!(),
            }
            let case = database.create_anchor_learning_case(request).await.unwrap();
            if mutate == 7 || mutate == 8 {
                let pool = database.db.read().await.clone().unwrap();
                pool.execute("PRAGMA ignore_check_constraints = ON")
                    .await
                    .unwrap();
                let column = if mutate == 7 {
                    "applicable_scope"
                } else {
                    "expiry_conditions"
                };
                sqlx::query(&format!(
                    "UPDATE anchor_learning_cases SET {column} = '  ' WHERE case_id = ?"
                ))
                .bind(&case.case_id)
                .execute(&pool)
                .await
                .unwrap();
                pool.execute("PRAGMA ignore_check_constraints = OFF")
                    .await
                    .unwrap();
            }
            assert!(
                database
                    .submit_anchor_learning_case(&case.case_id, &anchor.anchor_id, "operator")
                    .await
                    .is_err(),
                "submission gate unexpectedly accepted {marker}"
            );
        }
    }

    #[tokio::test]
    async fn learning_case_public_search_excludes_candidate_pending_rejected_d_retired_and_old_versions(
    ) {
        let database = database().await;
        let anchor = profile(&database, "主播A").await;
        let mut hidden_ids = Vec::new();

        let candidate = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "A"))
            .await
            .unwrap();
        hidden_ids.push(candidate.case_id.clone());

        let pending = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "B"))
            .await
            .unwrap();
        database
            .submit_anchor_learning_case(&pending.case_id, &anchor.anchor_id, "operator")
            .await
            .unwrap();
        hidden_ids.push(pending.case_id.clone());

        let rejected = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "C"))
            .await
            .unwrap();
        database
            .submit_anchor_learning_case(&rejected.case_id, &anchor.anchor_id, "operator")
            .await
            .unwrap();
        database
            .reject_anchor_learning_case(
                &rejected.case_id,
                &anchor.anchor_id,
                "reviewer",
                "证据不足",
            )
            .await
            .unwrap();
        hidden_ids.push(rejected.case_id.clone());

        let published_d = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "D"))
            .await
            .unwrap();
        let pool = database.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE anchor_knowledge_assets SET review_status='published', is_current=1, reviewed_at=?, reviewed_by='fixture', published_at=?, published_relative_path='fixture.md', published_file_hash='fixture-hash' WHERE asset_id=?",
        )
        .bind(now())
        .bind(now())
        .bind(&published_d.asset_id)
        .execute(&pool)
        .await
        .unwrap();
        hidden_ids.push(published_d.case_id.clone());

        let retired = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "A"))
            .await
            .unwrap();
        let retired = publish_learning_case(&database, &anchor.anchor_id, &retired.case_id).await;
        database
            .retire_anchor_learning_case(&retired.case_id, "reviewer", "规则变化")
            .await
            .unwrap();
        hidden_ids.push(retired.case_id.clone());

        let old = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "B"))
            .await
            .unwrap();
        let old = publish_learning_case(&database, &anchor.anchor_id, &old.case_id).await;
        let mut next_request = valid_learning_case_request(&anchor.anchor_id, "B");
        next_request.supersedes_asset_id = Some(old.asset_id.clone());
        let current = database
            .create_anchor_learning_case(next_request)
            .await
            .unwrap();
        let current = publish_learning_case(&database, &anchor.anchor_id, &current.case_id).await;
        hidden_ids.push(old.case_id.clone());

        let visible_a = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "A"))
            .await
            .unwrap();
        let visible_a =
            publish_learning_case(&database, &anchor.anchor_id, &visible_a.case_id).await;
        let visible_c = database
            .create_anchor_learning_case(valid_learning_case_request(&anchor.anchor_id, "C"))
            .await
            .unwrap();
        let visible_c =
            publish_learning_case(&database, &anchor.anchor_id, &visible_c.case_id).await;

        let result = database
            .search_public_learning_cases(Default::default())
            .await
            .unwrap();
        let result_ids = result
            .into_iter()
            .map(|case| case.case_id)
            .collect::<std::collections::HashSet<_>>();
        let expected = [visible_a.case_id, current.case_id, visible_c.case_id]
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(result_ids, expected);
        assert!(hidden_ids
            .into_iter()
            .all(|case_id| !result_ids.contains(&case_id)));
    }

    #[tokio::test]
    async fn learning_case_creation_rolls_back_all_rows_when_an_extension_insert_fails() {
        let database = database().await;
        let anchor = profile(&database, "主播A").await;
        let mut request = valid_learning_case_request(&anchor.anchor_id, "A");
        request.tags[0].tag_kind = "not-supported".into();

        assert!(database.create_anchor_learning_case(request).await.is_err());
        let pool = database.db.read().await.clone().unwrap();
        let assets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM anchor_knowledge_assets")
            .fetch_one(&pool)
            .await
            .unwrap();
        let cases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM anchor_learning_cases")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((assets, cases), (0, 0));
    }

    fn source(marker: &str) -> AnchorKnowledgeSourceInput {
        AnchorKnowledgeSourceInput {
            source_kind: "transcript".into(),
            source_locator: format!("video:{marker}"),
            video_id: Some(1),
            start_ms: Some(1000),
            end_ms: Some(3000),
            transcript_version: "v1".into(),
            transcript_hash: format!("transcript-{marker}"),
            product_fact_id: String::new(),
            product_fact_version: String::new(),
            analysis_version: String::new(),
            content_hash: format!("source-{marker}"),
        }
    }

    async fn profile(database: &Database, name: &str) -> AnchorKnowledgeProfile {
        database
            .create_anchor_knowledge_profile(CreateAnchorKnowledgeProfileRequest {
                display_name: name.into(),
                aliases: Vec::new(),
            })
            .await
            .unwrap()
    }

    async fn candidate(
        database: &Database,
        anchor_id: &str,
        marker: &str,
    ) -> AnchorKnowledgeAssetDetail {
        database
            .create_anchor_knowledge_candidate(CreateAnchorKnowledgeCandidateRequest {
                anchor_id: anchor_id.into(),
                asset_type: "speech".into(),
                title: format!("title-{marker}"),
                body: format!("body-{marker}"),
                product_id: "product-1".into(),
                created_by_kind: "model".into(),
                created_by_id: "analysis-model".into(),
                supersedes_asset_id: None,
                sources: vec![source(marker)],
            })
            .await
            .unwrap()
    }

    async fn publish(
        database: &Database,
        anchor_id: &str,
        asset_id: &str,
    ) -> AnchorKnowledgeAssetDetail {
        database
            .submit_anchor_knowledge_asset(SubmitAnchorKnowledgeAssetRequest {
                anchor_id: anchor_id.into(),
                asset_id: asset_id.into(),
                submitted_by: "anchor-user".into(),
            })
            .await
            .unwrap();
        database
            .review_anchor_knowledge_asset(
                ReviewAnchorKnowledgeAssetRequest {
                    anchor_id: anchor_id.into(),
                    asset_id: asset_id.into(),
                    decision: "publish".into(),
                    reviewer_id: "anchor-user".into(),
                    reason: "checked".into(),
                },
                Some(&format!("主播知识库/{anchor_id}/{asset_id}.md")),
                Some("file-hash"),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn private_search_never_crosses_anchor_scope() {
        let database = database().await;
        let anchor_a = profile(&database, "主播A").await;
        let anchor_b = profile(&database, "主播B").await;
        candidate(&database, &anchor_a.anchor_id, "only-a").await;
        candidate(&database, &anchor_b.anchor_id, "only-b").await;

        let result = database
            .search_anchor_knowledge(SearchAnchorKnowledgeRequest {
                requester_anchor_id: anchor_a.anchor_id.clone(),
                scope: "private".into(),
                owner_anchor_id: None,
                query: "only-b".into(),
                asset_type: None,
                review_status: None,
            })
            .await
            .unwrap();
        assert!(result.is_empty());
        let error = database
            .search_anchor_knowledge(SearchAnchorKnowledgeRequest {
                requester_anchor_id: anchor_a.anchor_id,
                scope: "private".into(),
                owner_anchor_id: Some(anchor_b.anchor_id),
                query: String::new(),
                asset_type: None,
                review_status: None,
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("不得查询其他主播"));
    }

    #[tokio::test]
    async fn team_search_only_returns_published_assets() {
        let database = database().await;
        let anchor = profile(&database, "主播A").await;
        let pending = candidate(&database, &anchor.anchor_id, "pending-secret").await;
        database
            .submit_anchor_knowledge_asset(SubmitAnchorKnowledgeAssetRequest {
                anchor_id: anchor.anchor_id.clone(),
                asset_id: pending.asset.asset_id,
                submitted_by: "anchor-user".into(),
            })
            .await
            .unwrap();
        let published = candidate(&database, &anchor.anchor_id, "published-team").await;
        publish(&database, &anchor.anchor_id, &published.asset.asset_id).await;

        let hidden = database
            .search_anchor_knowledge(SearchAnchorKnowledgeRequest {
                requester_anchor_id: anchor.anchor_id.clone(),
                scope: "team".into(),
                owner_anchor_id: None,
                query: "pending-secret".into(),
                asset_type: None,
                review_status: None,
            })
            .await
            .unwrap();
        assert!(hidden.is_empty());
        let visible = database
            .search_anchor_knowledge(SearchAnchorKnowledgeRequest {
                requester_anchor_id: anchor.anchor_id,
                scope: "team".into(),
                owner_anchor_id: None,
                query: "published-team".into(),
                asset_type: None,
                review_status: Some("published".into()),
            })
            .await
            .unwrap();
        assert_eq!(visible.len(), 1);
    }

    #[tokio::test]
    async fn product_facts_are_references_not_writable_asset_types() {
        let database = database().await;
        let anchor = profile(&database, "主播A").await;
        let error = database
            .create_anchor_knowledge_candidate(CreateAnchorKnowledgeCandidateRequest {
                anchor_id: anchor.anchor_id,
                asset_type: "product_fact".into(),
                title: "attempt".into(),
                body: "overwrite".into(),
                product_id: "fact-1".into(),
                created_by_kind: "model".into(),
                created_by_id: "model".into(),
                supersedes_asset_id: None,
                sources: vec![source("fact")],
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("产品事实只能引用"));
    }

    #[tokio::test]
    async fn published_versions_are_immutable_and_index_is_rebuildable_per_anchor() {
        let database = database().await;
        let anchor_a = profile(&database, "主播A").await;
        let anchor_b = profile(&database, "主播B").await;
        let first = candidate(&database, &anchor_a.anchor_id, "a-v1").await;
        let first = publish(&database, &anchor_a.anchor_id, &first.asset.asset_id).await;
        let b = candidate(&database, &anchor_b.anchor_id, "b-v1").await;
        publish(&database, &anchor_b.anchor_id, &b.asset.asset_id).await;

        let second = database
            .create_anchor_knowledge_candidate(CreateAnchorKnowledgeCandidateRequest {
                anchor_id: anchor_a.anchor_id.clone(),
                asset_type: "speech".into(),
                title: "title-a-v2".into(),
                body: "body-a-v2".into(),
                product_id: "product-1".into(),
                created_by_kind: "human".into(),
                created_by_id: "anchor-user".into(),
                supersedes_asset_id: Some(first.asset.asset_id.clone()),
                sources: vec![source("a-v2")],
            })
            .await
            .unwrap();
        assert_eq!(second.asset.version, 2);
        publish(&database, &anchor_a.anchor_id, &second.asset.asset_id).await;
        let old = database
            .get_anchor_knowledge_asset(GetAnchorKnowledgeAssetRequest {
                requester_anchor_id: anchor_a.anchor_id.clone(),
                scope: "private".into(),
                asset_id: first.asset.asset_id,
            })
            .await
            .unwrap();
        assert_eq!(old.asset.body, "body-a-v1");
        assert!(!old.asset.is_current);

        let rebuilt = database
            .rebuild_anchor_knowledge_search_index(&anchor_a.anchor_id, &anchor_a.anchor_id)
            .await
            .unwrap();
        assert_eq!(rebuilt, 1);
        let b_result = database
            .search_anchor_knowledge(SearchAnchorKnowledgeRequest {
                requester_anchor_id: anchor_b.anchor_id,
                scope: "team".into(),
                owner_anchor_id: None,
                query: "b-v1".into(),
                asset_type: None,
                review_status: None,
            })
            .await
            .unwrap();
        assert_eq!(b_result.len(), 1);
    }

    #[tokio::test]
    async fn approved_master_import_is_idempotent_and_never_overwrites_content() {
        let database = database().await;
        let anchor = profile(&database, "主播A").await;
        let input = PublishedAnchorKnowledgeAssetInput {
            asset_id: "master-batch-7-speech-v1".into(),
            anchor_id: anchor.anchor_id.clone(),
            asset_type: "speech".into(),
            title: "人工确认母稿".into(),
            body: "可检索内容".into(),
            product_id: String::new(),
            version: 1,
            supersedes_asset_id: None,
            reviewer_id: "master-sample-batch:7".into(),
            review_reason: "人工确认发布".into(),
            published_relative_path: format!("主播知识库/{}/话术/V1.0/README.md", anchor.anchor_id),
            published_file_hash: "published-file-hash".into(),
            sources: vec![source("approved-master")],
        };
        let imported = database
            .import_published_anchor_knowledge_asset(input.clone())
            .await
            .unwrap();
        assert_eq!(imported.asset.review_status, "published");
        let repeated = database
            .import_published_anchor_knowledge_asset(input.clone())
            .await
            .unwrap();
        assert_eq!(repeated.asset.asset_id, imported.asset.asset_id);

        let mut conflicting = input;
        conflicting.body = "不同内容".into();
        let error = database
            .import_published_anchor_knowledge_asset(conflicting)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("禁止覆盖"));
    }
}
