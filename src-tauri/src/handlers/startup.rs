use crate::state::State;
use crate::state_type;
use serde::Serialize;
use std::path::Path;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupReadiness {
    pub wizard_completed: bool,
    pub autostart_enabled: bool,
    pub account_count: usize,
    pub recorder_count: usize,
    pub ffmpeg_ok: bool,
    pub ffmpeg_detail: String,
    pub funasr_ok: bool,
    pub funasr_detail: String,
    pub cache_ok: bool,
    pub cache_detail: String,
    pub output_ok: bool,
    pub output_detail: String,
    pub nas_configured: bool,
    pub doudian_configured: bool,
    pub ready_for_recording: bool,
}

fn path_is_file(value: &str) -> bool {
    !value.trim().is_empty() && Path::new(value).is_file()
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_startup_readiness(state: state_type!()) -> Result<StartupReadiness, String> {
    let config = state.config.read().await.clone();
    let account_count = state.db.get_accounts().await.map_err(String::from)?.len();
    let recorder_count = state.db.get_recorders().await.map_err(String::from)?.len();
    let (ffmpeg_ok, ffmpeg_detail) = match crate::ffmpeg::check_ffmpeg().await {
        Ok(version) => (true, version),
        Err(error) => (false, error),
    };
    let (funasr_ok, funasr_detail) = match crate::subtitle_generator::funasr::inspect_runtime() {
        Ok(path) => (true, path),
        Err(error) => (false, error),
    };
    let (cache_ok, cache_detail) =
        match crate::storage_readiness::probe_writable_directory(Path::new(&config.cache)) {
            Ok(()) => (true, format!("录制目录可写：{}", config.cache)),
            Err(error) => (false, error),
        };
    let (output_ok, output_detail) =
        match crate::storage_readiness::probe_writable_directory(Path::new(&config.output)) {
            Ok(()) => (true, format!("切片目录可写：{}", config.output)),
            Err(error) => (false, error),
        };
    let nas_configured =
        config.nas_video_storage.enabled && Path::new(&config.nas_video_storage.root_path).is_dir();
    let doudian_configured = path_is_file(&config.doudian_order.env_file)
        && path_is_file(&config.doudian_order.token_file)
        && !config.doudian_order.shop_id.trim().is_empty();
    let autostart_enabled = crate::autostart::is_enabled();
    Ok(StartupReadiness {
        wizard_completed: config.startup_wizard_completed,
        autostart_enabled,
        account_count,
        recorder_count,
        ffmpeg_ok,
        ffmpeg_detail,
        funasr_ok,
        funasr_detail,
        cache_ok,
        cache_detail,
        output_ok,
        output_detail,
        nas_configured,
        doudian_configured,
        ready_for_recording: account_count > 0
            && recorder_count > 0
            && ffmpeg_ok
            && cache_ok
            && output_ok
            && autostart_enabled,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_autostart_enabled(state: state_type!(), enabled: bool) -> Result<(), String> {
    crate::autostart::set_enabled(enabled)?;
    let mut config = state.config.write().await;
    config.autostart_enabled = enabled;
    config.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn complete_startup_wizard(state: state_type!()) -> Result<(), String> {
    let mut config = state.config.write().await;
    config.startup_wizard_completed = true;
    config.save();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::path_is_file;

    #[test]
    fn empty_doudian_paths_are_not_ready() {
        assert!(!path_is_file(""));
        assert!(!path_is_file("not-a-real-file"));
    }
}
