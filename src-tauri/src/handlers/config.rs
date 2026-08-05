use crate::config::{Config, NasVideoStorageConfig};
#[cfg(feature = "headless")]
use crate::constants::API_PORT;
use crate::danmu2ass::Danmu2AssOptions;
use crate::state::State;
use crate::state_type;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

fn validate_storage_path(path: &str, label: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(format!("{label}不能为空"));
    }
    if path.contains("#recycle") {
        return Err(format!(
            "{label}不能设为 NAS 回收站 (#recycle)，请在 File Station 新建文件夹后再选择"
        ));
    }
    Ok(())
}

fn ensure_storage_dir(path: &str) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("无法创建目录 {path}：{error}"))
}

fn cache_migration_should_be_skipped(old_cache_path: &str) -> bool {
    old_cache_path.contains("#recycle")
}

fn migrate_storage_entries(old_root: &str, new_root: &str, label: &str) -> Result<(), String> {
    ensure_storage_dir(new_root)?;

    let mut old_entries = Vec::new();
    if let Ok(entries) = std::fs::read_dir(old_root) {
        for entry in entries.flatten() {
            if entry.path() == Path::new(new_root) {
                continue;
            }
            old_entries.push(entry.path());
        }
    }

    let total = old_entries.len();
    log::info!("{label} migration started: {total} entries {old_root} -> {new_root}");

    for (index, entry) in old_entries.iter().enumerate() {
        let file_name = entry
            .file_name()
            .ok_or_else(|| format!("{label}迁移失败：{old_root} 中存在无效目录项"))?;
        let new_entry = Path::new(new_root).join(file_name);
        log::info!(
            "{label} migration copying {}/{}: {}",
            index + 1,
            total,
            entry.display()
        );
        if entry.is_dir() {
            crate::handlers::utils::copy_dir_all(entry, &new_entry)
                .map_err(|error| format!("迁移{label}失败（{}）：{error}", entry.display()))?;
        } else {
            std::fs::copy(entry, &new_entry)
                .map_err(|error| format!("迁移{label}失败（{}）：{error}", entry.display()))?;
        }
    }

    for entry in old_entries {
        if entry.is_dir() {
            if let Err(error) = std::fs::remove_dir_all(&entry) {
                log::error!("Remove old {label} entry error: {error}");
            }
        } else if let Err(error) = std::fs::remove_file(&entry) {
            log::error!("Remove old {label} entry error: {error}");
        }
    }

    log::info!("{label} migration finished: {new_root}");
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_storage_migration_status(
    state: state_type!(),
) -> Result<crate::storage_migration::StorageMigrationSnapshot, ()> {
    Ok(state.storage_migration.snapshot())
}

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_config(state: state_type!()) -> Result<Config, ()> {
    let mut config = state.config.read().await.clone();
    // Configuration screens only need to know whether a key is configured.
    // Never send provider secrets into WebView JavaScript or browser storage.
    config.openai_api_key.clear();
    config.powerlive_key.clear();
    config.volcengine_api_key.clear();
    config.volcengine_access_token.clear();
    Ok(config)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_static_port(_state: state_type!()) -> Result<u16, ()> {
    #[cfg(feature = "headless")]
    {
        Ok(API_PORT)
    }
    #[cfg(not(feature = "headless"))]
    {
        Ok(_state.static_server.port)
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
#[allow(dead_code)]
pub async fn set_cache_path(state: state_type!(), cache_path: String) -> Result<(), String> {
    let old_cache_path = state.config.read().await.cache.clone();
    log::info!("Try to set cache path: {old_cache_path} -> {cache_path}");
    if old_cache_path == cache_path {
        return Ok(());
    }

    validate_storage_path(&cache_path, "缓存目录")?;
    ensure_storage_dir(&cache_path)?;

    let old_cache_path_obj = std::path::Path::new(&old_cache_path);
    let new_cache_path_obj = std::path::Path::new(&cache_path);
    if new_cache_path_obj.starts_with(old_cache_path_obj) {
        log::error!("New cache path is under old cache path: {old_cache_path} -> {cache_path}");
        return Err("新缓存目录不能位于旧缓存目录内部".to_string());
    }

    state.storage_migration.set_cache(true);
    state.recorder_manager.set_migrating(true);
    state.recorder_manager.stop_all().await;

    let migration_result: Result<(), String> = async {
        if cache_migration_should_be_skipped(&old_cache_path) {
            log::warn!("Skip cache migration from recycle bin path: {old_cache_path}");
            state.config.write().await.set_cache_path(&cache_path);
            state
                .db
                .new_message(
                    "缓存目录切换",
                    "已从回收站路径切换到新目录。回收站中的旧文件未自动迁移，可在 File Station 手动处理。",
                )
                .await?;
            return Ok(());
        }

        state
            .db
            .new_message(
                "缓存目录切换",
                "缓存正在迁移中，根据数据量情况可能花费较长时间，在此期间流预览功能不可用",
            )
            .await?;

        let old_cache_path_for_task = old_cache_path.clone();
        let cache_path_for_task = cache_path.clone();
        tokio::task::spawn_blocking(move || {
            migrate_storage_entries(&old_cache_path_for_task, &cache_path_for_task, "缓存")
        })
        .await
        .map_err(|error| format!("缓存迁移任务异常：{error}"))??;

        state.config.write().await.set_cache_path(&cache_path);
        state
            .db
            .new_message("缓存目录切换", "缓存切换完成")
            .await?;
        Ok(())
    }
    .await;

    state.recorder_manager.set_migrating(false);
    state.storage_migration.set_cache(false);
    migration_result
}

#[cfg_attr(feature = "gui", tauri::command)]
#[allow(dead_code)]
pub async fn set_output_path(state: state_type!(), output_path: String) -> Result<(), String> {
    let old_output_path = {
        let config = state.config.read().await;
        config.output.clone()
    };
    log::info!("Try to set output path: {old_output_path} -> {output_path}");
    if old_output_path == output_path {
        return Ok(());
    }

    validate_storage_path(&output_path, "切片保存路径")?;
    ensure_storage_dir(&output_path)?;

    let old_output_path_obj = std::path::Path::new(&old_output_path);
    let new_output_path_obj = std::path::Path::new(&output_path);
    if new_output_path_obj.starts_with(old_output_path_obj) {
        log::error!("New output path is under old output path: {old_output_path} -> {output_path}");
        return Err("新切片目录不能位于旧切片目录内部".to_string());
    }

    state.storage_migration.set_output(true);
    let old_output_path_for_task = old_output_path.clone();
    let output_path_for_task = output_path.clone();
    let migration_result = tokio::task::spawn_blocking(move || {
        migrate_storage_entries(&old_output_path_for_task, &output_path_for_task, "切片")
    })
    .await
    .map_err(|error| format!("切片迁移任务异常：{error}"))?;

    state.storage_migration.set_output(false);

    migration_result?;
    state.config.write().await.set_output_path(&output_path);
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_notify(
    state: state_type!(),
    live_start_notify: bool,
    live_end_notify: bool,
    clip_notify: bool,
    post_notify: bool,
) -> Result<(), ()> {
    state.config.write().await.live_start_notify = live_start_notify;
    state.config.write().await.live_end_notify = live_end_notify;
    state.config.write().await.clip_notify = clip_notify;
    state.config.write().await.post_notify = post_notify;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_whisper_model(state: state_type!(), whisper_model: String) -> Result<(), ()> {
    state.config.write().await.whisper_model = whisper_model;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_subtitle_setting(state: state_type!(), auto_subtitle: bool) -> Result<(), ()> {
    state.config.write().await.auto_subtitle = auto_subtitle;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_clip_name_format(
    state: state_type!(),
    clip_name_format: String,
) -> Result<(), ()> {
    state.config.write().await.clip_name_format = clip_name_format;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_whisper_prompt(state: state_type!(), whisper_prompt: String) -> Result<(), ()> {
    state.config.write().await.whisper_prompt = whisper_prompt;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_subtitle_generator_type(
    state: state_type!(),
    subtitle_generator_type: String,
) -> Result<(), ()> {
    log::info!("Updating subtitle generator type to {subtitle_generator_type}");
    let mut config = state.config.write().await;
    config.subtitle_generator_type = subtitle_generator_type;
    config.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_openai_api_key(state: state_type!(), openai_api_key: String) -> Result<(), ()> {
    log::info!("Updating openai api key");
    let mut config = state.config.write().await;
    config.openai_api_key = openai_api_key;
    config.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_openai_api_endpoint(
    state: state_type!(),
    openai_api_endpoint: String,
) -> Result<(), ()> {
    log::info!("Updating openai api endpoint to {openai_api_endpoint}");
    let mut config = state.config.write().await;
    config.openai_api_endpoint = openai_api_endpoint;
    config.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_volcengine_asr_config(
    state: state_type!(),
    api_key: String,
    app_id: String,
    access_token: String,
    resource_id: String,
    boosting_table_id: String,
    correct_table_id: String,
) -> Result<(), String> {
    let mut config = state.config.write().await;
    if !api_key.trim().is_empty() {
        config.volcengine_api_key = api_key.trim().to_string();
    }
    config.volcengine_app_id = app_id.trim().to_string();
    if !access_token.trim().is_empty() {
        config.volcengine_access_token = access_token.trim().to_string();
    }
    config.volcengine_resource_id = if resource_id.trim().is_empty() {
        "volc.seedasr.auc".to_string()
    } else {
        match resource_id.trim() {
            "volc.bigasr.auc_turbo" => "volc.seedasr.auc".to_string(),
            value => value.to_string(),
        }
    };
    config.volcengine_boosting_table_id = boosting_table_id.trim().to_string();
    config.volcengine_correct_table_id = correct_table_id.trim().to_string();
    config.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_auto_generate(
    state: state_type!(),
    enabled: bool,
    encode_danmu: bool,
) -> Result<(), String> {
    let mut config = state.config.write().await;
    config.auto_generate.enabled = enabled;
    config.auto_generate.encode_danmu = encode_danmu;
    config.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_status_check_interval(
    state: state_type!(),
    mut interval: u64,
) -> Result<(), ()> {
    if interval < 10 {
        interval = 10; // Minimum interval of 10 seconds
    }
    log::info!("Updating status check interval to {interval} seconds");
    state
        .config
        .write()
        .await
        .set_status_check_interval(interval);
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_whisper_language(
    state: state_type!(),
    whisper_language: String,
) -> Result<(), ()> {
    log::info!("Updating whisper language to {whisper_language}");
    state.config.write().await.whisper_language = whisper_language;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_webhook_url(state: state_type!(), webhook_url: String) -> Result<(), ()> {
    log::info!("Updating webhook url to {webhook_url}");
    let _ = state
        .webhook_poster
        .update_config(crate::webhook::poster::WebhookConfig {
            url: webhook_url.clone(),
            ..Default::default()
        })
        .await;
    state.config.write().await.webhook_url = webhook_url;
    state.config.write().await.save();
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_danmu_ass_options(
    state: state_type!(),
    font_size: f64,
    opacity: f64,
) -> Result<(), ()> {
    log::info!("Updating danmu ass options");
    state
        .config
        .write()
        .await
        .set_danmu_ass_options(Danmu2AssOptions { font_size, opacity });
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_powerlive_key(state: state_type!(), powerlive_key: String) -> Result<(), ()> {
    state.config.write().await.powerlive_key = powerlive_key.clone();
    state.config.write().await.save();
    log::info!("Updated powerlive key");
    Ok(())
}

fn validate_nas_root_path(root_path: &str) -> Result<PathBuf, String> {
    let trimmed = root_path.trim();
    if trimmed.is_empty() {
        return Err("请先填写 NAS 共享目录".to_string());
    }
    #[cfg(target_os = "windows")]
    if !trimmed.starts_with(r"\\") {
        return Err(r"NAS 共享目录必须使用 UNC 路径，例如 \\192.168.1.100\直播录像".to_string());
    }
    Ok(PathBuf::from(trimmed))
}

fn probe_nas_root(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err("NAS 共享目录不存在或当前无法访问".to_string());
    }
    let probe_path = root.join(format!(
        ".bili-shadowreplay-write-probe-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe_path)
            .map_err(|error| format!("NAS 共享目录不可写：{error}"))?;
        file.write_all(b"nas-write-probe")
            .map_err(|error| format!("NAS 写入测试失败：{error}"))?;
        file.sync_all()
            .map_err(|error| format!("NAS 写入同步失败：{error}"))?;
        Ok(())
    })();
    let cleanup_result = std::fs::remove_file(&probe_path);
    result?;
    cleanup_result.map_err(|error| format!("NAS 测试文件无法清理：{error}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NasConnectionResult {
    pub reachable: bool,
    pub root_path: String,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn test_nas_video_storage(root_path: String) -> Result<NasConnectionResult, String> {
    let root = validate_nas_root_path(&root_path)?;
    probe_nas_root(&root)?;
    Ok(NasConnectionResult {
        reachable: true,
        root_path: root.to_string_lossy().to_string(),
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_nas_video_storage(
    state: state_type!(),
    mut settings: NasVideoStorageConfig,
) -> Result<(), String> {
    settings.root_path = settings.root_path.trim().to_string();
    if settings.enabled {
        let root = validate_nas_root_path(&settings.root_path)?;
        probe_nas_root(&root)?;
    }
    let mut config = state.config.write().await;
    config.nas_video_storage = settings;
    config.save();
    drop(config);
    state.nas_archive.wake();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nas_connection_rejects_empty_path() {
        assert_eq!(
            validate_nas_root_path(" ").unwrap_err(),
            "请先填写 NAS 共享目录"
        );
    }

    #[test]
    fn storage_path_rejects_recycle_bin() {
        assert!(validate_storage_path(r"Z:\#recycle", "缓存目录").is_err());
    }

    #[test]
    fn cache_migration_is_skipped_for_recycle_bin_path() {
        assert!(cache_migration_should_be_skipped(r"Z:\#recycle"));
        assert!(!cache_migration_should_be_skipped(r"Z:\bsr-cache"));
    }

    #[test]
    fn nas_connection_probe_confirms_write_and_cleans_up() {
        let root = std::env::temp_dir().join(format!(
            "bili-shadowreplay-nas-probe-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        probe_nas_root(&root).unwrap();

        assert!(std::fs::read_dir(&root).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
