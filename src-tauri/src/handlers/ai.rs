use crate::state::State;
use crate::state_type;
use crate::subtitle_generator::funasr::CorrectionChange;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

const MINIMAX_URL: &str = "https://api.minimaxi.com/anthropic/v1/messages";
const MINIMAX_MODEL: &str = "MiniMax-VL-01";

#[derive(Debug, Deserialize, Serialize)]
pub struct MiniMaxMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ProposedCorrection {
    #[serde(rename = "原文")]
    source: String,
    #[serde(rename = "校对稿")]
    corrected: String,
    #[serde(rename = "修改类型", default)]
    change_type: String,
    #[serde(rename = "依据", default)]
    evidence: String,
    #[serde(rename = "是否需要人工确认", default)]
    needs_review: bool,
}

#[derive(Debug, Deserialize)]
struct ProposedCorrectionEnvelope {
    #[serde(default)]
    changes: Vec<ProposedCorrection>,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn minimax_chat(
    state: state_type!(),
    system_prompt: String,
    messages: Vec<MiniMaxMessage>,
) -> Result<String, String> {
    let api_key = state.config.read().await.openai_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("MiniMax API Key 尚未配置，请由管理员完成一次初始化。".to_string());
    }

    let messages: Vec<Value> = messages
        .into_iter()
        .filter(|message| message.role == "user" || message.role == "assistant")
        .map(|message| json!({ "role": message.role, "content": message.content }))
        .collect();

    request_minimax_text(&api_key, &system_prompt, messages, 4096).await
}

pub(crate) async fn request_minimax_text(
    api_key: &str,
    system_prompt: &str,
    messages: Vec<Value>,
    max_tokens: u32,
) -> Result<String, String> {
    let request_body = json!({
        "model": MINIMAX_MODEL,
        "max_tokens": max_tokens,
        "system": system_prompt,
        "messages": messages,
    });
    request_minimax_payload(api_key, &request_body).await
}

pub(crate) async fn request_minimax_payload(
    api_key: &str,
    request_body: &Value,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(180))
        .http1_only()
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|error| format!("创建 MiniMax 客户端失败：{error}"))?;

    let mut response = None;
    let mut last_error = None;
    for attempt in 0..4_u64 {
        match client
            .post(MINIMAX_URL)
            .bearer_auth(api_key)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("connection", "close")
            .json(request_body)
            .send()
            .await
        {
            Ok(result)
                if attempt < 3
                    && (result.status().is_server_error()
                        || result.status() == reqwest::StatusCode::TOO_MANY_REQUESTS) =>
            {
                last_error = Some(format!("MiniMax HTTP {}", result.status().as_u16()));
            }
            Ok(result) => {
                response = Some(result);
                break;
            }
            Err(error) if attempt < 3 => {
                log::warn!("MiniMax request attempt {} failed: {}", attempt + 1, error);
                last_error = Some(error.to_string());
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1_u64 << attempt)).await;
    }

    let response = response.ok_or_else(|| {
        format!(
            "连接 MiniMax 失败，系统已自动重试 4 次：{}",
            last_error.unwrap_or_else(|| "未知网络错误".to_string())
        )
    })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 MiniMax 响应失败：{error}"))?;

    if !status.is_success() {
        return Err(format!("MiniMax HTTP {}：{}", status.as_u16(), body));
    }

    let payload: Value = serde_json::from_str(&body)
        .map_err(|error| format!("MiniMax 响应格式错误：{error}；原始响应：{body}"))?;
    let text = payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if text.trim().is_empty() {
        return Err(format!("MiniMax 未返回文本内容；原始响应：{body}"));
    }

    Ok(text)
}

/// MiniMax may only propose replacements. Local validation applies a proposal
/// when its target is an exact scalar from the fact card, or a bracketed
/// `待确认` placeholder. It never accepts a free-form rewritten transcript.
pub async fn minimax_correct_transcript(
    api_key: &str,
    raw_text: &str,
    deterministic_text: &str,
    fact_card: Option<&Value>,
) -> Result<(String, Vec<CorrectionChange>), String> {
    if api_key.trim().is_empty() {
        return Ok((deterministic_text.to_string(), Vec::new()));
    }

    let fact_card_json = fact_card.cloned().unwrap_or(Value::Null);
    let system_prompt = r#"你是直播逐字稿受约束校对器，不是文案改写器。
只能输出 JSON：{"changes":[{"原文":"...","校对稿":"...","修改类型":"...","依据":"...","是否需要人工确认":true}]}
规则：
1. 不得重写句式、删减口头语、总结或润色。
2. 只建议纠正商品实体，或把不确定的价格、库存、链接号标成方括号待确认占位符。
3. 价格、库存、赠品、链接号、成交状态不得根据上下文猜测。
4. 确定值必须逐字存在于事实卡；事实卡没有则只能保留原文或使用[...待确认...]。
5. 原文必须是校对底稿中的连续原字符串。没有安全修改时输出 {"changes":[]}。"#;
    let user_payload = json!({
        "原始转写": raw_text,
        "确定性规则校对稿": deterministic_text,
        "事实卡": fact_card_json,
    });
    let response = request_minimax_text(
        api_key,
        system_prompt,
        vec![json!({"role": "user", "content": user_payload.to_string()})],
        1600,
    )
    .await?;
    let payload: ProposedCorrectionEnvelope = serde_json::from_str(extract_json_object(&response))
        .map_err(|error| format!("MiniMax 校对结果不是合法 JSON：{error}"))?;

    let mut allowed_targets = HashSet::new();
    if let Some(card) = fact_card {
        collect_safe_fact_scalars(card, &mut allowed_targets);
    }

    Ok(apply_proposed_corrections(
        deterministic_text,
        payload.changes,
        &allowed_targets,
    ))
}

fn apply_proposed_corrections(
    deterministic_text: &str,
    proposals: Vec<ProposedCorrection>,
    allowed_targets: &HashSet<String>,
) -> (String, Vec<CorrectionChange>) {
    let mut corrected_text = deterministic_text.to_string();
    let mut accepted = Vec::new();
    for proposal in proposals.into_iter().take(50) {
        let source = proposal.source.trim();
        let target = proposal.corrected.trim();
        if source.is_empty() || target.is_empty() || source.len() > 200 || target.len() > 200 {
            continue;
        }
        if corrected_text.matches(source).count() != 1 {
            continue;
        }
        let is_placeholder = target.starts_with('[')
            && target.ends_with(']')
            && target.contains("待确认")
            && !target.contains('\n');
        if !is_placeholder && !allowed_targets.contains(target) {
            continue;
        }

        corrected_text = corrected_text.replacen(source, target, 1);
        accepted.push(CorrectionChange {
            source: source.to_string(),
            corrected: target.to_string(),
            change_type: if proposal.change_type.trim().is_empty() {
                "MiniMax受约束校对".to_string()
            } else {
                proposal.change_type
            },
            evidence: if proposal.evidence.trim().is_empty() {
                "本地校验：目标来自事实卡或待确认占位符".to_string()
            } else {
                format!("MiniMax建议；本地规则已校验：{}", proposal.evidence)
            },
            needs_review: proposal.needs_review || is_placeholder,
        });
    }

    (corrected_text, accepted)
}

fn extract_json_object(text: &str) -> &str {
    let trimmed = text.trim();
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if start <= end => &trimmed[start..=end],
        _ => trimmed,
    }
}

fn collect_safe_fact_scalars(value: &Value, output: &mut HashSet<String>) {
    match value {
        Value::String(text) if !text.trim().is_empty() => {
            output.insert(text.trim().to_string());
        }
        Value::Number(number) => {
            output.insert(number.to_string());
        }
        Value::Array(items) => {
            for item in items {
                collect_safe_fact_scalars(item, output);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                if !is_high_risk_fact_key(key) {
                    collect_safe_fact_scalars(item, output);
                }
            }
        }
        _ => {}
    }
}

fn is_high_risk_fact_key(key: &str) -> bool {
    let lowered = key.to_lowercase();
    [
        "price",
        "link",
        "inventory",
        "stock",
        "gift",
        "benefit",
        "order",
        "价格",
        "链接",
        "库存",
        "存货",
        "赠品",
        "福利",
        "订单",
        "成交",
    ]
    .iter()
    .any(|word| lowered.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(source: &str, corrected: &str) -> ProposedCorrection {
        ProposedCorrection {
            source: source.to_string(),
            corrected: corrected.to_string(),
            change_type: "测试".to_string(),
            evidence: "测试事实卡".to_string(),
            needs_review: false,
        }
    }

    #[test]
    fn constrained_correction_accepts_fact_value_and_placeholder() {
        let mut allowed = HashSet::new();
        collect_safe_fact_scalars(
            &json!({"products": ["佳能R62"], "prices": [5839]}),
            &mut allowed,
        );
        let (text, changes) = apply_proposed_corrections(
            "佳能二六二优惠完价五千八百三十",
            vec![
                proposal("佳能二六二", "佳能R62"),
                proposal("五千八百三十", "[到手价待确认：识别为5830]"),
            ],
            &allowed,
        );
        assert_eq!(text, "佳能R62优惠完价[到手价待确认：识别为5830]");
        assert_eq!(changes.len(), 2);
        assert!(changes[1].needs_review);
    }

    #[test]
    fn constrained_correction_rejects_invented_fact_and_rewrite() {
        let mut allowed = HashSet::new();
        collect_safe_fact_scalars(&json!({"prices": [5839]}), &mut allowed);
        let original = "优惠完价五千八百三十，要的话直播间拍";
        let (text, changes) = apply_proposed_corrections(
            original,
            vec![
                proposal("五千八百三十", "5999"),
                proposal("要的话直播间拍", "家人们赶紧冲，这个价格全网最低"),
            ],
            &allowed,
        );
        assert_eq!(text, original);
        assert!(changes.is_empty());
    }

    #[test]
    fn constrained_correction_rejects_ambiguous_repeated_source() {
        let mut allowed = HashSet::new();
        allowed.insert("佳能R62".to_string());
        let original = "佳能二六二，后面再看佳能二六二";
        let (text, changes) =
            apply_proposed_corrections(original, vec![proposal("佳能二六二", "佳能R62")], &allowed);
        assert_eq!(text, original);
        assert!(changes.is_empty());
    }

    #[test]
    fn constrained_correction_cannot_confirm_conflicting_price_from_fact_card() {
        let mut allowed = HashSet::new();
        collect_safe_fact_scalars(
            &json!({"products": ["佳能R62"], "prices": [5839], "link_numbers": [56]}),
            &mut allowed,
        );
        assert!(allowed.contains("佳能R62"));
        assert!(!allowed.contains("5839"));
        assert!(!allowed.contains("56"));
        let original = "识别到优惠完价5830";
        let (text, changes) =
            apply_proposed_corrections(original, vec![proposal("5830", "5839")], &allowed);
        assert_eq!(text, original);
        assert!(changes.is_empty());
    }
}
