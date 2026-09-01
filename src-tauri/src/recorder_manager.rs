use crate::config::Config;
use crate::danmu2ass;
use crate::database::record::RecordRow;
use crate::database::recorder::RecorderRow;
use crate::database::task::TaskRow;
use crate::database::video::VideoRow;
use crate::database::{Database, DatabaseError};
use crate::ffmpeg::{encode_video_danmu, transcode, Range};
use crate::progress::progress_reporter::{EventEmitter, ProgressReporter, ProgressReporterTrait};
use crate::subtitle_generator::transcript_artifacts::{TranscriptArtifactStore, TranscriptSource};
use crate::subtitle_generator::{item_to_srt, GenerateResult, SubtitleGeneratorType};
use crate::task::{Task, TaskManager, TaskPriority};
use crate::webhook::events::{self, Payload};
use crate::webhook::poster::WebhookPoster;
use chrono::{DateTime, Utc};
use m3u8_rs::{MediaPlaylist, MediaPlaylistType, Playlist};
use recorder::account::Account;
use recorder::danmu::{DanmuEntry, DanmuStorage};
use recorder::errors::RecorderError;
use recorder::events::RecorderEvent;
use recorder::platforms::bilibili::BiliRecorder;
use recorder::platforms::douyin::DouyinRecorder;
use recorder::platforms::huya::HuyaRecorder;
use recorder::platforms::kuaishou::KuaishouRecorder;
use recorder::platforms::tiktok::TikTokRecorder;
use recorder::platforms::PlatformType;
use recorder::traits::RecorderTrait;
use recorder::RoomInfo;
use recorder::UserInfo;
use recorder::{CachePath, RecorderInfo};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(feature = "gui")]
use tauri_plugin_notification::NotificationExt;
use thiserror::Error;
use tokio::fs::{remove_file, write, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio::sync::{Mutex, RwLock, Semaphore};

#[cfg(not(feature = "headless"))]
use tauri::AppHandle;

async fn clone_lock_value<T: Clone>(lock: &RwLock<T>) -> T {
    lock.read().await.clone()
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct RecorderList {
    pub count: usize,
    pub recorders: Vec<RecorderInfo>,
}

const RECORDING_START_ERROR_CODE: &str = "REC-LIVE-NOT-RECORDING";
const RECORDING_FILE_STALLED_ERROR_CODE: &str = "REC-FILE-NOT-GROWING";
const RECORDER_INIT_ERROR_CODE: &str = "REC-INITIALIZE-FAILED";
const RECORDER_LOGIN_ERROR_CODE: &str = "REC-LOGIN-REQUIRED";
const RECORDING_STALL_CHECKS: u8 = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordingGrowthState {
    live_id: String,
    last_size: u64,
    unchanged_checks: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordingGrowthDecision {
    Starting,
    Growing,
    Stalled,
}

fn evaluate_recording_growth(
    previous: Option<&RecordingGrowthState>,
    live_id: &str,
    current_size: u64,
) -> (RecordingGrowthState, RecordingGrowthDecision) {
    let mut state = match previous {
        Some(previous) if previous.live_id == live_id => previous.clone(),
        _ => RecordingGrowthState {
            live_id: live_id.to_string(),
            last_size: current_size,
            unchanged_checks: 0,
        },
    };

    if previous.map(|item| item.live_id.as_str()) != Some(live_id) {
        return (state, RecordingGrowthDecision::Starting);
    }

    if current_size != state.last_size {
        state.last_size = current_size;
        state.unchanged_checks = 0;
        return (state, RecordingGrowthDecision::Growing);
    }

    state.unchanged_checks = state.unchanged_checks.saturating_add(1);
    if state.unchanged_checks >= RECORDING_STALL_CHECKS {
        return (state, RecordingGrowthDecision::Stalled);
    }
    if state.last_size == 0 {
        (state, RecordingGrowthDecision::Starting)
    } else {
        (state, RecordingGrowthDecision::Growing)
    }
}

fn retry_delay(completed_retries: u8) -> Option<Duration> {
    [60, 300, 1800]
        .get(completed_retries as usize)
        .copied()
        .map(Duration::from_secs)
}

fn retry_timestamp(delay: Duration) -> String {
    (Utc::now() + chrono::Duration::seconds(delay.as_secs() as i64))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[derive(Debug, Clone)]
struct RetryState {
    completed_retries: u8,
    next_retry_at: Instant,
    terminal: bool,
}

impl RetryState {
    fn first_failure(now: Instant) -> Self {
        Self {
            completed_retries: 0,
            next_retry_at: now + retry_delay(0).expect("first retry delay"),
            terminal: false,
        }
    }

    fn after_failed_retry(&mut self, now: Instant) -> Option<Duration> {
        self.completed_retries = self.completed_retries.saturating_add(1);
        let Some(delay) = retry_delay(self.completed_retries) else {
            self.terminal = true;
            return None;
        };
        self.next_retry_at = now + delay;
        Some(delay)
    }
}

#[cfg(test)]
mod recorder_retry_tests {
    use super::{
        evaluate_recording_growth, retry_delay, RecorderManager, RecordingGrowthDecision,
        RecordingGrowthState, RetryState, RECORDING_STALL_CHECKS,
    };
    use recorder::platforms::PlatformType;
    use std::time::{Duration, Instant};

    #[test]
    fn recording_start_retry_schedule_is_one_five_thirty_minutes() {
        assert_eq!(retry_delay(0), Some(Duration::from_secs(60)));
        assert_eq!(retry_delay(1), Some(Duration::from_secs(300)));
        assert_eq!(retry_delay(2), Some(Duration::from_secs(1800)));
        assert_eq!(retry_delay(3), None);
    }

    #[test]
    fn retry_state_becomes_terminal_after_three_retries() {
        let now = Instant::now();
        let mut state = RetryState::first_failure(now);
        assert_eq!(state.completed_retries, 0);
        assert_eq!(
            state.after_failed_retry(now),
            Some(Duration::from_secs(300))
        );
        assert_eq!(
            state.after_failed_retry(now),
            Some(Duration::from_secs(1800))
        );
        assert_eq!(state.after_failed_retry(now), None);
        assert_eq!(state.completed_retries, 3);
        assert!(state.terminal);
    }

    #[tokio::test]
    async fn discovers_legacy_archive_one_level_below_cache_root() {
        let root =
            std::env::temp_dir().join(format!("bsr-legacy-archive-path-{}", uuid::Uuid::new_v4()));
        let expected = root
            .join("直播大屏·专业版")
            .join("douyin")
            .join("room-1")
            .join("live-1");
        std::fs::create_dir_all(&expected).unwrap();

        let found = RecorderManager::first_archive_dir_below(
            &root,
            PlatformType::Douyin,
            "room-1",
            "live-1",
        )
        .await;

        assert_eq!(found, Some(expected));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recording_is_only_confirmed_after_bytes_change() {
        let (starting, decision) = evaluate_recording_growth(None, "live-1", 0);
        assert_eq!(decision, RecordingGrowthDecision::Starting);

        let (growing, decision) = evaluate_recording_growth(Some(&starting), "live-1", 2048);
        assert_eq!(decision, RecordingGrowthDecision::Growing);
        assert_eq!(growing.last_size, 2048);
    }

    #[test]
    fn unchanged_recording_is_marked_stalled_after_grace_period() {
        let mut state = RecordingGrowthState {
            live_id: "live-1".to_string(),
            last_size: 2048,
            unchanged_checks: 0,
        };
        let mut decision = RecordingGrowthDecision::Growing;
        for _ in 0..RECORDING_STALL_CHECKS {
            (state, decision) = evaluate_recording_growth(Some(&state), "live-1", 2048);
        }
        assert_eq!(decision, RecordingGrowthDecision::Stalled);
    }
}

#[cfg(test)]
mod playback_playlist_tests {
    use super::RecorderManager;
    use m3u8_rs::MediaPlaylistType;

    fn invalid_douyin_playlist() -> m3u8_rs::MediaPlaylist {
        let source = b"#EXTM3U\n#EXT-X-TARGETDURATION:0\n#EXTINF:3.999667,\nsegment-1.ts\n";
        m3u8_rs::parse_media_playlist(source)
            .expect("playlist should parse")
            .1
    }

    #[test]
    fn playback_manifest_repairs_zero_target_duration() {
        let mut playlist = invalid_douyin_playlist();

        RecorderManager::prepare_playlist_for_playback(&mut playlist, false);

        assert_eq!(playlist.target_duration, 4.0);
        assert!(!playlist.end_list);
    }

    #[test]
    fn finished_playback_manifest_is_served_as_vod() {
        let mut playlist = invalid_douyin_playlist();

        RecorderManager::prepare_playlist_for_playback(&mut playlist, true);

        assert_eq!(playlist.target_duration, 4.0);
        assert!(playlist.end_list);
        assert_eq!(playlist.playlist_type, Some(MediaPlaylistType::Vod));

        let mut bytes = Vec::new();
        playlist
            .write_to(&mut bytes)
            .expect("playlist should serialize");
        let rendered = String::from_utf8(bytes).expect("playlist should be UTF-8");
        assert!(rendered.contains("#EXT-X-TARGETDURATION:4"));
        assert!(rendered.contains("#EXT-X-PLAYLIST-TYPE:VOD"));
        assert!(rendered.contains("#EXT-X-ENDLIST"));
    }
}

async fn remove_archive_cache_dir(path: &Path) -> Result<(), RecorderManagerError> {
    const ATTEMPTS: usize = 5;
    let path = path.to_path_buf();
    let mut last_error = None;

    for attempt in 0..ATTEMPTS {
        let target = path.clone();
        match tokio::task::spawn_blocking(move || {
            crate::fs_util::remove_dir_all_with_fallbacks(&target)
        })
        .await
        {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Ok(Err(error)) => last_error = Some(error),
            Err(join_error) => {
                last_error = Some(std::io::Error::other(join_error.to_string()));
            }
        }
        if attempt + 1 < ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt as u64 + 1))).await;
        }
    }

    if !path.exists() {
        return Ok(());
    }

    let error = last_error.expect("archive delete attempt must record an error");
    let hint = crate::fs_util::archive_cache_delete_hint(&path);
    Err(RecorderManagerError::HLSError {
        err: format!("无法删除录播缓存 {}：{}。{}", path.display(), error, hint),
    })
}

/// Make a large archive disappear from the active cache immediately. Renaming
/// inside the same parent directory is normally atomic, while recursively
/// removing thousands of TS segments can take a long time. If staging is not
/// supported by the filesystem, keep the previous synchronous fallback.
async fn stage_archive_cache_dir(path: &Path) -> Result<Option<PathBuf>, RecorderManagerError> {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Ok(None);
    }

    let Some(parent) = path.parent() else {
        remove_archive_cache_dir(path).await?;
        return Ok(None);
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("archive");
    let staged = parent.join(format!(
        ".{name}.deleting-{}",
        uuid::Uuid::new_v4().simple()
    ));

    match tokio::fs::rename(path, &staged).await {
        Ok(()) => Ok(Some(staged)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            log::warn!(
                "Unable to stage archive cache for fast deletion ({}): {error}; using direct deletion",
                path.display()
            );
            remove_archive_cache_dir(path).await?;
            Ok(None)
        }
    }
}

async fn measure_recording_cache(work_dir: &Path) -> Result<(f64, u64), std::io::Error> {
    let mut total_length = 0.0;
    let mut total_size = 0u64;

    let playlist_path = work_dir.join("playlist.m3u8");
    if playlist_path.exists() {
        let bytes = tokio::fs::read(&playlist_path).await?;
        if let Ok((_, playlist)) = m3u8_rs::parse_playlist(&bytes) {
            if let Playlist::MediaPlaylist(playlist) = playlist {
                for segment in &playlist.segments {
                    total_length += f64::from(segment.duration);
                }
            }
        }
    }

    if work_dir.exists() {
        let mut entries = tokio::fs::read_dir(work_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
                total_size += entry.metadata().await?.len();
            }
        }
    }

    Ok((total_length, total_size))
}

fn is_indexable_active_recording(recording: bool, live_id: &str) -> bool {
    recording && !live_id.trim().is_empty()
}

#[cfg(test)]
mod recording_cache_measure_tests {
    use super::{is_indexable_active_recording, measure_recording_cache};

    #[test]
    fn empty_live_id_is_not_an_active_recording_for_archive_repair() {
        assert!(!is_indexable_active_recording(true, ""));
        assert!(!is_indexable_active_recording(true, "   "));
        assert!(is_indexable_active_recording(true, "1785290000000"));
    }

    #[tokio::test]
    async fn empty_recording_cache_has_zero_stats() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-empty-recording-cache-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();

        let (length, size) = measure_recording_cache(&path).await.unwrap();

        assert_eq!(length, 0.0);
        assert_eq!(size, 0);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[cfg(test)]
mod archive_delete_tests {
    use super::{remove_archive_cache_dir, stage_archive_cache_dir};

    #[tokio::test]
    async fn missing_archive_cache_is_already_deleted() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-missing-archive-{}",
            uuid::Uuid::new_v4()
        ));

        assert!(remove_archive_cache_dir(&path).await.is_ok());
    }

    #[tokio::test]
    async fn archive_cache_file_target_is_also_removed() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-archive-file-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, b"not a directory").unwrap();

        remove_archive_cache_dir(&path).await.unwrap();

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn archive_cache_is_staged_before_slow_recursive_deletion() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-stage-archive-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();
        for index in 0..32 {
            std::fs::write(path.join(format!("segment-{index}.ts")), b"data").unwrap();
        }

        let staged = stage_archive_cache_dir(&path)
            .await
            .unwrap()
            .expect("existing cache should be staged");

        assert!(!path.exists());
        assert!(staged.exists());
        remove_archive_cache_dir(&staged).await.unwrap();
        assert!(!staged.exists());
    }
}

#[cfg(test)]
mod config_snapshot_tests {
    use super::clone_lock_value;
    use std::time::Duration;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn cloned_snapshot_does_not_block_later_settings_write() {
        let config = RwLock::new(String::from("transcription settings"));

        let snapshot = clone_lock_value(&config).await;
        let writable = tokio::time::timeout(Duration::from_millis(50), config.write()).await;

        assert_eq!(snapshot, "transcription settings");
        assert!(writable.is_ok());
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSubtitleRefreshResult {
    pub subtitle: String,
    pub decision: String,
    pub similarity: f64,
    pub old_length: usize,
    pub new_length: usize,
}

fn normalize_transcript_for_comparison(value: &str) -> Vec<char> {
    value
        .lines()
        .filter(|line| !line.contains("-->") && !line.trim().chars().all(|ch| ch.is_ascii_digit()))
        .flat_map(|line| line.chars())
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn transcript_bigram_similarity(left: &str, right: &str) -> f64 {
    let make_bigrams = |value: &str| -> HashSet<String> {
        let chars = normalize_transcript_for_comparison(value);
        chars
            .windows(2)
            .map(|pair| pair.iter().collect::<String>())
            .collect()
    };
    let left_bigrams = make_bigrams(left);
    let right_bigrams = make_bigrams(right);
    if left_bigrams.is_empty() || right_bigrams.is_empty() {
        return 0.0;
    }
    let intersection = left_bigrams.intersection(&right_bigrams).count() as f64;
    let union = left_bigrams.union(&right_bigrams).count() as f64;
    let containment = intersection / left_bigrams.len().min(right_bigrams.len()) as f64;
    let jaccard = intersection / union.max(1.0);
    jaccard.max(containment * 0.82)
}

type ArchiveSubtitleLocks = Mutex<HashMap<String, Arc<Mutex<()>>>>;

async fn archive_subtitle_lock(locks: &ArchiveSubtitleLocks, archive_key: &str) -> Arc<Mutex<()>> {
    let mut locks = locks.lock().await;
    locks
        .entry(archive_key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn subtitle_end_ms(value: &str) -> Option<u64> {
    srtparse::from_str(value)
        .ok()?
        .into_iter()
        .map(|item| item.end_time.into_duration().as_millis() as u64)
        .max()
}

fn is_subtitle_coverage_regression(previous: &str, generated: &str) -> bool {
    let Some(previous_end_ms) = subtitle_end_ms(previous) else {
        return false;
    };
    let Some(generated_end_ms) = subtitle_end_ms(generated) else {
        return true;
    };

    generated_end_ms.saturating_add(60_000) < previous_end_ms
        && generated_end_ms.saturating_mul(10) < previous_end_ms.saturating_mul(9)
}

#[cfg(test)]
mod archive_subtitle_guard_tests {
    use super::{archive_subtitle_lock, is_subtitle_coverage_regression};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn cue(index: usize, start: &str, end: &str, text: &str) -> String {
        format!("{index}\n{start} --> {end}\n{text}\n\n")
    }

    #[test]
    fn rejects_a_refresh_that_truncates_a_two_hour_transcript_to_one_hour() {
        let previous = cue(1, "00:00:00,000", "02:18:20,000", "完整文稿");
        let generated = cue(1, "00:00:00,000", "00:59:59,000", "截断文稿");

        assert!(is_subtitle_coverage_regression(&previous, &generated));
        assert!(!is_subtitle_coverage_regression(&generated, &previous));
    }

    #[tokio::test]
    async fn the_same_archive_reuses_one_generation_lock() {
        let locks = Mutex::new(HashMap::new());

        let first = archive_subtitle_lock(&locks, "douyin:room:live").await;
        let second = archive_subtitle_lock(&locks, "douyin:room:live").await;
        let different = archive_subtitle_lock(&locks, "douyin:room:other").await;

        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &different));
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClipRangeParams {
    pub title: String,
    pub note: String,
    pub cover: String,
    pub platform: String,
    pub room_id: String,
    pub live_id: String,
    pub ranges: Vec<Range>,
    /// Encode danmu after clip
    pub danmu: bool,
    pub local_offset: i64,
    /// Fix encoding after clip
    pub fix_encoding: bool,
    /// Transition effect between clips (for multiple ranges)
    pub transition: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenerateWholeClipParams {
    pub encode_danmu: bool,
    pub platform: String,
    pub room_id: String,
    pub parent_id: String,
    pub selected_live_ids: Option<Vec<String>>,
    pub output_name: Option<String>,
}

pub struct RelatedPlaylist {
    pub live_id: String,
    pub title: String,
    pub path: PathBuf,
}

pub enum RecorderType {
    BiliBili(BiliRecorder),
    Douyin(DouyinRecorder),
    Huya(HuyaRecorder),
    Kuaishou(KuaishouRecorder),
    TikTok(TikTokRecorder),
}

impl RecorderType {
    async fn run(&self) {
        match self {
            RecorderType::BiliBili(recorder) => recorder.run().await,
            RecorderType::Douyin(recorder) => recorder.run().await,
            RecorderType::Huya(recorder) => recorder.run().await,
            RecorderType::Kuaishou(recorder) => recorder.run().await,
            RecorderType::TikTok(recorder) => recorder.run().await,
        }
    }

    async fn stop(&self) {
        match self {
            RecorderType::BiliBili(recorder) => recorder.stop().await,
            RecorderType::Douyin(recorder) => recorder.stop().await,
            RecorderType::Huya(recorder) => recorder.stop().await,
            RecorderType::Kuaishou(recorder) => recorder.stop().await,
            RecorderType::TikTok(recorder) => recorder.stop().await,
        }
    }

    async fn info(&self) -> RecorderInfo {
        match self {
            RecorderType::BiliBili(recorder) => recorder.info().await,
            RecorderType::Douyin(recorder) => recorder.info().await,
            RecorderType::Huya(recorder) => recorder.info().await,
            RecorderType::Kuaishou(recorder) => recorder.info().await,
            RecorderType::TikTok(recorder) => recorder.info().await,
        }
    }

    async fn enable(&self) {
        match self {
            RecorderType::BiliBili(recorder) => recorder.enable().await,
            RecorderType::Douyin(recorder) => recorder.enable().await,
            RecorderType::Huya(recorder) => recorder.enable().await,
            RecorderType::Kuaishou(recorder) => recorder.enable().await,
            RecorderType::TikTok(recorder) => recorder.enable().await,
        }
    }

    async fn disable(&self) {
        match self {
            RecorderType::BiliBili(recorder) => recorder.disable().await,
            RecorderType::Douyin(recorder) => recorder.disable().await,
            RecorderType::Huya(recorder) => recorder.disable().await,
            RecorderType::Kuaishou(recorder) => recorder.disable().await,
            RecorderType::TikTok(recorder) => recorder.disable().await,
        }
    }
}

#[derive(Clone)]
pub struct RecorderManager {
    #[cfg(not(feature = "headless"))]
    app_handle: AppHandle,
    emitter: EventEmitter,
    db: Arc<Database>,
    config: Arc<RwLock<Config>>,
    task_manager: Arc<TaskManager>,
    recorders: Arc<RwLock<HashMap<String, RecorderType>>>,
    to_remove: Arc<RwLock<HashSet<String>>>,
    initialization_retries: Arc<RwLock<HashMap<String, RetryState>>>,
    live_start_retries: Arc<RwLock<HashMap<String, RetryState>>>,
    recording_stall_retries: Arc<RwLock<HashMap<String, RetryState>>>,
    recording_growth: Arc<RwLock<HashMap<String, RecordingGrowthState>>>,
    event_tx: broadcast::Sender<RecorderEvent>,
    is_migrating: Arc<AtomicBool>,
    subtitle_generation_locks: Arc<ArchiveSubtitleLocks>,
    media_execution_gate: Arc<Semaphore>,
    live_transcription: crate::live_transcription::LiveTranscriptionCoordinator,
    webhook_poster: WebhookPoster,
    nas_archive: Arc<crate::nas_archive::NasArchiveService>,
}

#[derive(Error, Debug)]
pub enum RecorderManagerError {
    #[error("Recorder already exists: {room_id}")]
    AlreadyExisted { room_id: String },
    #[error("Recorder not found: {room_id}")]
    NotFound { room_id: String },
    #[error("Invalid platform type: {platform}")]
    InvalidPlatformType { platform: String },
    #[error("Recorder error: {0}")]
    RecorderError(#[from] RecorderError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("HLS error: {err}")]
    HLSError { err: String },
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
    #[error("Recording: {live_id}")]
    Recording { live_id: String },
    #[error("Clip error: {err}")]
    ClipError { err: String },
    #[error("M3u8 parse failed: {content}")]
    M3u8ParseFailed { content: String },
    #[error("Empty playlist")]
    EmptyPlaylist,
    #[error("Subtitle not found: {live_id}")]
    SubtitleNotFound { live_id: String },
    #[error("Subtitle generation failed: {error}")]
    SubtitleGenerationFailed { error: String },
    #[error("Invalid live id, not timestamp str")]
    InvalidLiveID,
    #[error("Archive danmu ass generation failed: {error}")]
    ArchiveDanmuAssGenerationFailed { error: String },
}

impl From<RecorderManagerError> for String {
    fn from(err: RecorderManagerError) -> Self {
        err.to_string()
    }
}

impl RecorderManager {
    fn archive_dir_under_root(
        root: &Path,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> PathBuf {
        root.join(platform.as_str()).join(room_id).join(live_id)
    }

    async fn first_archive_dir_below(
        container: &Path,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Option<PathBuf> {
        let mut entries = tokio::fs::read_dir(container).await.ok()?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }
            let candidate = Self::archive_dir_under_root(&entry.path(), platform, room_id, live_id);
            if tokio::fs::metadata(&candidate)
                .await
                .is_ok_and(|metadata| metadata.is_dir())
            {
                return Some(candidate);
            }
        }
        None
    }

    async fn persist_archive_source_path(&self, room_id: &str, live_id: &str, archive_dir: &Path) {
        if let Err(error) = self
            .db
            .update_record_source_path(room_id, live_id, &archive_dir.to_string_lossy())
            .await
        {
            log::warn!("Failed to remember archive source path for {room_id}:{live_id}: {error}");
        }
    }

    /// Resolve an archive independently from the currently selected cache root.
    ///
    /// New recordings remember their concrete directory. Legacy recordings are
    /// recovered from the active/preferred cache, the application default cache,
    /// or one level below/sibling to those roots. The successful legacy match is
    /// persisted so later playback, clipping and transcription do not rescan.
    pub async fn resolve_archive_dir(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> PathBuf {
        let config = self.config.read().await;
        let active_root = PathBuf::from(&config.cache);
        let preferred_root = PathBuf::from(config.preferred_cache_path());
        drop(config);

        if let Ok(record) = self.db.get_record(room_id, live_id).await {
            if !record.source_path.trim().is_empty() {
                let remembered = PathBuf::from(record.source_path.trim());
                if tokio::fs::metadata(&remembered)
                    .await
                    .is_ok_and(|metadata| metadata.is_dir())
                {
                    return remembered;
                }
            }
        }

        let mut roots = vec![active_root.clone(), preferred_root];
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let appdata = PathBuf::from(appdata);
            roots.push(
                appdata
                    .join("cn.vjoi.bili-shadowreplay")
                    .join("recording-cache"),
            );
            roots.push(
                appdata
                    .join("cn.vjoi.bilishadowreplay")
                    .join("recording-cache"),
            );
        }
        roots.dedup();

        for root in &roots {
            let candidate = Self::archive_dir_under_root(root, platform, room_id, live_id);
            if tokio::fs::metadata(&candidate)
                .await
                .is_ok_and(|metadata| metadata.is_dir())
            {
                self.persist_archive_source_path(room_id, live_id, &candidate)
                    .await;
                return candidate;
            }
        }

        let mut containers = Vec::new();
        for root in &roots {
            containers.push(root.clone());
            if let Some(parent) = root.parent() {
                containers.push(parent.to_path_buf());
            }
        }
        containers.dedup();
        for container in containers {
            if let Some(candidate) =
                Self::first_archive_dir_below(&container, platform, room_id, live_id).await
            {
                log::info!(
                    "Recovered legacy archive source path for {room_id}:{live_id}: {}",
                    candidate.display()
                );
                self.persist_archive_source_path(room_id, live_id, &candidate)
                    .await;
                return candidate;
            }
        }

        Self::archive_dir_under_root(&active_root, platform, room_id, live_id)
    }

    async fn remember_current_archive_dir(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) {
        let cache = self.config.read().await.cache.clone();
        let archive_dir =
            Self::archive_dir_under_root(Path::new(&cache), platform, room_id, live_id);
        self.persist_archive_source_path(room_id, live_id, &archive_dir)
            .await;
    }

    async fn resolve_archive_cache_path(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> CachePath {
        let archive_dir = self.resolve_archive_dir(platform, room_id, live_id).await;
        let root = archive_dir
            .ancestors()
            .nth(3)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| archive_dir.clone());
        CachePath::new(root, platform, room_id, live_id)
    }

    pub fn new(
        #[cfg(not(feature = "headless"))] app_handle: AppHandle,
        emitter: EventEmitter,
        db: Arc<Database>,
        config: Arc<RwLock<Config>>,
        task_manager: Arc<TaskManager>,
        media_execution_gate: Arc<Semaphore>,
        webhook_poster: WebhookPoster,
        nas_archive: Arc<crate::nas_archive::NasArchiveService>,
    ) -> RecorderManager {
        let (event_tx, _) = broadcast::channel(100);
        let manager = RecorderManager {
            #[cfg(not(feature = "headless"))]
            app_handle,
            emitter,
            db,
            config: config.clone(),
            task_manager,
            recorders: Arc::new(RwLock::new(HashMap::new())),
            to_remove: Arc::new(RwLock::new(HashSet::new())),
            initialization_retries: Arc::new(RwLock::new(HashMap::new())),
            live_start_retries: Arc::new(RwLock::new(HashMap::new())),
            recording_stall_retries: Arc::new(RwLock::new(HashMap::new())),
            recording_growth: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            is_migrating: Arc::new(AtomicBool::new(false)),
            subtitle_generation_locks: Arc::new(Mutex::new(HashMap::new())),
            media_execution_gate,
            live_transcription: crate::live_transcription::LiveTranscriptionCoordinator::new(
                config.clone(),
            ),
            webhook_poster,
            nas_archive,
        };

        // Start event listener
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.handle_events().await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.monitor_recorders().await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.watchdog_recording_starts().await;
        });

        manager
    }

    pub fn get_event_sender(&self) -> broadcast::Sender<RecorderEvent> {
        self.event_tx.clone()
    }

    async fn handle_events(&self) {
        let mut rx = self.event_tx.subscribe();
        while let Ok(event) = rx.recv().await {
            match event {
                RecorderEvent::LiveStart { recorder } => {
                    let recorder_id = format!(
                        "{}:{}",
                        recorder.room_info.platform, recorder.room_info.room_id
                    );
                    self.live_start_retries.write().await.remove(&recorder_id);
                    self.set_recorder_health(
                        &recorder.room_info.platform,
                        &recorder.room_info.room_id,
                        "live_waiting",
                        "",
                        "已检测到开播，正在启动录制",
                        0,
                        None,
                    )
                    .await;
                    log::info!(
                        "[recorder_lifecycle platform={} room_id={} live_id={}] live_started",
                        recorder.room_info.platform,
                        recorder.room_info.room_id,
                        recorder.live_id
                    );
                    let event = events::new_webhook_event(
                        events::LIVE_STARTED,
                        Payload::Room(recorder.clone()),
                    );
                    let _ = self.webhook_poster.post_event(&event).await;
                    if self.config.read().await.live_start_notify {
                        #[cfg(feature = "gui")]
                        self.app_handle
                            .notification()
                            .builder()
                            .title("BiliShadowReplay - 直播开始")
                            .body(format!(
                                "{} 开启了直播：{}",
                                recorder.user_info.user_name, recorder.room_info.room_title
                            ))
                            .show()
                            .unwrap();
                    }
                }
                RecorderEvent::LiveEnd {
                    platform,
                    room_id,
                    recorder,
                } => {
                    self.live_transcription
                        .finish(platform.as_str(), &room_id, &recorder.live_id)
                        .await;
                    self.live_start_retries.write().await.remove(&format!(
                        "{}:{}",
                        platform.as_str(),
                        room_id
                    ));
                    self.recording_growth.write().await.remove(&format!(
                        "{}:{}",
                        platform.as_str(),
                        room_id
                    ));
                    self.recording_stall_retries.write().await.remove(&format!(
                        "{}:{}",
                        platform.as_str(),
                        room_id
                    ));
                    self.set_recorder_health(
                        platform.as_str(),
                        &room_id,
                        "ready",
                        "",
                        "直播已结束，后台监控正常",
                        0,
                        None,
                    )
                    .await;
                    log::info!(
                        "[recorder_lifecycle platform={} room_id={} live_id={}] live_ended",
                        platform.as_str(),
                        room_id,
                        recorder.live_id
                    );
                    let event = events::new_webhook_event(
                        events::LIVE_ENDED,
                        Payload::Room(recorder.clone()),
                    );
                    let _ = self.webhook_poster.post_event(&event).await;
                    if !recorder.live_id.is_empty() {
                        self.cleanup_empty_record(&recorder).await;
                    }
                    self.handle_live_end(platform, &room_id, &recorder).await;
                    if self.config.read().await.live_end_notify {
                        #[cfg(feature = "gui")]
                        self.app_handle
                            .notification()
                            .builder()
                            .title("BiliShadowReplay - 直播结束")
                            .body(format!(
                                "{} 结束了直播：{}",
                                recorder.user_info.user_name, recorder.room_info.room_title
                            ))
                            .show()
                            .unwrap();
                    }
                }
                RecorderEvent::RecordStart { recorder } => {
                    // add record entry into db
                    let platform = PlatformType::from_str(&recorder.room_info.platform).unwrap();
                    let room_id = recorder.room_info.room_id.clone();
                    self.live_start_retries.write().await.remove(&format!(
                        "{}:{}",
                        platform.as_str(),
                        room_id
                    ));
                    self.set_recorder_health(
                        platform.as_str(),
                        &room_id,
                        "recording_starting",
                        "",
                        "录制进程已启动，正在确认录像文件写入",
                        0,
                        None,
                    )
                    .await;
                    log::info!(
                        "[recorder_lifecycle platform={} room_id={} live_id={}] recording_started",
                        platform.as_str(),
                        room_id,
                        recorder.live_id
                    );
                    if let Err(e) = self
                        .db
                        .add_record(
                            platform,
                            &recorder.platform_live_id,
                            &recorder.live_id,
                            &room_id,
                            &recorder.room_info.room_title,
                            None,
                        )
                        .await
                    {
                        log::error!("Failed to add record entry into db: {e}");
                    } else {
                        self.remember_current_archive_dir(platform, &room_id, &recorder.live_id)
                            .await;
                    }
                    self.apply_room_streamer_or_start_detection(
                        platform,
                        &room_id,
                        &recorder.live_id,
                    )
                    .await;
                    self.live_transcription
                        .start(platform.as_str(), &room_id, &recorder.live_id)
                        .await;
                    let event =
                        events::new_webhook_event(events::RECORD_STARTED, Payload::Room(recorder));
                    let _ = self.webhook_poster.post_event(&event).await;
                }
                RecorderEvent::RecordUpdate {
                    live_id,
                    duration_secs,
                    cached_size_bytes,
                } => {
                    if let Err(error) = self
                        .db
                        .update_record_delta(&live_id, duration_secs, cached_size_bytes)
                        .await
                    {
                        log::error!("Failed to update record stats for {live_id}: {error:?}");
                    }
                }
                RecorderEvent::RecordEnd { recorder } => {
                    self.live_transcription
                        .finish(
                            &recorder.room_info.platform,
                            &recorder.room_info.room_id,
                            &recorder.live_id,
                        )
                        .await;
                    self.recording_growth.write().await.remove(&format!(
                        "{}:{}",
                        recorder.room_info.platform, recorder.room_info.room_id
                    ));
                    self.recording_stall_retries.write().await.remove(&format!(
                        "{}:{}",
                        recorder.room_info.platform, recorder.room_info.room_id
                    ));
                    self.set_recorder_health(
                        &recorder.room_info.platform,
                        &recorder.room_info.room_id,
                        "ready",
                        "",
                        "本段录制已结束，继续监控直播状态",
                        0,
                        None,
                    )
                    .await;
                    log::info!(
                        "[recorder_lifecycle platform={} room_id={} live_id={}] recording_ended",
                        recorder.room_info.platform,
                        recorder.room_info.room_id,
                        recorder.live_id
                    );
                    let event = events::new_webhook_event(
                        events::RECORD_ENDED,
                        Payload::Room(recorder.clone()),
                    );
                    let _ = self.webhook_poster.post_event(&event).await;
                    self.cleanup_empty_record(&recorder).await;
                }
                RecorderEvent::ProgressUpdate { id, content } => {
                    let _ = self
                        .emitter
                        .emit(&RecorderEvent::ProgressUpdate { id, content });
                }
                RecorderEvent::ProgressFinished {
                    id,
                    success,
                    message,
                } => {
                    let _ = self.emitter.emit(&RecorderEvent::ProgressFinished {
                        id,
                        success,
                        message,
                    });
                }
                RecorderEvent::DanmuReceived { room, ts, content } => {
                    let _ = self
                        .emitter
                        .emit(&RecorderEvent::DanmuReceived { room, ts, content });
                }
            }
        }
    }

    async fn cleanup_empty_record(&self, recorder: &RecorderInfo) {
        let live_id = recorder.live_id.clone();
        let room_id = recorder.room_info.room_id.clone();
        let record = match self.db.get_record(&room_id, &live_id).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("Record not found in db: {recorder:?}, err={e:?}");
                return;
            }
        };
        if record.size == 0 {
            let platform = PlatformType::from_str(&recorder.room_info.platform)
                .unwrap_or(PlatformType::BiliBili);
            let cache_folder = self.resolve_archive_dir(platform, &room_id, &live_id).await;
            let _ = self.db.remove_record(&live_id).await;
            let _ = tokio::fs::remove_dir_all(&cache_folder).await;
            log::info!("Empty record folder removed: {cache_folder:?}");
        }
    }

    /// Non-empty related archives sharing the same parent live session.
    async fn count_nonempty_parent_segments(&self, room_id: &str, parent_id: &str) -> usize {
        match self.db.get_archives_by_parent_id(room_id, parent_id).await {
            Ok(archives) => archives
                .into_iter()
                .filter(|archive| archive.size > 0 || archive.length > 0.0)
                .count(),
            Err(error) => {
                log::error!("Failed to count parent segments for {room_id}/{parent_id}: {error}");
                0
            }
        }
    }

    async fn has_active_whole_clip_task(&self, parent_id: &str) -> bool {
        let Ok(tasks) = self.db.get_tasks().await else {
            return false;
        };
        tasks.into_iter().any(|task| {
            if task.task_type != "generate_whole_clip" {
                return false;
            }
            if task.status != "pending" && task.status != "processing" {
                return false;
            }
            serde_json::from_str::<serde_json::Value>(&task.metadata)
                .ok()
                .and_then(|value| {
                    value
                        .get("parent_id")
                        .and_then(|item| item.as_str())
                        .map(|item| item == parent_id)
                })
                .unwrap_or(false)
        })
    }

    async fn queue_auto_whole_clip(
        &self,
        platform: PlatformType,
        room_id: &str,
        parent_id: String,
        encode_danmu: bool,
        message: &str,
    ) {
        if self.has_active_whole_clip_task(&parent_id).await {
            log::info!(
                "Skip auto whole-clip for parent {parent_id}: task already pending/processing"
            );
            return;
        }

        let Ok(task) = self
            .db
            .generate_task(
                "generate_whole_clip",
                message,
                &serde_json::json!({
                    "platform": platform.as_str(),
                    "room_id": room_id,
                    "parent_id": parent_id,
                    "encode_danmu": encode_danmu,
                })
                .to_string(),
            )
            .await
        else {
            log::error!("Failed to generate auto whole-clip task");
            return;
        };

        let Ok(reporter) = ProgressReporter::new(self.db.clone(), &self.emitter, &task.id).await
        else {
            log::error!("Failed to create auto whole-clip reporter");
            let _ = self
                .db
                .update_task(&task.id, "failed", "Failed to create reporter", None)
                .await;
            return;
        };

        log::info!(
            "Create auto whole-clip task: {} {}",
            task.id,
            task.task_type
        );

        let self_clone = self.clone();
        let task_id = task.id.clone();
        let room_id = room_id.to_string();
        let _ = self
            .task_manager
            .add_task(Task::new(
                task_id.clone(),
                TaskPriority::Normal,
                async move {
                    if let Err(e) = self_clone
                        .generate_whole_clip(
                            Some(&reporter),
                            GenerateWholeClipParams {
                                encode_danmu,
                                platform: platform.as_str().to_string(),
                                room_id,
                                parent_id,
                                selected_live_ids: None,
                                output_name: None,
                            },
                        )
                        .await
                    {
                        log::error!("Failed to generate whole clip: {e}");
                        let _ = reporter
                            .finish(false, &format!("自动拼接整场视频失败: {e}"))
                            .await;
                        let _ = self_clone
                            .db
                            .update_task(
                                &task_id,
                                "failed",
                                &format!("自动拼接整场视频失败: {e}"),
                                None,
                            )
                            .await;
                        return Err(format!("Failed to generate whole clip: {e}"));
                    }

                    let _ = reporter.finish(true, "同场多段已自动拼接完成").await;
                    let _ = self_clone
                        .db
                        .update_task(&task_id, "success", "同场多段已自动拼接完成", None)
                        .await;
                    Ok(())
                },
            ))
            .await;
    }

    async fn handle_live_end(
        &self,
        platform: PlatformType,
        room_id: &str,
        recorder: &RecorderInfo,
    ) {
        let (auto_generate, auto_subtitle, encode_danmu) = {
            let config = self.config.read().await;
            (
                config.auto_generate.enabled,
                config.auto_subtitle,
                config.auto_generate.encode_danmu,
            )
        };

        let live_id = recorder.live_id.clone();
        let live_record = match self.db.get_record(room_id, &live_id).await {
            Ok(record) => record,
            Err(_) => {
                log::error!("Live not found in record: {room_id} {live_id}");
                return;
            }
        };

        let multi_segment_count = self
            .count_nonempty_parent_segments(room_id, &live_record.parent_id)
            .await;
        let auto_multi_stitch = multi_segment_count >= 2;

        if !auto_generate && !auto_subtitle && !auto_multi_stitch {
            return;
        }

        let recorder_id = format!("{}:{}", platform.as_str(), room_id);
        log::info!(
            "Start post-record processing for {recorder_id} (segments={multi_segment_count}, auto_generate={auto_generate}, auto_multi_stitch={auto_multi_stitch})"
        );

        // Company recordings are handled by the durable review pipeline. It
        // still produces a transcript when order matching fails, so queuing the
        // legacy subtitle job here would only transcribe the same archive twice.
        if auto_subtitle && live_record.archive_kind != "company" {
            let subtitle_task = self
                .db
                .generate_task(
                    "generate_archive_subtitle",
                    "等待生成整场逐字稿",
                    &serde_json::json!({
                        "platform": platform.as_str(),
                        "room_id": room_id,
                        "live_id": live_id,
                    })
                    .to_string(),
                )
                .await;

            match subtitle_task {
                Ok(task) => {
                    let self_clone = self.clone();
                    let task_id = task.id.clone();
                    let subtitle_room_id = room_id.to_string();
                    let subtitle_live_id = live_id.clone();
                    let reporter =
                        ProgressReporter::new(self.db.clone(), &self.emitter, &task.id).await;

                    match reporter {
                        Ok(reporter) => {
                            if let Err(error) = self
                                .task_manager
                                .add_task(Task::new(task.id, TaskPriority::Normal, async move {
                                    let _ = self_clone
                                        .db
                                        .update_task(
                                            &task_id,
                                            "processing",
                                            "正在生成整场逐字稿",
                                            None,
                                        )
                                        .await;
                                    match self_clone
                                        .generate_archive_subtitle(
                                            platform,
                                            &subtitle_room_id,
                                            &subtitle_live_id,
                                            Some(&reporter),
                                        )
                                        .await
                                    {
                                        Ok(_) => {
                                            reporter.finish(true, "整场逐字稿生成完成").await;
                                            let _ = self_clone
                                                .db
                                                .update_task(
                                                    &task_id,
                                                    "success",
                                                    "整场逐字稿生成完成",
                                                    None,
                                                )
                                                .await;
                                            Ok(())
                                        }
                                        Err(error) => {
                                            reporter
                                                .finish(
                                                    false,
                                                    &format!("整场逐字稿生成失败: {error}"),
                                                )
                                                .await;
                                            let _ = self_clone
                                                .db
                                                .update_task(
                                                    &task_id,
                                                    "failed",
                                                    &format!("整场逐字稿生成失败: {error}"),
                                                    None,
                                                )
                                                .await;
                                            Err(error.to_string())
                                        }
                                    }
                                }))
                                .await
                            {
                                log::error!("Failed to queue archive subtitle task: {error}");
                            }
                        }
                        Err(error) => {
                            log::error!("Failed to create archive subtitle reporter: {error}");
                        }
                    }
                }
                Err(error) => log::error!("Failed to create archive subtitle task: {error}"),
            }
        }

        if auto_multi_stitch {
            self.queue_auto_whole_clip(
                platform,
                room_id,
                live_record.parent_id.clone(),
                false,
                &format!("检测到同场 {multi_segment_count} 段录制，正在自动拼接整场视频"),
            )
            .await;
            return;
        }

        if !auto_generate {
            return;
        }

        self.queue_auto_whole_clip(
            platform,
            room_id,
            live_record.parent_id,
            encode_danmu,
            "正在自动生成完整切片",
        )
        .await;
    }

    pub fn set_migrating(&self, migrating: bool) {
        self.is_migrating
            .store(migrating, std::sync::atomic::Ordering::Relaxed);
    }

    pub async fn has_active_recording(&self) -> bool {
        let guard = self.recorders.read().await;
        for recorder in guard.values() {
            let info = recorder.info().await;
            if is_indexable_active_recording(info.recording, &info.live_id) {
                return true;
            }
        }
        false
    }

    /// Resume only idempotent, chunk-cached ASR work after an application
    /// restart. Rendering and AI clip tasks remain review-required because
    /// blindly repeating them could create duplicate user-visible files.
    pub async fn resume_interrupted_tasks(&self, tasks: Vec<TaskRow>) {
        for task in tasks {
            if task.task_type != "generate_archive_subtitle" {
                continue;
            }
            let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&task.metadata) else {
                continue;
            };
            let Some(platform) = metadata
                .get("platform")
                .and_then(|value| value.as_str())
                .and_then(|value| PlatformType::from_str(value).ok())
            else {
                continue;
            };
            let Some(room_id) = metadata.get("room_id").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(live_id) = metadata.get("live_id").and_then(|value| value.as_str()) else {
                continue;
            };

            let task_id = task.id.clone();
            if let Err(error) = self
                .db
                .update_task(&task_id, "pending", "程序重启后正在恢复整场逐字稿", None)
                .await
            {
                log::error!("Failed to prepare interrupted ASR task {task_id}: {error}");
                continue;
            }
            let Ok(reporter) =
                ProgressReporter::new(self.db.clone(), &self.emitter, &task_id).await
            else {
                let _ = self
                    .db
                    .update_task(
                        &task_id,
                        "failed",
                        "恢复逐字稿任务失败：无法创建进度记录",
                        None,
                    )
                    .await;
                continue;
            };
            let manager = self.clone();
            let room_id = room_id.to_string();
            let live_id = live_id.to_string();
            let scheduled_task_id = task_id.clone();
            if let Err(error) = self
                .task_manager
                .add_task(Task::new(task_id, TaskPriority::Normal, async move {
                    let _ = manager
                        .db
                        .update_task(
                            &scheduled_task_id,
                            "processing",
                            "正在从已完成分段继续生成整场逐字稿",
                            None,
                        )
                        .await;
                    match manager
                        .generate_archive_subtitle(platform, &room_id, &live_id, Some(&reporter))
                        .await
                    {
                        Ok(_) => {
                            reporter.finish(true, "整场逐字稿恢复完成").await;
                            let _ = manager
                                .db
                                .update_task(
                                    &scheduled_task_id,
                                    "success",
                                    "整场逐字稿恢复完成",
                                    None,
                                )
                                .await;
                            Ok(())
                        }
                        Err(error) => {
                            let message = format!("恢复整场逐字稿失败：{error}");
                            reporter.finish(false, &message).await;
                            let _ = manager
                                .db
                                .update_task(&scheduled_task_id, "failed", &message, None)
                                .await;
                            Err(message)
                        }
                    }
                }))
                .await
            {
                let _ = self
                    .db
                    .update_task(
                        &task.id,
                        "failed",
                        &format!("恢复逐字稿任务进入队列失败：{error}"),
                        None,
                    )
                    .await;
            }
        }
    }

    pub async fn wait_for_recording_idle(&self) {
        let mut logged = false;
        while self.has_active_recording().await {
            if !logged {
                log::info!(
                    "[media_scheduler code=REC-RESOURCE-PROTECTED] active recording has priority; heavy media work is waiting"
                );
                logged = true;
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    async fn set_recorder_health(
        &self,
        platform: &str,
        room_id: &str,
        status: &str,
        error_code: &str,
        message: &str,
        retry_count: u8,
        next_retry_at: Option<&str>,
    ) {
        if let Err(error) = self
            .db
            .upsert_recorder_health(
                platform,
                room_id,
                status,
                error_code,
                message,
                i64::from(retry_count),
                next_retry_at,
            )
            .await
        {
            log::error!(
                "[recorder_health platform={platform} room_id={room_id}] persist failed: {error}"
            );
        }
    }

    fn notify_recorder_issue(&self, title: &str, body: &str) {
        #[cfg(feature = "gui")]
        if let Err(error) = self
            .app_handle
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            log::warn!("Failed to show recorder health notification: {error}");
        }
    }

    async fn can_attempt_initialization(&self, recorder_id: &str, now: Instant) -> bool {
        self.initialization_retries
            .read()
            .await
            .get(recorder_id)
            .map(|state| !state.terminal && state.next_retry_at <= now)
            .unwrap_or(true)
    }

    async fn record_initialization_failure(
        &self,
        recorder_id: &str,
        platform: PlatformType,
        room_id: &str,
        error: &str,
    ) {
        let now = Instant::now();
        let (retry_count, delay, terminal) = {
            let mut retries = self.initialization_retries.write().await;
            if let Some(state) = retries.get_mut(recorder_id) {
                let delay = state.after_failed_retry(now);
                (state.completed_retries, delay, state.terminal)
            } else {
                let state = RetryState::first_failure(now);
                let delay = retry_delay(0);
                retries.insert(recorder_id.to_string(), state);
                (0, delay, false)
            }
        };
        let next_retry_at = delay.map(retry_timestamp);
        let (status, message) = if terminal {
            (
                "needs_attention",
                format!("录制服务连续启动失败，需要处理：{error}"),
            )
        } else {
            let minutes = delay.map(|value| value.as_secs() / 60).unwrap_or(0);
            (
                "retry_wait",
                format!("录制服务启动失败，{minutes} 分钟后自动重试：{error}"),
            )
        };
        log::error!(
            "[recorder_init platform={} room_id={} retry_count={retry_count} code={RECORDER_INIT_ERROR_CODE}] {error}",
            platform.as_str(),
            room_id
        );
        self.set_recorder_health(
            platform.as_str(),
            room_id,
            status,
            RECORDER_INIT_ERROR_CODE,
            &message,
            retry_count,
            next_retry_at.as_deref(),
        )
        .await;
        self.notify_recorder_issue(
            if terminal {
                "典典直播切片 - 录制服务需要处理"
            } else {
                "典典直播切片 - 录制服务启动失败"
            },
            &format!("直播间 {room_id}：{message}"),
        );
    }

    async fn restart_managed_recorder(
        &self,
        platform: PlatformType,
        room_id: &str,
    ) -> Result<(), String> {
        let recorder_id = format!("{}:{}", platform.as_str(), room_id);
        let row = self
            .db
            .get_recorders()
            .await
            .map_err(String::from)?
            .into_iter()
            .find(|row| row.platform == platform.as_str() && row.room_id == room_id)
            .ok_or_else(|| format!("直播间 {room_id} 已不在监控列表"))?;
        let account = self
            .db
            .get_account_by_platform(platform.as_str())
            .await
            .map_err(|_| format!("{} 账号已失效或不存在，请重新扫码", platform.as_str()))?
            .to_account();

        let recorder = {
            let mut recorders = self.recorders.write().await;
            recorders.remove(&recorder_id)
        };
        if let Some(recorder) = recorder {
            recorder.stop().await;
        }
        self.add_recorder(&account, platform, room_id, &row.extra, row.auto_start)
            .await
            .map_err(String::from)
    }

    async fn watchdog_recording_starts(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if self.is_migrating.load(std::sync::atomic::Ordering::Relaxed) {
                continue;
            }
            let cache_readiness = {
                let mut config = self.config.write().await;
                crate::storage_readiness::ensure_recording_cache(&mut config)
            };
            let recorders = {
                let guard = self.recorders.read().await;
                let mut values = Vec::with_capacity(guard.len());
                for recorder in guard.values() {
                    values.push(recorder.info().await);
                }
                values
            };

            let cache_readiness = match cache_readiness {
                Ok(value) => value,
                Err(error) => {
                    for info in &recorders {
                        self.set_recorder_health(
                            &info.room_info.platform,
                            &info.room_info.room_id,
                            "needs_attention",
                            "REC-STORAGE-UNAVAILABLE",
                            &error,
                            0,
                            None,
                        )
                        .await;
                    }
                    continue;
                }
            };
            if cache_readiness.fell_back {
                log::warn!("{}", cache_readiness.detail);
                self.notify_recorder_issue(
                    "典典直播切片 - 已切换本机录制目录",
                    &cache_readiness.detail,
                );
            }

            for info in recorders {
                let platform = info.room_info.platform.clone();
                let room_id = info.room_info.room_id.clone();
                let recorder_id = format!("{platform}:{room_id}");
                if !info.enabled {
                    self.live_start_retries.write().await.remove(&recorder_id);
                    self.set_recorder_health(
                        &platform,
                        &room_id,
                        "disabled",
                        "",
                        "直播间监控已关闭",
                        0,
                        None,
                    )
                    .await;
                    continue;
                }
                if cache_readiness.fell_back {
                    let Ok(platform_type) = PlatformType::from_str(&platform) else {
                        continue;
                    };
                    self.recording_growth.write().await.remove(&recorder_id);
                    match self.restart_managed_recorder(platform_type, &room_id).await {
                        Ok(()) => {
                            self.set_recorder_health(
                                &platform,
                                &room_id,
                                "live_waiting",
                                "REC-STORAGE-FALLBACK",
                                "原录制目录不可用，已切换本机目录并重启录制服务",
                                0,
                                None,
                            )
                            .await;
                        }
                        Err(error) => {
                            self.set_recorder_health(
                                &platform,
                                &room_id,
                                "needs_attention",
                                "REC-STORAGE-FALLBACK-RESTART",
                                &format!("已切换本机目录，但录制服务重启失败：{error}"),
                                0,
                                None,
                            )
                            .await;
                        }
                    }
                    continue;
                }
                if info.recording {
                    let Ok(platform_type) = PlatformType::from_str(&platform) else {
                        continue;
                    };
                    let work_dir = cache_readiness
                        .active_path
                        .join(platform_type.as_str())
                        .join(&room_id)
                        .join(&info.live_id);
                    let measured_size = measure_recording_cache(&work_dir)
                        .await
                        .map(|(_, size)| size);
                    let measurement_error = measured_size.as_ref().err().map(ToString::to_string);
                    let current_size = measured_size.as_ref().copied().unwrap_or(0);
                    let (growth, decision) = {
                        let previous = self.recording_growth.read().await;
                        evaluate_recording_growth(
                            previous.get(&recorder_id),
                            &info.live_id,
                            current_size,
                        )
                    };
                    self.recording_growth
                        .write()
                        .await
                        .insert(recorder_id.clone(), growth);

                    match decision {
                        RecordingGrowthDecision::Starting => {
                            self.set_recorder_health(
                                &platform,
                                &room_id,
                                "recording_starting",
                                "",
                                "录制进程已启动，正在确认录像文件写入",
                                0,
                                None,
                            )
                            .await;
                        }
                        RecordingGrowthDecision::Growing => {
                            self.live_start_retries.write().await.remove(&recorder_id);
                            self.recording_stall_retries
                                .write()
                                .await
                                .remove(&recorder_id);
                            self.set_recorder_health(
                                &platform,
                                &room_id,
                                "recording",
                                "",
                                &format!(
                                    "录像文件持续写入（已写入 {:.1} MB）",
                                    current_size as f64 / 1024.0 / 1024.0
                                ),
                                0,
                                None,
                            )
                            .await;
                        }
                        RecordingGrowthDecision::Stalled => {
                            let detail = measurement_error
                                .map(|error| format!("读取录像目录失败：{error}"))
                                .unwrap_or_else(|| "录像文件连续约 45 秒没有增长".to_string());
                            let now = Instant::now();
                            let existing = self
                                .recording_stall_retries
                                .read()
                                .await
                                .get(&recorder_id)
                                .cloned();
                            let (retry_count, next_delay, terminal, should_restart) = match existing
                            {
                                Some(state) if state.terminal => {
                                    (state.completed_retries, None, true, false)
                                }
                                Some(state) if state.next_retry_at > now => {
                                    let wait = state.next_retry_at.saturating_duration_since(now);
                                    (state.completed_retries, Some(wait), false, false)
                                }
                                Some(mut state) => {
                                    let delay = state.after_failed_retry(now);
                                    let values =
                                        (state.completed_retries, delay, state.terminal, true);
                                    self.recording_stall_retries
                                        .write()
                                        .await
                                        .insert(recorder_id.clone(), state);
                                    values
                                }
                                None => {
                                    let state = RetryState::first_failure(now);
                                    let delay = retry_delay(0);
                                    self.recording_stall_retries
                                        .write()
                                        .await
                                        .insert(recorder_id.clone(), state);
                                    (0, delay, false, true)
                                }
                            };
                            if terminal {
                                self.set_recorder_health(
                                    &platform,
                                    &room_id,
                                    "needs_attention",
                                    RECORDING_FILE_STALLED_ERROR_CODE,
                                    &format!(
                                        "{detail}；三次自动恢复后仍失败，请检查网络或录制目录"
                                    ),
                                    retry_count,
                                    None,
                                )
                                .await;
                                continue;
                            }
                            let next_retry_at = next_delay.map(retry_timestamp);
                            let wait_minutes = next_delay
                                .map(|delay| ((delay.as_secs() + 59) / 60).max(1))
                                .unwrap_or(1);
                            self.set_recorder_health(
                                &platform,
                                &room_id,
                                "retry_wait",
                                RECORDING_FILE_STALLED_ERROR_CODE,
                                &format!(
                                    "{detail}；{}",
                                    if should_restart {
                                        "正在重启录制服务".to_string()
                                    } else {
                                        format!("约 {wait_minutes} 分钟后再次自动恢复")
                                    }
                                ),
                                retry_count,
                                next_retry_at.as_deref(),
                            )
                            .await;
                            if !should_restart {
                                continue;
                            }
                            self.notify_recorder_issue(
                                "典典直播切片 - 录像写入已停止",
                                &format!("直播间 {room_id}：{detail}，系统正在自动恢复"),
                            );
                            self.recording_growth.write().await.remove(&recorder_id);
                            if let Err(error) =
                                self.restart_managed_recorder(platform_type, &room_id).await
                            {
                                self.set_recorder_health(
                                    &platform,
                                    &room_id,
                                    "retry_wait",
                                    RECORDING_FILE_STALLED_ERROR_CODE,
                                    &format!(
                                        "{detail}；本次自动重启失败：{error}，系统仍会继续重试"
                                    ),
                                    retry_count,
                                    next_retry_at.as_deref(),
                                )
                                .await;
                            }
                        }
                    }
                    continue;
                }
                if !info.room_info.status {
                    self.live_start_retries.write().await.remove(&recorder_id);
                    self.set_recorder_health(
                        &platform,
                        &room_id,
                        "ready",
                        "",
                        "后台监控正常，等待开播",
                        0,
                        None,
                    )
                    .await;
                    continue;
                }

                let now = Instant::now();
                let existing = self
                    .live_start_retries
                    .read()
                    .await
                    .get(&recorder_id)
                    .cloned();
                let Some(mut retry_state) = existing else {
                    let state = RetryState::first_failure(now);
                    let next_retry = retry_timestamp(retry_delay(0).expect("first retry delay"));
                    self.live_start_retries
                        .write()
                        .await
                        .insert(recorder_id.clone(), state);
                    self.set_recorder_health(
                        &platform,
                        &room_id,
                        "live_waiting",
                        RECORDING_START_ERROR_CODE,
                        "已检测到直播，正在等待录制启动",
                        0,
                        Some(&next_retry),
                    )
                    .await;
                    continue;
                };
                if retry_state.terminal || retry_state.next_retry_at > now {
                    continue;
                }

                let platform_type = match PlatformType::from_str(&platform) {
                    Ok(value) => value,
                    Err(error) => {
                        log::error!("Invalid recorder platform {platform}: {error}");
                        continue;
                    }
                };
                let restart_result = self.restart_managed_recorder(platform_type, &room_id).await;
                let next_delay = retry_state.after_failed_retry(now);
                let retry_count = retry_state.completed_retries;
                let terminal = retry_state.terminal;
                self.live_start_retries
                    .write()
                    .await
                    .insert(recorder_id.clone(), retry_state);
                let detail = restart_result
                    .err()
                    .unwrap_or_else(|| "已重新启动录制服务".to_string());
                let next_retry_at = next_delay.map(retry_timestamp);
                let (status, message) = if terminal {
                    (
                        "needs_attention",
                        format!("三次自动重试后仍未开始录制，需要处理。{detail}"),
                    )
                } else {
                    let minutes = next_delay.map(|value| value.as_secs() / 60).unwrap_or(0);
                    (
                        "retry_wait",
                        format!("第 {retry_count} 次自动重试已执行；若仍失败，{minutes} 分钟后继续。{detail}"),
                    )
                };
                log::warn!(
                    "[recorder_watchdog platform={platform} room_id={room_id} retry_count={retry_count} code={RECORDING_START_ERROR_CODE}] {message}"
                );
                self.set_recorder_health(
                    &platform,
                    &room_id,
                    status,
                    RECORDING_START_ERROR_CODE,
                    &message,
                    retry_count,
                    next_retry_at.as_deref(),
                )
                .await;
                self.notify_recorder_issue(
                    if terminal {
                        "典典直播切片 - 录制需要处理"
                    } else {
                        "典典直播切片 - 正在自动恢复录制"
                    },
                    &format!("直播间 {room_id}：{message}"),
                );
            }
        }
    }

    async fn monitor_recorders(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            if self.is_migrating.load(std::sync::atomic::Ordering::Relaxed) {
                interval.tick().await;
                continue;
            }
            // get a list of recorders in db, if not created yet, create them
            let recorders = self.db.get_recorders().await;
            if recorders.is_err() {
                log::error!(
                    "Failed to get recorders from db: {}",
                    recorders.err().unwrap()
                );
                return;
            }
            let recorders = recorders.unwrap();
            let mut recorder_map = HashMap::new();
            for recorder in recorders {
                let platform = PlatformType::from_str(&recorder.platform).unwrap();
                let room_id = recorder.room_id;
                let auto_start = recorder.auto_start;
                let extra = recorder.extra;
                recorder_map.insert((platform, room_id), (auto_start, extra));
            }
            let mut recorders_to_add = Vec::new();
            for (platform, room_id) in recorder_map.keys() {
                let recorder_id = format!("{}:{}", platform.as_str(), room_id);
                if !self.recorders.read().await.contains_key(&recorder_id)
                    && !self.to_remove.read().await.contains(&recorder_id)
                {
                    recorders_to_add.push((*platform, room_id.clone()));
                }
            }
            for (platform, room_id) in recorders_to_add {
                if self.is_migrating.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let recorder_id = format!("{}:{}", platform.as_str(), room_id);
                if !self
                    .can_attempt_initialization(&recorder_id, Instant::now())
                    .await
                {
                    continue;
                }
                let (auto_start, extra) = recorder_map.get(&(platform, room_id.clone())).unwrap();
                let account = self
                    .db
                    .get_account_by_platform(platform.clone().as_str())
                    .await;
                if platform != PlatformType::Huya
                    && platform != PlatformType::Kuaishou
                    && platform != PlatformType::TikTok
                    && account.is_err()
                {
                    log::warn!("Failed to find an account for {platform:?} {room_id}");
                    self.set_recorder_health(
                        platform.as_str(),
                        &room_id,
                        "login_required",
                        RECORDER_LOGIN_ERROR_CODE,
                        "登录已失效或账号不存在，请重新扫码；后台仍会继续检查",
                        0,
                        None,
                    )
                    .await;
                    continue;
                }
                let account = if let Ok(account) = account {
                    account.to_account()
                } else {
                    Account::default()
                };

                if let Err(e) = self
                    .add_recorder(&account, platform, &room_id, extra, *auto_start)
                    .await
                {
                    log::error!(
                        "Failed to add recorder: {} {} {}",
                        platform.as_str(),
                        room_id,
                        e
                    );
                    self.record_initialization_failure(
                        &recorder_id,
                        platform,
                        &room_id,
                        &e.to_string(),
                    )
                    .await;
                } else {
                    self.initialization_retries
                        .write()
                        .await
                        .remove(&recorder_id);
                    self.set_recorder_health(
                        platform.as_str(),
                        &room_id,
                        if *auto_start { "ready" } else { "disabled" },
                        "",
                        if *auto_start {
                            "录制服务已启动，等待开播"
                        } else {
                            "直播间监控已关闭"
                        },
                        0,
                        None,
                    )
                    .await;
                }
            }
            interval.tick().await;
        }
    }

    pub async fn add_recorder(
        &self,
        account: &Account,
        platform: PlatformType,
        room_id: &str,
        extra: &str,
        enabled: bool,
    ) -> Result<(), RecorderManagerError> {
        let recorder_id = format!("{}:{}", platform.as_str(), room_id);
        if self.recorders.read().await.contains_key(&recorder_id) {
            return Err(RecorderManagerError::AlreadyExisted {
                room_id: room_id.to_string(),
            });
        }

        let cache_dir = self.config.read().await.cache.clone();
        let cache_dir = PathBuf::from(&cache_dir);

        let event_tx = self.get_event_sender();
        let update_interval = self.config.read().await.update_interval.clone();
        let recorder: RecorderType = match platform {
            PlatformType::BiliBili => RecorderType::BiliBili(
                BiliRecorder::new(
                    room_id,
                    account,
                    cache_dir,
                    event_tx,
                    update_interval,
                    enabled,
                )
                .await?,
            ),
            PlatformType::Douyin => RecorderType::Douyin(
                DouyinRecorder::new(
                    room_id,
                    extra,
                    account,
                    cache_dir,
                    event_tx,
                    update_interval,
                    enabled,
                )
                .await?,
            ),
            PlatformType::Huya => RecorderType::Huya(
                HuyaRecorder::new(
                    room_id,
                    account,
                    cache_dir,
                    event_tx,
                    update_interval,
                    enabled,
                )
                .await?,
            ),
            PlatformType::Kuaishou => RecorderType::Kuaishou(
                KuaishouRecorder::new(
                    room_id,
                    account,
                    cache_dir,
                    event_tx,
                    update_interval,
                    enabled,
                )
                .await?,
            ),
            PlatformType::TikTok => RecorderType::TikTok(
                TikTokRecorder::new(
                    room_id,
                    account,
                    cache_dir,
                    event_tx,
                    update_interval,
                    enabled,
                )
                .await?,
            ),
            _ => {
                return Err(RecorderManagerError::InvalidPlatformType {
                    platform: platform.as_str().to_string(),
                })
            }
        };
        self.recorders
            .write()
            .await
            .insert(recorder_id.clone(), recorder);
        if let Some(recorder_ref) = self.recorders.read().await.get(&recorder_id) {
            recorder_ref.run().await;
        }
        Ok(())
    }

    pub async fn stop_all(&self) {
        for recorder_ref in self.recorders.read().await.values() {
            recorder_ref.stop().await;
        }

        // remove all recorders
        self.recorders.write().await.clear();
    }

    /// Remove a recorder from the manager
    ///
    /// This will stop the recorder and remove it from the manager
    /// while preserving historical archive files.
    pub async fn remove_recorder(
        &self,
        platform: PlatformType,
        room_id: &str,
    ) -> Result<RecorderRow, RecorderManagerError> {
        // check recorder exists
        let recorder_id = format!("{}:{}", platform.as_str(), room_id);
        if !self.recorders.read().await.contains_key(&recorder_id) {
            return Err(RecorderManagerError::NotFound {
                room_id: room_id.to_string(),
            });
        }

        // remove from db
        let recorder = self.db.remove_recorder(room_id).await?;
        let _ = self
            .db
            .delete_recorder_health(platform.as_str(), room_id)
            .await;
        self.initialization_retries
            .write()
            .await
            .remove(&recorder_id);
        self.live_start_retries.write().await.remove(&recorder_id);

        // add to to_remove
        log::debug!("Add to to_remove: {recorder_id}");
        self.to_remove.write().await.insert(recorder_id.clone());

        // stop recorder
        log::debug!("Stop recorder: {recorder_id}");
        if let Some(recorder_ref) = self.recorders.read().await.get(&recorder_id) {
            recorder_ref.stop().await;
        }

        // remove recorder
        log::debug!("Remove recorder from manager: {recorder_id}");
        self.recorders.write().await.remove(&recorder_id);

        // remove from to_remove
        log::debug!("Remove from to_remove: {recorder_id}");
        self.to_remove.write().await.remove(&recorder_id);

        Ok(recorder)
    }

    async fn load_playlist_bytes(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<Vec<u8>, RecorderManagerError> {
        let playlist_path = self
            .resolve_archive_dir(platform, room_id, live_id)
            .await
            .join("playlist.m3u8");
        if !playlist_path.exists() {
            return Err(RecorderManagerError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Playlist file not found",
            )));
        }
        let mut bytes: Vec<u8> = Vec::new();
        tokio::fs::File::open(playlist_path)
            .await
            .unwrap()
            .read_to_end(&mut bytes)
            .await
            .unwrap();
        Ok(bytes)
    }

    /// Check if the playlist is outdated
    ///
    /// This will check if the current recorder live id is the same as the live id
    /// and if the current recorder is recording
    /// and if the current recorder is recording, return false
    /// otherwise, return true
    async fn is_outdated_playlist(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> bool {
        // check current recorder live id is the same as the live id
        let recorder = self.get_recorder_info(platform, room_id).await;
        let Some(recorder) = recorder else {
            return true;
        };

        if recorder.live_id != live_id {
            return true;
        }

        false
    }

    fn prepare_playlist_for_playback(playlist: &mut MediaPlaylist, finished: bool) {
        // Some Douyin recordings are persisted with TARGETDURATION=0. That is
        // not a valid HLS manifest and leaves Shaka waiting at 0:00 even though
        // every TS segment exists. Derive a legal integer target duration from
        // the actual segments when serving the playlist; keep source files
        // untouched so recording and ASR behavior do not change.
        let required_target_duration = playlist
            .segments
            .iter()
            .map(|segment| segment.duration.ceil())
            .fold(1.0_f32, f32::max);
        if !playlist.target_duration.is_finite()
            || playlist.target_duration < required_target_duration
        {
            playlist.target_duration = required_target_duration;
        }

        if finished {
            playlist.end_list = true;
            playlist.playlist_type = Some(MediaPlaylistType::Vod);
        }
    }

    async fn load_playlist(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<MediaPlaylist, RecorderManagerError> {
        let bytes = self.load_playlist_bytes(platform, room_id, live_id).await?;
        if let Result::Ok((_, mut pl)) = m3u8_rs::parse_media_playlist(&bytes) {
            let finished = self.is_outdated_playlist(platform, room_id, live_id).await;
            Self::prepare_playlist_for_playback(&mut pl, finished);
            return Ok(pl);
        }
        Err(RecorderManagerError::M3u8ParseFailed {
            content: String::from_utf8(bytes).unwrap(),
        })
    }

    async fn playlist_range(
        &self,
        playlist: &MediaPlaylist,
        range: Option<Range>,
    ) -> Result<MediaPlaylist, RecorderManagerError> {
        let mut playlist = playlist.clone();
        if let Some(range) = range {
            let mut duration = 0.0f64;
            let mut segments = Vec::new();
            for s in playlist.segments {
                if range.is_in(duration) || range.is_in(duration + s.duration as f64) {
                    segments.push(s.clone());
                }
                duration += s.duration as f64;
            }
            playlist.segments = segments;
            playlist.end_list = true;
            playlist.playlist_type = Some(MediaPlaylistType::Vod);
        }

        Ok(playlist)
    }

    async fn first_segment_timestamp(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<i64, RecorderManagerError> {
        let playlist = self.load_playlist(platform, room_id, live_id).await?;
        if playlist.segments.is_empty() {
            return Err(RecorderManagerError::EmptyPlaylist);
        }

        let first_segment = playlist.segments.first().unwrap();
        if let Some(program_date_time) = first_segment.program_date_time {
            return Ok(program_date_time.timestamp_millis());
        }

        // else, find in unknown tags
        let program_date_time = first_segment
            .unknown_tags
            .iter()
            .find(|t| t.tag == "X-PROGRAM-DATE-TIME");

        let Some(program_date_time) = program_date_time else {
            return live_id
                .parse::<i64>()
                .map_err(|_| RecorderManagerError::InvalidLiveID);
        };

        let Some(value) = &program_date_time.rest else {
            return live_id
                .parse::<i64>()
                .map_err(|_| RecorderManagerError::InvalidLiveID);
        };

        // example: "2025-10-18T17:18:17.004+0800"
        // convert to timestamp
        let timestamp = DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.3f%z")
            .unwrap()
            .timestamp_millis();
        Ok(timestamp)
    }

    pub async fn load_danmus(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<Vec<DanmuEntry>, RecorderManagerError> {
        let danmus_path = self
            .resolve_archive_dir(platform, room_id, live_id)
            .await
            .join("danmu.txt");
        if !danmus_path.exists() {
            return Ok(Vec::new());
        }
        let Some(storage) = DanmuStorage::new(&danmus_path).await else {
            log::error!("Failed to load danmu storage: {danmus_path:?}");
            return Ok(Vec::new());
        };
        Ok(storage.get_entries(0).await)
    }

    /// Get related playlists by parent id
    ///
    /// This will return a list of tuples, the first element is the title of the archive,
    /// the second element is the path of the playlist
    async fn get_related_playlists(
        &self,
        platform: &PlatformType,
        room_id: &str,
        parent_id: &str,
    ) -> Vec<RelatedPlaylist> {
        let archives = self.db.get_archives_by_parent_id(room_id, parent_id).await;
        if let Err(e) = archives {
            log::error!(
                "[{}] Failed to get all related playlists: {} {}",
                room_id,
                parent_id,
                e
            );
            return Vec::new();
        }

        let archives = archives.unwrap();
        let playlists = archives
            .iter()
            .map(async |archive| RelatedPlaylist {
                live_id: archive.live_id.clone(),
                title: archive.title.clone(),
                path: self
                    .resolve_archive_dir(*platform, room_id, &archive.live_id)
                    .await
                    .join("playlist.m3u8"),
            })
            .collect::<Vec<_>>();

        let playlists = futures::future::join_all(playlists).await;

        playlists
    }

    pub async fn clip_range(
        &self,
        reporter: Option<&ProgressReporter>,
        clip_file: PathBuf,
        params: &ClipRangeParams,
    ) -> Result<PathBuf, RecorderManagerError> {
        let platform = PlatformType::from_str(&params.platform).map_err(|_| {
            RecorderManagerError::InvalidPlatformType {
                platform: params.platform.clone(),
            }
        })?;
        let playlist_path = self
            .resolve_archive_dir(platform, &params.room_id, &params.live_id)
            .await
            .join("playlist.m3u8");

        if !playlist_path.exists() {
            log::error!("Playlist file not found: {}", playlist_path.display());
            return Err(RecorderManagerError::ClipError {
                err: "Playlist file not found".to_string(),
            });
        }

        if params.ranges.is_empty() {
            crate::ffmpeg::playlist::clip_from_playlist(reporter, &playlist_path, &clip_file, None)
                .await
                .map_err(|e| RecorderManagerError::ClipError { err: e.to_string() })?;
        } else {
            crate::ffmpeg::playlist::clip_multiple_from_playlist(
                reporter,
                &playlist_path,
                &clip_file,
                &params.ranges,
                params.transition.as_deref(),
            )
            .await
            .map_err(|e| RecorderManagerError::ClipError { err: e.to_string() })?;
        }

        if params.fix_encoding && !params.danmu {
            // transcode clip_file
            let tmp_clip_file = clip_file.with_extension("tmp.mp4");
            if let Err(e) = transcode(reporter, &clip_file, &tmp_clip_file, false).await {
                log::error!("Failed to transcode clip file: {e}");
                return Err(RecorderManagerError::ClipError { err: e.to_string() });
            }

            // remove clip_file
            let _ = tokio::fs::remove_file(&clip_file).await;

            // rename tmp_clip_file to clip_file
            let _ = tokio::fs::rename(tmp_clip_file, &clip_file).await;
        }

        if !params.danmu {
            log::info!("Skip danmu encoding");
            return Ok(clip_file);
        }

        let Ok(platform) = PlatformType::from_str(&params.platform) else {
            return Err(RecorderManagerError::InvalidPlatformType {
                platform: params.platform.clone(),
            });
        };
        let stream_start_timestamp_milis = self
            .first_segment_timestamp(platform, &params.room_id, &params.live_id)
            .await?;

        let danmus = self
            .load_danmus(platform, &params.room_id, &params.live_id)
            .await;
        if danmus.is_err() {
            log::error!(
                "Failed to get danmus, skip danmu encoding: {}",
                danmus.err().unwrap()
            );
            return Ok(clip_file);
        }

        let mut danmus = danmus.unwrap();
        log::debug!("First danmu entry: {:?}", danmus.first());
        log::debug!("Last danmu entry: {:?}", danmus.last());
        log::debug!("Stream start timestamp: {}", stream_start_timestamp_milis);
        log::debug!("Local offset: {}", params.local_offset);
        log::debug!("Range: {:?}", params.ranges);

        // update danmu entry ts to relative offset
        for d in &mut danmus {
            d.ts -= stream_start_timestamp_milis + params.local_offset * 1000;
        }

        let mut range_anchors = vec![0; params.ranges.len()];
        for i in 0..params.ranges.len() {
            if i == 0 {
                continue;
            }
            range_anchors[i] =
                (params.ranges[i - 1].duration() * 1000.0) as i64 + range_anchors[i - 1];
        }

        log::debug!("Range anchors: {:?}", range_anchors);

        let mut filtered_danmus = Vec::<DanmuEntry>::new();
        for (i, range) in params.ranges.iter().enumerate() {
            filtered_danmus.extend(self.filter_danmus_in_range(
                danmus.clone(),
                range,
                range_anchors[i],
            ));
        }

        let ass_content = danmu2ass::danmu_to_ass(
            filtered_danmus,
            self.config.read().await.danmu_ass_options.clone(),
        );
        // dump ass_content into a temp file
        let ass_file_path = clip_file.with_extension("ass");
        if let Err(e) = write(&ass_file_path, ass_content).await {
            log::error!(
                "Failed to write temp ass file: {} {}",
                ass_file_path.display(),
                e
            );
            return Ok(clip_file);
        }

        let result = encode_video_danmu(reporter, &clip_file, &ass_file_path).await;
        // clean ass file
        let _ = remove_file(ass_file_path).await;
        let _ = remove_file(clip_file).await;

        result.map_err(|e| RecorderManagerError::ClipError { err: e })
    }

    fn filter_danmus_in_range(
        &self,
        mut danmus: Vec<DanmuEntry>,
        range: &Range,
        anchor: i64,
    ) -> Vec<DanmuEntry> {
        for d in &mut danmus {
            d.ts -= (range.start * 1000.0) as i64;
        }
        if range.duration() > 0.0 {
            danmus.retain(|x| x.ts >= 0 && x.ts <= (range.duration() * 1000.0).round() as i64);
        }

        for d in &mut danmus {
            d.ts += anchor;
        }

        danmus
    }

    async fn generate_archive_danmu_ass(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<PathBuf, RecorderManagerError> {
        log::info!(
            "Generate archive danmu ass file for {} {} {}",
            platform.as_str(),
            room_id,
            live_id
        );
        let first_segment_timestamp_milis = self
            .first_segment_timestamp(platform, room_id, live_id)
            .await?;
        let mut danmus = self.load_danmus(platform, room_id, live_id).await?;
        danmus.retain(|x| x.ts >= first_segment_timestamp_milis);
        for d in &mut danmus {
            d.ts -= first_segment_timestamp_milis;
        }
        let ass_content =
            danmu2ass::danmu_to_ass(danmus, self.config.read().await.danmu_ass_options.clone());
        let work_dir = self
            .resolve_archive_cache_path(platform, room_id, live_id)
            .await;
        let ass_file_path = work_dir.with_filename("danmu.ass");
        if let Err(e) = write(&ass_file_path.full_path(), ass_content).await {
            log::error!(
                "Failed to write archive danmu ass file: {} {}",
                ass_file_path.full_path().display(),
                e
            );
            return Err(RecorderManagerError::ArchiveDanmuAssGenerationFailed {
                error: e.to_string(),
            });
        }
        Ok(ass_file_path.full_path())
    }

    pub async fn get_recorder_list(&self) -> RecorderList {
        let mut summary = RecorderList {
            count: 0,
            recorders: Vec::new(),
        };

        let assignments = self
            .db
            .list_recorder_streamer_assignments()
            .await
            .unwrap_or_else(|error| {
                log::error!("Failed to list recorder streamer assignments: {error}");
                Vec::new()
            });
        let assignment_map = assignments
            .into_iter()
            .map(|assignment| {
                (
                    format!("{}:{}", assignment.platform, assignment.room_id),
                    assignment.streamer_name,
                )
            })
            .collect::<HashMap<_, _>>();

        // initialized recorder set
        let mut recorder_set = HashSet::new();
        for recorder_ref in self.recorders.read().await.iter() {
            let mut recorder_info = recorder_ref.1.info().await;
            self.enrich_current_streamer(&mut recorder_info, &assignment_map)
                .await;
            summary.recorders.push(recorder_info.clone());
            recorder_set.insert(recorder_info.room_info.room_id);
        }

        // get recorders from db
        let recorders = self.db.get_recorders().await;
        if recorders.is_err() {
            log::error!(
                "Failed to get recorders from db: {}",
                recorders.err().unwrap()
            );
            return summary;
        }
        let recorders = recorders.unwrap();
        summary.count = recorders.len();
        for recorder in recorders {
            // check if recorder is in recorder_set
            if !recorder_set.contains(&recorder.room_id.to_string()) {
                let mut recorder_info = RecorderInfo {
                    platform_live_id: "".to_string(),
                    live_id: "".to_string(),
                    recording: false,
                    enabled: false,
                    current_streamer: String::new(),
                    current_streamer_source: String::new(),
                    room_info: RoomInfo {
                        platform: recorder.platform.as_str().to_string(),
                        status: false,
                        room_id: recorder.room_id.to_string(),
                        room_title: recorder.room_id.to_string(),
                        room_cover: "".to_string(),
                    },
                    user_info: UserInfo {
                        user_id: "".to_string(),
                        user_name: "".to_string(),
                        user_avatar: "".to_string(),
                    },
                };
                self.enrich_current_streamer(&mut recorder_info, &assignment_map)
                    .await;
                summary.recorders.push(recorder_info);
            }
        }

        summary
            .recorders
            .sort_by(|a, b| a.room_info.room_id.cmp(&b.room_info.room_id));
        summary
    }

    async fn enrich_current_streamer(
        &self,
        recorder: &mut RecorderInfo,
        assignment_map: &HashMap<String, String>,
    ) {
        let key = format!(
            "{}:{}",
            recorder.room_info.platform, recorder.room_info.room_id
        );
        if let Some(streamer_name) = assignment_map.get(&key) {
            recorder.current_streamer = streamer_name.clone();
            recorder.current_streamer_source = "manual".to_string();
            return;
        }

        if recorder.recording && !recorder.live_id.trim().is_empty() {
            if let Ok(record) = self
                .db
                .get_record(&recorder.room_info.room_id, &recorder.live_id)
                .await
            {
                if !record.anchor_name.trim().is_empty()
                    && (record.anchor_source == "manual"
                        || record.anchor_detection_status == "confirmed")
                {
                    recorder.current_streamer = record.anchor_name;
                    recorder.current_streamer_source = if record.anchor_source == "manual" {
                        "manual".to_string()
                    } else {
                        "auto".to_string()
                    };
                }
            }
        }
    }

    pub async fn set_room_streamer_assignment(
        &self,
        platform: PlatformType,
        room_id: &str,
        streamer_name: Option<&str>,
    ) -> Result<(), RecorderManagerError> {
        let exists =
            self.db.get_recorders().await?.iter().any(|recorder| {
                recorder.platform == platform.as_str() && recorder.room_id == room_id
            });
        if !exists {
            return Err(RecorderManagerError::NotFound {
                room_id: room_id.to_string(),
            });
        }

        match streamer_name {
            Some(name) => {
                self.db
                    .set_recorder_streamer_assignment(platform, room_id, name)
                    .await?;
            }
            None => {
                self.db
                    .remove_recorder_streamer_assignment(platform, room_id)
                    .await?;
            }
        }

        let Some(active) = self.get_recorder_info(platform, room_id).await else {
            return Ok(());
        };
        if !active.recording || active.live_id.trim().is_empty() {
            return Ok(());
        }

        if let Some(name) = streamer_name {
            self.db
                .save_record_anchor_manual(&active.live_id, name)
                .await?;
        } else {
            self.db.clear_record_anchor_manual(&active.live_id).await?;
            crate::handlers::anchor_detection::start_live_anchor_detection(
                self.clone(),
                self.db.clone(),
                self.config.clone(),
                platform.as_str().to_string(),
                room_id.to_string(),
                active.live_id,
            );
        }
        Ok(())
    }

    async fn apply_room_streamer_or_start_detection(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) {
        match self
            .db
            .get_recorder_streamer_assignment(platform, room_id)
            .await
        {
            Ok(Some(assignment)) => {
                if let Err(error) = self
                    .db
                    .save_record_anchor_manual(live_id, &assignment.streamer_name)
                    .await
                {
                    log::error!(
                        "Failed to apply assigned streamer to live record {live_id}: {error}"
                    );
                }
            }
            Ok(None) => {
                crate::handlers::anchor_detection::start_live_anchor_detection(
                    self.clone(),
                    self.db.clone(),
                    self.config.clone(),
                    platform.as_str().to_string(),
                    room_id.to_string(),
                    live_id.to_string(),
                );
            }
            Err(error) => {
                log::error!(
                    "Failed to read assigned streamer for {}:{}: {error}",
                    platform.as_str(),
                    room_id
                );
            }
        }
    }

    pub async fn get_recorder_info(
        &self,
        platform: PlatformType,
        room_id: &str,
    ) -> Option<RecorderInfo> {
        let recorder_id = format!("{}:{}", platform.as_str(), room_id);
        if let Some(recorder_ref) = self.recorders.read().await.get(&recorder_id) {
            let room_info = recorder_ref.info().await;
            Some(room_info)
        } else {
            None
        }
    }

    pub async fn get_archive_disk_usage(&self) -> Result<i64, RecorderManagerError> {
        Ok(self.db.get_record_disk_usage().await?)
    }

    pub async fn get_archives(
        &self,
        room_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<RecordRow>, RecorderManagerError> {
        // A recording can keep writing HLS segments after its database row was
        // removed (for example, when an in-progress archive was deleted).  The
        // archive page is driven by the database, so repair the index from the
        // live recorder before returning the list.
        for recorder_ref in self.recorders.read().await.values() {
            let recorder = recorder_ref.info().await;
            if is_indexable_active_recording(recorder.recording, &recorder.live_id)
                && recorder.room_info.room_id == room_id
            {
                let platform =
                    PlatformType::from_str(&recorder.room_info.platform).map_err(|_| {
                        RecorderManagerError::InvalidPlatformType {
                            platform: recorder.room_info.platform.clone(),
                        }
                    })?;
                if self
                    .db
                    .get_record(room_id, &recorder.live_id)
                    .await
                    .is_err()
                {
                    log::warn!(
                        "Repairing missing record index for active recording {}:{}",
                        room_id,
                        recorder.live_id
                    );
                    self.db
                        .add_record(
                            platform,
                            &recorder.platform_live_id,
                            &recorder.live_id,
                            room_id,
                            &recorder.room_info.room_title,
                            None,
                        )
                        .await?;
                    self.remember_current_archive_dir(platform, room_id, &recorder.live_id)
                        .await;
                    self.apply_room_streamer_or_start_detection(
                        platform,
                        room_id,
                        &recorder.live_id,
                    )
                    .await;
                }
                if let Err(error) = self
                    .sync_active_recording_stats(platform, room_id, &recorder.live_id)
                    .await
                {
                    log::warn!(
                        "Failed to sync active recording stats for {room_id}:{}: {error}",
                        recorder.live_id
                    );
                }
            }
        }
        Ok(self.db.get_records(room_id, offset, limit).await?)
    }

    async fn sync_active_recording_stats(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<(), RecorderManagerError> {
        let work_dir = self.resolve_archive_dir(platform, room_id, live_id).await;
        let (cache_length, cache_size) = measure_recording_cache(&work_dir).await?;
        if cache_length <= 0.0 && cache_size == 0 {
            return Ok(());
        }

        let record = self.db.get_record(room_id, live_id).await?;
        let length_delta = cache_length - record.length;
        let size_delta = cache_size.saturating_sub(record.size as u64);
        if length_delta <= 0.0 && size_delta == 0 {
            return Ok(());
        }

        self.db
            .update_record_delta(live_id, length_delta.max(0.0), size_delta)
            .await?;
        Ok(())
    }

    pub async fn get_archive(
        &self,
        room_id: &str,
        live_id: &str,
    ) -> Result<RecordRow, RecorderManagerError> {
        Ok(self.db.get_record(room_id, live_id).await?)
    }

    pub async fn get_archive_subtitle(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<String, RecorderManagerError> {
        // read subtitle file under work_dir
        let work_dir = self
            .resolve_archive_cache_path(platform, room_id, live_id)
            .await;
        let subtitle_file_path = work_dir.with_filename("subtitle.srt");
        let subtitle_file = File::open(subtitle_file_path.full_path()).await;
        if subtitle_file.is_err() {
            return Err(RecorderManagerError::SubtitleNotFound {
                live_id: live_id.to_string(),
            });
        }
        let subtitle_file = subtitle_file.unwrap();
        let mut subtitle_file = BufReader::new(subtitle_file);
        let mut subtitle_content = String::new();
        subtitle_file.read_to_string(&mut subtitle_content).await?;
        Ok(subtitle_content)
    }

    /// Wait for this archive's recording-time ASR to flush its final tail. A
    /// missing, failed, or timed-out live session returns false so callers can
    /// safely fall back to the existing full-session transcription path.
    pub async fn wait_for_live_transcription(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> bool {
        self.live_transcription
            .wait_until_finalized(platform.as_str(), room_id, live_id)
            .await
    }

    pub async fn save_archive_fact_card(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
        fact_card: serde_json::Value,
    ) -> Result<(), RecorderManagerError> {
        let work_dir = self
            .resolve_archive_cache_path(platform, room_id, live_id)
            .await;
        let bytes = serde_json::to_vec_pretty(&fact_card).map_err(|error| {
            RecorderManagerError::SubtitleGenerationFailed {
                error: format!("参数卡格式错误: {error}"),
            }
        })?;
        tokio::fs::write(work_dir.full_path().join("tmp.facts.json"), &bytes).await?;
        tokio::fs::write(work_dir.full_path().join("transcript.facts.json"), bytes).await?;
        Ok(())
    }

    pub async fn generate_archive_subtitle(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
        reporter: Option<&ProgressReporter>,
    ) -> Result<String, RecorderManagerError> {
        if self
            .wait_for_live_transcription(platform, room_id, live_id)
            .await
        {
            if let Ok(subtitle) = self.get_archive_subtitle(platform, room_id, live_id).await {
                if !subtitle.trim().is_empty() {
                    if let Some(reporter) = reporter {
                        reporter.update("录制中逐字稿已完成，直接复用").await;
                    }
                    return Ok(subtitle);
                }
            }
        }
        let archive_key = format!("{}:{room_id}:{live_id}", platform.as_str());
        let generation_lock =
            archive_subtitle_lock(&self.subtitle_generation_locks, &archive_key).await;
        let _generation_guard = generation_lock.lock().await;
        // Completed archives may be transcribed while other rooms are live.
        // The shared media permit still serializes heavyweight media work, but
        // live recording must not make another host's finished replay unusable.
        if let Some(reporter) = reporter {
            reporter
                .update("正在等待媒体处理通道，随后开始整场逐字稿")
                .await;
        }
        let _media_permit = self
            .media_execution_gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| RecorderManagerError::SubtitleGenerationFailed {
                error: "media execution gate is closed".to_string(),
            })?;
        if let Some(reporter) = reporter {
            reporter.update("正在提取音频并生成整场逐字稿").await;
        }

        // generate subtitle file under work_dir
        let work_dir = self
            .resolve_archive_cache_path(platform, room_id, live_id)
            .await;
        let subtitle_file_path = work_dir.with_filename("subtitle.srt");
        // first generate a tmp clip file
        // generate a tmp m3u8 index file
        let m3u8_index_file_path = work_dir.with_filename("tmp.m3u8");
        let mut playlist = self.load_playlist(platform, room_id, live_id).await?;
        playlist.end_list = true;
        playlist.playlist_type = Some(MediaPlaylistType::Vod);

        let mut v: Vec<u8> = Vec::new();
        playlist.write_to(&mut v).unwrap();
        let m3u8_content: &str = std::str::from_utf8(&v).unwrap();
        tokio::fs::write(&m3u8_index_file_path.full_path(), m3u8_content).await?;
        log::info!(
            "[{}]M3U8 index file generated: {}",
            room_id,
            m3u8_index_file_path.full_path().display()
        );

        // Generate a tmp mp4 clip file first (reuse when a previous run already merged it).
        let clip_file_path = work_dir.with_filename("tmp.mp4");
        let reuse_tmp_mp4 = tokio::fs::metadata(&clip_file_path.full_path())
            .await
            .ok()
            .is_some_and(|meta| meta.is_file() && meta.len() > 1024 * 1024);
        if reuse_tmp_mp4 {
            log::info!(
                "[{}]Reusing existing temp clip for subtitle generation: {}",
                room_id,
                clip_file_path.full_path().display()
            );
            if let Some(reporter) = reporter {
                reporter.update("复用已合成的录播音频，跳过重新合并").await;
            }
        } else {
            if let Some(reporter) = reporter {
                reporter.update("正在准备整场录播音频...").await;
            }
            if let Err(e) = crate::ffmpeg::playlist::clip_from_playlist(
                None::<&crate::progress::progress_reporter::ProgressReporter>,
                Path::new(&m3u8_index_file_path.full_path()),
                Path::new(&clip_file_path.full_path()),
                None,
            )
            .await
            {
                return Err(RecorderManagerError::SubtitleGenerationFailed {
                    error: e.to_string(),
                });
            }
            log::info!("[{}]Temp clip file generated: {}", room_id, clip_file_path);
        }

        // Read config to determine generator type. For the local engine, prepare
        // the managed model automatically on first use so non-technical users do
        // not need to configure a filesystem path.
        let (generator_type, configured_model, config_path) = {
            let config = self.config.read().await;
            (
                config.subtitle_generator_type.clone(),
                config.whisper_model.clone(),
                config.config_path.clone(),
            )
        };
        if generator_type == "whisper" && !Path::new(&configured_model).is_file() {
            let model_path = crate::subtitle_generator::model_manager::ensure_model(
                &config_path,
                &configured_model,
            )
            .await
            .map_err(|error| RecorderManagerError::SubtitleGenerationFailed { error })?;
            let mut config = self.config.write().await;
            config.whisper_model = model_path.to_string_lossy().to_string();
            config.save();
        }

        let config = clone_lock_value(&self.config).await;
        let generator_type = config.subtitle_generator_type.as_str();

        // For third-party services (powerlive), extract opus audio from mp4
        let media_file_path = if generator_type == "powerlive" {
            let opus_file_path = work_dir.with_filename("tmp.opus");
            log::info!("[{}]Extracting opus audio for third-party service", room_id);

            // Extract opus audio using FFmpeg
            let ffmpeg_path = crate::ffmpeg::ffmpeg_path();
            let mut cmd = tokio::process::Command::new(ffmpeg_path);
            cmd.args([
                "-i",
                clip_file_path.full_path().to_str().unwrap(),
                "-vn", // no video
                "-acodec",
                "libopus",
                "-b:a",
                "128k",
                "-y",
                opus_file_path.full_path().to_str().unwrap(),
            ]);

            let output =
                cmd.output()
                    .await
                    .map_err(|e| RecorderManagerError::SubtitleGenerationFailed {
                        error: format!("Failed to run FFmpeg: {}", e),
                    })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(RecorderManagerError::SubtitleGenerationFailed {
                    error: format!("Failed to extract opus audio: {}", stderr),
                });
            }

            log::info!("[{}]Opus audio extracted: {}", room_id, opus_file_path);
            opus_file_path
        } else {
            // For whisper/whisper_online, use mp4 directly
            clip_file_path.clone()
        };

        // Long local recordings are split into stable 10-minute units before
        // Whisper processing. Each unit is persisted so an interrupted job can
        // resume without retranscribing the completed parts.
        let result = if config.subtitle_generator_type == "volcengine" {
            match crate::ffmpeg::generate_volcengine_video_subtitle(
                reporter,
                Path::new(&media_file_path.full_path()),
                &config.volcengine_api_key,
                &config.volcengine_app_id,
                &config.volcengine_access_token,
                &config.volcengine_resource_id,
                &config.volcengine_boosting_table_id,
                &config.volcengine_correct_table_id,
            )
            .await
            {
                Ok(result) => Ok(result),
                Err(error) => {
                    log::warn!("火山ASR失败，自动回退FunASR: {error}");
                    if let Some(reporter) = reporter {
                        reporter.update("火山ASR不可用，正在使用本地FunASR").await;
                    }
                    crate::ffmpeg::generate_video_subtitle(
                        reporter,
                        Path::new(&media_file_path.full_path()),
                        "funasr",
                        &config.whisper_model,
                        &config.whisper_prompt,
                        &config.openai_api_key,
                        &config.openai_api_endpoint,
                        &config.whisper_language,
                    )
                    .await
                }
            }
        } else if config.subtitle_generator_type == "whisper" {
            let chunk_dir = work_dir.full_path().join("transcript_chunks");
            tokio::fs::create_dir_all(&chunk_dir).await?;
            let chunk_pattern = chunk_dir.join("chunk_%04d.wav");
            let has_audio_chunks = std::fs::read_dir(&chunk_dir)
                .map(|entries| {
                    entries.filter_map(Result::ok).any(|entry| {
                        entry.path().extension().and_then(|ext| ext.to_str()) == Some("wav")
                    })
                })
                .unwrap_or(false);

            if !has_audio_chunks {
                let output = crate::ffmpeg::ffmpeg_command()
                    .args(["-i", clip_file_path.full_path().to_str().unwrap()])
                    .args(["-vn", "-ar", "16000", "-ac", "1"])
                    .args(["-c:a", "pcm_s16le"])
                    .args(["-f", "segment", "-segment_time", "600"])
                    .args(["-reset_timestamps", "1", "-y"])
                    .arg(&chunk_pattern)
                    .output()
                    .await
                    .map_err(|error| RecorderManagerError::SubtitleGenerationFailed {
                        error: format!("切分整场直播音频失败: {error}"),
                    })?;
                if !output.status.success() {
                    return Err(RecorderManagerError::SubtitleGenerationFailed {
                        error: format!(
                            "切分整场直播音频失败: {}",
                            String::from_utf8_lossy(&output.stderr)
                        ),
                    });
                }
            }

            let mut chunk_paths = std::fs::read_dir(&chunk_dir)
                .map_err(|error| RecorderManagerError::SubtitleGenerationFailed {
                    error: format!("读取转写分段失败: {error}"),
                })?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("wav"))
                .collect::<Vec<_>>();
            chunk_paths.sort();

            let mut full_result = GenerateResult {
                subtitle_id: String::new(),
                subtitle_content: vec![],
                generator_type: SubtitleGeneratorType::Whisper,
            };

            for (index, chunk_path) in chunk_paths.iter().enumerate() {
                if let Some(reporter) = reporter {
                    reporter
                        .update(&format!(
                            "正在转写第 {}/{} 段",
                            index + 1,
                            chunk_paths.len()
                        ))
                        .await;
                }
                let cached_srt = chunk_path.with_extension("srt");
                let chunk_result = if cached_srt.is_file() {
                    let content = std::fs::read_to_string(&cached_srt).map_err(|error| {
                        RecorderManagerError::SubtitleGenerationFailed {
                            error: format!("读取已完成分段字幕失败: {error}"),
                        }
                    })?;
                    GenerateResult {
                        subtitle_id: String::new(),
                        subtitle_content: srtparse::from_str(&content).map_err(|error| {
                            RecorderManagerError::SubtitleGenerationFailed {
                                error: format!("解析已完成分段字幕失败: {error}"),
                            }
                        })?,
                        generator_type: SubtitleGeneratorType::Whisper,
                    }
                } else {
                    let generated = crate::ffmpeg::generate_video_subtitle(
                        None,
                        chunk_path,
                        &config.subtitle_generator_type,
                        &config.whisper_model,
                        &config.whisper_prompt,
                        &config.openai_api_key,
                        &config.openai_api_endpoint,
                        &config.whisper_language,
                    )
                    .await
                    .map_err(|error| {
                        RecorderManagerError::SubtitleGenerationFailed {
                            error: format!(
                                "第 {}/{} 段转写失败: {error}",
                                index + 1,
                                chunk_paths.len()
                            ),
                        }
                    })?;
                    let content = generated
                        .subtitle_content
                        .iter()
                        .map(item_to_srt)
                        .collect::<String>();
                    tokio::fs::write(&cached_srt, content).await?;
                    generated
                };
                full_result.concat_with_offset_ms(&chunk_result, index as u64 * 600_000);
            }
            Ok(full_result)
        } else {
            crate::ffmpeg::generate_video_subtitle(
                None,
                Path::new(&media_file_path.full_path()),
                &config.subtitle_generator_type,
                &config.whisper_model,
                &config.whisper_prompt,
                &config.openai_api_key,
                &config.openai_api_endpoint,
                &config.whisper_language,
            )
            .await
        };
        // write subtitle file
        if let Err(e) = result {
            return Err(RecorderManagerError::SubtitleGenerationFailed {
                error: e.to_string(),
            });
        }
        log::info!("[{room_id}]Subtitle generated");
        let result = result.unwrap();
        let actual_generator = result.generator_type.as_str();
        let generated_subtitle_content = result
            .subtitle_content
            .iter()
            .map(item_to_srt)
            .collect::<String>();
        let subtitle_content = match tokio::fs::read_to_string(subtitle_file_path.full_path()).await
        {
            Ok(previous)
                if !previous.trim().is_empty()
                    && is_subtitle_coverage_regression(&previous, &generated_subtitle_content) =>
            {
                let truncated_candidate =
                    work_dir.with_filename("subtitle.truncated-candidate.srt");
                tokio::fs::write(
                    truncated_candidate.full_path(),
                    generated_subtitle_content.as_bytes(),
                )
                .await?;
                log::warn!(
                    "[{room_id}]Rejected truncated transcript generation: previous_end_ms={:?}, generated_end_ms={:?}",
                    subtitle_end_ms(&previous),
                    subtitle_end_ms(&generated_subtitle_content)
                );
                previous
            }
            _ => generated_subtitle_content,
        };
        // Do not truncate the existing transcript until ASR has completed.
        // This keeps the last known-good transcript intact when a provider fails.
        tokio::fs::write(subtitle_file_path.full_path(), subtitle_content.as_bytes()).await?;
        log::info!("[{room_id}]Subtitle file written");

        TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Archive {
                platform: platform.as_str().to_string(),
                room_id: room_id.to_string(),
                live_id: live_id.to_string(),
            },
            work_dir.full_path(),
            media_file_path.full_path(),
            &subtitle_content,
            actual_generator,
        )
        .await
        .map_err(|error| RecorderManagerError::SubtitleGenerationFailed {
            error: format!("保存规范逐字稿失败: {error}"),
        })?;

        // FunASR writes auditable side artifacts next to the temporary media.
        // Promote them to stable archive filenames before tmp.mp4 is removed.
        if actual_generator == "funasr" {
            for (source_name, target_name) in [
                ("tmp.asr.raw.txt", "transcript.raw.txt"),
                ("tmp.asr.raw.srt", "transcript.raw.srt"),
                ("tmp.asr.corrected.txt", "transcript.corrected.txt"),
                ("tmp.asr.corrected.srt", "transcript.corrected.srt"),
                ("tmp.asr.changes.json", "transcript.changes.json"),
                ("tmp.asr.review.json", "transcript.review.json"),
            ] {
                let source = work_dir.full_path().join(source_name);
                let target = work_dir.full_path().join(target_name);
                if source.is_file() {
                    let _ = tokio::fs::remove_file(&target).await;
                    tokio::fs::rename(&source, &target).await.map_err(|error| {
                        RecorderManagerError::SubtitleGenerationFailed {
                            error: format!("保存逐字稿审计文件 {target_name} 失败: {error}"),
                        }
                    })?;
                }
            }
        }
        if actual_generator == "volcengine" {
            let source = work_dir.full_path().join("tmp.mp4.asr.audit.json");
            let target = work_dir.full_path().join("transcript.audit.json");
            if source.is_file() {
                let _ = tokio::fs::remove_file(&target).await;
                tokio::fs::rename(&source, &target).await.map_err(|error| {
                    RecorderManagerError::SubtitleGenerationFailed {
                        error: format!("保存逐字稿审计文件 transcript.audit.json 失败: {error}"),
                    }
                })?;
            }
        }
        // remove tmp files
        tokio::fs::remove_file(&m3u8_index_file_path.full_path()).await?;

        // Remove both mp4 and opus files if they exist
        let clip_file_path = work_dir.with_filename("tmp.mp4");
        let _ = tokio::fs::remove_file(&clip_file_path.full_path()).await;

        let opus_file_path = work_dir.with_filename("tmp.opus");
        let _ = tokio::fs::remove_file(&opus_file_path.full_path()).await;

        log::info!("[{room_id}]Tmp files removed");
        Ok(subtitle_content)
    }

    pub async fn refresh_archive_subtitle(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
        reporter: Option<&ProgressReporter>,
    ) -> Result<ArchiveSubtitleRefreshResult, RecorderManagerError> {
        let existing = self
            .get_archive_subtitle(platform, room_id, live_id)
            .await
            .ok()
            .filter(|value| !value.trim().is_empty());
        let generated = self
            .generate_archive_subtitle(platform, room_id, live_id, reporter)
            .await?;

        let work_dir = self
            .resolve_archive_cache_path(platform, room_id, live_id)
            .await;
        let subtitle_path = work_dir.with_filename("subtitle.srt");
        let candidate_path = work_dir.with_filename("subtitle.refresh-candidate.srt");
        tokio::fs::write(candidate_path.full_path(), generated.as_bytes()).await?;

        let Some(previous) = existing else {
            return Ok(ArchiveSubtitleRefreshResult {
                subtitle: generated.clone(),
                decision: "created".to_string(),
                similarity: 0.0,
                old_length: 0,
                new_length: generated.len(),
            });
        };

        let similarity = transcript_bigram_similarity(&previous, &generated);
        let old_length = previous.len();
        let new_length = generated.len();
        if is_subtitle_coverage_regression(&previous, &generated) {
            log::warn!(
                "Rejected truncated archive transcript refresh: previous_end_ms={:?}, generated_end_ms={:?}",
                subtitle_end_ms(&previous),
                subtitle_end_ms(&generated)
            );
            tokio::fs::write(subtitle_path.full_path(), previous.as_bytes()).await?;
            Ok(ArchiveSubtitleRefreshResult {
                subtitle: previous,
                decision: "kept".to_string(),
                similarity,
                old_length,
                new_length,
            })
        } else if similarity >= 0.48 {
            tokio::fs::write(subtitle_path.full_path(), previous.as_bytes()).await?;
            Ok(ArchiveSubtitleRefreshResult {
                subtitle: previous,
                decision: "kept".to_string(),
                similarity,
                old_length,
                new_length,
            })
        } else {
            let previous_path = work_dir.with_filename("subtitle.previous.srt");
            tokio::fs::write(previous_path.full_path(), previous.as_bytes()).await?;
            Ok(ArchiveSubtitleRefreshResult {
                subtitle: generated,
                decision: "replaced".to_string(),
                similarity,
                old_length,
                new_length,
            })
        }
    }

    pub async fn delete_archive(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_id: &str,
    ) -> Result<RecordRow, RecorderManagerError> {
        if let Some(recorder) = self.get_recorder_info(platform, room_id).await {
            if recorder.recording && recorder.live_id == live_id {
                return Err(RecorderManagerError::Recording {
                    live_id: live_id.to_string(),
                });
            }
        }
        log::info!("Deleting archive {room_id}:{live_id}");
        let cache_folder = self.resolve_archive_dir(platform, room_id, live_id).await;
        let staged_cache = stage_archive_cache_dir(&cache_folder).await?;
        let index_result: Result<Option<RecordRow>, RecorderManagerError> = async {
            let deleted_task_count = self
                .db
                .delete_archive_tasks(platform.as_str(), room_id, live_id)
                .await?;
            if deleted_task_count > 0 {
                log::info!(
                    "Deleted {deleted_task_count} task(s) linked to archive {room_id}:{live_id}"
                );
            }
            self.db
                .try_remove_record(room_id, live_id)
                .await
                .map_err(RecorderManagerError::from)
        }
        .await;

        let to_delete = match index_result {
            Ok(value) => value,
            Err(error) => {
                if let Some(staged) = staged_cache.as_ref() {
                    if let Err(restore_error) = tokio::fs::rename(staged, &cache_folder).await {
                        log::error!(
                            "Failed to restore staged archive cache {} -> {}: {restore_error}",
                            staged.display(),
                            cache_folder.display()
                        );
                    }
                }
                return Err(error);
            }
        };

        if let Some(staged) = staged_cache {
            tokio::spawn(async move {
                if let Err(error) = remove_archive_cache_dir(&staged).await {
                    log::error!(
                        "Background archive cache cleanup failed for {}: {error}",
                        staged.display()
                    );
                }
            });
        }
        if let Some(to_delete) = to_delete {
            return Ok(to_delete);
        }
        log::warn!(
            "Archive cache removed but record index was already missing: {room_id}:{live_id}"
        );
        Ok(RecordRow {
            platform: platform.as_str().to_string(),
            parent_id: live_id.to_string(),
            live_id: live_id.to_string(),
            room_id: room_id.to_string(),
            title: String::new(),
            length: 0.0,
            size: 0,
            created_at: String::new(),
            cover: None,
            anchor_name: String::new(),
            anchor_source: String::new(),
            anchor_confidence: String::new(),
            anchor_detection_status: "not_requested".to_string(),
            anchor_detection_error: String::new(),
            anchor_detected_at: String::new(),
            archive_kind: "competitor".to_string(),
            classification_source: "auto_rule".to_string(),
            source_path: String::new(),
        })
    }

    pub async fn delete_archives(
        &self,
        platform: PlatformType,
        room_id: &str,
        live_ids: &[&str],
    ) -> Result<Vec<RecordRow>, RecorderManagerError> {
        log::info!("Deleting archives in batch: {live_ids:?}");
        let mut to_deletes = Vec::new();
        let mut failures = Vec::new();
        for live_id in live_ids {
            match self.delete_archive(platform, room_id, live_id).await {
                Ok(to_delete) => to_deletes.push(to_delete),
                Err(error) => failures.push(format!("{live_id}: {error}")),
            }
        }
        if to_deletes.is_empty() && !failures.is_empty() {
            return Err(RecorderManagerError::HLSError {
                err: failures.join("; "),
            });
        }
        if !failures.is_empty() {
            log::warn!(
                "Partial archive delete failure for {room_id}: {}",
                failures.join("; ")
            );
        }
        Ok(to_deletes)
    }

    pub async fn handle_hls_request(&self, uri: &str) -> Result<Vec<u8>, RecorderManagerError> {
        let path = uri.split('?').next().unwrap_or(uri);
        let params = uri.split('?').nth(1).unwrap_or("");
        let path_segs: Vec<&str> = path.split('/').collect();

        if path_segs.len() < 4 {
            log::warn!("Invalid request path: {path}");
            return Err(RecorderManagerError::HLSError {
                err: "Invalid hls path".into(),
            });
        }
        // parse recorder type
        let platform = path_segs[0];
        // parse room id
        let room_id = path_segs[1];
        // parse live id
        let live_id = path_segs[2];

        let params = Some(params);

        // parse params, example: start=10&end=20
        // start and end are optional
        // split params by &, and then split each param by =
        let params = if let Some(params) = params {
            let params = params
                .split('&')
                .map(|param| param.split('=').collect::<Vec<&str>>())
                .collect::<Vec<Vec<&str>>>();
            Some(params)
        } else {
            None
        };

        let start = if let Some(params) = &params {
            params
                .iter()
                .find(|param| param[0] == "start")
                .map_or(0, |param| param[1].parse::<i64>().unwrap())
        } else {
            0
        };
        let end = if let Some(params) = &params {
            params
                .iter()
                .find(|param| param[0] == "end")
                .map_or(0, |param| param[1].parse::<i64>().unwrap())
        } else {
            0
        };

        let platform = PlatformType::from_str(platform).map_err(|_| {
            RecorderManagerError::InvalidPlatformType {
                platform: platform.to_string(),
            }
        })?;

        let range = if start != 0 || end != 0 {
            Some(Range {
                start: start as f64,
                end: end as f64,
            })
        } else {
            None
        };

        // Check if this is a playlist request
        // The remaining path after platform/room_id/live_id could be:
        // - "playlist.m3u8" (4 segments total)
        // - "some_dir/playlist.m3u8" (5+ segments)
        // - "segment.ts" (4 segments total)
        // - "some_dir/segment.ts" (5+ segments)
        let remaining_path = path_segs[3..].join("/");

        if remaining_path == "playlist.m3u8" || remaining_path.ends_with("/playlist.m3u8") {
            let playlist = self.load_playlist(platform, room_id, live_id).await?;
            let playlist = self.playlist_range(&playlist, range).await?;
            let mut bytes: Vec<u8> = Vec::new();
            playlist.write_to(&mut bytes).unwrap();
            Ok(bytes)
        } else {
            // try to find requested ts file in recorder's cache
            // cache files are stored in {cache_dir}/{room_id}/{timestamp}/{ts_file}
            // remove path params
            let ts_file = self
                .resolve_archive_dir(platform, room_id, live_id)
                .await
                .join(remaining_path.replace("%7C", "|"));
            let ts_file_content = tokio::fs::read(&ts_file).await;
            if ts_file_content.is_err() {
                log::warn!("Segment file not found: {}", ts_file.display());
                return Err(RecorderManagerError::HLSError {
                    err: "Segment file not found".into(),
                });
            }

            Ok(ts_file_content.unwrap())
        }
    }

    pub async fn set_enable(&self, platform: PlatformType, room_id: &str, enabled: bool) {
        // update RecordRow auto_start field
        if let Err(e) = self.db.update_recorder(platform, room_id, enabled).await {
            log::error!("Failed to update recorder auto_start: {e}");
        }

        let recorder_id = format!("{}:{}", platform.as_str(), room_id);
        if let Some(recorder_ref) = self.recorders.read().await.get(&recorder_id) {
            if enabled {
                recorder_ref.enable().await;
            } else {
                recorder_ref.disable().await;
            }
        }
    }

    pub async fn generate_whole_clip(
        &self,
        reporter: Option<&ProgressReporter>,
        params: GenerateWholeClipParams,
    ) -> Result<(), RecorderManagerError> {
        let GenerateWholeClipParams {
            encode_danmu,
            platform,
            room_id,
            parent_id,
            selected_live_ids,
            output_name,
        } = params;

        let platform = PlatformType::from_str(&platform).map_err(|_| {
            RecorderManagerError::InvalidPlatformType {
                platform: platform.to_string(),
            }
        })?;

        let mut playlists = self
            .get_related_playlists(&platform, &room_id, &parent_id)
            .await;
        if playlists.is_empty() {
            log::error!("No related playlists found: {parent_id}");
            return Ok(());
        }

        if let Some(selected_live_ids) = selected_live_ids {
            let mut by_id = std::collections::HashMap::new();
            for playlist in playlists {
                by_id.insert(playlist.live_id.clone(), playlist);
            }
            let mut ordered = Vec::new();
            for live_id in selected_live_ids {
                if let Some(playlist) = by_id.remove(&live_id) {
                    ordered.push(playlist);
                }
            }
            playlists = ordered;
        }

        if playlists.is_empty() {
            log::error!("No selected playlists found: {parent_id}");
            return Ok(());
        }

        let title = playlists.first().unwrap().title.clone();

        // generate archive danmu ass file for all playlists
        let danmu_ass_files = if encode_danmu {
            let danmu_ass_files = playlists
                .iter()
                .map(async |p| {
                    (self
                        .generate_archive_danmu_ass(platform, &room_id, &p.live_id)
                        .await)
                        .ok()
                })
                .collect::<Vec<_>>();

            futures::future::join_all(danmu_ass_files).await
        } else {
            vec![None; playlists.len()]
        };

        let output_filename = if let Some(output_name) = output_name {
            let trimmed = output_name.trim();
            if trimmed.is_empty() {
                None
            } else {
                let mut sanitized = sanitize_filename::sanitize(trimmed);
                if !sanitized.to_lowercase().ends_with(".mp4") {
                    sanitized.push_str(".mp4");
                }
                Some(std::path::PathBuf::from(sanitized))
            }
        } else {
            None
        };

        let output_filename = if let Some(output_filename) = output_filename {
            output_filename
        } else {
            let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
            let sanitized_filename = sanitize_filename::sanitize(format!(
                "[full][{platform:?}][{room_id}][{parent_id}][{timestamp}]{title}.mp4"
            ));
            std::path::PathBuf::from(sanitized_filename)
        };

        let cover_filename = output_filename.with_extension("jpg");

        let output_path =
            Path::new(&self.config.read().await.output.as_str()).join(&output_filename);

        let playlists_refs: Vec<&Path> = playlists.iter().map(|p| p.path.as_path()).collect();

        log::info!("Concat playlists: {playlists_refs:?}");
        log::info!("Output path: {output_path:?}");

        if let Err(e) = crate::ffmpeg::playlist::concat_playlists_to_video(
            reporter,
            &playlists_refs,
            danmu_ass_files,
            &output_path,
        )
        .await
        {
            log::error!("Failed to concat playlists: {e}");
            return Err(RecorderManagerError::HLSError {
                err: "Failed to concat playlists".into(),
            });
        }

        let metadata = std::fs::metadata(&output_path);
        if metadata.is_err() {
            return Err(RecorderManagerError::HLSError {
                err: "Failed to get file metadata".into(),
            });
        }
        let size = metadata.unwrap().len() as i64;

        let video_metadata = crate::ffmpeg::extract_video_metadata(Path::new(&output_path)).await;
        let mut length = 0;
        if let Ok(video_metadata) = video_metadata {
            length = video_metadata.duration as i64;
        } else {
            log::error!(
                "Failed to get video metadata: {}",
                video_metadata.err().unwrap()
            );
        }

        let _ = crate::ffmpeg::generate_thumbnail(Path::new(&output_path), 0.0).await;
        let _ = crate::ffmpeg::extract_audio_sample(Path::new(&output_path)).await;

        let video = self
            .db
            .add_video(&VideoRow {
                id: 0,
                status: 0,
                room_id: room_id.to_string(),
                created_at: chrono::Local::now().to_rfc3339(),
                cover: cover_filename.to_string_lossy().to_string(),
                file: output_filename.to_string_lossy().to_string(),
                note: "".into(),
                length,
                size,
                bvid: String::new(),
                title: String::new(),
                desc: String::new(),
                tags: String::new(),
                area: 0,
                platform: platform.as_str().to_string(),
                anchor_name: String::new(),
                anchor_source: String::new(),
                anchor_confidence: String::new(),
                anchor_detection_status: "pending".to_string(),
                anchor_detection_error: String::new(),
                anchor_detected_at: String::new(),
            })
            .await?;

        if let Err(error) = self
            .nas_archive
            .enqueue(video.id, "recording", Path::new(&output_path))
            .await
        {
            log::error!("录制视频加入 NAS 转存队列失败：{error}");
        }

        let event =
            events::new_webhook_event(events::CLIP_GENERATED, events::Payload::Clip(video.clone()));
        if let Err(e) = self.webhook_poster.post_event(&event).await {
            log::error!("Post webhook event error: {e}");
        }

        Ok(())
    }
}
