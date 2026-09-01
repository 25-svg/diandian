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
    #[serde(default, skip_serializing)]
    pub orders_payload: Option<Value>,
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

struct PaymentEventsRunner {
    executable: String,
    script_path: Option<PathBuf>,
}

fn bundled_doudian_runtime_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("doudian-runtime")))
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
    if let Some(runtime_dir) = bundled_doudian_runtime_dir() {
        push_unique(&mut sdk_fallbacks, runtime_dir.join("sdk-python"));
    }
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

/// Resolve stale paths to the first usable local/bundled credential files.
/// Callers may persist this sanitized config after a successful fetch so the
/// next launch does not keep retrying paths copied from another computer.
pub fn resolve_doudian_order_config(config: &DoudianOrderConfig) -> DoudianOrderConfig {
    resolve_doudian_credential_paths(config).config
}

fn token_env_value(path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    content.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == key)
            .then(|| value.trim().trim_matches(['"', '\'']).to_string())
            .filter(|value| !value.is_empty())
    })
}

fn token_shop_identity(path: &Path) -> Option<(String, String)> {
    let shop_name = token_env_value(path, "DOUYIN_OPEN_SHOP_NAME")?;
    let shop_id = token_env_value(path, "DOUYIN_OPEN_SHOP_ID").unwrap_or_default();
    Some((shop_name, shop_id))
}

fn push_token_files_from_dir(paths: &mut Vec<PathBuf>, directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut candidates = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("doudian_token") && name.ends_with(".env"))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    for path in candidates {
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }
}

/// Select the locally bound token whose platform shop is the base shop named
/// by the Excel session.  This happens before any paid order API call so a
/// stale default account cannot query the wrong store.
pub fn resolve_doudian_order_config_for_shop(
    config: &DoudianOrderConfig,
    excel_shop_name: &str,
) -> Result<DoudianOrderConfig, String> {
    let mut resolved = resolve_doudian_order_config(config);
    if excel_shop_name.trim().is_empty() {
        return Ok(resolved);
    }

    let mut token_files = Vec::new();
    let configured_token = PathBuf::from(&resolved.token_file);
    if configured_token.is_file() {
        token_files.push(configured_token.clone());
    }
    if let Some(directory) = configured_token.parent() {
        push_token_files_from_dir(&mut token_files, directory);
    }
    for desktop in doudian_desktop_dirs() {
        push_token_files_from_dir(&mut token_files, &desktop.join("数据接口"));
        push_token_files_from_dir(&mut token_files, &desktop);
    }

    let mut observed_shops = Vec::new();
    let mut matches = Vec::new();
    for token_file in token_files {
        let Some((shop_name, shop_id)) = token_shop_identity(&token_file) else {
            continue;
        };
        if !observed_shops.iter().any(|name| name == &shop_name) {
            observed_shops.push(shop_name.clone());
        }
        let duplicate_identity = matches.iter().any(|(_, matched_name, matched_id)| {
            (!shop_id.is_empty() && matched_id == &shop_id) || matched_name == &shop_name
        });
        if crate::live_dashboard_binding::shop_names_match(excel_shop_name, &shop_name)
            && !duplicate_identity
        {
            matches.push((token_file, shop_name, shop_id));
        }
    }

    if matches.len() == 1 {
        let (token_file, _shop_name, shop_id) = matches.remove(0);
        resolved.token_file = token_file.to_string_lossy().to_string();
        resolved.shop_id = shop_id;
        return Ok(resolved);
    }

    observed_shops.sort();
    let bound = if observed_shops.is_empty() {
        "未发现可用抖店授权".to_string()
    } else {
        observed_shops.join("、")
    };
    if matches.is_empty() {
        Err(format!(
            "Excel 店铺是“{excel_shop_name}”，但本机没有匹配的抖店授权。当前已绑定：{bound}"
        ))
    } else {
        Err(format!(
            "Excel 店铺“{excel_shop_name}”匹配到多个抖店授权，请删除重复 token 后重试"
        ))
    }
}

fn payment_events_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/doudian_fetch_payment_events.py")
}

fn parse_payment_events_json(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err("解析 payment-events JSON 失败：脚本输出为空".into());
    }
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            if let Ok(value) = serde_json::from_str(&trimmed[start..=end]) {
                return Ok(value);
            }
        }
    }
    let preview: String = trimmed.chars().take(160).collect();
    Err(format!(
        "解析 payment-events JSON 失败：输出不是 JSON。开头：{preview}"
    ))
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
    // Some imported Excel cells contain a slash-formatted start/end range.
    // Keep accepting its start value without splitting normal hyphenated dates.
    if let Some((range_start, range_end)) = trimmed.split_once('-') {
        if range_start.contains('/') && range_end.contains('/') {
            if let Ok(naive) =
                NaiveDateTime::parse_from_str(range_start.trim(), "%Y/%m/%d %H:%M:%S")
            {
                return CN_OFFSET
                    .from_local_datetime(&naive)
                    .single()
                    .ok_or_else(|| format!("无法解析时间：{value}"));
            }
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
    runner: &PaymentEventsRunner,
    resolved: &ResolvedDoudianCredentials,
    live_started_at: &str,
    live_ended_at: &str,
) -> Result<String, String> {
    let config = &resolved.config;
    if let Some(script_path) = runner.script_path.as_deref() {
        if !script_path.is_file() {
            return Err(format!("找不到脚本：{}", script_path.display()));
        }
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

    let output_path = std::env::temp_dir().join(format!(
        "bsr-payment-events-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis())
            .unwrap_or(0)
    ));

    let mut command = Command::new(&runner.executable);
    if let Some(script_path) = runner.script_path.as_deref() {
        command.arg(script_path);
    }
    command
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
        .arg("--output")
        .arg(&output_path)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if !config.shop_id.trim().is_empty() {
        command.arg("--shop-id").arg(config.shop_id.trim());
    }

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .await
        .map_err(|error| format!("无法启动订单拉取运行时（{}）：{error}", runner.executable))?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !output.status.success() {
        let _ = std::fs::remove_file(&output_path);
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            "抖店 order.searchList 拉取失败".into()
        } else {
            detail
        });
    }

    let json_text = if output_path.is_file() {
        let text = std::fs::read_to_string(&output_path)
            .map_err(|error| format!("读取订单结果失败（{}）：{error}", output_path.display()))?;
        let _ = std::fs::remove_file(&output_path);
        text
    } else {
        stdout
    };

    if json_text.trim().is_empty() {
        return Err(if stderr.is_empty() {
            "抖店 order.searchList 没有返回 JSON".into()
        } else {
            stderr
        });
    }

    Ok(json_text)
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

async fn payment_events_runner() -> Result<PaymentEventsRunner, String> {
    if let Some(runtime_dir) = bundled_doudian_runtime_dir() {
        let executable = runtime_dir.join("doudian-fetch-payment-events.exe");
        if executable.is_file() {
            return Ok(PaymentEventsRunner {
                executable: executable.to_string_lossy().to_string(),
                script_path: None,
            });
        }
    }
    Ok(PaymentEventsRunner {
        executable: pick_python_executable().await?,
        script_path: Some(payment_events_script_path()),
    })
}

pub async fn fetch_payment_events(
    config: &DoudianOrderConfig,
    live_started_at: &str,
    live_ended_at: &str,
) -> Result<FetchDoudianPaymentEventsResult, String> {
    let runner = payment_events_runner().await?;
    let resolved_config = resolve_doudian_order_config(config);
    let resolved = resolve_doudian_credential_paths(&resolved_config);
    let stdout = run_python_script(&runner, &resolved, live_started_at, live_ended_at).await?;

    let payload = parse_payment_events_json(&stdout)?;
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
        orders_payload: payload.get("orders_payload").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_payment_events_json_skips_leading_noise() {
        let raw = "Fetching order.searchList\n{\"events\":[1],\"summary\":{}}";
        let value = parse_payment_events_json(raw).expect("json object");
        assert_eq!(value["events"][0], 1);
    }

    #[test]
    fn parse_payment_events_json_rejects_empty_output() {
        assert!(parse_payment_events_json("   ").is_err());
    }

    #[test]
    fn resolves_token_by_excel_base_shop_before_order_fetch() {
        let directory = tempfile::tempdir().expect("temp token dir");
        let wrong = directory.path().join("doudian_token.env");
        let correct = directory.path().join("doudian_token_141447741.env");
        std::fs::write(
            &wrong,
            "DOUYIN_OPEN_SHOP_ID=212709966\nDOUYIN_OPEN_SHOP_NAME=金典拍拍相机专卖店\n",
        )
        .expect("wrong token metadata");
        std::fs::write(
            &correct,
            "DOUYIN_OPEN_SHOP_ID=141447741\nDOUYIN_OPEN_SHOP_NAME=金典拍拍科创专卖店\n",
        )
        .expect("correct token metadata");
        let config = DoudianOrderConfig {
            env_file: String::new(),
            token_file: wrong.to_string_lossy().to_string(),
            sdk_path: String::new(),
            shop_id: "212709966".into(),
        };

        let resolved =
            resolve_doudian_order_config_for_shop(&config, "金典拍拍科创专卖店-复古相机专场")
                .expect("select science-and-technology shop token");

        assert_eq!(PathBuf::from(resolved.token_file), correct);
        assert_eq!(resolved.shop_id, "141447741");
    }

    #[test]
    fn parse_timestamp_accepts_iso_local_datetime_without_timezone() {
        let parsed = parse_timestamp("2026-08-01T08:57:19").expect("local ISO timestamp");
        assert_eq!(format_live_time(parsed), "2026/08/01 08:57:19");
    }

    #[test]
    fn parse_timestamp_keeps_slash_formatted_range_compatibility() {
        let parsed = parse_timestamp("2026/08/01 08:57:19-2026/08/01 16:01:19")
            .expect("slash-formatted time range");
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
            source_path: String::new(),
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
