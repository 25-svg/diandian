use crate::config::Config;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingCacheReadiness {
    pub active_path: PathBuf,
    pub fell_back: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRuntimeSnapshot {
    pub preferred_cache: String,
    pub active_cache: String,
    pub using_fallback: bool,
    pub preferred_available: bool,
    pub detail: String,
}

pub fn storage_runtime_snapshot(config: &Config) -> StorageRuntimeSnapshot {
    let preferred_cache = config.preferred_cache_path().to_string();
    let active_cache = config.cache.clone();
    let using_fallback = Path::new(&preferred_cache) != Path::new(&active_cache);
    let preferred_available = probe_writable_directory(Path::new(&preferred_cache)).is_ok();
    let detail = if using_fallback {
        "当前缓存路径与首选路径不一致。录制将停止，程序不会切换到本机目录。".to_string()
    } else {
        "当前正在使用首选缓存路径。".to_string()
    };
    StorageRuntimeSnapshot {
        preferred_cache,
        active_cache,
        using_fallback,
        preferred_available,
        detail,
    }
}

pub fn probe_writable_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|error| format!("无法创建目录 {}：{error}", path.display()))?;
    if !path.is_dir() {
        return Err(format!("{} 不是文件夹", path.display()));
    }

    let probe = path.join(format!(
        ".bsr-write-check-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| format!("目录不可写 {}：{error}", path.display()))?;
    let write_result = file
        .write_all(b"bili-shadowreplay-storage-check")
        .and_then(|_| file.sync_data());
    drop(file);
    let _ = std::fs::remove_file(&probe);
    write_result.map_err(|error| format!("目录写入测试失败 {}：{error}", path.display()))
}

/// Require the configured recording cache to be writable. A disconnected NAS or
/// mapped drive must stop recording instead of silently writing to the local disk.
pub fn ensure_recording_cache(config: &mut Config) -> Result<RecordingCacheReadiness, String> {
    let configured = PathBuf::from(config.preferred_cache_path().trim());
    probe_writable_directory(&configured)
        .map_err(|error| format!("NAS 录制目录不可用，已禁止录制且不会切换到本机目录。{error}"))?;
    config.set_runtime_cache_path(&configured.to_string_lossy());
    Ok(RecordingCacheReadiness {
        active_path: configured,
        fell_back: false,
        detail: "录制目录可写".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_at(root: &Path, cache: &Path) -> Config {
        Config::load(
            &root.join("config").join("Conf.toml"),
            cache,
            &root.join("output"),
        )
        .unwrap()
    }

    #[test]
    fn writable_directory_probe_creates_and_cleans_probe_file() {
        let root = std::env::temp_dir().join(format!(
            "bsr-storage-probe-{}",
            uuid::Uuid::new_v4().simple()
        ));
        probe_writable_directory(&root).unwrap();
        assert!(root.is_dir());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_configured_cache_is_rejected_without_local_fallback() {
        let root = std::env::temp_dir().join(format!(
            "bsr-storage-fallback-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let invalid_cache = root.join("not-a-directory");
        std::fs::write(&invalid_cache, b"file blocks directory creation").unwrap();
        let mut config = config_at(&root, &invalid_cache);

        let error = ensure_recording_cache(&mut config).unwrap_err();

        assert!(error.contains("不会切换到本机目录"));
        assert_eq!(PathBuf::from(&config.cache), invalid_cache);
        assert_eq!(PathBuf::from(config.preferred_cache_path()), invalid_cache);
        assert!(!root.join("config/recording-cache").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn storage_failure_does_not_overwrite_persisted_preferred_cache() {
        let root = std::env::temp_dir().join(format!(
            "bsr-storage-preferred-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let preferred = root.join("offline-drive");
        std::fs::write(&preferred, b"blocks directory creation").unwrap();
        let mut config = config_at(&root, &preferred);

        assert!(ensure_recording_cache(&mut config).is_err());
        config.status_check_interval = 41;
        config.save();

        let reloaded = Config::load(
            &root.join("config").join("Conf.toml"),
            &root.join("unused-cache"),
            &root.join("output"),
        )
        .unwrap();
        assert_eq!(PathBuf::from(&reloaded.cache), preferred);
        assert_eq!(PathBuf::from(reloaded.preferred_cache_path()), preferred);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_snapshot_distinguishes_preferred_and_fallback_paths() {
        let root = std::env::temp_dir().join(format!(
            "bsr-storage-snapshot-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let preferred = root.join("preferred");
        let active = root.join("fallback");
        let mut config = config_at(&root, &preferred);
        std::fs::write(&preferred, b"blocks directory creation").unwrap();
        config.set_runtime_cache_path(&active.to_string_lossy());

        let snapshot = storage_runtime_snapshot(&config);

        assert!(snapshot.using_fallback);
        assert!(!snapshot.preferred_available);
        assert_eq!(PathBuf::from(snapshot.preferred_cache), preferred);
        assert_eq!(PathBuf::from(snapshot.active_cache), active);
        let _ = std::fs::remove_dir_all(root);
    }
}
