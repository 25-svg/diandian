use sqlx::Pool;
use sqlx::Sqlite;
use thiserror::Error;
use tokio::sync::RwLock;

pub mod account;
pub mod knowledge;
pub mod live_dashboard;
pub mod live_dashboard_binding;
pub mod master_sample_batch;
pub mod master_script;
pub mod message;
pub mod record;
pub mod recorder;
pub mod review_sample;
pub mod task;
pub mod transcript_dictionary_candidate;
pub mod video;
pub mod video_archive;

pub struct Database {
    db: RwLock<Option<Pool<Sqlite>>>,
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Entry insert failed")]
    Insert,
    #[error("Entry not found")]
    NotFound,
    #[error("Cookies are invalid")]
    InvalidCookies,
    #[error("Number exceed i64 range")]
    NumberExceedI64Range,
    #[error("DB error: {0}")]
    DB(#[from] sqlx::Error),
    #[error("SQL is incorret: {sql}")]
    Sql { sql: String },
    #[error("invalid transcript dictionary candidate status: {0}")]
    InvalidTranscriptDictionaryCandidateStatus(String),
    #[error("invalid transcript dictionary candidate minimal metadata: {0}")]
    InvalidTranscriptDictionaryCandidateMetadata(String),
    #[error("invalid master script state: {0}")]
    InvalidMasterScriptState(String),
}

impl From<DatabaseError> for String {
    fn from(err: DatabaseError) -> Self {
        err.to_string()
    }
}

impl Database {
    pub fn new() -> Database {
        Database {
            db: RwLock::new(None),
        }
    }

    /// db *must* be set in tauri setup
    pub async fn set(&self, p: Pool<Sqlite>) {
        *self.db.write().await = Some(p);
    }

    /// Configure the shared SQLite pool for desktop use. WAL keeps readers from
    /// blocking a short write, while a bounded busy timeout turns contention
    /// into a wait instead of an immediate `database is locked` failure.
    pub async fn configure_sqlite_runtime(&self) -> Result<(), DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA synchronous = NORMAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA busy_timeout = 10000")
            .execute(&pool)
            .await?;
        Ok(())
    }
}
