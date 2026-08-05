use super::Database;
use super::DatabaseError;

// CREATE TABLE videos (id INTEGER PRIMARY KEY, room_id TEXT, cover TEXT, file TEXT, length INTEGER, size INTEGER, status INTEGER, bvid TEXT, title TEXT, desc TEXT, tags TEXT, area INTEGER, created_at TEXT);
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct VideoRow {
    pub id: i64,
    pub room_id: String,
    pub cover: String,
    pub file: String,
    pub note: String,
    pub length: i64,
    pub size: i64,
    pub status: i64,
    pub bvid: String,
    pub title: String,
    pub desc: String,
    pub tags: String,
    pub area: i64,
    pub created_at: String,
    pub platform: String,
    pub anchor_name: String,
    pub anchor_source: String,
    pub anchor_confidence: String,
    pub anchor_detection_status: String,
    pub anchor_detection_error: String,
    pub anchor_detected_at: String,
}

pub const VIDEO_ANCHOR_DETECTION_MIGRATION_SQL: &str = r#"
ALTER TABLE videos ADD COLUMN anchor_name TEXT NOT NULL DEFAULT '';
ALTER TABLE videos ADD COLUMN anchor_source TEXT NOT NULL DEFAULT '';
ALTER TABLE videos ADD COLUMN anchor_confidence TEXT NOT NULL DEFAULT '';
ALTER TABLE videos ADD COLUMN anchor_detection_status TEXT NOT NULL DEFAULT 'not_requested';
ALTER TABLE videos ADD COLUMN anchor_detection_error TEXT NOT NULL DEFAULT '';
ALTER TABLE videos ADD COLUMN anchor_detected_at TEXT NOT NULL DEFAULT '';
CREATE INDEX idx_videos_anchor_detection_status
ON videos(anchor_detection_status, anchor_source);
"#;

pub const VIDEO_TRANSCRIPT_CHUNKS_MIGRATION_SQL: &str = r#"
CREATE TABLE video_transcript_chunks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  video_id INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending','running','complete','failed')),
  input_hash TEXT NOT NULL,
  raw_srt TEXT NOT NULL DEFAULT '',
  reviewed_srt TEXT NOT NULL DEFAULT '',
  error TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(video_id, chunk_index)
);
CREATE INDEX idx_video_transcript_chunks_video_status
ON video_transcript_chunks(video_id, status, chunk_index);
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoTranscriptChunkInput {
    pub video_id: i64,
    pub chunk_index: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub status: String,
    pub input_hash: String,
    pub raw_srt: String,
    pub reviewed_srt: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct VideoTranscriptChunkRow {
    pub id: i64,
    pub video_id: i64,
    pub chunk_index: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub status: String,
    pub input_hash: String,
    pub raw_srt: String,
    pub reviewed_srt: String,
    pub error: Option<String>,
    pub updated_at: String,
}

impl Database {
    pub async fn upsert_video_transcript_chunk(
        &self,
        input: VideoTranscriptChunkInput,
    ) -> Result<VideoTranscriptChunkRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, VideoTranscriptChunkRow>(
            "INSERT INTO video_transcript_chunks (video_id, chunk_index, start_ms, end_ms, status, input_hash, raw_srt, reviewed_srt, error) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT(video_id, chunk_index) DO UPDATE SET \
                start_ms = excluded.start_ms, \
                end_ms = excluded.end_ms, \
                status = excluded.status, \
                input_hash = excluded.input_hash, \
                raw_srt = excluded.raw_srt, \
                reviewed_srt = excluded.reviewed_srt, \
                error = excluded.error, \
                updated_at = datetime('now') \
             RETURNING *",
        )
        .bind(input.video_id)
        .bind(input.chunk_index)
        .bind(input.start_ms)
        .bind(input.end_ms)
        .bind(input.status)
        .bind(input.input_hash)
        .bind(input.raw_srt)
        .bind(input.reviewed_srt)
        .bind(input.error)
        .fetch_one(&pool)
        .await?)
    }

    pub async fn list_video_transcript_chunks(
        &self,
        video_id: i64,
    ) -> Result<Vec<VideoTranscriptChunkRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, VideoTranscriptChunkRow>(
            "SELECT * FROM video_transcript_chunks WHERE video_id = $1 ORDER BY chunk_index ASC",
        )
        .bind(video_id)
        .fetch_all(&pool)
        .await?)
    }

    pub async fn get_videos(&self, room_id: &str) -> Result<Vec<VideoRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let videos = sqlx::query_as::<_, VideoRow>("SELECT * FROM videos WHERE room_id = $1;")
            .bind(room_id)
            .fetch_all(&lock)
            .await?;
        Ok(videos)
    }

    pub async fn get_video(&self, id: i64) -> Result<VideoRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as::<_, VideoRow>("SELECT * FROM videos WHERE id = $1")
                .bind(id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn get_video_source(&self, id: i64) -> Result<(i64, String), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as::<_, (i64, String)>("SELECT id, file FROM videos WHERE id = $1")
                .bind(id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn list_video_sources(&self) -> Result<Vec<(i64, String)>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as::<_, (i64, String)>("SELECT id, file FROM videos ORDER BY id ASC")
                .fetch_all(&lock)
                .await?,
        )
    }

    pub async fn count_videos_with_file(&self, file: &str) -> Result<i64, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM videos WHERE file = $1")
            .bind(file)
            .fetch_one(&lock)
            .await?;
        Ok(count)
    }

    pub async fn update_video(&self, video_row: &VideoRow) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("UPDATE videos SET status = $1, bvid = $2, title = $3, desc = $4, tags = $5, area = $6, note = $7 WHERE id = $8")
            .bind(video_row.status)
            .bind(&video_row.bvid)
            .bind(&video_row.title)
            .bind(&video_row.desc)
            .bind(&video_row.tags)
            .bind(video_row.area)
            .bind(&video_row.note)
            .bind(video_row.id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn set_video_anchor_detection_running(
        &self,
        id: i64,
    ) -> Result<VideoRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE videos
             SET anchor_detection_status = 'running',
                 anchor_detection_error = ''
             WHERE id = $1 AND anchor_source <> 'manual'",
        )
        .bind(id)
        .execute(&lock)
        .await?;
        drop(lock);
        self.get_video(id).await
    }

    pub async fn save_video_anchor_detection(
        &self,
        id: i64,
        anchor_name: &str,
        confidence: &str,
        status: &str,
        error: &str,
    ) -> Result<VideoRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE videos
             SET anchor_name = $1,
                 anchor_source = 'minimax_vision',
                 anchor_confidence = $2,
                 anchor_detection_status = $3,
                 anchor_detection_error = $4,
                 anchor_detected_at = datetime('now')
             WHERE id = $5 AND anchor_source <> 'manual'",
        )
        .bind(anchor_name)
        .bind(confidence)
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(&lock)
        .await?;
        drop(lock);
        self.get_video(id).await
    }

    pub async fn save_video_anchor_manual(
        &self,
        id: i64,
        anchor_name: &str,
    ) -> Result<VideoRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query(
            "UPDATE videos
             SET anchor_name = $1,
                 anchor_source = 'manual',
                 anchor_confidence = 'high',
                 anchor_detection_status = 'confirmed',
                 anchor_detection_error = '',
                 anchor_detected_at = datetime('now')
             WHERE id = $2",
        )
        .bind(anchor_name.trim())
        .bind(id)
        .execute(&lock)
        .await?;
        drop(lock);
        self.get_video(id).await
    }

    pub async fn delete_video(&self, id: i64) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("DELETE FROM videos WHERE id = $1")
            .bind(id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn add_video(&self, video: &VideoRow) -> Result<VideoRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let sql = sqlx::query("INSERT INTO videos (room_id, cover, file, note, length, size, status, bvid, title, desc, tags, area, created_at, platform, anchor_name, anchor_source, anchor_confidence, anchor_detection_status, anchor_detection_error, anchor_detected_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)")
            .bind(&video.room_id)
            .bind(&video.cover)
            .bind(&video.file)
            .bind(&video.note)
            .bind(video.length)
            .bind(video.size)
            .bind(video.status)
            .bind(&video.bvid)
            .bind(&video.title)
            .bind(&video.desc)
            .bind(&video.tags)
            .bind(video.area)
            .bind(&video.created_at)
            .bind(&video.platform)
            .bind(&video.anchor_name)
            .bind(&video.anchor_source)
            .bind(&video.anchor_confidence)
            .bind(&video.anchor_detection_status)
            .bind(&video.anchor_detection_error)
            .bind(&video.anchor_detected_at)
            .execute(&lock)
            .await?;
        let video = VideoRow {
            id: sql.last_insert_rowid(),
            ..video.clone()
        };
        Ok(video)
    }

    pub async fn update_video_cover(&self, id: i64, cover: &str) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("UPDATE videos SET cover = $1 WHERE id = $2")
            .bind(cover)
            .bind(id)
            .execute(&lock)
            .await?;
        Ok(())
    }

    pub async fn get_all_videos(&self) -> Result<Vec<VideoRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let videos =
            sqlx::query_as::<_, VideoRow>("SELECT * FROM videos ORDER BY created_at DESC;")
                .fetch_all(&lock)
                .await?;
        Ok(videos)
    }

    pub async fn get_video_cover(&self, id: i64) -> Result<String, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let video = sqlx::query_as::<_, VideoRow>("SELECT * FROM videos WHERE id = $1")
            .bind(id)
            .fetch_one(&lock)
            .await?;
        Ok(video.cover)
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
        pool.execute(
            "CREATE TABLE videos (id INTEGER PRIMARY KEY, room_id TEXT, cover TEXT, file TEXT, note TEXT, length INTEGER, size INTEGER, status INTEGER, bvid TEXT, title TEXT, desc TEXT, tags TEXT, area INTEGER, created_at TEXT, platform TEXT)",
        )
        .await
        .unwrap();
        pool.execute(VIDEO_TRANSCRIPT_CHUNKS_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute("INSERT INTO videos (id) VALUES (9)")
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    async fn anchor_test_database() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(
            "CREATE TABLE videos (
                id INTEGER PRIMARY KEY,
                room_id TEXT NOT NULL DEFAULT '',
                cover TEXT NOT NULL DEFAULT '',
                file TEXT NOT NULL DEFAULT '',
                note TEXT NOT NULL DEFAULT '',
                length INTEGER NOT NULL DEFAULT 0,
                size INTEGER NOT NULL DEFAULT 0,
                status INTEGER NOT NULL DEFAULT 0,
                bvid TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                desc TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '',
                area INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '',
                platform TEXT NOT NULL DEFAULT '',
                anchor_name TEXT NOT NULL DEFAULT '',
                anchor_source TEXT NOT NULL DEFAULT '',
                anchor_confidence TEXT NOT NULL DEFAULT '',
                anchor_detection_status TEXT NOT NULL DEFAULT 'pending',
                anchor_detection_error TEXT NOT NULL DEFAULT '',
                anchor_detected_at TEXT NOT NULL DEFAULT ''
            )",
        )
        .await
        .unwrap();
        pool.execute("INSERT INTO videos (id, file) VALUES (11, 'sample.mp4')")
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    #[tokio::test]
    async fn automatic_anchor_detection_cannot_overwrite_a_manual_name() {
        let database = anchor_test_database().await;
        database.save_video_anchor_manual(11, "小鱼").await.unwrap();
        database
            .save_video_anchor_detection(11, "小雨", "high", "confirmed", "")
            .await
            .unwrap();

        let video = database.get_video(11).await.unwrap();
        assert_eq!(video.anchor_name, "小鱼");
        assert_eq!(video.anchor_source, "manual");
        assert_eq!(video.anchor_detection_status, "confirmed");
    }

    #[tokio::test]
    async fn automatic_anchor_detection_persists_confirmed_result() {
        let database = anchor_test_database().await;
        database
            .save_video_anchor_detection(11, "小鱼", "high", "confirmed", "")
            .await
            .unwrap();

        let video = database.get_video(11).await.unwrap();
        assert_eq!(video.anchor_name, "小鱼");
        assert_eq!(video.anchor_source, "minimax_vision");
        assert_eq!(video.anchor_confidence, "high");
        assert_eq!(video.anchor_detection_status, "confirmed");
    }

    #[tokio::test]
    async fn video_transcript_chunks_upsert_by_video_and_chunk_index() {
        let database = test_database().await;
        let first = database
            .upsert_video_transcript_chunk(VideoTranscriptChunkInput {
                video_id: 9,
                chunk_index: 0,
                start_ms: 0,
                end_ms: 600_000,
                status: "complete".into(),
                input_hash: "first-input".into(),
                raw_srt: "raw-v1".into(),
                reviewed_srt: "reviewed-v1".into(),
                error: None,
            })
            .await
            .unwrap();
        let second = database
            .upsert_video_transcript_chunk(VideoTranscriptChunkInput {
                video_id: 9,
                chunk_index: 0,
                start_ms: 0,
                end_ms: 600_000,
                status: "complete".into(),
                input_hash: "first-input".into(),
                raw_srt: "raw-v1".into(),
                reviewed_srt: "reviewed-v2".into(),
                error: None,
            })
            .await
            .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(
            database
                .list_video_transcript_chunks(9)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(second.reviewed_srt, "reviewed-v2");
    }
}
