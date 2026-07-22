use super::{Database, DatabaseError};
use master_script_store::StoreError;

pub use master_script_store::{
    MasterChunkInput, MasterChunkRow, MasterScriptRow, MasterSectionRow, MasterSourceRow,
    NewMasterSection, NewMasterSource, NewMasterVersion, NewSupportCandidate, SupportCandidateRow,
    MASTER_SCRIPT_MIGRATION_SQL,
};

impl Database {
    pub async fn create_master_source(
        &self,
        input: NewMasterSource,
    ) -> Result<MasterSourceRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::create_master_source(&pool, &input)
            .await
            .map_err(map_store_error)
    }

    pub async fn upsert_master_chunk(
        &self,
        input: MasterChunkInput,
    ) -> Result<MasterChunkRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::upsert_master_chunk(&pool, &input)
            .await
            .map_err(map_store_error)
    }

    pub async fn list_master_chunks(
        &self,
        source_id: i64,
    ) -> Result<Vec<MasterChunkRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::list_master_chunks(&pool, source_id)
            .await
            .map_err(map_store_error)
    }

    pub async fn publish_master_version(
        &self,
        input: NewMasterVersion,
    ) -> Result<MasterScriptRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::publish_master_version(&pool, &input)
            .await
            .map_err(map_store_error)
    }

    pub async fn get_master_version(&self, id: i64) -> Result<MasterScriptRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::get_master_version(&pool, id)
            .await
            .map_err(map_store_error)
    }

    pub async fn list_master_sections(
        &self,
        master_script_id: i64,
    ) -> Result<Vec<MasterSectionRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::list_master_sections(&pool, master_script_id)
            .await
            .map_err(map_store_error)
    }

    pub async fn insert_support_candidate(
        &self,
        input: NewSupportCandidate,
    ) -> Result<SupportCandidateRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::insert_support_candidate(&pool, &input)
            .await
            .map_err(map_store_error)
    }

    pub async fn list_support_candidates(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<SupportCandidateRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::list_support_candidates(&pool, status)
            .await
            .map_err(map_store_error)
    }

    pub async fn decide_support_candidate(
        &self,
        id: i64,
        next_status: &str,
    ) -> Result<SupportCandidateRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        master_script_store::decide_support_candidate(&pool, id, next_status)
            .await
            .map_err(map_store_error)
    }
}

fn map_store_error(error: StoreError) -> DatabaseError {
    match error {
        StoreError::InvalidMasterScriptState(message) => {
            DatabaseError::InvalidMasterScriptState(message)
        }
        StoreError::Database(error) => DatabaseError::DB(error),
    }
}

#[cfg(test)]
mod master_script_database_tests {
    use super::*;
    use master_script::{CandidateAdmission, HardGateResult, MasterSectionKind, ScoreBreakdown};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;

    async fn test_db() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(MASTER_SCRIPT_MIGRATION_SQL).await.unwrap();
        let db = Database::new();
        db.set(pool).await;
        db
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

    async fn seeded_master() -> (Database, MasterScriptRow, MasterSectionRow) {
        let db = test_db().await;
        let source = db.create_master_source(source_input()).await.unwrap();
        let master = db
            .publish_master_version(NewMasterVersion {
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
            })
            .await
            .unwrap();
        let section = db.list_master_sections(master.id).await.unwrap().remove(0);
        (db, master, section)
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
            host_text: "supporting host text".into(),
            comparison_json: "{\"matches\":[\"product\"]}".into(),
            gates_json: serde_json::to_string(&passing_gates()).unwrap(),
            score_json: serde_json::to_string(&score).unwrap(),
            admission: CandidateAdmission::CandidateQueue,
        }
    }

    #[tokio::test]
    async fn chunk_checkpoint_is_idempotent_and_resumable() {
        let db = test_db().await;
        let source = db.create_master_source(source_input()).await.unwrap();
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
        let first = db.upsert_master_chunk(chunk.clone()).await.unwrap();
        chunk.reviewed_srt = "reviewed-v2".into();
        let second = db.upsert_master_chunk(chunk).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.reviewed_srt, "reviewed-v2");
        assert_eq!(db.list_master_chunks(source.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn master_source_rejects_key_reuse_for_different_media() {
        let db = test_db().await;
        db.create_master_source(source_input()).await.unwrap();
        let mut conflict = source_input();
        conflict.media_hash = "other-media".into();
        assert!(matches!(
            db.create_master_source(conflict).await,
            Err(DatabaseError::InvalidMasterScriptState(_))
        ));
    }

    #[tokio::test]
    async fn published_master_versions_are_immutable() {
        let db = test_db().await;
        let source = db.create_master_source(source_input()).await.unwrap();
        let v1 = db
            .publish_master_version(NewMasterVersion {
                script_key: "MS-001".into(),
                version: "1.0.0".into(),
                source_id: source.id,
                title: "master script".into(),
                index_relative_path: "10-master-scripts/MS-001/V1.0.md".into(),
                content_hash: "v1-hash".into(),
                sections: vec![],
            })
            .await
            .unwrap();
        db.publish_master_version(NewMasterVersion {
            script_key: "MS-001".into(),
            version: "1.0.1".into(),
            source_id: source.id,
            title: "master script".into(),
            index_relative_path: "10-master-scripts/MS-001/V1.0.1.md".into(),
            content_hash: "v2-hash".into(),
            sections: vec![],
        })
        .await
        .unwrap();
        let persisted_v1 = db.get_master_version(v1.id).await.unwrap();
        assert_eq!(persisted_v1.version, "1.0.0");
        assert_eq!(persisted_v1.content_hash, "v1-hash");
        assert!(db
            .publish_master_version(NewMasterVersion {
                script_key: "MS-001".into(),
                version: "1.0.0".into(),
                source_id: source.id,
                title: "changed".into(),
                index_relative_path: "changed.md".into(),
                content_hash: "changed-hash".into(),
                sections: vec![],
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn support_candidate_dedupes_identical_payload() {
        let (db, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        let first = db.insert_support_candidate(input.clone()).await.unwrap();
        let second = db.insert_support_candidate(input).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            db.list_support_candidates(Some("pending_review"))
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn support_candidate_rejects_conflicting_duplicate_payload() {
        let (db, master, section) = seeded_master().await;
        let input = queued_candidate(master.id, section.id, 85);
        db.insert_support_candidate(input.clone()).await.unwrap();
        let mut conflict = input;
        conflict.host_text = "changed host text".into();
        assert!(matches!(
            db.insert_support_candidate(conflict).await,
            Err(DatabaseError::InvalidMasterScriptState(_))
        ));
    }

    #[tokio::test]
    async fn score_84_cannot_be_persisted_as_queued() {
        let (db, master, section) = seeded_master().await;
        let error = db
            .insert_support_candidate(queued_candidate(master.id, section.id, 84))
            .await
            .unwrap_err();
        assert!(matches!(error, DatabaseError::InvalidMasterScriptState(_)));
    }

    #[tokio::test]
    async fn candidate_decisions_allow_only_pending_review_transitions() {
        let (db, master, section) = seeded_master().await;
        let candidate = db
            .insert_support_candidate(queued_candidate(master.id, section.id, 85))
            .await
            .unwrap();
        let approved = db
            .decide_support_candidate(candidate.id, "approved")
            .await
            .unwrap();
        assert_eq!(approved.status, "approved");
        assert!(matches!(
            db.decide_support_candidate(candidate.id, "merged").await,
            Err(DatabaseError::InvalidMasterScriptState(_))
        ));
    }
}
