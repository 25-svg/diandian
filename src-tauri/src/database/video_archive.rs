use super::{Database, DatabaseError};

pub const VIDEO_ARCHIVE_JOBS_MIGRATION_SQL: &str = r#"
CREATE TABLE video_archive_jobs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  video_id INTEGER NOT NULL UNIQUE REFERENCES videos(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL CHECK(source_kind IN ('recording','import')),
  status TEXT NOT NULL CHECK(status IN ('local_only','pending','uploading','archived','retry_wait','failed')),
  local_path TEXT NOT NULL,
  nas_path TEXT NOT NULL DEFAULT '',
  uploaded_bytes INTEGER NOT NULL DEFAULT 0,
  retry_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT NOT NULL DEFAULT '',
  next_retry_at TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_video_archive_jobs_status_retry
ON video_archive_jobs(status, next_retry_at, id);
"#;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct VideoArchiveRow {
    pub id: i64,
    pub video_id: i64,
    pub source_kind: String,
    pub status: String,
    pub local_path: String,
    pub nas_path: String,
    pub uploaded_bytes: i64,
    pub retry_count: i64,
    pub last_error: String,
    pub next_retry_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
    pub async fn enqueue_video_archive(
        &self,
        video_id: i64,
        source_kind: &str,
        local_path: &str,
    ) -> Result<VideoArchiveRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, VideoArchiveRow>(
            "INSERT INTO video_archive_jobs (video_id, source_kind, status, local_path) \
             VALUES ($1, $2, 'pending', $3) \
             ON CONFLICT(video_id) DO UPDATE SET \
               source_kind = excluded.source_kind, \
               local_path = CASE WHEN video_archive_jobs.status = 'archived' THEN video_archive_jobs.local_path ELSE excluded.local_path END, \
               status = CASE WHEN video_archive_jobs.status = 'archived' THEN video_archive_jobs.status ELSE 'pending' END, \
               last_error = CASE WHEN video_archive_jobs.status = 'archived' THEN video_archive_jobs.last_error ELSE '' END, \
               next_retry_at = CASE WHEN video_archive_jobs.status = 'archived' THEN video_archive_jobs.next_retry_at ELSE NULL END, \
               updated_at = datetime('now') \
             RETURNING *",
        )
        .bind(video_id)
        .bind(source_kind)
        .bind(local_path)
        .fetch_one(&pool)
        .await?)
    }

    pub async fn claim_next_video_archive(&self) -> Result<Option<VideoArchiveRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let mut transaction = pool.begin().await?;
        let candidate = sqlx::query_as::<_, VideoArchiveRow>(
            "SELECT * FROM video_archive_jobs \
             WHERE status = 'pending' \
                OR (status = 'retry_wait' AND (next_retry_at IS NULL OR next_retry_at <= datetime('now'))) \
             ORDER BY id ASC LIMIT 1",
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(candidate) = candidate else {
            transaction.commit().await?;
            return Ok(None);
        };
        let claimed = sqlx::query_as::<_, VideoArchiveRow>(
            "UPDATE video_archive_jobs \
             SET status = 'uploading', last_error = '', updated_at = datetime('now') \
             WHERE id = $1 AND status IN ('pending','retry_wait') \
             RETURNING *",
        )
        .bind(candidate.id)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(claimed)
    }

    pub async fn complete_video_archive(
        &self,
        archive_id: i64,
        nas_path: &str,
    ) -> Result<(), DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let mut transaction = pool.begin().await?;
        let (video_id,) =
            sqlx::query_as::<_, (i64,)>("SELECT video_id FROM video_archive_jobs WHERE id = $1")
                .bind(archive_id)
                .fetch_one(&mut *transaction)
                .await?;
        sqlx::query("UPDATE videos SET file = $1 WHERE id = $2")
            .bind(nas_path)
            .bind(video_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "UPDATE video_archive_jobs \
             SET status = 'archived', nas_path = $1, next_retry_at = NULL, \
                 last_error = '', updated_at = datetime('now') \
             WHERE id = $2",
        )
        .bind(nas_path)
        .bind(archive_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_video_archive(
        &self,
        archive_id: i64,
    ) -> Result<VideoArchiveRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as::<_, VideoArchiveRow>("SELECT * FROM video_archive_jobs WHERE id = $1")
                .bind(archive_id)
                .fetch_one(&pool)
                .await?,
        )
    }

    pub async fn get_video_archive_by_video(
        &self,
        video_id: i64,
    ) -> Result<Option<VideoArchiveRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let result = sqlx::query_as::<_, VideoArchiveRow>(
            "SELECT * FROM video_archive_jobs WHERE video_id = $1",
        )
        .bind(video_id)
        .fetch_optional(&pool)
        .await;
        match result {
            Ok(row) => Ok(row),
            Err(sqlx::Error::Database(error))
                if error
                    .message()
                    .contains("no such table: video_archive_jobs") =>
            {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn list_video_archives(&self) -> Result<Vec<VideoArchiveRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, VideoArchiveRow>(
            "SELECT * FROM video_archive_jobs ORDER BY id DESC",
        )
        .fetch_all(&pool)
        .await?)
    }

    pub async fn retry_video_archive(
        &self,
        video_id: i64,
    ) -> Result<VideoArchiveRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, VideoArchiveRow>(
            "UPDATE video_archive_jobs \
             SET status = 'pending', retry_count = 0, last_error = '', \
                 next_retry_at = NULL, updated_at = datetime('now') \
             WHERE video_id = $1 AND status IN ('retry_wait','failed') \
             RETURNING *",
        )
        .bind(video_id)
        .fetch_one(&pool)
        .await?)
    }

    pub async fn update_video_archive_progress(
        &self,
        archive_id: i64,
        uploaded_bytes: i64,
    ) -> Result<(), DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE video_archive_jobs \
             SET uploaded_bytes = $1, updated_at = datetime('now') \
             WHERE id = $2",
        )
        .bind(uploaded_bytes)
        .bind(archive_id)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn fail_video_archive(
        &self,
        archive_id: i64,
        error: &str,
        next_retry_at: Option<&str>,
        terminal: bool,
    ) -> Result<(), DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let status = if terminal { "failed" } else { "retry_wait" };
        sqlx::query(
            "UPDATE video_archive_jobs \
             SET status = $1, retry_count = retry_count + 1, last_error = $2, \
                 next_retry_at = $3, updated_at = datetime('now') \
             WHERE id = $4",
        )
        .bind(status)
        .bind(error)
        .bind(next_retry_at)
        .bind(archive_id)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn reset_interrupted_video_archives(&self) -> Result<u64, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query(
            "UPDATE video_archive_jobs \
             SET status = 'retry_wait', next_retry_at = NULL, \
                 last_error = '程序上次退出时转存尚未完成，已等待重试', \
                 updated_at = datetime('now') \
             WHERE status = 'uploading'",
        )
        .execute(&pool)
        .await?
        .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    async fn test_database() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("CREATE TABLE videos (id INTEGER PRIMARY KEY, file TEXT NOT NULL DEFAULT '')")
            .await
            .unwrap();
        pool.execute(VIDEO_ARCHIVE_JOBS_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute("INSERT INTO videos (id, file) VALUES (7, 'local.mp4')")
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    #[tokio::test]
    async fn archive_job_moves_from_pending_to_archived() {
        let database = test_database().await;
        let job = database
            .enqueue_video_archive(7, "recording", "D:\\videos\\local.mp4")
            .await
            .unwrap();
        assert_eq!(job.status, "pending");

        let claimed = database.claim_next_video_archive().await.unwrap().unwrap();
        assert_eq!(claimed.id, job.id);
        assert_eq!(claimed.status, "uploading");

        database
            .complete_video_archive(job.id, r"\\nas\直播录像\主播\2026-07-26\local.mp4")
            .await
            .unwrap();
        let completed = database.get_video_archive(job.id).await.unwrap();
        assert_eq!(completed.status, "archived");
        assert_eq!(
            database.get_video_source(7).await.unwrap().1,
            r"\\nas\直播录像\主播\2026-07-26\local.mp4"
        );
    }

    #[tokio::test]
    async fn interrupted_upload_is_reset_without_losing_source() {
        let database = test_database().await;
        let job = database
            .enqueue_video_archive(7, "recording", "D:\\videos\\local.mp4")
            .await
            .unwrap();
        database.claim_next_video_archive().await.unwrap();

        assert_eq!(
            database.reset_interrupted_video_archives().await.unwrap(),
            1
        );
        let reset = database.get_video_archive(job.id).await.unwrap();
        assert_eq!(reset.status, "retry_wait");
        assert_eq!(reset.local_path, "D:\\videos\\local.mp4");
    }

    #[tokio::test]
    async fn progress_and_retry_failure_are_persisted() {
        let database = test_database().await;
        let job = database
            .enqueue_video_archive(7, "recording", "D:\\videos\\local.mp4")
            .await
            .unwrap();
        database.claim_next_video_archive().await.unwrap();

        database
            .update_video_archive_progress(job.id, 4096)
            .await
            .unwrap();
        database
            .fail_video_archive(
                job.id,
                "NAS 共享目录当前不可访问",
                Some("2099-01-01 00:00:00"),
                false,
            )
            .await
            .unwrap();

        let failed = database.get_video_archive(job.id).await.unwrap();
        assert_eq!(failed.status, "retry_wait");
        assert_eq!(failed.uploaded_bytes, 4096);
        assert_eq!(failed.retry_count, 1);
        assert_eq!(failed.last_error, "NAS 共享目录当前不可访问");
        assert_eq!(failed.next_retry_at.as_deref(), Some("2099-01-01 00:00:00"));
    }

    #[tokio::test]
    async fn manual_retry_returns_failed_job_to_pending() {
        let database = test_database().await;
        let job = database
            .enqueue_video_archive(7, "recording", "D:\\videos\\local.mp4")
            .await
            .unwrap();
        database.claim_next_video_archive().await.unwrap();
        database
            .fail_video_archive(job.id, "网络中断", None, true)
            .await
            .unwrap();

        let retried = database.retry_video_archive(7).await.unwrap();

        assert_eq!(retried.status, "pending");
        assert_eq!(retried.retry_count, 0);
        assert!(retried.last_error.is_empty());
        assert_eq!(database.list_video_archives().await.unwrap().len(), 1);
    }
}
