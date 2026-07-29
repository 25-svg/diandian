// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anchor_detection;
mod audio_utils;
mod config;
mod constants;
mod danmu2ass;
mod database;
mod ffmpeg;
mod fs_util;
mod handlers;
#[cfg(feature = "headless")]
mod http_server;
mod knowledge_writer;
mod live_data_import;
mod master_script;
mod migration;
mod nas_archive;
mod progress;
mod recorder_manager;
mod security;
mod state;
mod static_server;
mod storage_migration;
mod subtitle_generator;
mod task;
#[cfg(feature = "gui")]
mod tray;
mod webhook;

use async_std::fs;
use chrono::Utc;
use config::Config;
use database::Database;
use migration::migration_methods::try_add_parent_id_to_records;
use migration::migration_methods::try_convert_clip_covers;
use migration::migration_methods::try_convert_entry_to_m3u8;
use migration::migration_methods::try_convert_live_covers;
use migration::migration_methods::try_rebuild_archives;
use recorder_manager::RecorderManager;
use simplelog::ConfigBuilder;
use state::State;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::MetadataExt;

#[cfg(target_os = "windows")]
use std::os::windows::fs::MetadataExt;

#[cfg(feature = "gui")]
use {
    tauri::{Manager, WindowEvent},
    tauri_plugin_sql::{Migration, MigrationKind},
};

#[cfg(feature = "headless")]
use {
    clap::{arg, command, Parser},
    futures_core::future::BoxFuture,
    migration::{Migration, MigrationKind},
    sqlx::error::BoxDynError,
    sqlx::migrate::Migration as SqlxMigration,
    sqlx::migrate::MigrationSource,
    sqlx::{
        migrate::{MigrateDatabase, Migrator},
        Pool, Sqlite,
    },
};

/// open a log file, if file size exceeds 1MB, backup log file and create a new one.
async fn open_log_file(log_dir: &Path) -> Result<File, Box<dyn std::error::Error>> {
    let log_filename = log_dir.join("bsr.log");

    if let Ok(meta) = fs::metadata(&log_filename).await {
        #[cfg(target_os = "windows")]
        let file_size = meta.file_size();
        #[cfg(not(target_os = "windows"))]
        let file_size = meta.size();
        if file_size > 1024 * 1024 {
            // move original file to backup
            let date_str = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
            let backup_filename = log_dir.join(format!("bsr-{date_str}.log"));
            fs::rename(&log_filename, backup_filename).await?;
        }
    }

    Ok(File::options()
        .create(true)
        .append(true)
        .open(&log_filename)?)
}

async fn setup_logging(log_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // mkdir if not exists
    if !log_dir.exists() {
        std::fs::create_dir_all(log_dir)?;
    }

    let file = open_log_file(log_dir).await?;

    let config = ConfigBuilder::new()
        .set_target_level(simplelog::LevelFilter::Debug)
        .set_location_level(simplelog::LevelFilter::Debug)
        .add_filter_ignore_str("tokio")
        .add_filter_ignore_str("hyper")
        .add_filter_ignore_str("sqlx")
        .add_filter_ignore_str("reqwest")
        .add_filter_ignore_str("h2")
        .add_filter_ignore_str("danmu_stream")
        .build();

    simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(
            simplelog::LevelFilter::Debug,
            config,
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        ),
        simplelog::WriteLogger::new(
            simplelog::LevelFilter::Info,
            simplelog::Config::default(),
            file,
        ),
    ])?;

    // logging current package version
    log::info!("Current version: {}", env!("CARGO_PKG_VERSION"));

    Ok(())
}

fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create_initial_tables",
            sql: r"
                CREATE TABLE accounts (uid INTEGER, platform TEXT NOT NULL DEFAULT 'bilibili', name TEXT, avatar TEXT, csrf TEXT, cookies TEXT, created_at TEXT, PRIMARY KEY(uid, platform));
                CREATE TABLE recorders (room_id INTEGER PRIMARY KEY, platform TEXT NOT NULL DEFAULT 'bilibili', created_at TEXT);
                CREATE TABLE records (live_id TEXT PRIMARY KEY, platform TEXT NOT NULL DEFAULT 'bilibili', room_id INTEGER, title TEXT, length INTEGER, size INTEGER, cover BLOB, created_at TEXT);
                CREATE TABLE danmu_statistics (live_id TEXT PRIMARY KEY, room_id INTEGER, value INTEGER, time_point TEXT);
                CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT, content TEXT, read INTEGER, created_at TEXT);
                CREATE TABLE videos (id INTEGER PRIMARY KEY AUTOINCREMENT, room_id INTEGER, cover TEXT, file TEXT, length INTEGER, size INTEGER, status INTEGER, bvid TEXT, title TEXT, desc TEXT, tags TEXT, area INTEGER, created_at TEXT);
                ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "add_auto_start_column",
            sql: r"ALTER TABLE recorders ADD COLUMN auto_start INTEGER NOT NULL DEFAULT 1;",
            kind: MigrationKind::Up,
        },
        // add platform column to videos table
        Migration {
            version: 3,
            description: "add_platform_column",
            sql: r"ALTER TABLE videos ADD COLUMN platform TEXT;",
            kind: MigrationKind::Up,
        },
        // add task table to record encode/upload task
        Migration {
            version: 4,
            description: "add_task_table",
            sql: r"CREATE TABLE tasks (id TEXT PRIMARY KEY, type TEXT, status TEXT, message TEXT, metadata TEXT, created_at TEXT);",
            kind: MigrationKind::Up,
        },
        // add id_str column to support string IDs like Douyin sec_uid while keeping uid for Bilibili compatibility
        Migration {
            version: 5,
            description: "add_id_str_column",
            sql: r"ALTER TABLE accounts ADD COLUMN id_str TEXT;",
            kind: MigrationKind::Up,
        },
        // add extra column to recorders
        Migration {
            version: 6,
            description: "add_extra_column_to_recorders",
            sql: r"ALTER TABLE recorders ADD COLUMN extra TEXT;",
            kind: MigrationKind::Up,
        },
        // add indexes
        Migration {
            version: 7,
            description: "add_indexes",
            sql: r"
                CREATE INDEX idx_records_live_id ON records (room_id, live_id);
                CREATE INDEX idx_records_created_at ON records (room_id, created_at);
                CREATE INDEX idx_videos_room_id ON videos (room_id);
                CREATE INDEX idx_videos_created_at ON videos (created_at);
            ",
            kind: MigrationKind::Up,
        },
        // add note column for video
        Migration {
            version: 8,
            description: "add_note_column_for_video",
            sql: r"ALTER TABLE videos ADD COLUMN note TEXT;",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 9,
            description: "add_parent_id_column_for_record",
            sql: r"ALTER TABLE records ADD COLUMN parent_id TEXT;",
            kind: MigrationKind::Up,
        },
        // change records table primary key to (parent_id, live_id)
        Migration {
            version: 10,
            description: "change_records_primary_key",
            sql: r"
                CREATE TABLE records_new (
                    parent_id TEXT NOT NULL DEFAULT '',
                    live_id TEXT NOT NULL,
                    platform TEXT NOT NULL DEFAULT 'bilibili',
                    room_id TEXT,
                    title TEXT,
                    length INTEGER,
                    size INTEGER,
                    cover BLOB,
                    created_at TEXT,
                    PRIMARY KEY (parent_id, live_id)
                );
                INSERT INTO records_new (
                    parent_id,
                    live_id,
                    platform,
                    room_id,
                    title,
                    length,
                    size,
                    cover,
                    created_at
                )
                SELECT
                    COALESCE(parent_id, live_id) AS parent_id,
                    live_id,
                    platform,
                    CAST(room_id AS TEXT),
                    title,
                    length,
                    size,
                    cover,
                    created_at
                FROM records;
                DROP TABLE records;
                ALTER TABLE records_new RENAME TO records;
                CREATE INDEX IF NOT EXISTS idx_records_live_id ON records (room_id, live_id);
                CREATE INDEX IF NOT EXISTS idx_records_created_at ON records (room_id, created_at);
            ",
            kind: MigrationKind::Up,
        },
        // convert room_id columns to TEXT across tables
        Migration {
            version: 11,
            description: "convert_room_id_to_text",
            sql: r"
                CREATE TABLE recorders_new (
                    room_id TEXT PRIMARY KEY,
                    platform TEXT NOT NULL DEFAULT 'bilibili',
                    created_at TEXT,
                    auto_start INTEGER NOT NULL DEFAULT 1,
                    extra TEXT
                );
                INSERT INTO recorders_new (
                    room_id,
                    platform,
                    created_at,
                    auto_start,
                    extra
                )
                SELECT
                    CAST(room_id AS TEXT),
                    platform,
                    created_at,
                    COALESCE(auto_start, 1),
                    extra
                FROM recorders;
                DROP TABLE recorders;
                ALTER TABLE recorders_new RENAME TO recorders;

                CREATE TABLE danmu_statistics_new (
                    live_id TEXT PRIMARY KEY,
                    room_id TEXT,
                    value INTEGER,
                    time_point TEXT
                );
                INSERT INTO danmu_statistics_new (
                    live_id,
                    room_id,
                    value,
                    time_point
                )
                SELECT
                    live_id,
                    CAST(room_id AS TEXT),
                    value,
                    time_point
                FROM danmu_statistics;
                DROP TABLE danmu_statistics;
                ALTER TABLE danmu_statistics_new RENAME TO danmu_statistics;

                CREATE TABLE videos_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    room_id TEXT,
                    cover TEXT,
                    file TEXT,
                    length INTEGER,
                    size INTEGER,
                    status INTEGER,
                    bvid TEXT,
                    title TEXT,
                    desc TEXT,
                    tags TEXT,
                    area INTEGER,
                    created_at TEXT,
                    platform TEXT,
                    note TEXT
                );
                INSERT INTO videos_new (
                    id,
                    room_id,
                    cover,
                    file,
                    length,
                    size,
                    status,
                    bvid,
                    title,
                    desc,
                    tags,
                    area,
                    created_at,
                    platform,
                    note
                )
                SELECT
                    id,
                    CAST(room_id AS TEXT),
                    cover,
                    file,
                    length,
                    size,
                    status,
                    bvid,
                    title,
                    desc,
                    tags,
                    area,
                    created_at,
                    platform,
                    note
                FROM videos;
                DROP TABLE videos;
                ALTER TABLE videos_new RENAME TO videos;

                CREATE INDEX IF NOT EXISTS idx_videos_room_id ON videos (room_id);
                CREATE INDEX IF NOT EXISTS idx_videos_created_at ON videos (created_at);
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 12,
            description: "convert_account_uid_to_text",
            sql: r"
                CREATE TABLE accounts_new (
                    uid TEXT,
                    platform TEXT NOT NULL DEFAULT 'bilibili',
                    name TEXT,
                    avatar TEXT,
                    csrf TEXT,
                    cookies TEXT,
                    created_at TEXT,
                    PRIMARY KEY (uid, platform)
                );
                INSERT INTO accounts_new (
                    uid,
                    platform,
                    name,
                    avatar,
                    csrf,
                    cookies,
                    created_at
                )
                SELECT
                    COALESCE(NULLIF(id_str, ''), CAST(uid AS TEXT)),
                    platform,
                    name,
                    avatar,
                    csrf,
                    cookies,
                    created_at
                FROM accounts;
                DROP TABLE accounts;
                ALTER TABLE accounts_new RENAME TO accounts;
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 13,
            description: "change_records_length_to_float",
            sql: r"
            ALTER TABLE records ADD COLUMN length_float FLOAT;
            UPDATE records SET length_float = CAST(length AS FLOAT);
            ALTER TABLE records DROP COLUMN length;
            ALTER TABLE records RENAME COLUMN length_float TO length;
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 14,
            description: "add_review_samples_table",
            sql: r"
                CREATE TABLE review_samples (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    sample_no TEXT NOT NULL UNIQUE,
                    product TEXT NOT NULL DEFAULT '',
                    category TEXT NOT NULL DEFAULT '',
                    deal_status TEXT NOT NULL DEFAULT '',
                    evidence_strength TEXT NOT NULL DEFAULT '',
                    transcript_path TEXT NOT NULL DEFAULT '',
                    data_screenshot_path TEXT NOT NULL DEFAULT '',
                    review_status TEXT NOT NULL DEFAULT '已复盘',
                    is_b_baseline INTEGER NOT NULL DEFAULT 0,
                    ops_score INTEGER,
                    host_score INTEGER,
                    control_score INTEGER,
                    main_issue TEXT NOT NULL DEFAULT '',
                    notes TEXT NOT NULL DEFAULT '',
                    clip_type TEXT NOT NULL DEFAULT '无法判断',
                    source_video_path TEXT NOT NULL DEFAULT '',
                    review_file_path TEXT NOT NULL DEFAULT '',
                    transcription_quality TEXT NOT NULL DEFAULT '',
                    agent_version TEXT NOT NULL DEFAULT 'V1.1',
                    calibration_score INTEGER,
                    fact_accuracy_score INTEGER,
                    key_action_score INTEGER,
                    oral_usability_score INTEGER,
                    training_value_score INTEGER,
                    review_content TEXT NOT NULL DEFAULT '',
                    video_id INTEGER,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE INDEX idx_review_samples_category ON review_samples(category);
                CREATE INDEX idx_review_samples_clip_type ON review_samples(clip_type);
                CREATE INDEX idx_review_samples_baseline ON review_samples(is_b_baseline);
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 15,
            description: "add_transcript_dictionary_candidates_table",
            sql: database::transcript_dictionary_candidate::TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 16,
            description: "add_knowledge_snapshot_tables",
            sql: database::knowledge::KNOWLEDGE_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 17,
            description: "add_knowledge_document_classification",
            sql: database::knowledge::KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 18,
            description: "backfill_knowledge_document_classification",
            sql: database::knowledge::KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 19,
            description: "add_master_script_tables",
            sql: database::master_script::MASTER_SCRIPT_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 20,
            description: "add_knowledge_asr_eligibility",
            sql: database::knowledge::KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 21,
            description: "add_master_sample_batches",
            sql: database::master_sample_batch::MASTER_SAMPLE_BATCH_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 22,
            description: "add_master_sample_batch_syntheses",
            sql: database::master_sample_batch::MASTER_SAMPLE_BATCH_SYNTHESIS_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 23,
            description: "add_video_transcript_chunks",
            sql: database::video::VIDEO_TRANSCRIPT_CHUNKS_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 24,
            description: "add_master_sample_batch_purpose",
            sql: database::master_sample_batch::MASTER_SAMPLE_BATCH_PURPOSE_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 25,
            description: "add_video_archive_jobs",
            sql: database::video_archive::VIDEO_ARCHIVE_JOBS_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 26,
            description: "add_master_upgrade_reviews",
            sql: database::master_script::MASTER_UPGRADE_REVIEW_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 27,
            description: "add_video_anchor_detection",
            sql: database::video::VIDEO_ANCHOR_DETECTION_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 28,
            description: "add_record_anchor_detection",
            sql: database::record::RECORD_ANCHOR_DETECTION_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: 29,
            description: "add_live_dashboard_tables",
            sql: database::live_dashboard::LIVE_DASHBOARD_MIGRATION_SQL,
            kind: MigrationKind::Up,
        },
    ]
}

#[cfg(feature = "headless")]
#[derive(Debug)]
struct MigrationList(Vec<Migration>);

#[cfg(feature = "headless")]
impl MigrationSource<'static> for MigrationList {
    fn resolve(self) -> BoxFuture<'static, std::result::Result<Vec<SqlxMigration>, BoxDynError>> {
        Box::pin(async move {
            let mut migrations = Vec::new();
            for migration in self.0 {
                if matches!(migration.kind, MigrationKind::Up) {
                    migrations.push(SqlxMigration::new(
                        migration.version,
                        migration.description.into(),
                        migration.kind.into(),
                        migration.sql.into(),
                        false,
                    ));
                }
            }
            Ok(migrations)
        })
    }
}

#[cfg(feature = "headless")]
async fn setup_server_state(args: Args) -> Result<State, Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    use crate::{
        constants::API_PORT, static_server::StaticServer,
        storage_migration::StorageMigrationStatus, task::TaskManager,
    };
    use progress::progress_manager::ProgressManager;
    use progress::progress_reporter::EventEmitter;

    setup_logging(Path::new("./")).await?;
    log::info!("Setting up server state...");
    let config_path = PathBuf::from(&args.config);
    let cache_path = PathBuf::from("./cache");
    let output_path = PathBuf::from("./output");
    let config = match Config::load(&config_path, &cache_path, &output_path) {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to load config: {e}");
            return Err(e.into());
        }
    };
    let config = Arc::new(RwLock::new(config));
    let db = Arc::new(Database::new());
    // connect to sqlite database

    let conn_url = format!("sqlite:{}/data_v2.db", args.db);
    // create db folder if not exists
    if !Path::new(&args.db).exists() {
        std::fs::create_dir_all(&args.db)?;
    }

    if !Sqlite::database_exists(&conn_url).await.unwrap_or(false) {
        Sqlite::create_database(&conn_url).await?;
    }
    let db_pool: Pool<Sqlite> = Pool::connect(&conn_url).await?;
    let migrations = get_migrations();

    let migrator = Migrator::new(MigrationList(migrations))
        .await
        .expect("Failed to create migrator");
    migrator
        .run(&db_pool)
        .await
        .expect("Failed to run migrations");

    db.set(db_pool).await;
    db.finish_pending_tasks().await?;

    let progress_manager = Arc::new(ProgressManager::new());
    let emitter = EventEmitter::new(progress_manager.get_event_sender());
    let webhook_poster =
        webhook::poster::create_webhook_poster(&config.read().await.webhook_url, None).unwrap();
    let mut task_manager = TaskManager::new();
    task_manager.start();
    let task_manager = Arc::new(task_manager);
    let nas_archive = Arc::new(nas_archive::NasArchiveService::new(
        db.clone(),
        config.clone(),
    ));
    let recorder_manager = Arc::new(RecorderManager::new(
        emitter,
        db.clone(),
        config.clone(),
        task_manager.clone(),
        webhook_poster.clone(),
        nas_archive.clone(),
    ));
    nas_archive.clone().start();

    // In headless/Docker, cache and output are served from the API server (API_PORT), so no
    // separate static server is needed and only one port need be exposed. Use a dedicated
    // constructor to make this intent explicit.
    let static_server = Arc::new(StaticServer::headless(API_PORT));

    let _ = try_rebuild_archives(&db, config.read().await.cache.clone().into()).await;
    let _ = try_convert_live_covers(&db, config.read().await.cache.clone().into()).await;
    let _ = try_convert_clip_covers(&db, config.read().await.output.clone().into()).await;
    let _ = try_add_parent_id_to_records(&db).await;
    let _ = try_convert_entry_to_m3u8(&db, config.read().await.cache.clone().into()).await;

    let storage_migration = StorageMigrationStatus::new();

    Ok(State {
        db,
        config,
        webhook_poster,
        recorder_manager,
        nas_archive,
        task_manager,
        static_server,
        storage_migration,
        progress_manager,
        readonly: args.readonly,
    })
}

#[cfg(feature = "gui")]
async fn setup_app_state(app: &tauri::App) -> Result<State, Box<dyn std::error::Error>> {
    use platform_dirs::AppDirs;
    use progress::progress_reporter::EventEmitter;

    use crate::{
        static_server::start_static_server, storage_migration::StorageMigrationStatus,
        task::TaskManager,
    };

    let log_dir = app.path().app_log_dir()?;
    setup_logging(&log_dir).await?;

    log::info!("Setting up app state...");
    let app_dirs = AppDirs::new(Some("cn.vjoi.bili-shadowreplay"), false).unwrap();
    let config_path = app_dirs.config_dir.join("Conf.toml");
    let cache_path = app_dirs.cache_dir.join("cache");
    let output_path = app_dirs.data_dir.join("output");
    log::info!("Loading config from {config_path:?}");
    let config = match Config::load(&config_path, &cache_path, &output_path) {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to load config, exiting: {e}");
            return Err(e.into());
        }
    };

    let config = Arc::new(RwLock::new(config));
    let config_clone = config.clone();
    let dbs = app.state::<tauri_plugin_sql::DbInstances>().inner();
    let db = Arc::new(Database::new());
    let db_clone = db.clone();
    let emitter = EventEmitter::new(app.handle().clone());
    let binding = dbs.0.read().await;
    let dbpool = binding.get("sqlite:data_v2.db").unwrap();
    let sqlite_pool = match dbpool {
        tauri_plugin_sql::DbPool::Sqlite(pool) => Some(pool),
    };
    db_clone.set(sqlite_pool.unwrap().clone()).await;
    db_clone.finish_pending_tasks().await?;
    let webhook_poster =
        webhook::poster::create_webhook_poster(&config.read().await.webhook_url, None).unwrap();
    let mut task_manager = TaskManager::new();
    task_manager.start();

    let task_manager = Arc::new(task_manager);
    let nas_archive = Arc::new(nas_archive::NasArchiveService::new(
        db.clone(),
        config.clone(),
    ));

    let recorder_manager = Arc::new(RecorderManager::new(
        app.app_handle().clone(),
        emitter,
        db.clone(),
        config.clone(),
        task_manager.clone(),
        webhook_poster.clone(),
        nas_archive.clone(),
    ));
    nas_archive.clone().start();

    let static_server = Arc::new(start_static_server(config.clone()).await?);

    // try to rebuild archive table
    let cache_path = config_clone.read().await.cache.clone();
    let output_path = config_clone.read().await.output.clone();
    if let Err(e) = try_rebuild_archives(&db_clone, cache_path.clone().into()).await {
        log::warn!("Rebuilding archive table failed: {e}");
    }
    let _ = try_convert_live_covers(&db_clone, cache_path.clone().into()).await;
    let _ = try_convert_clip_covers(&db_clone, output_path.clone().into()).await;
    let _ = try_add_parent_id_to_records(&db_clone).await;
    let _ = try_convert_entry_to_m3u8(&db_clone, cache_path.clone().into()).await;

    let storage_migration = StorageMigrationStatus::new();

    Ok(State {
        db,
        config,
        recorder_manager,
        nas_archive,
        task_manager,
        static_server,
        storage_migration,
        app_handle: app.handle().clone(),
        webhook_poster,
    })
}

#[cfg(feature = "gui")]
fn setup_plugins(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let migrations = get_migrations();
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:data_v2.db", migrations)
                .build(),
        )
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init());

    println!("Plugins initialized");

    builder
}

#[cfg(feature = "gui")]
fn setup_event_handlers(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.on_window_event(|window, event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            // main window is not closable
            if window.label() == "main" {
                window.hide().unwrap();
                api.prevent_close();
            }
        }
    })
}

#[cfg(feature = "gui")]
fn setup_invoke_handlers(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        crate::handlers::account::get_accounts,
        crate::handlers::account::add_account,
        crate::handlers::account::remove_account,
        crate::handlers::account::get_account_count,
        crate::handlers::account::get_qr_status,
        crate::handlers::account::get_qr,
        crate::handlers::account::open_douyin_login,
        crate::handlers::account::get_douyin_login_cookies,
        crate::handlers::account::close_douyin_login,
        crate::handlers::ai::minimax_chat,
        crate::handlers::config::get_config,
        crate::handlers::config::get_static_port,
        crate::handlers::config::get_storage_migration_status,
        crate::handlers::config::set_cache_path,
        crate::handlers::config::set_output_path,
        crate::handlers::config::update_notify,
        crate::handlers::config::update_whisper_model,
        crate::handlers::config::update_subtitle_setting,
        crate::handlers::config::update_clip_name_format,
        crate::handlers::config::update_whisper_prompt,
        crate::handlers::config::update_subtitle_generator_type,
        crate::handlers::config::update_openai_api_key,
        crate::handlers::config::update_openai_api_endpoint,
        crate::handlers::config::update_volcengine_asr_config,
        crate::handlers::config::update_auto_generate,
        crate::handlers::config::update_status_check_interval,
        crate::handlers::config::update_whisper_language,
        crate::handlers::config::update_webhook_url,
        crate::handlers::config::update_danmu_ass_options,
        crate::handlers::config::update_powerlive_key,
        crate::handlers::config::test_nas_video_storage,
        crate::handlers::config::update_nas_video_storage,
        crate::handlers::knowledge::inspect_knowledge_vault,
        crate::handlers::knowledge::connect_knowledge_vault,
        crate::handlers::knowledge::sync_knowledge_vault,
        crate::handlers::knowledge::get_knowledge_status,
        crate::handlers::knowledge::get_enterprise_product_dictionary,
        crate::handlers::knowledge::open_knowledge_vault,
        crate::handlers::live_dashboard::import_live_dashboard_xlsx,
        crate::handlers::live_dashboard::list_live_dashboard_sessions,
        crate::handlers::live_dashboard::get_live_dashboard_detail,
        crate::handlers::live_dashboard::get_live_dashboard_settings,
        crate::handlers::live_dashboard::set_live_dashboard_download_dir,
        crate::handlers::master_script::start_master_ingest,
        crate::handlers::master_script::create_master_sample_batch,
        crate::handlers::master_script::list_master_sample_batches,
        crate::handlers::master_script::get_master_sample_batch,
        crate::handlers::master_script::start_master_sample_batch_processing,
        crate::handlers::master_script::generate_master_sample_batch_draft,
        crate::handlers::master_script::publish_master_sample_batch_draft,
        crate::handlers::master_script::publish_corrected_master_sample_batch_version,
        crate::handlers::master_script::list_unbatched_master_source_video_ids,
        crate::handlers::master_script::resume_master_ingest,
        crate::handlers::master_script::preview_master_script,
        crate::handlers::master_script::publish_master_script,
        crate::handlers::master_script::get_master_script_status,
        crate::handlers::master_script::get_master_baseline,
        crate::handlers::master_script::list_support_candidates,
        crate::handlers::master_script::decide_support_candidate,
        crate::handlers::master_script::preview_master_upgrade,
        crate::handlers::master_script::publish_master_upgrade,
        crate::handlers::master_script::compare_highlight_to_master,
        crate::handlers::master_script::retry_master_upgrade_review,
        crate::handlers::message::get_messages,
        crate::handlers::message::read_message,
        crate::handlers::message::delete_message,
        crate::handlers::recorder::get_recorder_list,
        crate::handlers::recorder::add_recorder,
        crate::handlers::recorder::remove_recorder,
        crate::handlers::recorder::get_room_info,
        crate::handlers::recorder::get_archive_disk_usage,
        crate::handlers::recorder::get_archives,
        crate::handlers::recorder::get_archive,
        crate::handlers::recorder::get_archives_by_parent_id,
        crate::handlers::recorder::get_archive_subtitle,
        crate::handlers::recorder::get_archive_transcript_audit,
        crate::handlers::transcript_review::get_transcript_audit,
        crate::handlers::recorder::save_archive_fact_card,
        crate::handlers::recorder::resolve_archive_review_item,
        crate::handlers::transcript_review::resolve_transcript_correction,
        crate::handlers::transcript_review::list_transcript_dictionary_candidates,
        crate::handlers::transcript_review::set_transcript_dictionary_candidate_status,
        crate::handlers::transcript_review::export_transcript_dictionary_candidates,
        crate::handlers::recorder::generate_archive_subtitle,
        crate::handlers::recorder::refresh_archive_subtitle,
        crate::handlers::recorder::delete_archive,
        crate::handlers::recorder::delete_archives,
        crate::handlers::recorder::get_danmu_record,
        crate::handlers::recorder::export_danmu,
        crate::handlers::recorder::send_danmaku,
        crate::handlers::recorder::get_total_length,
        crate::handlers::recorder::get_today_record_count,
        crate::handlers::recorder::get_recent_record,
        crate::handlers::recorder::set_enable,
        crate::handlers::recorder::fetch_hls,
        crate::handlers::recorder::generate_whole_clip,
        crate::handlers::review_sample::get_review_samples,
        crate::handlers::review_sample::seed_builtin_review_samples,
        crate::handlers::review_sample::save_review_sample,
        crate::handlers::review_sample::delete_review_sample,
        crate::handlers::video::clip_range,
        crate::handlers::video::upload_procedure,
        crate::handlers::video::cancel,
        crate::handlers::video::get_video,
        crate::handlers::video::get_videos,
        crate::handlers::video::get_all_videos,
        crate::handlers::video::get_video_cover,
        crate::handlers::video::delete_video,
        crate::handlers::video::list_video_archives,
        crate::handlers::video::retry_video_archive,
        crate::handlers::video::open_video_archive_location,
        crate::handlers::video::get_video_typelist,
        crate::handlers::video::update_video_cover,
        crate::handlers::video::generate_video_subtitle,
        crate::handlers::video::get_video_subtitle,
        crate::handlers::video::get_video_playback_source,
        crate::handlers::video::prepare_video_playback,
        crate::handlers::video::update_video_subtitle,
        crate::handlers::video::update_video_note,
        crate::handlers::video::encode_video_subtitle,
        crate::handlers::video::generic_ffmpeg_command,
        crate::handlers::video::import_external_video,
        crate::handlers::video::batch_import_external_videos,
        crate::handlers::video::clip_video,
        crate::handlers::video::get_file_size,
        crate::handlers::video::get_import_progress,
        crate::handlers::video::generate_audio_sample,
        crate::handlers::anchor_detection::detect_video_anchor,
        crate::handlers::anchor_detection::save_video_anchor_manual,
        crate::handlers::anchor_detection::detect_archive_anchor,
        crate::handlers::anchor_detection::save_archive_anchor_manual,
        crate::handlers::task::get_tasks,
        crate::handlers::task::delete_task,
        crate::handlers::utils::show_in_folder,
        crate::handlers::utils::export_to_file,
        crate::handlers::utils::get_disk_info,
        crate::handlers::utils::open_live,
        crate::handlers::utils::open_clip,
        crate::handlers::utils::open_log_folder,
        crate::handlers::utils::file_exists,
        crate::handlers::utils::console_log,
        crate::handlers::utils::list_folder,
        crate::handlers::video_editing::extract_video_frames,
        crate::handlers::video_editing::get_video_metadata,
        crate::handlers::video_editing::get_archive_metadata,
        crate::handlers::video_editing::analyze_danmu_highlights,
        crate::handlers::video_editing::search_danmu_keywords,
        crate::handlers::video_editing::merge_videos,
        crate::handlers::video_editing::extract_video_audio,
    ])
}

/// Initializes Sentry only when a DSN is provided at build time via the
/// `SENTRY_ENDPOINT` env var. Returns `None` (Sentry disabled) when unset or
/// empty. The returned guard must be kept alive for the lifetime of the app.
fn init_sentry_from_env() -> Option<sentry::ClientInitGuard> {
    option_env!("SENTRY_ENDPOINT")
        .filter(|s| !s.is_empty())
        .map(|dsn| {
            sentry::init((
                dsn,
                sentry::ClientOptions {
                    release: sentry::release_name!(),
                    // Capture user IPs and potentially sensitive headers when using HTTP server integrations
                    // see https://docs.sentry.io/platforms/rust/data-management/data-collected for more info
                    send_default_pii: true,
                    session_mode: sentry::SessionMode::Application,
                    ..Default::default()
                },
            ))
        })
}

#[cfg(feature = "gui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = init_sentry_from_env();

    let _ = fix_path_env::fix();

    let builder = tauri::Builder::default().plugin(tauri_plugin_deep_link::init());
    let builder = setup_plugins(builder);
    let builder = setup_event_handlers(builder);
    let builder = setup_invoke_handlers(builder);

    builder
        .setup(|app| {
            tauri::async_runtime::block_on(async {
                let state = setup_app_state(app).await?;
                let _ = tray::create_tray(app.handle());

                // check ffmpeg status
                match ffmpeg::check_ffmpeg().await {
                    Err(e) => log::error!("Failed to check ffmpeg version: {e}"),
                    Ok(v) => log::info!("Checked ffmpeg version: {v}"),
                }

                let resume_state = state.clone();
                let live_dashboard_state = state.clone();
                app.manage(state);
                tauri::async_runtime::spawn(async move {
                    crate::handlers::master_script::resume_processing_master_sample_batches(
                        resume_state,
                    )
                    .await;
                });
                tauri::async_runtime::spawn(async move {
                    crate::handlers::live_dashboard::start_live_dashboard_download_poller(
                        live_dashboard_state,
                    )
                    .await;
                });
                Ok(())
            })
        })
        .build(tauri::generate_context!())?
        .run(|app_handle: &tauri::AppHandle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // stop all recorders
                let recorder_manager = app_handle.state::<State>().recorder_manager.clone();
                log::info!("Stopping all recorders...");
                tauri::async_runtime::block_on(async move {
                    recorder_manager.stop_all().await;
                });
                log::info!("All recorders stopped successfully.");
            }
        });

    Ok(())
}

#[cfg(feature = "headless")]
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the config file
    #[arg(short, long, default_value_t = String::from("config.toml"))]
    config: String,

    /// Path to the database folder
    #[arg(short, long, default_value_t = String::from("./data"))]
    db: String,

    /// ReadOnly mode
    #[arg(short, long, default_value_t = false)]
    readonly: bool,
}

#[cfg(feature = "headless")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = init_sentry_from_env();
    // get params from command line
    let args = Args::parse();
    let state = setup_server_state(args)
        .await
        .expect("Failed to setup server state");

    // check ffmpeg status
    match ffmpeg::check_ffmpeg().await {
        Err(e) => log::error!("Failed to check ffmpeg version: {e}"),
        Ok(v) => log::info!("Checked ffmpeg version: {v}"),
    }

    http_server::api_server::start_api_server(state).await;
    Ok(())
}
