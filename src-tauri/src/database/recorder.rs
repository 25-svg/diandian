use super::Database;
use super::DatabaseError;
use chrono::Utc;
use recorder::platforms::PlatformType;

pub const RECORDER_STREAMER_ASSIGNMENT_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS recorder_streamer_assignments (
    platform TEXT NOT NULL,
    room_id TEXT NOT NULL,
    streamer_name TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (platform, room_id)
);
CREATE INDEX IF NOT EXISTS idx_recorder_streamer_assignments_name
ON recorder_streamer_assignments(streamer_name);
"#;

/// Recorder in database is pretty simple
/// because many room infos are collected in realtime
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct RecorderRow {
    pub room_id: String,
    pub created_at: String,
    pub platform: String,
    pub auto_start: bool,
    pub extra: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStreamerAssignmentRow {
    pub platform: String,
    pub room_id: String,
    pub streamer_name: String,
    pub updated_at: String,
}

// recorders
impl Database {
    pub async fn add_recorder(
        &self,
        platform: PlatformType,
        room_id: &str,
        extra: &str,
    ) -> Result<RecorderRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let recorder = RecorderRow {
            room_id: room_id.to_string(),
            created_at: Utc::now().to_rfc3339(),
            platform: platform.as_str().to_string(),
            auto_start: true,
            extra: extra.to_string(),
        };
        let _ = sqlx::query(
            "INSERT OR REPLACE INTO recorders (room_id, created_at, platform, auto_start, extra) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(room_id)
        .bind(&recorder.created_at)
        .bind(platform.as_str())
        .bind(recorder.auto_start)
        .bind(extra)
        .execute(&lock)
        .await?;
        Ok(recorder)
    }

    pub async fn remove_recorder(&self, room_id: &str) -> Result<RecorderRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let recorder =
            sqlx::query_as::<_, RecorderRow>("SELECT * FROM recorders WHERE room_id = $1")
                .bind(room_id)
                .fetch_one(&lock)
                .await?;
        let sql = sqlx::query("DELETE FROM recorders WHERE room_id = $1")
            .bind(room_id)
            .execute(&lock)
            .await?;
        if sql.rows_affected() != 1 {
            return Err(DatabaseError::NotFound);
        }

        sqlx::query("DELETE FROM recorder_streamer_assignments WHERE room_id = $1")
            .bind(room_id)
            .execute(&lock)
            .await?;

        // remove related archive
        let _ = self.remove_archive(room_id).await;
        Ok(recorder)
    }

    pub async fn get_recorders(&self) -> Result<Vec<RecorderRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, RecorderRow>(
            "SELECT room_id, created_at, platform, auto_start, extra FROM recorders",
        )
        .fetch_all(&lock)
        .await?)
    }

    pub async fn remove_archive(&self, room_id: &str) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let _ = sqlx::query("DELETE FROM records WHERE room_id = $1")
            .bind(room_id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn update_recorder(
        &self,
        platform: PlatformType,
        room_id: &str,
        auto_start: bool,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let _ = sqlx::query(
            "UPDATE recorders SET auto_start = $1 WHERE platform = $2 AND room_id = $3",
        )
        .bind(auto_start)
        .bind(platform.as_str().to_string())
        .bind(room_id)
        .execute(&lock)
        .await?;
        Ok(())
    }

    pub async fn set_recorder_streamer_assignment(
        &self,
        platform: PlatformType,
        room_id: &str,
        streamer_name: &str,
    ) -> Result<RecorderStreamerAssignmentRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "INSERT INTO recorder_streamer_assignments
                (platform, room_id, streamer_name, updated_at)
             VALUES ($1, $2, $3, datetime('now'))
             ON CONFLICT(platform, room_id) DO UPDATE SET
                streamer_name = excluded.streamer_name,
                updated_at = datetime('now')",
        )
        .bind(platform.as_str())
        .bind(room_id)
        .bind(streamer_name.trim())
        .execute(&lock)
        .await?;

        Ok(sqlx::query_as::<_, RecorderStreamerAssignmentRow>(
            "SELECT platform, room_id, streamer_name, updated_at
             FROM recorder_streamer_assignments
             WHERE platform = $1 AND room_id = $2",
        )
        .bind(platform.as_str())
        .bind(room_id)
        .fetch_one(&lock)
        .await?)
    }

    pub async fn remove_recorder_streamer_assignment(
        &self,
        platform: PlatformType,
        room_id: &str,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "DELETE FROM recorder_streamer_assignments
             WHERE platform = $1 AND room_id = $2",
        )
        .bind(platform.as_str())
        .bind(room_id)
        .execute(&lock)
        .await?;
        Ok(())
    }

    pub async fn get_recorder_streamer_assignment(
        &self,
        platform: PlatformType,
        room_id: &str,
    ) -> Result<Option<RecorderStreamerAssignmentRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, RecorderStreamerAssignmentRow>(
            "SELECT platform, room_id, streamer_name, updated_at
             FROM recorder_streamer_assignments
             WHERE platform = $1 AND room_id = $2",
        )
        .bind(platform.as_str())
        .bind(room_id)
        .fetch_optional(&lock)
        .await?)
    }

    pub async fn list_recorder_streamer_assignments(
        &self,
    ) -> Result<Vec<RecorderStreamerAssignmentRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, RecorderStreamerAssignmentRow>(
            "SELECT platform, room_id, streamer_name, updated_at
             FROM recorder_streamer_assignments
             ORDER BY updated_at DESC, streamer_name COLLATE NOCASE",
        )
        .fetch_all(&lock)
        .await?)
    }

    pub async fn list_known_streamer_names(&self) -> Result<Vec<String>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT streamer_name FROM (
                SELECT TRIM(streamer_name) AS streamer_name
                FROM recorder_streamer_assignments
                WHERE TRIM(streamer_name) <> ''
                UNION
                SELECT TRIM(anchor_name) AS streamer_name
                FROM records
                WHERE TRIM(anchor_name) <> ''
                  AND (anchor_source = 'manual' OR anchor_detection_status = 'confirmed')
                UNION
                SELECT TRIM(anchor_name) AS streamer_name
                FROM videos
                WHERE TRIM(anchor_name) <> ''
                  AND (anchor_source = 'manual' OR anchor_detection_status = 'confirmed')
             )
             ORDER BY streamer_name COLLATE NOCASE",
        )
        .fetch_all(&lock)
        .await?;
        Ok(rows)
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
        pool.execute(RECORDER_STREAMER_ASSIGNMENT_MIGRATION_SQL)
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    #[tokio::test]
    async fn assignment_is_upserted_per_platform_and_room() {
        let database = database().await;
        database
            .set_recorder_streamer_assignment(PlatformType::Douyin, "room-1", "罗雨欣")
            .await
            .unwrap();
        database
            .set_recorder_streamer_assignment(PlatformType::Douyin, "room-1", "侯梦娜")
            .await
            .unwrap();

        let rows = database.list_recorder_streamer_assignments().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].streamer_name, "侯梦娜");
    }

    #[tokio::test]
    async fn assignment_can_be_cleared_without_touching_other_rooms() {
        let database = database().await;
        database
            .set_recorder_streamer_assignment(PlatformType::Douyin, "room-1", "罗雨欣")
            .await
            .unwrap();
        database
            .set_recorder_streamer_assignment(PlatformType::Douyin, "room-2", "侯梦娜")
            .await
            .unwrap();
        database
            .remove_recorder_streamer_assignment(PlatformType::Douyin, "room-1")
            .await
            .unwrap();

        assert!(database
            .get_recorder_streamer_assignment(PlatformType::Douyin, "room-1")
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            database
                .get_recorder_streamer_assignment(PlatformType::Douyin, "room-2")
                .await
                .unwrap()
                .unwrap()
                .streamer_name,
            "侯梦娜"
        );
    }
}
