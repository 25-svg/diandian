use crate::config::Config;
use crate::database::video_archive::VideoArchiveRow;
use crate::database::Database;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Notify, RwLock};

pub fn should_archive_video(
    config: &crate::config::NasVideoStorageConfig,
    source_kind: &str,
) -> bool {
    config.enabled
        && match source_kind {
            "recording" => config.archive_recordings,
            "import" => config.archive_imports,
            _ => false,
        }
}

pub fn retry_delay_minutes(retry_count: i64) -> Option<i64> {
    [1, 5, 15, 30, 60].get(retry_count.max(0) as usize).copied()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveOutcome {
    pub nas_path: PathBuf,
    pub bytes: u64,
}

fn sanitize_path_component(value: &str, fallback: &str) -> String {
    let replaced = value
        .trim()
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => character,
        })
        .collect::<String>();
    let sanitized = sanitize_filename::sanitize(replaced).trim().to_string();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

/// Import metadata JSON (`note`) must never become a NAS folder name.
/// Older builds accidentally used that payload as the archive anchor.
fn looks_like_analysis_metadata_anchor(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    trimmed.starts_with('{')
        || lowered.contains("analysispurpose")
        || lowered.contains("masterscriptkey")
        || lowered.contains("competitorname")
}

/// Prefer a human-readable title/room id. Skip JSON-looking metadata leftovers.
pub fn choose_nas_anchor<'a>(title: &'a str, room_id: &'a str) -> &'a str {
    let title = title.trim();
    if !title.is_empty() && !looks_like_analysis_metadata_anchor(title) {
        return title;
    }
    let room_id = room_id.trim();
    if !room_id.is_empty() && !looks_like_analysis_metadata_anchor(room_id) {
        return room_id;
    }
    "其他主播"
}

pub fn plan_nas_destination(
    root: &Path,
    anchor: &str,
    created_at: &str,
    source: &Path,
) -> Result<PathBuf, String> {
    let filename = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "本机视频文件名无效".to_string())?;
    let date = created_at.get(0..10).filter(|value| {
        value.len() == 10
            && value.as_bytes()[4] == b'-'
            && value.as_bytes()[7] == b'-'
            && value
                .chars()
                .enumerate()
                .all(|(index, character)| index == 4 || index == 7 || character.is_ascii_digit())
    });
    Ok(root
        .join(sanitize_path_component(anchor, "其他主播"))
        .join(date.unwrap_or("日期未知"))
        .join(sanitize_path_component(filename, "video.mp4")))
}

fn uploading_path(destination: &Path) -> PathBuf {
    match destination.extension().and_then(|value| value.to_str()) {
        Some(extension) => destination.with_extension(format!("{extension}.uploading")),
        None => destination.with_extension("uploading"),
    }
}

pub fn resolve_collision_path(requested: &Path) -> Result<PathBuf, String> {
    if !requested.exists() {
        return Ok(requested.to_path_buf());
    }
    let parent = requested
        .parent()
        .ok_or_else(|| "NAS 目标目录无效".to_string())?;
    let stem = requested
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "NAS 目标文件名无效".to_string())?;
    let extension = requested.extension().and_then(|value| value.to_str());
    for suffix in 2..=10_000 {
        let filename = match extension {
            Some(extension) => format!("{stem}-{suffix}.{extension}"),
            None => format!("{stem}-{suffix}"),
        };
        let candidate = parent.join(filename);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("NAS 目录中同名视频过多，无法生成安全文件名".to_string())
}

pub async fn copy_file_transactional<F>(
    source: &Path,
    destination: &Path,
    mut on_progress: F,
) -> Result<ArchiveOutcome, String>
where
    F: FnMut(u64),
{
    if destination.exists() {
        return Err("NAS 中已存在同名视频，拒绝覆盖".to_string());
    }
    let source_size = tokio::fs::metadata(source)
        .await
        .map_err(|error| format!("无法读取本机视频：{error}"))?
        .len();
    let temporary = uploading_path(destination);
    let _ = tokio::fs::remove_file(&temporary).await;

    let copy_result = async {
        let mut input = tokio::fs::File::open(source)
            .await
            .map_err(|error| format!("无法打开本机视频：{error}"))?;
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await
            .map_err(|error| format!("无法在 NAS 创建临时视频：{error}"))?;
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut copied = 0_u64;
        loop {
            let count = input
                .read(&mut buffer)
                .await
                .map_err(|error| format!("读取本机视频失败：{error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .await
                .map_err(|error| format!("写入 NAS 视频失败：{error}"))?;
            copied += count as u64;
            on_progress(copied);
        }
        output
            .sync_all()
            .await
            .map_err(|error| format!("NAS 视频落盘同步失败：{error}"))?;
        let copied_size = tokio::fs::metadata(&temporary)
            .await
            .map_err(|error| format!("无法校验 NAS 临时视频：{error}"))?
            .len();
        if copied_size != source_size {
            return Err(format!(
                "NAS 视频大小校验失败：本机 {source_size} 字节，NAS {copied_size} 字节"
            ));
        }
        tokio::fs::rename(&temporary, destination)
            .await
            .map_err(|error| format!("NAS 临时视频无法转为正式文件：{error}"))?;
        Ok(ArchiveOutcome {
            nas_path: destination.to_path_buf(),
            bytes: copied_size,
        })
    }
    .await;

    if copy_result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    copy_result
}

pub async fn verify_archived_media(path: &Path) -> Result<(), String> {
    let metadata = crate::ffmpeg::extract_video_metadata(path)
        .await
        .map_err(|error| format!("NAS 视频无法读取媒体信息：{error}"))?;
    if metadata.duration <= 0.0 {
        return Err("NAS 视频时长无效".to_string());
    }
    Ok(())
}

#[derive(Clone)]
pub struct NasArchiveService {
    db: Arc<Database>,
    config: Arc<RwLock<Config>>,
    notify: Arc<Notify>,
}

impl NasArchiveService {
    pub fn new(db: Arc<Database>, config: Arc<RwLock<Config>>) -> Self {
        Self {
            db,
            config,
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            if let Err(error) = self.db.reset_interrupted_video_archives().await {
                log::error!("恢复 NAS 视频转存任务失败：{error}");
            }
            self.run().await;
        });
    }

    pub async fn enqueue(
        &self,
        video_id: i64,
        source_kind: &str,
        local_path: &Path,
    ) -> Result<Option<VideoArchiveRow>, String> {
        let settings = self.config.read().await.nas_video_storage.clone();
        if !should_archive_video(&settings, source_kind) {
            return Ok(None);
        }
        let local_path = local_path
            .to_str()
            .ok_or_else(|| "本机视频路径包含无法识别的字符".to_string())?;
        let job = self
            .db
            .enqueue_video_archive(video_id, source_kind, local_path)
            .await
            .map_err(String::from)?;
        self.notify.notify_one();
        Ok(Some(job))
    }

    pub fn wake(&self) {
        self.notify.notify_one();
    }

    async fn run(&self) {
        loop {
            match self.process_next().await {
                Ok(true) => continue,
                Ok(false) => {
                    tokio::select! {
                        _ = self.notify.notified() => {}
                        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                    }
                }
                Err(error) => {
                    log::error!("NAS 视频转存队列异常：{error}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn process_next(&self) -> Result<bool, String> {
        if !self.config.read().await.nas_video_storage.enabled {
            return Ok(false);
        }
        let Some(job) = self
            .db
            .claim_next_video_archive()
            .await
            .map_err(String::from)?
        else {
            return Ok(false);
        };
        if let Err(error) = self.process_job(&job).await {
            let delay = retry_delay_minutes(job.retry_count);
            let next_retry_at = delay.map(|minutes| {
                (chrono::Utc::now() + chrono::Duration::minutes(minutes))
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            });
            self.db
                .fail_video_archive(job.id, &error, next_retry_at.as_deref(), delay.is_none())
                .await
                .map_err(String::from)?;
            log::warn!(
                "NAS 视频转存失败，任务 {} 将{}：{}",
                job.id,
                if delay.is_some() {
                    "自动重试"
                } else {
                    "等待人工处理"
                },
                error
            );
        }
        Ok(true)
    }

    async fn process_job(&self, job: &VideoArchiveRow) -> Result<(), String> {
        let settings = self.config.read().await.nas_video_storage.clone();
        if !should_archive_video(&settings, &job.source_kind) {
            return Err("NAS 视频存储当前未启用".to_string());
        }
        let root = PathBuf::from(&settings.root_path);
        if !root.is_dir() {
            return Err("NAS 共享目录不存在或当前无法访问".to_string());
        }
        let source = PathBuf::from(&job.local_path);
        if !source.is_file() {
            return Err("本机待转存视频不存在".to_string());
        }
        let video = self
            .db
            .get_video(job.video_id)
            .await
            .map_err(String::from)?;
        // Prefer human title/room id. `note` is JSON metadata and must not become a folder name.
        let anchor = choose_nas_anchor(&video.title, &video.room_id);
        let requested = plan_nas_destination(&root, anchor, &video.created_at, &source)?;
        let parent = requested
            .parent()
            .ok_or_else(|| "NAS 目标目录无效".to_string())?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("无法创建 NAS 归档目录：{error}"))?;
        let destination = resolve_collision_path(&requested)?;
        let progress_db = self.db.clone();
        let archive_id = job.id;
        let mut last_reported = 0_u64;
        let outcome = copy_file_transactional(&source, &destination, move |copied| {
            if copied.saturating_sub(last_reported) >= 64 * 1024 * 1024 {
                last_reported = copied;
                let db = progress_db.clone();
                tokio::spawn(async move {
                    let _ = db
                        .update_video_archive_progress(archive_id, copied as i64)
                        .await;
                });
            }
        })
        .await?;
        self.db
            .update_video_archive_progress(job.id, outcome.bytes as i64)
            .await
            .map_err(String::from)?;

        if let Err(error) = verify_archived_media(&outcome.nas_path).await {
            let _ = tokio::fs::remove_file(&outcome.nas_path).await;
            return Err(error);
        }
        if let Err(error) = self
            .db
            .complete_video_archive(job.id, &outcome.nas_path.to_string_lossy())
            .await
        {
            let _ = tokio::fs::remove_file(&outcome.nas_path).await;
            return Err(String::from(error));
        }
        if settings.delete_local_after_archive {
            if let Err(error) = tokio::fs::remove_file(&source).await {
                log::warn!(
                    "NAS 视频已归档，但本机副本无法删除 {}：{}",
                    source.display(),
                    error
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bili-shadowreplay-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn destination_groups_by_sanitized_anchor_and_date() {
        let root = std::path::Path::new(r"\\nas\直播录像");
        let path = plan_nas_destination(
            root,
            r#"于千惠:新品/专场"#,
            "2026-07-26 13:00:00",
            std::path::Path::new(r"D:\videos\a.mp4"),
        )
        .unwrap();

        assert_eq!(
            path,
            root.join("于千惠_新品_专场")
                .join("2026-07-26")
                .join("a.mp4")
        );
    }

    #[test]
    fn analysis_metadata_json_is_rejected_as_nas_anchor() {
        let note_json = r#"{"analysisPurpose":"enterprise_review","competitorName":"","masterScriptKey":"MS-BATCH-4"}"#;
        assert_eq!(choose_nas_anchor(note_json, "bsr:import"), "bsr:import");
        assert_eq!(choose_nas_anchor("", note_json), "其他主播");
        assert_eq!(
            choose_nas_anchor("直播大屏·专业版", note_json),
            "直播大屏·专业版"
        );

        let dirty_sanitized =
            "{analysisPurpose___enterprise_review_,_competitorName____,_masterScriptKey___MS-BATCH-4_}";
        assert!(looks_like_analysis_metadata_anchor(dirty_sanitized));
        assert_eq!(choose_nas_anchor(dirty_sanitized, "bsr:import"), "bsr:import");

        let root = std::path::Path::new(r"Z:\");
        let path = plan_nas_destination(
            root,
            choose_nas_anchor(dirty_sanitized, "bsr:import"),
            "2026-07-30T08:38:07Z",
            std::path::Path::new(r"D:\videos\imported-live.ts"),
        )
        .unwrap();
        let rendered = path.to_string_lossy();
        assert!(
            !rendered.to_ascii_lowercase().contains("analysispurpose"),
            "dirty metadata must not appear in NAS path: {rendered}"
        );
        assert_eq!(
            path,
            root.join("bsr_import").join("2026-07-30").join("imported-live.ts")
        );
    }

    #[tokio::test]
    async fn successful_copy_uses_temporary_name_then_final_name() {
        let root = unique_test_root("nas-copy");
        let source = root.join("source.mp4");
        let destination = root.join("nas").join("final.mp4");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&source, b"verified-video-bytes").unwrap();

        let outcome = copy_file_transactional(&source, &destination, |_| {})
            .await
            .unwrap();

        assert_eq!(outcome.bytes, 20);
        assert_eq!(outcome.nas_path, destination);
        assert_eq!(
            std::fs::read(&outcome.nas_path).unwrap(),
            b"verified-video-bytes"
        );
        assert!(!outcome.nas_path.with_extension("mp4.uploading").exists());
        assert!(source.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn failed_copy_keeps_local_source() {
        let root = unique_test_root("nas-copy-failure");
        let source = root.join("source.mp4");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&source, b"keep-me").unwrap();
        let invalid_destination = root.join("missing-parent").join("final.mp4");

        assert!(
            copy_file_transactional(&source, &invalid_destination, |_| {})
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"keep-me");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn collision_path_never_overwrites_existing_video() {
        let root = unique_test_root("nas-collision");
        std::fs::create_dir_all(&root).unwrap();
        let requested = root.join("直播录像.mp4");
        std::fs::write(&requested, b"first").unwrap();
        std::fs::write(root.join("直播录像-2.mp4"), b"second").unwrap();

        let resolved = resolve_collision_path(&requested).unwrap();

        assert_eq!(resolved, root.join("直播录像-3.mp4"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn archive_policy_respects_source_kind_switches() {
        let mut config = crate::config::NasVideoStorageConfig {
            enabled: true,
            root_path: r"\\nas\直播录像".into(),
            archive_recordings: true,
            archive_imports: false,
            delete_local_after_archive: true,
        };
        assert!(should_archive_video(&config, "recording"));
        assert!(!should_archive_video(&config, "import"));
        config.enabled = false;
        assert!(!should_archive_video(&config, "recording"));
    }

    #[test]
    fn retry_schedule_is_bounded_and_becomes_terminal() {
        assert_eq!(retry_delay_minutes(0), Some(1));
        assert_eq!(retry_delay_minutes(1), Some(5));
        assert_eq!(retry_delay_minutes(2), Some(15));
        assert_eq!(retry_delay_minutes(3), Some(30));
        assert_eq!(retry_delay_minutes(4), Some(60));
        assert_eq!(retry_delay_minutes(5), None);
    }

    #[tokio::test]
    async fn invalid_media_is_rejected_before_local_source_deletion() {
        let root = unique_test_root("nas-invalid-media");
        std::fs::create_dir_all(&root).unwrap();
        let invalid_video = root.join("invalid.mp4");
        std::fs::write(&invalid_video, b"not-a-video").unwrap();

        assert!(verify_archived_media(&invalid_video).await.is_err());
        assert!(invalid_video.exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
