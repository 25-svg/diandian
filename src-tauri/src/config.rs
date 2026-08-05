use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{self, AtomicU64};
use std::sync::Arc;

use crate::{danmu2ass::Danmu2AssOptions, recorder_manager::ClipRangeParams};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct NasVideoStorageConfig {
    pub enabled: bool,
    pub root_path: String,
    pub archive_recordings: bool,
    pub archive_imports: bool,
    pub delete_local_after_archive: bool,
}

impl Default for NasVideoStorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            root_path: String::new(),
            archive_recordings: true,
            archive_imports: true,
            delete_local_after_archive: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct DoudianOrderConfig {
    pub env_file: String,
    pub token_file: String,
    pub sdk_path: String,
    pub shop_id: String,
}

impl Default for DoudianOrderConfig {
    fn default() -> Self {
        let desktop = std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Public"))
            .join("Desktop");
        Self {
            env_file: desktop.join(".env").to_string_lossy().to_string(),
            token_file: desktop
                .join("doudian_token.env")
                .to_string_lossy()
                .to_string(),
            sdk_path: desktop
                .join("doudian-sdk-python-1.1.0-20260724091610")
                .join("sdk-python")
                .to_string_lossy()
                .to_string(),
            shop_id: String::new(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub cache: String,
    pub output: String,
    pub live_start_notify: bool,
    pub live_end_notify: bool,
    pub clip_notify: bool,
    pub post_notify: bool,
    #[serde(default = "default_auto_subtitle")]
    pub auto_subtitle: bool,
    #[serde(default = "default_subtitle_generator_type")]
    pub subtitle_generator_type: String,
    #[serde(default = "default_whisper_model")]
    pub whisper_model: String,
    #[serde(default = "default_whisper_prompt")]
    pub whisper_prompt: String,
    #[serde(default = "default_openai_api_endpoint")]
    pub openai_api_endpoint: String,
    #[serde(default = "default_openai_api_key")]
    pub openai_api_key: String,
    #[serde(default = "default_admin_mode")]
    pub admin_mode: bool,
    #[serde(default = "default_clip_name_format")]
    pub clip_name_format: String,
    #[serde(default = "default_auto_generate_config")]
    pub auto_generate: AutoGenerateConfig,
    #[serde(default = "default_status_check_interval")]
    pub status_check_interval: u64,
    #[serde(skip)]
    pub config_path: String,
    #[serde(default = "default_whisper_language")]
    pub whisper_language: String,
    #[serde(default = "default_webhook_url")]
    pub webhook_url: String,
    #[serde(default = "default_danmu_ass_options")]
    pub danmu_ass_options: Danmu2AssOptions,
    #[serde(skip)]
    pub update_interval: Arc<AtomicU64>,
    #[serde(default = "default_powerlive_key")]
    pub powerlive_key: String,
    #[serde(default)]
    pub volcengine_api_key: String,
    #[serde(default)]
    pub volcengine_app_id: String,
    #[serde(default)]
    pub volcengine_access_token: String,
    #[serde(default = "default_volcengine_resource_id")]
    pub volcengine_resource_id: String,
    #[serde(default)]
    pub volcengine_boosting_table_id: String,
    #[serde(default)]
    pub volcengine_correct_table_id: String,
    #[serde(default)]
    pub knowledge_vault_path: String,
    #[serde(default)]
    pub nas_video_storage: NasVideoStorageConfig,
    #[serde(default = "default_live_dashboard_download_dir")]
    pub live_dashboard_download_dir: String,
    #[serde(default)]
    pub auto_download_enabled: bool,
    #[serde(default)]
    pub doudian_order: DoudianOrderConfig,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct AutoGenerateConfig {
    pub enabled: bool,
    pub encode_danmu: bool,
}

fn default_danmu_ass_options() -> Danmu2AssOptions {
    Danmu2AssOptions::default()
}

fn default_auto_subtitle() -> bool {
    true
}

fn default_subtitle_generator_type() -> String {
    "funasr".to_string()
}

fn default_whisper_model() -> String {
    "whisper_model.bin".to_string()
}

fn default_whisper_prompt() -> String {
    "这是一段中文二手相机、镜头直播带货口播。常见词包括：佳能、尼康、索尼、小白兔、70-200、EOS R6 Mark II、R62、RF 24-240、成色、99新、在仓现货、前盖、后盖、遮光罩、脚架环、镜片、卡口、到手价、优惠价、小黄车、置顶链接、UV镜、下单、备注。请忠实转写主播原话，商品型号、链接号和价格数字必须准确，不补写没有听到的内容。".to_string()
}

fn default_openai_api_endpoint() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_api_key() -> String {
    String::new()
}

fn default_admin_mode() -> bool {
    false
}

fn default_clip_name_format() -> String {
    "[{room_id}][{note}][{live_id}][{title}][{created_at}].mp4".to_string()
}

fn default_auto_generate_config() -> AutoGenerateConfig {
    AutoGenerateConfig {
        enabled: false,
        encode_danmu: false,
    }
}

fn default_status_check_interval() -> u64 {
    30
}

fn default_whisper_language() -> String {
    "zh".to_string()
}

fn default_webhook_url() -> String {
    String::new()
}

fn default_powerlive_key() -> String {
    String::new()
}

fn default_volcengine_resource_id() -> String {
    "volc.seedasr.auc".to_string()
}

fn default_live_dashboard_download_dir() -> String {
    std::env::var("USERPROFILE")
        .map(|profile| {
            PathBuf::from(profile)
                .join("Downloads")
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default()
}

impl Config {
    pub fn load(
        config_path: &PathBuf,
        default_cache: &Path,
        default_output: &Path,
    ) -> Result<Self, String> {
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(mut config) = toml::from_str::<Config>(&content) {
                config.config_path = config_path.to_str().unwrap().into();
                config.update_interval = Arc::new(AtomicU64::new(config.status_check_interval));
                if config.volcengine_resource_id.trim().is_empty()
                    || config.volcengine_resource_id == "volc.bigasr.auc_turbo"
                {
                    log::info!("Migrating Volcengine ASR configuration to Seed ASR 2.0");
                    config.volcengine_resource_id = default_volcengine_resource_id();
                    config.save();
                }
                return Ok(config);
            }
        }

        if let Some(dir_path) = PathBuf::from(config_path).parent() {
            if let Err(e) = std::fs::create_dir_all(dir_path) {
                return Err(format!("Failed to create config dir: {e}"));
            }
        }

        let config = Config {
            cache: default_cache.to_str().unwrap().into(),
            output: default_output.to_str().unwrap().into(),
            live_start_notify: true,
            live_end_notify: true,
            clip_notify: true,
            post_notify: true,
            auto_subtitle: true,
            subtitle_generator_type: default_subtitle_generator_type(),
            whisper_model: default_whisper_model(),
            whisper_prompt: default_whisper_prompt(),
            openai_api_endpoint: default_openai_api_endpoint(),
            openai_api_key: default_openai_api_key(),
            admin_mode: default_admin_mode(),
            clip_name_format: default_clip_name_format(),
            auto_generate: default_auto_generate_config(),
            status_check_interval: default_status_check_interval(),
            config_path: config_path.to_str().unwrap().into(),
            whisper_language: default_whisper_language(),
            webhook_url: default_webhook_url(),
            danmu_ass_options: default_danmu_ass_options(),
            update_interval: Arc::new(AtomicU64::new(default_status_check_interval())),
            powerlive_key: default_powerlive_key(),
            volcengine_api_key: String::new(),
            volcengine_app_id: String::new(),
            volcengine_access_token: String::new(),
            volcengine_resource_id: default_volcengine_resource_id(),
            volcengine_boosting_table_id: String::new(),
            volcengine_correct_table_id: String::new(),
            knowledge_vault_path: String::new(),
            nas_video_storage: NasVideoStorageConfig::default(),
            live_dashboard_download_dir: default_live_dashboard_download_dir(),
            auto_download_enabled: false,
            doudian_order: DoudianOrderConfig::default(),
        };

        config.save();

        Ok(config)
    }

    pub fn save(&self) {
        let content = toml::to_string(&self).unwrap();
        if let Err(e) = std::fs::write(self.config_path.clone(), content) {
            log::error!("Failed to save config: {} {}", e, self.config_path);
        }
    }

    #[allow(dead_code)]
    pub fn set_cache_path(&mut self, path: &str) {
        self.cache = path.to_string();
        self.save();
    }

    #[allow(dead_code)]
    pub fn set_output_path(&mut self, path: &str) {
        self.output = path.into();
        self.save();
    }

    #[allow(dead_code)]
    pub fn set_whisper_language(&mut self, language: &str) {
        self.whisper_language = language.to_string();
        self.save();
    }

    #[allow(dead_code)]
    pub fn set_danmu_ass_options(&mut self, options: Danmu2AssOptions) {
        self.danmu_ass_options = options;
        self.save();
    }

    pub fn generate_clip_name(&self, params: &ClipRangeParams) -> PathBuf {
        // get format config
        // filter special characters from title to make sure file name is valid
        let title = params
            .title
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();
        let format_config = self.clip_name_format.clone();
        let format_config = format_config.replace("{title}", &title);
        let format_config = format_config.replace("{platform}", &params.platform);
        let format_config = format_config.replace("{room_id}", &params.room_id.to_string());
        let format_config = format_config.replace("{live_id}", &params.live_id);
        let format_config = format_config.replace("{note}", &params.note);
        let format_config = format_config.replace(
            "{x}",
            &params
                .ranges
                .first()
                .map_or("0".to_string(), |r| r.start.to_string()),
        );
        let format_config = format_config.replace(
            "{y}",
            &params
                .ranges
                .last()
                .map_or("0".to_string(), |r| r.end.to_string()),
        );
        let format_config = format_config.replace(
            "{created_at}",
            &Local::now().format("%Y-%m-%d_%H-%M-%S").to_string(),
        );
        let duration = params.ranges.iter().map(|r| r.duration()).sum::<f64>();
        let format_config = format_config.replace("{length}", &duration.to_string());

        let mut format_config = format_config;
        while format_config.contains("[]") {
            format_config = format_config.replace("[]", "");
        }

        let sanitized = sanitize_filename::sanitize(&format_config);
        let output = self.output.clone();

        Path::new(&output).join(&sanitized)
    }

    pub fn set_status_check_interval(&mut self, interval: u64) {
        self.status_check_interval = interval;
        self.update_interval
            .store(interval, atomic::Ordering::Relaxed);
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bili-shadowreplay-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn nas_storage_defaults_are_safe() {
        let root = unique_test_root("nas-defaults");
        let config_path = root.join("Conf.toml");
        let cache = root.join("cache");
        let output = root.join("output");

        let config = Config::load(&config_path, &cache, &output).unwrap();

        assert!(!config.nas_video_storage.enabled);
        assert!(config.nas_video_storage.root_path.is_empty());
        assert!(config.nas_video_storage.archive_recordings);
        assert!(config.nas_video_storage.archive_imports);
        assert!(config.nas_video_storage.delete_local_after_archive);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn new_config_prefers_funasr_with_local_whisper_fallback_available() {
        let root = unique_test_root("funasr-default");
        let config_path = root.join("Conf.toml");
        let cache = root.join("cache");
        let output = root.join("output");

        let config = Config::load(&config_path, &cache, &output).unwrap();

        assert_eq!(config.subtitle_generator_type, "funasr");
        assert!(!config.whisper_model.trim().is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_config_without_nas_section_loads_safe_defaults() {
        let root = unique_test_root("nas-legacy");
        let config_path = root.join("Conf.toml");
        let cache = root.join("cache");
        let output = root.join("output");
        let original = Config::load(&config_path, &cache, &output).unwrap();
        let serialized = std::fs::read_to_string(&config_path).unwrap();
        let legacy = serialized
            .lines()
            .take_while(|line| !line.trim().starts_with("[nas_video_storage]"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&config_path, legacy).unwrap();

        let loaded = Config::load(&config_path, &cache, &output).unwrap();

        assert_eq!(loaded.cache, original.cache);
        assert_eq!(loaded.output, original.output);
        assert_eq!(loaded.nas_video_storage, NasVideoStorageConfig::default());

        let _ = std::fs::remove_dir_all(root);
    }
}
