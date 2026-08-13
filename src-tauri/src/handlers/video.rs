use crate::database::task::TaskRow;
use crate::database::transcript_dictionary_candidate::{
    TranscriptDictionaryCandidateRow, TranscriptDictionaryCandidateStatus,
    TranscriptDictionaryCandidateType,
};
use crate::database::video::VideoRow;
use crate::database::video_archive::VideoArchiveRow;
use crate::ffmpeg;
use crate::handlers::transcript_review::{
    database_video_media_path, resolve_canonical_video_source,
};
use crate::handlers::utils::get_disk_info_inner;
use crate::master_script::{
    parameter_card_from_record, start_master_ingest, TranscriptArtifactSink, VideoCheckpointStore,
    VolcengineChunkTranscriber,
};
use crate::progress::progress_reporter::{EventEmitter, ProgressReporter, ProgressReporterTrait};
use crate::recorder_manager::ClipRangeParams;
use crate::security::{audit_tool_failure, audit_tool_success, require_sensitive_write};
use crate::state::VideoPreviewSession;
use crate::subtitle_generator::item_to_srt;
use crate::subtitle_generator::transcript_artifacts::{
    ApprovedTranscriptReplacement, TranscriptArtifactStore, TranscriptSource,
};
use crate::task::{Task, TaskPriority};
use crate::webhook::events;
use base64::Engine;
use chrono::{Local, Utc};
use master_ingest::{
    parameter_card_exact_match, parse_srt_cues, select_parameter_cards, IngestRequest,
    IngestStatus, ParameterCard, MAX_PARAMETER_CARDS,
};
use recorder::platforms::bilibili;
use recorder::platforms::bilibili::profile::Profile;
use serde_json::json;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
#[cfg(windows)]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::UNIX_EPOCH;

#[cfg(feature = "gui")]
use tauri::{Emitter, Manager};
#[cfg(windows)]
use windows::{
    core::HSTRING,
    Win32::Foundation::{HWND, LPARAM, POINT, WPARAM},
    Win32::Graphics::Gdi::ClientToScreen,
    Win32::UI::WindowsAndMessaging::{
        FindWindowW, PostMessageW, SetWindowLongPtrW, SetWindowPos, GWLP_HWNDPARENT, HWND_TOP,
        SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_CLOSE,
    },
};

pub(crate) struct CanonicalVideoTranscriptContext {
    pub(crate) source: TranscriptSource,
    pub(crate) artifact_dir: PathBuf,
    pub(crate) media_file: PathBuf,
    pub(crate) video_id: i64,
}

pub(crate) async fn resolve_video_transcript_context(
    state: &State,
    requested_video_id: i64,
) -> Result<CanonicalVideoTranscriptContext, String> {
    let output = state.config.read().await.output.clone();
    let (source, artifact_dir) =
        resolve_canonical_video_source(&state.db, Path::new(&output), requested_video_id).await?;
    let TranscriptSource::Video { video_id } = source else {
        return Err("canonical video resolver returned a non-video source".to_string());
    };
    let video = state.db.get_video(video_id).await?;
    let media_file =
        database_video_media_path(&state.db, Path::new(&output), video_id, &video.file).await?;
    Ok(CanonicalVideoTranscriptContext {
        source: TranscriptSource::Video { video_id },
        artifact_dir,
        media_file,
        video_id,
    })
}

const LEGACY_TRANSCRIPT_MIN_COVERAGE_PERCENT: u64 = 95;

fn validate_video_transcript_coverage(
    subtitle: &str,
    video_duration_ms: u64,
) -> Result<(), String> {
    if subtitle.trim().is_empty() || video_duration_ms == 0 {
        return Ok(());
    }
    let last_end_ms = parse_srt_cues(subtitle)?
        .iter()
        .map(|cue| cue.end_ms)
        .max()
        .unwrap_or_default();
    if last_end_ms.saturating_mul(100)
        >= video_duration_ms.saturating_mul(LEGACY_TRANSCRIPT_MIN_COVERAGE_PERCENT)
    {
        return Ok(());
    }
    Err(format!(
        "逐字稿不完整：视频时长 {}，文稿仅到 {}。请点击“重新识别”，系统会按 10 分钟分段补齐整场文稿。",
        format_media_duration(video_duration_ms),
        format_media_duration(last_end_ms),
    ))
}

fn format_media_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

/// 检测路径是否为网络协议路径（排除Windows盘符）
fn is_network_protocol(path_str: &str) -> bool {
    // 常见的网络协议
    let network_protocols = [
        "ftp://", "sftp://", "ftps://", "http://", "https://", "smb://", "cifs://", "nfs://",
        "afp://", "ssh://", "scp://",
    ];

    // 检查是否以网络协议开头
    for protocol in &network_protocols {
        if path_str.to_lowercase().starts_with(protocol) {
            return true;
        }
    }

    // 排除Windows盘符格式 (如 C:/, D:/, E:/ 等)
    if cfg!(windows) {
        // 检查是否为Windows盘符格式：单字母 + : + /
        if path_str.len() >= 3 {
            let chars: Vec<char> = path_str.chars().collect();
            if chars.len() >= 3
                && chars[0].is_ascii_alphabetic()
                && chars[1] == ':'
                && (chars[2] == '/' || chars[2] == '\\')
            {
                return false; // 这是Windows盘符，不是网络路径
            }
        }
    }

    false
}

/// 判断是否需要转换为浏览器兼容的视频格式。
/// 直播常见的传输流容器和 FLV 无法保证 HTML5 播放器可用，必须重编码为 MP4/H.264。
fn should_convert_video_format(extension: &str) -> bool {
    matches!(
        extension.to_lowercase().as_str(),
        "flv" | "ts" | "m2ts" | "mts"
    )
}

/// 传输流即使改了 MP4 扩展名，视频流仍可能是 H.265，HTML5 播放器无法解码。
/// 这类来源不能使用无损流复制，必须输出 H.264/AAC。
fn should_force_h264_reencode(extension: &str) -> bool {
    matches!(extension.to_lowercase().as_str(), "ts" | "m2ts" | "mts")
}

fn is_browser_playable_video_codec(codec: &str) -> bool {
    matches!(
        codec.trim().to_lowercase().as_str(),
        "h264" | "avc" | "avc1"
    )
}

fn is_browser_playable_audio_codec(codec: &str) -> bool {
    matches!(
        codec.trim().to_lowercase().as_str(),
        "" | "aac" | "mp3" | "mp2"
    )
}

fn can_remux_for_browser_playback(metadata: &ffmpeg::VideoMetadata) -> bool {
    is_browser_playable_video_codec(&metadata.video_codec)
        && is_browser_playable_audio_codec(&metadata.audio_codec)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackConversionKind {
    FastRemux,
    Reencode,
}

fn playback_conversion_kind(metadata: &ffmpeg::VideoMetadata) -> PlaybackConversionKind {
    if can_remux_for_browser_playback(metadata) {
        PlaybackConversionKind::FastRemux
    } else {
        PlaybackConversionKind::Reencode
    }
}

async fn requires_browser_playback_copy_for_path(source_path: &Path, file_label: &str) -> bool {
    if requires_browser_playback_copy(file_label) {
        return true;
    }
    if !source_path.is_file() {
        return false;
    }
    let extension = Path::new(file_label)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if matches!(extension.to_lowercase().as_str(), "flv") {
        return true;
    }
    if !matches!(
        extension.to_lowercase().as_str(),
        "mp4" | "m4v" | "mov" | "mkv" | "webm"
    ) {
        return false;
    }
    if matches!(extension.to_lowercase().as_str(), "mp4" | "m4v" | "mov") {
        match ffmpeg::mp4_moov_at_end(source_path) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(error) => {
                log::warn!(
                    "Could not inspect MP4 layout for {}: {error}",
                    source_path.display()
                );
            }
        }
    }
    match ffmpeg::extract_video_metadata(source_path).await {
        Ok(metadata) => !is_browser_playable_video_codec(&metadata.video_codec),
        Err(error) => {
            log::warn!(
                "Could not probe playback codec for {}: {error}",
                source_path.display()
            );
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportConversionStrategy {
    CopyOnly,
    ContainerConversion,
}

fn import_conversion_strategy(extension: &str) -> ImportConversionStrategy {
    if should_force_h264_reencode(extension) {
        // Preserve long transport streams on import. ASR can read the original
        // file, while the analysis page creates a playable H.264 sidecar only
        // when someone actually needs to watch it.
        ImportConversionStrategy::CopyOnly
    } else if should_convert_video_format(extension) {
        ImportConversionStrategy::ContainerConversion
    } else {
        ImportConversionStrategy::CopyOnly
    }
}

fn playback_sidecar_file_name(file: &str) -> Result<String, String> {
    let stem = Path::new(file)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "无法为该视频创建播放副本文件名".to_string())?;
    Ok(format!("{stem}.playable.mp4"))
}

fn playback_hls_directory_name(file: &str) -> Result<String, String> {
    let stem = Path::new(file)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "无法为该视频创建内嵌播放索引目录".to_string())?;
    Ok(format!("{stem}.playback-hls"))
}

fn playback_hls_playlist_path(source_path: &Path, file: &str) -> Result<PathBuf, String> {
    Ok(source_path
        .with_file_name(playback_hls_directory_name(file)?)
        .join("index.m3u8"))
}

fn playback_file_label(
    original_file: &str,
    playback_path: &Path,
    output: &Path,
) -> Result<String, String> {
    if Path::new(original_file).is_absolute() {
        return Ok(playback_path.to_string_lossy().to_string());
    }
    playback_path
        .strip_prefix(output)
        .map_err(|error| format!("无法生成播放文件路径: {error}"))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn requires_browser_playback_copy(file: &str) -> bool {
    Path::new(file)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(should_force_h264_reencode)
}

fn is_playback_conversion_task(task: &TaskRow, video_id: i64) -> bool {
    if task.task_type != "prepare_video_playback" {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&task.metadata)
        .ok()
        .and_then(|metadata| metadata.get("video_id").and_then(serde_json::Value::as_i64))
        == Some(video_id)
}

fn is_active_playback_conversion_task(task: &TaskRow, video_id: i64) -> bool {
    is_playback_conversion_task(task, video_id)
        && matches!(task.status.as_str(), "pending" | "processing")
}

fn playback_copy_is_ready(sidecar_exists: bool, latest_task_status: Option<&str>) -> bool {
    // Only trust a sidecar after a successful conversion. Interrupted/cancelled/
    // failed runs may leave a partial `.playable.mp4` that the browser cannot
    // decode; treating those as ready makes "生成可播放版本" look like a no-op.
    sidecar_exists && matches!(latest_task_status, None | Some("success"))
}

fn playback_hls_is_ready(playlist_path: &Path, latest_task_status: Option<&str>) -> bool {
    playlist_path.is_file() && matches!(latest_task_status, None | Some("success"))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPlaybackSource {
    pub file: String,
    pub requires_preparation: bool,
    pub ready: bool,
    pub preparing: bool,
    pub message: String,
}

/// A short, browser-compatible HLS window generated from a raw recording.
/// The original TS stays untouched: this is only a disposable playback cache
/// beginning at the requested transcript/order timestamp.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPlaybackPreview {
    pub file: String,
    pub start_offset: f64,
    pub message: String,
}

async fn stop_video_playback_preview_inner(state: &State, id: i64, remove_cache: bool) {
    let session = state.video_preview_sessions.lock().await.remove(&id);
    if let Some(mut session) = session {
        let _ = session.child.kill().await;
        let _ = session.child.wait().await;
        if remove_cache {
            let _ = tokio::fs::remove_dir_all(session.cache_dir).await;
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn stop_video_playback_preview(state: state_type!(), id: i64) -> Result<(), String> {
    let _prepare_guard = state.video_preview_prepare_gate.lock().await;
    stop_video_playback_preview_inner(&state, id, true).await;
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn prepare_video_playback_preview(
    state: state_type!(),
    id: i64,
    offset_sec: f64,
) -> Result<VideoPlaybackPreview, String> {
    let video = state.db.get_video(id).await?;
    let output = state.config.read().await.output.clone();
    let source_path =
        database_video_media_path(&state.db, Path::new(&output), id, &video.file).await?;
    if !source_path.is_file() {
        return Err(format!(
            "原始视频文件不存在，无法打开内嵌预览：{}",
            source_path.display()
        ));
    }
    let start_offset = offset_sec.max(0.0);
    // Svelte initialization and reactive source setup can request the same
    // preview concurrently. Serialize this entire readiness transaction so
    // only one request owns an FFmpeg child and cache directory.
    let _prepare_guard = state.video_preview_prepare_gate.lock().await;
    let reusable_cache = {
        let sessions = state.video_preview_sessions.lock().await;
        sessions.get(&id).and_then(|session| {
            let playlist = session.cache_dir.join("index.m3u8");
            let has_segment = std::fs::read_dir(&session.cache_dir)
                .ok()
                .is_some_and(|entries| {
                    entries.flatten().any(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                    })
                });
            ((session.start_offset - start_offset).abs() < 0.25
                && playlist.is_file()
                && has_segment)
                .then_some(playlist)
        })
    };
    if let Some(playlist) = reusable_cache {
        let relative_playlist = playlist
            .strip_prefix(Path::new(&output))
            .unwrap_or(&playlist)
            .to_string_lossy()
            .replace('\\', "/");
        return Ok(VideoPlaybackPreview {
            file: relative_playlist,
            start_offset,
            message: "原始 TS 内嵌预览已就绪".to_string(),
        });
    }
    // A seek supersedes the previous preview. Stop the real child process
    // before replacing its files so stale encoders never accumulate.
    stop_video_playback_preview_inner(&state, id, true).await;
    let output_root = PathBuf::from(&output);
    let preview_root = output_root.join(".playback-preview");
    // A force-killed app cannot run the normal shutdown cleanup. Remove only
    // disposable preview directories that are not owned by a live session.
    let active_cache_dirs = {
        let sessions = state.video_preview_sessions.lock().await;
        sessions
            .values()
            .map(|session| session.cache_dir.clone())
            .collect::<Vec<_>>()
    };
    if let Ok(mut entries) = tokio::fs::read_dir(&preview_root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let is_preview_dir = entry.file_name().to_string_lossy().starts_with("video-")
                && entry.file_type().await.is_ok_and(|kind| kind.is_dir());
            if is_preview_dir && !active_cache_dirs.contains(&path) {
                let _ = tokio::fs::remove_dir_all(path).await;
            }
        }
    }
    let cache_dir = preview_root.join(format!("video-{id}-{}", Utc::now().timestamp_millis()));
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|error| format!("无法创建内嵌播放缓存目录：{error}"))?;
    let playlist = cache_dir.join("index.m3u8");
    let segments = cache_dir.join("segment-%05d.ts");

    let encoder = ffmpeg::hwaccel::get_x264_encoder().await;
    let mut command = tokio::process::Command::new(ffmpeg::ffmpeg_path());
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    command.args([
        "-ss",
        &start_offset.to_string(),
        "-fflags",
        "+genpts+discardcorrupt",
        "-err_detect",
        "ignore_err",
        "-i",
        &source_path.to_string_lossy(),
        // A small presentation starts in seconds, not after a full recording
        // conversion. New transcript/order seeks replace this session.
        "-t",
        "300",
        "-map",
        "0:v:0",
        "-map",
        "0:a:0?",
    ]);
    ffmpeg::hwaccel::apply_x264_encoder_only(&mut command, encoder);
    if encoder == "h264_amf" {
        command.args(["-quality", "speed"]);
    } else {
        ffmpeg::hwaccel::apply_x264_quality_args(&mut command, encoder);
    }
    command.args([
        "-c:a",
        "aac",
        "-force_key_frames",
        "expr:gte(t,n_forced*2)",
        "-f",
        "hls",
        "-hls_time",
        "2",
        "-hls_list_size",
        "0",
        "-hls_segment_type",
        "mpegts",
        "-hls_segment_filename",
        &segments.to_string_lossy(),
        "-y",
        &playlist.to_string_lossy(),
    ]);
    command
        .kill_on_drop(true)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|error| format!("无法启动内嵌播放流：{error}"))?;
    state.video_preview_sessions.lock().await.insert(
        id,
        VideoPreviewSession {
            child,
            cache_dir: cache_dir.clone(),
            start_offset,
        },
    );

    // The playlist is written as soon as FFmpeg closes the first two-second
    // segment. Wait only for that readiness point, never for the full preview.
    for _ in 0..75 {
        if playlist.is_file()
            && std::fs::read_dir(&cache_dir).ok().is_some_and(|entries| {
                entries.flatten().any(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                })
            })
        {
            let relative_playlist = playlist
                .strip_prefix(&output_root)
                .unwrap_or(&playlist)
                .to_string_lossy()
                .replace('\\', "/");
            return Ok(VideoPlaybackPreview {
                file: relative_playlist,
                start_offset,
                message: "原始 TS 内嵌预览已就绪".to_string(),
            });
        }
        let exited = {
            let mut sessions = state.video_preview_sessions.lock().await;
            let Some(session) = sessions.get_mut(&id) else {
                return Err("播放定位已更新。".to_string());
            };
            if session.cache_dir != cache_dir {
                return Err("播放定位已更新。".to_string());
            }
            session
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
        };
        if let Some(status) = exited {
            stop_video_playback_preview_inner(&state, id, true).await;
            return Err(format!("内嵌播放流提前退出：{status}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    stop_video_playback_preview_inner(&state, id, true).await;
    Err("内嵌播放流未能生成首个片段，请重试。".to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePlayerBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[cfg(windows)]
fn native_player_title(video_id: i64) -> String {
    format!("BSR Native Player {video_id}")
}

#[cfg(windows)]
static NATIVE_PLAYER_GUARD: Mutex<()> = Mutex::new(());

#[cfg(windows)]
static NATIVE_PLAYER_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
fn close_native_player_window(video_id: i64) {
    let title = HSTRING::from(native_player_title(video_id));
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, &title) {
            if !hwnd.is_invalid() {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// FFplay owns an independent SDL window. WM_CLOSE is asynchronous, so wait
/// until it is actually gone before reusing its title for a seek/restart.
/// Without this barrier, two SDL windows can briefly coexist and paint over
/// each other when the user seeks or toggles immersive mode.
#[cfg(windows)]
fn close_native_player_window_and_wait(video_id: i64) {
    close_native_player_window(video_id);
    let title = HSTRING::from(native_player_title(video_id));
    for _ in 0..75 {
        let is_open = unsafe {
            FindWindowW(None, &title)
                .map(|hwnd| !hwnd.is_invalid())
                .unwrap_or(false)
        };
        if !is_open {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        close_native_player_window(video_id);
    }
}

/// Starts the bundled FFplay above the analysis player rectangle. It reads the
/// original TS directly and avoids placing a Win32 child behind the WebView.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn start_native_video_playback(
    state: state_type!(),
    id: i64,
    offset_sec: Option<f64>,
    bounds: NativePlayerBounds,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (state, id, offset_sec, bounds);
        return Err("原生 TS 播放器目前仅支持 Windows 桌面端。".to_string());
    }
    #[cfg(windows)]
    {
        let video = state.db.get_video(id).await?;
        let output = state.config.read().await.output.clone();
        let source =
            database_video_media_path(&state.db, Path::new(&output), id, &video.file).await?;
        if !source.is_file() {
            return Err(format!("原始视频文件不存在：{}", source.display()));
        }
        // Native seek/expand operations restart FFplay. Serialize the entire
        // close-and-spawn sequence so a second request cannot create another
        // SDL window before the previous one has exited. This lock is taken
        // only after all async database/path work has completed.
        let _guard = NATIVE_PLAYER_GUARD
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let generation = NATIVE_PLAYER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        close_native_player_window_and_wait(id);
        let title = native_player_title(id);
        let ffplay = ffmpeg::ffmpeg_path().with_file_name("ffplay.exe");
        if !ffplay.is_file() {
            return Err("未找到项目内置原生播放器 ffplay.exe。".to_string());
        }

        // Frontend sends CSS-pixel bounds relative to the webview viewport.
        // Map them through the client origin so we do not include the title bar
        // (the previous GetWindowRect offset left a black host with ffplay elsewhere).
        let window = state
            .app_handle
            .get_webview_window("main")
            .ok_or_else(|| "找不到主窗口。".to_string())?;
        let scale = window.scale_factor().unwrap_or(1.0);
        let parent = window
            .hwnd()
            .map_err(|error| format!("无法获取主窗口句柄：{error}"))?;
        let width = ((bounds.width as f64) * scale).round().max(1.0) as i32;
        let height = ((bounds.height as f64) * scale).round().max(1.0) as i32;
        let child_x = ((bounds.x as f64) * scale).round() as i32;
        let child_y = ((bounds.y as f64) * scale).round() as i32;
        let (screen_x, screen_y) = unsafe {
            let mut origin = POINT {
                x: ((bounds.x as f64) * scale).round() as i32,
                y: ((bounds.y as f64) * scale).round() as i32,
            };
            if !ClientToScreen(parent, &mut origin).as_bool() {
                return Err("无法换算播放器屏幕坐标。".to_string());
            }
            (origin.x, origin.y)
        };

        let source_arg = source.to_string_lossy().to_string();
        let mut command = std::process::Command::new(ffplay);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-noborder",
            "-window_title",
            &title,
            "-left",
            &screen_x.to_string(),
            "-top",
            &screen_y.to_string(),
            "-x",
            &width.to_string(),
            "-y",
            &height.to_string(),
        ]);
        if let Some(offset) = offset_sec.filter(|value| *value > 0.0) {
            command.args(["-ss", &format!("{offset:.3}")]);
        }
        command.arg(source_arg);
        // Do not use CREATE_NO_WINDOW here: ffplay is a GUI process and that
        // flag can prevent the SDL window from becoming visible on some setups.
        command
            .spawn()
            .map_err(|error| format!("无法启动原生播放器：{error}"))?;

        let parent_handle = parent.0 as isize;
        let title_for_thread = title.clone();
        std::thread::spawn(move || {
            let parent = HWND(parent_handle as _);
            let title = HSTRING::from(title_for_thread);
            for _ in 0..50 {
                if NATIVE_PLAYER_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                unsafe {
                    let Ok(hwnd) = FindWindowW(None, &title) else {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    };
                    if !hwnd.is_invalid() {
                        // WebView2 is composited above child HWNDs, so a decoder
                        // reparented as a child is always black. Keep SDL as an
                        // owned borderless window instead: it renders above the
                        // WebView while Windows hides it with the app on minimize.
                        let _ = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, parent.0 as isize);
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOP),
                            screen_x,
                            screen_y,
                            width,
                            height,
                            SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                        // Keep the owned renderer pinned to the player rectangle.
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if NATIVE_PLAYER_GENERATION.load(Ordering::SeqCst) == generation {
                log::warn!(
                    "Native player window did not appear in time (expected at {screen_x},{screen_y} {width}x{height})"
                );
            }
        });
        Ok(())
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub fn stop_native_video_playback(id: i64) -> Result<(), String> {
    #[cfg(windows)]
    {
        let _guard = NATIVE_PLAYER_GUARD
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        NATIVE_PLAYER_GENERATION.fetch_add(1, Ordering::SeqCst);
        close_native_player_window_and_wait(id);
    }
    #[cfg(not(windows))]
    let _ = id;
    Ok(())
}

/// Reposition the existing SDL window after an in-app layout change. Unlike a
/// seek, this must never restart FFplay: restarting is what caused delayed
/// first frames and duplicate windows during immersive-mode toggles.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resize_native_video_playback(
    state: state_type!(),
    id: i64,
    bounds: NativePlayerBounds,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (state, id, bounds);
        return Ok(());
    }
    #[cfg(windows)]
    {
        let window = state
            .app_handle
            .get_webview_window("main")
            .ok_or_else(|| "找不到主窗口。".to_string())?;
        let scale = window.scale_factor().unwrap_or(1.0);
        let parent = window
            .hwnd()
            .map_err(|error| format!("无法获取主窗口句柄：{error}"))?;
        let width = ((bounds.width as f64) * scale).round().max(1.0) as i32;
        let height = ((bounds.height as f64) * scale).round().max(1.0) as i32;
        let child_x = ((bounds.x as f64) * scale).round() as i32;
        let child_y = ((bounds.y as f64) * scale).round() as i32;
        let (screen_x, screen_y) = unsafe {
            let mut origin = POINT {
                x: ((bounds.x as f64) * scale).round() as i32,
                y: ((bounds.y as f64) * scale).round() as i32,
            };
            if !ClientToScreen(parent, &mut origin).as_bool() {
                return Err("无法换算播放器屏幕坐标。".to_string());
            }
            (origin.x, origin.y)
        };
        let title = HSTRING::from(native_player_title(id));
        unsafe {
            if let Ok(hwnd) = FindWindowW(None, &title) {
                if !hwnd.is_invalid() {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOP),
                        screen_x,
                        screen_y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
        Ok(())
    }
}

/// 获取视频的最佳缩略图截取时间点
/// 根据视频长度选择最佳时间点，避开开头可能的黑屏
fn get_optimal_thumbnail_timestamp(duration: f64) -> f64 {
    // 根据视频长度选择最佳时间点
    if duration <= 10.0 {
        // 短视频（10秒以内）：选择1/3位置，避免开头黑屏
        duration / 3.0
    } else if duration <= 60.0 {
        // 1分钟以内：选择第3秒
        3.0
    } else if duration <= 300.0 {
        // 5分钟以内：选择第5秒
        5.0
    } else {
        // 长视频：选择第10秒，确保跳过开头可能的黑屏/logo
        10.0
    }
}

use crate::state::State;
use crate::state_type;

// 带进度的文件复制函数
async fn copy_file_with_progress(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    let mut source_file = File::open(source).map_err(|e| format!("无法打开源文件: {e}"))?;
    let mut dest_file = File::create(dest).map_err(|e| format!("无法创建目标文件: {e}"))?;

    let total_size = source_file
        .metadata()
        .map_err(|e| format!("无法获取文件大小: {e}"))?
        .len();
    let mut copied = 0u64;

    // 使用固定的小缓冲区避免大文件时的内存占用
    let buffer_size = 64 * 1024; // 64KB buffer for all files

    let mut buffer = vec![0u8; buffer_size];

    let mut last_reported_percent = 0;

    loop {
        let bytes_read = source_file
            .read(&mut buffer)
            .map_err(|e| format!("读取文件失败: {e}"))?;
        if bytes_read == 0 {
            break;
        }

        dest_file
            .write_all(&buffer[..bytes_read])
            .map_err(|e| format!("写入文件失败: {e}"))?;
        copied += bytes_read as u64;

        // 计算进度百分比，只在变化时更新
        let percent = if total_size > 0 {
            ((copied as f64 / total_size as f64) * 100.0) as u32
        } else {
            0
        };

        // 使用固定的进度报告频率
        let report_threshold = 1; // 每1%报告一次

        if percent != last_reported_percent && (percent % report_threshold == 0 || percent == 100) {
            reporter
                .update(&format!("正在复制视频文件... {percent}%"))
                .await;
            last_reported_percent = percent;
        }
    }

    dest_file
        .flush()
        .map_err(|e| format!("刷新文件缓冲区失败: {e}"))?;
    Ok(())
}

// 智能边拷贝边转换函数（针对网络文件优化）
async fn copy_and_convert_with_progress(
    source: &Path,
    dest: &Path,
    need_conversion: bool,
    force_h264_reencode: bool,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    if !need_conversion {
        // 非转换文件直接使用原有拷贝逻辑
        return copy_file_with_progress(source, dest, reporter).await;
    }

    // 检查源文件是否在网络位置（启发式判断）
    let source_str = source.to_string_lossy();
    let is_network_source = source_str.starts_with("\\\\") ||  // UNC path (Windows网络共享)
                           is_network_protocol(&source_str); // 网络协议但排除Windows盘符

    if is_network_source {
        // 网络文件：先复制到本地临时位置，再转换
        reporter
            .update("检测到网络文件，使用先复制后转换策略...")
            .await;
        copy_then_convert_strategy(source, dest, force_h264_reencode, reporter).await
    } else {
        // 本地文件：直接转换（更高效）
        reporter.update("检测到本地文件，使用直接转换策略...").await;
        if force_h264_reencode {
            ffmpeg::try_browser_compatible_conversion(source, dest, reporter).await
        } else {
            ffmpeg::convert_video_format(source, dest, reporter).await
        }
    }
}

// 网络文件处理策略：先复制到本地临时位置，再转换
async fn copy_then_convert_strategy(
    source: &Path,
    dest: &Path,
    force_h264_reencode: bool,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    // 创建临时文件路径
    let temp_dir = std::env::temp_dir();
    let temp_filename = format!(
        "temp_video_{}.{}",
        chrono::Utc::now().timestamp(),
        source.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    );
    let temp_path = temp_dir.join(&temp_filename);

    // 确保临时目录存在
    if let Some(parent) = temp_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建临时目录失败: {e}"))?;
    }

    // 第一步：将网络文件复制到本地临时位置（使用优化的缓冲区）
    reporter
        .update("第1步：从网络复制文件到本地临时位置...")
        .await;
    copy_file_with_network_optimization(source, &temp_path, reporter).await?;

    // 第二步：从本地临时文件转换到目标位置
    reporter.update("第2步：从临时文件转换到目标格式...").await;
    let convert_result = if force_h264_reencode {
        ffmpeg::try_browser_compatible_conversion(&temp_path, dest, reporter).await
    } else {
        ffmpeg::convert_video_format(&temp_path, dest, reporter).await
    };

    // 清理临时文件
    if temp_path.exists() {
        if let Err(e) = std::fs::remove_file(&temp_path) {
            log::warn!("删除临时文件失败: {} - {}", temp_path.display(), e);
        } else {
            log::info!("已清理临时文件: {}", temp_path.display());
        }
    }

    convert_result
}

// 针对网络文件优化的复制函数
async fn copy_file_with_network_optimization(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    let mut source_file = File::open(source).map_err(|e| format!("无法打开网络源文件: {e}"))?;
    let mut dest_file = File::create(dest).map_err(|e| format!("无法创建本地临时文件: {e}"))?;

    let total_size = source_file
        .metadata()
        .map_err(|e| format!("无法获取文件大小: {e}"))?
        .len();
    let mut copied = 0u64;

    // 使用固定的小缓冲区，避免大文件时内存占用过多
    let buffer_size = 64 * 1024; // 64KB buffer for network files

    let mut buffer = vec![0u8; buffer_size];
    let mut last_reported_percent = 0;
    let mut consecutive_errors = 0;
    const MAX_RETRIES: u32 = 3;

    loop {
        match source_file.read(&mut buffer) {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    break; // 文件读取完成
                }

                // 重置错误计数
                consecutive_errors = 0;

                dest_file
                    .write_all(&buffer[..bytes_read])
                    .map_err(|e| format!("写入临时文件失败: {e}"))?;
                copied += bytes_read as u64;

                // 计算并报告进度
                let percent = if total_size > 0 {
                    ((copied as f64 / total_size as f64) * 100.0) as u32
                } else {
                    0
                };

                // 网络文件更频繁地报告进度
                if percent != last_reported_percent {
                    reporter
                        .update(&format!(
                            "正在从网络复制文件... {}% ({:.1}MB/{:.1}MB)",
                            percent,
                            copied as f64 / (1024.0 * 1024.0),
                            total_size as f64 / (1024.0 * 1024.0)
                        ))
                        .await;
                    last_reported_percent = percent;
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                log::warn!("网络读取错误 (尝试 {consecutive_errors}/{MAX_RETRIES}): {e}");

                if consecutive_errors >= MAX_RETRIES {
                    return Err(format!("网络文件读取失败，已重试{MAX_RETRIES}次: {e}"));
                }

                // 等待一小段时间后重试
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                reporter
                    .update(&format!(
                        "网络连接中断，正在重试... ({consecutive_errors}/{MAX_RETRIES})"
                    ))
                    .await;
            }
        }
    }

    dest_file
        .flush()
        .map_err(|e| format!("刷新临时文件缓冲区失败: {e}"))?;
    reporter.update("网络文件复制完成").await;
    Ok(())
}

#[cfg(feature = "gui")]
use {tauri::State as TauriState, tauri_plugin_notification::NotificationExt};

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn clip_range(
    state: state_type!(),
    event_id: String,
    params: ClipRangeParams,
) -> Result<VideoRow, String> {
    // check storage space, preserve 1GB for other usage
    let output = state.config.read().await.output.clone();
    let mut output = PathBuf::from(&output);
    if output.is_relative() {
        // get current working directory
        let cwd = std::env::current_dir().unwrap();
        output = cwd.join(output);
    }

    if let Ok(disk_info) = get_disk_info_inner(output).await {
        // if free space is less than 1GB, return error
        if disk_info.free < 1024 * 1024 * 1024 {
            return Err("Storage space is not enough, clip canceled".to_string());
        }
    }

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    let mut params_without_cover = params.clone();
    params_without_cover.cover = String::new();
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "clip_range".to_string(),
        status: "pending".to_string(),
        message: String::new(),
        metadata: json!({
            "params": params_without_cover,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };

    state.db.add_task(&task).await?;
    log::info!("Create task: {} {}", task.id, task.task_type);

    let (result_tx, result_rx) = tokio::sync::oneshot::channel();

    #[cfg(feature = "gui")]
    let state_clone = (*state).clone();
    #[cfg(feature = "headless")]
    let state_clone = state.clone();

    let task_id = event_id.clone();
    state
        .task_manager
        .add_task(Task::new(
            task_id.clone(),
            TaskPriority::Normal,
            async move {
                let result = match clip_range_inner(&state_clone, &reporter, params).await {
                    Ok(video) => {
                        reporter.finish(true, "切片完成").await;
                        let _ = state_clone
                            .db
                            .update_task(&task_id, "success", "切片完成", None)
                            .await;

                        if state_clone.config.read().await.auto_subtitle {
                            let subtitle_event_id = format!("{task_id}_subtitle");
                            let result = generate_video_subtitle_inner(
                                &state_clone,
                                subtitle_event_id,
                                video.id,
                            )
                            .await;
                            if let Ok(subtitle) = result {
                                let result =
                                    update_video_subtitle_inner(&state_clone, video.id, subtitle)
                                        .await;
                                if let Err(e) = result {
                                    log::error!("Update video subtitle error: {e}");
                                }
                            } else {
                                log::error!(
                                    "Generate video subtitle error: {}",
                                    result.err().unwrap()
                                );
                            }
                        }

                        let event = events::new_webhook_event(
                            events::CLIP_GENERATED,
                            events::Payload::Clip(video.clone()),
                        );

                        if let Err(e) = state_clone.webhook_poster.post_event(&event).await {
                            log::error!("Post webhook event error: {e}");
                        }

                        Ok(video)
                    }
                    Err(e) => {
                        reporter.finish(false, &format!("切片失败: {e}")).await;
                        let _ = state_clone
                            .db
                            .update_task(&task_id, "failed", &format!("切片失败: {e}"), None)
                            .await;
                        Err(e)
                    }
                };

                let task_result = result.as_ref().map(|_| ()).map_err(Clone::clone);
                let _ = result_tx.send(result);
                task_result
            },
        ))
        .await?;

    result_rx
        .await
        .unwrap_or_else(|_| Err("切片任务失败".to_string()))
}

async fn clip_range_inner(
    state: &State,
    reporter: &ProgressReporter,
    params: ClipRangeParams,
) -> Result<VideoRow, String> {
    log::info!(
        "[{}]Clip room_id: {}, ts: {}, ranges: {:?}",
        reporter.event_id,
        params.room_id,
        params.live_id,
        params.ranges,
    );

    let clip_file = state.config.read().await.generate_clip_name(&params);

    let file = state
        .recorder_manager
        .clip_range(Some(reporter), clip_file, &params)
        .await?;
    log::info!("Clip range done, doing post processing");
    // get file metadata from fs
    let metadata = std::fs::metadata(&file).map_err(|e| {
        log::error!("Get file metadata error: {} {}", e, file.display());
        e.to_string()
    })?;
    let mut cover_generate_ffmpeg = true;
    let cover_file = file.with_extension("jpg");
    if !params.cover.is_empty() {
        if let Some(base64) = params.cover.split("base64,").nth(1) {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(base64) {
                // write cover file to fs
                tokio::fs::write(&cover_file, bytes).await.map_err(|e| {
                    log::error!("Write cover file error: {} {}", e, cover_file.display());
                    e.to_string()
                })?;
                cover_generate_ffmpeg = false;
            } else {
                log::error!("Decode base64 error: {}", params.cover);
            }
        } else {
            log::error!("Invalid cover base64: {}", params.cover);
        }
    }
    // generate cover file from video as fallback
    if cover_generate_ffmpeg {
        ffmpeg::generate_thumbnail(&file, 0.0).await?;
    }
    let _ = crate::ffmpeg::extract_audio_sample(&file).await?;
    // get filename from path
    let filename = Path::new(&file)
        .file_name()
        .ok_or("Invalid file path")?
        .to_str()
        .ok_or("Invalid file path")?;
    // add video to db
    let Ok(size) = i64::try_from(metadata.len()) else {
        log::error!(
            "Failed to convert metadata length to i64: {}",
            metadata.len()
        );
        return Err("Failed to convert metadata length to i64".to_string());
    };
    let duration = params.ranges.iter().map(|r| r.duration()).sum::<f64>();
    let video = state
        .db
        .add_video(&VideoRow {
            id: 0,
            status: 0,
            room_id: params.room_id.clone(),
            created_at: Local::now().to_rfc3339(),
            cover: cover_file
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            file: filename.into(),
            note: params.note.clone(),
            length: duration as i64,
            size,
            bvid: String::new(),
            title: String::new(),
            desc: String::new(),
            tags: String::new(),
            area: 0,
            platform: params.platform.clone(),
            anchor_name: String::new(),
            anchor_source: String::new(),
            anchor_confidence: String::new(),
            anchor_detection_status: "pending".to_string(),
            anchor_detection_error: String::new(),
            anchor_detected_at: String::new(),
        })
        .await?;
    state
        .db
        .new_message(
            "生成新切片",
            &format!(
                "生成了房间 {} 的切片，长度 {}s：{}",
                &params.room_id, duration, filename
            ),
        )
        .await?;
    if state.config.read().await.clip_notify {
        #[cfg(feature = "gui")]
        state
            .app_handle
            .notification()
            .builder()
            .title("BiliShadowReplay - 切片完成")
            .body(format!(
                "生成了房间 {} 的切片: {}",
                &params.room_id, filename
            ))
            .show()
            .unwrap();
    }

    reporter.finish(true, "切片完成").await;

    Ok(video)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn upload_procedure(
    state: state_type!(),
    event_id: String,
    uid: String,
    room_id: String,
    video_id: i64,
    profile: Profile,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<String, String> {
    let audit = require_sensitive_write(
        "post_video_to_bilibili",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("upload:bilibili:{uid}:{room_id}:{video_id}:{event_id}"),
    )?;
    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "upload_procedure".to_string(),
        status: "pending".to_string(),
        message: String::new(),
        metadata: json!({
            "uid": uid,
            "room_id": room_id,
            "video_id": video_id,
            "profile": profile,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_task(&task).await?;
    log::info!("Create task: {task:?}");
    match upload_procedure_inner(&state, &reporter, uid, room_id, video_id, profile).await {
        Ok(bvid) => {
            reporter.finish(true, "投稿成功").await;
            state
                .db
                .update_task(&event_id, "success", "投稿成功", None)
                .await?;
            audit_tool_success(&audit);
            Ok(bvid)
        }
        Err(e) => {
            reporter.finish(false, &format!("投稿失败: {e}")).await;
            state
                .db
                .update_task(&event_id, "failed", &format!("投稿失败: {e}"), None)
                .await?;
            audit_tool_failure(&audit, &e);
            Err(e)
        }
    }
}

async fn upload_procedure_inner(
    state: &state_type!(),
    reporter: &ProgressReporter,
    uid: String,
    room_id: String,
    video_id: i64,
    mut profile: Profile,
) -> Result<String, String> {
    let account = state.db.get_account("bilibili", &uid).await?;
    // get video info from dbs
    let mut video_row = state.db.get_video(video_id).await?;
    // construct file path
    let output = state.config.read().await.output.clone();
    let file = Path::new(&output).join(&video_row.file);
    let path = Path::new(&file);
    let client = reqwest::Client::new();

    let cover_path = file.with_extension("jpg");
    let cover_bytes = tokio::fs::read(&cover_path).await.map_err(|e| {
        log::error!("Read cover file error: {} {}", e, cover_path.display());
        e.to_string()
    })?;
    let cover_base64 = format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(cover_bytes)
    );
    let cover_url =
        bilibili::api::upload_cover(&client, &account.to_account(), &cover_base64).await;

    reporter.update("投稿预处理中").await;

    match bilibili::api::prepare_video(&client, &account.to_account(), path).await {
        Ok(video) => {
            profile.cover = cover_url.unwrap_or(String::new());
            if let Ok(ret) =
                bilibili::api::submit_video(&client, &account.to_account(), &profile, &video).await
            {
                // update video status and details
                // 1 means uploaded
                video_row.status = 1;
                video_row.bvid = ret.bvid.clone();
                video_row.title = profile.title;
                video_row.desc = profile.desc;
                video_row.tags = profile.tag;
                video_row.area = profile.tid as i64;
                state.db.update_video(&video_row).await?;
                state
                    .db
                    .new_message(
                        "投稿成功",
                        &format!("投稿了房间 {} 的切片：{}", room_id, ret.bvid),
                    )
                    .await?;
                if state.config.read().await.post_notify {
                    #[cfg(feature = "gui")]
                    state
                        .app_handle
                        .notification()
                        .builder()
                        .title("BiliShadowReplay - 投稿成功")
                        .body(format!("投稿了房间 {} 的切片: {}", room_id, ret.bvid))
                        .show()
                        .unwrap();
                }
                reporter.finish(true, "投稿成功").await;
                Ok(ret.bvid)
            } else {
                reporter.finish(false, "投稿失败").await;
                Err("Submit video failed".to_string())
            }
        }
        Err(e) => {
            reporter
                .finish(false, &format!("Preload video failed: {e}"))
                .await;
            Err(format!("Preload video failed: {e}"))
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn cancel(state: state_type!(), event_id: String) -> Result<(), String> {
    log::info!("Cancel task: {event_id}");
    let cancel_result = state.task_manager.cancel_task(&event_id).await;
    match cancel_result {
        Ok(()) => {
            state
                .db
                .update_task(&event_id, "cancelled", "任务取消", None)
                .await?;
        }
        Err(e) if e == "Task not found" => {
            let task = state.db.get_task(&event_id).await?;
            if matches!(task.status.as_str(), "pending" | "processing") {
                state
                    .db
                    .update_task(&event_id, "cancelled", "任务取消", None)
                    .await?;
            }
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video(state: state_type!(), id: i64) -> Result<VideoRow, String> {
    Ok(state.db.get_video(id).await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_videos(state: state_type!(), room_id: String) -> Result<Vec<VideoRow>, String> {
    state
        .db
        .get_videos(&room_id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_all_videos(state: state_type!()) -> Result<Vec<VideoRow>, String> {
    state.db.get_all_videos().await.map_err(|e| e.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_cover(state: state_type!(), id: i64) -> Result<String, String> {
    state
        .db
        .get_video_cover(id)
        .await
        .map_err(|e| e.to_string())
}

async fn remove_required_media_file(path: &Path) -> Result<(), String> {
    const ATTEMPTS: usize = 3;
    let mut last_error = None;

    for attempt in 0..ATTEMPTS {
        match tokio::fs::remove_file(path).await {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last_error = Some(error),
        }

        if attempt + 1 < ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    let error = last_error.expect("delete attempt must record an error");
    Err(format!(
        "无法删除视频文件 {}：{}。请关闭正在播放或占用该视频的窗口后重试",
        path.display(),
        error
    ))
}

fn should_delete_media_file(
    file_references: i64,
    archived_on_nas: bool,
    delete_archived_file: bool,
) -> bool {
    file_references <= 1 && (!archived_on_nas || delete_archived_file)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn delete_video(
    state: state_type!(),
    id: i64,
    delete_archived_file: Option<bool>,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<(), String> {
    let audit = require_sensitive_write(
        "delete_video",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("video:{id}"),
    )?;
    // get video info from db
    let video = match state.db.get_video(id).await {
        Ok(value) => value,
        Err(error) => {
            let error = error.to_string();
            audit_tool_failure(&audit, &error);
            return Err(error);
        }
    };
    let output = state.config.read().await.output.clone();
    let archive = state
        .db
        .get_video_archive_by_video(id)
        .await
        .map_err(|error| error.to_string())?;
    let archived_on_nas = archive
        .as_ref()
        .is_some_and(|row| row.status == "archived" && !row.nas_path.trim().is_empty());
    let filepath = if archived_on_nas {
        PathBuf::from(
            archive
                .as_ref()
                .map(|row| row.nas_path.as_str())
                .unwrap_or_default(),
        )
    } else {
        Path::new(&output).join(&video.file)
    };
    let file = filepath.as_path();
    let file_references = match state.db.count_videos_with_file(&video.file).await {
        Ok(value) => value,
        Err(error) => {
            let error = error.to_string();
            audit_tool_failure(&audit, &error);
            return Err(error);
        }
    };

    // Imported aliases can share one physical file. Keep it until the final
    // database reference is removed.
    let should_delete_media = should_delete_media_file(
        file_references,
        archived_on_nas,
        delete_archived_file.unwrap_or(false),
    );
    if should_delete_media {
        if let Err(error) = remove_required_media_file(file).await {
            audit_tool_failure(&audit, &error);
            return Err(error);
        }
        log::info!("已删除视频文件: {}", file.display());
    }

    if let Err(error) = state.db.delete_video(id).await {
        let error = error.to_string();
        audit_tool_failure(&audit, &error);
        return Err(error);
    }

    let event =
        events::new_webhook_event(events::CLIP_DELETED, events::Payload::Clip(video.clone()));
    if let Err(e) = state.webhook_poster.post_event(&event).await {
        log::error!("Post webhook event error: {e}");
    }

    if should_delete_media {
        let srt_path = file.with_extension("srt");
        let _ = tokio::fs::remove_file(srt_path).await;
        let transcript_path = TranscriptArtifactStore::video_artifact_dir(file);
        if tokio::fs::try_exists(&transcript_path)
            .await
            .unwrap_or(false)
        {
            let _ = tokio::fs::remove_dir_all(transcript_path).await;
        }
        let wav_path = file.with_extension("wav");
        let _ = tokio::fs::remove_file(wav_path).await;
        let mp3_path = file.with_extension("mp3");
        let _ = tokio::fs::remove_file(mp3_path).await;
        let opus_path = file.with_extension("opus");
        let _ = tokio::fs::remove_file(opus_path).await;
        let cover_path = Path::new(&output).join(&video.cover);
        let _ = tokio::fs::remove_file(cover_path).await;
    }

    audit_tool_success(&audit);
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_video_archives(state: state_type!()) -> Result<Vec<VideoArchiveRow>, String> {
    state
        .db
        .list_video_archives()
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn retry_video_archive(
    state: state_type!(),
    video_id: i64,
) -> Result<VideoArchiveRow, String> {
    let row = state
        .db
        .retry_video_archive(video_id)
        .await
        .map_err(|error| format!("无法重新提交 NAS 转存任务：{error}"))?;
    state.nas_archive.wake();
    Ok(row)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn open_video_archive_location(
    state: state_type!(),
    video_id: i64,
) -> Result<(), String> {
    let archive = state
        .db
        .get_video_archive_by_video(video_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "该视频还没有 NAS 转存记录".to_string())?;
    if archive.status != "archived" || archive.nas_path.trim().is_empty() {
        return Err("该视频尚未完成 NAS 转存".to_string());
    }
    let path = PathBuf::from(&archive.nas_path);
    if !path.is_file() {
        return Err("NAS 视频当前不可访问，请检查网络或共享文件夹连接".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定 NAS 视频所在文件夹".to_string())?;
    open::that_detached(parent).map_err(|error| format!("无法打开 NAS 文件夹：{error}"))
}

/// Open the original video with the OS default player.
/// Imported TS/FLV/MP4 files are already on disk; do not convert first.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn open_video_externally(state: state_type!(), id: i64) -> Result<(), String> {
    let video = state.db.get_video(id).await?;
    let output = state.config.read().await.output.clone();
    let source_path =
        resolve_external_playback_path(state.db.as_ref(), Path::new(&output), id, &video.file)
            .await?;
    if !source_path.is_file() {
        return Err(format!(
            "视频文件不存在或当前不可访问：{}",
            source_path.display()
        ));
    }
    open_path_with_default_app(&source_path)
}

pub(crate) async fn resolve_external_playback_path(
    db: &crate::database::Database,
    output: &Path,
    video_id: i64,
    file: &str,
) -> Result<PathBuf, String> {
    let archive = db
        .get_video_archive_by_video(video_id)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(archive) = archive.as_ref() {
        if archive.status == "archived" && !archive.nas_path.trim().is_empty() {
            let nas = PathBuf::from(archive.nas_path.trim());
            if nas.is_file() {
                return Ok(crate::handlers::utils::prefer_accessible_windows_path(&nas));
            }
        }
    }

    let candidate = PathBuf::from(file.trim());
    if candidate.is_absolute() {
        if candidate.is_file() {
            return Ok(crate::handlers::utils::prefer_accessible_windows_path(
                &candidate,
            ));
        }
        if let Some(archive) = archive.as_ref() {
            if archive.status == "archived" && !archive.nas_path.trim().is_empty() {
                return Err(format!(
                    "本地/绝对路径不可访问，且 NAS 路径当前也打不开：local={}, nas={}",
                    candidate.display(),
                    archive.nas_path.trim()
                ));
            }
        }
        return Err(format!(
            "视频文件不存在或当前不可访问：{}",
            candidate.display()
        ));
    }

    match database_video_media_path(db, output, video_id, file).await {
        Ok(path) if path.is_file() => Ok(crate::handlers::utils::prefer_accessible_windows_path(
            &path,
        )),
        Ok(path) => {
            if let Some(archive) = archive.as_ref() {
                if archive.status == "archived" && !archive.nas_path.trim().is_empty() {
                    return Err(format!(
                        "本地文件不存在（可能已转存），且 NAS 路径当前不可访问：local={}, nas={}",
                        path.display(),
                        archive.nas_path.trim()
                    ));
                }
            }
            Err(format!("视频文件不存在或当前不可访问：{}", path.display()))
        }
        Err(error) => {
            let joined = output.join(file);
            if joined.is_file() {
                Ok(crate::handlers::utils::prefer_accessible_windows_path(
                    &joined,
                ))
            } else if let Some(archive) = archive.as_ref() {
                if archive.status == "archived" && !archive.nas_path.trim().is_empty() {
                    Err(format!(
                        "无法解析本地路径，且 NAS 路径当前不可访问：{error}；nas={}",
                        archive.nas_path.trim()
                    ))
                } else {
                    Err(error)
                }
            } else {
                Err(error)
            }
        }
    }
}

fn open_path_with_default_app(path: &Path) -> Result<(), String> {
    let accessible = crate::handlers::utils::prefer_accessible_windows_path(path);
    #[cfg(windows)]
    {
        // Reveal in Explorer using a drive-letter path when possible (UNC + [brackets] is flaky).
        let arg = format!("/select,{}", accessible.to_string_lossy());
        let _ = std::process::Command::new("explorer").arg(&arg).spawn();
        // Avoid `cmd /c start` — it mishandles `[imported]...` filenames.
        return open::that_detached(&accessible)
            .map_err(|error| format!("无法启动系统播放器（{}）：{error}", accessible.display()));
    }
    #[cfg(not(windows))]
    {
        open::that_detached(&accessible).map_err(|error| {
            format!(
                "无法用系统播放器打开视频（{}）：{error}",
                accessible.display()
            )
        })
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_typelist(
    state: state_type!(),
) -> Result<Vec<bilibili::response::Typelist>, String> {
    let account = state.db.get_account_by_platform("bilibili").await?;
    let client = reqwest::Client::new();
    match bilibili::api::get_video_typelist(&client, &account.to_account()).await {
        Ok(typelist) => Ok(typelist),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_video_cover(
    state: state_type!(),
    id: i64,
    cover: String,
) -> Result<(), String> {
    let video = state.db.get_video(id).await?;
    let output_path = Path::new(state.config.read().await.output.as_str()).join(&video.file);
    let cover_path = output_path.with_extension("jpg");
    // decode cover and write into file
    let base64 = cover.split("base64,").nth(1).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .unwrap();
    tokio::fs::write(&cover_path, bytes)
        .await
        .map_err(|e| e.to_string())?;
    let cover_file_name = cover_path.file_name().unwrap().to_str().unwrap();
    log::debug!("Update video cover: {id} {cover_file_name}");
    Ok(state.db.update_video_cover(id, cover_file_name).await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_subtitle(state: state_type!(), id: i64) -> Result<String, String> {
    log::debug!("Get video subtitle: {id}");
    let context = resolve_video_transcript_context(&state, id).await?;
    let video = state.db.get_video(context.video_id).await?;
    let duration_ms = if video.length > 0 {
        u64::try_from(video.length)
            .map_err(|error| error.to_string())?
            .saturating_mul(1_000)
    } else {
        ffmpeg::probe_media_duration_ms(&context.media_file).await?
    };
    let subtitle = if TranscriptArtifactStore::canonical_artifacts_exist(&context.artifact_dir)
        .await
        .map_err(|error| error.to_string())?
    {
        TranscriptArtifactStore::load_from_dir(context.source, context.artifact_dir)
            .await
            .map(|bundle| bundle.corrected_srt)
            .map_err(|error| error.to_string())?
    } else {
        match tokio::fs::read_to_string(context.media_file.with_extension("srt")).await {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.to_string()),
        }
    };
    validate_video_transcript_coverage(&subtitle, duration_ms)?;
    Ok(subtitle)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_playback_source(
    state: state_type!(),
    id: i64,
) -> Result<VideoPlaybackSource, String> {
    let video = state.db.get_video(id).await?;
    let output = state.config.read().await.output.clone();
    let source_path =
        database_video_media_path(&state.db, Path::new(&output), id, &video.file).await?;
    let sidecar_path = source_path.with_file_name(playback_sidecar_file_name(&video.file)?);
    let hls_playlist_path = playback_hls_playlist_path(&source_path, &video.file)?;
    let latest_task = state
        .db
        .get_tasks()
        .await?
        .into_iter()
        .filter(|task| is_playback_conversion_task(task, id))
        .max_by_key(|task| task.created_at.clone());
    let raw_transport_stream = requires_browser_playback_copy(&video.file);
    let hls_compatible = if raw_transport_stream && source_path.is_file() {
        ffmpeg::extract_video_metadata(&source_path)
            .await
            .map(|metadata| can_remux_for_browser_playback(&metadata))
            .unwrap_or(false)
    } else {
        false
    };
    if hls_compatible {
        if playback_hls_is_ready(
            &hls_playlist_path,
            latest_task.as_ref().map(|task| task.status.as_str()),
        ) {
            return Ok(VideoPlaybackSource {
                file: playback_file_label(&video.file, &hls_playlist_path, Path::new(&output))?,
                requires_preparation: true,
                ready: true,
                preparing: false,
                message: "Embedded TS playback is ready".to_string(),
            });
        }
        return Ok(VideoPlaybackSource {
            file: video.file,
            requires_preparation: true,
            ready: false,
            preparing: latest_task
                .as_ref()
                .is_some_and(|task| is_active_playback_conversion_task(task, id)),
            message: latest_task
                .map(|task| task.message)
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| "Waiting to build embedded TS playback index".to_string()),
        });
    }
    // Prefer a completed sidecar even when the original container looked playable.
    // Browser decode failures can force a conversion that must be served afterwards.
    if playback_copy_is_ready(
        sidecar_path.is_file(),
        latest_task.as_ref().map(|task| task.status.as_str()),
    ) {
        return Ok(VideoPlaybackSource {
            file: if Path::new(&video.file).is_absolute() {
                sidecar_path.to_string_lossy().to_string()
            } else {
                sidecar_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| "无法读取播放副本文件名".to_string())?
                    .to_string()
            },
            requires_preparation: true,
            ready: true,
            preparing: false,
            message: "已生成可播放版本".to_string(),
        });
    }

    let needs_playback_copy =
        requires_browser_playback_copy_for_path(&source_path, &video.file).await;
    if !needs_playback_copy {
        // An in-flight forced conversion must stay visible even if probing says
        // the original container is "already playable".
        if latest_task
            .as_ref()
            .is_some_and(|task| is_active_playback_conversion_task(task, id))
        {
            return Ok(VideoPlaybackSource {
                file: video.file,
                requires_preparation: true,
                ready: false,
                preparing: true,
                message: latest_task
                    .map(|task| task.message)
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "正在生成可播放版本".to_string()),
            });
        }
        return Ok(VideoPlaybackSource {
            file: video.file,
            requires_preparation: false,
            ready: true,
            preparing: false,
            message: String::new(),
        });
    }

    Ok(VideoPlaybackSource {
        file: video.file,
        requires_preparation: true,
        ready: false,
        preparing: latest_task
            .as_ref()
            .is_some_and(|task| is_active_playback_conversion_task(task, id)),
        message: latest_task
            .map(|task| task.message)
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| "需要生成可播放版本".to_string()),
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn prepare_video_playback(
    state: state_type!(),
    event_id: String,
    id: i64,
    force: Option<bool>,
) -> Result<VideoPlaybackSource, String> {
    let force = force.unwrap_or(false);
    let current = get_video_playback_source(state.clone(), id).await?;
    // A non-forced click can join an in-flight conversion. Forced retries must
    // supersede stuck DB rows that no longer have a live scheduler job.
    if current.preparing && !force {
        return Ok(current);
    }
    if !force && (current.ready || !current.requires_preparation) {
        return Ok(current);
    }

    let video = state.db.get_video(id).await?;
    let output = state.config.read().await.output.clone();
    let source_path =
        database_video_media_path(&state.db, Path::new(&output), id, &video.file).await?;
    if !source_path.is_file() {
        return Err(format!(
            "原始视频文件不存在，无法生成可播放版本：{}",
            source_path.display()
        ));
    }
    let source_metadata = ffmpeg::extract_video_metadata(&source_path).await.ok();
    let use_embedded_hls = requires_browser_playback_copy(&video.file)
        && source_metadata
            .as_ref()
            .is_some_and(can_remux_for_browser_playback);
    let playback_path = if use_embedded_hls {
        playback_hls_playlist_path(&source_path, &video.file)?
    } else {
        source_path.with_file_name(playback_sidecar_file_name(&video.file)?)
    };
    // Drop partial/untrusted sidecars left by interrupted runs before starting again.
    if (force || !current.ready) && use_embedded_hls {
        if let Some(parent) = playback_path.parent() {
            let _ = tokio::fs::remove_dir_all(parent).await;
        }
    } else if playback_path.is_file() && (force || !current.ready) {
        let _ = tokio::fs::remove_file(&playback_path).await;
    }

    if force || current.preparing {
        let _ = state
            .db
            .interrupt_active_playback_conversion_tasks(id, "已由用户重新触发生成可播放版本")
            .await?;
    }

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "prepare_video_playback".to_string(),
        status: "pending".to_string(),
        message: "等待生成 H.264 可播放版本".to_string(),
        metadata: json!({
            "video_id": id,
            "source_file": video.file,
            "playback_file": playback_path.file_name().and_then(|value| value.to_str()),
            "playback_kind": if use_embedded_hls { "hls" } else { "mp4" },
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    if !state
        .db
        .add_playback_conversion_task_if_idle(&task, id)
        .await?
    {
        // Another worker won the race. If the DB still looks idle, clear the
        // blocker once and retry so a manual click cannot silently no-op.
        let _ = state
            .db
            .interrupt_active_playback_conversion_tasks(id, "清除卡住的可播放版本任务后重试")
            .await?;
        if !state
            .db
            .add_playback_conversion_task_if_idle(&task, id)
            .await?
        {
            return get_video_playback_source(state, id).await;
        }
    }

    #[cfg(feature = "gui")]
    let state_clone = (*state).clone();
    #[cfg(feature = "headless")]
    let state_clone = state.clone();
    let task_id = event_id.clone();
    let worker_task_id = task_id.clone();
    let worker_source = source_path.clone();
    let worker_playback = playback_path.clone();
    let worker_video_id = id;
    let worker_embedded_hls = use_embedded_hls;
    let queued = state
        .task_manager
        .add_task(Task::new(task_id, TaskPriority::Normal, async move {
            if worker_embedded_hls {
                let segment_pattern = worker_playback
                    .parent()
                    .ok_or_else(|| "HLS playlist path has no parent directory".to_string())?
                    .join("segment-%06d.ts");
                let _media_permit = state_clone
                    .media_execution_gate
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| "media execution gate is closed".to_string())?;
                match ffmpeg::package_hls_for_browser(
                    &worker_source,
                    &worker_playback,
                    &segment_pattern,
                    &reporter,
                )
                .await
                {
                    Ok(()) => {
                        reporter.finish(true, "项目内嵌 TS 播放索引已完成").await;
                        let _ = state_clone
                            .db
                            .update_task(&worker_task_id, "success", "项目内嵌 TS 播放索引已完成", None)
                            .await;
                        return Ok(());
                    }
                    Err(error) => {
                        if let Some(parent) = worker_playback.parent() {
                            let _ = tokio::fs::remove_dir_all(parent).await;
                        }
                        reporter
                            .finish(false, &format!("内嵌 TS 播放索引失败: {error}"))
                            .await;
                        let _ = state_clone
                            .db
                            .update_task(
                                &worker_task_id,
                                "failed",
                                &format!("内嵌 TS 播放索引失败: {error}"),
                                None,
                            )
                            .await;
                        return Err(error);
                    }
                }
            }
            let metadata = ffmpeg::extract_video_metadata(&worker_source).await;
            let fast_remux_attempted = matches!(
                metadata.as_ref(),
                Ok(value) if playback_conversion_kind(value) == PlaybackConversionKind::FastRemux
            );
            if fast_remux_attempted {
                reporter
                    .update("检测到 H.264/AAC，正在快速封装 MP4（不重编码）…")
                    .await;
                let duration = metadata.as_ref().ok().map(|value| value.duration);
                match ffmpeg::remux_mp4_faststart_with_duration(
                    &worker_source,
                    &worker_playback,
                    &reporter,
                    duration,
                )
                .await
                {
                    Ok(()) => {
                        reporter.finish(true, "可播放版本快速封装完成").await;
                        let _ = state_clone
                            .db
                            .update_task(&worker_task_id, "success", "可播放版本快速封装完成", None)
                            .await;
                        return Ok(());
                    }
                    Err(remux_error) => {
                        log::warn!(
                            "Fast remux failed for {}: {remux_error}; falling back to re-encode",
                            worker_source.display()
                        );
                    }
                }
            }
            // Subtitle generation can also run FFmpeg across the full source.
            // Serialize it with browser conversion: otherwise a large recording
            // can saturate the CPU/GPU encoder and appear frozen.
            while state_clone
                .db
                .get_active_video_subtitle_task(worker_video_id)
                .await
                .map_err(|error| error.to_string())?
                .is_some()
            {
                reporter
                    .update("逐字稿正在生成；可播放版已排队，避免同时占用 CPU、显卡和磁盘")
                    .await;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            reporter
                .update("正在生成可播放版本，请保留原始视频不动…")
                .await;
            let _media_permit = state_clone
                .media_execution_gate
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| "media execution gate is closed".to_string())?;
            let conversion_result = match metadata {
                Ok(metadata)
                    if can_remux_for_browser_playback(&metadata) && !fast_remux_attempted =>
                {
                    match ffmpeg::remux_mp4_faststart_with_duration(
                        &worker_source,
                        &worker_playback,
                        &reporter,
                        Some(metadata.duration),
                    )
                    .await
                    {
                        Ok(()) => Ok(()),
                        Err(remux_error) => {
                            log::warn!(
                                "Fast remux failed for {}: {remux_error}; falling back to re-encode",
                                worker_source.display()
                            );
                            reporter
                                .update("快速封装失败，正在转为 H.264 MP4…")
                                .await;
                            ffmpeg::try_browser_compatible_conversion_with_metadata(
                                &worker_source,
                                &worker_playback,
                                &reporter,
                                Some(&metadata),
                            )
                            .await
                        }
                    }
                }
                Ok(metadata) => {
                    ffmpeg::try_browser_compatible_conversion_with_metadata(
                        &worker_source,
                        &worker_playback,
                        &reporter,
                        Some(&metadata),
                    )
                    .await
                }
                Err(_) => ffmpeg::try_browser_compatible_conversion(
                    &worker_source,
                    &worker_playback,
                    &reporter,
                )
                .await,
            };
            match conversion_result
            {
                Ok(()) => {
                    reporter.finish(true, "可播放版本生成完成").await;
                    let _ = state_clone
                        .db
                        .update_task(&worker_task_id, "success", "可播放版本生成完成", None)
                        .await;
                    Ok(())
                }
                Err(error) => {
                    let _ = tokio::fs::remove_file(&worker_playback).await;
                    reporter
                        .finish(false, &format!("生成可播放版本失败: {error}"))
                        .await;
                    let _ = state_clone
                        .db
                        .update_task(
                            &worker_task_id,
                            "failed",
                            &format!("生成可播放版本失败: {error}"),
                            None,
                        )
                        .await;
                    Err(error)
                }
            }
        }))
        .await;
    if let Err(error) = queued {
        state
            .db
            .update_task(
                &event_id,
                "failed",
                &format!("无法启动转换任务: {error}"),
                None,
            )
            .await?;
        return Err(error);
    }

    get_video_playback_source(state, id).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_video_subtitle(
    state: state_type!(),
    event_id: String,
    id: i64,
) -> Result<String, String> {
    generate_video_subtitle_inner(&state, event_id, id).await
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealTranscriptWindowRequest {
    pub start_sec: f64,
    pub end_sec: f64,
}

fn deal_window_subtitle_path(context: &CanonicalVideoTranscriptContext) -> PathBuf {
    context.artifact_dir.join("deal-windows.srt")
}

fn range_subtitle_paths(
    context: &CanonicalVideoTranscriptContext,
    artifact_stem: &str,
) -> (PathBuf, PathBuf, PathBuf) {
    (
        context.artifact_dir.join(format!("{artifact_stem}.srt")),
        context.artifact_dir.join(format!("{artifact_stem}.partial.srt")),
        context.artifact_dir.join(format!("{artifact_stem}.json")),
    )
}

fn merge_srt_documents(left: &str, right: &str) -> Result<String, String> {
    let mut items = Vec::new();
    for (label, content) in [("成交窗", left), ("补转段", right)] {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = srtparse::from_str(trimmed)
            .map_err(|error| format!("合并{label}逐字稿失败: {error}"))?;
        items.extend(parsed);
    }
    if items.is_empty() {
        return Ok(String::new());
    }
    items.sort_by(|left_item, right_item| {
        left_item
            .start_time
            .hours
            .cmp(&right_item.start_time.hours)
            .then(left_item.start_time.minutes.cmp(&right_item.start_time.minutes))
            .then(left_item.start_time.seconds.cmp(&right_item.start_time.seconds))
            .then(
                left_item
                    .start_time
                    .milliseconds
                    .cmp(&right_item.start_time.milliseconds),
            )
    });
    Ok(items
        .into_iter()
        .enumerate()
        .map(|(index, mut item)| {
            item.pos = index + 1;
            item_to_srt(&item)
        })
        .collect())
}

async fn promote_deal_windows_to_canonical_subtitle(
    state: &State,
    id: i64,
) -> Result<String, String> {
    let context = resolve_video_transcript_context(state, id).await?;
    let deal_path = deal_window_subtitle_path(&context);
    let deal = if deal_path.is_file() {
        tokio::fs::read_to_string(&deal_path)
            .await
            .map_err(|error| format!("读取成交窗口逐字稿失败: {error}"))?
    } else {
        String::new()
    };
    if deal.trim().is_empty() {
        return Err("没有可合并的成交窗口逐字稿。".to_string());
    }
    write_legacy_video_subtitle(&context.media_file, &deal).await?;
    Ok(deal)
}

fn validate_deal_transcript_windows(ranges: &[DealTranscriptWindowRequest]) -> Result<f64, String> {
    if ranges.is_empty() {
        return Err("没有可转写的成交时间窗口，请先导入成交订单。".to_string());
    }
    if ranges.len() > 100 {
        return Err("一次最多识别 100 个成交时间窗口。".to_string());
    }
    let mut total_duration = 0.0;
    let mut previous_end = 0.0;
    for (index, range) in ranges.iter().enumerate() {
        if !(range.start_sec.is_finite()
            && range.end_sec.is_finite()
            && range.start_sec >= 0.0
            && range.end_sec > range.start_sec)
        {
            return Err(format!("第 {} 个成交时间窗口无效。", index + 1));
        }
        if index > 0 && range.start_sec < previous_end {
            return Err("成交时间窗口必须按时间排序且不能重叠。".to_string());
        }
        total_duration += range.end_sec - range.start_sec;
        previous_end = range.end_sec;
    }
    Ok(total_duration)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_deal_window_subtitle(
    state: state_type!(),
    id: i64,
) -> Result<String, String> {
    let context = resolve_video_transcript_context(&state, id).await?;
    let path = deal_window_subtitle_path(&context);
    if !path.is_file() {
        return Ok(String::new());
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取成交窗口逐字稿失败: {error}"))
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_deal_window_partial_subtitle(
    state: state_type!(),
    id: i64,
) -> Result<String, String> {
    let context = resolve_video_transcript_context(&state, id).await?;
    let complete = deal_window_subtitle_path(&context);
    let path = if complete.is_file() {
        complete
    } else {
        context.artifact_dir.join("deal-windows.partial.srt")
    };
    if !path.is_file() {
        return Ok(String::new());
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取部分成交窗口逐字稿失败: {error}"))
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveProductScanProduct {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveProductMentionScanItem {
    pub id: String,
    pub name: String,
    pub mention_count: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveProductMentionScanSnapshot {
    pub version: u32,
    pub video_id: i64,
    pub catalog_signature: String,
    pub status: String,
    pub processed_duration_sec: f64,
    pub total_duration_sec: f64,
    pub products: Vec<LiveProductMentionScanItem>,
    pub error: Option<String>,
}

fn live_product_scan_path(context: &CanonicalVideoTranscriptContext) -> PathBuf {
    context.artifact_dir.join("full-live-product-mentions.json")
}

fn normalize_product_scan_text(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn validate_live_product_scan_catalog(
    products: &[LiveProductScanProduct],
) -> Result<Vec<LiveProductScanProduct>, String> {
    if products.is_empty() {
        return Err("本场没有可用于识别的订单商品。".to_string());
    }
    if products.len() > 200 {
        return Err("单场最多扫描 200 个订单商品。".to_string());
    }
    let mut validated = Vec::with_capacity(products.len());
    for product in products {
        let id = product.id.trim().to_string();
        let name = product.name.trim().to_string();
        let mut aliases = product
            .aliases
            .iter()
            .map(|alias| normalize_product_scan_text(alias))
            .filter(|alias| alias.len() >= 2)
            .collect::<Vec<_>>();
        aliases.sort();
        aliases.dedup();
        if id.is_empty() || name.is_empty() || aliases.is_empty() {
            continue;
        }
        validated.push(LiveProductScanProduct { id, name, aliases });
    }
    if validated.is_empty() {
        return Err("订单商品名称清洗后没有可识别的型号。".to_string());
    }
    Ok(validated)
}

fn sorted_live_product_scan_items(
    catalog: &[LiveProductScanProduct],
    counts: &[u64],
) -> Vec<LiveProductMentionScanItem> {
    let mut items = catalog
        .iter()
        .zip(counts.iter().copied())
        .map(|(product, mention_count)| LiveProductMentionScanItem {
            id: product.id.clone(),
            name: product.name.clone(),
            mention_count,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .mention_count
            .cmp(&left.mention_count)
            .then_with(|| left.name.cmp(&right.name))
    });
    items
}

async fn read_live_product_scan_snapshot(
    context: &CanonicalVideoTranscriptContext,
) -> Result<Option<LiveProductMentionScanSnapshot>, String> {
    let path = live_product_scan_path(context);
    if !path.is_file() {
        return Ok(None);
    }
    let content = tokio::fs::read(&path)
        .await
        .map_err(|error| format!("读取整场商品统计失败: {error}"))?;
    serde_json::from_slice(&content)
        .map(Some)
        .map_err(|error| format!("解析整场商品统计失败: {error}"))
}

async fn write_live_product_scan_snapshot(
    context: &CanonicalVideoTranscriptContext,
    snapshot: &LiveProductMentionScanSnapshot,
) -> Result<(), String> {
    tokio::fs::create_dir_all(&context.artifact_dir)
        .await
        .map_err(|error| format!("创建整场商品统计目录失败: {error}"))?;
    let destination = live_product_scan_path(context);
    let temporary = context
        .artifact_dir
        .join(".full-live-product-mentions.json.tmp");
    let content = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| format!("序列化整场商品统计失败: {error}"))?;
    tokio::fs::write(&temporary, content)
        .await
        .map_err(|error| format!("保存整场商品统计失败: {error}"))?;
    if destination.is_file() {
        tokio::fs::remove_file(&destination)
            .await
            .map_err(|error| format!("更新整场商品统计失败: {error}"))?;
    }
    tokio::fs::rename(&temporary, &destination)
        .await
        .map_err(|error| format!("提交整场商品统计失败: {error}"))
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_video_product_mention_scan(
    state: state_type!(),
    id: i64,
    catalog_signature: String,
) -> Result<LiveProductMentionScanSnapshot, String> {
    let context = resolve_video_transcript_context(&state, id).await?;
    if let Some(snapshot) = read_live_product_scan_snapshot(&context).await? {
        if snapshot.catalog_signature == catalog_signature {
            return Ok(snapshot);
        }
    }
    let total_duration_sec =
        ffmpeg::probe_media_duration_ms(&context.media_file).await? as f64 / 1000.0;
    Ok(LiveProductMentionScanSnapshot {
        version: 1,
        video_id: context.video_id,
        catalog_signature,
        status: "idle".to_string(),
        processed_duration_sec: 0.0,
        total_duration_sec,
        products: Vec::new(),
        error: None,
    })
}

/// Scan the complete original video in fixed audio chunks. Only per-product
/// sentence counts are persisted; existing subtitles and order rows are never changed.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_video_product_mention_scan(
    state: state_type!(),
    event_id: String,
    id: i64,
    catalog_signature: String,
    products: Vec<LiveProductScanProduct>,
    force: bool,
) -> Result<LiveProductMentionScanSnapshot, String> {
    let catalog = validate_live_product_scan_catalog(&products)?;
    let context = resolve_video_transcript_context(&state, id).await?;
    let total_duration_sec =
        ffmpeg::probe_media_duration_ms(&context.media_file).await? as f64 / 1000.0;
    if total_duration_sec <= 0.0 {
        return Err("无法读取整场直播时长。".to_string());
    }
    let existing = read_live_product_scan_snapshot(&context).await?;
    if !force {
        if let Some(snapshot) = existing.as_ref() {
            if snapshot.catalog_signature == catalog_signature && snapshot.status == "completed" {
                return Ok(snapshot.clone());
            }
        }
    }

    let mut counts = vec![0_u64; catalog.len()];
    let mut processed_duration_sec = 0.0;
    if !force {
        if let Some(snapshot) = existing.as_ref() {
            if snapshot.catalog_signature == catalog_signature {
                processed_duration_sec = snapshot
                    .processed_duration_sec
                    .clamp(0.0, total_duration_sec);
                for (index, product) in catalog.iter().enumerate() {
                    counts[index] = snapshot
                        .products
                        .iter()
                        .find(|item| item.id == product.id)
                        .map(|item| item.mention_count)
                        .unwrap_or_default();
                }
            }
        }
    }

    let task = TaskRow {
        id: event_id.clone(),
        task_type: "generate_video_product_mention_scan".to_string(),
        status: "pending".to_string(),
        message: "整场商品扫描正在排队".to_string(),
        metadata: json!({
            "video_id": id,
            "catalog_signature": catalog_signature,
            "product_count": catalog.len(),
            "processed_duration_sec": processed_duration_sec,
            "total_duration_sec": total_duration_sec,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_task(&task).await?;

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    reporter.update("整场商品扫描正在等待媒体任务").await;
    let temp_dir = std::env::temp_dir().join(format!("bsr-product-scan-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|error| format!("创建整场商品扫描目录失败: {error}"))?;
    let hotwords = catalog
        .iter()
        .flat_map(|product| {
            std::iter::once(product.name.as_str()).chain(product.aliases.iter().map(String::as_str))
        })
        .take(500)
        .collect::<Vec<_>>()
        .join(" ");

    let generation_result: Result<LiveProductMentionScanSnapshot, String> = async {
        let asr = crate::subtitle_generator::funasr::FunAsr::new().await?;
        const CHUNK_DURATION_SEC: f64 = 300.0;
        let total_chunks = (total_duration_sec / CHUNK_DURATION_SEC).ceil().max(1.0) as usize;
        let mut chunk_index = (processed_duration_sec / CHUNK_DURATION_SEC).floor() as usize;
        while processed_duration_sec < total_duration_sec {
            let duration_sec = CHUNK_DURATION_SEC.min(total_duration_sec - processed_duration_sec);
            reporter
                .update(&format!(
                    "正在扫描整场商品 {}/{}（已完成 {:.1}%）",
                    chunk_index + 1,
                    total_chunks,
                    processed_duration_sec / total_duration_sec * 100.0
                ))
                .await;
            let segment_path = temp_dir.join(format!("product-scan-{chunk_index:04}.wav"));
            // Only FFmpeg extraction uses the shared media gate. Releasing it
            // before ASR keeps cutting and playback preparation responsive.
            let media_permit = state
                .media_execution_gate
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| "media execution gate is closed".to_string())?;
            ffmpeg::extract_audio_segment(
                &context.media_file,
                processed_duration_sec,
                duration_sec,
                &segment_path,
            )
            .await?;
            drop(media_permit);
            let response = asr
                .transcribe_with_hotwords(&segment_path, None, &hotwords)
                .await?;
            let segments = if response.segments.is_empty() {
                &response.raw_segments
            } else {
                &response.segments
            };
            for segment in segments {
                let text = normalize_product_scan_text(&segment.text);
                if text.is_empty() {
                    continue;
                }
                for (index, product) in catalog.iter().enumerate() {
                    if product.aliases.iter().any(|alias| text.contains(alias)) {
                        counts[index] = counts[index].saturating_add(1);
                    }
                }
            }
            let _ = tokio::fs::remove_file(&segment_path).await;
            processed_duration_sec =
                (processed_duration_sec + duration_sec).min(total_duration_sec);
            chunk_index += 1;
            let snapshot = LiveProductMentionScanSnapshot {
                version: 1,
                video_id: context.video_id,
                catalog_signature: catalog_signature.clone(),
                status: "processing".to_string(),
                processed_duration_sec,
                total_duration_sec,
                products: sorted_live_product_scan_items(&catalog, &counts),
                error: None,
            };
            write_live_product_scan_snapshot(&context, &snapshot).await?;
        }
        let snapshot = LiveProductMentionScanSnapshot {
            version: 1,
            video_id: context.video_id,
            catalog_signature: catalog_signature.clone(),
            status: "completed".to_string(),
            processed_duration_sec: total_duration_sec,
            total_duration_sec,
            products: sorted_live_product_scan_items(&catalog, &counts),
            error: None,
        };
        write_live_product_scan_snapshot(&context, &snapshot).await?;
        Ok(snapshot)
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    match generation_result {
        Ok(snapshot) => {
            reporter.finish(true, "整场高频商品 TOP5 统计完成").await;
            state
                .db
                .update_task(
                    &event_id,
                    "success",
                    "整场高频商品 TOP5 统计完成",
                    Some(
                        json!({
                            "video_id": id,
                            "catalog_signature": catalog_signature,
                            "processed_duration_sec": total_duration_sec,
                            "total_duration_sec": total_duration_sec,
                        })
                        .to_string()
                        .as_str(),
                    ),
                )
                .await?;
            Ok(snapshot)
        }
        Err(error) => {
            let snapshot = LiveProductMentionScanSnapshot {
                version: 1,
                video_id: context.video_id,
                catalog_signature: catalog_signature.clone(),
                status: "failed".to_string(),
                processed_duration_sec,
                total_duration_sec,
                products: sorted_live_product_scan_items(&catalog, &counts),
                error: Some(error.clone()),
            };
            let _ = write_live_product_scan_snapshot(&context, &snapshot).await;
            reporter
                .finish(false, &format!("整场商品扫描失败: {error}"))
                .await;
            state
                .db
                .update_task(
                    &event_id,
                    "failed",
                    &format!("整场商品扫描失败: {error}"),
                    None,
                )
                .await?;
            Err(error)
        }
    }
}

/// Transcribe only merged windows around paid orders. Returned SRT timestamps
/// are shifted back to the original full-video clock so seeking and clipping
/// keep using one timeline.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_video_deal_window_subtitle(
    state: state_type!(),
    event_id: String,
    id: i64,
    ranges: Vec<DealTranscriptWindowRequest>,
    artifact_stem: Option<String>,
) -> Result<String, String> {
    let stem = artifact_stem
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("deal-windows")
        .to_string();
    let is_gap_fill = stem == "gap-windows";
    if is_gap_fill && ranges.is_empty() {
        let merged = promote_deal_windows_to_canonical_subtitle(&state, id).await?;
        let task = TaskRow {
            id: event_id.clone(),
            task_type: "generate_video_gap_fill_subtitle".to_string(),
            status: "success".to_string(),
            message: "无需补转空洞，已合并成交窗文稿".to_string(),
            metadata: json!({
                "video_id": id,
                "range_count": 0,
                "artifact_stem": stem,
            })
            .to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        state.db.add_task(&task).await?;
        return Ok(merged);
    }

    let total_duration = validate_deal_transcript_windows(&ranges)?;
    let context = resolve_video_transcript_context(&state, id).await?;
    let task_type = if is_gap_fill {
        "generate_video_gap_fill_subtitle"
    } else {
        "generate_video_deal_window_subtitle"
    };
    let pending_message = if is_gap_fill {
        format!(
            "等待补转 {} 个非成交时段，共 {:.1} 分钟",
            ranges.len(),
            total_duration / 60.0
        )
    } else {
        format!(
            "等待转写 {} 个成交窗口，共 {:.1} 分钟",
            ranges.len(),
            total_duration / 60.0
        )
    };
    let task = TaskRow {
        id: event_id.clone(),
        task_type: task_type.to_string(),
        status: "pending".to_string(),
        message: pending_message,
        metadata: json!({
            "video_id": id,
            "range_count": ranges.len(),
            "audio_duration_sec": total_duration,
            "artifact_stem": stem,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_task(&task).await?;

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    reporter
        .update(if is_gap_fill {
            "非成交段补转正在排队"
        } else {
            "成交窗口转写正在排队"
        })
        .await;
    let _media_permit = state
        .media_execution_gate
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "media execution gate is closed".to_string())?;

    let (
        generator_type,
        whisper_model,
        whisper_prompt,
        openai_api_key,
        openai_api_endpoint,
        language_hint,
    ) = {
        let config = state.config.read().await;
        (
            config.subtitle_generator_type.clone(),
            config.whisper_model.clone(),
            config.whisper_prompt.clone(),
            config.openai_api_key.clone(),
            config.openai_api_endpoint.clone(),
            config.whisper_language.clone(),
        )
    };
    let temp_dir = std::env::temp_dir().join(format!("bsr-deal-asr-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|error| format!("创建成交窗口转写目录失败: {error}"))?;
    let mut work_chunks = Vec::new();
    const MAX_ASR_CHUNK_SEC: f64 = 60.0;
    /// Parallel ffmpeg extract of deal-window WAVs.
    const DEAL_EXTRACT_CONCURRENCY: usize = 4;
    /// Match FunASR worker processes (see BSR_FUNASR_WORKERS, default 2).
    let deal_asr_concurrency = crate::subtitle_generator::funasr::worker_count().max(1);
    for range in &ranges {
        let mut start_sec = range.start_sec;
        while start_sec < range.end_sec {
            let end_sec = (start_sec + MAX_ASR_CHUNK_SEC).min(range.end_sec);
            work_chunks.push((start_sec, end_sec));
            start_sec = end_sec;
        }
    }
    let (final_subtitle_path, partial_subtitle_path, index_path) =
        range_subtitle_paths(&context, &stem);
    let chunk_count = work_chunks.len();
    let label = if is_gap_fill { "非成交段" } else { "成交窗口" };

    let generation_result: Result<String, String> = async {
        tokio::fs::create_dir_all(&context.artifact_dir)
            .await
            .map_err(|error| format!("创建成交逐字稿目录失败: {error}"))?;
        let _ = tokio::fs::remove_file(&partial_subtitle_path).await;

        // Phase 1: extract all WAV segments in parallel.
        let extract_sem = Arc::new(Semaphore::new(DEAL_EXTRACT_CONCURRENCY.max(1)));
        let mut extract_set = JoinSet::new();
        for (index, (start_sec, end_sec)) in work_chunks.iter().copied().enumerate() {
            let permit = extract_sem
                .clone()
                .acquire_owned()
                .await
                .map_err(|error| format!("成交音频提取调度失败: {error}"))?;
            let media_file = context.media_file.clone();
            let segment_path = temp_dir.join(format!("window-{index:03}.wav"));
            extract_set.spawn(async move {
                let _permit = permit;
                ffmpeg::extract_audio_segment(
                    &media_file,
                    start_sec,
                    end_sec - start_sec,
                    &segment_path,
                )
                .await
                .map_err(|error| {
                    format!(
                        "提取成交音频 {}/{}（{start_sec:.0}-{end_sec:.0} 秒）失败: {error}",
                        index + 1,
                        chunk_count
                    )
                })?;
                Ok::<(usize, f64, PathBuf), String>((index, start_sec, segment_path))
            });
        }
        let mut extracted: Vec<Option<(f64, PathBuf)>> = vec![None; chunk_count];
        let mut extract_done = 0usize;
        while let Some(joined) = extract_set.join_next().await {
            let (index, start_sec, segment_path) = joined
                .map_err(|error| format!("成交音频提取任务异常: {error}"))??;
            extracted[index] = Some((start_sec, segment_path));
            extract_done += 1;
            reporter
                .update(&format!(
                    "并行提取成交音频 {extract_done}/{chunk_count}"
                ))
                .await;
        }

        // Phase 2: ASR in parallel across FunASR worker processes.
        let asr_sem = Arc::new(Semaphore::new(deal_asr_concurrency));
        let mut asr_set = JoinSet::new();
        // FunASR correction is deliberately skipped here. AI analysis runs
        // after all windows are ready; per-window correction only adds wait.
        let asr_api_key = if generator_type == "funasr" {
            String::new()
        } else {
            openai_api_key.clone()
        };
        for (index, item) in extracted.into_iter().enumerate() {
            let (start_sec, segment_path) = item.ok_or_else(|| {
                format!("成交音频分段 {}/{} 提取结果缺失", index + 1, chunk_count)
            })?;
            let permit = asr_sem
                .clone()
                .acquire_owned()
                .await
                .map_err(|error| format!("成交 ASR 调度失败: {error}"))?;
            let generator_type = generator_type.clone();
            let whisper_model = whisper_model.clone();
            let whisper_prompt = whisper_prompt.clone();
            let openai_api_endpoint = openai_api_endpoint.clone();
            let language_hint = language_hint.clone();
            let asr_api_key = asr_api_key.clone();
            asr_set.spawn(async move {
                let _permit = permit;
                let result = ffmpeg::generate_video_subtitle(
                    None,
                    &segment_path,
                    &generator_type,
                    &whisper_model,
                    &whisper_prompt,
                    &asr_api_key,
                    &openai_api_endpoint,
                    &language_hint,
                )
                .await
                .map_err(|error| {
                    format!(
                        "并行转写成交音频 {}/{}（{start_sec:.0} 秒起）失败: {error}",
                        index + 1,
                        chunk_count
                    )
                })?;
                Ok::<(usize, f64, crate::subtitle_generator::GenerateResult), String>((
                    index, start_sec, result,
                ))
            });
        }

        let mut asr_results: Vec<Option<(f64, crate::subtitle_generator::GenerateResult)>> =
            vec![None; chunk_count];
        let mut asr_done = 0usize;
        while let Some(joined) = asr_set.join_next().await {
            let (index, start_sec, result) = joined
                .map_err(|error| format!("成交 ASR 任务异常: {error}"))??;
            asr_results[index] = Some((start_sec, result));
            asr_done += 1;
            reporter
                .update(&format!(
                    "并行转写成交音频 {asr_done}/{chunk_count}"
                ))
                .await;

            let mut ordered: Vec<(f64, &crate::subtitle_generator::GenerateResult)> = asr_results
                .iter()
                .filter_map(|item| item.as_ref().map(|(start, result)| (*start, result)))
                .collect();
            ordered.sort_by(|left, right| {
                left.0
                    .partial_cmp(&right.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut combined: Option<crate::subtitle_generator::GenerateResult> = None;
            for (start, result) in ordered {
                if let Some(output) = combined.as_mut() {
                    output.concat_with_offset_ms(result, (start * 1000.0).round() as u64);
                } else {
                    let mut output = crate::subtitle_generator::GenerateResult {
                        generator_type: result.generator_type.clone(),
                        subtitle_id: result.subtitle_id.clone(),
                        subtitle_content: Vec::new(),
                    };
                    output.concat_with_offset_ms(result, (start * 1000.0).round() as u64);
                    combined = Some(output);
                }
            }
            let partial_subtitle = combined
                .as_ref()
                .map(|output| {
                    output
                        .subtitle_content
                        .iter()
                        .map(item_to_srt)
                        .collect::<String>()
                })
                .unwrap_or_default();
            if !partial_subtitle.trim().is_empty() {
                tokio::fs::write(&partial_subtitle_path, &partial_subtitle)
                    .await
                    .map_err(|error| format!("保存部分成交窗口逐字稿失败: {error}"))?;
            }
        }

        let mut ordered_final: Vec<(f64, crate::subtitle_generator::GenerateResult)> = asr_results
            .into_iter()
            .flatten()
            .collect();
        ordered_final.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut combined: Option<crate::subtitle_generator::GenerateResult> = None;
        for (start_sec, result) in ordered_final {
            if let Some(output) = combined.as_mut() {
                output.concat_with_offset_ms(&result, (start_sec * 1000.0).round() as u64);
            } else {
                let mut output = crate::subtitle_generator::GenerateResult {
                    generator_type: result.generator_type.clone(),
                    subtitle_id: result.subtitle_id.clone(),
                    subtitle_content: Vec::new(),
                };
                output.concat_with_offset_ms(&result, (start_sec * 1000.0).round() as u64);
                combined = Some(output);
            }
        }

        let subtitle = combined
            .as_ref()
            .map(|result| {
                result
                    .subtitle_content
                    .iter()
                    .map(item_to_srt)
                    .collect::<String>()
            })
            .unwrap_or_default();
        if subtitle.trim().is_empty() {
            return Err(format!("{label}中没有识别到可用人声。"));
        }
        tokio::fs::write(&final_subtitle_path, &subtitle)
            .await
            .map_err(|error| format!("保存{label}逐字稿失败: {error}"))?;
        tokio::fs::write(
            &index_path,
            serde_json::to_vec_pretty(&ranges)
                .map_err(|error| format!("保存{label}索引失败: {error}"))?,
        )
        .await
        .map_err(|error| format!("保存{label}索引失败: {error}"))?;
        let _ = tokio::fs::remove_file(&partial_subtitle_path).await;

        if is_gap_fill {
            let deal = if deal_window_subtitle_path(&context).is_file() {
                tokio::fs::read_to_string(deal_window_subtitle_path(&context))
                    .await
                    .map_err(|error| format!("读取成交窗口逐字稿失败: {error}"))?
            } else {
                String::new()
            };
            let merged = merge_srt_documents(&deal, &subtitle)?;
            if merged.trim().is_empty() {
                return Err("合并成交窗与补转段后没有可用文稿。".to_string());
            }
            write_legacy_video_subtitle(&context.media_file, &merged).await?;
            return Ok(merged);
        }
        Ok(subtitle)
    }
    .await;
    if let Err(error) = tokio::fs::remove_dir_all(&temp_dir).await {
        log::warn!("清理成交窗口转写临时目录失败 {:?}: {error}", temp_dir);
    }

    let success_message = if is_gap_fill {
        "非成交段补转完成，已写入同一份文稿"
    } else {
        "成交窗口逐字稿生成完成"
    };
    match generation_result {
        Ok(subtitle) => {
            reporter.finish(true, success_message).await;
            state
                .db
                .update_task(
                    &event_id,
                    "success",
                    success_message,
                    Some(
                        json!({
                            "video_id": id,
                            "range_count": ranges.len(),
                            "audio_duration_sec": total_duration,
                            "service": generator_type,
                            "artifact_stem": stem,
                        })
                        .to_string()
                        .as_str(),
                    ),
                )
                .await?;
            Ok(subtitle)
        }
        Err(error) => {
            let failure = format!("{label}逐字稿生成失败: {error}");
            reporter.finish(false, &failure).await;
            state
                .db
                .update_task(&event_id, "failed", &failure, None)
                .await?;
            Err(error)
        }
    }
}

async fn generate_video_subtitle_inner(
    state: &State,
    event_id: String,
    id: i64,
) -> Result<String, String> {
    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "generate_video_subtitle".to_string(),
        status: "pending".to_string(),
        message: String::new(),
        metadata: json!({
            "video_id": id,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    if !state.db.add_subtitle_task_if_idle(&task, id).await? {
        return Err("该视频已有逐字稿任务正在进行，请等待当前任务完成后再试".to_string());
    }
    log::info!("Create task: {task:?}");
    reporter
        .update("媒体任务正在排队，避免同时运行转码和逐字稿识别")
        .await;
    let _media_permit = state
        .media_execution_gate
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "media execution gate is closed".to_string())?;

    let (generator_type, configured_model, config_path) = {
        let config = state.config.read().await;
        (
            config.subtitle_generator_type.clone(),
            config.whisper_model.clone(),
            config.config_path.clone(),
        )
    };
    if generator_type == "whisper" && !Path::new(&configured_model).is_file() {
        reporter
            .update("首次使用，正在自动准备本地转写模型...")
            .await;
        let model_path =
            crate::subtitle_generator::model_manager::ensure_model(&config_path, &configured_model)
                .await?;
        let mut config = state.config.write().await;
        config.whisper_model = model_path.to_string_lossy().to_string();
        config.save();
    }

    let config = state.config.read().await;
    let generator_type = config.subtitle_generator_type.clone();
    let whisper_model = config.whisper_model.clone();
    let whisper_prompt = config.whisper_prompt.clone();
    let openai_api_key = config.openai_api_key.clone();
    let openai_api_endpoint = config.openai_api_endpoint.clone();
    let volcengine_api_key = config.volcengine_api_key.clone();
    let volcengine_app_id = config.volcengine_app_id.clone();
    let volcengine_access_token = config.volcengine_access_token.clone();
    let volcengine_resource_id = config.volcengine_resource_id.clone();
    let volcengine_boosting_table_id = config.volcengine_boosting_table_id.clone();
    let volcengine_correct_table_id = config.volcengine_correct_table_id.clone();
    drop(config);
    let language_hint = state.config.read().await.whisper_language.clone();
    let language_hint = language_hint.as_str();

    let context = resolve_video_transcript_context(state, id).await?;
    let file = context.media_file.as_path();

    if generator_type == "volcengine" {
        let generation_result =
            generate_resumable_volcengine_video_subtitle(state, &context, &reporter).await;
        return match generation_result {
            Ok(canonical_subtitle) => {
                write_legacy_video_subtitle(file, &canonical_subtitle).await?;
                reporter.finish(true, "字幕生成完成").await;
                state
                    .db
                    .update_task(
                        &event_id,
                        "success",
                        "字幕生成完成",
                        Some(
                            json!({
                            "video_id": id,
                            "task_id": event_id,
                            "service": "volcengine",
                            })
                            .to_string()
                            .as_str(),
                        ),
                    )
                    .await?;
                Ok(canonical_subtitle)
            }
            Err(error) => {
                reporter
                    .finish(false, &format!("字幕生成失败: {error}"))
                    .await;
                state
                    .db
                    .update_task(&event_id, "failed", &format!("字幕生成失败: {error}"), None)
                    .await?;
                Err(error)
            }
        };
    }

    let generation_result = if generator_type == "volcengine" {
        match ffmpeg::generate_volcengine_video_subtitle(
            Some(&reporter),
            file,
            &volcengine_api_key,
            &volcengine_app_id,
            &volcengine_access_token,
            &volcengine_resource_id,
            &volcengine_boosting_table_id,
            &volcengine_correct_table_id,
        )
        .await
        {
            Ok(result) => Ok(result),
            Err(error) => {
                log::warn!("Volcengine ASR failed, falling back to FunASR: {error}");
                reporter
                    .update("火山引擎识别失败，正在自动切换本地识别...")
                    .await;
                ffmpeg::generate_video_subtitle(
                    Some(&reporter),
                    file,
                    "funasr",
                    &whisper_model,
                    &whisper_prompt,
                    &openai_api_key,
                    &openai_api_endpoint,
                    language_hint,
                )
                .await
            }
        }
    } else {
        ffmpeg::generate_video_subtitle(
            Some(&reporter),
            file,
            &generator_type,
            &whisper_model,
            &whisper_prompt,
            &openai_api_key,
            &openai_api_endpoint,
            language_hint,
        )
        .await
    };
    let generation_result = match generation_result {
        Ok(result) => {
            let subtitle = result
                .subtitle_content
                .iter()
                .map(item_to_srt)
                .collect::<String>();
            TranscriptArtifactStore::initialize_from_asr_outputs(
                context.source,
                context.artifact_dir,
                file,
                &subtitle,
                result.generator_type.as_str(),
            )
            .await
            .map(|bundle| (result, bundle.corrected_srt))
            .map_err(|error| format!("保存规范逐字稿失败: {error}"))
        }
        Err(error) => Err(error),
    };

    match generation_result {
        Ok((result, canonical_subtitle)) => {
            write_legacy_video_subtitle(file, &canonical_subtitle).await?;
            reporter.finish(true, "字幕生成完成").await;
            // for local whisper, we need to update the task status to success
            state
                .db
                .update_task(
                    &event_id,
                    "success",
                    "字幕生成完成",
                    Some(
                        json!({
                            "video_id": id,
                            "task_id": result.subtitle_id,
                            "service": result.generator_type.as_str(),
                        })
                        .to_string()
                        .as_str(),
                    ),
                )
                .await?;

            Ok(canonical_subtitle)
        }
        Err(e) => {
            reporter.finish(false, &format!("字幕生成失败: {e}")).await;
            state
                .db
                .update_task(&event_id, "failed", &format!("字幕生成失败: {e}"), None)
                .await?;
            Err(e)
        }
    }
}

async fn generate_resumable_volcengine_video_subtitle(
    state: &State,
    context: &CanonicalVideoTranscriptContext,
    reporter: &ProgressReporter,
) -> Result<String, String> {
    reporter
        .update("正在按 10 分钟分段调用火山引擎；中断后可再次点击继续")
        .await;
    let video = state.db.get_video(context.video_id).await?;
    let duration_ms = if video.length > 0 {
        u64::try_from(video.length)
            .map_err(|error| error.to_string())?
            .saturating_mul(1000)
    } else {
        ffmpeg::probe_media_duration_ms(&context.media_file).await?
    };
    let metadata = tokio::fs::metadata(&context.media_file)
        .await
        .map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let source_key = format!("video:{}", context.video_id);
    let source_hash = format!(
        "{:x}",
        md5::compute(format!(
            "{}\n{}\n{}\n{}",
            source_key,
            metadata.len(),
            modified,
            duration_ms
        ))
    );
    let config = state.config.read().await;
    let transcriber = VolcengineChunkTranscriber::configured(
        &context.media_file,
        context.artifact_dir.join("volcengine-video-chunks"),
        &config,
    )?;
    drop(config);
    let available_cards = state
        .db
        .list_asr_parameter_cards()
        .await
        .map_err(String::from)?
        .into_iter()
        .filter_map(parameter_card_from_record)
        .collect::<Vec<_>>();
    let selected_cards = select_video_asr_parameter_cards(
        &video.title,
        &[video.desc.clone(), video.tags.clone(), video.note.clone()],
        &available_cards,
    );
    let approved_replacements = approved_transcript_replacements(
        state
            .db
            .list_transcript_dictionary_candidates(Some("approved"))
            .await
            .map_err(String::from)?,
    );
    if let Err(error) = write_video_asr_card_audit(
        &context.artifact_dir,
        &source_key,
        &source_hash,
        &selected_cards,
    )
    .await
    {
        log::warn!("Unable to persist video ASR parameter-card audit: {error}");
    }
    let status = start_master_ingest(
        IngestRequest {
            source_id: context.video_id,
            source_key,
            source_hash,
            duration_ms,
            chunk_duration_ms: 600_000,
            selected_cards,
        },
        VideoCheckpointStore::new(state.db.clone()),
        transcriber,
        TranscriptArtifactSink::new(context.source.clone(), &context.artifact_dir)
            .with_approved_replacements(approved_replacements),
    )
    .await?;
    match status {
        IngestStatus::Complete { completed, total } => {
            reporter
                .update(&format!("火山引擎转写完成：{completed}/{total} 个分段"))
                .await;
            TranscriptArtifactStore::load_from_dir(context.source.clone(), &context.artifact_dir)
                .await
                .map(|bundle| bundle.corrected_srt)
                .map_err(|error| error.to_string())
        }
        IngestStatus::Failed {
            completed,
            total,
            failed_chunk,
            error,
        } => Err(format!(
            "火山引擎第 {} 段转写失败（已完成 {completed}/{total} 段）：{error}。请重新点击识别以从失败分段继续",
            failed_chunk + 1,
        )),
    }
}

fn select_video_asr_parameter_cards(
    title: &str,
    related_text: &[String],
    available_cards: &[ParameterCard],
) -> Vec<ParameterCard> {
    let recognized_terms = related_text
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let matched_cards = available_cards
        .iter()
        .filter(|card| parameter_card_exact_match(card, title, &recognized_terms))
        .cloned()
        .collect::<Vec<_>>();
    select_parameter_cards(
        title,
        &recognized_terms,
        &[],
        &matched_cards,
        MAX_PARAMETER_CARDS,
    )
    .cards
}

fn approved_transcript_replacements(
    candidates: Vec<TranscriptDictionaryCandidateRow>,
) -> Vec<ApprovedTranscriptReplacement> {
    candidates
        .into_iter()
        .filter(|candidate| {
            candidate.status == TranscriptDictionaryCandidateStatus::Approved
                && candidate.candidate_type == TranscriptDictionaryCandidateType::Replacement
                && !candidate.source_text.trim().is_empty()
                && !candidate.target_text.trim().is_empty()
                && candidate.source_text != candidate.target_text
        })
        .map(|candidate| ApprovedTranscriptReplacement {
            candidate_id: candidate.id,
            source_text: candidate.source_text,
            target_text: candidate.target_text,
        })
        .collect()
}

fn video_asr_card_audit_payload(
    source_key: &str,
    source_hash: &str,
    selected_cards: &[ParameterCard],
) -> serde_json::Value {
    json!({
        "sourceKey": source_key,
        "sourceHash": source_hash,
        "selectedCards": selected_cards.iter().map(|card| json!({
            "cardId": card.card_id,
            "version": card.version,
            "canonicalName": card.canonical_name,
            "aliases": card.aliases,
        })).collect::<Vec<_>>(),
    })
}

async fn write_video_asr_card_audit(
    directory: &Path,
    source_key: &str,
    source_hash: &str,
    selected_cards: &[ParameterCard],
) -> Result<(), String> {
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| error.to_string())?;
    let destination = directory.join("transcript.asr-context.json");
    let temporary = directory.join(".transcript.asr-context.json.tmp");
    let content = serde_json::to_vec_pretty(&video_asr_card_audit_payload(
        source_key,
        source_hash,
        selected_cards,
    ))
    .map_err(|error| error.to_string())?;
    tokio::fs::write(&temporary, content)
        .await
        .map_err(|error| error.to_string())?;
    if tokio::fs::try_exists(&destination)
        .await
        .map_err(|error| error.to_string())?
    {
        tokio::fs::remove_file(&destination)
            .await
            .map_err(|error| error.to_string())?;
    }
    tokio::fs::rename(temporary, destination)
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_video_subtitle(
    state: state_type!(),
    id: i64,
    subtitle: String,
) -> Result<(), String> {
    update_video_subtitle_inner(&state, id, subtitle).await
}

async fn update_video_subtitle_inner(
    state: &State,
    id: i64,
    subtitle: String,
) -> Result<(), String> {
    let context = resolve_video_transcript_context(state, id).await?;
    let file = context.media_file.as_path();
    let bundle = if TranscriptArtifactStore::canonical_artifacts_exist(&context.artifact_dir)
        .await
        .map_err(|error| error.to_string())?
    {
        TranscriptArtifactStore::apply_manual_edit(context.source, &context.artifact_dir, &subtitle)
            .await
            .map_err(|error| error.to_string())?
    } else {
        let legacy = match tokio::fs::read_to_string(file.with_extension("srt")).await {
            Ok(content) => Some(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if let Some(legacy) = legacy {
            let initialized = TranscriptArtifactStore::initialize(
                context.source.clone(),
                &context.artifact_dir,
                &legacy,
                &legacy,
                Vec::new(),
            )
            .await
            .map_err(|error| error.to_string())?;
            if initialized.corrected_srt == subtitle {
                initialized
            } else {
                TranscriptArtifactStore::apply_manual_edit(
                    context.source,
                    &context.artifact_dir,
                    &subtitle,
                )
                .await
                .map_err(|error| error.to_string())?
            }
        } else {
            TranscriptArtifactStore::initialize_manual_import(
                context.source,
                &context.artifact_dir,
                &subtitle,
            )
            .await
            .map_err(|error| error.to_string())?
        }
    };
    write_legacy_video_subtitle(file, &bundle.corrected_srt).await?;
    Ok(())
}

async fn write_legacy_video_subtitle(file: &Path, subtitle: &str) -> Result<(), String> {
    tokio::fs::write(file.with_extension("srt"), subtitle)
        .await
        .map_err(|error| format!("同步兼容字幕失败: {error}"))
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_video_note(state: state_type!(), id: i64, note: String) -> Result<(), String> {
    log::info!("Update video note: {id} -> {note}");
    let mut video = state.db.get_video(id).await?;
    video.note = note;
    state.db.update_video(&video).await?;
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn encode_video_subtitle(
    state: state_type!(),
    event_id: String,
    id: i64,
    srt_style: String,
) -> Result<VideoRow, String> {
    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "encode_video_subtitle".to_string(),
        status: "pending".to_string(),
        message: String::new(),
        metadata: json!({
            "video_id": id,
            "srt_style": srt_style,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_task(&task).await?;
    log::info!("Create task: {task:?}");
    match encode_video_subtitle_inner(&state, &reporter, id, srt_style).await {
        Ok(video) => {
            reporter.finish(true, "字幕编码完成").await;
            state
                .db
                .update_task(&event_id, "success", "字幕编码完成", None)
                .await?;
            Ok(video)
        }
        Err(e) => {
            reporter.finish(false, &format!("字幕编码失败: {e}")).await;
            state
                .db
                .update_task(&event_id, "failed", &format!("字幕编码失败: {e}"), None)
                .await?;
            Err(e)
        }
    }
}

async fn encode_video_subtitle_inner(
    state: &state_type!(),
    reporter: &ProgressReporter,
    id: i64,
    srt_style: String,
) -> Result<VideoRow, String> {
    let context = resolve_video_transcript_context(state, id).await?;
    let video = state.db.get_video(context.video_id).await?;
    let filepath = context.media_file;
    let subtitle_path = filepath.with_extension("srt");
    if TranscriptArtifactStore::canonical_artifacts_exist(&context.artifact_dir)
        .await
        .map_err(|error| error.to_string())?
    {
        let bundle = TranscriptArtifactStore::load_from_dir(context.source, context.artifact_dir)
            .await
            .map_err(|error| error.to_string())?;
        write_legacy_video_subtitle(&filepath, &bundle.corrected_srt).await?;
    }

    let output_filename =
        ffmpeg::encode_video_subtitle(reporter, &filepath, &subtitle_path, srt_style).await?;
    let output_filepath = filepath.with_file_name(&output_filename);
    let cover_path = filepath.with_extension("jpg");
    let output_cover_path = output_filepath.with_extension("jpg");

    if cover_path.exists() {
        tokio::fs::copy(&cover_path, &output_cover_path)
            .await
            .map_err(|e| {
                format!(
                    "复制字幕视频封面失败: {} -> {}: {e}",
                    cover_path.display(),
                    output_cover_path.display()
                )
            })?;
    } else {
        log::warn!(
            "Cover file not found for subtitle video: {}",
            cover_path.display()
        );
    }

    let new_video = state
        .db
        .add_video(&VideoRow {
            id: 0,
            status: video.status,
            room_id: video.room_id,
            created_at: Local::now().to_rfc3339(),
            cover: video.cover.clone(),
            file: output_filename,
            note: video.note.clone(),
            length: video.length,
            size: video.size,
            bvid: video.bvid.clone(),
            title: video.title.clone(),
            desc: video.desc.clone(),
            tags: video.tags.clone(),
            area: video.area,
            platform: video.platform,
            anchor_name: video.anchor_name,
            anchor_source: video.anchor_source,
            anchor_confidence: video.anchor_confidence,
            anchor_detection_status: video.anchor_detection_status,
            anchor_detection_error: video.anchor_detection_error,
            anchor_detected_at: video.anchor_detected_at,
        })
        .await?;

    Ok(new_video)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generic_ffmpeg_command(
    _state: state_type!(),
    args: Vec<String>,
) -> Result<String, String> {
    if !generic_ffmpeg_command_enabled() {
        log::warn!(
            "Rejected generic_ffmpeg_command because BSR_ENABLE_GENERIC_FFMPEG is not enabled"
        );
        return Err(
            "generic_ffmpeg_command is disabled by default. Use fixed FFmpeg tools instead."
                .to_string(),
        );
    }

    let args_str: Vec<&str> = args.iter().map(std::string::String::as_str).collect();
    ffmpeg::generic_ffmpeg_command(&args_str).await
}

fn generic_ffmpeg_command_enabled() -> bool {
    std::env::var("BSR_ENABLE_GENERIC_FFMPEG")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

#[cfg(test)]
mod p0_security_tests {
    use super::generic_ffmpeg_command_enabled;

    #[test]
    fn test_generic_ffmpeg_command_disabled_by_default() {
        std::env::remove_var("BSR_ENABLE_GENERIC_FFMPEG");
        assert!(!generic_ffmpeg_command_enabled());
    }

    #[test]
    fn test_generic_ffmpeg_command_requires_explicit_enable() {
        std::env::set_var("BSR_ENABLE_GENERIC_FFMPEG", "1");
        assert!(generic_ffmpeg_command_enabled());
        std::env::set_var("BSR_ENABLE_GENERIC_FFMPEG", "false");
        assert!(!generic_ffmpeg_command_enabled());
        std::env::remove_var("BSR_ENABLE_GENERIC_FFMPEG");
    }
}

#[cfg(test)]
mod import_video_compatibility_tests {
    use super::{
        can_remux_for_browser_playback, import_conversion_strategy,
        is_browser_playable_video_codec, playback_conversion_kind, playback_copy_is_ready,
        requires_browser_playback_copy, should_convert_video_format, should_force_h264_reencode,
        ImportConversionStrategy, PlaybackConversionKind,
    };
    use crate::ffmpeg::VideoMetadata;

    #[test]
    fn transport_stream_imports_require_browser_compatible_conversion() {
        assert!(should_convert_video_format("ts"));
        assert!(should_convert_video_format("m2ts"));
        assert!(should_convert_video_format("mts"));
        assert!(!should_convert_video_format("mp4"));
    }

    #[test]
    fn browser_playback_accepts_h264_codecs_only() {
        assert!(is_browser_playable_video_codec("h264"));
        assert!(is_browser_playable_video_codec("avc1"));
        assert!(!is_browser_playable_video_codec("hevc"));
        assert!(!is_browser_playable_video_codec("h265"));
    }

    #[test]
    fn h264_aac_transport_stream_uses_fast_remux_instead_of_reencoding() {
        assert!(can_remux_for_browser_playback(&VideoMetadata {
            duration: 60.0,
            width: 1920,
            height: 1080,
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
        }));
        assert!(!can_remux_for_browser_playback(&VideoMetadata {
            duration: 60.0,
            width: 1920,
            height: 1080,
            video_codec: "hevc".to_string(),
            audio_codec: "aac".to_string(),
        }));
    }

    #[test]
    fn h264_aac_playback_uses_fast_remux() {
        let metadata = VideoMetadata {
            duration: 60.0,
            width: 1920,
            height: 1080,
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
        };

        assert_eq!(
            playback_conversion_kind(&metadata),
            PlaybackConversionKind::FastRemux
        );
    }

    #[test]
    fn transport_streams_must_not_use_lossless_stream_copy() {
        assert!(should_force_h264_reencode("ts"));
        assert!(should_force_h264_reencode("m2ts"));
        assert!(should_force_h264_reencode("mts"));
        assert!(!should_force_h264_reencode("flv"));
        assert!(!should_force_h264_reencode("mp4"));
    }

    #[test]
    fn transport_stream_imports_stay_original_until_playback_is_requested() {
        assert_eq!(
            import_conversion_strategy("ts"),
            ImportConversionStrategy::CopyOnly
        );
        assert!(requires_browser_playback_copy("recording.ts"));
        assert_eq!(
            import_conversion_strategy("mp4"),
            ImportConversionStrategy::CopyOnly
        );
    }

    #[test]
    fn unfinished_playback_copy_is_never_sent_to_the_player() {
        assert!(!playback_copy_is_ready(true, Some("pending")));
        assert!(!playback_copy_is_ready(true, Some("processing")));
        assert!(!playback_copy_is_ready(true, Some("failed")));
        assert!(!playback_copy_is_ready(true, Some("interrupted")));
        assert!(!playback_copy_is_ready(true, Some("cancelled")));
        assert!(playback_copy_is_ready(true, Some("success")));
        assert!(playback_copy_is_ready(true, None));
        assert!(!playback_copy_is_ready(false, Some("success")));
    }
}

#[cfg(test)]
mod video_asr_parameter_card_tests {
    use super::{
        approved_transcript_replacements, select_video_asr_parameter_cards,
        video_asr_card_audit_payload,
    };
    use master_ingest::ParameterCard;

    fn card(id: &str, name: &str, aliases: &[&str]) -> ParameterCard {
        ParameterCard {
            card_id: id.into(),
            version: "1.0".into(),
            canonical_name: name.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
            context: String::new(),
        }
    }

    #[test]
    fn video_asr_cards_prioritize_titles_and_metadata_terms() {
        let cards = vec![
            card("R5", "佳能 R5", &[]),
            card("R50", "佳能 R50", &["R50 白色"]),
        ];
        let selected = select_video_asr_parameter_cards(
            "佳能 R50 99 新直播",
            &["白色现货".into(), "EF 70-200 套装".into()],
            &cards,
        );

        assert_eq!(
            selected
                .iter()
                .map(|card| card.card_id.as_str())
                .collect::<Vec<_>>(),
            vec!["R50"]
        );
    }

    #[test]
    fn video_asr_card_audit_identifies_the_exact_card_versions_used() {
        let payload = video_asr_card_audit_payload(
            "video:9",
            "media-hash",
            &[card("R50", "佳能 R50", &["R50 白色"])],
        );

        assert_eq!(payload["sourceKey"], "video:9");
        assert_eq!(payload["selectedCards"][0]["cardId"], "R50");
        assert_eq!(payload["selectedCards"][0]["version"], "1.0");
    }

    #[test]
    fn only_approved_replacement_candidates_become_transcript_rules() {
        use crate::database::transcript_dictionary_candidate::{
            TranscriptDictionaryCandidateRow, TranscriptDictionaryCandidateStatus,
            TranscriptDictionaryCandidateType,
        };

        let candidates = vec![
            TranscriptDictionaryCandidateRow {
                id: 1,
                candidate_type: TranscriptDictionaryCandidateType::Replacement,
                source_text: "A4PRO299".into(),
                target_text: "A4 PRO 2 99 new".into(),
                evidence_json: "[]".into(),
                source_json: "{}".into(),
                status: TranscriptDictionaryCandidateStatus::Approved,
                created_at: String::new(),
                updated_at: String::new(),
            },
            TranscriptDictionaryCandidateRow {
                id: 2,
                candidate_type: TranscriptDictionaryCandidateType::Hotword,
                source_text: "ignored".into(),
                target_text: String::new(),
                evidence_json: "[]".into(),
                source_json: "{}".into(),
                status: TranscriptDictionaryCandidateStatus::Approved,
                created_at: String::new(),
                updated_at: String::new(),
            },
        ];

        let replacements = approved_transcript_replacements(candidates);
        assert_eq!(replacements.len(), 1);
        assert_eq!(replacements[0].candidate_id, 1);
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn import_external_video(
    state: state_type!(),
    event_id: String,
    file_path: String,
    title: String,
    room_id: String,
    analysis_purpose: Option<String>,
    competitor_name: Option<String>,
    master_script_key: Option<String>,
) -> Result<VideoRow, String> {
    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());

    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;

    let source_path = Path::new(&file_path);
    if !source_path.exists() {
        return Err("文件不存在".to_string());
    }

    reporter.update("正在提取视频元数据...").await;
    let metadata = ffmpeg::extract_video_metadata(source_path).await?;
    let output_str = state.config.read().await.output.clone();
    let output_dir = Path::new(&output_str);
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    }

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("mp4");
    let mut target_filename = format!(
        "{}{}{}.{}",
        crate::constants::PREFIX_IMPORTED,
        sanitize_filename(&title),
        timestamp,
        extension
    );
    let target_full_path = output_dir.join(&target_filename);

    let conversion_strategy = import_conversion_strategy(extension);
    let need_conversion = !matches!(conversion_strategy, ImportConversionStrategy::CopyOnly);
    let final_target_full_path = if need_conversion {
        let mp4_target_full_path = target_full_path.with_extension("mp4");

        reporter.update("准备转换为浏览器兼容 MP4...").await;

        copy_and_convert_with_progress(source_path, &mp4_target_full_path, true, false, &reporter)
            .await?;

        // 更新最终文件名和路径
        target_filename = mp4_target_full_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        mp4_target_full_path
    } else {
        // 其他格式使用智能拷贝
        copy_and_convert_with_progress(source_path, &target_full_path, false, false, &reporter)
            .await?;
        target_full_path
    };

    // 步骤3: 生成缩略图
    reporter.update("正在生成视频缩略图...").await;

    // 生成缩略图，使用智能时间点选择
    let thumbnail_timestamp = get_optimal_thumbnail_timestamp(metadata.duration);
    let cover_path =
        match ffmpeg::generate_thumbnail(&final_target_full_path, thumbnail_timestamp).await {
            Ok(path) => path.file_name().unwrap().to_str().unwrap().to_string(),
            Err(e) => {
                log::warn!("生成缩略图失败: {e}");
                String::new() // 使用空字符串，前端会显示默认图标
            }
        };

    // 步骤4: 保存到数据库
    reporter.update("正在保存视频信息...").await;

    let Ok(size) = i64::try_from(
        final_target_full_path
            .metadata()
            .map_err(|e| e.to_string())?
            .len(),
    ) else {
        log::error!(
            "Failed to convert metadata length to i64: {}",
            final_target_full_path
                .metadata()
                .map_err(|e| e.to_string())?
                .len()
        );
        return Err("Failed to convert metadata length to i64".to_string());
    };

    // 添加到数据库
    let analysis_purpose = if analysis_purpose.as_deref() == Some("competitor_benchmark") {
        "competitor_benchmark"
    } else {
        "enterprise_review"
    };
    let source_file_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let inferred_live_started_at =
        crate::live_dashboard_binding::infer_live_started_at_from_texts(&[
            source_file_name,
            &title,
        ])
        .unwrap_or_default();
    let note = json!({
        "analysisPurpose": analysis_purpose,
        "competitorName": competitor_name.unwrap_or_default().trim(),
        "masterScriptKey": master_script_key.unwrap_or_default().trim(),
        "sourceFileName": source_file_name,
        "inferredLiveStartedAt": inferred_live_started_at,
    })
    .to_string();
    let video = VideoRow {
        id: 0,
        room_id, // 使用传入的 room_id
        platform: "imported".to_string(),
        title,
        file: target_filename,
        note,
        length: metadata.duration as i64,
        size,
        status: 1, // 导入完成
        cover: cover_path,
        desc: String::new(),
        tags: String::new(),
        bvid: String::new(),
        area: 0,
        created_at: Utc::now().to_rfc3339(),
        anchor_name: String::new(),
        anchor_source: String::new(),
        anchor_confidence: String::new(),
        anchor_detection_status: "pending".to_string(),
        anchor_detection_error: String::new(),
        anchor_detected_at: String::new(),
    };

    let result = state.db.add_video(&video).await?;

    if let Err(error) = state
        .nas_archive
        .enqueue(result.id, "import", &final_target_full_path)
        .await
    {
        log::error!("导入视频加入 NAS 转存队列失败：{error}");
    }

    // 完成进度通知
    reporter.finish(true, "视频导入完成").await;

    // 发送通知消息
    state
        .db
        .new_message("视频导入完成", &format!("成功导入视频：{}", result.title))
        .await?;

    log::info!("导入视频成功: {} -> {}", file_path, result.file);
    Ok(result)
}

// 通用视频切片函数（支持所有类型的视频）
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DealAutoClipRangeRequest {
    pub start: f64,
    pub end: f64,
    pub title: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DealAutoClipFailure {
    index: usize,
    title: String,
    start: f64,
    end: f64,
    error: String,
}

fn validate_clip_time_range(start: f64, end: f64, video_duration: i64) -> Result<(), String> {
    if !(start.is_finite() && end.is_finite() && start >= 0.0 && end > start) {
        return Err(format!("切片时间范围无效：{start:.3}s - {end:.3}s"));
    }
    // Imported media occasionally reports a duration one or two seconds shorter
    // than the final decodable timestamp. Allow a small tail tolerance, but stop
    // obviously mismatched timelines before FFmpeg starts.
    if video_duration > 0 && start >= video_duration as f64 + 3.0 {
        return Err(format!(
            "切片起点 {start:.1}s 已超出原视频时长 {}s，请检查视频与订单时间轴是否匹配",
            video_duration
        ));
    }
    Ok(())
}

async fn resolve_clip_source_path(
    state: &State,
    parent_video: &VideoRow,
) -> Result<PathBuf, String> {
    let output = state.config.read().await.output.clone();
    let input_path = resolve_external_playback_path(
        state.db.as_ref(),
        Path::new(&output),
        parent_video.id,
        &parent_video.file,
    )
    .await
    .map_err(|error| {
        format!(
            "无法定位切片原视频（视频 ID {}，记录路径 {}）：{error}。请确认原视频仍在本地，或重新连接 NAS 后重试",
            parent_video.id, parent_video.file
        )
    })?;

    let metadata = tokio::fs::metadata(&input_path).await.map_err(|error| {
        format!(
            "切片原视频当前无法读取：{}（{error}）。请检查文件是否被移动、删除或 NAS 是否断开",
            input_path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "切片源不是有效的视频文件：{}",
            input_path.display()
        ));
    }
    if metadata.len() == 0 {
        return Err(format!("切片原视频是空文件：{}", input_path.display()));
    }
    tokio::fs::File::open(&input_path).await.map_err(|error| {
        format!(
            "切片原视频存在但无法打开：{}（{error}）。请关闭占用程序或检查共享目录权限",
            input_path.display()
        )
    })?;
    Ok(input_path)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealAutoClipQueueResult {
    pub task_id: String,
    pub count: usize,
}

/// Queue all AI-selected deal ranges as one backend job. The frontend returns
/// immediately and TaskManager runs the FFmpeg work serially, preventing tens
/// of simultaneous Tauri invokes and GPU encoder contention.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn queue_deal_auto_clips(
    state: state_type!(),
    event_id: String,
    parent_video_id: i64,
    ranges: Vec<DealAutoClipRangeRequest>,
) -> Result<DealAutoClipQueueResult, String> {
    if ranges.is_empty() {
        return Err("没有可切片的成交话术区间。".to_string());
    }
    if ranges.len() > 100 {
        return Err("单次自动切片最多支持 100 个区间。".to_string());
    }
    let parent_video = state.db.get_video(parent_video_id).await?;
    for (index, range) in ranges.iter().enumerate() {
        validate_clip_time_range(range.start, range.end, parent_video.length)
            .map_err(|error| format!("第 {} 个切片无效：{error}", index + 1))?;
    }
    // Preflight once before creating the background task. A moved local file or
    // disconnected NAS must produce one actionable error, not dozens of failed
    // clip records for the same missing source.
    let source_path = resolve_clip_source_path(&state, &parent_video).await?;
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "deal_auto_clip_batch".to_string(),
        status: "pending".to_string(),
        message: format!("等待生成 {} 条完整成交链路视频", ranges.len()),
        metadata: json!({
            "parent_video_id": parent_video_id,
            "range_count": ranges.len(),
            "source_path": source_path.to_string_lossy(),
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_task(&task).await?;

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;
    #[cfg(feature = "gui")]
    let state_clone = (*state).clone();
    #[cfg(feature = "headless")]
    let state_clone = state.clone();
    let worker_task_id = event_id.clone();
    let range_count = ranges.len();

    let queued = state
        .task_manager
        .add_task(Task::new(
            event_id.clone(),
            TaskPriority::Normal,
            async move {
                let _media_permit = state_clone
                    .media_execution_gate
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| "媒体任务执行通道已关闭。".to_string())?;
                // The TS preview and the final clip both use the H.264 hardware
                // encoder. Keep the already-written HLS files available to the UI,
                // but stop its encoder before starting the final MP4 job so AMD
                // drivers are not asked to run two long sessions concurrently.
                stop_video_playback_preview_inner(&state_clone, parent_video.id, false).await;
                let mut succeeded = 0usize;
                let mut failures = Vec::new();
                let mut generated_clips = Vec::new();
                for (index, range) in ranges.into_iter().enumerate() {
                    reporter
                        .update(&format!(
                            "正在生成完整成交链路视频 {}/{}：{}",
                            index + 1,
                            range_count,
                            range.title
                        ))
                        .await;
                    match clip_video_inner(
                        &state_clone,
                        &reporter,
                        parent_video.clone(),
                        range.start,
                        range.end,
                        range.title.clone(),
                    )
                    .await
                    {
                        Ok(video) => {
                            succeeded += 1;
                            generated_clips.push(json!({
                                "index": index,
                                "video_id": video.id,
                                "title": video.title,
                                "start": range.start,
                                "end": range.end,
                            }));
                            let partial_metadata = json!({
                                "parent_video_id": parent_video.id,
                                "range_count": range_count,
                                "generated_clips": generated_clips,
                                "failed_clips": failures,
                            })
                            .to_string();
                            let _ = state_clone
                                .db
                                .update_task(
                                    &worker_task_id,
                                    "processing",
                                    &format!(
                                        "已生成 {succeeded}/{range_count} 条完整成交链路视频，后台继续切片"
                                    ),
                                    Some(&partial_metadata),
                                )
                                .await;
                            #[cfg(feature = "gui")]
                            {
                                let _ = state_clone.app_handle.emit(
                                    "deal-auto-clip-ready",
                                    json!({
                                        "taskId": worker_task_id,
                                        "parentVideoId": parent_video.id,
                                        "index": index,
                                        "videoId": video.id,
                                        "title": video.title,
                                        "start": range.start,
                                        "end": range.end,
                                        "succeeded": succeeded,
                                        "rangeCount": range_count,
                                    }),
                                );
                            }
                        }
                        Err(error) => {
                            log::error!("Deal auto clip failed for {}: {error}", range.title);
                            failures.push(DealAutoClipFailure {
                                index,
                                title: range.title,
                                start: range.start,
                                end: range.end,
                                error,
                            });
                        }
                    }
                }

                if succeeded == 0 {
                    let first_error = failures
                        .first()
                        .map(|failure| failure.error.as_str())
                        .unwrap_or("未知错误");
                    let message = format!(
                        "完整成交链路视频全部生成失败（共 {} 条）。首条原因：{}",
                        range_count, first_error
                    );
                    reporter.finish(false, &message).await;
                    let failed_metadata = json!({
                        "parent_video_id": parent_video.id,
                        "range_count": range_count,
                        "generated_video_ids": Vec::<i64>::new(),
                        "generated_clips": Vec::<serde_json::Value>::new(),
                        "failed_clips": failures,
                    })
                    .to_string();
                    state_clone
                        .db
                        .update_task(&worker_task_id, "failed", &message, Some(&failed_metadata))
                        .await?;
                    return Err(message);
                }
                let message = if failures.is_empty() {
                    format!("完整成交链路视频生成完成：成功 {succeeded}/{range_count} 条")
                } else {
                    format!(
                        "完整成交链路视频生成完成：成功 {succeeded}/{range_count} 条，失败 {} 条",
                        failures.len()
                    )
                };
                reporter.finish(true, &message).await;
                let completed_metadata = json!({
                    "parent_video_id": parent_video.id,
                    "range_count": range_count,
                    "generated_video_ids": generated_clips
                        .iter()
                        .filter_map(|clip| clip.get("video_id").and_then(|value| value.as_i64()))
                        .collect::<Vec<_>>(),
                    "generated_clips": generated_clips,
                    "failed_clips": failures,
                })
                .to_string();
                state_clone
                    .db
                    .update_task(
                        &worker_task_id,
                        "success",
                        &message,
                        Some(&completed_metadata),
                    )
                    .await?;
                Ok(())
            },
        ))
        .await;

    if let Err(error) = queued {
        state
            .db
            .update_task(
                &event_id,
                "failed",
                &format!("无法排队自动切片：{error}"),
                None,
            )
            .await?;
        return Err(error);
    }
    Ok(DealAutoClipQueueResult {
        task_id: event_id,
        count: range_count,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn clip_video(
    state: state_type!(),
    event_id: String,
    parent_video_id: i64,
    start_time: f64,
    end_time: f64,
    clip_title: String,
) -> Result<VideoRow, String> {
    // 获取父视频信息
    let parent_video = state.db.get_video(parent_video_id).await?;
    validate_clip_time_range(start_time, end_time, parent_video.length)?;
    // Return a clear error to the caller before creating a task record or
    // waiting for the shared media execution gate.
    resolve_clip_source_path(&state, &parent_video).await?;

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;

    // 创建任务记录
    let task = TaskRow {
        id: event_id.clone(),
        task_type: "clip_video".to_string(),
        status: "pending".to_string(),
        message: String::new(),
        metadata: json!({
            "parent_video_id": parent_video_id,
            "start_time": start_time,
            "end_time": end_time,
            "clip_title": clip_title,
        })
        .to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_task(&task).await?;

    let _media_permit = state
        .media_execution_gate
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "媒体任务执行通道已关闭。".to_string())?;

    match clip_video_inner(
        &state,
        &reporter,
        parent_video,
        start_time,
        end_time,
        clip_title,
    )
    .await
    {
        Ok(video) => {
            reporter.finish(true, "切片完成").await;
            state
                .db
                .update_task(&event_id, "success", "切片完成", None)
                .await?;
            Ok(video)
        }
        Err(e) => {
            reporter.finish(false, &format!("切片失败: {e}")).await;
            state
                .db
                .update_task(&event_id, "failed", &format!("切片失败: {e}"), None)
                .await?;
            Err(e)
        }
    }
}

async fn clip_video_inner(
    state: &State,
    reporter: &ProgressReporter,
    parent_video: VideoRow,
    start_time: f64,
    end_time: f64,
    clip_title: String,
) -> Result<VideoRow, String> {
    validate_clip_time_range(start_time, end_time, parent_video.length)?;
    let output = state.config.read().await.output.clone();
    // Resolve local output-relative files and archived NAS paths through the
    // same checked path used by queue preflight and single-clip calls.
    let input_path = resolve_clip_source_path(state, &parent_video).await?;

    // 统一的输出目录：clips
    let output_dir = Path::new(&output).join("clips");
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| format!("无法创建切片目录 {}：{e}", output_dir.display()))?;
    }

    let timestamp = Local::now().format("%Y%m%d%H%M%S%3f").to_string();

    // 获取原文件名（不含扩展名）；切片统一输出 mp4，便于列表播放
    let original_filename = sanitize_filename(
        input_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("video")
            .trim_start_matches("[imported]")
            .trim_start_matches("[clip]")
            .trim(),
    );
    let original_filename = if original_filename.is_empty() {
        "video".to_string()
    } else {
        original_filename
    };

    // 生成新的文件名格式：[clip]原文件名[时间戳].mp4
    let output_filename = format!(
        "{}{}[{}].mp4",
        crate::constants::PREFIX_CLIP,
        original_filename,
        timestamp,
    );
    let output_full_path = output_dir.join(&output_filename);

    log::info!(
        "开始切片：{} ({}s-{}s) -> {}",
        input_path.display(),
        start_time,
        end_time,
        output_full_path.display()
    );

    // 执行切片
    reporter.update("开始切片处理").await;
    if let Err(error) = ffmpeg::clip_from_video_file(
        Some(reporter),
        &input_path,
        &output_full_path,
        start_time,
        end_time - start_time,
    )
    .await
    {
        let _ = tokio::fs::remove_file(&output_full_path).await;
        return Err(error);
    }

    // 生成缩略图文件名，确保路径安全
    let thumbnail_full_path = output_full_path.with_extension("jpg");

    // 生成缩略图，选择切片开头的合理位置
    let clip_duration = end_time - start_time;
    let clip_thumbnail_timestamp = get_optimal_thumbnail_timestamp(clip_duration);
    let clip_cover_path =
        match ffmpeg::generate_thumbnail(&output_full_path, clip_thumbnail_timestamp).await {
            Ok(_) => thumbnail_full_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            Err(e) => {
                log::warn!("生成切片缩略图失败: {e}");
                String::new() // 使用空字符串，前端会显示默认图标
            }
        };

    let file_metadata = output_full_path.metadata().map_err(|e| {
        format!(
            "切片文件写入后无法读取（{}）：{e}",
            output_full_path.display()
        )
    })?;

    let clip_video = VideoRow {
        id: 0,
        room_id: parent_video.room_id,
        platform: "clip".to_string(),
        title: clip_title,
        file: format!("clips/{output_filename}"),
        note: String::new(),
        length: (end_time - start_time) as i64,
        size: i64::try_from(file_metadata.len()).map_err(|e| e.to_string())?,
        status: 1,
        cover: if clip_cover_path.is_empty() {
            String::new()
        } else {
            format!("clips/{clip_cover_path}")
        },
        desc: String::new(),
        tags: String::new(),
        bvid: String::new(),
        area: parent_video.area,
        created_at: Local::now().to_rfc3339(),
        anchor_name: parent_video.anchor_name,
        anchor_source: parent_video.anchor_source,
        anchor_confidence: parent_video.anchor_confidence,
        anchor_detection_status: parent_video.anchor_detection_status,
        anchor_detection_error: parent_video.anchor_detection_error,
        anchor_detected_at: parent_video.anchor_detected_at,
    };

    let result = state.db.add_video(&clip_video).await?;

    // 发送通知消息
    state
        .db
        .new_message("视频切片完成", &format!("生成切片：{}", result.title))
        .await?;

    Ok(result)
}

// 获取文件大小
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_file_size(file_path: String) -> Result<u64, String> {
    let path = Path::new(&file_path);
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(e) => Err(format!("无法获取文件信息: {e}")),
    }
}

// 辅助函数：清理文件名
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .chars()
        .take(50) // 限制长度
        .collect()
}

/// 批量导入结果结构
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BatchImportResult {
    pub successful_imports: i32,
    pub failed_imports: i32,
    pub imported_video_ids: Vec<i64>,
    pub errors: Vec<String>,
}

/// 批量导入外部视频文件
///
/// # 参数
/// - `state`: 应用状态
/// - `event_id`: 进度事件ID
/// - `file_paths`: 要导入的文件路径列表
/// - `room_id`: 房间ID
///
/// # 返回值
/// 返回批量导入结果，包含成功数量、失败数量、视频ID列表和错误信息
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn batch_import_external_videos(
    state: state_type!(),
    event_id: String,
    file_paths: Vec<String>,
    room_id: String,
) -> Result<BatchImportResult, String> {
    if file_paths.is_empty() {
        return Ok(BatchImportResult {
            successful_imports: 0,
            failed_imports: 0,
            imported_video_ids: Vec::new(),
            errors: Vec::new(),
        });
    }

    let mut successful_imports = 0;
    let mut failed_imports = 0;
    let mut imported_video_ids = Vec::new();
    let mut errors = Vec::new();

    // 设置批量进度事件发射器
    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let batch_reporter = ProgressReporter::new(state.db.clone(), &emitter, &event_id).await?;

    let total_files = file_paths.len();

    for (index, file_path) in file_paths.iter().enumerate() {
        let current_index = index + 1;
        let file_name = Path::new(file_path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 更新批量进度，只显示进度信息
        batch_reporter
            .update(&format!(
                "正在导入第{current_index}个，共{total_files}个文件"
            ))
            .await;

        // 为每个文件创建独立的事件ID
        let file_event_id = format!("{event_id}_file_{index}");

        // 从文件名生成标题（去掉扩展名）
        let title = file_name.clone();

        // 调用现有的单文件导入函数
        match import_external_video(
            state.clone(),
            file_event_id,
            file_path.clone(),
            title,
            room_id.clone(),
            None,
            None,
            None,
        )
        .await
        {
            Ok(video) => {
                imported_video_ids.push(video.id);
                successful_imports += 1;
                log::info!("批量导入成功: {} (ID: {})", file_path, video.id);
            }
            Err(e) => {
                let error_msg = format!("导入失败 {file_path}: {e}");
                errors.push(error_msg.clone());
                failed_imports += 1;
                log::error!("批量导入失败: {error_msg}");
            }
        }
    }

    // 完成批量导入
    let result_msg = if failed_imports == 0 {
        format!("批量导入完成：成功导入{successful_imports}个文件")
    } else {
        format!("批量导入完成：成功{successful_imports}个，失败{failed_imports}个")
    };
    batch_reporter
        .finish(failed_imports == 0, &result_msg)
        .await;

    // 发送通知消息
    state
        .db
        .new_message("批量视频导入完成", &result_msg)
        .await?;

    Ok(BatchImportResult {
        successful_imports,
        failed_imports,
        imported_video_ids,
        errors,
    })
}

// 查询当前导入进度
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_import_progress(
    state: state_type!(),
) -> Result<Option<serde_json::Value>, String> {
    // 查询进行中的FLV转换任务
    let all_tasks = state.db.get_tasks().await.map_err(|e| e.to_string())?;

    // 查找状态为 "pending" 或 "running" 的 import_flv_conversion 任务
    for task in &all_tasks {
        if task.task_type == "import_flv_conversion"
            && (task.status == "pending" || task.status == "running")
        {
            // 解析任务元数据
            let metadata: serde_json::Value =
                serde_json::from_str(&task.metadata).unwrap_or_default();

            return Ok(Some(serde_json::json!({
                "task_id": task.id,
                "file_name": metadata.get("file_name").and_then(|v| v.as_str()).unwrap_or("未知文件"),
                "file_size": metadata.get("file_size").and_then(serde_json::Value::as_u64).unwrap_or(0),
                "message": task.message,
                "status": task.status,
                "created_at": task.created_at
            })));
        }
    }

    Ok(None)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_audio_sample(state: state_type!(), video_id: i64) -> Result<(), String> {
    let video = state.db.get_video(video_id).await?;
    let video_path = Path::new(&state.config.read().await.output).join(&video.file);
    let opus_path = video_path.with_extension("opus");
    if !opus_path.exists() {
        let _ = crate::ffmpeg::extract_audio_sample(&video_path).await?;
    }
    Ok(())
}

#[cfg(test)]
mod delete_file_tests {
    use super::{
        normalize_product_scan_text, remove_required_media_file, should_delete_media_file,
        sorted_live_product_scan_items, validate_clip_time_range,
        validate_live_product_scan_catalog, validate_video_transcript_coverage,
        LiveProductScanProduct,
    };

    #[test]
    fn archived_video_is_kept_unless_nas_deletion_is_explicit() {
        assert!(!should_delete_media_file(1, true, false));
        assert!(should_delete_media_file(1, true, true));
        assert!(!should_delete_media_file(2, true, true));
        assert!(should_delete_media_file(1, false, false));
    }

    #[tokio::test]
    async fn missing_media_file_is_already_deleted() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-missing-video-{}.mp4",
            uuid::Uuid::new_v4()
        ));

        assert!(remove_required_media_file(&path).await.is_ok());
    }

    #[tokio::test]
    async fn media_delete_failure_is_returned_to_the_caller() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-video-directory-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&path).unwrap();

        let error = remove_required_media_file(&path).await.unwrap_err();

        assert!(error.contains("无法删除视频文件"));
        std::fs::remove_dir(&path).unwrap();
    }

    #[test]
    fn rejects_a_legacy_transcript_that_only_covers_the_first_few_minutes() {
        let subtitle = concat!(
            "1\n00:00:00,000 --> 00:00:03,000\n开场\n\n",
            "2\n00:06:20,000 --> 00:06:23,000\n最后一句\n\n"
        );

        let error = validate_video_transcript_coverage(subtitle, 4_532_000).unwrap_err();

        assert!(error.contains("01:15:32"));
        assert!(error.contains("00:06:23"));
    }

    #[test]
    fn accepts_a_transcript_with_a_short_quiet_tail() {
        let subtitle = "1\n01:13:41,000 --> 01:13:45,000\n感谢观看\n\n";

        assert!(validate_video_transcript_coverage(subtitle, 4_532_000).is_ok());
    }

    #[test]
    fn clip_time_preflight_rejects_invalid_and_mismatched_ranges() {
        assert!(validate_clip_time_range(10.0, 20.0, 120).is_ok());
        assert!(validate_clip_time_range(10.0, 10.0, 120)
            .unwrap_err()
            .contains("时间范围无效"));
        assert!(validate_clip_time_range(130.0, 140.0, 120)
            .unwrap_err()
            .contains("时间轴是否匹配"));
    }

    #[test]
    fn normalizes_and_counts_product_scan_catalog_without_mutating_orders() {
        assert_eq!(
            normalize_product_scan_text("索尼 70-200 GM II"),
            "索尼70200gmii"
        );
        let catalog = validate_live_product_scan_catalog(&[LiveProductScanProduct {
            id: "sony-70200".to_string(),
            name: "索尼 70-200 GM II".to_string(),
            aliases: vec!["70-200".to_string(), " 70 200 ".to_string()],
        }])
        .unwrap();
        assert_eq!(catalog[0].aliases, vec!["70200"]);

        let ranked = sorted_live_product_scan_items(&catalog, &[12]);
        assert_eq!(ranked[0].mention_count, 12);
        assert_eq!(ranked[0].name, "索尼 70-200 GM II");
    }
}
