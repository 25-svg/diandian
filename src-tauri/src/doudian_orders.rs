use chrono::{DateTime, Duration, FixedOffset, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::config::{doudian_desktop_dirs, DoudianOrderConfig};
use crate::database::live_dashboard::LiveDashboardSessionRow;
use crate::database::record::RecordRow;

const CN_OFFSET: FixedOffset = FixedOffset::east_opt(8 * 3600).expect("valid CN offset");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchDoudianPaymentEventsResult {
    pub summary: Value,
    pub events: Value,
    pub stats: Option<Value>,
    pub fetched_at: Option<String>,
    pub source_label: Option<String>,
}

pub fn default_doudian_order_config() -> DoudianOrderConfig {
    DoudianOrderConfig::default()
}

struct ResolvedDoudianCredentials {
    config: DoudianOrderConfig,
    env_tried: Vec<String>,
    token_tried: Vec<String>,
    sdk_tried: Vec<String>,
}

/// Prefer configured paths; if missing, try each Desktop/数据接口 then Desktop roots.
/// Does not mutate saved user config — only resolves files for this fetch.
fn resolve_doudian_credential_paths(config: &DoudianOrderConfig) -> ResolvedDoudianCredentials {
    let desktops = doudian_desktop_dirs();

    fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }

    fn pick_existing_file(configured: &str, fallbacks: &[PathBuf]) -> (String, Vec<String>) {
        let mut tried = Vec::new();
        let configured_path = PathBuf::from(configured);
        tried.push(configured_path.to_string_lossy().to_string());
        if configured_path.is_file() {
            return (configured.to_string(), tried);
        }
        for candidate in fallbacks {
            let display = candidate.to_string_lossy().to_string();
            if tried.iter().any(|item| item == &display) {
                continue;
            }
            tried.push(display.clone());
            if candidate.is_file() {
                return (display, tried);
            }
        }
        (configured.to_string(), tried)
    }

    fn pick_existing_dir(configured: &str, fallbacks: &[PathBuf]) -> (String, Vec<String>) {
        let mut tried = Vec::new();
        let configured_path = PathBuf::from(configured);
        tried.push(configured_path.to_string_lossy().to_string());
        if configured_path.is_dir() {
            return (configured.to_string(), tried);
        }
        for candidate in fallbacks {
            let display = candidate.to_string_lossy().to_string();
            if tried.iter().any(|item| item == &display) {
                continue;
            }
            tried.push(display.clone());
            if candidate.is_dir() {
                return (display, tried);
            }
        }
        (configured.to_string(), tried)
    }

    let mut env_fallbacks = Vec::new();
    let mut token_fallbacks = Vec::new();
    let mut sdk_fallbacks = Vec::new();
        for desktop in &desktops {
        let data_api = desktop.join("数据接口");
        push_unique(&mut env_fallbacks, data_api.join(".env"));
        push_unique(&mut env_fallbacks, desktop.join(".env"));
        push_unique(&mut token_fallbacks, data_api.join("doudian_token.env"));
        push_unique(&mut token_fallbacks, desktop.join("doudian_token.env"));
        let sdk_root = desktop.join("doudian-sdk-python-1.1.0-20260724091610");
        push_unique(&mut sdk_fallbacks, sdk_root.join("sdk-python"));
        push_unique(&mut sdk_fallbacks, sdk_root);
    }

    let (env_file, env_tried) = pick_existing_file(&config.env_file, &env_fallbacks);
    let (token_file, token_tried) = pick_existing_file(&config.token_file, &token_fallbacks);
    let (sdk_path, sdk_tried) = pick_existing_dir(&config.sdk_path, &sdk_fallbacks);

    let mut resolved = config.clone();
    resolved.env_file = env_file;
    resolved.token_file = token_file;
    resolved.sdk_path = sdk_path;
    ResolvedDoudianCredentials {
        config: resolved,
        env_tried,
        token_tried,
        sdk_tried,
    }
}

fn payment_events_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/doudian_fetch_payment_events.py")
}

fn parse_timestamp(value: &str) -> Result<DateTime<FixedOffset>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("时间不能为空".into());
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(parsed.with_timezone(&CN_OFFSET));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return CN_OFFSET
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| format!("无法解析时间：{value}"));
        }
    }
    Err(format!("无法解析时间：{value}"))
}

fn format_live_time(value: DateTime<FixedOffset>) -> String {
    value.format("%Y/%m/%d %H:%M:%S").to_string()
}

pub fn resolve_live_window(
    record: Option<&RecordRow>,
    dashboard_session: Option<&LiveDashboardSessionRow>,
    live_started_at: Option<String>,
    live_ended_at: Option<String>,
) -> Result<(String, String), String> {
    let started = if let Some(value) = live_started_at.filter(|item| !item.trim().is_empty()) {
        parse_timestamp(&value)?
    } else if let Some(session) = dashboard_session {
        parse_timestamp(&session.started_at)?
    } else if let Some(record) = record {
        parse_timestamp(&record.created_at)?
    } else {
        return Err("无法确定本场开播时间，请先绑定直播 Excel 或从录播进入".into());
    };

    let ended = if let Some(value) = live_ended_at.filter(|item| !item.trim().is_empty()) {
        parse_timestamp(&value)?
    } else if let Some(session) =
        dashboard_session.filter(|session| !session.ended_at.trim().is_empty())
    {
        parse_timestamp(&session.ended_at)?
    } else if let Some(item) = record.filter(|item| item.length > 0.0) {
        started + Duration::seconds(item.length.round() as i64)
    } else {
        started + Duration::hours(8)
    };

    if ended <= started {
        return Err("直播结束时间必须晚于开始时间".into());
    }

    Ok((format_live_time(started), format_live_time(ended)))
}

async fn run_python_script(
    python: &str,
    script_path: &Path,
    resolved: &ResolvedDoudianCredentials,
    live_started_at: &str,
    live_ended_at: &str,
) -> Result<String, String> {
    let config = &resolved.config;
    if !script_path.is_file() {
        return Err(format!("找不到脚本：{}", script_path.display()));
    }
    if !Path::new(&config.token_file).is_file() {
        return Err(format!(
            "找不到抖店 token 文件：{}。已尝试：{}。请先运行 scripts/doudian_get_token.py",
            config.token_file,
            resolved.token_tried.join(" | ")
        ));
    }
    if !Path::new(&config.env_file).is_file() {
        return Err(format!(
            "找不到抖店 env 文件：{}。已尝试：{}。需包含 DOUYIN_OPEN_APP_KEY/SECRET",
            config.env_file,
            resolved.env_tried.join(" | ")
        ));
    }
    if !Path::new(&config.sdk_path).is_dir() {
        return Err(format!(
            "找不到抖店 SDK：{}。已尝试：{}",
            config.sdk_path,
            resolved.sdk_tried.join(" | ")
        ));
    }

    let mut command = Command::new(python);
    command
        .arg(script_path)
        .arg("--env-file")
        .arg(&config.env_file)
        .arg("--token-file")
        .arg(&config.token_file)
        .arg("--sdk-path")
        .arg(&config.sdk_path)
        .arg("--live-started-at")
        .arg(live_started_at)
        .arg("--live-ended-at")
        .arg(live_ended_at)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if !config.shop_id.trim().is_empty() {
        command.arg("--shop-id").arg(config.shop_id.trim());
    }

    let output = command
        .output()
        .await
        .map_err(|error| format!("无法启动 Python（{python}）：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            "抖店 order.searchList 拉取失败".into()
        } else {
            detail
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn pick_python_executable() -> Result<String, String> {
    for candidate in ["python", "python3", "py"] {
        let mut command = Command::new(candidate);
        command
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if command
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return Ok(candidate.to_string());
        }
    }
    Err("未找到 Python。请安装 Python 3 并加入 PATH".into())
}

pub async fn fetch_payment_events(
    config: &DoudianOrderConfig,
    live_started_at: &str,
    live_ended_at: &str,
) -> Result<FetchDoudianPaymentEventsResult, String> {
    let python = pick_python_executable().await?;
    let script_path = payment_events_script_path();
    let resolved = resolve_doudian_credential_paths(config);
    let stdout = run_python_script(
        &python,
        &script_path,
        &resolved,
        live_started_at,
        live_ended_at,
    )
    .await?;

    let payload: Value = serde_json::from_str(&stdout)
        .map_err(|error| format!("解析 payment-events JSON 失败：{error}"))?;
    let events = payload
        .get("events")
        .cloned()
        .ok_or_else(|| "返回 JSON 缺少 events".to_string())?;
    let event_count = events.as_array().map(|items| items.len()).unwrap_or(0);
    if event_count == 0 {
        return Err(
            "拉取成功，但直播窗口内没有带 pay_time 的订单。请确认 token、shop_id 和场次时间".into(),
        );
    }

    Ok(FetchDoudianPaymentEventsResult {
        summary: payload.get("summary").cloned().unwrap_or(Value::Null),
        events,
        stats: payload.get("stats").cloned(),
        fetched_at: payload
            .get("fetched_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_label: payload
            .get("source_label")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_timestamp_accepts_iso_local_datetime_without_timezone() {
        let parsed = parse_timestamp("2026-08-01T08:57:19").expect("local ISO timestamp");
        assert_eq!(format_live_time(parsed), "2026/08/01 08:57:19");
    }

    #[test]
    fn resolve_live_window_prefers_dashboard_start_and_end() {
        let record = RecordRow {
            platform: "douyin".into(),
            parent_id: String::new(),
            live_id: "1".into(),
            room_id: "room".into(),
            title: "test".into(),
            length: 3600.0,
            size: 0,
            created_at: "2026-07-28T00:00:00+08:00".into(),
            cover: None,
            anchor_name: String::new(),
            anchor_source: String::new(),
            anchor_confidence: String::new(),
            anchor_detection_status: String::new(),
            anchor_detection_error: String::new(),
            anchor_detected_at: String::new(),
            archive_kind: "competitor".into(),
            classification_source: "auto_rule".into(),
        };
        let session = LiveDashboardSessionRow {
            id: 1,
            account_key: "acc".into(),
            shop_name: "shop".into(),
            started_at: "2026-07-28T08:15:49+08:00".into(),
            ended_at: "2026-07-28T15:44:39+08:00".into(),
            payment_amount_fen: 0,
            per_thousand_payment_amount_fen: None,
            viewer_count: None,
            average_online: None,
            average_watch_seconds: None,
            viewer_conversion_rate: None,
            deal_buyer_count: None,
            deal_item_count: None,
            product_click_conversion_rate: None,
            exposure_viewer_rate: None,
            qianchuan_spend_fen: None,
            source_file: "xlsx".into(),
            imported_at: chrono::Utc::now().to_rfc3339(),
        };

        let (start, end) =
            resolve_live_window(Some(&record), Some(&session), None, None).expect("window");
        assert_eq!(start, "2026/07/28 08:15:49");
        assert_eq!(end, "2026/07/28 15:44:39");
    }
}
