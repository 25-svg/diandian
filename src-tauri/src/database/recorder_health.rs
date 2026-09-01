use super::{Database, DatabaseError};

pub const RECORDER_HEALTH_MIGRATION_SQL: &str = r#"
CREATE TABLE recorder_health (
  platform TEXT NOT NULL,
  room_id TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'initializing',
  error_code TEXT NOT NULL DEFAULT '',
  message TEXT NOT NULL DEFAULT '',
  retry_count INTEGER NOT NULL DEFAULT 0,
  next_retry_at TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (platform, room_id)
);
CREATE INDEX idx_recorder_health_status
ON recorder_health(status, updated_at);
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecorderHealthRow {
    pub platform: String,
    pub room_id: String,
    pub status: String,
    pub error_code: String,
    pub message: String,
    pub retry_count: i64,
    pub next_retry_at: Option<String>,
    pub updated_at: String,
}

impl Database {
    pub async fn upsert_recorder_health(
        &self,
        platform: &str,
        room_id: &str,
        status: &str,
        error_code: &str,
        message: &str,
        retry_count: i64,
        next_retry_at: Option<&str>,
    ) -> Result<(), DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query(
            r#"
            INSERT INTO recorder_health
              (platform, room_id, status, error_code, message, retry_count, next_retry_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, datetime('now'))
            ON CONFLICT(platform, room_id) DO UPDATE SET
              status = excluded.status,
              error_code = excluded.error_code,
              message = excluded.message,
              retry_count = excluded.retry_count,
              next_retry_at = excluded.next_retry_at,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(platform)
        .bind(room_id)
        .bind(status)
        .bind(error_code)
        .bind(message)
        .bind(retry_count)
        .bind(next_retry_at)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn list_recorder_health(&self) -> Result<Vec<RecorderHealthRow>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        Ok(sqlx::query_as::<_, RecorderHealthRow>(
            "SELECT platform, room_id, status, error_code, message, retry_count, next_retry_at, updated_at FROM recorder_health ORDER BY platform, room_id",
        )
        .fetch_all(&pool)
        .await?)
    }

    pub async fn delete_recorder_health(
        &self,
        platform: &str,
        room_id: &str,
    ) -> Result<(), DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query("DELETE FROM recorder_health WHERE platform = $1 AND room_id = $2")
            .bind(platform)
            .bind(room_id)
            .execute(&pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;

    async fn database() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(RECORDER_HEALTH_MIGRATION_SQL).await.unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    #[tokio::test]
    async fn health_updates_are_idempotent_per_room() {
        let database = database().await;
        database
            .upsert_recorder_health(
                "douyin",
                "123",
                "live_waiting",
                "REC-LIVE-NOT-RECORDING",
                "等待录制启动",
                0,
                Some("2099-01-01 00:00:00"),
            )
            .await
            .unwrap();
        database
            .upsert_recorder_health("douyin", "123", "recording", "", "录制正常", 0, None)
            .await
            .unwrap();

        let rows = database.list_recorder_health().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "recording");
        assert_eq!(rows[0].retry_count, 0);
        assert!(rows[0].next_retry_at.is_none());
    }
}
