use crate::anchor_detection::{
    anchor_frame_timestamps, build_minimax_anchor_request, live_anchor_retry_delay_seconds,
    parse_anchor_response, resolve_anchor_consensus, select_available_anchor_segments,
    validate_manual_anchor_name, AnchorDetectionStatus,
};
use crate::config::Config;
use crate::database::record::RecordRow;
use crate::database::video::VideoRow;
use crate::database::Database;
use crate::handlers::ai::request_minimax_payload;
use crate::handlers::video::resolve_video_transcript_context;
use crate::handlers::video_editing::extract_frame_at_timestamp;
use crate::recorder_manager::RecorderManager;
use crate::state::State;
use crate::state_type;
use recorder::platforms::PlatformType;
use recorder::CachePath;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn detect_video_anchor(
    state: state_type!(),
    video_id: i64,
    force: Option<bool>,
) -> Result<VideoRow, String> {
    detect_video_anchor_inner(&state, video_id, force.unwrap_or(false)).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn save_video_anchor_manual(
    state: state_type!(),
    video_id: i64,
    anchor_name: String,
) -> Result<VideoRow, String> {
    let anchor_name = validate_manual_anchor_name(&anchor_name)?;
    state
        .db
        .save_video_anchor_manual(video_id, &anchor_name)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn detect_archive_anchor(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
    force: Option<bool>,
) -> Result<RecordRow, String> {
    detect_archive_anchor_inner(
        state.db.as_ref(),
        state.config.as_ref(),
        &platform,
        &room_id,
        &live_id,
        force.unwrap_or(false),
    )
    .await
}

pub fn start_live_anchor_detection(
    recorder_manager: RecorderManager,
    db: Arc<Database>,
    config: Arc<RwLock<Config>>,
    platform: String,
    room_id: String,
    live_id: String,
) {
    tokio::spawn(async move {
        for attempt in 0.. {
            let Some(delay_seconds) = live_anchor_retry_delay_seconds(attempt) else {
                return;
            };
            if delay_seconds > 0 {
                sleep(Duration::from_secs(delay_seconds)).await;
            }

            // The initial attempt starts as soon as the recorder creates the archive row.
            // Every later retry must still refer to the same active live recording.
            if attempt > 0 {
                let Ok(platform_type) = PlatformType::from_str(&platform) else {
                    return;
                };
                let Some(recorder) = recorder_manager
                    .get_recorder_info(platform_type, &room_id)
                    .await
                else {
                    return;
                };
                if !recorder.recording || recorder.live_id != live_id {
                    return;
                }
            }

            match detect_archive_anchor_inner(
                db.as_ref(),
                config.as_ref(),
                &platform,
                &room_id,
                &live_id,
                attempt > 0,
            )
            .await
            {
                Ok(record)
                    if record.anchor_source == "manual"
                        || record.anchor_detection_status == "confirmed" =>
                {
                    return;
                }
                Ok(_) => {}
                Err(error) => {
                    if error.contains("API Key") {
                        log::warn!("直播 {live_id} 主播识别未启动：{error}");
                        return;
                    }
                    log::warn!(
                        "直播 {live_id} 主播识别第 {} 次未完成：{error}",
                        attempt + 1
                    );
                }
            }
        }
    });
}

async fn detect_archive_anchor_inner(
    db: &Database,
    config: &RwLock<Config>,
    platform: &str,
    room_id: &str,
    live_id: &str,
    force: bool,
) -> Result<RecordRow, String> {
    let archive = db.get_record(room_id, live_id).await?;
    if archive.anchor_source == "manual"
        || (!force && archive.anchor_detection_status == "confirmed")
        || (!force && archive.anchor_detection_status == "running")
    {
        return Ok(archive);
    }

    let running = db.set_record_anchor_detection_running(&live_id).await?;
    if running.anchor_source == "manual" {
        return Ok(running);
    }

    match run_archive_minimax_anchor_detection(config, platform, room_id, live_id, archive.length)
        .await
    {
        Ok((anchor_name, confidence, status, error)) => db
            .save_record_anchor_detection(&live_id, &anchor_name, &confidence, &status, &error)
            .await
            .map_err(String::from),
        Err(error) => {
            log::warn!("archive {live_id} anchor detection failed: {error}");
            db.save_record_anchor_detection(
                &live_id,
                "",
                "",
                "failed",
                &safe_detection_error(&error),
            )
            .await
            .map_err(String::from)
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn save_archive_anchor_manual(
    state: state_type!(),
    live_id: String,
    anchor_name: String,
) -> Result<RecordRow, String> {
    let anchor_name = validate_manual_anchor_name(&anchor_name)?;
    state
        .db
        .save_record_anchor_manual(&live_id, &anchor_name)
        .await
        .map_err(String::from)
}

async fn detect_video_anchor_inner(
    state: &State,
    video_id: i64,
    force: bool,
) -> Result<VideoRow, String> {
    let video = state.db.get_video(video_id).await?;
    if video.anchor_source == "manual"
        || (!force && video.anchor_detection_status == "confirmed")
        || (!force && video.anchor_detection_status == "running")
    {
        return Ok(video);
    }

    let running = state
        .db
        .set_video_anchor_detection_running(video_id)
        .await?;
    if running.anchor_source == "manual" {
        return Ok(running);
    }

    match run_minimax_anchor_detection(state, video_id).await {
        Ok((anchor_name, confidence, status, error)) => state
            .db
            .save_video_anchor_detection(video_id, &anchor_name, &confidence, &status, &error)
            .await
            .map_err(String::from),
        Err(error) => {
            log::warn!("视频 {video_id} 主播识别失败：{error}");
            state
                .db
                .save_video_anchor_detection(
                    video_id,
                    "",
                    "",
                    "failed",
                    &safe_detection_error(&error),
                )
                .await
                .map_err(String::from)
        }
    }
}

async fn run_minimax_anchor_detection(
    state: &State,
    video_id: i64,
) -> Result<(String, String, String, String), String> {
    let api_key = crate::handlers::ai::configured_minimax_api_key(state).await?;

    let context = resolve_video_transcript_context(state, video_id).await?;
    let duration_ms = crate::ffmpeg::probe_media_duration_ms(&context.media_file).await?;
    let timestamps = anchor_frame_timestamps(duration_ms as f64 / 1_000.0);
    if timestamps.len() != 3 {
        return Err("视频时长无效，无法选择主播识别画面".to_string());
    }

    let mut frames = Vec::with_capacity(timestamps.len());
    for timestamp in timestamps {
        frames.push(extract_frame_at_timestamp(&context.media_file, timestamp).await?);
    }

    let request = build_minimax_anchor_request(&frames);
    let response = request_minimax_payload(&api_key, &request).await?;
    let readings = parse_anchor_response(&response)?;
    let decision = resolve_anchor_consensus(&readings);
    let status = match decision.status {
        AnchorDetectionStatus::Confirmed => "confirmed",
        AnchorDetectionStatus::Failed => "failed",
    };

    Ok((
        decision.anchor_name,
        decision.confidence,
        status.to_string(),
        decision.error,
    ))
}

async fn run_archive_minimax_anchor_detection(
    config: &RwLock<Config>,
    platform: &str,
    room_id: &str,
    live_id: &str,
    _duration_seconds: f64,
) -> Result<(String, String, String, String), String> {
    let api_key = {
        let config = config.read().await;
        crate::handlers::ai::configured_minimax_api_key_from_config(&config)?
    };
    let platform_type = PlatformType::from_str(platform)
        .map_err(|_| format!("unsupported archive platform: {platform}"))?;
    let cache = config.read().await.cache.clone();
    let playlist = CachePath::new(cache.into(), platform_type, room_id, live_id)
        .with_filename("playlist.m3u8")
        .full_path();
    if !playlist.exists() {
        return Err("recording media is missing".to_string());
    }
    let playlist_source = tokio::fs::read_to_string(&playlist)
        .await
        .map_err(|error| format!("failed to read recording playlist: {error}"))?;
    let archive_dir = playlist
        .parent()
        .ok_or_else(|| "recording directory is invalid".to_string())?;
    let segment_names =
        select_available_anchor_segments(&playlist_source, |name| archive_dir.join(name).is_file());
    if segment_names.len() != 3 {
        return Err("recording does not contain three readable local video segments".to_string());
    }

    let mut frames = Vec::with_capacity(segment_names.len());
    for segment_name in segment_names {
        frames.push(extract_frame_at_timestamp(&archive_dir.join(segment_name), 0.5).await?);
    }

    let request = build_minimax_anchor_request(&frames);
    let response = request_minimax_payload(&api_key, &request).await?;
    let readings = parse_anchor_response(&response)?;
    let decision = resolve_anchor_consensus(&readings);
    let status = match decision.status {
        AnchorDetectionStatus::Confirmed => "confirmed",
        AnchorDetectionStatus::Failed => "failed",
    };
    Ok((
        decision.anchor_name,
        decision.confidence,
        status.to_string(),
        decision.error,
    ))
}

fn safe_detection_error(error: &str) -> String {
    error
        .replace(['\r', '\n'], " ")
        .chars()
        .take(300)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::safe_detection_error;

    #[test]
    fn detection_error_is_single_line_and_bounded() {
        let error = format!("第一行\n第二行{}", "很长".repeat(200));
        let safe = safe_detection_error(&error);
        assert!(!safe.contains('\n'));
        assert!(safe.chars().count() <= 300);
    }
}
