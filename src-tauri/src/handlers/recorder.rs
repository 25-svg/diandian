use std::str::FromStr;
use std::time::Duration;

use super::transcript_review::{
    load_legacy_archive_transcript_audit, resolve_legacy_archive_review_item,
    LegacyTranscriptAuditBundle,
};
use crate::anchor_detection::validate_manual_anchor_name;
use crate::danmu2ass;
use crate::database::record::RecordRow;
use crate::database::recorder::RecorderRow;
use crate::database::task::TaskRow;
use crate::progress::progress_reporter::EventEmitter;
use crate::progress::progress_reporter::ProgressReporter;
use crate::progress::progress_reporter::ProgressReporterTrait;
use crate::recorder_manager::{
    ArchiveSubtitleRefreshResult, GenerateWholeClipParams, RecorderList,
};
use crate::security::{audit_tool_failure, audit_tool_success, require_sensitive_write};
use crate::state::State;
use crate::state_type;
use crate::subtitle_generator::transcript_artifacts::TranscriptSource;
use crate::task::Task;
use crate::task::TaskPriority;
use crate::webhook::events;
use recorder::account::Account;
use recorder::danmu::DanmuEntry;
use recorder::platforms::bilibili;
use recorder::platforms::douyin;
use recorder::platforms::PlatformType;
use recorder::RecorderInfo;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

use serde::Deserialize;
use serde::Serialize;

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_recorder_list(state: state_type!()) -> Result<RecorderList, ()> {
    Ok(state.recorder_manager.get_recorder_list().await)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_known_streamer_names(state: state_type!()) -> Result<Vec<String>, String> {
    state
        .db
        .list_known_streamer_names()
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn set_room_streamer(
    state: state_type!(),
    platform: String,
    room_id: String,
    streamer_name: String,
) -> Result<(), String> {
    let platform = PlatformType::from_str(&platform)?;
    let normalized_name = if streamer_name.trim().is_empty() {
        None
    } else {
        Some(validate_manual_anchor_name(&streamer_name)?)
    };
    state
        .recorder_manager
        .set_room_streamer_assignment(platform, &room_id, normalized_name.as_deref())
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_recorder_health(
    state: state_type!(),
) -> Result<Vec<crate::database::recorder_health::RecorderHealthRow>, String> {
    state.db.list_recorder_health().await.map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn add_recorder(
    state: state_type!(),
    platform: String,
    room_id: String,
    mut extra: String,
) -> Result<RecorderRow, String> {
    log::info!("Add recorder: {platform} {room_id}");
    let platform = PlatformType::from_str(&platform).unwrap();
    let account = match platform {
        PlatformType::BiliBili => {
            if let Ok(account) = state.db.get_account_by_platform("bilibili").await {
                Ok(account.to_account())
            } else {
                log::error!("No available bilibili account found");
                Err("没有可用账号，请先添加账号".to_string())
            }
        }
        PlatformType::Douyin => {
            let client = reqwest::Client::new();
            let sec_uid = douyin::api::get_room_owner_sec_uid(&client, &room_id)
                .await
                .map_err(|e| e.to_string())?;
            extra = sec_uid;

            if let Ok(account) = state.db.get_account_by_platform("douyin").await {
                Ok(account.to_account())
            } else {
                log::error!("No available douyin account found");
                Err("没有可用账号，请先添加账号".to_string())
            }
        }
        PlatformType::Huya => {
            if let Ok(account) = state.db.get_account_by_platform("huya").await {
                Ok(account.to_account())
            } else {
                Ok(Account::default())
            }
        }
        PlatformType::Kuaishou => {
            if let Ok(account) = state.db.get_account_by_platform("kuaishou").await {
                Ok(account.to_account())
            } else {
                Ok(Account::default())
            }
        }
        PlatformType::TikTok => {
            if let Ok(account) = state.db.get_account_by_platform("tiktok").await {
                Ok(account.to_account())
            } else {
                Ok(Account::default())
            }
        }
        PlatformType::Xiaohongshu => {
            if let Ok(account) = state.db.get_account_by_platform("xiaohongshu").await {
                Ok(account.to_account())
            } else {
                Ok(Account::default())
            }
        }
        PlatformType::Weibo => {
            if let Ok(account) = state.db.get_account_by_platform("weibo").await {
                Ok(account.to_account())
            } else {
                Ok(Account::default())
            }
        }
        _ => Err("不支持的平台".to_string()),
    };

    match account {
        Ok(account) => match state
            .recorder_manager
            .add_recorder(&account, platform, &room_id, &extra, true)
            .await
        {
            Ok(()) => {
                let room = state.db.add_recorder(platform, &room_id, &extra).await?;
                state
                    .db
                    .new_message("添加直播间", &format!("添加了新直播间 {room_id}"))
                    .await?;
                // post webhook event
                let event = events::new_webhook_event(
                    events::RECORDER_ADDED,
                    events::Payload::Recorder(room.clone()),
                );
                if let Err(e) = state.webhook_poster.post_event(&event).await {
                    log::error!("Post webhook event error: {e}");
                }
                Ok(room)
            }
            Err(e) => {
                log::error!("Failed to add recorder: {e}");
                Err(format!("添加失败: {e}"))
            }
        },
        Err(e) => {
            log::error!("Failed to add recorder: {e}");
            Err(format!("添加失败: {e}"))
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn remove_recorder(
    state: state_type!(),
    platform: String,
    room_id: String,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<(), String> {
    log::info!("Remove recorder: {platform} {room_id}");
    let audit = require_sensitive_write(
        "remove_recorder",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("recorder:{platform}:{room_id}"),
    )?;
    let platform = PlatformType::from_str(&platform).unwrap();
    match state
        .recorder_manager
        .remove_recorder(platform, &room_id)
        .await
    {
        Ok(recorder) => {
            state
                .db
                .new_message("移除直播间", &format!("移除了直播间 {room_id}"))
                .await?;
            // post webhook event
            let event = events::new_webhook_event(
                events::RECORDER_REMOVED,
                events::Payload::Recorder(recorder),
            );
            if let Err(e) = state.webhook_poster.post_event(&event).await {
                log::error!("Post webhook event error: {e}");
            }
            log::info!("Removed recorder: {} {}", platform.as_str(), room_id);
            audit_tool_success(&audit);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to remove recorder: {e}");
            let error = e.to_string();
            audit_tool_failure(&audit, &error);
            Err(error)
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_room_info(
    state: state_type!(),
    platform: String,
    room_id: String,
) -> Result<RecorderInfo, String> {
    let platform = PlatformType::from_str(&platform).unwrap();
    if let Some(info) = state
        .recorder_manager
        .get_recorder_info(platform, &room_id)
        .await
    {
        Ok(info)
    } else {
        Err("Not found".to_string())
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_archive_disk_usage(state: state_type!()) -> Result<i64, String> {
    Ok(state.recorder_manager.get_archive_disk_usage().await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_archives(
    state: state_type!(),
    room_id: String,
    offset: i64,
    limit: i64,
) -> Result<Vec<RecordRow>, String> {
    Ok(state
        .recorder_manager
        .get_archives(&room_id, offset, limit)
        .await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_archive(
    state: state_type!(),
    room_id: String,
    live_id: String,
) -> Result<RecordRow, String> {
    Ok(state
        .recorder_manager
        .get_archive(&room_id, &live_id)
        .await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn set_archive_kind(
    state: state_type!(),
    live_id: String,
    archive_kind: String,
) -> Result<RecordRow, String> {
    state
        .db
        .set_record_archive_kind(&live_id, &archive_kind)
        .await
        .map_err(String::from)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveAutoClassificationInput {
    pub live_id: String,
    pub account_name: String,
}

#[cfg(test)]
mod archive_auto_classification_tests {
    use super::ArchiveAutoClassificationInput;

    #[test]
    fn accepts_camel_case_tauri_payload() {
        let value =
            serde_json::json!({ "liveId": "record-1", "accountName": "金典拍拍相机专卖店" });
        let input: ArchiveAutoClassificationInput = serde_json::from_value(value).unwrap();
        assert_eq!(input.live_id, "record-1");
        assert_eq!(input.account_name, "金典拍拍相机专卖店");
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn auto_classify_archive_kinds(
    state: state_type!(),
    archives: Vec<ArchiveAutoClassificationInput>,
) -> Result<Vec<RecordRow>, String> {
    let mut updated = Vec::with_capacity(archives.len());
    for archive in archives {
        updated.push(
            state
                .db
                .auto_classify_record_archive_kind(&archive.live_id, &archive.account_name)
                .await
                .map_err(String::from)?,
        );
    }
    Ok(updated)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_archives_by_parent_id(
    state: state_type!(),
    room_id: String,
    parent_id: String,
) -> Result<Vec<RecordRow>, String> {
    Ok(state
        .db
        .get_archives_by_parent_id(&room_id, &parent_id)
        .await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_archive_subtitle(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<String, String> {
    let platform = PlatformType::from_str(&platform)?;
    Ok(state
        .recorder_manager
        .get_archive_subtitle(platform, &room_id, &live_id)
        .await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_archive_subtitle(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<String, String> {
    let platform = PlatformType::from_str(&platform)?;
    Ok(state
        .recorder_manager
        .generate_archive_subtitle(platform, &room_id, &live_id, None)
        .await?)
}

async fn wait_until_archive_subtitle_idle(
    state: &State,
    platform: PlatformType,
    room_id: &str,
    live_id: &str,
) -> Result<(), String> {
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let active = state
            .db
            .get_active_archive_subtitle_task(platform.as_str(), room_id, live_id)
            .await
            .map_err(|error| error.to_string())?;
        if active.is_none() {
            return Ok(());
        }
    }
}

async fn wait_for_archive_subtitle_task(
    state: &State,
    platform: PlatformType,
    room_id: &str,
    live_id: &str,
) -> Result<ArchiveSubtitleRefreshResult, String> {
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let active = state
            .db
            .get_active_archive_subtitle_task(platform.as_str(), room_id, live_id)
            .await
            .map_err(|error| error.to_string())?;
        if active.is_some() {
            continue;
        }

        let subtitle = state
            .recorder_manager
            .get_archive_subtitle(platform, room_id, live_id)
            .await
            .map_err(|error| error.to_string())?;
        if subtitle.trim().is_empty() {
            return Err(
                "逐字稿任务已结束，但没有生成可用文稿。请稍后点击重新识别再试。".to_string(),
            );
        }
        let new_length = subtitle.len();
        return Ok(ArchiveSubtitleRefreshResult {
            subtitle,
            decision: "resumed".to_string(),
            similarity: 0.0,
            old_length: 0,
            new_length,
        });
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn refresh_archive_subtitle(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
    force: Option<bool>,
) -> Result<ArchiveSubtitleRefreshResult, String> {
    let platform = PlatformType::from_str(&platform)?;
    let force = force.unwrap_or(false);
    let task = TaskRow {
        id: uuid::Uuid::new_v4().to_string(),
        task_type: "generate_archive_subtitle".to_string(),
        status: "pending".to_string(),
        message: "等待生成整场逐字稿".to_string(),
        metadata: serde_json::json!({
            "platform": platform.as_str(),
            "room_id": room_id,
            "live_id": live_id,
        })
        .to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if !state
        .db
        .add_archive_subtitle_task_if_idle(&task, platform.as_str(), &room_id, &live_id)
        .await?
    {
        if !force {
            return wait_for_archive_subtitle_task(&state, platform, &room_id, &live_id).await;
        }
        wait_until_archive_subtitle_idle(&state, platform, &room_id, &live_id).await?;
        if !state
            .db
            .add_archive_subtitle_task_if_idle(&task, platform.as_str(), &room_id, &live_id)
            .await?
        {
            return wait_for_archive_subtitle_task(&state, platform, &room_id, &live_id).await;
        }
    }

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &task.id).await?;
    reporter.update("正在生成整场逐字稿").await;

    let result = state
        .recorder_manager
        .refresh_archive_subtitle(platform, &room_id, &live_id, Some(&reporter))
        .await;
    match result {
        Ok(result) => {
            reporter.finish(true, "整场逐字稿生成完成").await;
            state
                .db
                .update_task(&task.id, "success", "整场逐字稿生成完成", None)
                .await?;
            Ok(result)
        }
        Err(error) => {
            reporter
                .finish(false, &format!("整场逐字稿生成失败: {error}"))
                .await;
            state
                .db
                .update_task(
                    &task.id,
                    "failed",
                    &format!("整场逐字稿生成失败: {error}"),
                    None,
                )
                .await?;
            Err(error.to_string())
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_archive_transcript_audit(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<LegacyTranscriptAuditBundle, String> {
    load_legacy_archive_transcript_audit(
        &state,
        TranscriptSource::Archive {
            platform,
            room_id,
            live_id,
        },
    )
    .await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn save_archive_fact_card(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
    fact_card: serde_json::Value,
) -> Result<(), String> {
    let platform = PlatformType::from_str(&platform)?;
    Ok(state
        .recorder_manager
        .save_archive_fact_card(platform, &room_id, &live_id, fact_card)
        .await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resolve_archive_review_item(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
    index: usize,
    correction: String,
) -> Result<LegacyTranscriptAuditBundle, String> {
    let source = TranscriptSource::Archive {
        platform,
        room_id,
        live_id,
    };
    resolve_legacy_archive_review_item(&state, source, index, correction).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn delete_archive(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<(), String> {
    let audit = require_sensitive_write(
        "delete_archive",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("archive:{platform}:{room_id}:{live_id}"),
    )?;
    let platform = PlatformType::from_str(&platform)?;
    let to_delete = match state
        .recorder_manager
        .delete_archive(platform, &room_id, &live_id)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let error = error.to_string();
            audit_tool_failure(&audit, &error);
            return Err(error);
        }
    };
    state
        .db
        .new_message(
            "删除历史缓存",
            &format!("删除了房间 {room_id} 的历史缓存 {live_id}"),
        )
        .await?;
    // post webhook event
    let event =
        events::new_webhook_event(events::ARCHIVE_DELETED, events::Payload::Archive(to_delete));
    if let Err(e) = state.webhook_poster.post_event(&event).await {
        log::error!("Post webhook event error: {e}");
    }
    audit_tool_success(&audit);
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn delete_archives(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_ids: Vec<String>,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<(), String> {
    let audit = require_sensitive_write(
        "delete_archives",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("archives:{platform}:{room_id}:{}", live_ids.join(",")),
    )?;
    let platform = PlatformType::from_str(&platform)?;
    let to_deletes = match state
        .recorder_manager
        .delete_archives(
            platform,
            &room_id,
            &live_ids
                .iter()
                .map(std::string::String::as_str)
                .collect::<Vec<&str>>(),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let error = error.to_string();
            audit_tool_failure(&audit, &error);
            return Err(error);
        }
    };
    state
        .db
        .new_message(
            "删除历史缓存",
            &format!("删除了房间 {} 的历史缓存 {}", room_id, live_ids.join(", ")),
        )
        .await?;
    for to_delete in to_deletes {
        // post webhook event
        let event =
            events::new_webhook_event(events::ARCHIVE_DELETED, events::Payload::Archive(to_delete));
        if let Err(e) = state.webhook_poster.post_event(&event).await {
            log::error!("Post webhook event error: {e}");
        }
    }
    audit_tool_success(&audit);
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_danmu_record(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<Vec<DanmuEntry>, String> {
    let platform = PlatformType::from_str(&platform)?;
    Ok(state
        .recorder_manager
        .load_danmus(platform, &room_id, &live_id)
        .await?)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDanmuOptions {
    platform: String,
    room_id: String,
    live_id: String,
    x: i64,
    y: i64,
    ass: bool,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn export_danmu(
    state: state_type!(),
    options: ExportDanmuOptions,
) -> Result<String, String> {
    let platform = PlatformType::from_str(&options.platform)?;
    let mut danmus = state
        .recorder_manager
        .load_danmus(platform, &options.room_id, &options.live_id)
        .await?;

    log::debug!("First danmu entry: {:?}", danmus.first());
    // update entry ts to offset
    for d in &mut danmus {
        d.ts -= (options.x + options.y) * 1000;
    }
    if options.x != 0 || options.y != 0 {
        danmus.retain(|e| e.ts >= 0 && e.ts <= (options.y - options.x) * 1000);
    }

    if options.ass {
        Ok(danmu2ass::danmu_to_ass(
            danmus,
            danmu2ass::Danmu2AssOptions::default(),
        ))
    } else {
        // map and join entries
        Ok(danmus
            .iter()
            .map(|e| format!("{}:{}", e.ts, e.content))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn send_danmaku(
    state: state_type!(),
    uid: String,
    room_id: String,
    message: String,
) -> Result<(), String> {
    let account = state.db.get_account("bilibili", &uid).await?;
    let client = reqwest::Client::new();
    match bilibili::api::send_danmaku(&client, &account.to_account(), &room_id, &message).await {
        Ok(()) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_total_length(state: state_type!()) -> Result<f64, String> {
    match state.db.get_total_length().await {
        Ok(total_length) => Ok(total_length),
        Err(e) => Err(format!("Failed to get total length: {e}")),
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_today_record_count(state: state_type!()) -> Result<i64, String> {
    match state.db.get_today_record_count().await {
        Ok(count) => Ok(count),
        Err(e) => Err(format!("Failed to get today record count: {e}")),
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_recent_record(
    state: state_type!(),
    room_id: String,
    offset: i64,
    limit: i64,
) -> Result<Vec<RecordRow>, String> {
    match state.db.get_recent_record(&room_id, offset, limit).await {
        Ok(records) => Ok(records),
        Err(e) => Err(format!("Failed to get recent record: {e}")),
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn set_enable(
    state: state_type!(),
    platform: String,
    room_id: String,
    enabled: bool,
) -> Result<(), String> {
    log::info!("Set enable for recorder {platform} {room_id} {enabled}");
    let platform = PlatformType::from_str(&platform)?;
    state
        .recorder_manager
        .set_enable(platform, &room_id, enabled)
        .await;
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn fetch_hls(state: state_type!(), uri: String) -> Result<Vec<u8>, String> {
    // Handle wildcard pattern in the URI
    let uri = if uri.contains("/hls/") {
        uri.split("/hls/").last().unwrap_or(&uri).to_string()
    } else {
        uri
    };
    state
        .recorder_manager
        .handle_hls_request(&uri)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_whole_clip(
    state: state_type!(),
    encode_danmu: bool,
    platform: String,
    room_id: String,
    parent_id: String,
    selected_live_ids: Option<Vec<String>>,
    output_name: Option<String>,
) -> Result<TaskRow, String> {
    log::info!("Generate whole clip for {platform} {room_id} {parent_id}");

    let task = state
        .db
        .generate_task(
            "generate_whole_clip",
            "",
            &serde_json::json!({
                "platform": platform,
                "room_id": room_id,
                "parent_id": parent_id,
                "encode_danmu": encode_danmu,
                "selected_live_ids": selected_live_ids,
                "output_name": output_name,
            })
            .to_string(),
        )
        .await?;

    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &task.id).await?;

    log::info!("Create task: {} {}", task.id, task.task_type);
    // create a tokio task to run in background
    #[cfg(feature = "gui")]
    let state_clone = (*state).clone();
    #[cfg(feature = "headless")]
    let state_clone = state.clone();

    let task_id = task.id.clone();
    state
        .task_manager
        .add_task(Task::new(
            task_id.clone(),
            TaskPriority::Normal,
            async move {
                match state_clone
                    .recorder_manager
                    .generate_whole_clip(
                        Some(&reporter),
                        GenerateWholeClipParams {
                            encode_danmu,
                            platform,
                            room_id,
                            parent_id,
                            selected_live_ids,
                            output_name,
                        },
                    )
                    .await
                {
                    Ok(()) => {
                        reporter.finish(true, "切片生成完成").await;
                        let _ = state_clone
                            .db
                            .update_task(&task_id, "success", "切片生成完成", None)
                            .await;
                        Ok(())
                    }
                    Err(e) => {
                        reporter.finish(false, &format!("切片生成失败: {e}")).await;
                        let _ = state_clone
                            .db
                            .update_task(&task_id, "failed", &format!("切片生成失败: {e}"), None)
                            .await;
                        Err(format!("切片生成失败: {e}"))
                    }
                }
            },
        ))
        .await?;
    Ok(task)
}
