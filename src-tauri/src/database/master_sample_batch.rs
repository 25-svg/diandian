use super::{Database, DatabaseError};

pub const MASTER_SAMPLE_BATCH_MIGRATION_SQL: &str = r#"
CREATE TABLE master_sample_batches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_key TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    host_label TEXT NOT NULL DEFAULT '',
    target_sample_count INTEGER NOT NULL DEFAULT 6,
    status TEXT NOT NULL DEFAULT 'collecting'
        CHECK (status IN ('collecting', 'processing', 'ready_for_synthesis', 'draft_ready', 'published', 'failed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE master_sample_batch_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id INTEGER NOT NULL REFERENCES master_sample_batches(id) ON DELETE CASCADE,
    video_id INTEGER NOT NULL REFERENCES videos(id) ON DELETE RESTRICT,
    source_id INTEGER REFERENCES master_sources(id) ON DELETE SET NULL,
    processing_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (processing_status IN ('pending', 'transcribing', 'reviewing', 'ready', 'failed')),
    error TEXT,
    display_order INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (batch_id, video_id)
);

CREATE INDEX idx_master_sample_batch_items_batch_id
    ON master_sample_batch_items(batch_id, display_order);
CREATE INDEX idx_master_sample_batch_items_video_id
    ON master_sample_batch_items(video_id);
"#;

pub const MASTER_SAMPLE_BATCH_SYNTHESIS_MIGRATION_SQL: &str = r#"
CREATE TABLE master_sample_batch_syntheses (
    batch_id INTEGER PRIMARY KEY REFERENCES master_sample_batches(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'generating'
        CHECK (status IN ('generating', 'ready', 'failed', 'published')),
    draft_json TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    generated_at TEXT,
    published_at TEXT
);
"#;

pub const MASTER_SAMPLE_BATCH_PURPOSE_MIGRATION_SQL: &str = r#"
ALTER TABLE master_sample_batches
    ADD COLUMN purpose TEXT NOT NULL DEFAULT 'sample'
    CHECK (purpose IN ('sample', 'enterprise'));
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterSampleBatchRow {
    pub id: i64,
    pub batch_key: String,
    pub title: String,
    pub host_label: String,
    pub purpose: String,
    pub target_sample_count: i64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterSampleBatchSummary {
    #[sqlx(flatten)]
    pub batch: MasterSampleBatchRow,
    pub sample_count: i64,
    pub ready_count: i64,
    pub failed_count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterSampleBatchItemRow {
    pub id: i64,
    pub batch_id: i64,
    pub video_id: i64,
    pub source_id: Option<i64>,
    pub processing_status: String,
    pub error: Option<String>,
    pub display_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub video_title: String,
    pub video_length: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MasterSampleBatchSynthesisRow {
    pub batch_id: i64,
    pub status: String,
    pub draft_json: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub generated_at: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterSampleBatchDetail {
    pub batch: MasterSampleBatchRow,
    pub items: Vec<MasterSampleBatchItemRow>,
    pub synthesis: Option<MasterSampleBatchSynthesisRow>,
    #[serde(default)]
    pub correctable_model_format_issue_count: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewMasterSampleBatch {
    pub batch_key: String,
    pub title: String,
    pub host_label: String,
    pub purpose: String,
    pub target_sample_count: i64,
    pub video_ids: Vec<i64>,
}

impl Database {
    pub async fn create_master_sample_batch(
        &self,
        input: NewMasterSampleBatch,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        if input.video_ids.is_empty() {
            return Err(DatabaseError::InvalidMasterScriptState(
                "母稿样本批次至少需要一场直播".into(),
            ));
        }
        if !(1..=20).contains(&input.target_sample_count) {
            return Err(DatabaseError::InvalidMasterScriptState(
                "样本目标数量必须在 1 到 20 场之间".into(),
            ));
        }
        if !matches!(input.purpose.as_str(), "sample" | "enterprise") {
            return Err(DatabaseError::InvalidMasterScriptState(
                "母稿批次用途只能是样本或企业母稿".into(),
            ));
        }
        if input.video_ids.len()
            != input
                .video_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        {
            return Err(DatabaseError::InvalidMasterScriptState(
                "母稿样本批次中存在重复视频".into(),
            ));
        }

        let pool = self.db.read().await.clone().unwrap();
        let mut transaction = pool.begin().await?;
        for video_id in &input.video_ids {
            let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM videos WHERE id = $1")
                .bind(video_id)
                .fetch_one(&mut *transaction)
                .await?;
            if exists != 1 {
                return Err(DatabaseError::InvalidMasterScriptState(format!(
                    "视频 {video_id} 不存在，无法加入母稿样本批次"
                )));
            }
        }

        let result = sqlx::query(
            r#"INSERT INTO master_sample_batches (
                batch_key, title, host_label, purpose, target_sample_count, status, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, 'collecting', datetime('now'), datetime('now'))"#,
        )
        .bind(input.batch_key.trim())
        .bind(input.title.trim())
        .bind(input.host_label.trim())
        .bind(input.purpose.trim())
        .bind(input.target_sample_count)
        .execute(&mut *transaction)
        .await?;
        let batch_id = result.last_insert_rowid();

        for (index, video_id) in input.video_ids.iter().enumerate() {
            let source_key = format!("video:{video_id}");
            let source_id = sqlx::query_scalar::<_, i64>(
                "SELECT id FROM master_sources WHERE source_key = $1 LIMIT 1",
            )
            .bind(source_key)
            .fetch_optional(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO master_sample_batch_items (
                    batch_id, video_id, source_id, processing_status, display_order, created_at, updated_at
                ) VALUES ($1, $2, $3, 'pending', $4, datetime('now'), datetime('now'))"#,
            )
            .bind(batch_id)
            .bind(video_id)
            .bind(source_id)
            .bind(i64::try_from(index + 1).map_err(|_| DatabaseError::NumberExceedI64Range)?)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        self.get_master_sample_batch(batch_id).await
    }

    pub async fn get_master_sample_batch(
        &self,
        batch_id: i64,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        // A desktop restart aborts in-flight commands. Mark a synthesis that
        // outlived the MiniMax request timeout as retryable instead of leaving
        // the batch permanently displayed as "generating".
        sqlx::query(
            r#"UPDATE master_sample_batch_syntheses
            SET status = 'failed',
                error = '母稿草稿生成已超时或被应用重启中断，请重新生成。',
                updated_at = datetime('now')
            WHERE batch_id = $1
              AND status = 'generating'
              AND updated_at < datetime('now', '-4 minutes')"#,
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        let batch = sqlx::query_as::<_, MasterSampleBatchRow>(
            "SELECT * FROM master_sample_batches WHERE id = $1",
        )
        .bind(batch_id)
        .fetch_one(&pool)
        .await?;
        let items = sqlx::query_as::<_, MasterSampleBatchItemRow>(
            r#"SELECT
                items.id, items.batch_id, items.video_id, items.source_id,
                items.processing_status, items.error, items.display_order,
                items.created_at, items.updated_at,
                COALESCE(NULLIF(videos.title, ''), videos.file) AS video_title,
                videos.length AS video_length
            FROM master_sample_batch_items AS items
            INNER JOIN videos ON videos.id = items.video_id
            WHERE items.batch_id = $1
            ORDER BY items.display_order ASC"#,
        )
        .bind(batch_id)
        .fetch_all(&pool)
        .await?;
        let synthesis = sqlx::query_as::<_, MasterSampleBatchSynthesisRow>(
            "SELECT * FROM master_sample_batch_syntheses WHERE batch_id = $1",
        )
        .bind(batch_id)
        .fetch_optional(&pool)
        .await?;
        Ok(MasterSampleBatchDetail {
            batch,
            items,
            synthesis,
            correctable_model_format_issue_count: None,
        })
    }

    pub async fn begin_master_sample_batch_processing(
        &self,
        batch_id: i64,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let updated = sqlx::query(
            r#"UPDATE master_sample_batches
            SET status = 'processing', updated_at = datetime('now')
            WHERE id = $1
              AND status != 'published'"#,
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DatabaseError::InvalidMasterScriptState(
                "已发布的企业母稿批次不能再次执行逐场处理".into(),
            ));
        }
        self.get_master_sample_batch(batch_id).await
    }

    pub async fn update_master_sample_batch_item(
        &self,
        batch_id: i64,
        item_id: i64,
        source_id: Option<i64>,
        processing_status: &str,
        error: Option<&str>,
    ) -> Result<(), DatabaseError> {
        if !matches!(
            processing_status,
            "pending" | "transcribing" | "reviewing" | "ready" | "failed"
        ) {
            return Err(DatabaseError::InvalidMasterScriptState(format!(
                "未知的母稿样本处理状态: {processing_status}"
            )));
        }
        let pool = self.db.read().await.clone().unwrap();
        let updated = sqlx::query(
            r#"UPDATE master_sample_batch_items
            SET source_id = COALESCE($1, source_id),
                processing_status = $2,
                error = $3,
                updated_at = datetime('now')
            WHERE id = $4 AND batch_id = $5"#,
        )
        .bind(source_id)
        .bind(processing_status)
        .bind(error)
        .bind(item_id)
        .bind(batch_id)
        .execute(&pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DatabaseError::InvalidMasterScriptState(
                "母稿样本场次不存在或不属于当前批次".into(),
            ));
        }
        Ok(())
    }

    pub async fn finish_master_sample_batch_processing(
        &self,
        batch_id: i64,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let (unfinished, failed): (i64, i64) = sqlx::query_as(
            r#"SELECT
                SUM(CASE WHEN processing_status IN ('pending', 'transcribing', 'reviewing') THEN 1 ELSE 0 END),
                SUM(CASE WHEN processing_status = 'failed' THEN 1 ELSE 0 END)
            FROM master_sample_batch_items
            WHERE batch_id = $1"#,
        )
        .bind(batch_id)
        .fetch_one(&pool)
        .await?;
        let status = if unfinished > 0 {
            "processing"
        } else if failed > 0 {
            "failed"
        } else {
            "ready_for_synthesis"
        };
        sqlx::query(
            "UPDATE master_sample_batches SET status = $1, updated_at = datetime('now') WHERE id = $2",
        )
        .bind(status)
        .bind(batch_id)
        .execute(&pool)
        .await?;
        self.get_master_sample_batch(batch_id).await
    }

    pub async fn list_master_sample_batches(
        &self,
    ) -> Result<Vec<MasterSampleBatchSummary>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, MasterSampleBatchSummary>(
            r#"SELECT
                batches.id, batches.batch_key, batches.title, batches.host_label,
                batches.purpose, batches.target_sample_count, batches.status, batches.created_at, batches.updated_at,
                COUNT(items.id) AS sample_count,
                SUM(CASE WHEN items.processing_status = 'ready' THEN 1 ELSE 0 END) AS ready_count,
                SUM(CASE WHEN items.processing_status = 'failed' THEN 1 ELSE 0 END) AS failed_count
            FROM master_sample_batches AS batches
            LEFT JOIN master_sample_batch_items AS items ON items.batch_id = batches.id
            GROUP BY batches.id
            ORDER BY batches.updated_at DESC, batches.id DESC"#,
        )
        .fetch_all(&pool)
        .await?)
    }

    pub async fn list_unbatched_master_source_video_ids(&self) -> Result<Vec<i64>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT videos.id
            FROM master_sources
            INNER JOIN videos ON master_sources.source_key = ('video:' || videos.id)
            WHERE master_sources.source_kind = 'video'
              AND NOT EXISTS (
                SELECT 1
                FROM master_sample_batch_items
                WHERE master_sample_batch_items.video_id = videos.id
              )
            ORDER BY master_sources.id ASC"#,
        )
        .fetch_all(&pool)
        .await?)
    }

    pub async fn find_master_source_id_for_video(
        &self,
        video_id: i64,
    ) -> Result<Option<i64>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        sqlx::query_scalar::<_, i64>("SELECT id FROM master_sources WHERE source_key = $1 LIMIT 1")
            .bind(format!("video:{video_id}"))
            .fetch_optional(&pool)
            .await
            .map_err(DatabaseError::from)
    }

    pub async fn begin_master_sample_batch_synthesis(
        &self,
        batch_id: i64,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let (total, ready): (i64, i64) = sqlx::query_as(
            r#"SELECT COUNT(*), SUM(CASE WHEN processing_status = 'ready' THEN 1 ELSE 0 END)
            FROM master_sample_batch_items WHERE batch_id = $1"#,
        )
        .bind(batch_id)
        .fetch_one(&pool)
        .await?;
        if total == 0 || total != ready {
            return Err(DatabaseError::InvalidMasterScriptState(
                "所有样本完成逐场整理后才能生成最终母稿".into(),
            ));
        }
        let updated = sqlx::query(
            r#"UPDATE master_sample_batches
            SET status = 'ready_for_synthesis', updated_at = datetime('now')
            WHERE id = $1 AND status != 'published'"#,
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DatabaseError::InvalidMasterScriptState(
                "已发布的企业母稿批次不能重新生成草稿".into(),
            ));
        }
        sqlx::query(
            r#"INSERT INTO master_sample_batch_syntheses (
                batch_id, status, draft_json, error, created_at, updated_at, generated_at
            ) VALUES ($1, 'generating', NULL, NULL, datetime('now'), datetime('now'), NULL)
            ON CONFLICT(batch_id) DO UPDATE SET
                status = 'generating', draft_json = NULL, error = NULL,
                updated_at = datetime('now'), generated_at = NULL, published_at = NULL"#,
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        self.get_master_sample_batch(batch_id).await
    }

    pub async fn finish_master_sample_batch_synthesis(
        &self,
        batch_id: i64,
        draft_json: &str,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        sqlx::query(
            r#"UPDATE master_sample_batch_syntheses
            SET status = 'ready', draft_json = $1, error = NULL,
                updated_at = datetime('now'), generated_at = datetime('now')
            WHERE batch_id = $2"#,
        )
        .bind(draft_json)
        .bind(batch_id)
        .execute(&pool)
        .await?;
        sqlx::query(
            "UPDATE master_sample_batches SET status = 'draft_ready', updated_at = datetime('now') WHERE id = $1",
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        self.get_master_sample_batch(batch_id).await
    }

    pub async fn fail_master_sample_batch_synthesis(
        &self,
        batch_id: i64,
        error: &str,
    ) -> Result<(), DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        sqlx::query(
            r#"UPDATE master_sample_batch_syntheses
            SET status = 'failed', error = $1, updated_at = datetime('now')
            WHERE batch_id = $2"#,
        )
        .bind(error)
        .bind(batch_id)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn mark_master_sample_batch_published(
        &self,
        batch_id: i64,
    ) -> Result<MasterSampleBatchDetail, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE master_sample_batch_syntheses SET status = 'published', updated_at = datetime('now'), published_at = datetime('now') WHERE batch_id = $1",
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        sqlx::query(
            "UPDATE master_sample_batches SET status = 'published', updated_at = datetime('now') WHERE id = $1",
        )
        .bind(batch_id)
        .execute(&pool)
        .await?;
        self.get_master_sample_batch(batch_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;

    async fn test_db() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(
            "CREATE TABLE videos (id INTEGER PRIMARY KEY, title TEXT, file TEXT, length INTEGER)",
        )
        .await
        .unwrap();
        pool.execute(
            "CREATE TABLE master_sources (id INTEGER PRIMARY KEY, source_key TEXT UNIQUE, source_kind TEXT)",
        )
        .await
        .unwrap();
        pool.execute(MASTER_SAMPLE_BATCH_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(MASTER_SAMPLE_BATCH_SYNTHESIS_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(MASTER_SAMPLE_BATCH_PURPOSE_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute("INSERT INTO videos (id, title, file, length) VALUES (1, '第一场', 'one.ts', 3600), (2, '第二场', 'two.ts', 7200)")
            .await
            .unwrap();
        let db = Database::new();
        db.set(pool).await;
        db
    }

    #[tokio::test]
    async fn creates_a_batch_and_keeps_selected_videos_in_order() {
        let db = test_db().await;
        let detail = db
            .create_master_sample_batch(NewMasterSampleBatch {
                batch_key: "MSB-001".into(),
                title: "头牌主播样本".into(),
                host_label: "头牌主播".into(),
                purpose: "sample".into(),
                target_sample_count: 6,
                video_ids: vec![2, 1],
            })
            .await
            .unwrap();
        assert_eq!(detail.items.len(), 2);
        assert_eq!(detail.batch.purpose, "sample");
        assert_eq!(detail.items[0].video_id, 2);
        assert_eq!(detail.items[1].video_title, "第一场");
        let summaries = db.list_master_sample_batches().await.unwrap();
        assert_eq!(summaries[0].sample_count, 2);
        assert_eq!(summaries[0].ready_count, 0);
    }

    #[tokio::test]
    async fn lists_existing_master_sources_not_yet_in_a_batch() {
        let db = test_db().await;
        let pool = db.db.read().await.clone().unwrap();
        sqlx::query(
            "INSERT INTO master_sources (id, source_key, source_kind) VALUES (7, 'video:1', 'video')",
        )
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            db.list_unbatched_master_source_video_ids().await.unwrap(),
            vec![1]
        );
        db.create_master_sample_batch(NewMasterSampleBatch {
            batch_key: "MSB-source".into(),
            title: "source".into(),
            host_label: "".into(),
            purpose: "sample".into(),
            target_sample_count: 6,
            video_ids: vec![1],
        })
        .await
        .unwrap();
        assert!(db
            .list_unbatched_master_source_video_ids()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn rejects_empty_or_duplicate_video_selection() {
        let db = test_db().await;
        let empty = NewMasterSampleBatch {
            batch_key: "MSB-empty".into(),
            title: "empty".into(),
            host_label: "".into(),
            purpose: "sample".into(),
            target_sample_count: 6,
            video_ids: vec![],
        };
        assert!(db.create_master_sample_batch(empty).await.is_err());
        let duplicate = NewMasterSampleBatch {
            batch_key: "MSB-duplicate".into(),
            title: "duplicate".into(),
            host_label: "".into(),
            purpose: "sample".into(),
            target_sample_count: 6,
            video_ids: vec![1, 1],
        };
        assert!(db.create_master_sample_batch(duplicate).await.is_err());
    }

    #[tokio::test]
    async fn batch_processing_progress_is_persistent_and_derived_from_items() {
        let db = test_db().await;
        let detail = db
            .create_master_sample_batch(NewMasterSampleBatch {
                batch_key: "MSB-progress".into(),
                title: "progress".into(),
                host_label: "".into(),
                purpose: "enterprise".into(),
                target_sample_count: 2,
                video_ids: vec![1, 2],
            })
            .await
            .unwrap();
        let processing = db
            .begin_master_sample_batch_processing(detail.batch.id)
            .await
            .unwrap();
        assert_eq!(processing.batch.status, "processing");
        db.update_master_sample_batch_item(
            detail.batch.id,
            detail.items[0].id,
            None,
            "ready",
            None,
        )
        .await
        .unwrap();
        db.update_master_sample_batch_item(
            detail.batch.id,
            detail.items[1].id,
            None,
            "ready",
            None,
        )
        .await
        .unwrap();
        let finished = db
            .finish_master_sample_batch_processing(detail.batch.id)
            .await
            .unwrap();
        assert_eq!(finished.batch.status, "ready_for_synthesis");
        assert!(finished
            .items
            .iter()
            .all(|item| item.processing_status == "ready"));
    }

    #[tokio::test]
    async fn synthesis_requires_ready_items_and_persists_a_reviewable_draft() {
        let db = test_db().await;
        let detail = db
            .create_master_sample_batch(NewMasterSampleBatch {
                batch_key: "MSB-synthesis".into(),
                title: "batch".into(),
                host_label: "host".into(),
                purpose: "enterprise".into(),
                target_sample_count: 2,
                video_ids: vec![1, 2],
            })
            .await
            .unwrap();
        assert!(db
            .begin_master_sample_batch_synthesis(detail.batch.id)
            .await
            .is_err());
        for item in &detail.items {
            db.update_master_sample_batch_item(detail.batch.id, item.id, None, "ready", None)
                .await
                .unwrap();
        }
        db.begin_master_sample_batch_synthesis(detail.batch.id)
            .await
            .unwrap();
        let finished = db
            .finish_master_sample_batch_synthesis(detail.batch.id, r#"{"title":"draft"}"#)
            .await
            .unwrap();
        assert_eq!(finished.batch.status, "draft_ready");
        assert_eq!(finished.synthesis.unwrap().status, "ready");
    }

    #[tokio::test]
    async fn stale_synthesis_is_recovered_as_retryable() {
        let db = test_db().await;
        let pool = db.db.read().await.clone().unwrap();
        sqlx::query(
            "INSERT INTO master_sources (id, source_key, source_kind) VALUES (1, 'video:1', 'video')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let detail = db
            .create_master_sample_batch(NewMasterSampleBatch {
                batch_key: "MSB-stale-synthesis".into(),
                title: "batch".into(),
                host_label: "host".into(),
                purpose: "sample".into(),
                target_sample_count: 1,
                video_ids: vec![1],
            })
            .await
            .unwrap();
        db.update_master_sample_batch_item(
            detail.batch.id,
            detail.items[0].id,
            Some(1),
            "ready",
            None,
        )
        .await
        .unwrap();
        db.begin_master_sample_batch_synthesis(detail.batch.id)
            .await
            .unwrap();

        sqlx::query(
            "UPDATE master_sample_batch_syntheses SET updated_at = datetime('now', '-5 minutes') WHERE batch_id = $1",
        )
        .bind(detail.batch.id)
        .execute(&pool)
        .await
        .unwrap();

        let recovered = db.get_master_sample_batch(detail.batch.id).await.unwrap();
        let synthesis = recovered.synthesis.unwrap();
        assert_eq!(synthesis.status, "failed");
        assert!(synthesis.error.unwrap().contains("超时或被应用重启中断"));
    }
}
