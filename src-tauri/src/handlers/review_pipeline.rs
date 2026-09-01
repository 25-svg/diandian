use std::cmp::Ordering;
use std::str::FromStr;

use chrono::Utc;
use recorder::events::RecorderEvent;
use recorder::platforms::PlatformType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::database::task::TaskRow;
use crate::ffmpeg::Range;
use crate::handlers::doudian_orders::{
    fetch_doudian_payment_events_state, FetchDoudianPaymentEventsRequest,
};
use crate::progress::progress_reporter::{EventEmitter, ProgressReporter, ProgressReporterTrait};
use crate::recorder_manager::ClipRangeParams;
use crate::state::State;
use crate::state_type;
use crate::task::{Task, TaskPriority};

#[cfg(feature = "gui")]
use tauri::State as TauriState;

const PIPELINE_TASK_TYPE: &str = "auto_review_pipeline";
const MAX_AI_CHAINS: usize = 20;
const CLUSTER_GAP_SEC: f64 = 5.0 * 60.0;
const CONTEXT_PRE_SEC: f64 = 10.0 * 60.0;
const CONTEXT_POST_SEC: f64 = 2.0 * 60.0;
const MIN_CLIP_SEC: f64 = 45.0;
const MAX_CLIP_SEC: f64 = 8.0 * 60.0;

const DEAL_CLIP_SYSTEM_PROMPT: &str = "你是直播带货完整成交链路剪辑师。订单支付时间只是成交锚点，不是视频开始时间。必须向前回溯，并定位这一商品从需求或疑问、产品匹配、卖点价值、风险消除或售后承诺、价格优惠与上链接、引导下单，直到支付或成交确认的完整连续话术。每个订单簇只输出一个连续视频区间，禁止拆成多个零散片段。只输出一个 JSON 对象，不要 Markdown。字段：start（秒）、end（秒）、title（商品名+完整成交话术）、reason（一句话说明链路起止证据）。start/end 必须位于给定上下文范围，最终片长约45秒到8分钟。不得编造逐字稿中不存在的话术。";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedPaymentEvents {
    summary: Value,
    events: Value,
    source_label: Option<String>,
}

#[derive(Debug, Clone)]
struct PaymentEvent {
    offset_sec: f64,
    pay_amount_fen: i64,
    product_name: String,
}

#[derive(Debug, Clone)]
struct TranscriptCue {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Debug, Clone)]
struct OrderCluster {
    product_key: String,
    product_name: String,
    first_sec: f64,
    last_sec: f64,
    order_count: usize,
    pay_amount_fen: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiClipRange {
    start: f64,
    end: f64,
    title: String,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineMetricAnalysis {
    status: String,
    message: String,
    session_id: Option<i64>,
    capture_id: Option<String>,
    analysis_id: Option<String>,
    metric_count: usize,
    decline_event_count: usize,
    generated_at: Option<String>,
}

impl PipelineMetricAnalysis {
    fn active() -> Self {
        Self {
            status: "active".to_string(),
            message: "正在读取05直播大屏曲线并关联逐字稿".to_string(),
            ..Self::default()
        }
    }

    fn attention(message: impl Into<String>, session_id: Option<i64>) -> Self {
        Self {
            status: "attention".to_string(),
            message: message.into(),
            session_id,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryAutoReviewResult {
    pub task_id: Option<String>,
    pub queued: bool,
}

fn payment_event_number(value: &Value, snake: &str, camel: &str) -> Option<f64> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_f64().or_else(|| item.as_str()?.parse().ok()))
}

fn payment_event_string(value: &Value, snake: &str, camel: &str) -> String {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn parse_payment_events(value: &Value) -> Vec<PaymentEvent> {
    let mut events = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let offset_sec = payment_event_number(item, "offset_sec", "offsetSec")?;
            if !offset_sec.is_finite() || offset_sec < 0.0 {
                return None;
            }
            Some(PaymentEvent {
                offset_sec,
                pay_amount_fen: payment_event_number(item, "pay_amount_fen", "payAmountFen")
                    .unwrap_or(0.0)
                    .max(0.0) as i64,
                product_name: payment_event_string(item, "product_name", "productName"),
            })
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.offset_sec
            .partial_cmp(&right.offset_sec)
            .unwrap_or(Ordering::Equal)
    });
    events
}

fn normalize_product_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn cluster_orders(events: &[PaymentEvent]) -> Vec<OrderCluster> {
    let mut clusters: Vec<OrderCluster> = Vec::new();
    for event in events {
        let product_key = normalize_product_name(&event.product_name);
        let max_gap = if product_key.is_empty() {
            2.0 * 60.0
        } else {
            CLUSTER_GAP_SEC
        };
        if let Some(cluster) = clusters.iter_mut().rev().find(|cluster| {
            cluster.product_key == product_key && event.offset_sec - cluster.last_sec <= max_gap
        }) {
            cluster.last_sec = event.offset_sec;
            cluster.order_count += 1;
            cluster.pay_amount_fen += event.pay_amount_fen;
        } else {
            clusters.push(OrderCluster {
                product_key,
                product_name: if event.product_name.is_empty() {
                    "未命名商品".to_string()
                } else {
                    event.product_name.clone()
                },
                first_sec: event.offset_sec,
                last_sec: event.offset_sec,
                order_count: 1,
                pay_amount_fen: event.pay_amount_fen,
            });
        }
    }
    clusters
}

fn parse_srt_timestamp(value: &str) -> Option<f64> {
    let normalized = value.trim().replace(',', ".");
    let parts = normalized.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<f64>().ok()?;
    let minutes = parts[1].parse::<f64>().ok()?;
    let seconds = parts[2].parse::<f64>().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn parse_srt(value: &str) -> Vec<TranscriptCue> {
    value
        .replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|block| {
            let lines = block.lines().collect::<Vec<_>>();
            let timeline_index = lines.iter().position(|line| line.contains("-->"))?;
            let (start, end) = lines[timeline_index].split_once("-->")?;
            let text = lines[timeline_index + 1..]
                .iter()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if text.is_empty() {
                return None;
            }
            Some(TranscriptCue {
                start: parse_srt_timestamp(start)?,
                end: parse_srt_timestamp(end)?,
                text,
            })
        })
        .collect()
}

fn format_clock(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

fn build_ai_prompt(
    cluster: &OrderCluster,
    cues: &[TranscriptCue],
    duration: f64,
) -> Option<String> {
    let context_start = (cluster.first_sec - CONTEXT_PRE_SEC).max(0.0);
    let context_end = (cluster.last_sec + CONTEXT_POST_SEC).min(duration.max(cluster.last_sec));
    let lines = cues
        .iter()
        .filter(|cue| cue.end >= context_start && cue.start <= context_end)
        .map(|cue| {
            format!(
                "[{}-{}] {}",
                format_clock(cue.start),
                format_clock(cue.end),
                cue.text
            )
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "订单簇：{}–{}；商品：{}；成交 {} 单；金额 ¥{:.2}。\n上下文范围：{:.1}–{:.1} 秒。请从逐字稿中定位一条连续完整成交链。\n逐字稿：\n{}",
        format_clock(cluster.first_sec),
        format_clock(cluster.last_sec),
        cluster.product_name,
        cluster.order_count,
        cluster.pay_amount_fen as f64 / 100.0,
        context_start,
        context_end,
        lines.join("\n")
    ))
}

fn parse_ai_range(raw: &str, cluster: &OrderCluster, duration: f64) -> Result<AiClipRange, String> {
    let trimmed = raw.trim();
    let json_text = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        trimmed
    } else {
        let start = trimmed.find('{').ok_or("MiniMax 未返回 JSON 对象")?;
        let end = trimmed.rfind('}').ok_or("MiniMax 未返回完整 JSON 对象")?;
        &trimmed[start..=end]
    };
    let mut range: AiClipRange = serde_json::from_str(json_text)
        .map_err(|error| format!("MiniMax 切片范围格式错误：{error}"))?;
    let context_start = (cluster.first_sec - CONTEXT_PRE_SEC).max(0.0);
    let context_end = (cluster.last_sec + CONTEXT_POST_SEC).min(duration.max(cluster.last_sec));
    if !(range.start.is_finite() && range.end.is_finite() && range.end > range.start) {
        return Err("MiniMax 返回的切片起止时间无效".into());
    }
    range.start = range.start.max(context_start).max(0.0);
    range.end = range.end.min(context_end).min(duration);
    if range.end - range.start > MAX_CLIP_SEC {
        range.start = (range.end - MAX_CLIP_SEC).max(context_start);
    }
    if range.end - range.start < MIN_CLIP_SEC {
        range.start = (range.end - MIN_CLIP_SEC).max(context_start).max(0.0);
        range.end = (range.start + MIN_CLIP_SEC).min(context_end).min(duration);
    }
    if range.end <= range.start {
        return Err("成交链路范围超出本场录像时长".into());
    }
    if range.title.trim().is_empty() {
        range.title = format!("{}完整成交话术", cluster.product_name);
    }
    Ok(range)
}

#[cfg(feature = "gui")]
fn normalized_session_clock(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .take(12)
        .collect()
}

#[cfg(feature = "gui")]
fn capture_matches_session(
    capture: &crate::compass_capture::CompassCaptureSummary,
    session: &crate::database::live_dashboard::LiveDashboardSessionRow,
) -> bool {
    let target_date = session.started_at.get(..10).unwrap_or_default();
    if capture.target_date != target_date
        || capture.target_shop_name.trim() != session.shop_name.trim()
        || capture.analysis_response_count == 0
    {
        return false;
    }
    capture.session_keys.is_empty()
        || capture.session_keys.iter().any(|session_key| {
            let mut parts = session_key.split('|');
            parts.next().unwrap_or_default().trim() == session.shop_name.trim()
                && normalized_session_clock(parts.next().unwrap_or_default())
                    == normalized_session_clock(&session.started_at)
        })
}

#[cfg(feature = "gui")]
fn select_capture_for_session<'a>(
    captures: &'a [crate::compass_capture::CompassCaptureSummary],
    session: &crate::database::live_dashboard::LiveDashboardSessionRow,
) -> Option<&'a crate::compass_capture::CompassCaptureSummary> {
    captures
        .iter()
        .find(|capture| {
            capture_matches_session(capture, session)
                && matches!(capture.status.as_str(), "completed" | "partial")
        })
        .or_else(|| {
            captures
                .iter()
                .find(|capture| capture_matches_session(capture, session))
        })
}

#[cfg(feature = "gui")]
async fn analyze_live_dashboard_metrics(
    state: &State,
    platform: &str,
    room_id: &str,
    live_id: &str,
) -> PipelineMetricAnalysis {
    let resolved =
        match crate::handlers::live_dashboard_binding::resolve_live_dashboard_for_record_state(
            state,
            platform.to_string(),
            room_id.to_string(),
            live_id.to_string(),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                return PipelineMetricAnalysis::attention(
                    format!("05直播大屏场次匹配失败：{error}"),
                    None,
                )
            }
        };
    let Some(session) = resolved.session else {
        let detail = if resolved.candidates.is_empty() {
            "没有找到同店铺、同开播时间的场次".to_string()
        } else {
            format!(
                "找到 {} 个候选场次，无法安全自动选择",
                resolved.candidates.len()
            )
        };
        return PipelineMetricAnalysis::attention(format!("尚未匹配05直播大屏：{detail}"), None);
    };
    let captures = match crate::compass_capture::list_compass_captures_state(state).await {
        Ok(value) => value,
        Err(error) => {
            return PipelineMetricAnalysis::attention(
                format!("无法读取05直播大屏采集结果：{error}"),
                Some(session.id),
            )
        }
    };
    let Some(capture) = select_capture_for_session(&captures, &session) else {
        return PipelineMetricAnalysis::attention(
            "已匹配直播场次，但05直播大屏尚未采集本场分钟曲线",
            Some(session.id),
        );
    };
    let raw_capture_id = capture.capture_id.clone();
    match crate::compass_analysis::analyze_compass_capture_state(
        state,
        raw_capture_id.clone(),
        Some(session.id),
    )
    .await
    {
        Ok(analysis) if !analysis.metrics.is_empty() => PipelineMetricAnalysis {
            status: "done".to_string(),
            message: format!(
                "已从05直播大屏分析 {} 条曲线、{} 个关键下降区间",
                analysis.metrics.len(),
                analysis.decline_events.len()
            ),
            session_id: Some(session.id),
            capture_id: Some(raw_capture_id),
            analysis_id: Some(analysis.capture_id),
            metric_count: analysis.metrics.len(),
            decline_event_count: analysis.decline_events.len(),
            generated_at: Some(analysis.generated_at),
        },
        Ok(_) => PipelineMetricAnalysis::attention(
            "05直播大屏采集存在，但没有识别到可绘制的分钟曲线",
            Some(session.id),
        ),
        Err(error) => PipelineMetricAnalysis::attention(
            format!("05直播大屏指标分析失败：{error}"),
            Some(session.id),
        ),
    }
}

#[cfg(not(feature = "gui"))]
async fn analyze_live_dashboard_metrics(
    _state: &State,
    _platform: &str,
    _room_id: &str,
    _live_id: &str,
) -> PipelineMetricAnalysis {
    PipelineMetricAnalysis::attention("当前运行模式不支持05直播大屏分析", None)
}

fn pipeline_metadata(
    platform: &str,
    room_id: &str,
    live_id: &str,
    stage: &str,
    metric_analysis: Option<&PipelineMetricAnalysis>,
    payments: Option<&PersistedPaymentEvents>,
    ranges: &[AiClipRange],
    generated_video_ids: &[i64],
) -> String {
    json!({
        "platform": platform,
        "room_id": room_id,
        "live_id": live_id,
        "pipeline_version": 2,
        "stage": stage,
        "metric_analysis": metric_analysis,
        "payment_events": payments,
        "ranges": ranges,
        "generated_video_ids": generated_video_ids,
        "review_status": if generated_video_ids.is_empty() { "processing" } else { "pending_host_review" },
    })
    .to_string()
}

async fn run_pipeline(
    state: State,
    task_id: String,
    platform: PlatformType,
    room_id: String,
    live_id: String,
) -> Result<(), String> {
    #[cfg(feature = "gui")]
    let emitter = EventEmitter::new(state.app_handle.clone());
    #[cfg(feature = "headless")]
    let emitter = EventEmitter::new(state.progress_manager.get_event_sender());
    let reporter = ProgressReporter::new(state.db.clone(), &emitter, &task_id).await?;
    let platform_name = platform.as_str();
    let record = state
        .db
        .get_record(&room_id, &live_id)
        .await
        .map_err(|error| format!("找不到本场录播：{error}"))?;
    let duration = record.length.max(1.0);

    state
        .db
        .update_task(&task_id, "processing", "正在自动匹配本场订单", None)
        .await
        .map_err(|error| error.to_string())?;
    reporter.update("正在自动匹配本场订单").await;
    let order_result = fetch_doudian_payment_events_state(
        &state,
        FetchDoudianPaymentEventsRequest {
            room_id: room_id.clone(),
            live_id: live_id.clone(),
            live_started_at: None,
            live_ended_at: None,
        },
    )
    .await;
    let order_progress = match &order_result {
        Ok(result) => format!(
            "已匹配 {} 笔订单，正在生成逐字稿",
            result.events.as_array().map(Vec::len).unwrap_or(0)
        ),
        Err(_) => "订单暂未匹配，仍继续生成逐字稿".to_string(),
    };
    let persisted_order_preview = order_result
        .as_ref()
        .ok()
        .map(|result| PersistedPaymentEvents {
            summary: result.summary.clone(),
            events: result.events.clone(),
            source_label: result.source_label.clone(),
        });
    state
        .db
        .update_task(
            &task_id,
            "processing",
            &order_progress,
            Some(&pipeline_metadata(
                platform_name,
                &room_id,
                &live_id,
                "transcript",
                None,
                persisted_order_preview.as_ref(),
                &[],
                &[],
            )),
        )
        .await
        .map_err(|error| error.to_string())?;
    reporter.update(&order_progress).await;

    let subtitle = match state
        .recorder_manager
        .get_archive_subtitle(platform, &room_id, &live_id)
        .await
    {
        Ok(value) if !value.trim().is_empty() => value,
        _ => state
            .recorder_manager
            .generate_archive_subtitle(platform, &room_id, &live_id, Some(&reporter))
            .await
            .map_err(|error| format!("自动转写失败：{error}"))?,
    };
    let active_metric_analysis = PipelineMetricAnalysis::active();
    state
        .db
        .update_task(
            &task_id,
            "processing",
            &active_metric_analysis.message,
            Some(&pipeline_metadata(
                platform_name,
                &room_id,
                &live_id,
                "metrics",
                Some(&active_metric_analysis),
                persisted_order_preview.as_ref(),
                &[],
                &[],
            )),
        )
        .await
        .map_err(|error| error.to_string())?;
    reporter.update(&active_metric_analysis.message).await;
    let metric_analysis =
        analyze_live_dashboard_metrics(&state, platform_name, &room_id, &live_id).await;
    state
        .db
        .update_task(
            &task_id,
            "processing",
            &metric_analysis.message,
            Some(&pipeline_metadata(
                platform_name,
                &room_id,
                &live_id,
                "metrics",
                Some(&metric_analysis),
                persisted_order_preview.as_ref(),
                &[],
                &[],
            )),
        )
        .await
        .map_err(|error| error.to_string())?;
    reporter.update(&metric_analysis.message).await;
    let order_result = order_result.map_err(|error| {
        format!("逐字稿和指标分析已完成；自动匹配订单失败，可在复盘页重试：{error}")
    })?;
    let payments = PersistedPaymentEvents {
        summary: order_result.summary,
        events: order_result.events,
        source_label: order_result.source_label,
    };
    let payment_events = parse_payment_events(&payments.events);
    if payment_events.is_empty() {
        return Err("逐字稿已生成；本场没有可用成交订单，暂不生成切片".into());
    }
    let cues = parse_srt(&subtitle);
    if cues.is_empty() {
        return Err("逐字稿已生成，但没有可分析的时间轴内容".into());
    }

    let clusters = cluster_orders(&payment_events);
    let api_key = crate::handlers::ai::configured_minimax_api_key(&state).await?;
    let mut ranges = Vec::new();
    let mut analysis_errors = Vec::new();
    for (index, cluster) in clusters.iter().take(MAX_AI_CHAINS).enumerate() {
        let Some(prompt) = build_ai_prompt(cluster, &cues, duration) else {
            analysis_errors.push(format!("{}附近没有逐字稿", cluster.product_name));
            continue;
        };
        let progress = format!(
            "AI 正在定位完整成交链路 {}/{}：{}",
            index + 1,
            clusters.len().min(MAX_AI_CHAINS),
            cluster.product_name
        );
        reporter.update(&progress).await;
        state
            .db
            .update_task(
                &task_id,
                "processing",
                &progress,
                Some(&pipeline_metadata(
                    platform_name,
                    &room_id,
                    &live_id,
                    "analysis",
                    Some(&metric_analysis),
                    Some(&payments),
                    &ranges,
                    &[],
                )),
            )
            .await
            .map_err(|error| error.to_string())?;
        match crate::handlers::ai::request_minimax_text(
            &api_key,
            DEAL_CLIP_SYSTEM_PROMPT,
            vec![json!({"role": "user", "content": prompt})],
            512,
        )
        .await
        .and_then(|raw| parse_ai_range(&raw, cluster, duration))
        {
            Ok(range) => ranges.push(range),
            Err(error) => analysis_errors.push(format!("{}：{error}", cluster.product_name)),
        }
    }
    if ranges.is_empty() {
        return Err(format!(
            "没有识别到可切片的完整成交链路{}",
            if analysis_errors.is_empty() {
                String::new()
            } else {
                format!("：{}", analysis_errors.join("；"))
            }
        ));
    }

    let mut generated_video_ids = Vec::new();
    let mut clip_errors = Vec::new();
    for (index, range) in ranges.iter().enumerate() {
        let child_task_id = format!("{task_id}_clip_{index}");
        let child_task = TaskRow {
            id: child_task_id.clone(),
            task_type: "clip_range".to_string(),
            status: "processing".to_string(),
            message: format!("正在生成待审核切片：{}", range.title),
            metadata: json!({
                "parent_task_id": task_id,
                "platform": platform_name,
                "room_id": room_id,
                "live_id": live_id,
                "start": range.start,
                "end": range.end,
            })
            .to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        if state.db.add_task(&child_task).await.is_err() {
            continue;
        }
        let child_reporter =
            ProgressReporter::new(state.db.clone(), &emitter, &child_task_id).await?;
        let progress = format!(
            "正在生成学习切片 {}/{}：{}",
            index + 1,
            ranges.len(),
            range.title
        );
        reporter.update(&progress).await;
        state
            .db
            .update_task(
                &task_id,
                "processing",
                &progress,
                Some(&pipeline_metadata(
                    platform_name,
                    &room_id,
                    &live_id,
                    "clip",
                    Some(&metric_analysis),
                    Some(&payments),
                    &ranges,
                    &generated_video_ids,
                )),
            )
            .await
            .map_err(|error| error.to_string())?;
        let params = ClipRangeParams {
            title: range.title.clone(),
            note: json!({
                "analysisPurpose": "enterprise_review",
                "reviewStatus": "pending_host_review",
                "sourcePlatform": platform_name,
                "sourceRoomId": room_id,
                "sourceLiveId": live_id,
                "reason": range.reason,
            })
            .to_string(),
            cover: String::new(),
            platform: platform_name.to_string(),
            room_id: room_id.clone(),
            live_id: live_id.clone(),
            ranges: vec![Range {
                start: range.start,
                end: range.end,
            }],
            danmu: false,
            local_offset: 0,
            fix_encoding: false,
            transition: None,
        };
        match crate::handlers::video::clip_range_inner(&state, &child_reporter, params).await {
            Ok(video) => {
                generated_video_ids.push(video.id);
                let _ = state
                    .db
                    .update_task(&child_task_id, "success", "待审核学习切片生成完成", None)
                    .await;
            }
            Err(error) => {
                clip_errors.push(format!("{}：{error}", range.title));
                let _ = state
                    .db
                    .update_task(
                        &child_task_id,
                        "failed",
                        &format!("切片失败：{error}"),
                        None,
                    )
                    .await;
            }
        }
    }
    if generated_video_ids.is_empty() {
        return Err(format!("全部学习切片生成失败：{}", clip_errors.join("；")));
    }

    let message = if clip_errors.is_empty() {
        format!(
            "已生成 {} 条学习切片，等待主播审核",
            generated_video_ids.len()
        )
    } else {
        format!(
            "已生成 {} 条学习切片，另有 {} 条失败，等待主播审核",
            generated_video_ids.len(),
            clip_errors.len()
        )
    };
    let metadata = pipeline_metadata(
        platform_name,
        &room_id,
        &live_id,
        "review",
        Some(&metric_analysis),
        Some(&payments),
        &ranges,
        &generated_video_ids,
    );
    state
        .db
        .update_task(&task_id, "success", &message, Some(&metadata))
        .await
        .map_err(|error| error.to_string())?;
    reporter.finish(true, &message).await;
    Ok(())
}

async fn schedule_existing_pipeline(
    state: State,
    task_id: String,
    platform: PlatformType,
    room_id: String,
    live_id: String,
) -> Result<(), String> {
    state
        .db
        .update_task(&task_id, "pending", "自动复盘任务正在排队", None)
        .await
        .map_err(|error| error.to_string())?;
    let worker_state = state.clone();
    let worker_task_id = task_id.clone();
    let queued = state
        .task_manager
        .add_task(Task::new(
            task_id.clone(),
            TaskPriority::Normal,
            async move {
                match run_pipeline(
                    worker_state.clone(),
                    worker_task_id.clone(),
                    platform,
                    room_id,
                    live_id,
                )
                .await
                {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        #[cfg(feature = "gui")]
                        let emitter = EventEmitter::new(worker_state.app_handle.clone());
                        #[cfg(feature = "headless")]
                        let emitter =
                            EventEmitter::new(worker_state.progress_manager.get_event_sender());
                        if let Ok(reporter) = ProgressReporter::new(
                            worker_state.db.clone(),
                            &emitter,
                            &worker_task_id,
                        )
                        .await
                        {
                            reporter.finish(false, &error).await;
                        }
                        let _ = worker_state
                            .db
                            .update_task(&worker_task_id, "failed", &error, None)
                            .await;
                        Err(error)
                    }
                }
            },
        ))
        .await;
    if let Err(error) = queued {
        state
            .db
            .update_task(
                &task_id,
                "failed",
                &format!("自动复盘任务无法排队：{error}"),
                None,
            )
            .await
            .map_err(|db_error| db_error.to_string())?;
        return Err(error);
    }
    Ok(())
}

pub async fn enqueue_auto_review_pipeline(
    state: State,
    platform: PlatformType,
    room_id: String,
    live_id: String,
) -> Result<RetryAutoReviewResult, String> {
    let record = state
        .db
        .get_record(&room_id, &live_id)
        .await
        .map_err(|error| error.to_string())?;
    if record.archive_kind != "company" || record.length <= 0.0 || record.size <= 0 {
        return Ok(RetryAutoReviewResult {
            task_id: None,
            queued: false,
        });
    }
    let task = TaskRow {
        id: uuid::Uuid::new_v4().to_string(),
        task_type: PIPELINE_TASK_TYPE.to_string(),
        status: "pending".to_string(),
        message: "下播完成，等待自动匹配订单".to_string(),
        metadata: pipeline_metadata(
            platform.as_str(),
            &room_id,
            &live_id,
            "orders",
            None,
            None,
            &[],
            &[],
        ),
        created_at: Utc::now().to_rfc3339(),
    };
    let inserted = state
        .db
        .add_auto_review_pipeline_task_if_absent(&task, platform.as_str(), &room_id, &live_id)
        .await
        .map_err(|error| error.to_string())?;
    if !inserted {
        let existing = state
            .db
            .get_latest_auto_review_pipeline_task(platform.as_str(), &room_id, &live_id)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(RetryAutoReviewResult {
            task_id: existing.map(|item| item.id),
            queued: false,
        });
    }
    schedule_existing_pipeline(state, task.id.clone(), platform, room_id, live_id).await?;
    Ok(RetryAutoReviewResult {
        task_id: Some(task.id),
        queued: true,
    })
}

async fn resume_interrupted_pipelines(state: &State) {
    let Ok(tasks) = state.db.get_tasks().await else {
        return;
    };
    for task in tasks
        .into_iter()
        .filter(|task| task.task_type == PIPELINE_TASK_TYPE && task.status == "interrupted")
    {
        let Ok(metadata) = serde_json::from_str::<Value>(&task.metadata) else {
            continue;
        };
        let Some(platform) = metadata
            .get("platform")
            .and_then(Value::as_str)
            .and_then(|value| PlatformType::from_str(value).ok())
        else {
            continue;
        };
        let Some(room_id) = metadata.get("room_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(live_id) = metadata.get("live_id").and_then(Value::as_str) else {
            continue;
        };
        let _ = schedule_existing_pipeline(
            state.clone(),
            task.id,
            platform,
            room_id.to_string(),
            live_id.to_string(),
        )
        .await;
    }
}

pub async fn start_auto_review_pipeline(state: State) {
    resume_interrupted_pipelines(&state).await;
    let mut receiver = state.recorder_manager.get_event_sender().subscribe();
    loop {
        match receiver.recv().await {
            Ok(RecorderEvent::LiveEnd {
                platform,
                room_id,
                recorder,
            }) if !recorder.live_id.is_empty() => {
                let pipeline_state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) = enqueue_auto_review_pipeline(
                        pipeline_state,
                        platform,
                        room_id,
                        recorder.live_id,
                    )
                    .await
                    {
                        log::error!("Failed to enqueue auto review pipeline: {error}");
                    }
                });
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                log::warn!("Auto review pipeline skipped {skipped} recorder events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn retry_auto_review_pipeline(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<RetryAutoReviewResult, String> {
    let platform = PlatformType::from_str(&platform)?;
    #[cfg(feature = "gui")]
    let state = (*state).clone();
    #[cfg(feature = "headless")]
    let state = state.clone();
    enqueue_auto_review_pipeline(state, platform, room_id, live_id).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_auto_review_payment_events(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<Option<Value>, String> {
    let task = state
        .db
        .get_latest_auto_review_pipeline_task(&platform, &room_id, &live_id)
        .await
        .map_err(|error| error.to_string())?;
    let Some(task) = task else {
        return Ok(None);
    };
    let metadata: Value =
        serde_json::from_str(&task.metadata).map_err(|error| error.to_string())?;
    Ok(metadata.get("payment_events").cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_repeat_orders_for_the_same_product() {
        let clusters = cluster_orders(&[
            PaymentEvent {
                offset_sec: 100.0,
                pay_amount_fen: 1000,
                product_name: "佳能 R6 二代".into(),
            },
            PaymentEvent {
                offset_sec: 250.0,
                pay_amount_fen: 2000,
                product_name: "佳能R6二代".into(),
            },
            PaymentEvent {
                offset_sec: 700.0,
                pay_amount_fen: 3000,
                product_name: "佳能R6二代".into(),
            },
        ]);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].order_count, 2);
        assert_eq!(clusters[0].pay_amount_fen, 3000);
    }

    #[test]
    fn parses_srt_timeline_for_ai_context() {
        let cues = parse_srt("1\n00:00:01,000 --> 00:00:03,500\n这台相机支持防抖\n\n2\n00:00:04,000 --> 00:00:05,000\n现在下单\n");
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start, 1.0);
        assert_eq!(cues[1].text, "现在下单");
    }

    #[test]
    fn clamps_ai_range_to_context_and_duration() {
        let cluster = OrderCluster {
            product_key: "camera".into(),
            product_name: "相机".into(),
            first_sec: 700.0,
            last_sec: 710.0,
            order_count: 1,
            pay_amount_fen: 100,
        };
        let range = parse_ai_range(
            r#"{"start":0,"end":9999,"title":"相机完整成交话术","reason":"test"}"#,
            &cluster,
            800.0,
        )
        .unwrap();
        assert!(range.start >= 100.0);
        assert!(range.end <= 800.0);
        assert!(range.end - range.start <= MAX_CLIP_SEC);
    }
}
