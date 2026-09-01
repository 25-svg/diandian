use recorder::platforms::PlatformType;

use super::Database;
use super::DatabaseError;
use chrono::Utc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct RecordRow {
    pub platform: String,
    pub parent_id: String,
    pub live_id: String,
    pub room_id: String,
    pub title: String,
    pub length: f64,
    pub size: i64,
    pub created_at: String,
    pub cover: Option<String>,
    pub anchor_name: String,
    pub anchor_source: String,
    pub anchor_confidence: String,
    pub anchor_detection_status: String,
    pub anchor_detection_error: String,
    pub anchor_detected_at: String,
    pub archive_kind: String,
    pub classification_source: String,
    pub source_path: String,
}

pub const RECORD_SOURCE_PATH_MIGRATION_SQL: &str = r#"
ALTER TABLE records ADD COLUMN source_path TEXT NOT NULL DEFAULT '';
"#;

pub const RECORD_ARCHIVE_KIND_MIGRATION_SQL: &str = r#"
ALTER TABLE records ADD COLUMN archive_kind TEXT NOT NULL DEFAULT 'competitor';
ALTER TABLE records ADD COLUMN classification_source TEXT NOT NULL DEFAULT 'auto_rule';
UPDATE records
SET archive_kind = CASE WHEN title LIKE '%金典拍拍%' OR anchor_name LIKE '%金典拍拍%' THEN 'company' ELSE 'competitor' END,
    classification_source = 'auto_rule'
WHERE classification_source = 'auto_rule';
CREATE INDEX idx_records_archive_kind ON records(archive_kind, created_at DESC);
"#;

pub const RECORD_ANCHOR_DETECTION_MIGRATION_SQL: &str = r#"
ALTER TABLE records ADD COLUMN anchor_name TEXT NOT NULL DEFAULT '';
ALTER TABLE records ADD COLUMN anchor_source TEXT NOT NULL DEFAULT '';
ALTER TABLE records ADD COLUMN anchor_confidence TEXT NOT NULL DEFAULT '';
ALTER TABLE records ADD COLUMN anchor_detection_status TEXT NOT NULL DEFAULT 'not_requested';
ALTER TABLE records ADD COLUMN anchor_detection_error TEXT NOT NULL DEFAULT '';
ALTER TABLE records ADD COLUMN anchor_detected_at TEXT NOT NULL DEFAULT '';
CREATE INDEX idx_records_anchor_detection_status
ON records(anchor_detection_status, anchor_source);
"#;

// CREATE TABLE records (live_id INTEGER PRIMARY KEY, room_id TEXT, title TEXT, length INTEGER, size INTEGER, created_at TEXT);
impl Database {
    pub async fn get_records(
        &self,
        room_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<RecordRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, RecordRow>(
            "SELECT * FROM records WHERE room_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(room_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&lock)
        .await?)
    }

    pub async fn get_record(
        &self,
        room_id: &str,
        live_id: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE room_id = $1 and live_id = $2")
            .bind(room_id)
            .bind(live_id)
            .fetch_one(&lock)
            .await
            .map_err(|error| match error {
                sqlx::Error::RowNotFound => DatabaseError::NotFound,
                error => DatabaseError::DB(error),
            })
    }

    pub async fn get_archives_by_parent_id(
        &self,
        room_id: &str,
        parent_id: &str,
    ) -> Result<Vec<RecordRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, RecordRow>(
            "SELECT * FROM records WHERE room_id = $1 and parent_id = $2",
        )
        .bind(room_id)
        .bind(parent_id)
        .fetch_all(&lock)
        .await?)
    }

    pub async fn add_record(
        &self,
        platform: PlatformType,
        parent_id: &str,
        live_id: &str,
        room_id: &str,
        title: &str,
        cover: Option<String>,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let record = RecordRow {
            platform: platform.as_str().to_string(),
            parent_id: parent_id.to_string(),
            live_id: live_id.to_string(),
            room_id: room_id.to_string(),
            title: title.into(),
            length: 0.0,
            size: 0,
            created_at: Utc::now().to_rfc3339().to_string(),
            cover,
            anchor_name: String::new(),
            anchor_source: String::new(),
            anchor_confidence: String::new(),
            anchor_detection_status: "pending".to_string(),
            anchor_detection_error: String::new(),
            anchor_detected_at: String::new(),
            archive_kind: if title.contains("金典拍拍") {
                "company"
            } else {
                "competitor"
            }
            .to_string(),
            classification_source: "auto_rule".to_string(),
            source_path: String::new(),
        };
        if let Err(e) = sqlx::query("INSERT INTO records (live_id, room_id, title, length, size, cover, created_at, platform, parent_id, anchor_name, anchor_source, anchor_confidence, anchor_detection_status, anchor_detection_error, anchor_detected_at, archive_kind, classification_source, source_path) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)").bind(record.live_id.clone())
            .bind(&record.room_id).bind(&record.title).bind(0).bind(0).bind(&record.cover).bind(&record.created_at).bind(platform.as_str().to_string()).bind(parent_id)
            .bind(&record.anchor_name).bind(&record.anchor_source).bind(&record.anchor_confidence).bind(&record.anchor_detection_status).bind(&record.anchor_detection_error).bind(&record.anchor_detected_at).bind(&record.archive_kind).bind(&record.classification_source).bind(&record.source_path).execute(&lock).await {
                // if the record already exists, return the existing record
                if e.to_string().contains("UNIQUE constraint failed") {
                    return self.get_record(room_id, live_id).await;
                }
            }
        Ok(record)
    }

    pub async fn update_record_source_path(
        &self,
        room_id: &str,
        live_id: &str,
        source_path: &str,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("UPDATE records SET source_path = $1 WHERE room_id = $2 AND live_id = $3")
            .bind(source_path)
            .bind(room_id)
            .bind(live_id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn set_record_archive_kind(
        &self,
        live_id: &str,
        archive_kind: &str,
    ) -> Result<RecordRow, DatabaseError> {
        if !matches!(archive_kind, "company" | "competitor") {
            return Err(DatabaseError::InvalidMasterScriptState(
                "invalid archive kind".into(),
            ));
        }
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query_as::<_, RecordRow>(
            "UPDATE records SET archive_kind = $1, classification_source = 'manual' WHERE live_id = $2 RETURNING *",
        ).bind(archive_kind).bind(live_id).fetch_one(&lock).await.map_err(Into::into)
    }

    /// Re-run the automatic rule with the monitored account/store name. Manual
    /// classifications are a user decision and must never be overwritten.
    pub async fn auto_classify_record_archive_kind(
        &self,
        live_id: &str,
        account_name: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let updated = sqlx::query_as::<_, RecordRow>(
            "UPDATE records
             SET archive_kind = CASE
                 WHEN title LIKE '%金典拍拍%'
                   OR anchor_name LIKE '%金典拍拍%'
                   OR $1 LIKE '%金典拍拍%'
                 THEN 'company' ELSE 'competitor' END,
                 classification_source = 'auto_rule'
             WHERE live_id = $2 AND classification_source = 'auto_rule'
             RETURNING *",
        )
        .bind(account_name)
        .bind(live_id)
        .fetch_optional(&lock)
        .await?;
        if let Some(record) = updated {
            Ok(record)
        } else {
            self.get_record_by_live_id(live_id).await
        }
    }

    pub(crate) async fn get_record_by_live_id(
        &self,
        live_id: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE live_id = $1")
            .bind(live_id)
            .fetch_one(&lock)
            .await
            .map_err(Into::into)
    }

    pub async fn set_record_anchor_detection_running(
        &self,
        live_id: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE records
             SET anchor_detection_status = 'running',
                 anchor_detection_error = ''
             WHERE live_id = $1 AND anchor_source <> 'manual'",
        )
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(
            sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE live_id = $1")
                .bind(live_id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn save_record_anchor_detection(
        &self,
        live_id: &str,
        anchor_name: &str,
        confidence: &str,
        status: &str,
        error: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE records
             SET anchor_name = $1,
                 anchor_source = 'minimax_vision',
                 anchor_confidence = $2,
                 anchor_detection_status = $3,
                 anchor_detection_error = $4,
                 anchor_detected_at = datetime('now')
             WHERE live_id = $5 AND anchor_source <> 'manual'",
        )
        .bind(anchor_name)
        .bind(confidence)
        .bind(status)
        .bind(error)
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(
            sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE live_id = $1")
                .bind(live_id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn save_record_anchor_manual(
        &self,
        live_id: &str,
        anchor_name: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE records
             SET anchor_name = $1,
                 anchor_source = 'manual',
                 anchor_confidence = 'high',
                 anchor_detection_status = 'confirmed',
                 anchor_detection_error = '',
                 anchor_detected_at = datetime('now')
             WHERE live_id = $2",
        )
        .bind(anchor_name.trim())
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(
            sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE live_id = $1")
                .bind(live_id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn clear_record_anchor_manual(
        &self,
        live_id: &str,
    ) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE records
             SET anchor_name = '',
                 anchor_source = '',
                 anchor_confidence = '',
                 anchor_detection_status = 'pending',
                 anchor_detection_error = '',
                 anchor_detected_at = ''
             WHERE live_id = $1 AND anchor_source = 'manual'",
        )
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(
            sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE live_id = $1")
                .bind(live_id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn remove_record(&self, live_id: &str) -> Result<RecordRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let to_delete = sqlx::query_as::<_, RecordRow>("SELECT * FROM records WHERE live_id = $1")
            .bind(live_id)
            .fetch_one(&lock)
            .await?;
        sqlx::query("DELETE FROM records WHERE live_id = $1")
            .bind(live_id)
            .execute(&lock)
            .await?;
        Ok(to_delete)
    }

    /// Remove a record when present. Returns `None` if the index row is already gone.
    pub async fn try_remove_record(
        &self,
        room_id: &str,
        live_id: &str,
    ) -> Result<Option<RecordRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let Some(to_delete) = sqlx::query_as::<_, RecordRow>(
            "SELECT * FROM records WHERE room_id = $1 AND live_id = $2",
        )
        .bind(room_id)
        .bind(live_id)
        .fetch_optional(&lock)
        .await?
        else {
            return Ok(None);
        };
        sqlx::query("DELETE FROM records WHERE live_id = $1")
            .bind(live_id)
            .execute(&lock)
            .await?;
        Ok(Some(to_delete))
    }

    pub async fn update_record_delta(
        &self,
        live_id: &str,
        length: f64,
        size: u64,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let size = i64::try_from(size).map_err(|_| DatabaseError::NumberExceedI64Range)?;
        sqlx::query("UPDATE records SET length = length + $1, size = size + $2 WHERE live_id = $3")
            .bind(length)
            .bind(size)
            .bind(live_id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn update_record_parent_id(
        &self,
        live_id: &str,
        parent_id: &str,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("UPDATE records SET parent_id = $1 WHERE live_id = $2")
            .bind(parent_id)
            .bind(live_id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn update_record_cover(
        &self,
        live_id: &str,
        cover: Option<String>,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("UPDATE records SET cover = $1 WHERE live_id = $2")
            .bind(cover)
            .bind(live_id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn get_total_length(&self) -> Result<f64, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result: (f64,) = sqlx::query_as("SELECT SUM(length) FROM records;")
            .fetch_one(&lock)
            .await?;
        Ok(result.0)
    }

    pub async fn get_today_record_count(&self) -> Result<i64, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM records WHERE created_at >= $1;")
            .bind(
                Utc::now()
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .to_string(),
            )
            .fetch_one(&lock)
            .await?;
        Ok(result.0)
    }

    pub async fn get_recent_record(
        &self,
        room_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<RecordRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        if room_id.is_empty() {
            Ok(sqlx::query_as::<_, RecordRow>(
                "SELECT * FROM records ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&lock)
            .await?)
        } else {
            Ok(sqlx::query_as::<_, RecordRow>(
                "SELECT * FROM records WHERE room_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(room_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&lock)
            .await?)
        }
    }

    pub async fn get_record_disk_usage(&self) -> Result<i64, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result: (i64,) = sqlx::query_as("SELECT SUM(size) FROM records;")
            .fetch_one(&lock)
            .await?;
        Ok(result.0)
    }
}
