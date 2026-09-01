use crate::config::Config;
use crate::subtitle_generator::{item_to_srt, GenerateResult, SubtitleGeneratorType};
use m3u8_rs::{MediaPlaylist, MediaPlaylistType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex, RwLock, Semaphore};

const TARGET_CHUNK_MS: u64 = 120_000;
const PLAYLIST_POLL_INTERVAL: Duration = Duration::from_secs(15);
const RETRY_INTERVAL: Duration = Duration::from_secs(20);
const FINALIZATION_TIMEOUT: Duration = Duration::from_secs(180);
const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LiveTranscriptStatus {
    Waiting {
        message: String,
    },
    Transcribing {
        completed_chunks: usize,
        processed_duration_ms: u64,
        message: String,
    },
    Finalized {
        completed_chunks: usize,
        processed_duration_ms: u64,
        subtitle_path: String,
    },
    Failed {
        message: String,
    },
}

impl LiveTranscriptStatus {
    fn terminal(&self) -> bool {
        matches!(self, Self::Finalized { .. } | Self::Failed { .. })
    }

    fn successful(&self) -> bool {
        matches!(self, Self::Finalized { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveTranscriptChunk {
    index: usize,
    first_segment: usize,
    next_segment: usize,
    start_ms: u64,
    duration_ms: u64,
    srt_file: String,
    provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveTranscriptManifest {
    version: u32,
    platform: String,
    room_id: String,
    live_id: String,
    next_segment: usize,
    processed_duration_ms: u64,
    completed: bool,
    chunks: Vec<LiveTranscriptChunk>,
}

impl LiveTranscriptManifest {
    fn new(platform: &str, room_id: &str, live_id: &str) -> Self {
        Self {
            version: MANIFEST_VERSION,
            platform: platform.to_string(),
            room_id: room_id.to_string(),
            live_id: live_id.to_string(),
            next_segment: 0,
            processed_duration_ms: 0,
            completed: false,
            chunks: Vec::new(),
        }
    }
}

struct SessionHandle {
    stop_tx: watch::Sender<bool>,
    status_rx: watch::Receiver<LiveTranscriptStatus>,
}

#[derive(Clone)]
pub struct LiveTranscriptionCoordinator {
    config: Arc<RwLock<Config>>,
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    /// Live ASR is deliberately serialized. Recording stays independent and
    /// several live rooms cannot load the CPU with simultaneous inference.
    live_asr_gate: Arc<Semaphore>,
}

impl LiveTranscriptionCoordinator {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            live_asr_gate: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn start(&self, platform: &str, room_id: &str, live_id: &str) {
        if live_id.trim().is_empty() {
            return;
        }
        let config = self.config.read().await.clone();
        if !config.auto_subtitle
            || !matches!(
                config.subtitle_generator_type.as_str(),
                "funasr" | "whisper"
            )
        {
            return;
        }

        let key = session_key(platform, room_id, live_id);
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(&key) {
            if !existing.status_rx.borrow().terminal() {
                return;
            }
        }

        let work_dir = Path::new(&config.cache)
            .join(platform)
            .join(room_id)
            .join(live_id);
        let (stop_tx, stop_rx) = watch::channel(false);
        let initial_status = LiveTranscriptStatus::Waiting {
            message: "等待录播分段，准备增量转写".to_string(),
        };
        let (status_tx, status_rx) = watch::channel(initial_status);
        sessions.insert(key.clone(), SessionHandle { stop_tx, status_rx });
        drop(sessions);

        let config = self.config.clone();
        let live_asr_gate = self.live_asr_gate.clone();
        let platform = platform.to_string();
        let room_id = room_id.to_string();
        let live_id = live_id.to_string();
        tokio::spawn(async move {
            let result = run_session(
                config,
                live_asr_gate,
                work_dir,
                platform,
                room_id,
                live_id,
                stop_rx,
                status_tx.clone(),
            )
            .await;
            if let Err(error) = result {
                log::error!("Live incremental transcription failed for {key}: {error}");
                publish_status(
                    &status_tx,
                    LiveTranscriptStatus::Failed { message: error },
                    None,
                )
                .await;
            }
        });
    }

    pub async fn finish(&self, platform: &str, room_id: &str, live_id: &str) {
        let key = session_key(platform, room_id, live_id);
        let sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get(&key) {
            let _ = handle.stop_tx.send(true);
        }
    }

    /// Wait only for the matching archive. Other active live rooms do not
    /// prevent a completed archive from entering its review pipeline.
    pub async fn wait_until_finalized(&self, platform: &str, room_id: &str, live_id: &str) -> bool {
        let key = session_key(platform, room_id, live_id);
        let mut status_rx = {
            let sessions = self.sessions.lock().await;
            let Some(handle) = sessions.get(&key) else {
                return false;
            };
            handle.status_rx.clone()
        };

        let wait = async {
            loop {
                let current = status_rx.borrow().clone();
                if current.terminal() {
                    return current.successful();
                }
                if status_rx.changed().await.is_err() {
                    return false;
                }
            }
        };
        tokio::time::timeout(FINALIZATION_TIMEOUT, wait)
            .await
            .unwrap_or(false)
    }
}

fn session_key(platform: &str, room_id: &str, live_id: &str) -> String {
    format!("{platform}:{room_id}:{live_id}")
}

async fn run_session(
    config: Arc<RwLock<Config>>,
    live_asr_gate: Arc<Semaphore>,
    work_dir: PathBuf,
    platform: String,
    room_id: String,
    live_id: String,
    mut stop_rx: watch::Receiver<bool>,
    status_tx: watch::Sender<LiveTranscriptStatus>,
) -> Result<(), String> {
    let transcript_dir = work_dir.join("live-transcript");
    tokio::fs::create_dir_all(&transcript_dir)
        .await
        .map_err(|error| format!("创建增量转写目录失败: {error}"))?;
    let manifest_path = transcript_dir.join("manifest.json");
    let mut manifest = load_manifest(&manifest_path)
        .await
        .unwrap_or_else(|| LiveTranscriptManifest::new(&platform, &room_id, &live_id));
    if manifest.version != MANIFEST_VERSION
        || manifest.platform != platform
        || manifest.room_id != room_id
        || manifest.live_id != live_id
    {
        manifest = LiveTranscriptManifest::new(&platform, &room_id, &live_id);
    }

    if manifest.completed && work_dir.join("subtitle.srt").is_file() {
        publish_status(
            &status_tx,
            LiveTranscriptStatus::Finalized {
                completed_chunks: manifest.chunks.len(),
                processed_duration_ms: manifest.processed_duration_ms,
                subtitle_path: work_dir.join("subtitle.srt").to_string_lossy().to_string(),
            },
            Some(&transcript_dir),
        )
        .await;
        return Ok(());
    }

    let mut consecutive_failures = 0usize;
    loop {
        let stopping = *stop_rx.borrow();
        match process_next_chunk(
            &config,
            &live_asr_gate,
            &work_dir,
            &transcript_dir,
            &mut manifest,
            stopping,
        )
        .await
        {
            Ok(true) => {
                consecutive_failures = 0;
                write_manifest(&manifest_path, &manifest).await?;
                let partial = merge_chunk_subtitles(&transcript_dir, &manifest)?;
                if !partial.trim().is_empty() {
                    replace_file(&work_dir.join("subtitle.partial.srt"), partial.as_bytes())
                        .await?;
                }
                publish_status(
                    &status_tx,
                    LiveTranscriptStatus::Transcribing {
                        completed_chunks: manifest.chunks.len(),
                        processed_duration_ms: manifest.processed_duration_ms,
                        message: format!(
                            "录制中增量转写：已完成 {} 段（约 {} 分钟）",
                            manifest.chunks.len(),
                            manifest.processed_duration_ms / 60_000
                        ),
                    },
                    Some(&transcript_dir),
                )
                .await;
                continue;
            }
            Ok(false) if stopping => {
                let subtitle = merge_chunk_subtitles(&transcript_dir, &manifest)?;
                if subtitle.trim().is_empty() {
                    return Err("录制结束，但增量转写没有生成可用文字".to_string());
                }
                let subtitle_path = work_dir.join("subtitle.srt");
                replace_file(&subtitle_path, subtitle.as_bytes()).await?;
                manifest.completed = true;
                write_manifest(&manifest_path, &manifest).await?;

                crate::subtitle_generator::transcript_artifacts::TranscriptArtifactStore::initialize_from_asr_outputs(
                    crate::subtitle_generator::transcript_artifacts::TranscriptSource::Archive {
                        platform: platform.clone(),
                        room_id: room_id.clone(),
                        live_id: live_id.clone(),
                    },
                    &work_dir,
                    work_dir.join("playlist.m3u8"),
                    &subtitle,
                    manifest
                        .chunks
                        .last()
                        .map(|chunk| chunk.provider.as_str())
                        .unwrap_or("funasr"),
                )
                .await
                .map_err(|error| format!("保存增量逐字稿审计文件失败: {error}"))?;

                publish_status(
                    &status_tx,
                    LiveTranscriptStatus::Finalized {
                        completed_chunks: manifest.chunks.len(),
                        processed_duration_ms: manifest.processed_duration_ms,
                        subtitle_path: subtitle_path.to_string_lossy().to_string(),
                    },
                    Some(&transcript_dir),
                )
                .await;
                return Ok(());
            }
            Ok(false) => {
                publish_status(
                    &status_tx,
                    LiveTranscriptStatus::Waiting {
                        message: if manifest.chunks.is_empty() {
                            "录制正常，等待首个完整转写窗口".to_string()
                        } else {
                            format!("录制正常，已准备 {} 段文稿", manifest.chunks.len())
                        },
                    },
                    Some(&transcript_dir),
                )
                .await;
            }
            Err(error) => {
                consecutive_failures += 1;
                log::warn!(
                    "Live ASR chunk retry {consecutive_failures} for {platform}:{room_id}:{live_id}: {error}"
                );
                publish_status(
                    &status_tx,
                    LiveTranscriptStatus::Waiting {
                        message: format!("增量转写暂时失败，录播不受影响，稍后重试：{error}"),
                    },
                    Some(&transcript_dir),
                )
                .await;
                if stopping && consecutive_failures >= 3 {
                    return Err(format!("尾段连续转写失败：{error}"));
                }
                tokio::select! {
                    _ = tokio::time::sleep(RETRY_INTERVAL) => {}
                    _ = stop_rx.changed() => {}
                }
                continue;
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(PLAYLIST_POLL_INTERVAL) => {}
            _ = stop_rx.changed() => {}
        }
    }
}

async fn process_next_chunk(
    config: &Arc<RwLock<Config>>,
    live_asr_gate: &Arc<Semaphore>,
    work_dir: &Path,
    transcript_dir: &Path,
    manifest: &mut LiveTranscriptManifest,
    stopping: bool,
) -> Result<bool, String> {
    let playlist_path = work_dir.join("playlist.m3u8");
    let bytes = match tokio::fs::read(&playlist_path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("读取录播索引失败: {error}")),
    };
    let (_, playlist) = m3u8_rs::parse_media_playlist(&bytes)
        .map_err(|_| "录播索引仍在写入，等待下一次读取".to_string())?;
    if manifest.next_segment > playlist.segments.len() {
        return Err("录播索引分段数量回退，拒绝重复转写".to_string());
    }

    // While recording, hold the newest segment back. Some platforms append to
    // the most recent TS file before the playlist duration is finalized.
    let available_end = if stopping {
        playlist.segments.len()
    } else {
        playlist.segments.len().saturating_sub(1)
    };
    let Some((next_segment, duration_ms)) =
        select_chunk_range(&playlist, manifest.next_segment, available_end, stopping)
    else {
        return Ok(false);
    };

    let chunk_index = manifest.chunks.len();
    let snapshot_path = work_dir.join(format!(".live-asr-chunk-{chunk_index:04}.m3u8"));
    write_playlist_snapshot(
        &playlist,
        manifest.next_segment,
        next_segment,
        &snapshot_path,
    )
    .await?;
    let wav_path = transcript_dir.join(format!("chunk_{chunk_index:04}.wav"));
    let srt_path = transcript_dir.join(format!("chunk_{chunk_index:04}.srt"));

    let _permit = live_asr_gate
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "增量转写调度器已关闭".to_string())?;
    if !wav_path.is_file() {
        extract_chunk_audio(&snapshot_path, &wav_path).await?;
    }

    let config = config.read().await.clone();
    let generated = crate::ffmpeg::generate_video_subtitle(
        None,
        &wav_path,
        &config.subtitle_generator_type,
        &config.whisper_model,
        &config.whisper_prompt,
        &config.openai_api_key,
        &config.openai_api_endpoint,
        &config.whisper_language,
    )
    .await
    .map_err(|error| format!("FunASR/Whisper 分段转写失败: {error}"))?;
    let provider = generated.generator_type.as_str().to_string();
    let chunk_srt = generated
        .subtitle_content
        .iter()
        .map(item_to_srt)
        .collect::<String>();
    replace_file(&srt_path, chunk_srt.as_bytes()).await?;

    manifest.chunks.push(LiveTranscriptChunk {
        index: chunk_index,
        first_segment: manifest.next_segment,
        next_segment,
        start_ms: manifest.processed_duration_ms,
        duration_ms,
        srt_file: srt_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        provider,
    });
    manifest.next_segment = next_segment;
    manifest.processed_duration_ms += duration_ms;

    let _ = tokio::fs::remove_file(&snapshot_path).await;
    let _ = tokio::fs::remove_file(&wav_path).await;
    Ok(true)
}

fn select_chunk_range(
    playlist: &MediaPlaylist,
    start: usize,
    available_end: usize,
    stopping: bool,
) -> Option<(usize, u64)> {
    if start >= available_end || start >= playlist.segments.len() {
        return None;
    }
    let limit = available_end.min(playlist.segments.len());
    let mut duration_ms = 0u64;
    let mut end = start;
    while end < limit {
        duration_ms += (f64::from(playlist.segments[end].duration) * 1000.0).round() as u64;
        end += 1;
        if duration_ms >= TARGET_CHUNK_MS {
            break;
        }
    }
    if !stopping && duration_ms < TARGET_CHUNK_MS {
        return None;
    }
    Some((end, duration_ms))
}

async fn write_playlist_snapshot(
    playlist: &MediaPlaylist,
    start: usize,
    end: usize,
    path: &Path,
) -> Result<(), String> {
    let mut snapshot = playlist.clone();
    snapshot.media_sequence = snapshot.media_sequence.saturating_add(start as u64);
    snapshot.segments = playlist.segments[start..end]
        .iter()
        .cloned()
        .map(|mut segment| {
            // Recorder downloads the local file using the URI before `?`.
            // Keep the original playlist untouched, but make the local ASR
            // snapshot point at the actual filename on disk.
            segment.uri = segment
                .uri
                .split_once('?')
                .map(|(path, _)| path)
                .unwrap_or(&segment.uri)
                .to_string();
            segment
        })
        .collect();
    snapshot.end_list = true;
    snapshot.playlist_type = Some(MediaPlaylistType::Vod);
    let mut bytes = Vec::new();
    snapshot
        .write_to(&mut bytes)
        .map_err(|error| format!("生成增量音频索引失败: {error}"))?;
    replace_file(path, &bytes).await
}

async fn extract_chunk_audio(snapshot_path: &Path, wav_path: &Path) -> Result<(), String> {
    let temp_path = wav_path.with_extension("part.wav");
    let mut command = crate::ffmpeg::ffmpeg_command();
    #[cfg(target_os = "windows")]
    command.creation_flags(0x0800_0000);
    let output = command
        .args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(snapshot_path)
        .args(["-vn", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "-y"])
        .arg(&temp_path)
        .output()
        .await
        .map_err(|error| format!("提取增量音频失败: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "提取增量音频失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    replace_from_path(wav_path, &temp_path).await
}

fn merge_chunk_subtitles(
    transcript_dir: &Path,
    manifest: &LiveTranscriptManifest,
) -> Result<String, String> {
    let mut full = GenerateResult {
        subtitle_id: String::new(),
        subtitle_content: Vec::new(),
        generator_type: SubtitleGeneratorType::FunAsr,
    };
    for chunk in &manifest.chunks {
        let path = transcript_dir.join(&chunk.srt_file);
        let content = std::fs::read_to_string(&path)
            .map_err(|error| format!("读取增量字幕 {} 失败: {error}", path.display()))?;
        if content.trim().is_empty() {
            continue;
        }
        let items = srtparse::from_str(&content)
            .map_err(|error| format!("解析增量字幕 {} 失败: {error}", path.display()))?;
        let chunk_result = GenerateResult {
            subtitle_id: String::new(),
            subtitle_content: items,
            generator_type: SubtitleGeneratorType::from_str(&chunk.provider)
                .unwrap_or(SubtitleGeneratorType::FunAsr),
        };
        full.concat_with_offset_ms(&chunk_result, chunk.start_ms);
    }
    Ok(full.subtitle_content.iter().map(item_to_srt).collect())
}

async fn load_manifest(path: &Path) -> Option<LiveTranscriptManifest> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&content).ok()
}

async fn write_manifest(path: &Path, manifest: &LiveTranscriptManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("序列化增量转写进度失败: {error}"))?;
    replace_file(path, &bytes).await
}

async fn publish_status(
    status_tx: &watch::Sender<LiveTranscriptStatus>,
    status: LiveTranscriptStatus,
    transcript_dir: Option<&Path>,
) {
    let _ = status_tx.send(status.clone());
    if let Some(dir) = transcript_dir {
        if let Ok(bytes) = serde_json::to_vec_pretty(&status) {
            let _ = replace_file(&dir.join("status.json"), &bytes).await;
        }
    }
}

async fn replace_from_path(target: &Path, source: &Path) -> Result<(), String> {
    if target.exists() {
        tokio::fs::remove_file(target)
            .await
            .map_err(|error| format!("替换文件 {} 失败: {error}", target.display()))?;
    }
    tokio::fs::rename(source, target)
        .await
        .map_err(|error| format!("提交文件 {} 失败: {error}", target.display()))
}

async fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("创建目录 {} 失败: {error}", parent.display()))?;
    }
    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
    ));
    tokio::fs::write(&temp_path, bytes)
        .await
        .map_err(|error| format!("写入临时文件 {} 失败: {error}", temp_path.display()))?;
    replace_from_path(path, &temp_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use m3u8_rs::MediaSegment;

    fn playlist(durations: &[f32]) -> MediaPlaylist {
        MediaPlaylist {
            segments: durations
                .iter()
                .enumerate()
                .map(|(index, duration)| MediaSegment {
                    uri: format!("{index}.ts"),
                    duration: *duration,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn recording_holds_latest_segment_and_waits_for_two_minutes() {
        let value = playlist(&[30.0, 30.0, 30.0, 30.0, 30.0]);
        assert_eq!(select_chunk_range(&value, 0, 4, false), Some((4, 120_000)));
        assert_eq!(select_chunk_range(&value, 4, 4, false), None);
        assert_eq!(select_chunk_range(&value, 0, 3, false), None);
    }

    #[test]
    fn stopping_flushes_short_tail() {
        let value = playlist(&[30.0, 20.0]);
        assert_eq!(select_chunk_range(&value, 0, 2, true), Some((2, 50_000)));
    }

    #[test]
    fn merged_chunks_receive_global_offsets_and_positions() {
        let root =
            std::env::temp_dir().join(format!("bsr-live-transcript-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("chunk_0000.srt"),
            "1\n00:00:01,000 --> 00:00:02,000\n第一段\n\n",
        )
        .unwrap();
        std::fs::write(
            root.join("chunk_0001.srt"),
            "1\n00:00:00,500 --> 00:00:01,500\n第二段\n\n",
        )
        .unwrap();
        let manifest = LiveTranscriptManifest {
            version: MANIFEST_VERSION,
            platform: "douyin".into(),
            room_id: "room".into(),
            live_id: "live".into(),
            next_segment: 2,
            processed_duration_ms: 122_000,
            completed: false,
            chunks: vec![
                LiveTranscriptChunk {
                    index: 0,
                    first_segment: 0,
                    next_segment: 1,
                    start_ms: 0,
                    duration_ms: 120_000,
                    srt_file: "chunk_0000.srt".into(),
                    provider: "funasr".into(),
                },
                LiveTranscriptChunk {
                    index: 1,
                    first_segment: 1,
                    next_segment: 2,
                    start_ms: 120_000,
                    duration_ms: 2_000,
                    srt_file: "chunk_0001.srt".into(),
                    provider: "funasr".into(),
                },
            ],
        };
        let merged = merge_chunk_subtitles(&root, &manifest).unwrap();
        assert!(merged.contains("1\n00:00:01,000 --> 00:00:02,000"));
        assert!(merged.contains("2\n00:02:00,500 --> 00:02:01,500"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn extracts_audio_from_a_live_hls_snapshot() {
        let root =
            std::env::temp_dir().join(format!("bsr-live-hls-snapshot-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let playlist_path = root.join("playlist.m3u8");
        let segment_pattern = root.join("segment_%03d.ts");
        let output = crate::ffmpeg::ffmpeg_command()
            .args(["-hide_banner", "-loglevel", "error"])
            .args(["-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=16000"])
            .args(["-t", "3", "-c:a", "aac", "-f", "hls", "-hls_time", "1"])
            .arg("-hls_segment_filename")
            .arg(&segment_pattern)
            .arg("-y")
            .arg(&playlist_path)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let bytes = tokio::fs::read(&playlist_path).await.unwrap();
        let (_, mut playlist) = m3u8_rs::parse_media_playlist(&bytes).unwrap();
        assert!(!playlist.segments.is_empty());
        for segment in &mut playlist.segments {
            segment.uri.push_str("?remote_download_token=ignored");
        }
        let snapshot = root.join("snapshot.m3u8");
        write_playlist_snapshot(&playlist, 0, playlist.segments.len(), &snapshot)
            .await
            .unwrap();
        let wav = root.join("snapshot.wav");
        extract_chunk_audio(&snapshot, &wav).await.unwrap();
        assert!(std::fs::metadata(&wav).unwrap().len() > 16_000);
        let _ = std::fs::remove_dir_all(root);
    }
}
