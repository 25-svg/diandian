use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::config::Config;
use crate::database::Database;
use crate::nas_archive::NasArchiveService;
#[cfg(not(feature = "headless"))]
use crate::private_distribution::updater::{BusyProbe, BusySnapshot, PrivateUpdateCoordinator};
use crate::recorder_manager::RecorderManager;
use crate::static_server::StaticServer;
use crate::storage_migration::StorageMigrationStatus;
use crate::task::TaskManager;
use crate::webhook::poster::WebhookPoster;

#[cfg(feature = "headless")]
use crate::progress::progress_manager::ProgressManager;

/// One managed embedded-preview encoder per video. Keeping the child handle
/// here lets seek, page teardown, task cancellation, and app shutdown stop the
/// actual FFmpeg process instead of only changing a database status.
pub struct VideoPreviewSession {
    pub child: Child,
    pub cache_dir: PathBuf,
    pub start_offset: f64,
}

#[derive(Clone)]
pub struct State {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub webhook_poster: WebhookPoster,
    pub recorder_manager: Arc<RecorderManager>,
    pub nas_archive: Arc<NasArchiveService>,
    pub task_manager: Arc<TaskManager>,
    pub video_preview_sessions: Arc<Mutex<HashMap<i64, VideoPreviewSession>>>,
    /// Serializes preview create/replace requests. The analysis page can emit
    /// two initialization calls at nearly the same time; without this gate
    /// both calls can spawn an encoder before either session is registered.
    pub video_preview_prepare_gate: Arc<Mutex<()>>,
    /// Gate full-media FFmpeg and ASR work so a recording never starts two
    /// resource-heavy jobs at once.
    pub media_execution_gate: Arc<Semaphore>,
    pub static_server: Arc<StaticServer>,
    pub storage_migration: Arc<StorageMigrationStatus>,
    #[cfg(not(feature = "headless"))]
    pub app_handle: tauri::AppHandle,
    #[cfg(not(feature = "headless"))]
    pub private_updater: PrivateUpdateCoordinator,
    #[cfg(feature = "headless")]
    pub progress_manager: Arc<ProgressManager>,
    #[cfg(feature = "headless")]
    pub readonly: bool,
}

impl State {
    pub async fn stop_all_video_previews(&self) {
        let sessions = {
            let mut active = self.video_preview_sessions.lock().await;
            active
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for mut session in sessions {
            let _ = session.child.kill().await;
            let _ = session.child.wait().await;
            let _ = tokio::fs::remove_dir_all(session.cache_dir).await;
        }
    }

    #[cfg(not(feature = "headless"))]
    pub async fn prepare_for_exit(&self) {
        self.private_updater.shutdown().await;
        self.task_manager.shutdown().await;
        self.stop_all_video_previews().await;
        self.recorder_manager.stop_all().await;
    }
}

#[cfg(not(feature = "headless"))]
#[async_trait::async_trait]
impl BusyProbe for State {
    async fn snapshot(&self) -> BusySnapshot {
        BusySnapshot {
            recording: self.recorder_manager.has_active_recording().await,
            queued: self.task_manager.queue_size().await,
            running: self.task_manager.running_count().await,
            previews: self.video_preview_sessions.lock().await.len(),
        }
    }
}
