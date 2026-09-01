use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::{Database, DatabaseError};

pub const TRAINING_MIGRATION_SQL: &str = r#"
CREATE TABLE training_roles (
  role_id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL UNIQUE,
  avatar_key TEXT NOT NULL DEFAULT '',
  is_active INTEGER NOT NULL DEFAULT 1 CHECK(is_active IN (0, 1)),
  unavailable_reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE training_role_aliases (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  role_id TEXT NOT NULL REFERENCES training_roles(role_id) ON DELETE CASCADE,
  source_alias TEXT NOT NULL UNIQUE,
  is_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(is_confirmed IN (0, 1)),
  confirmed_at TEXT,
  created_at TEXT NOT NULL
);
CREATE TABLE training_cases (
  case_id TEXT PRIMARY KEY,
  role_id TEXT NOT NULL REFERENCES training_roles(role_id) ON DELETE RESTRICT,
  module TEXT NOT NULL CHECK(module IN ('opening','retention_interaction','needs_discovery','product_explanation','objection_handling','conversion','transition','incident')),
  prompt TEXT NOT NULL,
  real_answer TEXT NOT NULL,
  clip_path TEXT NOT NULL,
  evidence_id TEXT NOT NULL UNIQUE,
  fact_constraints_json TEXT NOT NULL DEFAULT '{}',
  review_status TEXT NOT NULL DEFAULT 'pending' CHECK(review_status IN ('draft','pending','approved','rejected')),
  identity_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(identity_confirmed IN (0, 1)),
  comment_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(comment_confirmed IN (0, 1)),
  answer_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(answer_confirmed IN (0, 1)),
  clip_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(clip_confirmed IN (0, 1)),
  facts_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(facts_confirmed IN (0, 1)),
  module_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(module_confirmed IN (0, 1)),
  is_active INTEGER NOT NULL DEFAULT 1 CHECK(is_active IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE training_sessions (
  session_id TEXT PRIMARY KEY,
  role_id TEXT NOT NULL REFERENCES training_roles(role_id) ON DELETE RESTRICT,
  mode TEXT NOT NULL CHECK(mode IN ('comprehensive','specialized')),
  selected_module TEXT,
  seed INTEGER NOT NULL,
  case_order_json TEXT NOT NULL,
  current_index INTEGER NOT NULL DEFAULT 0 CHECK(current_index >= 0),
  total_questions INTEGER NOT NULL CHECK(total_questions >= 0),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','completed','abandoned')),
  started_at TEXT NOT NULL,
  completed_at TEXT,
  CHECK(
    (mode = 'comprehensive' AND selected_module IS NULL) OR
    (mode = 'specialized' AND selected_module IS NOT NULL)
  )
);
CREATE TABLE training_turns (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES training_sessions(session_id) ON DELETE CASCADE,
  case_id TEXT NOT NULL REFERENCES training_cases(case_id) ON DELETE RESTRICT,
  question_index INTEGER NOT NULL CHECK(question_index >= 0),
  module_snapshot TEXT NOT NULL,
  prompt_snapshot TEXT NOT NULL,
  trainee_answer TEXT NOT NULL CHECK(trim(trainee_answer) <> ''),
  evaluation_status TEXT NOT NULL CHECK(evaluation_status IN ('scored','needs_review')),
  needs_confirmation_score INTEGER CHECK(needs_confirmation_score BETWEEN 1 AND 5),
  fact_accuracy_score INTEGER CHECK(fact_accuracy_score BETWEEN 1 AND 5),
  trust_building_score INTEGER CHECK(trust_building_score BETWEEN 1 AND 5),
  expression_clarity_score INTEGER CHECK(expression_clarity_score BETWEEN 1 AND 5),
  deal_advancement_score INTEGER CHECK(deal_advancement_score BETWEEN 1 AND 5),
  risk_compliance_score INTEGER CHECK(risk_compliance_score BETWEEN 1 AND 5),
  live_pacing_score INTEGER CHECK(live_pacing_score BETWEEN 1 AND 5),
  priority_improvement TEXT NOT NULL DEFAULT '',
  coach_suggestion TEXT NOT NULL DEFAULT '',
  real_answer_snapshot TEXT NOT NULL,
  clip_path_snapshot TEXT NOT NULL,
  evidence_id_snapshot TEXT NOT NULL,
  answered_at TEXT NOT NULL,
  UNIQUE(session_id, case_id),
  UNIQUE(session_id, question_index),
  CHECK(
    (evaluation_status = 'scored' AND
      needs_confirmation_score IS NOT NULL AND fact_accuracy_score IS NOT NULL AND
      trust_building_score IS NOT NULL AND expression_clarity_score IS NOT NULL AND
      deal_advancement_score IS NOT NULL AND risk_compliance_score IS NOT NULL AND
      live_pacing_score IS NOT NULL) OR
    (evaluation_status = 'needs_review' AND
      needs_confirmation_score IS NULL AND fact_accuracy_score IS NULL AND
      trust_building_score IS NULL AND expression_clarity_score IS NULL AND
      deal_advancement_score IS NULL AND risk_compliance_score IS NULL AND
      live_pacing_score IS NULL)
  )
);
CREATE INDEX idx_training_aliases_role ON training_role_aliases(role_id, is_confirmed);
CREATE INDEX idx_training_cases_gate ON training_cases(role_id, module, review_status, is_active);
CREATE INDEX idx_training_sessions_role ON training_sessions(role_id, started_at DESC);
CREATE INDEX idx_training_turns_session ON training_turns(session_id, question_index);

INSERT OR IGNORE INTO training_roles
  (role_id, display_name, avatar_key, is_active, unavailable_reason, created_at, updated_at)
VALUES
  ('luo-yuxin', '罗雨欣', 'luo-yuxin', 1, '', datetime('now'), datetime('now')),
  ('yu-qianhui', '于千惠', 'yu-qianhui', 1, '', datetime('now'), datetime('now')),
  ('hou-mengna', '侯梦娜', 'hou-mengna', 1, '', datetime('now'), datetime('now')),
  ('xiao-e', '小鹅', 'xiao-e', 1, '', datetime('now'), datetime('now'));
INSERT OR IGNORE INTO training_role_aliases
  (role_id, source_alias, is_confirmed, confirmed_at, created_at)
VALUES
  ('luo-yuxin', '小罗', 1, datetime('now'), datetime('now')),
  ('yu-qianhui', '于千惠', 1, datetime('now'), datetime('now')),
  ('hou-mengna', '天乐', 1, datetime('now'), datetime('now')),
  ('xiao-e', '小鹅', 1, datetime('now'), datetime('now')),
  ('xiao-e', '小鸦', 1, datetime('now'), datetime('now'));
"#;

pub const HUMAN_MACHINE_SCENARIO_MIGRATION_SQL: &str = r#"
CREATE TABLE training_scenario_runs (
  run_id TEXT PRIMARY KEY,
  role_id TEXT NOT NULL REFERENCES training_roles(role_id) ON DELETE RESTRICT,
  role_name_snapshot TEXT NOT NULL,
  case_id TEXT NOT NULL REFERENCES training_cases(case_id) ON DELETE RESTRICT,
  module_snapshot TEXT NOT NULL,
  case_snapshot_json TEXT NOT NULL,
  current_turn INTEGER NOT NULL DEFAULT 1 CHECK(current_turn BETWEEN 1 AND 3),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','completed','abandoned')),
  evaluation_status TEXT NOT NULL DEFAULT 'pending' CHECK(evaluation_status IN ('pending','scored','needs_review')),
  needs_confirmation_score INTEGER CHECK(needs_confirmation_score BETWEEN 1 AND 5),
  fact_accuracy_score INTEGER CHECK(fact_accuracy_score BETWEEN 1 AND 5),
  trust_building_score INTEGER CHECK(trust_building_score BETWEEN 1 AND 5),
  expression_clarity_score INTEGER CHECK(expression_clarity_score BETWEEN 1 AND 5),
  deal_advancement_score INTEGER CHECK(deal_advancement_score BETWEEN 1 AND 5),
  risk_compliance_score INTEGER CHECK(risk_compliance_score BETWEEN 1 AND 5),
  live_pacing_score INTEGER CHECK(live_pacing_score BETWEEN 1 AND 5),
  priority_improvement TEXT NOT NULL DEFAULT '',
  coach_suggestion TEXT NOT NULL DEFAULT '',
  hard_checks_json TEXT NOT NULL DEFAULT '[]',
  started_at TEXT NOT NULL,
  completed_at TEXT,
  updated_at TEXT NOT NULL
);
CREATE TABLE training_scenario_turns (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL REFERENCES training_scenario_runs(run_id) ON DELETE CASCADE,
  turn_index INTEGER NOT NULL CHECK(turn_index BETWEEN 1 AND 3),
  viewer_message TEXT NOT NULL CHECK(trim(viewer_message) <> ''),
  viewer_source TEXT NOT NULL CHECK(viewer_source IN ('approved_comment','ai_simulated_follow_up')),
  trainee_answer TEXT,
  answered_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(run_id, turn_index),
  CHECK((trainee_answer IS NULL AND answered_at IS NULL) OR (trim(trainee_answer) <> '' AND answered_at IS NOT NULL))
);
CREATE INDEX idx_training_scenario_runs_role ON training_scenario_runs(role_id, started_at DESC);
CREATE INDEX idx_training_scenario_turns_run ON training_scenario_turns(run_id, turn_index);
"#;

pub const HUMAN_MACHINE_ADAPTIVE_TURNS_MIGRATION_SQL: &str = r#"
CREATE TABLE training_scenario_runs_v2 (
  run_id TEXT PRIMARY KEY,
  role_id TEXT NOT NULL REFERENCES training_roles(role_id) ON DELETE RESTRICT,
  role_name_snapshot TEXT NOT NULL,
  case_id TEXT NOT NULL REFERENCES training_cases(case_id) ON DELETE RESTRICT,
  module_snapshot TEXT NOT NULL,
  case_snapshot_json TEXT NOT NULL,
  current_turn INTEGER NOT NULL DEFAULT 1 CHECK(current_turn BETWEEN 1 AND 5),
  total_turns INTEGER NOT NULL DEFAULT 3 CHECK(total_turns IN (3, 5)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','completed','abandoned')),
  evaluation_status TEXT NOT NULL DEFAULT 'pending' CHECK(evaluation_status IN ('pending','scored','needs_review')),
  needs_confirmation_score INTEGER CHECK(needs_confirmation_score BETWEEN 1 AND 5),
  fact_accuracy_score INTEGER CHECK(fact_accuracy_score BETWEEN 1 AND 5),
  trust_building_score INTEGER CHECK(trust_building_score BETWEEN 1 AND 5),
  expression_clarity_score INTEGER CHECK(expression_clarity_score BETWEEN 1 AND 5),
  deal_advancement_score INTEGER CHECK(deal_advancement_score BETWEEN 1 AND 5),
  risk_compliance_score INTEGER CHECK(risk_compliance_score BETWEEN 1 AND 5),
  live_pacing_score INTEGER CHECK(live_pacing_score BETWEEN 1 AND 5),
  priority_improvement TEXT NOT NULL DEFAULT '',
  coach_suggestion TEXT NOT NULL DEFAULT '',
  hard_checks_json TEXT NOT NULL DEFAULT '[]',
  started_at TEXT NOT NULL,
  completed_at TEXT,
  updated_at TEXT NOT NULL,
  CHECK(current_turn <= total_turns)
);
CREATE TABLE training_scenario_turns_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL REFERENCES training_scenario_runs_v2(run_id) ON DELETE CASCADE,
  turn_index INTEGER NOT NULL CHECK(turn_index BETWEEN 1 AND 5),
  viewer_message TEXT NOT NULL CHECK(trim(viewer_message) <> ''),
  viewer_source TEXT NOT NULL CHECK(viewer_source IN ('approved_comment','ai_simulated_follow_up')),
  follow_up_focus TEXT CHECK(follow_up_focus IN ('adaptive_follow_up','needs_confirmation','fact_clarification','trust_building','objection_resolution','deal_advancement','risk_compliance','live_pacing')),
  trainee_answer TEXT,
  answered_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(run_id, turn_index),
  CHECK((trainee_answer IS NULL AND answered_at IS NULL) OR (trim(trainee_answer) <> '' AND answered_at IS NOT NULL)),
  CHECK((viewer_source = 'approved_comment' AND turn_index = 1 AND follow_up_focus IS NULL) OR (viewer_source = 'ai_simulated_follow_up' AND turn_index > 1 AND follow_up_focus IS NOT NULL))
);
INSERT INTO training_scenario_runs_v2
  (run_id, role_id, role_name_snapshot, case_id, module_snapshot, case_snapshot_json,
   current_turn, total_turns, status, evaluation_status, needs_confirmation_score,
   fact_accuracy_score, trust_building_score, expression_clarity_score,
   deal_advancement_score, risk_compliance_score, live_pacing_score,
   priority_improvement, coach_suggestion, hard_checks_json, started_at,
   completed_at, updated_at)
SELECT run_id, role_id, role_name_snapshot, case_id, module_snapshot, case_snapshot_json,
       current_turn, 3, status, evaluation_status, needs_confirmation_score,
       fact_accuracy_score, trust_building_score, expression_clarity_score,
       deal_advancement_score, risk_compliance_score, live_pacing_score,
       priority_improvement, coach_suggestion, hard_checks_json, started_at,
       completed_at, updated_at
FROM training_scenario_runs;
INSERT INTO training_scenario_turns_v2
  (id, run_id, turn_index, viewer_message, viewer_source, follow_up_focus,
   trainee_answer, answered_at, created_at)
SELECT id, run_id, turn_index, viewer_message, viewer_source,
       CASE WHEN viewer_source = 'ai_simulated_follow_up' THEN 'adaptive_follow_up' ELSE NULL END,
       trainee_answer, answered_at, created_at
FROM training_scenario_turns;
DROP TABLE training_scenario_turns;
DROP TABLE training_scenario_runs;
ALTER TABLE training_scenario_runs_v2 RENAME TO training_scenario_runs;
ALTER TABLE training_scenario_turns_v2 RENAME TO training_scenario_turns;
CREATE INDEX idx_training_scenario_runs_role ON training_scenario_runs(role_id, started_at DESC);
CREATE INDEX idx_training_scenario_turns_run ON training_scenario_turns(run_id, turn_index);
"#;

pub const TRAINING_MODULES: [&str; 8] = [
    "opening",
    "retention_interaction",
    "needs_discovery",
    "product_explanation",
    "objection_handling",
    "conversion",
    "transition",
    "incident",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TrainingScores {
    pub needs_confirmation: i64,
    pub fact_accuracy: i64,
    pub trust_building: i64,
    pub expression_clarity: i64,
    pub deal_advancement: i64,
    pub risk_compliance: i64,
    pub live_pacing: i64,
}

impl TrainingScores {
    pub fn is_valid(&self) -> bool {
        [
            self.needs_confirmation,
            self.fact_accuracy,
            self.trust_building,
            self.expression_clarity,
            self.deal_advancement,
            self.risk_compliance,
            self.live_pacing,
        ]
        .into_iter()
        .all(|score| (1..=5).contains(&score))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingRoleSummary {
    pub role_id: String,
    pub display_name: String,
    pub avatar_key: String,
    pub source_aliases: Vec<String>,
    pub approved_case_count: i64,
    pub module_counts: BTreeMap<String, i64>,
    pub unavailable_reason: Option<String>,
    pub available_case_count: i64,
    pub available_modules: Vec<String>,
    pub evidence_status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrainingQuestion {
    pub case_id: String,
    pub module: String,
    pub prompt: String,
    pub viewer_role: String,
    pub source_kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedTrainingSession {
    pub session_id: String,
    pub role_id: String,
    pub display_name: String,
    pub mode: String,
    pub selected_module: Option<String>,
    pub current_index: i64,
    pub total_questions: i64,
    pub question: TrainingQuestion,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextTrainingQuestion {
    pub session_id: String,
    pub role_id: String,
    pub display_name: String,
    pub mode: String,
    pub selected_module: Option<String>,
    pub current_index: i64,
    pub total_questions: i64,
    pub question: Option<TrainingQuestion>,
    pub is_complete: bool,
}

#[derive(Debug, Clone)]
pub struct TrainingSubmissionContext {
    pub session_id: String,
    pub question_index: i64,
    pub case_id: String,
    pub module: String,
    pub prompt: String,
    pub real_answer: String,
    pub clip_path: String,
    pub evidence_id: String,
    pub fact_constraints_json: String,
    pub case_order_json: String,
    pub clip_size_bytes: u64,
    pub clip_modified_nanos: Option<u64>,
    pub clip_content_fingerprint: u64,
    pub clip_duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct StoredTrainingEvaluation {
    pub evaluation_status: String,
    pub scores: Option<TrainingScores>,
    pub priority_dimension: Option<TrainingPriorityDimension>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrainingPriorityDimension {
    NeedsConfirmation,
    FactAccuracy,
    TrustBuilding,
    ExpressionClarity,
    DealAdvancement,
    RiskCompliance,
    LivePacing,
}

impl TrainingPriorityDimension {
    pub(crate) fn safe_feedback(self) -> (&'static str, &'static str) {
        match self {
            Self::NeedsConfirmation => (
                "需求确认",
                "AI练习建议（非主播原话）：先追问使用场景和核心需求；未确认的信息明确标记待确认。",
            ),
            Self::FactAccuracy => (
                "事实准确",
                "AI练习建议（非主播原话）：只复述已确认事实；价格、库存、赠品、售后和链接缺少依据时先标记待确认。",
            ),
            Self::TrustBuilding => (
                "信任建立",
                "AI练习建议（非主播原话）：先回应顾虑，再说明可核验依据；无法确认的内容不作保证。",
            ),
            Self::ExpressionClarity => (
                "表达清晰",
                "AI练习建议（非主播原话）：使用短句依次说明需求、已确认事实和下一步；不补充未经确认的信息。",
            ),
            Self::DealAdvancement => (
                "成交推进",
                "AI练习建议（非主播原话）：在需求和事实确认后给出一个可执行下一步，不承诺未确认的优惠或库存。",
            ),
            Self::RiskCompliance => (
                "风险合规",
                "AI练习建议（非主播原话）：遇到价格、库存、赠品、售后、链接或成色信息不足时，先说明待确认再继续。",
            ),
            Self::LivePacing => (
                "直播节奏",
                "AI练习建议（非主播原话）：先用一句话承接问题，再快速追问或标记待确认，随后回到当前讲解主线。",
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingSubmissionResponse {
    pub session_id: String,
    pub case_id: String,
    pub module: String,
    pub evaluation_status: String,
    pub scores: Option<TrainingScores>,
    pub priority_improvement: String,
    pub coach_suggestion: String,
    pub real_answer: String,
    pub clip_path: String,
    pub evidence_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingModuleScoreSummary {
    pub module: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedTrainingSession {
    pub session_id: String,
    pub role_id: String,
    pub role_name: String,
    pub mode: String,
    pub module_scores: Vec<TrainingModuleScoreSummary>,
    pub evidence_ids: Vec<String>,
    pub recurring_issues: Vec<String>,
    pub next_training_suggestions: Vec<String>,
    pub answered_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbandonedTrainingSession {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanMachineHardCheck {
    pub key: String,
    pub label: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanMachineTurn {
    pub turn_index: i64,
    pub viewer_message: String,
    pub viewer_source: String,
    pub follow_up_focus: Option<String>,
    pub trainee_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanMachineScenarioResponse {
    pub run_id: String,
    pub role_id: String,
    pub display_name: String,
    pub module: String,
    pub current_turn: i64,
    pub total_turns: i64,
    pub status: String,
    pub turns: Vec<HumanMachineTurn>,
    pub fact_constraints: serde_json::Value,
    pub hard_checks: Vec<HumanMachineHardCheck>,
    pub evaluation_status: String,
    pub scores: Option<TrainingScores>,
    pub priority_improvement: Option<String>,
    pub coach_suggestion: Option<String>,
    pub real_answer: String,
    pub clip_path: String,
    pub evidence_id: String,
}

#[derive(Debug, Clone)]
pub struct HumanMachineScenarioContext {
    pub run_id: String,
    pub role_id: String,
    pub display_name: String,
    pub current_turn: i64,
    pub total_turns: i64,
    pub module: String,
    pub prompt: String,
    pub real_answer: String,
    pub clip_path: String,
    pub evidence_id: String,
    pub fact_constraints_json: String,
    pub turns: Vec<HumanMachineTurn>,
}

#[derive(Debug, Clone, FromRow)]
struct TrainingRoleRow {
    role_id: String,
    display_name: String,
    avatar_key: String,
    unavailable_reason: String,
}

#[derive(Debug, Clone, FromRow)]
struct TrainingCaseRow {
    case_id: String,
    module: String,
    prompt: String,
    real_answer: String,
    clip_path: String,
    evidence_id: String,
    fact_constraints_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct TrainingCaseSnapshot {
    case_id: String,
    module: String,
    prompt: String,
    real_answer: String,
    clip_path: String,
    evidence_id: String,
    fact_constraints_json: String,
    clip_size_bytes: u64,
    clip_modified_nanos: Option<u64>,
    clip_content_fingerprint: u64,
    clip_duration_ms: u64,
}

#[derive(Debug, Clone)]
struct EligibleTrainingCase {
    case: TrainingCaseRow,
    clip_signature: MediaFileSignature,
}

#[derive(Debug, Clone, FromRow)]
struct TrainingSessionRow {
    session_id: String,
    role_id: String,
    display_name: String,
    mode: String,
    selected_module: Option<String>,
    case_order_json: String,
    current_index: i64,
    total_questions: i64,
    status: String,
}

#[derive(Debug, Clone, FromRow)]
struct TrainingTurnRow {
    session_id: String,
    case_id: String,
    module_snapshot: String,
    evaluation_status: String,
    needs_confirmation_score: Option<i64>,
    fact_accuracy_score: Option<i64>,
    trust_building_score: Option<i64>,
    expression_clarity_score: Option<i64>,
    deal_advancement_score: Option<i64>,
    risk_compliance_score: Option<i64>,
    live_pacing_score: Option<i64>,
    priority_improvement: String,
    coach_suggestion: String,
    real_answer_snapshot: String,
    clip_path_snapshot: String,
    evidence_id_snapshot: String,
}

#[derive(Debug, Clone, FromRow)]
struct TrainingSummaryTurnRow {
    module_snapshot: String,
    evaluation_status: String,
    needs_confirmation_score: Option<i64>,
    fact_accuracy_score: Option<i64>,
    trust_building_score: Option<i64>,
    expression_clarity_score: Option<i64>,
    deal_advancement_score: Option<i64>,
    risk_compliance_score: Option<i64>,
    live_pacing_score: Option<i64>,
    priority_improvement: String,
    coach_suggestion: String,
    evidence_id_snapshot: String,
}

#[derive(Debug, Clone, FromRow)]
struct HumanMachineRunRow {
    run_id: String,
    role_id: String,
    role_name_snapshot: String,
    case_id: String,
    module_snapshot: String,
    case_snapshot_json: String,
    current_turn: i64,
    total_turns: i64,
    status: String,
    evaluation_status: String,
    needs_confirmation_score: Option<i64>,
    fact_accuracy_score: Option<i64>,
    trust_building_score: Option<i64>,
    expression_clarity_score: Option<i64>,
    deal_advancement_score: Option<i64>,
    risk_compliance_score: Option<i64>,
    live_pacing_score: Option<i64>,
    priority_improvement: String,
    coach_suggestion: String,
    hard_checks_json: String,
}

#[derive(Debug, Clone, FromRow)]
struct HumanMachineTurnRow {
    turn_index: i64,
    viewer_message: String,
    viewer_source: String,
    follow_up_focus: Option<String>,
    trainee_answer: Option<String>,
}

fn training_error(message: impl Into<String>) -> DatabaseError {
    DatabaseError::InvalidTrainingState(message.into())
}

fn valid_module(module: &str) -> bool {
    TRAINING_MODULES.contains(&module)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MediaFileSignature {
    size_bytes: u64,
    modified_nanos: Option<u64>,
    content_fingerprint: u64,
    duration_ms: u64,
}

fn update_media_fingerprint(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn media_file_identity(clip_path: &Path) -> Option<MediaFileSignature> {
    if !clip_path.is_absolute() || !clip_path.is_file() {
        return None;
    }
    let extension = clip_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    let Some(extension) = extension else {
        return None;
    };
    let metadata = match std::fs::metadata(clip_path) {
        Ok(metadata) if metadata.len() >= 32 => metadata,
        _ => return None,
    };
    let mut file = match std::fs::File::open(clip_path) {
        Ok(file) => file,
        Err(_) => return None,
    };
    let mut header = [0_u8; 4096];
    let read = match file.read(&mut header) {
        Ok(read) if read >= 4 => read,
        _ => return None,
    };
    let header_bytes = &header[..read];
    let recognizable = match extension.as_str() {
        "mp4" | "mov" | "m4v" => read >= 12 && &header_bytes[4..8] == b"ftyp",
        "mkv" | "webm" => header_bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]),
        "ts" => metadata.len() >= 188 && header_bytes.first() == Some(&0x47),
        "m3u8" => std::str::from_utf8(header_bytes).is_ok_and(|text| {
            text.trim_start().starts_with("#EXTM3U")
                && (text.contains("#EXTINF") || text.contains("#EXT-X-STREAM-INF"))
        }),
        _ => false,
    };
    if !recognizable {
        return None;
    }
    let mut content_fingerprint = update_media_fingerprint(0xcbf2_9ce4_8422_2325, header_bytes);
    loop {
        let read = file.read(&mut header).ok()?;
        if read == 0 {
            break;
        }
        content_fingerprint = update_media_fingerprint(content_fingerprint, &header[..read]);
    }
    Some(MediaFileSignature {
        size_bytes: metadata.len(),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok()),
        content_fingerprint,
        duration_ms: 0,
    })
}

#[cfg(not(test))]
async fn probe_playable_video_duration_ms(clip_path: &Path) -> Option<u64> {
    let (duration, metadata) = tokio::join!(
        crate::ffmpeg::probe_media_duration_ms(clip_path),
        crate::ffmpeg::extract_video_metadata(clip_path)
    );
    let duration = duration.ok()?;
    let metadata = metadata.ok()?;
    (duration > 0
        && metadata.width > 0
        && metadata.height > 0
        && !metadata.video_codec.trim().is_empty())
    .then_some(duration)
}

#[cfg(test)]
async fn probe_playable_video_duration_ms(clip_path: &Path) -> Option<u64> {
    let mut ffprobe = Path::new(env!("CARGO_MANIFEST_DIR")).join("ffprobe");
    if cfg!(windows) {
        ffprobe.set_extension("exe");
    }
    if !ffprobe.is_file() {
        ffprobe = Path::new(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        })
        .to_path_buf();
    }
    let output = tokio::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(clip_path)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let duration_seconds = payload
        .get("format")?
        .get("duration")?
        .as_str()?
        .parse::<f64>()
        .ok()?;
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return None;
    }
    let has_video = payload.get("streams")?.as_array()?.iter().any(|stream| {
        stream.get("codec_type").and_then(|value| value.as_str()) == Some("video")
            && stream
                .get("width")
                .and_then(|value| value.as_u64())
                .is_some_and(|width| width > 0)
            && stream
                .get("height")
                .and_then(|value| value.as_u64())
                .is_some_and(|height| height > 0)
    });
    has_video.then_some((duration_seconds * 1000.0).ceil() as u64)
}

async fn media_file_signature(clip_path: &Path) -> Option<MediaFileSignature> {
    let mut signature = media_file_identity(clip_path)?;
    signature.duration_ms = probe_playable_video_duration_ms(clip_path).await?;
    Some(signature)
}

async fn case_runtime_signature(case: &TrainingCaseRow) -> Option<MediaFileSignature> {
    let facts_are_structured =
        serde_json::from_str::<serde_json::Value>(&case.fact_constraints_json)
            .is_ok_and(|value| value.is_object());
    if case.prompt.trim().is_empty() {
        return None;
    }
    if case.real_answer.trim().is_empty()
        || case.evidence_id.trim().is_empty()
        || !facts_are_structured
    {
        return None;
    }
    media_file_signature(Path::new(case.clip_path.trim())).await
}

impl TrainingCaseSnapshot {
    fn from_eligible(eligible: &EligibleTrainingCase) -> Self {
        Self {
            case_id: eligible.case.case_id.clone(),
            module: eligible.case.module.clone(),
            prompt: eligible.case.prompt.clone(),
            real_answer: eligible.case.real_answer.clone(),
            clip_path: eligible.case.clip_path.clone(),
            evidence_id: eligible.case.evidence_id.clone(),
            fact_constraints_json: eligible.case.fact_constraints_json.clone(),
            clip_size_bytes: eligible.clip_signature.size_bytes,
            clip_modified_nanos: eligible.clip_signature.modified_nanos,
            clip_content_fingerprint: eligible.clip_signature.content_fingerprint,
            clip_duration_ms: eligible.clip_signature.duration_ms,
        }
    }

    fn media_signature(&self) -> MediaFileSignature {
        MediaFileSignature {
            size_bytes: self.clip_size_bytes,
            modified_nanos: self.clip_modified_nanos,
            content_fingerprint: self.clip_content_fingerprint,
            duration_ms: self.clip_duration_ms,
        }
    }

    fn is_well_formed(&self) -> bool {
        valid_module(&self.module)
            && !self.case_id.trim().is_empty()
            && !self.prompt.trim().is_empty()
            && !self.real_answer.trim().is_empty()
            && !self.clip_path.trim().is_empty()
            && !self.evidence_id.trim().is_empty()
            && self.clip_size_bytes > 0
            && self.clip_duration_ms > 0
            && serde_json::from_str::<serde_json::Value>(&self.fact_constraints_json)
                .is_ok_and(|value| value.is_object())
    }
}

fn safe_question(case: &TrainingCaseSnapshot) -> TrainingQuestion {
    TrainingQuestion {
        case_id: case.case_id.clone(),
        module: case.module.clone(),
        prompt: case.prompt.clone(),
        viewer_role: training_viewer_role(&case.fact_constraints_json),
        source_kind: "approved_comment".to_string(),
    }
}

fn training_viewer_role(fact_constraints_json: &str) -> String {
    let value = serde_json::from_str::<serde_json::Value>(fact_constraints_json).ok();
    let role = value
        .as_ref()
        .and_then(|value| value.as_object())
        .and_then(|object| {
            object
                .get("viewerRole")
                .or_else(|| object.get("viewer_role"))
        })
        .and_then(|value| value.as_str())
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|value| !value.is_empty() && value.chars().count() <= 40);
    role.unwrap_or_else(|| "直播间观众".to_string())
}

fn parse_case_order(raw: &str) -> Result<Vec<TrainingCaseSnapshot>, DatabaseError> {
    let snapshots: Vec<TrainingCaseSnapshot> = serde_json::from_str(raw)
        .map_err(|_| training_error("训练会话缺少安全案例快照，请重新开始"))?;
    let mut case_ids = HashSet::new();
    if snapshots.is_empty()
        || snapshots
            .iter()
            .any(|snapshot| !snapshot.is_well_formed() || !case_ids.insert(&snapshot.case_id))
    {
        return Err(training_error("训练会话案例快照损坏，请重新开始"));
    }
    Ok(snapshots)
}

fn context_matches_snapshot(
    context: &TrainingSubmissionContext,
    snapshot: &TrainingCaseSnapshot,
) -> bool {
    context.case_id == snapshot.case_id
        && context.module == snapshot.module
        && context.prompt == snapshot.prompt
        && context.real_answer == snapshot.real_answer
        && context.clip_path == snapshot.clip_path
        && context.evidence_id == snapshot.evidence_id
        && context.fact_constraints_json == snapshot.fact_constraints_json
        && context.clip_size_bytes == snapshot.clip_size_bytes
        && context.clip_modified_nanos == snapshot.clip_modified_nanos
        && context.clip_content_fingerprint == snapshot.clip_content_fingerprint
        && context.clip_duration_ms == snapshot.clip_duration_ms
}

fn deterministic_shuffle<T>(items: &mut [T], seed: i64) {
    if items.len() < 2 {
        return;
    }
    let mut state = (seed as u64) ^ 0x9e37_79b9_7f4a_7c15;
    for index in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let swap_with = (state as usize) % (index + 1);
        items.swap(index, swap_with);
    }
}

impl Database {
    async fn eligible_training_cases(
        &self,
        role_id: &str,
        selected_module: Option<&str>,
    ) -> Result<Vec<EligibleTrainingCase>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let rows = sqlx::query_as::<_, TrainingCaseRow>(
            r#"SELECT case_id, module, prompt, real_answer, clip_path, evidence_id,
                      fact_constraints_json
               FROM training_cases
               WHERE role_id = $1
                 AND ($2 IS NULL OR module = $2)
                 AND review_status = 'approved'
                 AND identity_confirmed = 1
                 AND comment_confirmed = 1
                 AND answer_confirmed = 1
                 AND clip_confirmed = 1
                 AND facts_confirmed = 1
                 AND module_confirmed = 1
                 AND is_active = 1
                 AND trim(prompt) <> ''
                 AND trim(real_answer) <> ''
                 AND trim(clip_path) <> ''
                 AND trim(evidence_id) <> ''
               ORDER BY module ASC, case_id ASC"#,
        )
        .bind(role_id)
        .bind(selected_module)
        .fetch_all(&pool)
        .await?;
        let mut eligible = Vec::with_capacity(rows.len());
        for case in rows {
            if let Some(clip_signature) = case_runtime_signature(&case).await {
                eligible.push(EligibleTrainingCase {
                    case,
                    clip_signature,
                });
            }
        }
        Ok(eligible)
    }

    async fn training_session(
        &self,
        session_id: &str,
    ) -> Result<TrainingSessionRow, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query_as::<_, TrainingSessionRow>(
            r#"SELECT s.session_id, s.role_id, r.display_name, s.mode, s.selected_module,
                      s.case_order_json, s.current_index, s.total_questions, s.status
               FROM training_sessions s
               JOIN training_roles r ON r.role_id = s.role_id
               WHERE s.session_id = $1"#,
        )
        .bind(session_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| training_error("训练会话不存在"))
    }

    async fn eligible_case_by_id(
        &self,
        role_id: &str,
        case_id: &str,
        required_module: Option<&str>,
    ) -> Result<EligibleTrainingCase, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let case = sqlx::query_as::<_, TrainingCaseRow>(
            r#"SELECT case_id, module, prompt, real_answer, clip_path, evidence_id,
                      fact_constraints_json
               FROM training_cases
               WHERE role_id = $1
                 AND case_id = $2
                 AND ($3 IS NULL OR module = $3)
                 AND review_status = 'approved'
                 AND identity_confirmed = 1
                 AND comment_confirmed = 1
                 AND answer_confirmed = 1
                 AND clip_confirmed = 1
                 AND facts_confirmed = 1
                 AND module_confirmed = 1
                 AND is_active = 1
                 AND trim(prompt) <> ''
                 AND trim(real_answer) <> ''
                 AND trim(clip_path) <> ''
                 AND trim(evidence_id) <> ''"#,
        )
        .bind(role_id)
        .bind(case_id)
        .bind(required_module)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| training_error("该训练证据已不可用，请重新开始训练"))?;
        let clip_signature = case_runtime_signature(&case)
            .await
            .ok_or_else(|| training_error("该训练证据已不可用，请重新开始训练"))?;
        Ok(EligibleTrainingCase {
            case,
            clip_signature,
        })
    }

    pub async fn list_training_roles(&self) -> Result<Vec<TrainingRoleSummary>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let roles = sqlx::query_as::<_, TrainingRoleRow>(
            "SELECT role_id, display_name, avatar_key, unavailable_reason FROM training_roles WHERE is_active = 1 ORDER BY display_name ASC",
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::with_capacity(roles.len());
        for role in roles {
            let source_aliases = sqlx::query_scalar::<_, String>(
                // “小鸦” is a confirmed raw-to-canonical identity mapping for audit/import,
                // not a production-facing name. The role card must only display “小鹅”.
                "SELECT source_alias FROM training_role_aliases WHERE role_id = $1 AND is_confirmed = 1 AND source_alias <> '小鸦' ORDER BY id ASC",
            )
            .bind(&role.role_id)
            .fetch_all(&pool)
            .await?;
            let cases = self.eligible_training_cases(&role.role_id, None).await?;
            let mut module_counts = TRAINING_MODULES
                .iter()
                .map(|module| ((*module).to_string(), 0_i64))
                .collect::<BTreeMap<_, _>>();
            for case in &cases {
                *module_counts.entry(case.case.module.clone()).or_default() += 1;
            }
            let unavailable_reason = if cases.is_empty() {
                Some(if role.unavailable_reason.trim().is_empty() {
                    "暂无经审核训练证据".to_string()
                } else {
                    role.unavailable_reason.clone()
                })
            } else {
                None
            };
            result.push(TrainingRoleSummary {
                role_id: role.role_id,
                display_name: role.display_name,
                avatar_key: role.avatar_key,
                source_aliases,
                approved_case_count: cases.len() as i64,
                available_case_count: cases.len() as i64,
                available_modules: TRAINING_MODULES
                    .iter()
                    .filter(|module| module_counts.get(**module).copied().unwrap_or_default() > 0)
                    .map(|module| (*module).to_string())
                    .collect(),
                evidence_status: if cases.is_empty() {
                    "empty".to_string()
                } else {
                    "ready".to_string()
                },
                module_counts,
                unavailable_reason,
            });
        }
        Ok(result)
    }

    pub async fn start_training_session(
        &self,
        role_id: &str,
        mode: &str,
        selected_module: Option<&str>,
        seed: Option<i64>,
    ) -> Result<StartedTrainingSession, DatabaseError> {
        let role_id = role_id.trim();
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let role = sqlx::query_as::<_, TrainingRoleRow>(
            "SELECT role_id, display_name, avatar_key, unavailable_reason FROM training_roles WHERE role_id = $1 AND is_active = 1",
        )
        .bind(role_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| training_error("训练主播不存在或已停用"))?;

        let normalized_module = selected_module
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match mode {
            "comprehensive" if normalized_module.is_none() => {}
            "specialized" if normalized_module.is_some_and(valid_module) => {}
            "comprehensive" => return Err(training_error("综合训练不能指定专项板块")),
            "specialized" => return Err(training_error("专项训练必须选择有效板块")),
            _ => return Err(training_error("未知训练模式")),
        }

        let mut cases = self
            .eligible_training_cases(role_id, normalized_module)
            .await?;
        if cases.is_empty() {
            let scope = normalized_module
                .map(|module| format!("“{module}”板块"))
                .unwrap_or_else(|| "当前主播".to_string());
            return Err(training_error(format!("{scope}暂无经审核训练证据")));
        }
        let seed = seed.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        deterministic_shuffle(&mut cases, seed);
        let case_order = cases
            .iter()
            .map(TrainingCaseSnapshot::from_eligible)
            .collect::<Vec<_>>();
        let case_order_json = serde_json::to_string(&case_order)
            .map_err(|_| training_error("无法创建训练题目顺序"))?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let total_questions = cases.len() as i64;
        sqlx::query(
            r#"INSERT INTO training_sessions
               (session_id, role_id, mode, selected_module, seed, case_order_json,
                current_index, total_questions, status, started_at)
               VALUES ($1,$2,$3,$4,$5,$6,0,$7,'active',$8)"#,
        )
        .bind(&session_id)
        .bind(role_id)
        .bind(mode)
        .bind(normalized_module)
        .bind(seed)
        .bind(case_order_json)
        .bind(total_questions)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await?;

        Ok(StartedTrainingSession {
            session_id,
            role_id: role.role_id,
            display_name: role.display_name,
            mode: mode.to_string(),
            selected_module: normalized_module.map(str::to_string),
            current_index: 0,
            total_questions,
            question: safe_question(&case_order[0]),
        })
    }

    pub async fn training_submission_context(
        &self,
        session_id: &str,
    ) -> Result<TrainingSubmissionContext, DatabaseError> {
        let session = self.training_session(session_id).await?;
        if session.status != "active" {
            return Err(training_error("训练会话已经结束"));
        }
        let order = parse_case_order(&session.case_order_json)?;
        let snapshot = order
            .get(session.current_index as usize)
            .ok_or_else(|| training_error("当前训练题目不存在"))?
            .clone();
        self.eligible_case_by_id(&session.role_id, &snapshot.case_id, Some(&snapshot.module))
            .await?;
        let clip_signature = media_file_signature(Path::new(snapshot.clip_path.trim()))
            .await
            .ok_or_else(|| training_error("该训练片段已不可播放，请重新开始训练"))?;
        if clip_signature != snapshot.media_signature() {
            return Err(training_error("训练片段版本已变化，请重新开始训练"));
        }
        Ok(TrainingSubmissionContext {
            session_id: session.session_id,
            question_index: session.current_index,
            case_id: snapshot.case_id,
            module: snapshot.module,
            prompt: snapshot.prompt,
            real_answer: snapshot.real_answer,
            clip_path: snapshot.clip_path,
            evidence_id: snapshot.evidence_id,
            fact_constraints_json: snapshot.fact_constraints_json,
            case_order_json: session.case_order_json,
            clip_size_bytes: clip_signature.size_bytes,
            clip_modified_nanos: clip_signature.modified_nanos,
            clip_content_fingerprint: clip_signature.content_fingerprint,
            clip_duration_ms: clip_signature.duration_ms,
        })
    }

    pub async fn current_training_submission(
        &self,
        session_id: &str,
    ) -> Result<Option<TrainingSubmissionResponse>, DatabaseError> {
        let session = self.training_session(session_id).await?;
        let order = parse_case_order(&session.case_order_json)?;
        let Some(snapshot) = order.get(session.current_index as usize) else {
            return Ok(None);
        };
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let row = sqlx::query_as::<_, TrainingTurnRow>(
            r#"SELECT session_id, case_id, module_snapshot, evaluation_status,
                      needs_confirmation_score, fact_accuracy_score, trust_building_score,
                      expression_clarity_score, deal_advancement_score, risk_compliance_score,
                      live_pacing_score, priority_improvement, coach_suggestion,
                      real_answer_snapshot, clip_path_snapshot, evidence_id_snapshot
               FROM training_turns WHERE session_id = $1 AND case_id = $2"#,
        )
        .bind(session_id)
        .bind(&snapshot.case_id)
        .fetch_optional(&pool)
        .await?;
        Ok(row.map(turn_response))
    }

    pub async fn save_training_submission(
        &self,
        context: &TrainingSubmissionContext,
        trainee_answer: &str,
        evaluation: &StoredTrainingEvaluation,
    ) -> Result<TrainingSubmissionResponse, DatabaseError> {
        if trainee_answer.trim().is_empty() {
            return Err(training_error("训练回答不能为空"));
        }
        if evaluation.evaluation_status == "scored"
            && !evaluation
                .scores
                .as_ref()
                .is_some_and(TrainingScores::is_valid)
        {
            return Err(training_error("AI评分不完整，必须进入待复核状态"));
        }
        if evaluation.evaluation_status == "needs_review" && evaluation.scores.is_some() {
            return Err(training_error("待复核结果不得保存推测分数"));
        }
        if evaluation.evaluation_status == "scored" && evaluation.priority_dimension.is_none() {
            return Err(training_error("AI评分缺少改进维度，必须进入待复核状态"));
        }
        if evaluation.evaluation_status == "needs_review" && evaluation.priority_dimension.is_some()
        {
            return Err(training_error("待复核结果不得保存推测改进维度"));
        }
        if evaluation.evaluation_status != "scored"
            && evaluation.evaluation_status != "needs_review"
        {
            return Err(training_error("未知评分状态"));
        }
        let snapshots = parse_case_order(&context.case_order_json)?;
        let snapshot = snapshots
            .get(context.question_index as usize)
            .ok_or_else(|| training_error("训练会话案例快照损坏，请重新开始"))?;
        if !context_matches_snapshot(context, snapshot) {
            return Err(training_error(
                "训练评分上下文与用户所见题目不一致，未保存回答，也未解锁真实证据",
            ));
        }
        let current_clip_signature =
            media_file_signature(Path::new(context.clip_path.trim())).await;
        let expected_clip_signature = MediaFileSignature {
            size_bytes: context.clip_size_bytes,
            modified_nanos: context.clip_modified_nanos,
            content_fingerprint: context.clip_content_fingerprint,
            duration_ms: context.clip_duration_ms,
        };
        if current_clip_signature != Some(expected_clip_signature) {
            return Err(training_error(
                "训练片段已变化或不可播放，未保存回答，也未解锁真实证据",
            ));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let scores = evaluation.scores.as_ref();
        let (priority_improvement, coach_suggestion) = evaluation
            .priority_dimension
            .map(TrainingPriorityDimension::safe_feedback)
            .unwrap_or(("AI评分待人工复核", "AI练习建议待复核，未生成建议。"));
        let result = sqlx::query(
            r#"INSERT INTO training_turns
               (session_id, case_id, question_index, module_snapshot, prompt_snapshot,
                trainee_answer, evaluation_status, needs_confirmation_score,
                fact_accuracy_score, trust_building_score, expression_clarity_score,
                deal_advancement_score, risk_compliance_score, live_pacing_score,
                priority_improvement, coach_suggestion, real_answer_snapshot,
                clip_path_snapshot, evidence_id_snapshot, answered_at)
               SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20
               FROM training_sessions s
               JOIN training_cases c ON c.case_id = $2 AND c.role_id = s.role_id
               WHERE s.session_id = $1
                 AND s.status = 'active'
                 AND s.current_index = $3
                 AND s.current_index >= 0
                 AND s.current_index < s.total_questions
                 AND s.case_order_json = $21
                 AND json_extract(s.case_order_json, '$[' || s.current_index || '].caseId') = c.case_id
                 AND c.module = $4
                 AND c.review_status = 'approved'
                 AND c.identity_confirmed = 1
                 AND c.comment_confirmed = 1
                 AND c.answer_confirmed = 1
                 AND c.clip_confirmed = 1
                 AND c.facts_confirmed = 1
                 AND c.module_confirmed = 1
                 AND c.is_active = 1
                 AND trim(c.prompt) <> ''
                 AND trim(c.real_answer) <> ''
                 AND trim(c.clip_path) <> ''
                 AND trim(c.evidence_id) <> ''
                 AND (
                   s.mode = 'comprehensive' OR
                   (s.mode = 'specialized' AND s.selected_module = c.module)
                 )
                 AND json_valid($22)
                 AND json_type($22) = 'object'"#,
        )
        .bind(&context.session_id)
        .bind(&context.case_id)
        .bind(context.question_index)
        .bind(&context.module)
        .bind(&context.prompt)
        .bind(trainee_answer.trim())
        .bind(&evaluation.evaluation_status)
        .bind(scores.map(|scores| scores.needs_confirmation))
        .bind(scores.map(|scores| scores.fact_accuracy))
        .bind(scores.map(|scores| scores.trust_building))
        .bind(scores.map(|scores| scores.expression_clarity))
        .bind(scores.map(|scores| scores.deal_advancement))
        .bind(scores.map(|scores| scores.risk_compliance))
        .bind(scores.map(|scores| scores.live_pacing))
        .bind(priority_improvement)
        .bind(coach_suggestion)
        .bind(&context.real_answer)
        .bind(&context.clip_path)
        .bind(&context.evidence_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&context.case_order_json)
        .bind(&context.fact_constraints_json)
        .execute(&pool)
        .await;

        match result {
            Ok(done) if done.rows_affected() == 1 => {}
            Ok(_) => {
                return Err(training_error(
                    "训练证据或会话状态已变化，未保存回答，也未解锁真实证据",
                ))
            }
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                return self
                    .current_training_submission(&context.session_id)
                    .await?
                    .ok_or_else(|| training_error("该题已经提交"));
            }
            Err(error) => return Err(error.into()),
        }

        self.current_training_submission(&context.session_id)
            .await?
            .ok_or_else(|| training_error("训练回答保存失败"))
    }

    pub async fn next_training_question(
        &self,
        session_id: &str,
    ) -> Result<NextTrainingQuestion, DatabaseError> {
        let session = self.training_session(session_id).await?;
        if session.status == "abandoned" {
            return Err(training_error("训练会话已放弃"));
        }
        if session.status == "completed" || session.current_index >= session.total_questions {
            return Ok(NextTrainingQuestion {
                session_id: session.session_id,
                role_id: session.role_id,
                display_name: session.display_name,
                mode: session.mode,
                selected_module: session.selected_module,
                current_index: session.total_questions,
                total_questions: session.total_questions,
                question: None,
                is_complete: true,
            });
        }
        if self
            .current_training_submission(session_id)
            .await?
            .is_none()
        {
            return Err(training_error("请先提交当前问题的回答"));
        }

        let next_index = session.current_index + 1;
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        if next_index >= session.total_questions {
            let done = sqlx::query(
                "UPDATE training_sessions SET current_index = total_questions, status = 'completed', completed_at = $1 WHERE session_id = $2 AND status = 'active' AND current_index = $3",
            )
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(session_id)
            .bind(session.current_index)
            .execute(&pool)
            .await?;
            if done.rows_affected() != 1 {
                return Err(training_error("训练会话状态已变化，请刷新"));
            }
            return Ok(NextTrainingQuestion {
                session_id: session.session_id,
                role_id: session.role_id,
                display_name: session.display_name,
                mode: session.mode,
                selected_module: session.selected_module,
                current_index: session.total_questions,
                total_questions: session.total_questions,
                question: None,
                is_complete: true,
            });
        }

        let order = parse_case_order(&session.case_order_json)?;
        let snapshot = order
            .get(next_index as usize)
            .ok_or_else(|| training_error("下一道训练题目不存在"))?;
        self.eligible_case_by_id(&session.role_id, &snapshot.case_id, Some(&snapshot.module))
            .await?;
        let clip_signature = media_file_signature(Path::new(snapshot.clip_path.trim()))
            .await
            .ok_or_else(|| training_error("下一道训练片段已不可播放，请重新开始训练"))?;
        if clip_signature != snapshot.media_signature() {
            return Err(training_error("下一道训练片段版本已变化，请重新开始训练"));
        }
        let updated = sqlx::query(
            r#"UPDATE training_sessions
               SET current_index = $1
               WHERE session_id = $2
                 AND status = 'active'
                 AND current_index = $3
                 AND json_extract(case_order_json, '$[' || $1 || '].caseId') = $4
                 AND EXISTS (
                   SELECT 1 FROM training_cases c
                   WHERE c.case_id = $4
                     AND c.role_id = training_sessions.role_id
                     AND c.module = $5
                     AND c.review_status = 'approved'
                     AND c.identity_confirmed = 1
                     AND c.comment_confirmed = 1
                     AND c.answer_confirmed = 1
                     AND c.clip_confirmed = 1
                     AND c.facts_confirmed = 1
                     AND c.module_confirmed = 1
                     AND c.is_active = 1
                 )"#,
        )
        .bind(next_index)
        .bind(session_id)
        .bind(session.current_index)
        .bind(&snapshot.case_id)
        .bind(&snapshot.module)
        .execute(&pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(training_error("训练会话状态已变化，请刷新"));
        }
        Ok(NextTrainingQuestion {
            session_id: session.session_id,
            role_id: session.role_id,
            display_name: session.display_name,
            mode: session.mode,
            selected_module: session.selected_module,
            current_index: next_index,
            total_questions: session.total_questions,
            question: Some(safe_question(snapshot)),
            is_complete: false,
        })
    }

    pub async fn complete_training_session(
        &self,
        session_id: &str,
    ) -> Result<CompletedTrainingSession, DatabaseError> {
        let mut session = self.training_session(session_id).await?;
        if session.status == "abandoned" {
            return Err(training_error("已放弃的训练会话不能完成"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        if session.status == "active" {
            let updated = sqlx::query(
                r#"UPDATE training_sessions
                   SET status = 'completed', completed_at = $1
                   WHERE session_id = $2
                     AND status = 'active'
                     AND total_questions > 0
                     AND (
                       SELECT COUNT(*) FROM training_turns t
                       WHERE t.session_id = training_sessions.session_id
                     ) = total_questions"#,
            )
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(session_id)
            .execute(&pool)
            .await?;
            if updated.rows_affected() != 1 {
                session = self.training_session(session_id).await?;
                match session.status.as_str() {
                    "completed" => {}
                    "abandoned" => return Err(training_error("训练会话状态已变化，当前已放弃")),
                    _ => return Err(training_error("请完成并提交全部训练题目后再结束本轮训练")),
                }
            } else {
                session.status = "completed".to_string();
            }
        }
        let turns = sqlx::query_as::<_, TrainingSummaryTurnRow>(
            r#"SELECT module_snapshot, evaluation_status, needs_confirmation_score,
                      fact_accuracy_score, trust_building_score, expression_clarity_score,
                      deal_advancement_score, risk_compliance_score, live_pacing_score,
                      priority_improvement, coach_suggestion, evidence_id_snapshot
               FROM training_turns WHERE session_id = $1 ORDER BY question_index ASC"#,
        )
        .bind(session_id)
        .fetch_all(&pool)
        .await?;
        if turns.len() as i64 != session.total_questions || session.total_questions <= 0 {
            return Err(training_error(
                "训练答题记录不完整，不能生成本轮总结，请重新开始训练",
            ));
        }

        #[derive(Default)]
        struct ScoreAccumulator {
            count: i64,
            values: [i64; 7],
        }
        let mut accumulators = BTreeMap::<String, ScoreAccumulator>::new();
        let mut evidence_ids = Vec::new();
        let mut evidence_seen = HashSet::new();
        let mut issue_counts = HashMap::<String, usize>::new();
        let mut suggestions = Vec::new();
        let mut suggestion_seen = HashSet::new();
        for turn in &turns {
            if evidence_seen.insert(turn.evidence_id_snapshot.clone()) {
                evidence_ids.push(turn.evidence_id_snapshot.clone());
            }
            if turn.evaluation_status == "scored" {
                if let Some(values) = turn_scores(turn) {
                    let entry = accumulators
                        .entry(turn.module_snapshot.clone())
                        .or_default();
                    entry.count += 1;
                    for (index, value) in values.into_iter().enumerate() {
                        entry.values[index] += value;
                    }
                }
                if !turn.priority_improvement.trim().is_empty() {
                    *issue_counts
                        .entry(turn.priority_improvement.trim().to_string())
                        .or_default() += 1;
                }
                if !turn.coach_suggestion.trim().is_empty()
                    && suggestion_seen.insert(turn.coach_suggestion.trim().to_string())
                {
                    suggestions.push(turn.coach_suggestion.trim().to_string());
                }
            }
        }
        let module_scores = TRAINING_MODULES
            .iter()
            .filter_map(|module| {
                let accumulator = accumulators.get(*module)?;
                if accumulator.count == 0 {
                    return None;
                }
                let total: i64 = accumulator.values.iter().sum();
                let divisor = (accumulator.count * 7) as f64;
                Some(TrainingModuleScoreSummary {
                    module: (*module).to_string(),
                    score: ((total as f64 / divisor) * 100.0).round() / 100.0,
                })
            })
            .collect();
        let mut recurring_issues = issue_counts.into_iter().collect::<Vec<_>>();
        recurring_issues.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
        let mut recurring_issues = recurring_issues
            .into_iter()
            .take(3)
            .map(|(issue, _)| issue)
            .collect::<Vec<_>>();
        if turns
            .iter()
            .any(|turn| turn.evaluation_status == "needs_review")
        {
            recurring_issues.push("部分回答评分待人工复核".to_string());
        }
        suggestions.truncate(3);

        Ok(CompletedTrainingSession {
            session_id: session.session_id,
            role_id: session.role_id,
            role_name: session.display_name,
            mode: session.mode,
            module_scores,
            evidence_ids,
            recurring_issues,
            next_training_suggestions: suggestions,
            answered_count: turns.len() as i64,
        })
    }

    pub async fn abandon_training_session(
        &self,
        session_id: &str,
    ) -> Result<AbandonedTrainingSession, DatabaseError> {
        let session = self.training_session(session_id).await?;
        if session.status == "completed" {
            return Err(training_error("已完成的训练会话不能放弃"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        if session.status == "active" {
            let updated = sqlx::query(
                "UPDATE training_sessions SET status = 'abandoned', completed_at = $1 WHERE session_id = $2 AND status = 'active'",
            )
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(session_id)
            .execute(&pool)
            .await?;
            if updated.rows_affected() != 1 {
                let refreshed = self.training_session(session_id).await?;
                return match refreshed.status.as_str() {
                    "abandoned" => Ok(AbandonedTrainingSession {
                        session_id: refreshed.session_id,
                        status: "abandoned".to_string(),
                    }),
                    "completed" => {
                        Err(training_error("训练会话状态已变化，当前已经完成，不能放弃"))
                    }
                    _ => Err(training_error("训练会话状态已变化，请刷新")),
                };
            }
        }
        Ok(AbandonedTrainingSession {
            session_id: session.session_id,
            status: "abandoned".to_string(),
        })
    }
}

fn turn_scores(turn: &TrainingSummaryTurnRow) -> Option<[i64; 7]> {
    Some([
        turn.needs_confirmation_score?,
        turn.fact_accuracy_score?,
        turn.trust_building_score?,
        turn.expression_clarity_score?,
        turn.deal_advancement_score?,
        turn.risk_compliance_score?,
        turn.live_pacing_score?,
    ])
}

impl Database {
    async fn human_machine_run(&self, run_id: &str) -> Result<HumanMachineRunRow, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query_as::<_, HumanMachineRunRow>(
            r#"SELECT run_id, role_id, role_name_snapshot, case_id, module_snapshot,
                      case_snapshot_json, current_turn, total_turns, status, evaluation_status,
                      needs_confirmation_score, fact_accuracy_score, trust_building_score,
                      expression_clarity_score, deal_advancement_score, risk_compliance_score,
                      live_pacing_score, priority_improvement, coach_suggestion, hard_checks_json
               FROM training_scenario_runs WHERE run_id = $1"#,
        )
        .bind(run_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| training_error("人机情景训练会话不存在"))
    }

    async fn human_machine_turns(
        &self,
        run_id: &str,
    ) -> Result<Vec<HumanMachineTurn>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let rows = sqlx::query_as::<_, HumanMachineTurnRow>(
            r#"SELECT turn_index, viewer_message, viewer_source, follow_up_focus, trainee_answer
               FROM training_scenario_turns WHERE run_id = $1 ORDER BY turn_index ASC"#,
        )
        .bind(run_id)
        .fetch_all(&pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| HumanMachineTurn {
                turn_index: row.turn_index,
                viewer_message: row.viewer_message,
                viewer_source: row.viewer_source,
                follow_up_focus: row.follow_up_focus,
                trainee_answer: row.trainee_answer,
            })
            .collect())
    }

    pub async fn start_human_machine_scenario(
        &self,
        role_id: &str,
        selected_module: Option<&str>,
        seed: Option<i64>,
        total_turns: i64,
    ) -> Result<HumanMachineScenarioResponse, DatabaseError> {
        let role_id = role_id.trim();
        let selected_module = selected_module
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if selected_module.is_some_and(|module| !valid_module(module)) {
            return Err(training_error("人机训练板块无效"));
        }
        if !matches!(total_turns, 3 | 5) {
            return Err(training_error("人机训练轮数只能选择 3 轮或 5 轮"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let role = sqlx::query_as::<_, TrainingRoleRow>(
            "SELECT role_id, display_name, avatar_key, unavailable_reason FROM training_roles WHERE role_id = $1 AND is_active = 1",
        )
        .bind(role_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| training_error("训练主播不存在或已停用"))?;
        let mut cases = self
            .eligible_training_cases(role_id, selected_module)
            .await?;
        if cases.is_empty() {
            return Err(training_error("所选主播或板块暂无可用于人机训练的真实案例"));
        }
        deterministic_shuffle(
            &mut cases,
            seed.unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
        );
        let snapshot = TrainingCaseSnapshot::from_eligible(&cases[0]);
        let snapshot_json = serde_json::to_string(&snapshot)
            .map_err(|_| training_error("无法创建人机训练安全快照"))?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let mut transaction = pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO training_scenario_runs
               (run_id, role_id, role_name_snapshot, case_id, module_snapshot,
                case_snapshot_json, current_turn, total_turns, status, evaluation_status,
                started_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,1,$7,'active','pending',$8,$8)"#,
        )
        .bind(&run_id)
        .bind(&role.role_id)
        .bind(&role.display_name)
        .bind(&snapshot.case_id)
        .bind(&snapshot.module)
        .bind(snapshot_json)
        .bind(total_turns)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO training_scenario_turns
               (run_id, turn_index, viewer_message, viewer_source, created_at)
               VALUES ($1,1,$2,'approved_comment',$3)"#,
        )
        .bind(&run_id)
        .bind(&snapshot.prompt)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.human_machine_scenario(&run_id).await
    }

    pub async fn human_machine_scenario(
        &self,
        run_id: &str,
    ) -> Result<HumanMachineScenarioResponse, DatabaseError> {
        let run = self.human_machine_run(run_id).await?;
        let snapshot: TrainingCaseSnapshot = serde_json::from_str(&run.case_snapshot_json)
            .map_err(|_| training_error("人机训练安全快照损坏，请重新开始"))?;
        if !snapshot.is_well_formed()
            || snapshot.case_id != run.case_id
            || snapshot.module != run.module_snapshot
        {
            return Err(training_error("人机训练安全快照不一致，请重新开始"));
        }
        if run.status == "active" {
            let eligible = self
                .eligible_case_by_id(&run.role_id, &run.case_id, Some(&run.module_snapshot))
                .await?;
            if TrainingCaseSnapshot::from_eligible(&eligible) != snapshot {
                return Err(training_error(
                    "真实案例或视频版本已变化，请重新开始人机训练",
                ));
            }
        }
        let turns = self.human_machine_turns(run_id).await?;
        if turns.is_empty()
            || turns[0].turn_index != 1
            || turns[0].viewer_source != "approved_comment"
            || turns
                .iter()
                .skip(1)
                .any(|turn| turn.viewer_source != "ai_simulated_follow_up")
        {
            return Err(training_error("人机训练对话记录不完整，请重新开始"));
        }
        let fact_constraints = serde_json::from_str(&snapshot.fact_constraints_json)
            .map_err(|_| training_error("真实案例事实边界损坏，请重新开始"))?;
        let hard_checks = serde_json::from_str(&run.hard_checks_json)
            .map_err(|_| training_error("硬规则检查结果损坏"))?;
        let scores = if run.evaluation_status == "scored" {
            let scores = TrainingScores {
                needs_confirmation: run.needs_confirmation_score.unwrap_or_default(),
                fact_accuracy: run.fact_accuracy_score.unwrap_or_default(),
                trust_building: run.trust_building_score.unwrap_or_default(),
                expression_clarity: run.expression_clarity_score.unwrap_or_default(),
                deal_advancement: run.deal_advancement_score.unwrap_or_default(),
                risk_compliance: run.risk_compliance_score.unwrap_or_default(),
                live_pacing: run.live_pacing_score.unwrap_or_default(),
            };
            if !scores.is_valid() {
                return Err(training_error("人机训练七维评分损坏"));
            }
            Some(scores)
        } else {
            None
        };
        Ok(HumanMachineScenarioResponse {
            run_id: run.run_id,
            role_id: run.role_id,
            display_name: run.role_name_snapshot,
            module: run.module_snapshot,
            current_turn: run.current_turn,
            total_turns: run.total_turns,
            status: run.status,
            turns,
            fact_constraints,
            hard_checks,
            evaluation_status: run.evaluation_status,
            scores,
            priority_improvement: (!run.priority_improvement.trim().is_empty())
                .then_some(run.priority_improvement),
            coach_suggestion: (!run.coach_suggestion.trim().is_empty())
                .then_some(run.coach_suggestion),
            real_answer: snapshot.real_answer,
            clip_path: snapshot.clip_path,
            evidence_id: snapshot.evidence_id,
        })
    }

    pub async fn save_human_machine_answer(
        &self,
        run_id: &str,
        trainee_answer: &str,
    ) -> Result<HumanMachineScenarioContext, DatabaseError> {
        let answer = trainee_answer.trim();
        if answer.is_empty() {
            return Err(training_error("训练回答不能为空"));
        }
        let run = self.human_machine_run(run_id).await?;
        if run.status != "active" {
            return Err(training_error("人机训练会话已经结束"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let existing = sqlx::query_scalar::<_, Option<String>>(
            "SELECT trainee_answer FROM training_scenario_turns WHERE run_id = $1 AND turn_index = $2",
        )
        .bind(run_id)
        .bind(run.current_turn)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| training_error("当前人机训练轮次不存在"))?;
        if let Some(existing) = existing {
            if existing != answer {
                return Err(training_error("本轮回答已提交，不能覆盖原回答"));
            }
        } else {
            let now = chrono::Utc::now().to_rfc3339();
            let updated = sqlx::query(
                r#"UPDATE training_scenario_turns
                   SET trainee_answer = $1, answered_at = $2
                   WHERE run_id = $3 AND turn_index = $4 AND trainee_answer IS NULL"#,
            )
            .bind(answer)
            .bind(&now)
            .bind(run_id)
            .bind(run.current_turn)
            .execute(&pool)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(training_error("人机训练轮次状态已变化，请重试"));
            }
            sqlx::query("UPDATE training_scenario_runs SET updated_at = $1 WHERE run_id = $2")
                .bind(now)
                .bind(run_id)
                .execute(&pool)
                .await?;
        }
        self.human_machine_context(run_id).await
    }

    pub async fn human_machine_context(
        &self,
        run_id: &str,
    ) -> Result<HumanMachineScenarioContext, DatabaseError> {
        let run = self.human_machine_run(run_id).await?;
        if run.status != "active" {
            return Err(training_error("人机训练会话已经结束"));
        }
        let snapshot: TrainingCaseSnapshot = serde_json::from_str(&run.case_snapshot_json)
            .map_err(|_| training_error("人机训练安全快照损坏，请重新开始"))?;
        let eligible = self
            .eligible_case_by_id(&run.role_id, &run.case_id, Some(&run.module_snapshot))
            .await?;
        if TrainingCaseSnapshot::from_eligible(&eligible) != snapshot {
            return Err(training_error(
                "真实案例或视频版本已变化，请重新开始人机训练",
            ));
        }
        Ok(HumanMachineScenarioContext {
            run_id: run.run_id,
            role_id: run.role_id,
            display_name: run.role_name_snapshot,
            current_turn: run.current_turn,
            total_turns: run.total_turns,
            module: snapshot.module,
            prompt: snapshot.prompt,
            real_answer: snapshot.real_answer,
            clip_path: snapshot.clip_path,
            evidence_id: snapshot.evidence_id,
            fact_constraints_json: snapshot.fact_constraints_json,
            turns: self.human_machine_turns(run_id).await?,
        })
    }

    pub async fn add_human_machine_follow_up(
        &self,
        run_id: &str,
        viewer_message: &str,
        follow_up_focus: &str,
    ) -> Result<HumanMachineScenarioResponse, DatabaseError> {
        let message = viewer_message.trim();
        if message.is_empty() || message.chars().count() > 240 {
            return Err(training_error("AI模拟追问长度无效"));
        }
        let run = self.human_machine_run(run_id).await?;
        let allowed_focuses = [
            "adaptive_follow_up",
            "needs_confirmation",
            "fact_clarification",
            "trust_building",
            "objection_resolution",
            "deal_advancement",
            "risk_compliance",
            "live_pacing",
        ];
        if !allowed_focuses.contains(&follow_up_focus) {
            return Err(training_error("AI模拟追问方向无效"));
        }
        if run.status != "active" || run.current_turn >= run.total_turns {
            return Err(training_error("当前会话不能继续生成追问"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let answered = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_scenario_turns WHERE run_id = $1 AND turn_index = $2 AND trainee_answer IS NOT NULL",
        )
        .bind(run_id)
        .bind(run.current_turn)
        .fetch_one(&pool)
        .await?;
        if answered != 1 {
            return Err(training_error("请先提交当前轮回答"));
        }
        let next_turn = run.current_turn + 1;
        let now = chrono::Utc::now().to_rfc3339();
        let mut transaction = pool.begin().await?;
        sqlx::query(
            r#"INSERT OR IGNORE INTO training_scenario_turns
               (run_id, turn_index, viewer_message, viewer_source, follow_up_focus, created_at)
               VALUES ($1,$2,$3,'ai_simulated_follow_up',$4,$5)"#,
        )
        .bind(run_id)
        .bind(next_turn)
        .bind(message)
        .bind(follow_up_focus)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE training_scenario_runs SET current_turn = $1, updated_at = $2 WHERE run_id = $3 AND status = 'active' AND current_turn = $4",
        )
        .bind(next_turn)
        .bind(&now)
        .bind(run_id)
        .bind(run.current_turn)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.human_machine_scenario(run_id).await
    }

    pub async fn complete_human_machine_scenario(
        &self,
        run_id: &str,
        evaluation: &StoredTrainingEvaluation,
        hard_checks: &[HumanMachineHardCheck],
    ) -> Result<HumanMachineScenarioResponse, DatabaseError> {
        let run = self.human_machine_run(run_id).await?;
        if run.status == "completed" {
            return self.human_machine_scenario(run_id).await;
        }
        if run.status != "active" || run.current_turn != run.total_turns {
            return Err(training_error("连续对话未完成，不能生成结果"));
        }
        if hard_checks.len() != 3 {
            return Err(training_error("硬规则检查不完整"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let answered = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_scenario_turns WHERE run_id = $1 AND trainee_answer IS NOT NULL",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await?;
        if answered != run.total_turns {
            return Err(training_error("连续对话记录不完整"));
        }
        if evaluation.evaluation_status == "scored"
            && !evaluation
                .scores
                .as_ref()
                .is_some_and(TrainingScores::is_valid)
        {
            return Err(training_error("人机训练评分不完整"));
        }
        let (priority, suggestion) = evaluation
            .priority_dimension
            .map(TrainingPriorityDimension::safe_feedback)
            .unwrap_or(("AI评分待人工复核", "AI练习建议待复核，未生成建议。"));
        let scores = evaluation.scores.as_ref();
        let hard_checks_json =
            serde_json::to_string(hard_checks).map_err(|_| training_error("无法保存硬规则检查"))?;
        let now = chrono::Utc::now().to_rfc3339();
        let updated = sqlx::query(
            r#"UPDATE training_scenario_runs SET
                 status = 'completed', evaluation_status = $1,
                 needs_confirmation_score = $2, fact_accuracy_score = $3,
                 trust_building_score = $4, expression_clarity_score = $5,
                 deal_advancement_score = $6, risk_compliance_score = $7,
                 live_pacing_score = $8, priority_improvement = $9,
                 coach_suggestion = $10, hard_checks_json = $11,
                 completed_at = $12, updated_at = $12
               WHERE run_id = $13 AND status = 'active'"#,
        )
        .bind(&evaluation.evaluation_status)
        .bind(scores.map(|value| value.needs_confirmation))
        .bind(scores.map(|value| value.fact_accuracy))
        .bind(scores.map(|value| value.trust_building))
        .bind(scores.map(|value| value.expression_clarity))
        .bind(scores.map(|value| value.deal_advancement))
        .bind(scores.map(|value| value.risk_compliance))
        .bind(scores.map(|value| value.live_pacing))
        .bind(priority)
        .bind(suggestion)
        .bind(hard_checks_json)
        .bind(now)
        .bind(run_id)
        .execute(&pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(training_error("人机训练状态已变化，请刷新"));
        }
        self.human_machine_scenario(run_id).await
    }

    pub async fn active_human_machine_scenario(
        &self,
        role_id: &str,
    ) -> Result<Option<HumanMachineScenarioResponse>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let run_id = sqlx::query_scalar::<_, String>(
            r#"SELECT run_id FROM training_scenario_runs
               WHERE role_id = $1 AND status = 'active'
               ORDER BY started_at DESC LIMIT 1"#,
        )
        .bind(role_id.trim())
        .fetch_optional(&pool)
        .await?;
        match run_id {
            Some(run_id) => self.human_machine_scenario(&run_id).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn abandon_human_machine_scenario(
        &self,
        run_id: &str,
    ) -> Result<AbandonedTrainingSession, DatabaseError> {
        let run = self.human_machine_run(run_id).await?;
        if run.status == "completed" {
            return Err(training_error("已完成的人机训练不能放弃"));
        }
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        if run.status == "active" {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE training_scenario_runs SET status = 'abandoned', completed_at = $1, updated_at = $1 WHERE run_id = $2 AND status = 'active'",
            )
            .bind(now)
            .bind(run_id)
            .execute(&pool)
            .await?;
        }
        Ok(AbandonedTrainingSession {
            session_id: run.run_id,
            status: "abandoned".to_string(),
        })
    }
}

fn turn_response(turn: TrainingTurnRow) -> TrainingSubmissionResponse {
    let scores = if turn.evaluation_status == "scored" {
        Some(TrainingScores {
            needs_confirmation: turn.needs_confirmation_score.unwrap_or_default(),
            fact_accuracy: turn.fact_accuracy_score.unwrap_or_default(),
            trust_building: turn.trust_building_score.unwrap_or_default(),
            expression_clarity: turn.expression_clarity_score.unwrap_or_default(),
            deal_advancement: turn.deal_advancement_score.unwrap_or_default(),
            risk_compliance: turn.risk_compliance_score.unwrap_or_default(),
            live_pacing: turn.live_pacing_score.unwrap_or_default(),
        })
    } else {
        None
    };
    TrainingSubmissionResponse {
        session_id: turn.session_id,
        case_id: turn.case_id,
        module: turn.module_snapshot,
        evaluation_status: turn.evaluation_status,
        scores,
        priority_improvement: turn.priority_improvement,
        coach_suggestion: turn.coach_suggestion,
        real_answer: turn.real_answer_snapshot,
        clip_path: turn.clip_path_snapshot,
        evidence_id: turn.evidence_id_snapshot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    async fn test_database() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        pool.execute(TRAINING_MIGRATION_SQL).await.unwrap();
        pool.execute(HUMAN_MACHINE_SCENARIO_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(HUMAN_MACHINE_ADAPTIVE_TURNS_MIGRATION_SQL)
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    fn temp_clip(label: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "bsr-training-{label}-{}.webm",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &path,
            include_bytes!("../../../tests/fixtures/media/evidence-1s.webm"),
        )
        .unwrap();
        path.to_string_lossy().into_owned()
    }

    fn temp_invalid_media(label: &str, extension: &str, bytes: &[u8]) -> String {
        let path = std::env::temp_dir().join(format!(
            "bsr-training-{label}-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        ));
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    async fn insert_case(
        database: &Database,
        case_id: &str,
        role_id: &str,
        module: &str,
        clip_path: &str,
        approved: bool,
    ) {
        let pool = database.db.read().await.clone().unwrap();
        sqlx::query(
            r#"INSERT INTO training_cases
               (case_id, role_id, module, prompt, real_answer, clip_path, evidence_id,
                fact_constraints_json, review_status, identity_confirmed, comment_confirmed,
                answer_confirmed, clip_confirmed, facts_confirmed, module_confirmed,
                is_active, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,'{}',$8,1,1,1,1,1,1,1,$9,$9)"#,
        )
        .bind(case_id)
        .bind(role_id)
        .bind(module)
        .bind(format!("问题-{case_id}"))
        .bind(format!("真实回答-{case_id}"))
        .bind(clip_path)
        .bind(format!("evidence-{case_id}"))
        .bind(if approved { "approved" } else { "pending" })
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
    }

    fn scored_evaluation() -> StoredTrainingEvaluation {
        StoredTrainingEvaluation {
            evaluation_status: "scored".to_string(),
            scores: Some(TrainingScores {
                needs_confirmation: 5,
                fact_accuracy: 4,
                trust_building: 4,
                expression_clarity: 5,
                deal_advancement: 3,
                risk_compliance: 5,
                live_pacing: 4,
            }),
            priority_dimension: Some(TrainingPriorityDimension::DealAdvancement),
        }
    }

    #[test]
    fn question_exposes_only_safe_viewer_role_metadata() {
        assert_eq!(
            training_viewer_role(r#"{"viewerRole":"  犹豫买家  "}"#),
            "犹豫买家"
        );
        assert_eq!(training_viewer_role("{}"), "直播间观众");
        assert_eq!(
            training_viewer_role(&format!(r#"{{"viewerRole":"{}"}}"#, "x".repeat(41))),
            "直播间观众"
        );
        assert_eq!(training_viewer_role("not-json"), "直播间观众");
    }

    #[tokio::test]
    async fn migration_seeds_confirmed_role_aliases_without_inventing_cases() {
        let database = test_database().await;
        let roles = database.list_training_roles().await.unwrap();
        assert_eq!(roles.len(), 4);
        let hou = roles
            .iter()
            .find(|role| role.display_name == "侯梦娜")
            .unwrap();
        assert_eq!(hou.source_aliases, vec!["天乐"]);
        assert_eq!(hou.approved_case_count, 0);
        assert_eq!(hou.available_case_count, 0);
        assert!(hou.available_modules.is_empty());
        assert_eq!(hou.evidence_status, "empty");
        assert_eq!(
            hou.unavailable_reason.as_deref(),
            Some("暂无经审核训练证据")
        );
        let xiao_e = roles.iter().find(|role| role.role_id == "xiao-e").unwrap();
        assert_eq!(xiao_e.display_name, "小鹅");
        assert_eq!(xiao_e.source_aliases, vec!["小鹅"]);
        assert!(!xiao_e.source_aliases.iter().any(|alias| alias == "小鸦"));
        let pool = database.db.read().await.clone().unwrap();
        let raw_mapping_confirmed = sqlx::query_scalar::<_, i64>(
            "SELECT is_confirmed FROM training_role_aliases WHERE role_id = 'xiao-e' AND source_alias = '小鸦'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(raw_mapping_confirmed, 1);
    }

    #[tokio::test]
    async fn evidence_gate_requires_approval_and_an_existing_clip() {
        let database = test_database().await;
        let valid_clip = temp_clip("gate-valid");
        let empty_mp4 = temp_invalid_media("empty", "mp4", &[]);
        let invalid_mp4 = temp_invalid_media("invalid-header", "mp4", &[0_u8; 64]);
        let fake_webm = temp_invalid_media(
            "recognizable-but-unplayable",
            "webm",
            &[&[0x1a, 0x45, 0xdf, 0xa3][..], &[0_u8; 128][..]].concat(),
        );
        let wrong_extension = temp_invalid_media(
            "wrong-extension",
            "txt",
            include_bytes!("../../../tests/fixtures/media/evidence-1s.webm"),
        );
        insert_case(
            &database,
            "approved-case",
            "hou-mengna",
            "needs_discovery",
            &valid_clip,
            true,
        )
        .await;
        insert_case(
            &database,
            "pending-case",
            "hou-mengna",
            "needs_discovery",
            &valid_clip,
            false,
        )
        .await;
        insert_case(
            &database,
            "missing-clip-case",
            "hou-mengna",
            "needs_discovery",
            "Z:\\missing\\clip.mp4",
            true,
        )
        .await;
        insert_case(
            &database,
            "unconfirmed-answer-case",
            "hou-mengna",
            "needs_discovery",
            &valid_clip,
            true,
        )
        .await;
        insert_case(
            &database,
            "empty-media-case",
            "hou-mengna",
            "needs_discovery",
            &empty_mp4,
            true,
        )
        .await;
        insert_case(
            &database,
            "invalid-media-case",
            "hou-mengna",
            "needs_discovery",
            &invalid_mp4,
            true,
        )
        .await;
        insert_case(
            &database,
            "fake-webm-case",
            "hou-mengna",
            "needs_discovery",
            &fake_webm,
            true,
        )
        .await;
        insert_case(
            &database,
            "wrong-extension-case",
            "hou-mengna",
            "needs_discovery",
            &wrong_extension,
            true,
        )
        .await;
        let pool = database.db.read().await.clone().unwrap();
        sqlx::query("UPDATE training_cases SET answer_confirmed = 0 WHERE case_id = $1")
            .bind("unconfirmed-answer-case")
            .execute(&pool)
            .await
            .unwrap();
        let roles = database.list_training_roles().await.unwrap();
        let hou = roles
            .iter()
            .find(|role| role.role_id == "hou-mengna")
            .unwrap();
        assert_eq!(hou.approved_case_count, 1);
        assert_eq!(hou.module_counts["needs_discovery"], 1);
        assert!(media_file_identity(Path::new(&fake_webm)).is_some());
        assert!(media_file_signature(Path::new(&fake_webm)).await.is_none());
        std::fs::remove_file(valid_clip).unwrap();
        std::fs::remove_file(empty_mp4).unwrap();
        std::fs::remove_file(invalid_mp4).unwrap();
        std::fs::remove_file(fake_webm).unwrap();
        std::fs::remove_file(wrong_extension).unwrap();
    }

    #[tokio::test]
    async fn human_machine_scenario_persists_three_turns_and_completes_once() {
        let database = test_database().await;
        let clip = temp_clip("human-machine");
        insert_case(
            &database,
            "hm-real-case",
            "luo-yuxin",
            "objection_handling",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_human_machine_scenario("luo-yuxin", Some("objection_handling"), Some(31), 3)
            .await
            .unwrap();
        assert_eq!(started.current_turn, 1);
        assert_eq!(started.turns[0].viewer_source, "approved_comment");

        database
            .save_human_machine_answer(&started.run_id, "请问你主要是什么用途？")
            .await
            .unwrap();
        // Same answer is an idempotent retry while a follow-up is pending.
        database
            .save_human_machine_answer(&started.run_id, "请问你主要是什么用途？")
            .await
            .unwrap();
        database
            .add_human_machine_follow_up(
                &started.run_id,
                "那我主要拍人像，怎么选？",
                "needs_confirmation",
            )
            .await
            .unwrap();
        let resumed = database
            .active_human_machine_scenario("luo-yuxin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resumed.run_id, started.run_id);
        assert_eq!(resumed.current_turn, 2);
        database
            .save_human_machine_answer(&started.run_id, "参数我先核实，再给你选择。")
            .await
            .unwrap();
        database
            .add_human_machine_follow_up(
                &started.run_id,
                "如果不能确认，我下一步怎么做？",
                "deal_advancement",
            )
            .await
            .unwrap();
        let context = database
            .save_human_machine_answer(&started.run_id, "确认后可以再决定是否下单。")
            .await
            .unwrap();
        assert_eq!(context.turns.len(), 3);
        let checks = vec![
            HumanMachineHardCheck {
                key: "needs_confirmation".into(),
                label: "先确认需求".into(),
                passed: true,
                detail: "通过".into(),
            },
            HumanMachineHardCheck {
                key: "fact_boundary".into(),
                label: "不编造未确认事实".into(),
                passed: true,
                detail: "通过".into(),
            },
            HumanMachineHardCheck {
                key: "compliant_next_step".into(),
                label: "给出合规下一步".into(),
                passed: true,
                detail: "通过".into(),
            },
        ];
        let completed = database
            .complete_human_machine_scenario(&started.run_id, &scored_evaluation(), &checks)
            .await
            .unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.turns.len(), 3);
        assert_eq!(completed.hard_checks.len(), 3);
        assert!(completed.scores.unwrap().is_valid());
        assert!(database
            .complete_human_machine_scenario(&started.run_id, &scored_evaluation(), &checks)
            .await
            .is_ok());
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn human_machine_scenario_supports_five_adaptive_turns() {
        let database = test_database().await;
        let clip = temp_clip("human-machine-five");
        insert_case(
            &database,
            "hm-five-real-case",
            "luo-yuxin",
            "needs_discovery",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_human_machine_scenario("luo-yuxin", Some("needs_discovery"), Some(32), 5)
            .await
            .unwrap();
        assert_eq!(started.total_turns, 5);

        for turn in 1..5 {
            database
                .save_human_machine_answer(
                    &started.run_id,
                    &format!("第{turn}轮先确认需求，再核实事实。"),
                )
                .await
                .unwrap();
            let response = database
                .add_human_machine_follow_up(
                    &started.run_id,
                    &format!("第{}轮还需要确认什么？", turn + 1),
                    "needs_confirmation",
                )
                .await
                .unwrap();
            assert_eq!(response.current_turn, turn + 1);
        }
        database
            .save_human_machine_answer(&started.run_id, "最后给出合规的下一步。")
            .await
            .unwrap();
        let completed = database
            .complete_human_machine_scenario(
                &started.run_id,
                &scored_evaluation(),
                &[
                    HumanMachineHardCheck {
                        key: "needs_confirmation".into(),
                        label: "先确认需求".into(),
                        passed: true,
                        detail: "通过".into(),
                    },
                    HumanMachineHardCheck {
                        key: "fact_boundary".into(),
                        label: "不编造未确认事实".into(),
                        passed: true,
                        detail: "通过".into(),
                    },
                    HumanMachineHardCheck {
                        key: "compliant_next_step".into(),
                        label: "给出合规下一步".into(),
                        passed: true,
                        detail: "通过".into(),
                    },
                ],
            )
            .await
            .unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.total_turns, 5);
        assert_eq!(completed.turns.len(), 5);
        assert_eq!(
            completed.turns[1].follow_up_focus.as_deref(),
            Some("needs_confirmation")
        );
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn specialized_session_never_leaks_other_roles_modules_or_answers() {
        let database = test_database().await;
        let clip = temp_clip("scope");
        insert_case(
            &database,
            "hou-needs",
            "hou-mengna",
            "needs_discovery",
            &clip,
            true,
        )
        .await;
        insert_case(
            &database,
            "hou-product",
            "hou-mengna",
            "product_explanation",
            &clip,
            true,
        )
        .await;
        insert_case(
            &database,
            "luo-needs",
            "luo-yuxin",
            "needs_discovery",
            &clip,
            true,
        )
        .await;

        let started = database
            .start_training_session(
                "hou-mengna",
                "specialized",
                Some("needs_discovery"),
                Some(20260828),
            )
            .await
            .unwrap();
        assert_eq!(started.total_questions, 1);
        assert_eq!(started.question.case_id, "hou-needs");
        let json = serde_json::to_value(&started).unwrap();
        assert!(json.get("realAnswer").is_none());
        assert!(json.get("clipPath").is_none());
        assert_eq!(json["question"]["module"], "needs_discovery");
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn session_freezes_every_case_field_used_for_questions_scoring_and_unlock() {
        let database = test_database().await;
        let original_clip = temp_clip("snapshot-original");
        let revised_clip = temp_clip("snapshot-revised");
        for case_id in ["snapshot-a", "snapshot-b"] {
            insert_case(
                &database,
                case_id,
                "hou-mengna",
                "product_explanation",
                &original_clip,
                true,
            )
            .await;
        }
        let started = database
            .start_training_session("hou-mengna", "comprehensive", None, Some(20260828))
            .await
            .unwrap();
        let snapshots = parse_case_order(
            &database
                .training_session(&started.session_id)
                .await
                .unwrap()
                .case_order_json,
        )
        .unwrap();
        assert_eq!(started.question, safe_question(&snapshots[0]));

        let pool = database.db.read().await.clone().unwrap();
        for snapshot in &snapshots {
            sqlx::query(
                r#"UPDATE training_cases
                   SET prompt = $1, real_answer = $2, clip_path = $3, evidence_id = $4,
                       fact_constraints_json = '{"revised":true}', updated_at = $5
                   WHERE case_id = $6"#,
            )
            .bind(format!("改版题面-{}", snapshot.case_id))
            .bind(format!("改版回答-{}", snapshot.case_id))
            .bind(&revised_clip)
            .bind(format!("revised-evidence-{}", snapshot.case_id))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(&snapshot.case_id)
            .execute(&pool)
            .await
            .unwrap();
        }

        let first_context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        assert!(context_matches_snapshot(&first_context, &snapshots[0]));
        assert_eq!(
            first_context.prompt,
            format!("问题-{}", snapshots[0].case_id)
        );
        assert_eq!(first_context.clip_path, original_clip);
        assert_eq!(first_context.fact_constraints_json, "{}");
        let mut tampered_context = first_context.clone();
        tampered_context.real_answer = "不属于用户所见快照的回答".to_string();
        let tampered_error = database
            .save_training_submission(
                &tampered_context,
                "不能用被篡改的评分上下文解锁",
                &scored_evaluation(),
            )
            .await
            .unwrap_err();
        assert!(tampered_error
            .to_string()
            .contains("上下文与用户所见题目不一致"));
        let first_saved = database
            .save_training_submission(
                &first_context,
                "按用户实际看到的原题作答",
                &scored_evaluation(),
            )
            .await
            .unwrap();
        assert_eq!(first_saved.real_answer, snapshots[0].real_answer);
        assert_eq!(first_saved.clip_path, snapshots[0].clip_path);
        assert_eq!(first_saved.evidence_id, snapshots[0].evidence_id);

        let next = database
            .next_training_question(&started.session_id)
            .await
            .unwrap();
        assert_eq!(next.question, Some(safe_question(&snapshots[1])));
        let second_context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        assert!(context_matches_snapshot(&second_context, &snapshots[1]));
        assert_eq!(second_context.real_answer, snapshots[1].real_answer);
        assert_eq!(second_context.clip_path, snapshots[1].clip_path);
        assert_eq!(second_context.evidence_id, snapshots[1].evidence_id);
        assert_eq!(second_context.fact_constraints_json, "{}");

        std::fs::remove_file(original_clip).unwrap();
        std::fs::remove_file(revised_clip).unwrap();
    }

    #[tokio::test]
    async fn submitted_answer_reveals_snapshot_and_next_requires_submission() {
        let database = test_database().await;
        let clip = temp_clip("turn");
        insert_case(
            &database,
            "turn-case",
            "hou-mengna",
            "objection_handling",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_training_session("hou-mengna", "comprehensive", None, Some(7))
            .await
            .unwrap();
        let blocked = database
            .next_training_question(&started.session_id)
            .await
            .unwrap_err();
        assert!(blocked.to_string().contains("请先提交当前问题"));
        let context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        let saved = database
            .save_training_submission(&context, "我先确认您的使用需求。", &scored_evaluation())
            .await
            .unwrap();
        assert_eq!(saved.real_answer, "真实回答-turn-case");
        assert_eq!(saved.clip_path, clip);
        let next = database
            .next_training_question(&started.session_id)
            .await
            .unwrap();
        assert!(next.is_complete);
        let summary = database
            .complete_training_session(&started.session_id)
            .await
            .unwrap();
        assert_eq!(summary.answered_count, 1);
        assert_eq!(summary.evidence_ids, vec!["evidence-turn-case"]);
        std::fs::remove_file(saved.clip_path).unwrap();
    }

    #[tokio::test]
    async fn submission_rechecks_revoked_evidence_after_evaluation_wait() {
        let database = test_database().await;
        let clip = temp_clip("revoked-during-evaluation");
        insert_case(
            &database,
            "revoked-case",
            "hou-mengna",
            "incident",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_training_session("hou-mengna", "comprehensive", None, Some(17))
            .await
            .unwrap();
        let context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();

        let pool = database.db.read().await.clone().unwrap();
        sqlx::query("UPDATE training_cases SET review_status = 'rejected' WHERE case_id = $1")
            .bind(&context.case_id)
            .execute(&pool)
            .await
            .unwrap();

        let error = database
            .save_training_submission(&context, "这是等待AI后的回答", &scored_evaluation())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("未解锁真实证据"));
        assert!(database
            .current_training_submission(&started.session_id)
            .await
            .unwrap()
            .is_none());
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn frozen_snapshot_does_not_bypass_source_case_disable() {
        let database = test_database().await;
        let clip = temp_clip("disabled-during-evaluation");
        insert_case(
            &database,
            "disabled-case",
            "hou-mengna",
            "incident",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_training_session("hou-mengna", "comprehensive", None, Some(18))
            .await
            .unwrap();
        let context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        let pool = database.db.read().await.clone().unwrap();
        sqlx::query("UPDATE training_cases SET is_active = 0 WHERE case_id = $1")
            .bind(&context.case_id)
            .execute(&pool)
            .await
            .unwrap();

        let error = database
            .save_training_submission(&context, "停用后不能解锁", &scored_evaluation())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("未解锁真实证据"));
        assert!(database
            .current_training_submission(&started.session_id)
            .await
            .unwrap()
            .is_none());
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn submission_rechecks_clip_after_evaluation_wait() {
        let database = test_database().await;
        let clip = temp_clip("clip-removed-during-evaluation");
        insert_case(
            &database,
            "clip-recheck-case",
            "hou-mengna",
            "incident",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_training_session("hou-mengna", "comprehensive", None, Some(19))
            .await
            .unwrap();
        let context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        let mut replacement =
            include_bytes!("../../../tests/fixtures/media/evidence-1s.webm").to_vec();
        let last = replacement.last_mut().unwrap();
        *last ^= 0x01;
        std::fs::write(&clip, replacement).unwrap();
        assert!(media_file_signature(Path::new(&clip)).await.is_some());

        let error = database
            .save_training_submission(&context, "片段失效后的回答", &scored_evaluation())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("片段已变化或不可播放"));
        assert!(database
            .current_training_submission(&started.session_id)
            .await
            .unwrap()
            .is_none());
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn specialized_session_rejects_case_module_changes_before_submit_and_next() {
        let database = test_database().await;
        let clip = temp_clip("specialized-module-change");
        for case_id in ["specialized-a", "specialized-b"] {
            insert_case(&database, case_id, "xiao-e", "needs_discovery", &clip, true).await;
        }
        let started = database
            .start_training_session(
                "xiao-e",
                "specialized",
                Some("needs_discovery"),
                Some(20260828),
            )
            .await
            .unwrap();
        let first_context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        let order = parse_case_order(&first_context.case_order_json).unwrap();
        let second_case_id = order
            .iter()
            .find(|snapshot| snapshot.case_id != first_context.case_id)
            .map(|snapshot| snapshot.case_id.clone())
            .unwrap();
        let pool = database.db.read().await.clone().unwrap();
        sqlx::query("UPDATE training_cases SET module = 'product_explanation' WHERE case_id = $1")
            .bind(&first_context.case_id)
            .execute(&pool)
            .await
            .unwrap();
        let submit_error = database
            .save_training_submission(&first_context, "板块已变化时不能解锁", &scored_evaluation())
            .await
            .unwrap_err();
        assert!(submit_error.to_string().contains("未解锁真实证据"));
        assert!(database
            .current_training_submission(&started.session_id)
            .await
            .unwrap()
            .is_none());
        sqlx::query("UPDATE training_cases SET module = 'needs_discovery' WHERE case_id = $1")
            .bind(&first_context.case_id)
            .execute(&pool)
            .await
            .unwrap();
        database
            .save_training_submission(&first_context, "先完成第一道专项题", &scored_evaluation())
            .await
            .unwrap();

        sqlx::query("UPDATE training_cases SET module = 'product_explanation' WHERE case_id = $1")
            .bind(&second_case_id)
            .execute(&pool)
            .await
            .unwrap();
        let error = database
            .next_training_question(&started.session_id)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("训练证据已不可用"));
        assert_eq!(
            database
                .training_session(&started.session_id)
                .await
                .unwrap()
                .current_index,
            0
        );
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn complete_rejects_unanswered_session_and_accepts_last_submitted_answer() {
        let database = test_database().await;
        let clip = temp_clip("complete-requires-all");
        insert_case(
            &database,
            "complete-case",
            "luo-yuxin",
            "opening",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_training_session("luo-yuxin", "comprehensive", None, Some(23))
            .await
            .unwrap();
        let error = database
            .complete_training_session(&started.session_id)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("完成并提交全部训练题目"));
        assert_eq!(
            database
                .training_session(&started.session_id)
                .await
                .unwrap()
                .status,
            "active"
        );

        let context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        database
            .save_training_submission(&context, "完成最后一道题", &scored_evaluation())
            .await
            .unwrap();
        let summary = database
            .complete_training_session(&started.session_id)
            .await
            .unwrap();
        assert_eq!(summary.answered_count, 1);
        std::fs::remove_file(clip).unwrap();
    }

    #[tokio::test]
    async fn complete_and_abandon_cas_never_report_conflicting_terminal_states() {
        let database = test_database().await;
        let clip = temp_clip("terminal-cas");
        insert_case(
            &database,
            "terminal-case",
            "hou-mengna",
            "incident",
            &clip,
            true,
        )
        .await;
        let started = database
            .start_training_session("hou-mengna", "comprehensive", None, Some(29))
            .await
            .unwrap();
        let context = database
            .training_submission_context(&started.session_id)
            .await
            .unwrap();
        database
            .save_training_submission(&context, "终态竞态测试回答", &scored_evaluation())
            .await
            .unwrap();

        let (completed, abandoned) = tokio::join!(
            database.complete_training_session(&started.session_id),
            database.abandon_training_session(&started.session_id)
        );
        assert_ne!(completed.is_ok(), abandoned.is_ok());
        let status = database
            .training_session(&started.session_id)
            .await
            .unwrap()
            .status;
        assert_eq!(completed.is_ok(), status == "completed");
        assert_eq!(abandoned.is_ok(), status == "abandoned");
        std::fs::remove_file(clip).unwrap();
    }

    #[test]
    fn deterministic_order_is_repeatable_for_a_fixed_seed() {
        let mut first = vec![1, 2, 3, 4, 5];
        let mut second = first.clone();
        deterministic_shuffle(&mut first, 20260828);
        deterministic_shuffle(&mut second, 20260828);
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn abandoning_a_session_blocks_further_answers() {
        let database = test_database().await;
        let clip = temp_clip("abandon");
        insert_case(&database, "abandon-case", "xiao-e", "incident", &clip, true).await;
        let started = database
            .start_training_session("xiao-e", "comprehensive", None, Some(11))
            .await
            .unwrap();
        let abandoned = database
            .abandon_training_session(&started.session_id)
            .await
            .unwrap();
        assert_eq!(abandoned.status, "abandoned");
        assert!(database
            .training_submission_context(&started.session_id)
            .await
            .is_err());
        std::fs::remove_file(clip).unwrap();
    }
}
