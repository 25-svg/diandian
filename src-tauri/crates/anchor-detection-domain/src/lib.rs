use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const ANCHOR_DETECTION_SYSTEM_PROMPT: &str = r#"你是直播画面文字读取器。
任务：逐张读取画面上明确可见、格式类似“主播 XX”的固定字幕牌。
严格规则：
1. 只读取画面文字，不得根据人脸、声音、服装、场景或历史信息猜测主播身份。
2. 不得把商品名、直播间标题、用户名、评论或水印当作主播姓名。
3. 看不清、没有标签或不确定时，anchor_name 必须返回空字符串，visible 返回 false。
4. 每张图片独立判断，不得用上一张图片的结果补全下一张。
5. 只输出 JSON，不得输出解释、Markdown 或其他文字。
输出格式：
{"frames":[{"frame_index":0,"anchor_name":"小鱼","label_text":"主播 小鱼","visible":true}]}"#;

pub fn build_minimax_anchor_request(image_base64: &[String]) -> Value {
    let mut content = vec![json!({
        "type": "text",
        "text": "请按图片顺序逐帧读取主播姓名标签，并严格返回指定 JSON。"
    })];
    content.extend(image_base64.iter().map(|data| {
        json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/jpeg",
                "data": data
            }
        })
    }));

    json!({
        "model": "MiniMax-M3",
        "max_tokens": 800,
        "system": ANCHOR_DETECTION_SYSTEM_PROMPT,
        "messages": [{
            "role": "user",
            "content": content
        }]
    })
}

pub fn anchor_frame_timestamps(duration_seconds: f64) -> Vec<f64> {
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return Vec::new();
    }
    let ratios = if duration_seconds > 600.0 {
        [0.1, 0.5, 0.9]
    } else {
        [0.2, 0.5, 0.8]
    };
    ratios
        .into_iter()
        .map(|ratio| duration_seconds * ratio)
        .collect()
}

pub fn live_anchor_retry_delay_seconds(attempt: usize) -> Option<u64> {
    Some(if attempt == 0 { 0 } else { 30 })
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct FrameAnchorReading {
    pub frame_index: usize,
    #[serde(default)]
    pub anchor_name: String,
    #[serde(default)]
    pub label_text: String,
    #[serde(default)]
    pub visible: bool,
}

#[derive(Debug, Deserialize)]
struct AnchorResponse {
    #[serde(default)]
    frames: Vec<FrameAnchorReading>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorDetectionStatus {
    Confirmed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorDetectionDecision {
    pub anchor_name: String,
    pub confidence: String,
    pub status: AnchorDetectionStatus,
    pub error: String,
}

pub fn parse_anchor_response(response: &str) -> Result<Vec<FrameAnchorReading>, String> {
    let json = extract_json_object(response);
    let mut payload: AnchorResponse =
        serde_json::from_str(json).map_err(|error| format!("主播识别结果格式错误：{error}"))?;

    for reading in &mut payload.frames {
        reading.anchor_name = normalize_anchor_name(&reading.anchor_name);
        if reading.anchor_name.is_empty() {
            reading.visible = false;
        }
    }

    Ok(payload.frames)
}

pub fn resolve_anchor_consensus(readings: &[FrameAnchorReading]) -> AnchorDetectionDecision {
    let visible_names = readings
        .iter()
        .filter(|reading| reading.visible)
        .map(|reading| normalize_anchor_name(&reading.anchor_name))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();

    if visible_names.is_empty() {
        return failed("画面中未识别到明确的“主播姓名”标签");
    }

    let mut counts = HashMap::new();
    for name in visible_names {
        *counts.entry(name).or_insert(0_usize) += 1;
    }

    if counts.len() > 1 {
        return failed("不同画面识别出的主播姓名不一致，请人工确认");
    }

    let (anchor_name, count) = counts.into_iter().next().expect("name count is not empty");
    if count < 2 {
        return failed("只有一个画面读到主播姓名，证据不足，请人工确认");
    }

    AnchorDetectionDecision {
        anchor_name,
        confidence: "high".to_string(),
        status: AnchorDetectionStatus::Confirmed,
        error: String::new(),
    }
}

pub fn validate_manual_anchor_name(value: &str) -> Result<String, String> {
    let name = normalize_anchor_name(value);
    if name.is_empty() {
        Err("主播姓名应为 1 至 12 个字符，不能包含标点或换行".to_string())
    } else {
        Ok(name)
    }
}

pub fn sanitize_local_playlist(source: &str) -> String {
    let mut output = String::new();
    for line in source.lines() {
        if line.starts_with('#') {
            output.push_str(line);
        } else {
            output.push_str(line.split('?').next().unwrap_or(line));
        }
        output.push('\n');
    }
    if !source.lines().any(|line| line.trim() == "#EXT-X-ENDLIST") {
        output.push_str("#EXT-X-ENDLIST\n");
    }
    output
}

pub fn select_available_anchor_segments<F>(source: &str, mut is_available: F) -> Vec<String>
where
    F: FnMut(&str) -> bool,
{
    let available = source
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| line.split('?').next().unwrap_or(line).trim())
        .filter(|line| is_available(line))
        .map(str::to_string)
        .collect::<Vec<_>>();

    if available.len() < 3 {
        return Vec::new();
    }

    [0.2_f64, 0.5, 0.8]
        .into_iter()
        .map(|ratio| {
            let index = ((available.len() - 1) as f64 * ratio).round() as usize;
            available[index].clone()
        })
        .collect()
}

fn normalize_anchor_name(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_start_matches("主播")
        .trim_start_matches([':', '：'])
        .trim();
    let char_count = trimmed.chars().count();
    let contains_invalid_character = trimmed.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '{' | '}' | '[' | ']' | ',' | '，' | '。' | ':' | '：' | '\n' | '\r'
            )
    });

    if !(1..=12).contains(&char_count) || contains_invalid_character {
        String::new()
    } else {
        trimmed.to_string()
    }
}

fn extract_json_object(text: &str) -> &str {
    let trimmed = text.trim();
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if start <= end => &trimmed[start..=end],
        _ => trimmed,
    }
}

fn failed(error: &str) -> AnchorDetectionDecision {
    AnchorDetectionDecision {
        anchor_name: String::new(),
        confidence: String::new(),
        status: AnchorDetectionStatus::Failed,
        error: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(frame_index: usize, anchor_name: &str, visible: bool) -> FrameAnchorReading {
        FrameAnchorReading {
            frame_index,
            anchor_name: anchor_name.to_string(),
            label_text: if visible {
                format!("主播 {anchor_name}")
            } else {
                String::new()
            },
            visible,
        }
    }

    #[test]
    fn confirms_only_when_two_frames_show_the_same_anchor() {
        let decision = resolve_anchor_consensus(&[
            reading(0, "小鱼", true),
            reading(1, "小鱼", true),
            reading(2, "", false),
        ]);

        assert_eq!(decision.status, AnchorDetectionStatus::Confirmed);
        assert_eq!(decision.anchor_name, "小鱼");
        assert_eq!(decision.confidence, "high");
    }

    #[test]
    fn rejects_a_single_visible_frame() {
        let decision = resolve_anchor_consensus(&[
            reading(0, "小鱼", true),
            reading(1, "", false),
            reading(2, "", false),
        ]);

        assert_eq!(decision.status, AnchorDetectionStatus::Failed);
        assert!(decision.anchor_name.is_empty());
    }

    #[test]
    fn rejects_conflicting_names_instead_of_guessing() {
        let decision = resolve_anchor_consensus(&[
            reading(0, "小鱼", true),
            reading(1, "小雨", true),
            reading(2, "", false),
        ]);

        assert_eq!(decision.status, AnchorDetectionStatus::Failed);
        assert!(decision.anchor_name.is_empty());
        assert!(decision.error.contains("不一致"));
    }

    #[test]
    fn parses_json_wrapped_in_a_markdown_code_fence() {
        let response = r#"```json
        {"frames":[
          {"frame_index":0,"anchor_name":"小鱼","label_text":"主播 小鱼","visible":true},
          {"frame_index":1,"anchor_name":"","label_text":"","visible":false}
        ]}
        ```"#;

        let readings = parse_anchor_response(response).unwrap();

        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].anchor_name, "小鱼");
    }

    #[test]
    fn rejects_names_that_cannot_be_a_visible_name_label() {
        let response = r#"{"frames":[
          {"frame_index":0,"anchor_name":"这是一个明显超过主播姓名合理长度的模型解释文本","label_text":"主播","visible":true}
        ]}"#;

        let readings = parse_anchor_response(response).unwrap();

        assert!(readings[0].anchor_name.is_empty());
        assert!(!readings[0].visible);
    }

    #[test]
    fn minimax_request_contains_three_images_and_forbids_identity_guessing() {
        let request = build_minimax_anchor_request(&[
            "frame-a".to_string(),
            "frame-b".to_string(),
            "frame-c".to_string(),
        ]);

        assert_eq!(request["model"], "MiniMax-M3");
        let content = request["messages"][0]["content"].as_array().unwrap();
        assert_eq!(
            content
                .iter()
                .filter(|block| block["type"] == "image")
                .count(),
            3
        );
        assert!(request["system"].as_str().unwrap().contains("不得根据人脸"));
        assert_eq!(
            content[1]["source"]["media_type"],
            serde_json::Value::String("image/jpeg".to_string())
        );
    }

    #[test]
    fn frame_timestamps_are_spread_across_the_video() {
        assert_eq!(
            anchor_frame_timestamps(7_200.0),
            vec![720.0, 3_600.0, 6_480.0]
        );
        assert_eq!(anchor_frame_timestamps(60.0), vec![12.0, 30.0, 48.0]);
    }

    #[test]
    fn live_anchor_detection_starts_immediately_and_retries_every_thirty_seconds() {
        assert_eq!(live_anchor_retry_delay_seconds(0), Some(0));
        assert_eq!(live_anchor_retry_delay_seconds(1), Some(30));
        assert_eq!(live_anchor_retry_delay_seconds(5), Some(30));
        assert_eq!(live_anchor_retry_delay_seconds(6), Some(30));
        assert_eq!(live_anchor_retry_delay_seconds(120), Some(30));
    }

    #[test]
    fn manual_anchor_name_uses_the_same_name_safety_rules() {
        assert_eq!(validate_manual_anchor_name("  小鱼  ").unwrap(), "小鱼");
        assert!(validate_manual_anchor_name("这是一个明显过长且不可能是姓名的说明文字").is_err());
    }

    #[test]
    fn local_playlist_removes_download_queries_and_closes_the_stream() {
        let source = "#EXTM3U\n#EXTINF:4.0,\nsegment.ts?token=abc\n";
        assert_eq!(
            sanitize_local_playlist(source),
            "#EXTM3U\n#EXTINF:4.0,\nsegment.ts\n#EXT-X-ENDLIST\n"
        );
    }

    #[test]
    fn anchor_segments_are_selected_only_from_available_local_media() {
        let source = "#EXTM3U\nold.ts?token=x\none.ts?token=x\ntwo.ts\nthree.ts\nfour.ts\nfive.ts\nmissing.ts\n";
        let selected = select_available_anchor_segments(source, |name| {
            matches!(
                name,
                "one.ts" | "two.ts" | "three.ts" | "four.ts" | "five.ts"
            )
        });
        assert_eq!(selected, vec!["two.ts", "three.ts", "four.ts"]);
    }
}
