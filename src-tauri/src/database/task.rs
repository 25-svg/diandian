use super::Database;
use super::DatabaseError;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: String,
    #[sqlx(rename = "type")]
    pub task_type: String,
    pub status: String,
    pub message: String,
    pub metadata: String,
    pub created_at: String,
}

impl Database {
    pub async fn generate_task(
        &self,
        task_type: &str,
        message: &str,
        metadata: &str,
    ) -> Result<TaskRow, DatabaseError> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let task = TaskRow {
            id: task_id,
            task_type: task_type.to_string(),
            status: "pending".to_string(),
            message: message.to_string(),
            metadata: metadata.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.add_task(&task).await?;

        Ok(task)
    }

    pub async fn add_task(&self, task: &TaskRow) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let _ = sqlx::query(
            "INSERT INTO tasks (id, type, status, message, metadata, created_at) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&task.id)
        .bind(&task.task_type)
        .bind(&task.status)
        .bind(&task.message)
        .bind(&task.metadata)
        .bind(&task.created_at)
        .execute(&lock)
        .await?;
        Ok(())
    }

    /// Creates a subtitle task only when this video has no queued or running
    /// subtitle task. The single statement keeps concurrent UI requests from
    /// creating colliding ASR jobs for the same media file.
    pub async fn add_subtitle_task_if_idle(
        &self,
        task: &TaskRow,
        video_id: i64,
    ) -> Result<bool, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result = sqlx::query(
            r#"
            INSERT INTO tasks (id, type, status, message, metadata, created_at)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE NOT EXISTS (
                SELECT 1
                FROM tasks
                WHERE type = 'generate_video_subtitle'
                  AND status IN ('pending', 'processing')
                  AND json_extract(metadata, '$.video_id') = $7
            )
            "#,
        )
        .bind(&task.id)
        .bind(&task.task_type)
        .bind(&task.status)
        .bind(&task.message)
        .bind(&task.metadata)
        .bind(&task.created_at)
        .bind(video_id)
        .execute(&lock)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn add_archive_subtitle_task_if_idle(
        &self,
        task: &TaskRow,
        platform: &str,
        room_id: &str,
        live_id: &str,
    ) -> Result<bool, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result = sqlx::query(
            r#"
            INSERT INTO tasks (id, type, status, message, metadata, created_at)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE NOT EXISTS (
                SELECT 1
                FROM tasks
                WHERE type = 'generate_archive_subtitle'
                  AND status IN ('pending', 'processing')
                  AND json_extract(metadata, '$.platform') = $7
                  AND json_extract(metadata, '$.room_id') = $8
                  AND json_extract(metadata, '$.live_id') = $9
            )
            "#,
        )
        .bind(&task.id)
        .bind(&task.task_type)
        .bind(&task.status)
        .bind(&task.message)
        .bind(&task.metadata)
        .bind(&task.created_at)
        .bind(platform)
        .bind(room_id)
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// One durable post-record review workflow per archive. Completed work is
    /// also considered a duplicate: recorder lifecycle events can be replayed
    /// during reconnects and must never create a second set of learning clips.
    pub async fn add_auto_review_pipeline_task_if_absent(
        &self,
        task: &TaskRow,
        platform: &str,
        room_id: &str,
        live_id: &str,
    ) -> Result<bool, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result = sqlx::query(
            r#"
            INSERT INTO tasks (id, type, status, message, metadata, created_at)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE NOT EXISTS (
                SELECT 1
                FROM tasks
                WHERE type = 'auto_review_pipeline'
                  AND status NOT IN ('failed', 'interrupted')
                  AND json_extract(metadata, '$.platform') = $7
                  AND json_extract(metadata, '$.room_id') = $8
                  AND json_extract(metadata, '$.live_id') = $9
            )
            "#,
        )
        .bind(&task.id)
        .bind(&task.task_type)
        .bind(&task.status)
        .bind(&task.message)
        .bind(&task.metadata)
        .bind(&task.created_at)
        .bind(platform)
        .bind(room_id)
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn get_latest_auto_review_pipeline_task(
        &self,
        platform: &str,
        room_id: &str,
        live_id: &str,
    ) -> Result<Option<TaskRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, TaskRow>(
            r#"
            SELECT * FROM tasks
            WHERE type = 'auto_review_pipeline'
              AND json_extract(metadata, '$.platform') = $1
              AND json_extract(metadata, '$.room_id') = $2
              AND json_extract(metadata, '$.live_id') = $3
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(platform)
        .bind(room_id)
        .bind(live_id)
        .fetch_optional(&lock)
        .await?)
    }

    /// Creates a playback conversion task only when this video does not already
    /// have a queued or running conversion. H.264 conversion is expensive, so
    /// concurrent clicks must never create duplicate full-length files.
    pub async fn add_playback_conversion_task_if_idle(
        &self,
        task: &TaskRow,
        video_id: i64,
    ) -> Result<bool, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result = sqlx::query(
            r#"
            INSERT INTO tasks (id, type, status, message, metadata, created_at)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE NOT EXISTS (
                SELECT 1
                FROM tasks
                WHERE type = 'prepare_video_playback'
                  AND status IN ('pending', 'processing')
                  AND json_extract(metadata, '$.video_id') = $7
            )
            "#,
        )
        .bind(&task.id)
        .bind(&task.task_type)
        .bind(&task.status)
        .bind(&task.message)
        .bind(&task.metadata)
        .bind(&task.created_at)
        .bind(video_id)
        .execute(&lock)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Returns the queued or running ASR task for a video. Playback conversion
    /// waits for this work so two full-media FFmpeg jobs do not compete for the
    /// same CPU, GPU encoder and disk at the same time.
    pub async fn get_active_video_subtitle_task(
        &self,
        video_id: i64,
    ) -> Result<Option<TaskRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let task = sqlx::query_as::<_, TaskRow>(
            r#"
            SELECT *
            FROM tasks
            WHERE type = 'generate_video_subtitle'
              AND status IN ('pending', 'processing')
              AND json_extract(metadata, '$.video_id') = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(video_id)
        .fetch_optional(&lock)
        .await?;
        Ok(task)
    }

    pub async fn get_active_archive_subtitle_task(
        &self,
        platform: &str,
        room_id: &str,
        live_id: &str,
    ) -> Result<Option<TaskRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let task = sqlx::query_as::<_, TaskRow>(
            r#"
            SELECT *
            FROM tasks
            WHERE type = 'generate_archive_subtitle'
              AND status IN ('pending', 'processing')
              AND json_extract(metadata, '$.platform') = $1
              AND json_extract(metadata, '$.room_id') = $2
              AND json_extract(metadata, '$.live_id') = $3
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(platform)
        .bind(room_id)
        .bind(live_id)
        .fetch_optional(&lock)
        .await?;
        Ok(task)
    }

    pub async fn get_tasks(&self) -> Result<Vec<TaskRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let tasks = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks")
            .fetch_all(&lock)
            .await?;
        Ok(tasks)
    }

    pub async fn get_task(&self, id: &str) -> Result<TaskRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let task = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = $1")
            .bind(id)
            .fetch_one(&lock)
            .await?;
        Ok(task)
    }

    pub async fn update_task(
        &self,
        id: &str,
        status: &str,
        message: &str,
        metadata: Option<&str>,
    ) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        if let Some(metadata) = metadata {
            let _ = sqlx::query(
                "UPDATE tasks SET status = $1, message = $2, metadata = $3 WHERE id = $4",
            )
            .bind(status)
            .bind(message)
            .bind(metadata)
            .bind(id)
            .execute(&lock)
            .await?;
        } else {
            let _ = sqlx::query("UPDATE tasks SET status = $1, message = $2 WHERE id = $3")
                .bind(status)
                .bind(message)
                .bind(id)
                .execute(&lock)
                .await?;
        }

        Ok(())
    }

    pub async fn delete_task(&self, id: &str) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let _ = sqlx::query("DELETE FROM tasks WHERE id = $1")
            .bind(id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn delete_archive_tasks(
        &self,
        platform: &str,
        room_id: &str,
        live_id: &str,
    ) -> Result<u64, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result = sqlx::query(
            r#"
            DELETE FROM tasks
            WHERE json_extract(metadata, '$.platform') = $1
              AND json_extract(metadata, '$.room_id') = $2
              AND json_extract(metadata, '$.live_id') = $3
            "#,
        )
        .bind(platform)
        .bind(room_id)
        .bind(live_id)
        .execute(&lock)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn finish_pending_tasks(&self) -> Result<Vec<TaskRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let mut interrupted = sqlx::query_as::<_, TaskRow>(
            "SELECT * FROM tasks WHERE status = 'pending' or status = 'processing'",
        )
        .fetch_all(&lock)
        .await?;
        let _ = sqlx::query(
            "UPDATE tasks SET status = 'interrupted', message = '程序重启时任务未完成，系统将尝试恢复' WHERE status = 'pending' or status = 'processing'",
        )
        .execute(&lock)
        .await?;
        for task in &mut interrupted {
            task.status = "interrupted".to_string();
            task.message = "程序重启时任务未完成，系统将尝试恢复".to_string();
        }
        Ok(interrupted)
    }

    /// Marks queued/running playback conversions for one video as interrupted so a
    /// manual retry can insert a fresh task. Used when the in-memory scheduler no
    /// longer owns the work but the DB row still blocks `add_playback_conversion_task_if_idle`.
    pub async fn interrupt_active_playback_conversion_tasks(
        &self,
        video_id: i64,
        message: &str,
    ) -> Result<u64, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let result = sqlx::query(
            r#"
            UPDATE tasks
            SET status = 'interrupted', message = $1
            WHERE type = 'prepare_video_playback'
              AND status IN ('pending', 'processing')
              AND json_extract(metadata, '$.video_id') = $2
            "#,
        )
        .bind(message)
        .bind(video_id)
        .execute(&lock)
        .await?;
        Ok(result.rows_affected())
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
            "CREATE TABLE tasks (id TEXT PRIMARY KEY, type TEXT, status TEXT, message TEXT, metadata TEXT, created_at TEXT)",
        )
        .await
        .unwrap();
        let db = Database::new();
        db.set(pool).await;
        db
    }

    fn subtitle_task(id: &str, video_id: i64) -> TaskRow {
        TaskRow {
            id: id.into(),
            task_type: "generate_video_subtitle".into(),
            status: "pending".into(),
            message: String::new(),
            metadata: format!(r#"{{"video_id":{video_id}}}"#),
            created_at: "2026-07-23T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn rejects_a_second_active_subtitle_task_for_the_same_video() {
        let db = test_db().await;
        let first = subtitle_task("first", 39);
        let second = subtitle_task("second", 39);

        assert!(db.add_subtitle_task_if_idle(&first, 39).await.unwrap());
        assert!(!db.add_subtitle_task_if_idle(&second, 39).await.unwrap());

        let tasks = db.get_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "first");
    }

    #[tokio::test]
    async fn auto_review_pipeline_is_idempotent_after_success_but_retryable_after_failure() {
        let db = test_db().await;
        let task = |id: &str| TaskRow {
            id: id.into(),
            task_type: "auto_review_pipeline".into(),
            status: "pending".into(),
            message: String::new(),
            metadata: r#"{"platform":"douyin","room_id":"room","live_id":"live"}"#.into(),
            created_at: "2026-08-27T00:00:00Z".into(),
        };

        assert!(db
            .add_auto_review_pipeline_task_if_absent(&task("first"), "douyin", "room", "live")
            .await
            .unwrap());
        assert!(!db
            .add_auto_review_pipeline_task_if_absent(&task("duplicate"), "douyin", "room", "live")
            .await
            .unwrap());

        db.update_task("first", "failed", "network", None)
            .await
            .unwrap();
        assert!(db
            .add_auto_review_pipeline_task_if_absent(&task("retry"), "douyin", "room", "live")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn startup_recovery_marks_active_tasks_interrupted_and_keeps_history() {
        let db = test_db().await;
        db.add_task(&subtitle_task("pending-task", 39))
            .await
            .unwrap();
        let mut processing = subtitle_task("processing-task", 40);
        processing.status = "processing".into();
        db.add_task(&processing).await.unwrap();

        db.finish_pending_tasks().await.unwrap();

        assert_eq!(
            db.get_task("pending-task").await.unwrap().status,
            "interrupted"
        );
        assert_eq!(
            db.get_task("processing-task").await.unwrap().status,
            "interrupted"
        );
    }

    #[tokio::test]
    async fn sqlite_runtime_configuration_sets_a_busy_timeout() {
        let db = test_db().await;

        db.configure_sqlite_runtime().await.unwrap();

        let pool = db.db.read().await.clone().unwrap();
        let timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(timeout, 10_000);
    }

    #[tokio::test]
    async fn rejects_a_second_active_playback_conversion_for_the_same_video() {
        let db = test_db().await;
        let first = TaskRow {
            id: "playback-first".into(),
            task_type: "prepare_video_playback".into(),
            status: "pending".into(),
            message: String::new(),
            metadata: r#"{"video_id":39}"#.into(),
            created_at: "2026-07-23T00:00:00Z".into(),
        };
        let second = TaskRow {
            id: "playback-second".into(),
            ..first.clone()
        };

        assert!(db
            .add_playback_conversion_task_if_idle(&first, 39)
            .await
            .unwrap());
        assert!(!db
            .add_playback_conversion_task_if_idle(&second, 39)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn interrupting_active_playback_conversion_allows_a_manual_retry() {
        let db = test_db().await;
        let stuck = TaskRow {
            id: "playback-stuck".into(),
            task_type: "prepare_video_playback".into(),
            status: "pending".into(),
            message: "waiting".into(),
            metadata: r#"{"video_id":39}"#.into(),
            created_at: "2026-07-23T00:00:00Z".into(),
        };
        let retry = TaskRow {
            id: "playback-retry".into(),
            ..stuck.clone()
        };

        assert!(db
            .add_playback_conversion_task_if_idle(&stuck, 39)
            .await
            .unwrap());
        assert_eq!(
            db.interrupt_active_playback_conversion_tasks(39, "manual retry")
                .await
                .unwrap(),
            1
        );
        assert!(db
            .add_playback_conversion_task_if_idle(&retry, 39)
            .await
            .unwrap());
        assert_eq!(
            db.get_task("playback-stuck").await.unwrap().status,
            "interrupted"
        );
    }

    #[tokio::test]
    async fn finds_only_active_subtitle_task_for_the_requested_video() {
        let db = test_db().await;
        let active = subtitle_task("active", 39);
        let completed = TaskRow {
            id: "completed".into(),
            status: "success".into(),
            ..subtitle_task("unused", 39)
        };
        let other_video = subtitle_task("other-video", 40);
        db.add_task(&active).await.unwrap();
        db.add_task(&completed).await.unwrap();
        db.add_task(&other_video).await.unwrap();

        assert_eq!(
            db.get_active_video_subtitle_task(39)
                .await
                .unwrap()
                .map(|task| task.id),
            Some("active".into())
        );
        assert!(db
            .get_active_video_subtitle_task(41)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn rejects_a_second_active_subtitle_task_for_the_same_archive() {
        let db = test_db().await;
        let first = TaskRow {
            id: "archive-first".into(),
            task_type: "generate_archive_subtitle".into(),
            status: "pending".into(),
            message: String::new(),
            metadata: r#"{"platform":"douyin","room_id":"room-1","live_id":"live-1"}"#.into(),
            created_at: "2026-07-28T00:00:00Z".into(),
        };
        let second = TaskRow {
            id: "archive-second".into(),
            ..first.clone()
        };

        assert!(db
            .add_archive_subtitle_task_if_idle(&first, "douyin", "room-1", "live-1")
            .await
            .unwrap());
        assert!(!db
            .add_archive_subtitle_task_if_idle(&second, "douyin", "room-1", "live-1")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn deletes_only_tasks_linked_to_the_deleted_archive() {
        let db = test_db().await;
        let matching_archive_task = TaskRow {
            id: "archive-matching".into(),
            task_type: "generate_archive_subtitle".into(),
            status: "pending".into(),
            message: String::new(),
            metadata: r#"{"platform":"douyin","room_id":"room-1","live_id":"live-1"}"#.into(),
            created_at: "2026-07-28T00:00:00Z".into(),
        };
        let other_archive_task = TaskRow {
            id: "archive-other".into(),
            metadata: r#"{"platform":"douyin","room_id":"room-1","live_id":"live-2"}"#.into(),
            ..matching_archive_task.clone()
        };
        let video_task = subtitle_task("video-task", 39);

        db.add_task(&matching_archive_task).await.unwrap();
        db.add_task(&other_archive_task).await.unwrap();
        db.add_task(&video_task).await.unwrap();

        let deleted = db
            .delete_archive_tasks("douyin", "room-1", "live-1")
            .await
            .unwrap();

        assert_eq!(deleted, 1);
        let mut remaining_ids: Vec<_> = db
            .get_tasks()
            .await
            .unwrap()
            .into_iter()
            .map(|task| task.id)
            .collect();
        remaining_ids.sort();
        assert_eq!(remaining_ids, vec!["archive-other", "video-task"]);
    }
}
