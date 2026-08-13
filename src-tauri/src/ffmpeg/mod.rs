use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

mod audited_calibration;
pub mod general;
pub mod hwaccel;
pub mod playlist;

use crate::constants;
use crate::progress::progress_reporter::{ProgressReporter, ProgressReporterTrait};
#[cfg(feature = "local-whisper")]
use crate::subtitle_generator::whisper_cpp;
use crate::subtitle_generator::{funasr, powerlive, whisper_online};
use crate::subtitle_generator::{
    item_to_srt, volcengine::VolcengineAsr, GenerateResult, SubtitleGenerator,
    SubtitleGeneratorType,
};
use async_ffmpeg_sidecar::event::{FfmpegEvent, LogLevel};
use async_ffmpeg_sidecar::log_parser::FfmpegLogParser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

// 视频元数据结构
#[derive(Debug, Clone, PartialEq)]
pub struct VideoMetadata {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub audio_codec: String,
}

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(target_os = "windows")]
#[allow(unused_imports)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: f64,
    pub end: f64,
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.start, self.end)
    }
}

impl Range {
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    pub fn is_in(&self, v: f64) -> bool {
        v >= self.start && v <= self.end
    }
}

fn output_is_mp4(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "m4v" | "mov"))
}

pub async fn transcode(
    reporter: Option<&impl ProgressReporterTrait>,
    file: &Path,
    output_path: &Path,
    copy_codecs: bool,
) -> Result<(), String> {
    // ffmpeg -i fixed_\[30655190\]1742887114_0325084106_81.5.mp4 -c:v libx264 -c:a aac -b:v 6000k -b:a 64k -compression_level 0 -threads 0 output.mp3
    log::info!("Transcode: {} copy: {}", file.display(), copy_codecs);
    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    if copy_codecs {
        ffmpeg_process.args(["-fflags", "+genpts+igndts"]);
    }
    ffmpeg_process.args(["-i", file.to_str().unwrap()]);

    if copy_codecs {
        ffmpeg_process
            .args(["-c:v", "copy"])
            .args(["-c:a", "copy"])
            .args(["-avoid_negative_ts", "make_zero"]);
    } else {
        let video_encoder = hwaccel::get_x264_encoder().await;
        hwaccel::apply_x264_encoder_args(
            &mut ffmpeg_process,
            video_encoder,
            Some(hwaccel::H264_SCALE_PAD_FILTER),
        );
        ffmpeg_process.args(["-c:a", "aac"]);
        hwaccel::apply_x264_quality_args(&mut ffmpeg_process, video_encoder);
        ffmpeg_process.args(["-threads", "0"]);
    }

    if output_is_mp4(output_path) {
        ffmpeg_process.args(["-movflags", "+faststart"]);
    }

    let child = ffmpeg_process
        .args([output_path.to_str().unwrap()])
        .args(["-y"])
        .args(["-progress", "pipe:2"])
        .stderr(Stdio::piped())
        .spawn();
    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);
    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Progress(p) => {
                if reporter.is_none() {
                    continue;
                }
                reporter
                    .unwrap()
                    .update(format!("压制中：{}", p.time).as_str())
                    .await;
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Error(e) => {
                log::error!("Transcode error: {e}");
                return Err(e.to_string());
            }
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        return Err(e.to_string());
    }

    Ok(())
}

pub async fn trim_video(
    reporter: Option<&impl ProgressReporterTrait>,
    file: &Path,
    output_path: &Path,
    start_time: f64,
    duration: f64,
) -> Result<(), String> {
    // ffmpeg -i fixed_\[30655190\]1742887114_0325084106_81.5.mp4 -ss 0 -t 10 output.mp4
    log::info!("Trim video task start: {}", file.display());
    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    ffmpeg_process.args(["-ss", &start_time.to_string()]);
    ffmpeg_process.args(["-i", file.to_str().unwrap()]);
    ffmpeg_process.args(["-t", &duration.to_string()]);
    ffmpeg_process.args(["-c", "copy"]);
    ffmpeg_process.args([output_path.to_str().unwrap()]);
    ffmpeg_process.args(["-y"]);
    ffmpeg_process.args(["-progress", "pipe:2"]);
    ffmpeg_process.stderr(Stdio::piped());
    let child = ffmpeg_process.spawn();
    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Progress(p) => {
                if reporter.is_none() {
                    continue;
                }
                reporter
                    .unwrap()
                    .update(format!("切片中：{}", p.time).as_str())
                    .await;
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Error(e) => {
                log::error!("Trim video error: {e}");
                return Err(e.to_string());
            }
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Trim video error: {e}");
        return Err(e.to_string());
    }

    log::info!("Trim video task end: {}", output_path.display());
    Ok(())
}

/// Extract a sample audio from the video file for waveform display
pub async fn extract_audio_sample(file: &Path) -> Result<PathBuf, String> {
    // ffmpeg -i fixed_\[30655592\]1742887114_0325084106_81.5.mp4 -ar 16000 test.wav
    log::info!("Extract audio sample task start: {}", file.display());
    let output_path = file.with_extension("opus");
    let mut extract_error = None;

    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let child = ffmpeg_process
        .args(["-i", file.to_str().unwrap()])
        .args(["-c:a", "libopus"])
        .args(["-ar", "16000"])
        .args(["-ac", "1"])
        .args(["-vn"])
        .args(["-b:a", "64k"])
        .args(["-vbr", "on"])
        .args(["-compression_level", "10"])
        .args([output_path.to_str().unwrap()])
        .args(["-y"])
        .args(["-progress", "pipe:2"])
        .stderr(Stdio::piped())
        .spawn();

    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);
    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Error(e) => {
                log::error!("Extract audio sample error: {e}");
                extract_error = Some(e.to_string());
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Progress(p) => {
                log::info!("Extract audio sample progress: {}", p.time);
            }
            FfmpegEvent::Log(_level, _content) => {}
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Extract audio sample error: {e}");
        return Err(e.to_string());
    }

    if let Some(error) = extract_error {
        log::error!("Extract audio sample error: {error}");
        Err(error)
    } else {
        log::info!("Extract audio sample task end: {}", output_path.display());
        Ok(output_path)
    }
}
pub async fn extract_audio_chunks(file: &Path, format: &str) -> Result<PathBuf, String> {
    // ffmpeg -i fixed_\[30655190\]1742887114_0325084106_81.5.mp4 -ar 16000 test.wav
    log::info!("Extract audio task start: {}", file.display());
    let output_path = file.with_extension(format);
    let mut extract_error = None;

    // 降低采样率以提高处理速度，同时保持足够的音质用于语音识别
    let sample_rate = if format == "mp3" { "22050" } else { "16000" };

    // First, get the duration of the input file
    let duration = get_audio_duration(file).await?;
    log::info!("Audio duration: {duration} seconds");

    // Split into chunks of 30 seconds
    let chunk_duration = 30;
    let chunk_count = (duration as f64 / f64::from(chunk_duration)).ceil() as usize;
    log::info!("Splitting into {chunk_count} chunks of {chunk_duration} seconds each");

    // Create output directory for chunks
    let output_dir = output_path.parent().unwrap();
    let base_name = output_path.file_stem().unwrap().to_str().unwrap();
    let chunk_dir = output_dir.join(format!("{base_name}_chunks"));

    if !chunk_dir.exists() {
        std::fs::create_dir_all(&chunk_dir)
            .map_err(|e| format!("Failed to create chunk directory: {e}"))?;
    }

    // Use ffmpeg segment feature to split audio into chunks
    let segment_pattern = chunk_dir.join(format!("{base_name}_%03d.{format}"));

    // 构建优化的ffmpeg命令参数
    let file_str = file.to_str().unwrap();
    let chunk_duration_str = chunk_duration.to_string();
    let segment_pattern_str = segment_pattern.to_str().unwrap();

    let mut args = vec![
        "-i",
        file_str,
        "-ar",
        sample_rate,
        "-vn",
        "-f",
        "segment",
        "-segment_time",
        &chunk_duration_str,
        "-reset_timestamps",
        "1",
        "-y",
        "-progress",
        "pipe:2",
    ];

    // 根据格式添加优化的编码参数
    if format == "mp3" {
        args.extend_from_slice(&[
            "-c:a",
            "mp3",
            "-b:a",
            "64k", // 降低比特率以提高速度
            "-compression_level",
            "0", // 最快压缩
        ]);
    } else {
        args.extend_from_slice(&[
            "-c:a",
            "pcm_s16le", // 使用PCM编码，速度更快
        ]);
    }

    // 添加性能优化参数
    args.extend_from_slice(&[
        "-threads", "0", // 使用所有可用CPU核心
    ]);

    args.push(segment_pattern_str);

    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let child = ffmpeg_process.args(&args).stderr(Stdio::piped()).spawn();

    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Error(e) => {
                log::error!("Extract audio error: {e}");
                extract_error = Some(e.to_string());
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Log(_level, _content) => {}
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Extract audio error: {e}");
        return Err(e.to_string());
    }

    if let Some(error) = extract_error {
        log::error!("Extract audio error: {error}");
        Err(error)
    } else {
        log::info!(
            "Extract audio task end: {} chunks created in {}",
            chunk_count,
            chunk_dir.display()
        );
        Ok(chunk_dir)
    }
}

/// Extract the full audio track as a single 16kHz mono WAV file.
/// Returns the path to the extracted WAV.
pub async fn extract_full_audio(file: &Path) -> Result<PathBuf, String> {
    log::info!("Extract full audio: {}", file.display());
    let output_path = file.with_extension("full.wav");

    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let child = ffmpeg_process
        .arg("-i")
        .arg(file)
        .args(["-ar", "16000"])
        .args(["-ac", "1"]) // mono for VAD
        .args(["-c:a", "pcm_s16le"])
        .args(["-vn"])
        .args(["-y"])
        .args(["-progress", "pipe:2"])
        .arg(&output_path)
        .stderr(Stdio::piped())
        .spawn();

    let mut child = child.map_err(|e| format!("Failed to spawn ffmpeg: {e}"))?;
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Error(e) => {
                log::error!("Extract full audio error: {e}");
            }
            FfmpegEvent::LogEOF => break,
            _ => {}
        }
    }

    child
        .wait()
        .await
        .map_err(|e| format!("ffmpeg wait error: {e}"))?;

    if output_path.exists() {
        log::info!("Full audio extracted: {}", output_path.display());
        Ok(output_path)
    } else {
        Err("Full audio extraction failed: output file not found".to_string())
    }
}

/// Extract a time segment from a video as a 16kHz mono WAV file.
pub async fn extract_audio_segment(
    file: &Path,
    start_sec: f64,
    duration_sec: f64,
    output_path: &Path,
) -> Result<(), String> {
    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let child = ffmpeg_process
        .args(["-ss", &start_sec.to_string()])
        .arg("-i")
        .arg(file)
        .args(["-t", &duration_sec.to_string()])
        .args(["-ar", "16000"])
        .args(["-ac", "1"])
        .args(["-c:a", "pcm_s16le"])
        .args(["-vn"])
        .args(["-y"])
        .args(["-progress", "pipe:2"])
        .arg(output_path)
        .stderr(Stdio::piped())
        .spawn();

    let mut child = child.map_err(|e| format!("Failed to spawn ffmpeg: {e}"))?;
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Error(e) => {
                log::error!("Extract audio segment error: {e}");
            }
            FfmpegEvent::LogEOF => break,
            _ => {}
        }
    }

    child
        .wait()
        .await
        .map_err(|e| format!("ffmpeg wait error: {e}"))?;

    if output_path.exists() {
        Ok(())
    } else {
        Err("Audio segment extraction failed: output file not found".to_string())
    }
}

/// Extract one master-ingest chunk as a 16kHz mono 64kbps MP3.
pub async fn extract_volcengine_audio_segment(
    file: &Path,
    start_ms: u64,
    end_ms: u64,
    output_dir: &Path,
) -> Result<PathBuf, String> {
    if end_ms <= start_ms || end_ms - start_ms > 600_000 {
        return Err("Volcengine ASR segment bounds are invalid".to_string());
    }
    tokio::fs::create_dir_all(output_dir)
        .await
        .map_err(|error| format!("Failed to create ASR segment directory: {error}"))?;
    let output_path = output_dir.join(format!("chunk_{start_ms:012}_{end_ms:012}.mp3"));
    match tokio::fs::remove_file(&output_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Failed to clear prior ASR segment: {error}")),
    }
    let temporary = output_dir.join(format!(
        ".chunk_{start_ms:012}_{end_ms:012}.{}.tmp.mp3",
        uuid::Uuid::new_v4()
    ));

    let start_seconds = format!("{:.3}", start_ms as f64 / 1000.0);
    let duration_seconds = format!("{:.3}", (end_ms - start_ms) as f64 / 1000.0);
    let output = match ffmpeg_command()
        .args(["-ss", &start_seconds])
        .arg("-i")
        .arg(file)
        .args(["-t", &duration_seconds])
        .args(["-vn", "-ar", "16000", "-ac", "1"])
        .args(["-c:a", "libmp3lame", "-b:a", "64k", "-y"])
        .arg(&temporary)
        .output()
        .await
    {
        Ok(output) => output,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(format!(
                "Failed to start FFmpeg ASR segment extraction: {error}"
            ));
        }
    };
    if !output.status.success() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(format!(
            "Failed to extract Volcengine ASR segment: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    master_ingest::publish_extracted_chunk(&temporary, &output_path).await?;
    Ok(output_path)
}

pub async fn probe_media_duration_ms(file: &Path) -> Result<u64, String> {
    get_audio_duration(file)
        .await
        .map(|seconds| seconds.saturating_mul(1000))
}

/// Get the duration of an audio/video file in seconds
async fn get_audio_duration(file: &Path) -> Result<u64, String> {
    // Use ffprobe with format option to get duration
    let mut ffprobe_process = tokio::process::Command::new(ffprobe_path());
    #[cfg(target_os = "windows")]
    ffprobe_process.creation_flags(CREATE_NO_WINDOW);

    let child = ffprobe_process
        .args(["-v", "quiet"])
        .args(["-show_entries", "format=duration"])
        .args(["-of", "csv=p=0"])
        .args(["-i", file.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    if let Err(e) = child {
        return Err(format!("Failed to spawn ffprobe process: {e}"));
    }

    let mut child = child.unwrap();
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    let mut parser = FfmpegLogParser::new(reader);

    let mut duration = None;
    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Log(_level, content) => {
                // The new command outputs duration directly as a float
                if let Ok(seconds_f64) = content.trim().parse::<f64>() {
                    duration = Some(seconds_f64.ceil() as u64);
                    log::debug!("Parsed duration: {seconds_f64} seconds");
                }
            }
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Failed to get duration: {e}");
        return Err(e.to_string());
    }

    duration.ok_or_else(|| "Failed to parse duration".to_string())
}

/// Encode video subtitle using ffmpeg, output is file name with prefix [subtitle]
pub async fn encode_video_subtitle(
    reporter: &impl ProgressReporterTrait,
    file: &Path,
    subtitle: &Path,
    srt_style: String,
) -> Result<String, String> {
    // ffmpeg -i fixed_\[30655190\]1742887114_0325084106_81.5.mp4 -vf "subtitles=test.srt:force_style='FontSize=24'" -c:v libx264 -c:a copy output.mp4
    log::info!("Encode video subtitle task start: {}", file.display());
    log::info!("SRT style: {srt_style}");
    // output path is file with prefix [subtitle]
    let output_filename = format!(
        "{}{}",
        constants::PREFIX_SUBTITLE,
        file.file_name().unwrap().to_str().unwrap()
    );
    let output_path = file.with_file_name(&output_filename);

    // check output path exists - log but allow overwrite
    if output_path.exists() {
        log::info!(
            "Output path already exists, will overwrite: {}",
            output_path.display()
        );
    }

    let mut command_error = None;

    // if windows
    let subtitle = if cfg!(target_os = "windows") {
        // escape characters in subtitle path
        let subtitle = subtitle
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace(':', "\\:");
        format!("'{subtitle}'")
    } else {
        format!("'{}'", subtitle.display())
    };
    let vf = format!(
        "{},subtitles={subtitle}:force_style='{srt_style}'",
        hwaccel::H264_SCALE_PAD_FILTER
    );
    log::info!("vf: {vf}");

    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let video_encoder = hwaccel::get_x264_encoder().await;

    ffmpeg_process.args(["-i", file.to_str().unwrap()]);
    hwaccel::apply_x264_encoder_args(&mut ffmpeg_process, video_encoder, Some(vf.as_str()));
    ffmpeg_process.args(["-c:a", "copy"]);
    hwaccel::apply_x264_quality_args(&mut ffmpeg_process, video_encoder);
    let child = ffmpeg_process
        .args([output_path.to_str().unwrap()])
        .args(["-y"])
        .args(["-progress", "pipe:2"])
        .stderr(Stdio::piped())
        .spawn();

    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Error(e) => {
                log::error!("Encode video subtitle error: {e}");
                command_error = Some(e.to_string());
            }
            FfmpegEvent::Progress(p) => {
                log::info!("Encode video subtitle progress: {}", p.time);
                reporter
                    .update(format!("压制中：{}", p.time).as_str())
                    .await;
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Log(_level, _content) => {}
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Encode video subtitle error: {e}");
        return Err(e.to_string());
    }

    if let Some(error) = command_error {
        log::error!("Encode video subtitle error: {error}");
        Err(error)
    } else {
        log::info!("Encode video subtitle task end: {}", output_path.display());
        Ok(output_filename)
    }
}

pub async fn encode_video_danmu(
    reporter: Option<&impl ProgressReporterTrait>,
    file: &Path,
    subtitle: &Path,
) -> Result<PathBuf, String> {
    // ffmpeg -i fixed_\[30655190\]1742887114_0325084106_81.5.mp4 -vf ass=subtitle.ass -c:v libx264 -c:a copy output.mp4
    log::info!("Encode video danmu task start: {}", file.display());
    let danmu_filename = format!(
        "{}{}",
        constants::PREFIX_DANMAKU,
        file.file_name().unwrap().to_str().unwrap()
    );
    let output_file_path = file.with_file_name(danmu_filename);

    // check output path exists - log but allow overwrite
    if output_file_path.exists() {
        log::info!(
            "Output path already exists, will overwrite: {}",
            output_file_path.display()
        );
    }

    let mut command_error = None;

    // if windows
    let subtitle = if cfg!(target_os = "windows") {
        // escape characters in subtitle path
        let subtitle = subtitle
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace(':', "\\:");
        format!("'{subtitle}'")
    } else {
        format!("'{}'", subtitle.display())
    };

    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let video_encoder = hwaccel::get_x264_encoder().await;

    let vf = format!("{},ass={subtitle}", hwaccel::H264_SCALE_PAD_FILTER);
    ffmpeg_process.args(["-i", file.to_str().unwrap()]);
    hwaccel::apply_x264_encoder_args(&mut ffmpeg_process, video_encoder, Some(vf.as_str()));
    ffmpeg_process.args(["-c:a", "copy"]);
    hwaccel::apply_x264_quality_args(&mut ffmpeg_process, video_encoder);
    let child = ffmpeg_process
        .args([output_file_path.to_str().unwrap()])
        .args(["-y"])
        .args(["-progress", "pipe:2"])
        .stderr(Stdio::piped())
        .spawn();

    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Error(e) => {
                log::error!("Encode video danmu error: {e}");
                command_error = Some(e.to_string());
            }
            FfmpegEvent::Progress(p) => {
                log::debug!("Encode video danmu progress: {}", p.time);
                if reporter.is_none() {
                    continue;
                }
                reporter
                    .unwrap()
                    .update(format!("压制中：{}", p.time).as_str())
                    .await;
            }
            FfmpegEvent::Log(_level, _content) => {}
            FfmpegEvent::LogEOF => break,
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Encode video danmu error: {e}");
        return Err(e.to_string());
    }

    if let Some(error) = command_error {
        log::error!("Encode video danmu error: {error}");
        Err(error)
    } else {
        log::info!(
            "Encode video danmu task end: {}",
            output_file_path.display()
        );
        Ok(output_file_path)
    }
}

pub async fn generic_ffmpeg_command(args: &[&str]) -> Result<String, String> {
    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let child = ffmpeg_process.args(args).stderr(Stdio::piped()).spawn();
    if let Err(e) = child {
        return Err(e.to_string());
    }

    let mut child = child.unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    let mut logs = Vec::new();

    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Log(_level, content) => {
                logs.push(content);
            }
            FfmpegEvent::LogEOF => break,
            _ => {}
        }
    }

    if let Err(e) = child.wait().await {
        log::error!("Generic ffmpeg command error: {e}");
        return Err(e.to_string());
    }

    Ok(logs.join("\n"))
}

#[allow(clippy::too_many_arguments)]
/// Local FunASR/Whisper: split long recordings into fixed chunks so progress,
/// resume, and peak memory stay manageable. 10 minutes matches Volcengine.
const LOCAL_ASR_CHUNK_SEC: u64 = 600;
/// Only chunk when the source is longer than this (15 minutes).
const LOCAL_ASR_CHUNK_THRESHOLD_SEC: u64 = 900;
const WHISPER_CHUNK_CONCURRENCY: usize = 2;

fn local_asr_chunk_dir(file: &Path) -> Result<PathBuf, String> {
    let parent = file
        .parent()
        .ok_or_else(|| "输入视频没有父目录".to_string())?;
    let stem = file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("recording");
    Ok(parent.join(format!("{stem}.local-asr-chunks")))
}

fn list_local_asr_wav_chunks(chunk_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut chunks = std::fs::read_dir(chunk_dir)
        .map_err(|error| format!("读取本地ASR分段失败: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("wav"))
        .collect::<Vec<_>>();
    chunks.sort();
    Ok(chunks)
}

async fn ensure_local_asr_audio_chunks(
    file: &Path,
    reporter: Option<&ProgressReporter>,
) -> Result<Vec<PathBuf>, String> {
    let chunk_dir = local_asr_chunk_dir(file)?;
    tokio::fs::create_dir_all(&chunk_dir)
        .await
        .map_err(|error| format!("创建本地ASR分段目录失败: {error}"))?;
    let existing = list_local_asr_wav_chunks(&chunk_dir)?;
    if !existing.is_empty() {
        return Ok(existing);
    }

    if let Some(reporter) = reporter {
        reporter
            .update("正在按 10 分钟切分音频以加速整场转写…")
            .await;
    }
    let pattern = chunk_dir.join("chunk_%04d.wav");
    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);
    let output = ffmpeg_process
        .arg("-i")
        .arg(file)
        .args(["-vn", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
        .args([
            "-f",
            "segment",
            "-segment_time",
            &LOCAL_ASR_CHUNK_SEC.to_string(),
            "-reset_timestamps",
            "1",
            "-y",
        ])
        .arg(&pattern)
        .output()
        .await
        .map_err(|error| format!("切分整场音频失败: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "切分整场音频失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let chunks = list_local_asr_wav_chunks(&chunk_dir)?;
    if chunks.is_empty() {
        return Err("切分整场音频后没有得到分段文件".to_string());
    }
    Ok(chunks)
}

fn load_cached_local_asr_chunk_srt(
    chunk: &Path,
    generator_type: SubtitleGeneratorType,
) -> Result<Option<GenerateResult>, String> {
    let cached_srt = chunk.with_extension("srt");
    if !cached_srt.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&cached_srt)
        .map_err(|error| format!("读取本地ASR缓存分段失败: {error}"))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(GenerateResult {
        subtitle_id: String::new(),
        subtitle_content: srtparse::from_str(&content)
            .map_err(|error| format!("解析本地ASR缓存分段失败: {error}"))?,
        generator_type,
    }))
}

async fn save_cached_local_asr_chunk_srt(
    chunk: &Path,
    result: &GenerateResult,
) -> Result<(), String> {
    let content = result
        .subtitle_content
        .iter()
        .map(item_to_srt)
        .collect::<String>();
    tokio::fs::write(chunk.with_extension("srt"), content)
        .await
        .map_err(|error| format!("写入本地ASR缓存分段失败: {error}"))
}

fn local_asr_generator_label(generator_type: &str) -> SubtitleGeneratorType {
    match generator_type {
        "whisper" => SubtitleGeneratorType::Whisper,
        _ => SubtitleGeneratorType::FunAsr,
    }
}

/// Transcribe long local recordings in 10-minute WAV chunks with per-chunk
/// resume cache. FunASR concurrency follows `BSR_FUNASR_WORKERS` / the worker pool.
pub async fn generate_chunked_local_video_subtitle(
    reporter: Option<&ProgressReporter>,
    file: &Path,
    generator_type: &str,
    whisper_model: &str,
    whisper_prompt: &str,
    openai_api_key: &str,
    openai_api_endpoint: &str,
    language_hint: &str,
) -> Result<GenerateResult, String> {
    let chunks = ensure_local_asr_audio_chunks(file, reporter).await?;
    let total = chunks.len();
    let label = local_asr_generator_label(generator_type);
    let concurrency = if generator_type == "whisper" {
        WHISPER_CHUNK_CONCURRENCY
    } else {
        funasr::worker_count()
    }
    .max(1)
    .min(total);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut chunk_results: Vec<Option<GenerateResult>> = vec![None; total];
    let mut pending = JoinSet::new();
    let mut next_index = 0usize;
    let mut completed = 0usize;

    // Prefill from cache so resume skips network/model work.
    for (index, chunk) in chunks.iter().enumerate() {
        if let Some(cached) = load_cached_local_asr_chunk_srt(chunk, label.clone())? {
            chunk_results[index] = Some(cached);
            completed += 1;
        }
    }
    if completed > 0 {
        if let Some(reporter) = reporter {
            reporter
                .update(&format!("已复用 {completed}/{total} 个分段缓存，继续补齐…"))
                .await;
        }
    }

    while completed < total {
        while next_index < total && pending.len() < concurrency {
            if chunk_results[next_index].is_some() {
                next_index += 1;
                continue;
            }
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|error| format!("本地ASR分段调度失败: {error}"))?;
            let chunk = chunks[next_index].clone();
            let index = next_index;
            next_index += 1;
            let generator_type = generator_type.to_string();
            let whisper_model = whisper_model.to_string();
            let whisper_prompt = whisper_prompt.to_string();
            let openai_api_key = openai_api_key.to_string();
            let openai_api_endpoint = openai_api_endpoint.to_string();
            let language_hint = language_hint.to_string();
            pending.spawn(async move {
                let _permit = permit;
                let result = generate_video_subtitle_once(
                    None,
                    &chunk,
                    &generator_type,
                    &whisper_model,
                    &whisper_prompt,
                    &openai_api_key,
                    &openai_api_endpoint,
                    &language_hint,
                )
                .await
                .map_err(|error| format!("第 {}/{} 段转写失败: {error}", index + 1, total))?;
                save_cached_local_asr_chunk_srt(&chunk, &result).await?;
                Ok::<(usize, GenerateResult), String>((index, result))
            });
        }

        let Some(joined) = pending.join_next().await else {
            break;
        };
        let (index, result) = joined
            .map_err(|error| format!("本地ASR分段任务异常: {error}"))??;
        chunk_results[index] = Some(result);
        completed += 1;
        if let Some(reporter) = reporter {
            let parallel_hint = if concurrency > 1 {
                format!("，并行 {concurrency} 路")
            } else {
                String::new()
            };
            reporter
                .update(&format!(
                    "分段转写进度 {completed}/{total}（每段约 {} 分钟{parallel_hint}）",
                    LOCAL_ASR_CHUNK_SEC / 60
                ))
                .await;
        }
    }

    let mut full = GenerateResult {
        subtitle_id: String::new(),
        subtitle_content: vec![],
        generator_type: label,
    };
    for (index, chunk_result) in chunk_results.into_iter().enumerate() {
        let result = chunk_result
            .ok_or_else(|| format!("第 {}/{} 段缺少识别结果", index + 1, total))?;
        full.concat_with_offset_ms(&result, index as u64 * LOCAL_ASR_CHUNK_SEC * 1000);
    }
    Ok(full)
}

pub async fn generate_video_subtitle(
    reporter: Option<&ProgressReporter>,
    file: &Path,
    generator_type: &str,
    whisper_model: &str,
    whisper_prompt: &str,
    openai_api_key: &str,
    openai_api_endpoint: &str,
    language_hint: &str,
) -> Result<GenerateResult, String> {
    let prefer_chunks = matches!(generator_type, "funasr" | "whisper")
        && probe_media_duration_ms(file)
            .await
            .map(|ms| ms > LOCAL_ASR_CHUNK_THRESHOLD_SEC * 1000)
            .unwrap_or(false);

    if prefer_chunks {
        let chunked = generate_chunked_local_video_subtitle(
            reporter,
            file,
            generator_type,
            whisper_model,
            whisper_prompt,
            openai_api_key,
            openai_api_endpoint,
            language_hint,
        )
        .await;
        if generator_type == "funasr" {
            if let Err(error) = &chunked {
                #[cfg(feature = "local-whisper")]
                {
                    log::warn!("FunASR 分段转写失败，回退本地 Whisper 分段: {error}");
                    if let Some(reporter) = reporter {
                        reporter
                            .update("FunASR分段失败，正在用本地 Whisper 分段重试")
                            .await;
                    }
                    return generate_chunked_local_video_subtitle(
                        reporter,
                        file,
                        "whisper",
                        whisper_model,
                        whisper_prompt,
                        openai_api_key,
                        openai_api_endpoint,
                        language_hint,
                    )
                    .await
                    .map_err(|whisper_error| {
                        format!(
                            "FunASR 分段失败: {error}; Whisper 分段也失败: {whisper_error}"
                        )
                    });
                }
                #[cfg(not(feature = "local-whisper"))]
                {
                    return Err(format!(
                        "中文直播识别（FunASR）分段失败：{error}。当前构建未启用本地 Whisper。"
                    ));
                }
            }
        }
        return chunked;
    }

    let result = generate_video_subtitle_once(
        reporter,
        file,
        generator_type,
        whisper_model,
        whisper_prompt,
        openai_api_key,
        openai_api_endpoint,
        language_hint,
    )
    .await;

    if generator_type == "funasr" {
        if let Err(error) = &result {
            // Only fall back when this binary actually ships local Whisper.
            // Otherwise the fallback masks the real FunASR failure with
            // "Local Whisper is disabled in this build".
            #[cfg(feature = "local-whisper")]
            {
                log::warn!("FunASR failed, falling back to local Whisper: {error}");
                if let Some(reporter) = reporter {
                    reporter.update("FunASR不可用，正在使用本地备用识别").await;
                }
                let whisper_result = generate_video_subtitle_once(
                    reporter,
                    file,
                    "whisper",
                    whisper_model,
                    whisper_prompt,
                    openai_api_key,
                    openai_api_endpoint,
                    language_hint,
                )
                .await;
                return whisper_result.map_err(|whisper_error| {
                    format!(
                        "FunASR failed: {error}; local Whisper fallback also failed: {whisper_error}"
                    )
                });
            }
            #[cfg(not(feature = "local-whisper"))]
            {
                log::warn!("FunASR failed (no Whisper fallback in this build): {error}");
                return Err(format!(
                    "中文直播识别（FunASR）失败：{error}。当前构建未启用本地 Whisper，不会自动回退。请检查 FunASR 服务/模型是否已安装，或改用火山识别。"
                ));
            }
        }
    }
    result
}

/// Parallel Volcengine chunk requests. Keep conservative to avoid rate limits.
const VOLCENGINE_ASR_CONCURRENCY: usize = 3;

fn list_volcengine_mp3_chunks(chunk_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut chunks = std::fs::read_dir(chunk_dir)
        .map_err(|error| format!("读取火山ASR临时分段失败: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("mp3"))
        .collect::<Vec<_>>();
    chunks.sort();
    Ok(chunks)
}

fn load_cached_volcengine_chunk_srt(chunk: &Path) -> Result<Option<GenerateResult>, String> {
    let cached_srt = chunk.with_extension("srt");
    if !cached_srt.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&cached_srt)
        .map_err(|error| format!("读取火山ASR缓存分段失败: {error}"))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(GenerateResult {
        subtitle_id: String::new(),
        subtitle_content: srtparse::from_str(&content)
            .map_err(|error| format!("解析火山ASR缓存分段失败: {error}"))?,
        generator_type: SubtitleGeneratorType::Volcengine,
    }))
}

async fn save_cached_volcengine_chunk_srt(
    chunk: &Path,
    result: &GenerateResult,
) -> Result<(), String> {
    let content = result
        .subtitle_content
        .iter()
        .map(item_to_srt)
        .collect::<String>();
    tokio::fs::write(chunk.with_extension("srt"), content)
        .await
        .map_err(|error| format!("写入火山ASR缓存分段失败: {error}"))
}

/// Convert an MP4 (or any FFmpeg-readable media) into 10-minute, 16kHz mono
/// MP3 chunks. At 64kbps each chunk is about 4.8MB, comfortably below the
/// Volcengine binary-upload recommendation of 20MB.
pub async fn extract_volcengine_audio_chunks(file: &Path) -> Result<Vec<PathBuf>, String> {
    let parent = file
        .parent()
        .ok_or_else(|| "输入视频没有父目录".to_string())?;
    let stem = file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("recording");
    let chunk_dir = parent.join(format!("{stem}.volc-asr-chunks"));
    if chunk_dir.is_dir() {
        let existing = list_volcengine_mp3_chunks(&chunk_dir)?;
        if !existing.is_empty() {
            log::info!(
                "Reusing {} existing Volcengine ASR audio chunks from {}",
                existing.len(),
                chunk_dir.display()
            );
            return Ok(existing);
        }
    }
    tokio::fs::create_dir_all(&chunk_dir)
        .await
        .map_err(|error| format!("创建火山ASR临时目录失败: {error}"))?;
    let pattern = chunk_dir.join("chunk_%05d.mp3");
    let output = ffmpeg_command()
        .arg("-i")
        .arg(file)
        .args(["-vn", "-ar", "16000", "-ac", "1"])
        .args(["-c:a", "libmp3lame", "-b:a", "64k"])
        .args(["-f", "segment", "-segment_time", "600"])
        .args(["-reset_timestamps", "1", "-y"])
        .arg(&pattern)
        .output()
        .await
        .map_err(|error| format!("启动FFmpeg提取火山ASR音频失败: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "提取火山ASR音频失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut chunks = std::fs::read_dir(&chunk_dir)
        .map_err(|error| format!("读取火山ASR临时分段失败: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("mp3"))
        .collect::<Vec<_>>();
    chunks.sort();
    if chunks.is_empty() {
        return Err("视频中没有可提取的音轨".to_string());
    }
    Ok(chunks)
}

#[allow(clippy::too_many_arguments)]
pub async fn generate_volcengine_video_subtitle(
    reporter: Option<&ProgressReporter>,
    file: &Path,
    api_key: &str,
    app_id: &str,
    access_token: &str,
    resource_id: &str,
    boosting_table_id: &str,
    correct_table_id: &str,
) -> Result<GenerateResult, String> {
    let audit_path =
        crate::subtitle_generator::transcript_artifacts::TranscriptArtifactStore::asr_audit_path(
            file,
        );
    match tokio::fs::remove_file(&audit_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("清理旧火山ASR审计文件失败: {error}")),
    }
    if let Some(reporter) = reporter {
        reporter.update("正在从MP4提取火山ASR音频").await;
    }
    let chunks = extract_volcengine_audio_chunks(file).await?;
    let chunk_dir = chunks
        .first()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf);
    let client = crate::subtitle_generator::volcengine::VolcengineAsr::new(
        api_key,
        app_id,
        access_token,
        resource_id,
        boosting_table_id,
        correct_table_id,
    )?;
    let danmu_timeline =
        match crate::subtitle_generator::asr_context::DanmuTimeline::load_for_media(file) {
            Ok(timeline) => timeline,
            Err(error) => {
                log::warn!("Dynamic ASR context unavailable: {error}");
                None
            }
        };
    let total_chunks = chunks.len();
    let mut chunk_results: Vec<Option<GenerateResult>> = vec![None; total_chunks];
    let mut pending_indices = Vec::new();
    let mut cached_count = 0usize;

    for (index, chunk) in chunks.iter().enumerate() {
        match load_cached_volcengine_chunk_srt(chunk)? {
            Some(cached) => {
                chunk_results[index] = Some(cached);
                cached_count += 1;
            }
            None => pending_indices.push(index),
        }
    }

    if cached_count > 0 {
        log::info!("Reusing {cached_count}/{total_chunks} cached Volcengine ASR chunk transcripts");
    }

    if !pending_indices.is_empty() {
        if let Some(reporter) = reporter {
            reporter
                .update(&format!(
                    "火山ASR识别中（待处理 {} 段，已缓存 {} 段，并发 {}）",
                    pending_indices.len(),
                    cached_count,
                    VOLCENGINE_ASR_CONCURRENCY
                ))
                .await;
        }

        let semaphore = Arc::new(Semaphore::new(VOLCENGINE_ASR_CONCURRENCY));
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(cached_count));
        let mut join_set = JoinSet::new();

        for index in pending_indices {
            let chunk = chunks[index].clone();
            let client = client.clone();
            let danmu_timeline = danmu_timeline.clone();
            let semaphore = Arc::clone(&semaphore);
            let completed = Arc::clone(&completed);
            let reporter = reporter.cloned();

            join_set.spawn(async move {
                let _permit = semaphore
                    .acquire_owned()
                    .await
                    .map_err(|error| format!("火山ASR并发控制失败: {error}"))?;

                let chunk_context = danmu_timeline
                    .as_ref()
                    .and_then(|timeline| timeline.for_chunk(index as u64 * 600_000, 600_000));

                let result = client
                    .recognize_file(
                        &chunk,
                        chunk_context
                            .as_ref()
                            .map(|context| context.payload.as_str()),
                    )
                    .await
                    .map_err(|error| {
                        format!("火山ASR第 {}/{} 段失败: {error}", index + 1, total_chunks)
                    })?;

                save_cached_volcengine_chunk_srt(&chunk, &result).await?;

                let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if let Some(reporter) = reporter.as_ref() {
                    reporter
                        .update(&format!(
                            "火山ASR识别第 {}/{} 段（{done}/{total_chunks} 完成）",
                            index + 1,
                            total_chunks
                        ))
                        .await;
                }

                Ok::<(usize, GenerateResult), String>((index, result))
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            let (index, result) =
                join_result.map_err(|error| format!("火山ASR分段任务异常: {error}"))??;
            chunk_results[index] = Some(result);
        }
    }

    let mut full_result = GenerateResult {
        generator_type: SubtitleGeneratorType::Volcengine,
        subtitle_id: String::new(),
        subtitle_content: Vec::new(),
    };
    for (index, chunk_result) in chunk_results.iter().enumerate() {
        let result = chunk_result
            .as_ref()
            .ok_or_else(|| format!("火山ASR第 {}/{} 段缺少识别结果", index + 1, total_chunks))?;
        full_result.concat_with_offset_ms(result, index as u64 * 600_000);
    }
    if let Some(timeline) = &danmu_timeline {
        let (audited_result, audit) = audited_calibration::apply_calibration_if_audited(
            full_result,
            |candidate| {
                crate::subtitle_generator::asr_evidence::calibrate_with_danmu(candidate, timeline)
            },
            |audit| audited_calibration::persist_json_durably(&audit_path, audit),
        );
        full_result = audited_result;
        match audit {
            Ok(audit) => {
                log::info!(
                    "ASR evidence calibration: changes={}, quality_flags={}",
                    audit.changes.len(),
                    audit.quality_flags.len()
                );
            }
            Err(error) => {
                log::warn!(
                    "ASR evidence calibration audit was not persisted; using original text: {error}"
                );
            }
        }
    }
    if let Some(path) = &chunk_dir {
        log::info!(
            "Keeping Volcengine ASR chunk cache for faster resume: {}",
            path.display()
        );
    }
    if full_result.subtitle_content.is_empty() {
        return Err("火山ASR没有识别到可用文字".to_string());
    }
    Ok(full_result)
}

#[allow(clippy::too_many_arguments)]
async fn generate_video_subtitle_once(
    reporter: Option<&ProgressReporter>,
    file: &Path,
    generator_type: &str,
    whisper_model: &str,
    whisper_prompt: &str,
    openai_api_key: &str,
    openai_api_endpoint: &str,
    language_hint: &str,
) -> Result<GenerateResult, String> {
    match generator_type {
        "funasr" => {
            if let Some(reporter) = reporter {
                reporter.update("正在提取音频").await;
            }
            // Windowed deal transcription already supplies a normalized WAV.
            // Reuse it directly instead of making a second full-size copy.
            let use_input_wav = file.extension().and_then(|value| value.to_str()) == Some("wav");
            let full_wav = if use_input_wav {
                file.to_path_buf()
            } else {
                extract_full_audio(file).await?
            };
            let fact_card_path = file.with_extension("facts.json");
            let fact_card = if fact_card_path.is_file() {
                match tokio::fs::read_to_string(&fact_card_path).await {
                    Ok(content) => match serde_json::from_str(&content) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            log::warn!(
                                "Ignoring invalid ASR fact card {:?}: {error}",
                                fact_card_path
                            );
                            None
                        }
                    },
                    Err(error) => {
                        log::warn!("Failed to read ASR fact card {:?}: {error}", fact_card_path);
                        None
                    }
                }
            } else {
                None
            };

            if let Some(reporter) = reporter {
                reporter
                    .update("首次使用正在加载中文识别模型，后续会更快")
                    .await;
            }
            let response = match funasr::FunAsr::new().await {
                Ok(generator) => {
                    if let Some(reporter) = reporter {
                        reporter.update("正在生成逐字稿").await;
                    }
                    generator.transcribe(&full_wav, fact_card.clone()).await
                }
                Err(error) => Err(error),
            };
            if !use_input_wav {
                let _ = tokio::fs::remove_file(&full_wav).await;
            }
            let mut response = response?;

            if let Some(reporter) = reporter {
                reporter.update("正在校对商品和价格").await;
            }
            if !openai_api_key.trim().is_empty() {
                match crate::handlers::ai::minimax_correct_transcript(
                    openai_api_key,
                    &response.raw_text,
                    &response.corrected_text,
                    fact_card.as_ref(),
                )
                .await
                {
                    Ok((corrected_text, accepted_changes)) => {
                        for change in &accepted_changes {
                            for segment in &mut response.segments {
                                if segment.text.contains(&change.source) {
                                    segment.text =
                                        segment.text.replacen(&change.source, &change.corrected, 1);
                                }
                            }
                            for item in &mut response.review_items {
                                if item.recognized.contains(&change.source) {
                                    item.recognized = item.recognized.replacen(
                                        &change.source,
                                        &change.corrected,
                                        1,
                                    );
                                }
                                if item.context.contains(&change.source) {
                                    item.context =
                                        item.context.replacen(&change.source, &change.corrected, 1);
                                }
                            }
                        }
                        response.corrected_text = corrected_text;
                        response.changes.extend(accepted_changes);
                    }
                    Err(error) => {
                        log::warn!("MiniMax constrained transcript correction skipped: {error}");
                        response.changes.push(funasr::CorrectionChange {
                            source: String::new(),
                            corrected: String::new(),
                            change_type: "MiniMax校对未执行".to_string(),
                            evidence: error,
                            needs_review: true,
                        });
                    }
                }
            }

            log::info!(
                "FunASR completed via {} in {:.3}s (model load {:.3}s)",
                response.engine,
                response.inference_seconds,
                response.model_load_seconds
            );

            // Preserve all three audit artifacts. The corrected transcript is
            // used for SRT, while the raw transcript is never overwritten.
            let raw_path = file.with_extension("asr.raw.txt");
            let raw_srt_path = file.with_extension("asr.raw.srt");
            let corrected_path = file.with_extension("asr.corrected.txt");
            let corrected_srt_path = file.with_extension("asr.corrected.srt");
            let changes_path = file.with_extension("asr.changes.json");
            let review_path = file.with_extension("asr.review.json");
            tokio::fs::write(&raw_path, &response.raw_text)
                .await
                .map_err(|e| format!("Failed to save raw FunASR transcript: {e}"))?;
            tokio::fs::write(
                &raw_srt_path,
                funasr::segments_to_srt(&response.raw_segments),
            )
            .await
            .map_err(|e| format!("Failed to save raw FunASR SRT: {e}"))?;
            tokio::fs::write(&corrected_path, &response.corrected_text)
                .await
                .map_err(|e| format!("Failed to save corrected FunASR transcript: {e}"))?;
            tokio::fs::write(
                &corrected_srt_path,
                funasr::segments_to_srt(&response.segments),
            )
            .await
            .map_err(|e| format!("Failed to save corrected FunASR SRT: {e}"))?;
            let changes = serde_json::to_vec_pretty(&response.changes)
                .map_err(|e| format!("Failed to serialize FunASR change log: {e}"))?;
            tokio::fs::write(&changes_path, changes)
                .await
                .map_err(|e| format!("Failed to save FunASR change log: {e}"))?;
            let review_items = serde_json::to_vec_pretty(&response.review_items)
                .map_err(|e| format!("Failed to serialize review items: {e}"))?;
            tokio::fs::write(&review_path, review_items)
                .await
                .map_err(|e| format!("Failed to save ASR review list: {e}"))?;

            Ok(funasr::into_generate_result(&response))
        }
        #[cfg(feature = "local-whisper")]
        "whisper" => {
            if whisper_model.is_empty() {
                return Err("Whisper model not configured".to_string());
            }
            let generator = match whisper_cpp::new(Path::new(whisper_model), whisper_prompt).await {
                Ok(g) => g,
                Err(e) => return Err(format!("Failed to initialize Whisper model: {e}")),
            };

            // Extract full audio as single 16kHz mono WAV
            if let Some(reporter) = reporter {
                reporter.update("提取完整音频中").await;
            }
            let full_wav = extract_full_audio(file).await?;

            // Read samples for VAD
            let audio = hound::WavReader::open(&full_wav).map_err(|e| e.to_string())?;
            let spec = audio.spec();
            let sample_rate = spec.sample_rate;
            let raw_samples: Vec<i16> = audio
                .into_samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed to decode WAV samples: {e}"))?;
            let mut f32_samples = vec![0.0f32; raw_samples.len()];
            whisper_cpp_rs::convert_integer_to_float_audio(&raw_samples, &mut f32_samples)
                .map_err(|e| format!("Audio conversion error: {e}"))?;

            // VAD: find speech segments
            if let Some(reporter) = reporter {
                reporter.update("检测语音片段中").await;
            }
            let (speech_segments, energies, frame_sec) =
                crate::audio_utils::energy_vad_with_energies(&f32_samples, sample_rate);
            log::info!(
                "VAD detected {} speech segments from {:.1}s audio",
                speech_segments.len(),
                f32_samples.len() as f64 / sample_rate as f64
            );

            // Cut & Merge: normalize to ≤30s chunks
            let max_chunk = 30.0;
            let chunks = crate::audio_utils::cut_and_merge(
                &speech_segments,
                &energies,
                frame_sec,
                max_chunk, // cut_max: split segments >30s
                10.0,      // merge_max: merge adjacent segments ≤10s
            );
            log::info!(
                "Cut & Merge: {} speech segments → {} chunks (≤{}s each)",
                speech_segments.len(),
                chunks.len(),
                max_chunk
            );

            // Process each chunk
            let mut results = Vec::new();
            let temp_dir =
                std::env::temp_dir().join(format!("bsr_whisper_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("Failed to create temp dir: {e}"))?;

            let chunk_padding = 0.35;
            for (i, chunk) in chunks.iter().enumerate() {
                let segment_path = temp_dir.join(format!("seg_{:03}.wav", i));
                let padded_start = (chunk.start - chunk_padding).max(0.0);
                let padded_end = chunk.end + chunk_padding;
                let duration = padded_end - padded_start;
                if let Some(reporter) = reporter {
                    reporter
                        .update(&format!(
                            "字幕生成中 ({}/{}, {:.0}s-{:.0}s)",
                            i + 1,
                            chunks.len(),
                            padded_start,
                            padded_end
                        ))
                        .await;
                }
                // Trim the original video to get this segment's audio
                match extract_audio_segment(file, padded_start, duration, &segment_path).await {
                    Ok(()) => {
                        let result = generator
                            .generate_subtitle(reporter, &segment_path, language_hint)
                            .await;
                        results.push(((padded_start * 1000.0).round() as u64, result));
                    }
                    Err(e) => {
                        log::error!("Failed to extract segment {}: {e}", i);
                        continue;
                    }
                }
            }

            // Clean up temp files
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            let _ = tokio::fs::remove_file(&full_wav).await;

            // Stitch results with time offsets
            let mut full_result = GenerateResult {
                subtitle_id: String::new(),
                subtitle_content: vec![],
                generator_type: SubtitleGeneratorType::Whisper,
            };

            for (offset_ms, result) in &results {
                if let Ok(result) = result {
                    full_result.subtitle_id = result.subtitle_id.clone();
                    full_result.concat_with_offset_ms(result, *offset_ms);
                }
            }

            Ok(full_result)
        }
        #[cfg(not(feature = "local-whisper"))]
        "whisper" => Err("Local Whisper is disabled in this build. Use FunASR, Volcengine, or enable the local-whisper feature.".to_string()),
        "whisper_online" => {
            if openai_api_key.is_empty() {
                return Err("API key not configured".to_string());
            }
            if let Ok(generator) = whisper_online::new(
                Some(openai_api_endpoint),
                Some(openai_api_key),
                Some(whisper_prompt),
            )
            .await
            {
                let chunk_dir = extract_audio_chunks(file, "mp3").await?;

                let mut full_result = GenerateResult {
                    subtitle_id: String::new(),
                    subtitle_content: vec![],
                    generator_type: SubtitleGeneratorType::WhisperOnline,
                };

                let mut chunk_paths = vec![];
                for entry in std::fs::read_dir(&chunk_dir)
                    .map_err(|e| format!("Failed to read chunk directory: {e}"))?
                {
                    let entry =
                        entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
                    let path = entry.path();
                    chunk_paths.push(path);
                }
                // sort chunk paths by name
                chunk_paths
                    .sort_by_key(|path| path.file_name().unwrap().to_str().unwrap().to_string());

                let mut results = Vec::new();
                for path in chunk_paths {
                    let result = generator
                        .generate_subtitle(reporter, &path, language_hint)
                        .await;
                    results.push(result);
                }

                for (i, result) in results.iter().enumerate() {
                    if let Ok(result) = result {
                        full_result.subtitle_id = result.subtitle_id.clone();
                        full_result.concat(result, 30 * i as u64);
                    }
                }

                // delete chunk directory
                let _ = tokio::fs::remove_dir_all(chunk_dir).await;

                Ok(full_result)
            } else {
                Err("Failed to initialize Whisper Online".to_string())
            }
        }
        "powerlive" => {
            if let Ok(generator) = powerlive::new(
                "pk_d2755cd38ef03f7ed3a92be1f1471e4adea90a1a5d4b3900345298a68fba0821",
            )
            .await
            {
                let opus_file = file.with_extension("opus");
                if !opus_file.exists() {
                    return Err("Opus file not found".to_string());
                }
                let result = generator
                    .generate_subtitle(reporter, &opus_file, language_hint)
                    .await;
                match result {
                    Ok(result) => Ok(result),
                    Err(e) => Err(e),
                }
            } else {
                Err("Failed to initialize PowerLive".to_string())
            }
        }
        _ => Err(format!("Unknown subtitle generator type: {generator_type}")),
    }
}

/// Trying to run ffmpeg for version
pub async fn check_ffmpeg() -> Result<String, String> {
    let child = tokio::process::Command::new(ffmpeg_path())
        .arg("-version")
        .stdout(Stdio::piped())
        .spawn();
    if let Err(e) = child {
        log::error!("Failed to spawn ffmpeg process: {e}");
        return Err(e.to_string());
    }

    let mut child = child.unwrap();

    let stdout = child.stdout.take();
    if stdout.is_none() {
        log::error!("Failed to take ffmpeg output");
        return Err("Failed to take ffmpeg output".into());
    }

    let stdout = stdout.unwrap();
    let reader = BufReader::new(stdout);
    let mut parser = FfmpegLogParser::new(reader);

    let mut version = None;
    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::ParsedVersion(v) => version = Some(v.version),
            FfmpegEvent::LogEOF => break,
            _ => {}
        }
    }

    if let Some(version) = version {
        Ok(version)
    } else {
        Err("Failed to parse version from output".into())
    }
}

pub fn ffmpeg_command() -> tokio::process::Command {
    let mut command = tokio::process::Command::new(ffmpeg_path());
    command.kill_on_drop(true);
    command
}

pub fn ffmpeg_path() -> PathBuf {
    resolve_bundled_binary("ffmpeg")
}

/// FFmpeg on Windows opens UNC shares more reliably with forward slashes.
fn ffmpeg_cli_media_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        if raw.starts_with("\\\\") || raw.starts_with("//") {
            return PathBuf::from(raw.replace('\\', "/"));
        }
    }
    path.to_path_buf()
}

fn ffprobe_path() -> PathBuf {
    resolve_bundled_binary("ffprobe")
}

fn resolve_bundled_binary(name: &str) -> PathBuf {
    let mut file_name = PathBuf::from(name);
    if cfg!(windows) {
        file_name.set_extension("exe");
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(&file_name);
            if candidate.is_file() {
                return candidate;
            }

            // Tauri bundles macOS resources in <App>.app/Contents/Resources,
            // while the executable lives in <App>.app/Contents/MacOS.
            #[cfg(target_os = "macos")]
            if let Some(contents) = dir.parent() {
                let candidate = contents.join("Resources").join(&file_name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }

    file_name
}

// 从视频文件切片
pub async fn clip_from_video_file(
    reporter: Option<&impl ProgressReporterTrait>,
    input_path: &Path,
    output_path: &Path,
    start_time: f64,
    duration: f64,
) -> Result<(), String> {
    let video_encoder = hwaccel::get_x264_encoder().await;
    match clip_from_video_file_with_encoder(
        reporter,
        input_path,
        output_path,
        start_time,
        duration,
        &video_encoder,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(hardware_error) if video_encoder != "libx264" => {
            log::warn!(
                "切片硬件编码器 {video_encoder} 失败，自动使用 libx264 重试：{hardware_error}"
            );
            let _ = tokio::fs::remove_file(output_path).await;
            clip_from_video_file_with_encoder(
                reporter,
                input_path,
                output_path,
                start_time,
                duration,
                "libx264",
            )
            .await
            .map_err(|software_error| {
                format!("切片硬件与软件编码均失败。硬件：{hardware_error}；软件：{software_error}")
            })
        }
        Err(error) => Err(error),
    }
}

async fn clip_from_video_file_with_encoder(
    reporter: Option<&impl ProgressReporterTrait>,
    input_path: &Path,
    output_path: &Path,
    start_time: f64,
    duration: f64,
    video_encoder: &str,
) -> Result<(), String> {
    if !input_path.is_file() {
        return Err(format!(
            "切片源文件不存在或当前不可访问：{}",
            input_path.display()
        ));
    }
    if !(duration.is_finite() && duration > 0.0) {
        return Err(format!("切片时长无效：{duration}"));
    }

    let ffmpeg_input = ffmpeg_cli_media_path(input_path);

    let output_folder = output_path
        .parent()
        .ok_or_else(|| format!("切片输出路径无效：{}", output_path.display()))?;
    if !output_folder.exists() {
        std::fs::create_dir_all(output_folder)
            .map_err(|e| format!("无法创建切片目录 {}：{e}", output_folder.display()))?;
    }

    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    // Pass Path directly so Windows keeps wide/UNC paths intact.
    // Seek slightly before the requested boundary, then perform the remaining
    // seek after input. HEVC transport streams frequently begin between GOPs;
    // the pre-roll gives the decoder reference frames without scanning from 0.
    let fast_seek_start = (start_time - 12.0).max(0.0);
    let accurate_seek = start_time - fast_seek_start;
    ffmpeg_process
        .args(["-ss", &fast_seek_start.to_string()])
        // Network TS recordings can start between HEVC keyframes.  Keep the
        // fast seek, generate timestamps, and discard corrupt pre-roll rather
        // than aborting the whole learning clip on those recoverable frames.
        .args(["-fflags", "+genpts+discardcorrupt"])
        .args(["-err_detect", "ignore_err"])
        .arg("-i")
        .arg(&ffmpeg_input)
        .args(["-ss", &accurate_seek.to_string()])
        .args(["-t", &duration.to_string()])
        .args(["-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn"]);
    hwaccel::apply_x264_encoder_args(&mut ffmpeg_process, video_encoder, None);
    ffmpeg_process.args(["-c:a", "aac"]);
    if video_encoder == "h264_amf" {
        ffmpeg_process.args(["-quality", "speed"]);
    } else if video_encoder == "libx264" {
        ffmpeg_process.args(["-preset", "veryfast", "-crf", "20"]);
    } else {
        hwaccel::apply_x264_quality_args(&mut ffmpeg_process, video_encoder);
    }
    ffmpeg_process.kill_on_drop(true);
    let child = ffmpeg_process
        .args(["-avoid_negative_ts", "make_zero"])
        .args(["-movflags", "+faststart"])
        // `-progress` is a global option.  It must precede the output path;
        // placing it after the path makes FFmpeg parse it as another output
        // option and the clip job exits with `Invalid argument` / -22.
        .args(["-progress", "pipe:2"])
        .arg("-y")
        .arg(output_path)
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            return Err(format!(
                "启动ffmpeg进程失败（{}）：{e}",
                ffmpeg_path().display()
            ));
        }
    };
    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    let mut clip_error = None;
    let mut last_log = String::new();
    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Progress(p) => {
                if let Some(reporter) = reporter {
                    reporter.update(&format!("切片进度: {}", p.time)).await;
                }
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Log(level, content) => {
                if content.to_ascii_lowercase().contains("error") || level == LogLevel::Error {
                    log::error!("切片错误: {content}");
                    last_log = content;
                }
            }
            FfmpegEvent::Error(e) => {
                log::error!("切片错误: {e}");
                clip_error = Some(e.to_string());
            }
            _ => {}
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("等待ffmpeg结束失败: {e}"))?;

    if let Some(error) = clip_error {
        return Err(format!("切片失败（源: {}）：{error}", input_path.display()));
    }
    if !status.success() {
        return Err(format!(
            "切片失败（源: {}，退出码: {}）{}",
            input_path.display(),
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            if last_log.is_empty() {
                String::new()
            } else {
                format!("：{last_log}")
            }
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "切片完成但未生成文件（源: {} → 输出: {}）{}",
            input_path.display(),
            output_path.display(),
            if last_log.is_empty() {
                String::new()
            } else {
                format!("：{last_log}")
            }
        ));
    }

    log::info!(
        "切片任务完成（编码器: {}）: {}",
        video_encoder,
        output_path.display()
    );
    Ok(())
}

/// Exports an MP4 learning segment. It first attempts a stream copy so a short
/// selection is available almost immediately. If the source/container cannot be
/// copied into MP4, it retries with H.264/AAC for broadly compatible output.
pub async fn export_learning_segment_mp4(
    input_path: &Path,
    output_path: &Path,
    start_time: f64,
    duration: f64,
) -> Result<bool, String> {
    if !input_path.is_file() {
        return Err(format!(
            "Learning segment source file does not exist: {}",
            input_path.display()
        ));
    }
    if !(start_time.is_finite() && start_time >= 0.0 && duration.is_finite() && duration > 0.0) {
        return Err(
            "Learning segment timestamps must be finite, with start >= 0 and duration > 0"
                .to_string(),
        );
    }
    let output_dir = output_path
        .parent()
        .ok_or_else(|| "Learning segment output path has no parent directory".to_string())?;
    std::fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "Unable to create learning segment output directory {}: {error}",
            output_dir.display()
        )
    })?;

    async fn run(
        input_path: &Path,
        output_path: &Path,
        start_time: f64,
        duration: f64,
        copy: bool,
    ) -> Result<(), String> {
        let _ = tokio::fs::remove_file(output_path).await;
        let ffmpeg_input = ffmpeg_cli_media_path(input_path);
        let mut command = ffmpeg_command();
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);

        command
            .args(["-ss", &start_time.to_string()])
            .arg("-i")
            .arg(ffmpeg_input)
            .args(["-t", &duration.to_string(), "-map", "0"]);
        if copy {
            command.args(["-c", "copy"]);
        } else {
            command.args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-c:a", "aac", "-b:a",
                "192k",
            ]);
        }
        let output = command
            .args([
                "-avoid_negative_ts",
                "make_zero",
                "-movflags",
                "+faststart",
                "-y",
            ])
            .arg(output_path)
            .output()
            .await
            .map_err(|error| format!("Unable to start FFmpeg: {error}"))?;
        if output.status.success() && output_path.is_file() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    match run(input_path, output_path, start_time, duration, true).await {
        Ok(()) => Ok(false),
        Err(copy_error) => {
            log::warn!(
                "Learning segment stream-copy export failed; retrying with H.264/AAC: {copy_error}"
            );
            run(input_path, output_path, start_time, duration, false)
                .await
                .map_err(|encode_error| format!("Learning segment export failed. Stream copy: {copy_error}; re-encode: {encode_error}"))?;
            Ok(true)
        }
    }
}

/// Extract basic information from a video file.
///
/// # Arguments
/// * `file_path` - The path to the video file.
///
/// # Returns
/// A `Result` containing the video metadata or an error message.
pub async fn extract_video_metadata(file_path: &Path) -> Result<VideoMetadata, String> {
    let mut ffprobe_process = tokio::process::Command::new("ffprobe");
    #[cfg(target_os = "windows")]
    ffprobe_process.creation_flags(CREATE_NO_WINDOW);

    let output = ffprobe_process
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            &format!("{}", file_path.display()),
        ])
        .output()
        .await
        .map_err(|e| format!("执行ffprobe失败: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe执行失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("解析ffprobe输出失败: {e}"))?;

    // 解析视频流信息
    let streams = json["streams"].as_array().ok_or("未找到视频流信息")?;

    if streams.is_empty() {
        return Err("未找到视频流".to_string());
    }

    let mut metadata = VideoMetadata {
        duration: 0.0,
        width: 0,
        height: 0,
        video_codec: String::new(),
        audio_codec: String::new(),
    };

    for stream in streams {
        let codec_name = stream["codec_type"].as_str().unwrap_or("");
        if codec_name == "video" {
            metadata.video_codec = stream["codec_name"].as_str().unwrap_or("").to_owned();
            metadata.width = stream["width"].as_u64().unwrap_or(0) as u32;
            metadata.height = stream["height"].as_u64().unwrap_or(0) as u32;
            metadata.duration = stream["duration"]
                .as_str()
                .unwrap_or("0.0")
                .parse::<f64>()
                .unwrap_or(0.0);
        } else if codec_name == "audio" {
            metadata.audio_codec = stream["codec_name"].as_str().unwrap_or("").to_owned();
        }
    }
    Ok(metadata)
}

/// Generate thumbnail file from video, capturing a frame at the specified timestamp.
///
/// # Arguments
/// * `video_full_path` - The full path to the video file.
/// * `timestamp` - The timestamp (in seconds) to capture the thumbnail.
///
/// # Returns
/// The path to the generated thumbnail image.
pub async fn generate_thumbnail(video_full_path: &Path, timestamp: f64) -> Result<PathBuf, String> {
    let mut ffmpeg_process = ffmpeg_command();
    #[cfg(target_os = "windows")]
    ffmpeg_process.creation_flags(CREATE_NO_WINDOW);

    let thumbnail_full_path = video_full_path.with_extension("jpg");

    let output = ffmpeg_process
        .args(["-i", &format!("{}", video_full_path.display())])
        .args(["-ss", &timestamp.to_string()])
        .args(["-vframes", "1"])
        .args(["-y", thumbnail_full_path.to_str().unwrap()])
        .output()
        .await
        .map_err(|e| format!("生成缩略图失败: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffmpeg生成缩略图失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // 记录生成的缩略图信息
    if let Ok(metadata) = std::fs::metadata(&thumbnail_full_path) {
        log::info!(
            "生成缩略图完成: {} (文件大小: {} bytes)",
            thumbnail_full_path.display(),
            metadata.len()
        );
    } else {
        log::info!("生成缩略图完成: {}", thumbnail_full_path.display());
    }
    Ok(thumbnail_full_path)
}

// 执行FFmpeg转换的通用函数
fn format_conversion_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0).ceil() as u64;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes > 0 {
        format!("{minutes}分{seconds}秒")
    } else {
        format!("{seconds}秒")
    }
}

fn parse_ffmpeg_progress_time(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn format_conversion_progress(
    processed_secs: f64,
    total_secs: f64,
    elapsed_secs: f64,
    mode_name: &str,
) -> String {
    if total_secs <= 0.0 || elapsed_secs <= 0.0 {
        return format!(
            "正在转换视频格式... {:.1}秒（{mode_name}）",
            processed_secs.max(0.0)
        );
    }
    let processed_secs = processed_secs.clamp(0.0, total_secs);
    let percent = processed_secs / total_secs * 100.0;
    let speed = processed_secs / elapsed_secs;
    let remaining_secs = (total_secs - processed_secs) / speed.max(0.01);
    format!(
        "转换 {:.0}% · {:.1}x · 预计剩余 {}（{mode_name}）",
        percent,
        speed,
        format_conversion_duration(remaining_secs)
    )
}

pub async fn execute_ffmpeg_conversion(
    cmd: tokio::process::Command,
    reporter: &ProgressReporter,
    mode_name: &str,
) -> Result<(), String> {
    execute_ffmpeg_conversion_with_duration(cmd, reporter, mode_name, None).await
}

async fn execute_ffmpeg_conversion_with_duration(
    mut cmd: tokio::process::Command,
    reporter: &ProgressReporter,
    mode_name: &str,
    total_duration_secs: Option<f64>,
) -> Result<(), String> {
    use async_ffmpeg_sidecar::event::FfmpegEvent;
    use async_ffmpeg_sidecar::log_parser::FfmpegLogParser;
    use std::process::Stdio;
    use tokio::io::BufReader;

    // TaskManager cancellation aborts the owning future. Without this flag,
    // dropping Tokio's Child leaves FFmpeg running even though the task row is
    // already marked cancelled.
    cmd.kill_on_drop(true);
    let mut child = cmd
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动FFmpeg进程失败: {e}"))?;

    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);
    let mut parser = FfmpegLogParser::new(reader);

    let started_at = Instant::now();
    let mut conversion_error = None;
    while let Ok(event) = parser.parse_next_event().await {
        match event {
            FfmpegEvent::Progress(p) => {
                let message = total_duration_secs
                    .map(|total| {
                        parse_ffmpeg_progress_time(&p.time)
                            .map(|processed| {
                                format_conversion_progress(
                                    processed,
                                    total,
                                    started_at.elapsed().as_secs_f64(),
                                    mode_name,
                                )
                            })
                            .unwrap_or_else(|| {
                                format!("正在转换视频格式... {} ({})", p.time, mode_name)
                            })
                    })
                    .unwrap_or_else(|| format!("正在转换视频格式... {} ({})", p.time, mode_name));
                reporter.update(&message).await;
            }
            FfmpegEvent::LogEOF => break,
            FfmpegEvent::Log(level, content) => {
                if matches!(level, async_ffmpeg_sidecar::event::LogLevel::Error)
                    && content.contains("Error")
                {
                    conversion_error = Some(content);
                }
            }
            FfmpegEvent::Error(e) => {
                conversion_error = Some(e);
            }
            _ => {} // 忽略其他事件类型
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("等待FFmpeg进程失败: {e}"))?;

    if !status.success() {
        let error_msg = conversion_error
            .unwrap_or_else(|| format!("FFmpeg退出码: {}", status.code().unwrap_or(-1)));
        return Err(format!("视频格式转换失败 ({mode_name}): {error_msg}"));
    }

    reporter
        .update(&format!("视频格式转换完成 100% ({mode_name})"))
        .await;
    Ok(())
}

// 尝试流复制转换（无损，速度快）
pub async fn try_stream_copy_conversion(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    reporter.update("正在转换视频格式... 0% (无损模式)").await;

    // 构建ffmpeg命令 - 流复制模式
    let mut cmd = tokio::process::Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    cmd.args([
        "-i",
        &source.to_string_lossy(),
        "-c:v",
        "copy", // 直接复制视频流，零损失
        "-c:a",
        "copy", // 直接复制音频流，零损失
        "-avoid_negative_ts",
        "make_zero", // 修复时间戳问题
        "-movflags",
        "+faststart", // 优化web播放
        "-progress",
        "pipe:2", // 输出进度到stderr
        "-y",     // 覆盖输出文件
        &dest.to_string_lossy(),
    ]);

    execute_ffmpeg_conversion(cmd, reporter, "无损转换").await
}

// 高质量重编码转换（兼容性好，质量高）
pub async fn try_high_quality_conversion(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    reporter.update("正在转换视频格式... 0% (高质量模式)").await;

    // 构建ffmpeg命令 - 高质量重编码
    let mut cmd = tokio::process::Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    cmd.args([
        "-i",
        &source.to_string_lossy(),
        "-c:v",
        "libx264", // H.264编码器
        "-preset",
        "slow", // 慢速预设，更好的压缩效率
        "-crf",
        "18", // 高质量设置 (18-23范围，越小质量越高)
        "-c:a",
        "aac", // AAC音频编码器
        "-b:a",
        "192k", // 高音频码率
        "-avoid_negative_ts",
        "make_zero", // 修复时间戳问题
        "-movflags",
        "+faststart", // 优化web播放
        "-progress",
        "pipe:2", // 输出进度到stderr
        "-y",     // 覆盖输出文件
        &dest.to_string_lossy(),
    ]);

    execute_ffmpeg_conversion(cmd, reporter, "高质量转换").await
}

/// Many recorders (including Windows Screen Sketch) write `mdat` first and `moov`
/// at EOF. WebView/HTML5 players then show a black frame at 0:00 until the full
/// file is fetched, or fail outright over HTTP range requests.
pub fn mp4_moov_at_end(source_path: &Path) -> Result<bool, String> {
    use std::io::{Read, Seek, SeekFrom};

    let metadata = std::fs::metadata(source_path).map_err(|error| error.to_string())?;
    let file_len = metadata.len();
    if file_len < 12 {
        return Ok(false);
    }

    let mut file = std::fs::File::open(source_path).map_err(|error| error.to_string())?;
    let head_len = usize::try_from(file_len.min(1024 * 1024)).unwrap_or(1024 * 1024);
    let mut head = vec![0u8; head_len];
    file.read_exact(&mut head)
        .map_err(|error| format!("读取 MP4 头部失败: {error}"))?;
    let head_has_mdat = head.windows(4).any(|window| window == b"mdat");
    let head_has_moov = head.windows(4).any(|window| window == b"moov");
    if head_has_moov {
        return Ok(false);
    }

    let tail_len = usize::try_from(file_len.min(2 * 1024 * 1024)).unwrap_or(2 * 1024 * 1024);
    let mut tail = vec![0u8; tail_len];
    file.seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|error| format!("读取 MP4 尾部失败: {error}"))?;
    file.read_exact(&mut tail)
        .map_err(|error| format!("读取 MP4 尾部失败: {error}"))?;
    let tail_has_moov = tail.windows(4).any(|window| window == b"moov");

    Ok(head_has_mdat && tail_has_moov)
}

/// Move the `moov` atom to the front for streaming/web playback without re-encoding.
pub async fn remux_mp4_faststart(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    remux_mp4_faststart_with_duration(source, dest, reporter, None).await
}

pub async fn remux_mp4_faststart_with_duration(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
    total_duration_secs: Option<f64>,
) -> Result<(), String> {
    reporter
        .update("正在优化 MP4 网页播放结构（不重编码）…")
        .await;

    let mut cmd = tokio::process::Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    cmd.args([
        "-fflags",
        "+genpts+igndts",
        "-i",
        &source.to_string_lossy(),
        "-c",
        "copy",
        "-avoid_negative_ts",
        "make_zero",
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:2",
        "-y",
        &dest.to_string_lossy(),
    ]);
    execute_ffmpeg_conversion_with_duration(cmd, reporter, "MP4 网页优化", total_duration_secs)
        .await
}

/// Build a seekable HLS presentation from a browser-compatible source without
/// touching its original bytes.  This is deliberately a real segmented
/// playlist, rather than a one-entry playlist that points at a multi-GB TS
/// file: WebView MediaSource implementations can otherwise block while
/// parsing the whole transport stream and make the desktop application appear
/// unresponsive.
pub async fn package_hls_for_browser(
    source: &Path,
    playlist: &Path,
    segment_pattern: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    let parent = playlist
        .parent()
        .ok_or_else(|| "HLS playlist path has no parent directory".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("Failed to create HLS directory: {error}"))?;

    reporter
        .update("正在建立项目内嵌 TS 播放索引（不重编码、不修改原文件）…")
        .await;
    let mut cmd = tokio::process::Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    cmd.args([
        "-fflags",
        "+genpts+igndts",
        "-i",
        &source.to_string_lossy(),
        "-map",
        "0:v:0",
        "-map",
        "0:a:0?",
        "-c",
        "copy",
        "-avoid_negative_ts",
        "make_zero",
        "-f",
        "hls",
        "-hls_time",
        "4",
        "-hls_list_size",
        "0",
        "-hls_playlist_type",
        "vod",
        "-hls_segment_type",
        "mpegts",
        "-hls_segment_filename",
        &segment_pattern.to_string_lossy(),
        "-progress",
        "pipe:2",
        "-y",
        &playlist.to_string_lossy(),
    ]);
    execute_ffmpeg_conversion(cmd, reporter, "TS 内嵌播放索引").await
}

/// Re-encode a source that the embedded browser cannot decode. Prefer the
/// locally available AMD encoder so long livestream recordings do not occupy
/// the CPU for tens of hours. Fall back to a fast software H.264 profile when
/// the driver rejects AMF.
pub async fn try_browser_compatible_conversion(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    try_browser_compatible_conversion_with_metadata(source, dest, reporter, None).await
}

pub async fn try_browser_compatible_conversion_with_metadata(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
    metadata: Option<&VideoMetadata>,
) -> Result<(), String> {
    let total_duration_secs = metadata
        .map(|value| value.duration)
        .filter(|duration| *duration > 0.0);
    let copy_audio = metadata.is_some_and(|value| {
        matches!(
            value.audio_codec.trim().to_ascii_lowercase().as_str(),
            "aac" | "mp3" | "mp2"
        )
    });
    let encoder = hwaccel::get_x264_encoder().await;

    if encoder != "libx264" {
        reporter
            .update(&format!("正在使用 {encoder} 硬件编码生成可播放版本…"))
            .await;

        let mut hardware = tokio::process::Command::new(ffmpeg_path());
        #[cfg(target_os = "windows")]
        hardware.creation_flags(0x08000000);
        hardware.args(["-i", &source.to_string_lossy()]);
        hwaccel::apply_x264_encoder_only(&mut hardware, encoder);
        if encoder == "h264_amf" {
            hardware.args(["-quality", "speed"]);
        } else {
            hwaccel::apply_x264_quality_args(&mut hardware, encoder);
        }
        append_browser_audio_args(&mut hardware, copy_audio);
        hardware.args([
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:2",
            "-y",
            &dest.to_string_lossy(),
        ]);

        match execute_ffmpeg_conversion_with_duration(
            hardware,
            reporter,
            &format!("{encoder} 硬件转换"),
            total_duration_secs,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(hardware_error) => {
                log::warn!(
                    "{encoder} H.264 conversion failed: {hardware_error}; falling back to fast libx264"
                );
                reporter
                    .update("硬件编码不可用，正在使用快速兼容转换…")
                    .await;
            }
        }
    } else {
        reporter
            .update("未发现可用硬件编码器，正在使用快速兼容转换…")
            .await;
    }

    let mut software = tokio::process::Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    software.creation_flags(0x08000000);
    software.args([
        "-i",
        &source.to_string_lossy(),
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "20",
    ]);
    append_browser_audio_args(&mut software, copy_audio);
    software.args([
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:2",
        "-y",
        &dest.to_string_lossy(),
    ]);
    execute_ffmpeg_conversion_with_duration(software, reporter, "快速兼容转换", total_duration_secs)
        .await
}

fn append_browser_audio_args(command: &mut tokio::process::Command, copy_audio: bool) {
    if copy_audio {
        command.args(["-c:a", "copy"]);
    } else {
        command.args(["-c:a", "aac", "-b:a", "160k"]);
    }
}

/*
    reporter
        .update("正在使用 AMD 硬件编码生成可播放版本...")
        .await;

    let mut amd = tokio::process::Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    amd.creation_flags(0x08000000);
    amd.args([
        "-i",
        &source.to_string_lossy(),
        "-c:v",
        "h264_amf",
        "-quality",
        "speed",
        "-c:a",
        "aac",
        "-b:a",
        "160k",
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:2",
        "-y",
        &dest.to_string_lossy(),
    ]);

    match execute_ffmpeg_conversion(amd, reporter, "AMD 硬件转换").await {
        Ok(()) => Ok(()),
        Err(amd_error) => {
            log::warn!("AMD H.264 conversion failed: {amd_error}; falling back to fast libx264");
            reporter
                .update("AMD 编码不可用，正在使用快速兼容转换...")
                .await;

            let mut software = tokio::process::Command::new(ffmpeg_path());
            #[cfg(target_os = "windows")]
            software.creation_flags(0x08000000);
            software.args([
                "-i",
                &source.to_string_lossy(),
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "20",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
                "-progress",
                "pipe:2",
                "-y",
                &dest.to_string_lossy(),
            ]);
            execute_ffmpeg_conversion(software, reporter, "快速兼容转换").await
        }
    }
}*/

// 带进度的视频格式转换函数（智能质量保持策略）
pub async fn convert_video_format(
    source: &Path,
    dest: &Path,
    reporter: &ProgressReporter,
) -> Result<(), String> {
    // 先尝试stream copy（无损转换），如果失败则使用高质量重编码
    match try_stream_copy_conversion(source, dest, reporter).await {
        Ok(()) => Ok(()),
        Err(stream_copy_error) => {
            reporter.update("流复制失败，使用高质量重编码模式...").await;
            log::warn!("Stream copy failed: {stream_copy_error}, falling back to re-encoding");
            try_high_quality_conversion(source, dest, reporter).await
        }
    }
}

/// Check if all videos have same encoding and resolution
pub async fn check_videos(video_paths: &[&Path]) -> bool {
    // check if all playlist paths exist
    let mut video_codec = "".to_owned();
    let mut audio_codec = "".to_owned();
    let mut width = 0;
    let mut height = 0;
    for video_path in video_paths.iter() {
        if !Path::new(video_path).exists() {
            continue;
        }
        let metadata = match extract_video_metadata(Path::new(video_path)).await {
            Ok(metadata) => metadata,
            Err(error) => {
                log::error!("Failed to extract video metadata: {error}");
                return false;
            }
        };

        // check video codec
        if !video_codec.is_empty() && metadata.video_codec != video_codec {
            log::error!("Video codec does not match: {}", video_path.display());
            return false;
        } else {
            video_codec = metadata.video_codec;
        }

        // check audio codec
        if !audio_codec.is_empty() && metadata.audio_codec != audio_codec {
            log::error!("Audio codec does not match: {}", video_path.display());
            return false;
        } else {
            audio_codec = metadata.audio_codec;
        }

        // check width
        if width > 0 && metadata.width != width {
            log::error!("Video width does not match: {}", video_path.display());
            return false;
        } else {
            width = metadata.width;
        }

        // check height
        if height > 0 && metadata.height != height {
            log::error!("Video height does not match: {}", video_path.display());
            return false;
        } else {
            height = metadata.height;
        }
    }

    true
}

// tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_progress_shows_percent_speed_and_eta() {
        assert_eq!(
            format_conversion_progress(300.0, 600.0, 100.0, "硬件转换"),
            "转换 50% · 3.0x · 预计剩余 1分40秒（硬件转换）"
        );
    }

    #[test]
    fn output_is_mp4_detects_browser_playback_containers() {
        assert!(output_is_mp4(Path::new("clip.mp4")));
        assert!(output_is_mp4(Path::new("clip.M4V")));
        assert!(output_is_mp4(Path::new("clip.mov")));
        assert!(!output_is_mp4(Path::new("clip.ts")));
        assert!(!output_is_mp4(Path::new("clip.opus")));
    }

    // 测试 Range 结构体
    #[test]
    fn test_range_creation() {
        let range = Range {
            start: 10.0,
            end: 30.0,
        };
        assert_eq!(range.start, 10.0);
        assert_eq!(range.end, 30.0);
        assert_eq!(range.duration(), 20.0);
    }

    #[test]
    fn test_range_duration() {
        let range = Range {
            start: 0.0,
            end: 60.0,
        };
        assert_eq!(range.duration(), 60.0);

        let range2 = Range {
            start: 15.5,
            end: 45.5,
        };
        assert_eq!(range2.duration(), 30.0);
    }

    #[test]
    fn test_range_display() {
        let range = Range {
            start: 5.0,
            end: 25.0,
        };
        assert_eq!(range.to_string(), "[5, 25]");
    }

    #[test]
    fn test_range_edge_cases() {
        let zero_range = Range {
            start: 0.0,
            end: 0.0,
        };
        assert_eq!(zero_range.duration(), 0.0);

        let negative_start = Range {
            start: -5.0,
            end: 10.0,
        };
        assert_eq!(negative_start.duration(), 15.0);

        let large_range = Range {
            start: 1000.0,
            end: 2000.0,
        };
        assert_eq!(large_range.duration(), 1000.0);
    }

    // 测试视频元数据提取
    #[tokio::test]
    async fn test_extract_video_metadata() {
        let test_video = Path::new("tests/video/test.mp4");
        if test_video.exists() {
            let metadata = extract_video_metadata(test_video).await.unwrap();
            println!("metadata: {:?}", metadata);
            assert!(metadata.duration > 0.0);
            assert!(metadata.width > 0);
            assert!(metadata.height > 0);
        }
    }

    // 测试音频时长获取
    #[tokio::test]
    async fn test_get_audio_duration() {
        let test_audio = Path::new("tests/audio/test.wav");
        if test_audio.exists() {
            let duration = get_audio_duration(test_audio).await.unwrap();
            assert!(duration > 0);
        }
    }

    // 测试缩略图生成
    #[tokio::test]
    async fn test_generate_thumbnail() {
        let file = Path::new("tests/video/test.mp4");
        if file.exists() {
            let thumbnail_file = generate_thumbnail(file, 0.0).await.unwrap();
            assert!(thumbnail_file.exists());
            assert_eq!(thumbnail_file.extension().unwrap(), "jpg");
            // clean up
            let _ = std::fs::remove_file(thumbnail_file);
        }
    }

    // 测试 FFmpeg 版本检查
    #[tokio::test]
    async fn test_check_ffmpeg() {
        let result = check_ffmpeg().await;
        match result {
            Ok(version) => {
                assert!(!version.is_empty());
                // FFmpeg 版本字符串可能不包含 "ffmpeg" 这个词，所以检查是否包含数字
                assert!(version.chars().any(|c| c.is_ascii_digit()));
            }
            Err(_) => {
                // FFmpeg 可能没有安装，这是正常的
                println!("FFmpeg not available for testing");
            }
        }
    }

    // 测试通用 FFmpeg 命令
    #[tokio::test]
    async fn test_generic_ffmpeg_command() {
        let result = generic_ffmpeg_command(&["-version"]).await;
        match result {
            Ok(_output) => {
                // 输出可能为空或者不包含 "ffmpeg" 字符串，我们只检查函数能正常执行
                println!("FFmpeg command executed successfully");
            }
            Err(_) => {
                // FFmpeg 可能没有安装，这是正常的
                println!("FFmpeg not available for testing");
            }
        }
    }

    // 测试硬件加速能力探测
    #[tokio::test]
    async fn test_list_supported_hwaccels() {
        match super::hwaccel::list_supported_hwaccels().await {
            Ok(hwaccels) => {
                println!("hwaccels: {:?}", hwaccels);
                let mut sorted = hwaccels.clone();
                sorted.sort();
                sorted.dedup();
                assert_eq!(sorted.len(), hwaccels.len());
            }
            Err(_) => {
                println!("FFmpeg hardware acceleration query not available for testing");
            }
        }
    }

    // 测试字幕生成错误处理
    #[tokio::test]
    async fn test_generate_video_subtitle_errors() {
        let test_file = Path::new("tests/video/test.mp4");

        // 测试 Whisper 类型 - 模型未配置
        let result =
            generate_video_subtitle(None, test_file, "whisper", "", "", "", "", "zh").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Whisper model not configured"));

        // 测试 Whisper Online 类型 - API key 未配置
        let result =
            generate_video_subtitle(None, test_file, "whisper_online", "", "", "", "", "zh").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("API key not configured"));

        // 测试未知类型
        let result =
            generate_video_subtitle(None, test_file, "unknown_type", "", "", "", "", "").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Unknown subtitle generator type"));
    }

    // 测试路径构建函数
    #[test]
    fn mp4_moov_at_end_detects_tail_metadata() {
        use std::io::Write;

        let temp_dir =
            std::env::temp_dir().join(format!("bsr-mp4-layout-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("tail-moov.mp4");
        {
            let mut file = std::fs::File::create(&file_path).unwrap();
            file.write_all(b"----ftypmp42----mdat").unwrap();
            file.write_all(&vec![0u8; 4096]).unwrap();
            file.write_all(b"----moov").unwrap();
        }

        assert!(mp4_moov_at_end(&file_path).unwrap());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_ffmpeg_paths() {
        let ffmpeg_path = ffmpeg_path();
        let ffprobe_path = ffprobe_path();

        #[cfg(windows)]
        {
            assert_eq!(ffmpeg_path.extension().unwrap(), "exe");
            assert_eq!(ffprobe_path.extension().unwrap(), "exe");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(ffmpeg_path.file_name().unwrap(), "ffmpeg");
            assert_eq!(ffprobe_path.file_name().unwrap(), "ffprobe");
        }
    }

    // 测试文件名和路径处理
    #[test]
    fn test_filename_processing() {
        let test_file = Path::new("tests/video/test.mp4");

        // 测试字幕文件名生成
        let subtitle_filename = format!(
            "{}{}",
            constants::PREFIX_SUBTITLE,
            test_file.file_name().unwrap().to_str().unwrap()
        );
        assert!(subtitle_filename.starts_with(constants::PREFIX_SUBTITLE));
        assert!(subtitle_filename.contains("test.mp4"));

        // 测试弹幕文件名生成
        let danmu_filename = format!(
            "{}{}",
            constants::PREFIX_DANMAKU,
            test_file.file_name().unwrap().to_str().unwrap()
        );
        assert!(danmu_filename.starts_with(constants::PREFIX_DANMAKU));
        assert!(danmu_filename.contains("test.mp4"));
    }

    // 测试音频分块目录结构
    #[test]
    fn test_audio_chunk_directory_structure() {
        let test_file = Path::new("tests/audio/test.wav");
        let output_path = test_file.with_extension("wav");
        let output_dir = output_path.parent().unwrap();
        let base_name = output_path.file_stem().unwrap().to_str().unwrap();
        let chunk_dir = output_dir.join(format!("{base_name}_chunks"));

        assert!(chunk_dir.to_string_lossy().contains("_chunks"));
        assert!(chunk_dir.to_string_lossy().contains("test"));
    }

    #[test]
    fn test_range_is_in_inside() {
        let r = Range {
            start: 1.0,
            end: 5.0,
        };
        assert!(r.is_in(3.0));
    }

    #[test]
    fn test_range_is_in_at_boundaries() {
        let r = Range {
            start: 1.0,
            end: 5.0,
        };
        assert!(r.is_in(1.0));
        assert!(r.is_in(5.0));
    }

    #[test]
    fn test_range_is_in_outside() {
        let r = Range {
            start: 1.0,
            end: 5.0,
        };
        assert!(!r.is_in(0.9));
        assert!(!r.is_in(5.1));
    }

    #[test]
    fn test_video_metadata_equality() {
        let m1 = VideoMetadata {
            duration: 10.0,
            width: 1920,
            height: 1080,
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
        };
        let m2 = m1.clone();
        assert_eq!(m1, m2);
    }

    #[test]
    fn test_video_metadata_different_resolution() {
        let m1 = VideoMetadata {
            duration: 10.0,
            width: 1920,
            height: 1080,
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
        };
        let m2 = VideoMetadata {
            duration: 10.0,
            width: 1280,
            height: 720,
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
        };
        assert_ne!(m1, m2);
    }

    #[test]
    fn test_video_metadata_different_codec() {
        let m1 = VideoMetadata {
            duration: 10.0,
            width: 1920,
            height: 1080,
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
        };
        let m2 = VideoMetadata {
            duration: 10.0,
            width: 1920,
            height: 1080,
            video_codec: "hevc".to_string(),
            audio_codec: "aac".to_string(),
        };
        assert_ne!(m1, m2);
    }
}
