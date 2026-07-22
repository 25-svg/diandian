use super::{Database, DatabaseError};

pub(crate) const TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL: &str = r#"
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
"#;

pub(crate) const SET_TRANSCRIPT_DICTIONARY_CANDIDATE_STATUS_SQL: &str =
    "UPDATE transcript_dictionary_candidates \
     SET status = $1, updated_at = datetime('now') WHERE id = $2 \
     RETURNING *";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum TranscriptDictionaryCandidateType {
    Hotword,
    Replacement,
    LocalRule,
}

impl TranscriptDictionaryCandidateType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Hotword => "hotword",
            Self::Replacement => "replacement",
            Self::LocalRule => "local_rule",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum TranscriptDictionaryCandidateStatus {
    Pending,
    Approved,
    Rejected,
    Synced,
    SyncFailed,
}

impl TranscriptDictionaryCandidateStatus {
    fn parse(value: &str) -> Result<Self, DatabaseError> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "synced" => Ok(Self::Synced),
            "sync_failed" => Ok(Self::SyncFailed),
            _ => Err(DatabaseError::InvalidTranscriptDictionaryCandidateStatus(
                value.to_string(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Synced => "synced",
            Self::SyncFailed => "sync_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct TranscriptDictionaryCandidateRow {
    pub id: i64,
    pub candidate_type: TranscriptDictionaryCandidateType,
    pub source_text: String,
    pub target_text: String,
    pub evidence_json: String,
    pub source_json: String,
    pub status: TranscriptDictionaryCandidateStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSourceKind {
    Archive,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSourceMetadata {
    pub correction_id: String,
    pub source_kind: CandidateSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

impl CandidateSourceMetadata {
    pub fn validate(&self) -> Result<(), DatabaseError> {
        validate_metadata_identifier("correctionId", &self.correction_id, 128)?;
        validate_timestamp_range(self.start_ms, self.end_ms)?;

        match self.source_kind {
            CandidateSourceKind::Archive => {
                let (Some(platform), Some(room_id), Some(live_id)) =
                    (&self.platform, &self.room_id, &self.live_id)
                else {
                    return Err(invalid_candidate_metadata(
                        "archive source requires platform, roomId, and liveId",
                    ));
                };
                if self.video_id.is_some() {
                    return Err(invalid_candidate_metadata(
                        "archive source may not include videoId",
                    ));
                }
                validate_metadata_identifier("platform", platform, 64)?;
                validate_metadata_identifier("roomId", room_id, 128)?;
                validate_metadata_identifier("liveId", live_id, 128)?;
            }
            CandidateSourceKind::Video => {
                let Some(video_id) = self.video_id else {
                    return Err(invalid_candidate_metadata(
                        "video source requires a positive videoId",
                    ));
                };
                if video_id <= 0 {
                    return Err(invalid_candidate_metadata(
                        "video source requires a positive videoId",
                    ));
                }
                if self.platform.is_some() || self.room_id.is_some() || self.live_id.is_some() {
                    return Err(invalid_candidate_metadata(
                        "video source may not include archive identifiers",
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateEvidenceKind {
    AsrProvider,
    FactCardField,
    HumanReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAsrProvider {
    Volcengine,
    Funasr,
    Whisper,
    Minimax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateFactCardField {
    ProductName,
    Model,
    Price,
    Condition,
    Stock,
    Discount,
    Link,
    Gift,
    Warranty,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateEvidence {
    pub kind: CandidateEvidenceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<CandidateAsrProvider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<CandidateFactCardField>,
}

impl CandidateEvidence {
    pub fn validate(&self) -> Result<(), DatabaseError> {
        match self.kind {
            CandidateEvidenceKind::AsrProvider
                if self.provider.is_some() && self.field.is_none() =>
            {
                Ok(())
            }
            CandidateEvidenceKind::FactCardField
                if self.provider.is_none() && self.field.is_some() =>
            {
                Ok(())
            }
            CandidateEvidenceKind::HumanReview
                if self.provider.is_none() && self.field.is_none() =>
            {
                Ok(())
            }
            CandidateEvidenceKind::AsrProvider => Err(invalid_candidate_metadata(
                "asr_provider evidence requires provider and may not include field",
            )),
            CandidateEvidenceKind::FactCardField => Err(invalid_candidate_metadata(
                "fact_card_field evidence requires field and may not include provider",
            )),
            CandidateEvidenceKind::HumanReview => Err(invalid_candidate_metadata(
                "human_review evidence may not include provider or field",
            )),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTranscriptDictionaryCandidate {
    pub candidate_type: TranscriptDictionaryCandidateType,
    pub source_text: String,
    pub target_text: String,
    pub evidence: Vec<CandidateEvidence>,
    pub source: CandidateSourceMetadata,
}

impl Database {
    pub async fn upsert_transcript_dictionary_candidate(
        &self,
        input: NewTranscriptDictionaryCandidate,
    ) -> Result<TranscriptDictionaryCandidateRow, DatabaseError> {
        input.source.validate()?;
        validate_evidence(&input.evidence)?;
        let evidence_json = serde_json::to_string(&input.evidence)
            .map_err(|error| invalid_candidate_metadata(error.to_string()))?;
        let source_json = serde_json::to_string(&input.source)
            .map_err(|error| invalid_candidate_metadata(error.to_string()))?;
        let source_text = input.source_text.trim().to_string();
        let target_text = input.target_text.trim().to_string();
        let pool = self.db.read().await.clone().unwrap();

        sqlx::query(
            r#"
            INSERT INTO transcript_dictionary_candidates (
                candidate_type, source_text, target_text, evidence_json, source_json
            ) VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(candidate_type, source_text, target_text) DO UPDATE SET
                evidence_json = excluded.evidence_json,
                updated_at = datetime('now')
            "#,
        )
        .bind(input.candidate_type.as_str())
        .bind(&source_text)
        .bind(&target_text)
        .bind(evidence_json)
        .bind(source_json)
        .execute(&pool)
        .await?;

        Ok(sqlx::query_as::<_, TranscriptDictionaryCandidateRow>(
            "SELECT * FROM transcript_dictionary_candidates \
             WHERE candidate_type = $1 AND source_text = $2 AND target_text = $3",
        )
        .bind(input.candidate_type.as_str())
        .bind(source_text)
        .bind(target_text)
        .fetch_one(&pool)
        .await?)
    }

    pub async fn list_transcript_dictionary_candidates(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<TranscriptDictionaryCandidateRow>, DatabaseError> {
        let status = status
            .map(TranscriptDictionaryCandidateStatus::parse)
            .transpose()?;
        let pool = self.db.read().await.clone().unwrap();

        let candidates = match status {
            Some(status) => {
                sqlx::query_as::<_, TranscriptDictionaryCandidateRow>(
                    "SELECT * FROM transcript_dictionary_candidates \
                 WHERE status = $1 ORDER BY updated_at DESC, id DESC",
                )
                .bind(status.as_str())
                .fetch_all(&pool)
                .await?
            }
            None => sqlx::query_as::<_, TranscriptDictionaryCandidateRow>(
                "SELECT * FROM transcript_dictionary_candidates ORDER BY updated_at DESC, id DESC",
            )
            .fetch_all(&pool)
            .await?,
        };

        Ok(candidates)
    }

    pub async fn set_transcript_dictionary_candidate_status(
        &self,
        id: i64,
        status: &str,
    ) -> Result<TranscriptDictionaryCandidateRow, DatabaseError> {
        let status = TranscriptDictionaryCandidateStatus::parse(status)?;
        let pool = self.db.read().await.clone().unwrap();

        sqlx::query_as::<_, TranscriptDictionaryCandidateRow>(
            SET_TRANSCRIPT_DICTIONARY_CANDIDATE_STATUS_SQL,
        )
        .bind(status.as_str())
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or(DatabaseError::NotFound)
    }
}

pub fn export_transcript_dictionary_candidates_json(
    candidates: &[TranscriptDictionaryCandidateRow],
) -> Result<String, serde_json::Error> {
    serde_json::to_string(candidates)
}

pub fn export_transcript_dictionary_candidates_csv(
    candidates: &[TranscriptDictionaryCandidateRow],
) -> String {
    let mut output = String::from(
        "\u{7c7b}\u{578b},\u{539f}\u{6587},\u{76ee}\u{6807}\u{6587}\u{672c},\u{72b6}\u{6001},\u{521b}\u{5efa}\u{65f6}\u{95f4}\r\n",
    );
    for candidate in candidates {
        let fields = [
            candidate.candidate_type.as_str(),
            candidate.source_text.as_str(),
            candidate.target_text.as_str(),
            candidate.status.as_str(),
            candidate.created_at.as_str(),
        ];
        output.push_str(
            &fields
                .into_iter()
                .map(escape_csv_field)
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push_str("\r\n");
    }
    output
}

fn escape_csv_field(value: &str) -> String {
    let value = if value
        .chars()
        .find(|character| !character.is_whitespace())
        .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@'))
    {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if value.contains(',') || value.contains('"') || value.contains('\r') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

const MAX_EVIDENCE_ITEMS: usize = 8;

fn validate_evidence(evidence: &[CandidateEvidence]) -> Result<(), DatabaseError> {
    if evidence.is_empty() || evidence.len() > MAX_EVIDENCE_ITEMS {
        return Err(invalid_candidate_metadata(
            "evidence must contain between one and eight typed entries",
        ));
    }
    for item in evidence {
        item.validate()?;
    }
    Ok(())
}

fn validate_metadata_identifier(
    name: &str,
    value: &str,
    max_length: usize,
) -> Result<(), DatabaseError> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.chars().count() > max_length
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || value == "."
        || value == ".."
    {
        return Err(invalid_candidate_metadata(format!(
            "{name} must be a bounded identifier without path components"
        )));
    }
    Ok(())
}

fn validate_timestamp_range(
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<(), DatabaseError> {
    match (start_ms, end_ms) {
        (Some(start_ms), Some(end_ms)) if start_ms <= end_ms => Ok(()),
        (Some(_), Some(_)) => Err(invalid_candidate_metadata(
            "startMs must not be greater than endMs",
        )),
        _ => Ok(()),
    }
}

fn invalid_candidate_metadata(message: impl Into<String>) -> DatabaseError {
    DatabaseError::InvalidTranscriptDictionaryCandidateMetadata(message.into())
}

#[cfg(test)]
mod tests {
    use super::{
        escape_csv_field, export_transcript_dictionary_candidates_csv,
        export_transcript_dictionary_candidates_json, CandidateAsrProvider, CandidateEvidence,
        CandidateEvidenceKind, CandidateSourceKind, CandidateSourceMetadata,
        NewTranscriptDictionaryCandidate, TranscriptDictionaryCandidateStatus,
        TranscriptDictionaryCandidateType, SET_TRANSCRIPT_DICTIONARY_CANDIDATE_STATUS_SQL,
        TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL,
    };
    use crate::database::Database;
    use sqlx::{Executor, SqlitePool};

    fn new_replacement(source_text: &str, target_text: &str) -> NewTranscriptDictionaryCandidate {
        NewTranscriptDictionaryCandidate {
            candidate_type: TranscriptDictionaryCandidateType::Replacement,
            source_text: source_text.to_string(),
            target_text: target_text.to_string(),
            evidence: vec![CandidateEvidence {
                kind: CandidateEvidenceKind::AsrProvider,
                provider: Some(CandidateAsrProvider::Volcengine),
                field: None,
            }],
            source: CandidateSourceMetadata {
                correction_id: "review-1".to_string(),
                source_kind: CandidateSourceKind::Video,
                platform: None,
                room_id: None,
                live_id: None,
                video_id: Some(7),
                start_ms: Some(1_000),
                end_ms: Some(2_000),
            },
        }
    }

    async fn database(pool: SqlitePool) -> Database {
        pool.execute(TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL)
            .await
            .unwrap();
        let db = Database::new();
        db.set(pool).await;
        db
    }

    #[sqlx::test]
    async fn migration_enforces_candidate_types_statuses_unique_dedupe_and_index(pool: SqlitePool) {
        pool.execute(TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL)
            .await
            .unwrap();

        for candidate_type in ["hotword", "replacement", "local_rule"] {
            sqlx::query(
                r#"
                INSERT INTO transcript_dictionary_candidates
                    (candidate_type, source_text, target_text, source_json)
                VALUES ($1, $2, '', '{}')
                "#,
            )
            .bind(candidate_type)
            .bind(format!("source-{candidate_type}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        for status in ["pending", "approved", "rejected", "synced", "sync_failed"] {
            sqlx::query(
                r#"
                INSERT INTO transcript_dictionary_candidates
                    (candidate_type, source_text, target_text, source_json, status)
                VALUES ('replacement', $1, $2, '{}', $3)
                "#,
            )
            .bind(format!("status-source-{status}"))
            .bind(format!("status-target-{status}"))
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert!(sqlx::query(
            "INSERT INTO transcript_dictionary_candidates \
             (candidate_type, source_text, target_text, source_json) \
             VALUES ('arbitrary', 'source', 'target', '{}')",
        )
        .execute(&pool)
        .await
        .is_err());
        assert!(sqlx::query(
            "INSERT INTO transcript_dictionary_candidates \
             (candidate_type, source_text, target_text, source_json, status) \
             VALUES ('replacement', 'source', 'target', '{}', 'arbitrary')",
        )
        .execute(&pool)
        .await
        .is_err());

        sqlx::query(
            "INSERT INTO transcript_dictionary_candidates \
             (candidate_type, source_text, target_text, source_json) \
             VALUES ('replacement', 'source', 'target', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(sqlx::query(
            "INSERT INTO transcript_dictionary_candidates \
             (candidate_type, source_text, target_text, source_json) \
             VALUES ('replacement', 'source', 'target', '{}')",
        )
        .execute(&pool)
        .await
        .is_err());

        let index_sql: String = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'index' AND name = 'idx_transcript_dictionary_candidates_status'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(index_sql.contains("status, updated_at DESC"));
    }

    #[sqlx::test]
    async fn upsert_candidate_deduplicates_same_replacement(pool: SqlitePool) {
        let db = database(pool).await;

        let first = db
            .upsert_transcript_dictionary_candidate(new_replacement(
                "A4PRO299\u{7f8a}",
                "A4PRO2 99\u{7f8a}",
            ))
            .await
            .unwrap();
        let second = db
            .upsert_transcript_dictionary_candidate(new_replacement(
                "A4PRO299\u{7f8a}",
                "A4PRO2 99\u{7f8a}",
            ))
            .await
            .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(
            db.list_transcript_dictionary_candidates(None)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[sqlx::test]
    async fn upsert_deduplication_preserves_first_source_provenance(pool: SqlitePool) {
        let db = database(pool).await;
        let first = new_replacement("source", "target");
        let mut duplicate = new_replacement("source", "target");
        duplicate.source.correction_id = "review-2".to_string();
        duplicate.source.video_id = Some(8);

        let first = db
            .upsert_transcript_dictionary_candidate(first)
            .await
            .unwrap();
        let duplicate = db
            .upsert_transcript_dictionary_candidate(duplicate)
            .await
            .unwrap();

        assert_eq!(duplicate.id, first.id);
        assert_eq!(duplicate.source_json, first.source_json);
        assert!(duplicate.source_json.contains(r#""videoId":7"#));
        assert!(!duplicate.source_json.contains(r#""videoId":8"#));
    }

    #[sqlx::test]
    async fn upsert_serializes_only_typed_evidence(pool: SqlitePool) {
        let db = database(pool).await;

        let row = db
            .upsert_transcript_dictionary_candidate(new_replacement("source", "target"))
            .await
            .unwrap();

        assert_eq!(
            row.evidence_json,
            r#"[{"kind":"asr_provider","provider":"volcengine"}]"#
        );
    }

    #[sqlx::test]
    async fn list_filters_by_status_and_status_transition_returns_typed_row(pool: SqlitePool) {
        let db = database(pool).await;
        let pending = db
            .upsert_transcript_dictionary_candidate(new_replacement(
                "\u{539f}\u{6587}",
                "\u{76ee}\u{6807}\u{6587}\u{672c}",
            ))
            .await
            .unwrap();

        let approved = db
            .set_transcript_dictionary_candidate_status(pending.id, "approved")
            .await
            .unwrap();

        assert_eq!(
            approved.candidate_type,
            TranscriptDictionaryCandidateType::Replacement
        );
        assert_eq!(
            approved.status,
            TranscriptDictionaryCandidateStatus::Approved
        );
        assert_eq!(
            db.list_transcript_dictionary_candidates(Some("approved"))
                .await
                .unwrap(),
            vec![approved]
        );
        assert!(db
            .list_transcript_dictionary_candidates(Some("pending"))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn status_validator_rejects_arbitrary_values_before_sql() {
        let db = Database::new();

        let set_error = db
            .set_transcript_dictionary_candidate_status(1, "arbitrary")
            .await
            .unwrap_err();
        let list_error = db
            .list_transcript_dictionary_candidates(Some("arbitrary"))
            .await
            .unwrap_err();

        assert_eq!(
            set_error.to_string(),
            "invalid transcript dictionary candidate status: arbitrary"
        );
        assert_eq!(list_error.to_string(), set_error.to_string());
    }

    #[sqlx::test]
    async fn status_transition_accepts_each_allowed_value(pool: SqlitePool) {
        let db = database(pool).await;
        let candidate = db
            .upsert_transcript_dictionary_candidate(new_replacement("source", "target"))
            .await
            .unwrap();

        for status in ["pending", "approved", "rejected", "synced", "sync_failed"] {
            let updated = db
                .set_transcript_dictionary_candidate_status(candidate.id, status)
                .await
                .unwrap();
            assert_eq!(serde_json::to_value(updated).unwrap()["status"], status);
        }
    }

    #[test]
    fn status_update_query_returns_the_mutated_row_atomically() {
        let normalized = SET_TRANSCRIPT_DICTIONARY_CANDIDATE_STATUS_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_uppercase();

        assert!(normalized.starts_with("UPDATE TRANSCRIPT_DICTIONARY_CANDIDATES"));
        assert!(normalized.ends_with("RETURNING *"));
        assert!(!normalized.contains("SELECT"));
    }

    #[test]
    fn source_metadata_rejects_unknown_and_non_audit_fields() {
        for source in [
            r#"{"correctionId":"review-1","sourceKind":"video","videoId":7,"prices":[5839]}"#,
            r#"{"correctionId":"review-1","sourceKind":"video","videoId":7,"note":"secret"}"#,
            r#"{"correctionId":"review-1","sourceKind":"video","videoId":7,"factCard":{"sku":"A4PRO2"}}"#,
        ] {
            assert!(serde_json::from_str::<CandidateSourceMetadata>(source).is_err());
        }
    }

    #[test]
    fn source_metadata_requires_matching_identity_and_safe_identifiers() {
        assert!(serde_json::from_str::<CandidateSourceMetadata>(
            r#"{"correctionId":"review-1","sourceKind":"archive","platform":"bilibili","roomId":"room-1","liveId":"live-1"}"#,
        )
        .unwrap()
        .validate()
        .is_ok());
        assert!(serde_json::from_str::<CandidateSourceMetadata>(
            r#"{"correctionId":"review-1","sourceKind":"video","videoId":7,"startMs":1000}"#,
        )
        .unwrap()
        .validate()
        .is_ok());
        for source in [
            r#"{"correctionId":"review-1","sourceKind":"archive","platform":"bilibili","roomId":"../room","liveId":"live-1"}"#,
            r#"{"correctionId":"review-1","sourceKind":"archive","platform":"bilibili","roomId":"room-1","liveId":"live-1","videoId":7}"#,
            r#"{"correctionId":"review-1","sourceKind":"video","videoId":7,"platform":"bilibili"}"#,
            r#"{"correctionId":"review-1","sourceKind":"video"}"#,
        ] {
            assert!(serde_json::from_str::<CandidateSourceMetadata>(source)
                .unwrap()
                .validate()
                .is_err());
        }
    }

    #[test]
    fn evidence_requires_typed_allowlist_and_matching_fields() {
        for evidence in [
            r#"[{"kind":"asr_provider","provider":"volcengine"}]"#,
            r#"[{"kind":"fact_card_field","field":"product_name"}]"#,
            r#"[{"kind":"human_review"}]"#,
        ] {
            let evidence = serde_json::from_str::<Vec<CandidateEvidence>>(evidence).unwrap();
            assert!(super::validate_evidence(&evidence).is_ok());
        }

        for evidence in [
            r#"["provider:asr"]"#,
            r#"[{"kind":"asr_provider","provider":"unknown"}]"#,
            r#"[{"kind":"fact_card_field","field":"sku"}]"#,
            r#"[{"kind":"unknown"}]"#,
            r#"[{"kind":"fact_card_field","field":"{\"sku\":\"A4PRO2\"}"}]"#,
            r#"[{"kind":"asr_provider","provider":"Bearer volcengine"}]"#,
            r#"[{"kind":"human_review","note":"Authorization: Bearer token"}]"#,
        ] {
            assert!(serde_json::from_str::<Vec<CandidateEvidence>>(evidence).is_err());
        }

        for evidence in [
            r#"[{"kind":"asr_provider","field":"model"}]"#,
            r#"[{"kind":"fact_card_field","provider":"whisper"}]"#,
            r#"[{"kind":"human_review","provider":"whisper"}]"#,
        ] {
            let evidence = serde_json::from_str::<Vec<CandidateEvidence>>(evidence).unwrap();
            assert!(super::validate_evidence(&evidence).is_err());
        }

        let human_review = CandidateEvidence {
            kind: CandidateEvidenceKind::HumanReview,
            provider: None,
            field: None,
        };
        assert!(super::validate_evidence(&[]).is_err());
        assert!(super::validate_evidence(&vec![human_review; 9]).is_err());
    }

    #[sqlx::test]
    async fn export_json_serializes_typed_rows(pool: SqlitePool) {
        let db = database(pool).await;
        let row = db
            .upsert_transcript_dictionary_candidate(new_replacement(
                "\u{539f}\u{6587}",
                "\u{76ee}\u{6807}\u{6587}\u{672c}",
            ))
            .await
            .unwrap();

        let exported = export_transcript_dictionary_candidates_json(&[row]).unwrap();
        let value: serde_json::Value = serde_json::from_str(&exported).unwrap();

        assert_eq!(value[0]["candidate_type"], "replacement");
        assert_eq!(value[0]["source_text"], "\u{539f}\u{6587}");
        assert_eq!(value[0]["target_text"], "\u{76ee}\u{6807}\u{6587}\u{672c}");
        assert_eq!(value[0]["status"], "pending");
    }

    #[test]
    fn csv_fields_neutralize_spreadsheet_formulas_exactly() {
        for (input, expected) in [
            ("=1+1", "'=1+1"),
            ("+1", "'+1"),
            ("-1", "'-1"),
            ("@SUM(A1)", "'@SUM(A1)"),
            ("  =1+1", "'  =1+1"),
            ("\t+1", "'\t+1"),
            ("ordinary text", "ordinary text"),
            ("=SUM(A1,A2)", "\"'=SUM(A1,A2)\""),
        ] {
            assert_eq!(escape_csv_field(input), expected, "input: {input:?}");
        }
    }

    #[sqlx::test]
    async fn export_csv_uses_exact_columns_and_escapes_quotes_and_newlines(pool: SqlitePool) {
        let db = database(pool).await;
        let row = db
            .upsert_transcript_dictionary_candidate(new_replacement(
                "\u{539f}\u{6587},\"\u{5f15}\u{7528}\"\n\u{4e0b}\u{4e00}\u{884c}",
                "\u{76ee}\u{6807},\u{6587}\u{672c}",
            ))
            .await
            .unwrap();

        let exported = export_transcript_dictionary_candidates_csv(&[row]);

        assert!(exported.starts_with(
            "\u{7c7b}\u{578b},\u{539f}\u{6587},\u{76ee}\u{6807}\u{6587}\u{672c},\u{72b6}\u{6001},\u{521b}\u{5efa}\u{65f6}\u{95f4}\r\n"
        ));
        assert!(exported.contains(
            "replacement,\"\u{539f}\u{6587},\"\"\u{5f15}\u{7528}\"\"\n\u{4e0b}\u{4e00}\u{884c}\",\"\u{76ee}\u{6807},\u{6587}\u{672c}\",pending,"
        ));
        assert!(exported.ends_with("\r\n"));
    }
}
