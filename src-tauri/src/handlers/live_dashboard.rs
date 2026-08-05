use std::collections::HashMap;
use std::path::Path;

use crate::database::live_dashboard::{
    LiveDashboardChannelRow, LiveDashboardProductRow, LiveDashboardSessionRow,
    LiveDashboardShortVideoRow,
};
use crate::live_dashboard_download_filter::is_official_live_dashboard_export;
use crate::live_data_import::parse_live_dashboard_xlsx;
use crate::state::State;
use crate::state_type;

#[cfg(feature = "gui")]
use tauri::{Emitter, State as TauriState};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardDetail {
    pub session: LiveDashboardSessionRow,
    pub channels: Vec<LiveDashboardChannelRow>,
    pub short_videos: Vec<LiveDashboardShortVideoRow>,
    pub products: Vec<LiveDashboardProductRow>,
}

#[cfg(feature = "gui")]
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDashboardImportFailure {
    path: String,
    message: String,
}

#[cfg(feature = "gui")]
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDashboardWatchError {
    message: String,
}

pub async fn import_live_dashboard_xlsx_path(
    state: &State,
    path: &Path,
) -> Result<LiveDashboardSessionRow, String> {
    let imported = parse_live_dashboard_xlsx(path).map_err(|error| error.to_string())?;
    state
        .db
        .upsert_live_dashboard(imported, &path.to_string_lossy())
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn import_live_dashboard_xlsx(
    state: state_type!(),
    path: String,
) -> Result<LiveDashboardSessionRow, String> {
    import_live_dashboard_xlsx_path(&state, Path::new(&path)).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_live_dashboard_sessions(
    state: state_type!(),
) -> Result<Vec<LiveDashboardSessionRow>, String> {
    state
        .db
        .list_live_dashboard_sessions()
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_live_dashboard_detail(
    state: state_type!(),
    session_id: i64,
) -> Result<LiveDashboardDetail, String> {
    let session = state
        .db
        .list_live_dashboard_sessions()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| "直播数据场次不存在".to_string())?;
    Ok(LiveDashboardDetail {
        channels: state
            .db
            .list_live_dashboard_channels(session_id)
            .await
            .map_err(|error| error.to_string())?,
        short_videos: state
            .db
            .list_live_dashboard_short_videos(session_id)
            .await
            .map_err(|error| error.to_string())?,
        products: state
            .db
            .list_live_dashboard_products(session_id)
            .await
            .map_err(|error| error.to_string())?,
        session,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_live_dashboard_settings(
    state: state_type!(),
) -> Result<HashMap<String, serde_json::Value>, String> {
    let config = state.config.read().await;
    Ok(HashMap::from([
        (
            "downloadDir".to_string(),
            serde_json::Value::String(config.live_dashboard_download_dir.clone()),
        ),
        (
            "autoDownloadEnabled".to_string(),
            serde_json::Value::Bool(config.auto_download_enabled),
        ),
    ]))
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn set_live_dashboard_download_dir(
    state: state_type!(),
    path: String,
) -> Result<(), String> {
    if path.trim().is_empty() || !Path::new(&path).is_dir() {
        return Err("下载目录不可访问，请重新选择".to_string());
    }
    let mut config = state.config.write().await;
    config.live_dashboard_download_dir = path;
    config.save();
    Ok(())
}

pub async fn start_live_dashboard_download_poller(state: State) {
    let mut observed_sizes = HashMap::<std::path::PathBuf, u64>::new();
    let mut imported_fingerprints =
        HashMap::<std::path::PathBuf, (u64, std::time::SystemTime)>::new();
    let mut last_directory_error = None::<String>;
    loop {
        let directory = state
            .config
            .read()
            .await
            .live_dashboard_download_dir
            .clone();
        if !directory.trim().is_empty() {
            match std::fs::read_dir(&directory) {
                Ok(entries) => {
                    last_directory_error = None;
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !is_official_live_dashboard_export(&path) {
                            continue;
                        }
                        let Ok(metadata) = entry.metadata() else {
                            continue;
                        };
                        let size = metadata.len();
                        let unchanged = observed_sizes.insert(path.clone(), size) == Some(size);
                        let modified = metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        if !unchanged || imported_fingerprints.get(&path) == Some(&(size, modified))
                        {
                            continue;
                        }
                        match import_live_dashboard_xlsx_path(&state, &path).await {
                            Ok(session) => {
                                imported_fingerprints.insert(path.clone(), (size, modified));
                                #[cfg(feature = "gui")]
                                if let Err(error) =
                                    state.app_handle.emit("live-dashboard-imported", &session)
                                {
                                    log::warn!(
                                        "Failed to emit live dashboard import event: {error}"
                                    );
                                }
                                #[cfg(feature = "gui")]
                                crate::compass_auto_download::notify_compass_imported(
                                    &state.app_handle,
                                    &session.started_at,
                                );
                            }
                            Err(error) => {
                                log::warn!(
                                    "Live dashboard import skipped for {}: {error}",
                                    path.display()
                                );
                                #[cfg(feature = "gui")]
                                if let Err(emit_error) = state.app_handle.emit(
                                    "live-dashboard-import-failed",
                                    LiveDashboardImportFailure {
                                        path: path.to_string_lossy().into_owned(),
                                        message: error,
                                    },
                                ) {
                                    log::warn!("Failed to emit live dashboard import failure: {emit_error}");
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    let message = format!("下载目录不可访问，已暂停自动导入：{directory}（{error}）。手动导入仍可使用。");
                    log::warn!("{message}");
                    if last_directory_error.as_deref() != Some(message.as_str()) {
                        last_directory_error = Some(message.clone());
                        #[cfg(feature = "gui")]
                        if let Err(emit_error) = state.app_handle.emit(
                            "live-dashboard-watch-error",
                            LiveDashboardWatchError { message },
                        ) {
                            log::warn!("Failed to emit live dashboard watch error: {emit_error}");
                        }
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
