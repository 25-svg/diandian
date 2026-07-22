use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use uuid::Uuid;

use super::{
    asr_text::normalize_commerce_text_boundaries, GenerateResult, SubtitleGeneratorType,
};

const SUBMIT_ENDPOINT: &str = "https://openspeech.bytedance.com/api/v3/auc/bigmodel/submit";
const QUERY_ENDPOINT: &str = "https://openspeech.bytedance.com/api/v3/auc/bigmodel/query";
const DEFAULT_RESOURCE_ID: &str = "volc.seedasr.auc";
const LEGACY_TURBO_RESOURCE_ID: &str = "volc.bigasr.auc_turbo";
const MAX_SEGMENT_MS: u64 = 15_000;
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_POLL_DURATION: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct VolcengineAsr {
    client: Client,
    app_id: String,
    access_token: String,
    api_key: String,
    resource_id: String,
    boosting_table_id: String,
    correct_table_id: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    result: ApiResult,
}

#[derive(Debug, Default, Deserialize)]
struct ApiResult {
    #[serde(default)]
    text: String,
    #[serde(default)]
    utterances: Vec<Utterance>,
}

#[derive(Debug, Deserialize)]
struct Utterance {
    start_time: i64,
    end_time: i64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    words: Vec<Word>,
}

#[derive(Debug, Deserialize)]
struct Word {
    start_time: i64,
    end_time: i64,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Serialize)]
struct RequestBody {
    user: User,
    audio: Audio,
    request: RequestOptions,
}

#[derive(Debug, Serialize)]
struct User {
    uid: String,
}

#[derive(Debug, Serialize)]
struct Audio {
    data: String,
    format: &'static str,
}

#[derive(Debug, Serialize)]
struct RequestOptions {
    model_name: &'static str,
    enable_itn: bool,
    enable_punc: bool,
    enable_ddc: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    corpus: Option<Corpus>,
}

#[derive(Debug, Serialize)]
struct Corpus {
    #[serde(skip_serializing_if = "Option::is_none")]
    boosting_table_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correct_table_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

impl VolcengineAsr {
    pub fn new(
        api_key: &str,
        app_id: &str,
        access_token: &str,
        resource_id: &str,
        boosting_table_id: &str,
        correct_table_id: &str,
    ) -> Result<Self, String> {
        if api_key.trim().is_empty() && (app_id.trim().is_empty() || access_token.trim().is_empty())
        {
            return Err("火山ASR尚未配置 API Key，或旧版 App ID + Access Token".to_string());
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(10 * 60))
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|error| format!("创建火山ASR客户端失败: {error}"))?;
        Ok(Self {
            client,
            app_id: app_id.trim().to_string(),
            access_token: access_token.trim().to_string(),
            api_key: api_key.trim().to_string(),
            // Existing installations used the synchronous 1.0 turbo resource.
            // Migrate that default transparently so users really call Seed ASR 2.0.
            resource_id: match resource_id.trim() {
                "" | LEGACY_TURBO_RESOURCE_ID => DEFAULT_RESOURCE_ID.to_string(),
                value => value.to_string(),
            },
            boosting_table_id: boosting_table_id.trim().to_string(),
            correct_table_id: correct_table_id.trim().to_string(),
        })
    }

    pub async fn recognize_file(
        &self,
        audio_path: &Path,
        dynamic_context: Option<&str>,
    ) -> Result<GenerateResult, String> {
        let bytes = tokio::fs::read(audio_path)
            .await
            .map_err(|error| format!("读取火山ASR音频分段失败: {error}"))?;
        if bytes.is_empty() {
            return Err("火山ASR音频分段为空".to_string());
        }
        if bytes.len() > 20 * 1024 * 1024 {
            return Err(format!(
                "火山ASR音频分段超过20MB建议上限: {:.2}MB",
                bytes.len() as f64 / 1024.0 / 1024.0
            ));
        }
        let request_id = Uuid::new_v4().to_string();
        // A hotword table created in the legacy console belongs to its App ID.
        // Prefer those credentials when both legacy values are configured.
        let use_legacy_app = !self.app_id.is_empty() && !self.access_token.is_empty();
        let hotword_enabled = !self.boosting_table_id.is_empty();
        let replacement_enabled = !self.correct_table_id.is_empty();
        let context_enabled = dynamic_context.is_some_and(|value| !value.trim().is_empty());
        log::info!(
            "Volcengine ASR request: auth_mode={}, hotword_table={}, replacement_table={}, dynamic_context={}",
            if use_legacy_app { "app_id" } else { "api_key" },
            if hotword_enabled {
                "enabled"
            } else {
                "disabled"
            },
            if replacement_enabled {
                "enabled"
            } else {
                "disabled"
            },
            if context_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        let uid = if self.app_id.is_empty() {
            "bili-shadowreplay".to_string()
        } else {
            self.app_id.clone()
        };
        let body = RequestBody {
            user: User { uid },
            audio: Audio {
                data: STANDARD.encode(bytes),
                format: "mp3",
            },
            request: RequestOptions {
                model_name: "bigmodel",
                enable_itn: true,
                enable_punc: true,
                enable_ddc: false,
                corpus: if !hotword_enabled && !replacement_enabled && !context_enabled {
                    None
                } else {
                    Some(Corpus {
                        boosting_table_id: hotword_enabled.then(|| self.boosting_table_id.clone()),
                        correct_table_id: replacement_enabled
                            .then(|| self.correct_table_id.clone()),
                        context: dynamic_context
                            .filter(|value| !value.trim().is_empty())
                            .map(str::to_string),
                    })
                },
            },
        };

        self.submit(&request_id, &body, use_legacy_app).await?;
        self.poll_result(
            &request_id,
            use_legacy_app,
            hotword_enabled,
            replacement_enabled,
        )
        .await
    }

    fn authenticated_request(
        &self,
        endpoint: &str,
        request_id: &str,
        use_legacy_app: bool,
    ) -> reqwest::RequestBuilder {
        let request = self
            .client
            .post(endpoint)
            .header("X-Api-Resource-Id", &self.resource_id)
            .header("X-Api-Request-Id", request_id)
            .header("X-Api-Sequence", "-1");
        if use_legacy_app {
            request
                .header("X-Api-App-Key", &self.app_id)
                .header("X-Api-Access-Key", &self.access_token)
        } else {
            request.header("X-Api-Key", &self.api_key)
        }
    }

    async fn submit(
        &self,
        request_id: &str,
        body: &RequestBody,
        use_legacy_app: bool,
    ) -> Result<(), String> {
        let mut last_error = String::new();
        for attempt in 0..4_u64 {
            match self
                .authenticated_request(SUBMIT_ENDPOINT, request_id, use_legacy_app)
                .json(body)
                .send()
                .await
            {
                Ok(response) => {
                    let response = read_response(response, "提交").await?;
                    if response.http_success && response.status_code == "20000000" {
                        log::info!(
                            "Volcengine Seed ASR 2.0 submitted: request_id={}, logid={}",
                            request_id,
                            response.log_id
                        );
                        return Ok(());
                    }
                    last_error = response.error_summary();
                    if !response.retryable() {
                        break;
                    }
                }
                Err(error) => last_error = error.to_string(),
            }
            if attempt < 3 {
                tokio::time::sleep(Duration::from_secs(1_u64 << attempt)).await;
            }
        }
        Err(format!(
            "火山录音文件识别2.0提交失败，已自动重试: {last_error}。请确认已开通2.0资源 volc.seedasr.auc"
        ))
    }

    async fn poll_result(
        &self,
        request_id: &str,
        use_legacy_app: bool,
        hotword_enabled: bool,
        replacement_enabled: bool,
    ) -> Result<GenerateResult, String> {
        let started = tokio::time::Instant::now();
        let mut transient_failures = 0_u8;
        loop {
            if started.elapsed() >= MAX_POLL_DURATION {
                return Err(format!(
                    "火山录音文件识别2.0等待超过{}分钟，任务ID={request_id}",
                    MAX_POLL_DURATION.as_secs() / 60
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
            let response = self
                .authenticated_request(QUERY_ENDPOINT, request_id, use_legacy_app)
                .json(&serde_json::json!({}))
                .send()
                .await;
            let response = match response {
                Ok(response) => {
                    transient_failures = 0;
                    read_response(response, "查询").await?
                }
                Err(error) => {
                    transient_failures += 1;
                    if transient_failures <= 4 {
                        log::warn!(
                            "Volcengine Seed ASR 2.0 query transient error ({}/4): {}",
                            transient_failures,
                            error
                        );
                        continue;
                    }
                    return Err(format!("火山录音文件识别2.0查询失败: {error}"));
                }
            };
            match response.status_code.as_str() {
                "20000000" if response.http_success => {
                    let payload: ApiResponse =
                        serde_json::from_str(&response.body).map_err(|error| {
                            format!(
                                "解析火山录音文件识别2.0响应失败: {error}; logid={}",
                                response.log_id
                            )
                        })?;
                    log::info!(
                        "Volcengine Seed ASR 2.0 success: request_id={}, logid={}, hotword_table={}, replacement_table={}",
                        request_id,
                        response.log_id,
                        if hotword_enabled { "enabled" } else { "disabled" },
                        if replacement_enabled { "enabled" } else { "disabled" }
                    );
                    return Ok(to_generate_result(payload.result));
                }
                // Official standard-file API status: running / queued.
                "20000001" | "20000002" => continue,
                _ if response.retryable() => continue,
                _ => {
                    return Err(format!(
                        "火山录音文件识别2.0查询失败: {}",
                        response.error_summary()
                    ));
                }
            }
        }
    }
}

struct ResponseSnapshot {
    http_status: u16,
    http_success: bool,
    status_code: String,
    message: String,
    log_id: String,
    body: String,
}

impl ResponseSnapshot {
    fn retryable(&self) -> bool {
        self.http_status >= 500 || self.status_code.starts_with("55") || self.status_code.is_empty()
    }

    fn error_summary(&self) -> String {
        format!(
            "HTTP {} / 状态码 {} / {} / logid={} / {}",
            self.http_status, self.status_code, self.message, self.log_id, self.body
        )
    }
}

async fn read_response(
    response: reqwest::Response,
    action: &str,
) -> Result<ResponseSnapshot, String> {
    let http_status = response.status();
    let status_code = response
        .headers()
        .get("X-Api-Status-Code")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let message = response
        .headers()
        .get("X-Api-Message")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let log_id = response
        .headers()
        .get("X-Tt-Logid")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取火山ASR{action}响应失败: {error}"))?;
    Ok(ResponseSnapshot {
        http_status: http_status.as_u16(),
        http_success: http_status.is_success(),
        status_code,
        message,
        log_id,
        body,
    })
}

fn to_generate_result(result: ApiResult) -> GenerateResult {
    let mut items = Vec::new();
    for utterance in result.utterances {
        let utterance_start = valid_ms(utterance.start_time).unwrap_or(0);
        let utterance_end =
            valid_ms(utterance.end_time).unwrap_or_else(|| utterance_start.saturating_add(1000));
        if utterance_end.saturating_sub(utterance_start) <= MAX_SEGMENT_MS
            || utterance.words.is_empty()
        {
            items.push((utterance_start, utterance_end, utterance.text));
            continue;
        }
        let mut start = 0;
        while start < utterance.words.len() {
            let first_ms = valid_ms(utterance.words[start].start_time).unwrap_or(utterance_start);
            let mut end = start + 1;
            while end < utterance.words.len()
                && valid_ms(utterance.words[end].end_time)
                    .unwrap_or(first_ms)
                    .saturating_sub(first_ms)
                    <= MAX_SEGMENT_MS
            {
                end += 1;
            }
            let words = &utterance.words[start..end];
            items.push((
                words
                    .first()
                    .and_then(|word| valid_ms(word.start_time))
                    .unwrap_or(first_ms),
                words
                    .last()
                    .and_then(|word| valid_ms(word.end_time))
                    .unwrap_or(first_ms + 1000),
                words
                    .iter()
                    .map(|word| word.text.as_str())
                    .collect::<String>(),
            ));
            start = end;
        }
    }
    if items.is_empty() && !result.text.trim().is_empty() {
        items.push((0, 1000, result.text));
    }
    let subtitle_content = items
        .into_iter()
        .enumerate()
        .filter(|(_, (_, end, text))| *end > 0 && !text.trim().is_empty())
        .map(|(index, (start, end, text))| srtparse::Item {
            pos: index + 1,
            start_time: milliseconds_to_time(start),
            end_time: milliseconds_to_time(end.max(start + 200)),
            text: normalize_commerce_text_boundaries(&text),
        })
        .collect();
    GenerateResult {
        generator_type: SubtitleGeneratorType::Volcengine,
        subtitle_id: String::new(),
        subtitle_content,
    }
}

fn valid_ms(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

fn milliseconds_to_time(total_ms: u64) -> srtparse::Time {
    srtparse::Time {
        hours: total_ms / 3_600_000,
        minutes: (total_ms % 3_600_000) / 60_000,
        seconds: (total_ms % 60_000) / 1000,
        milliseconds: total_ms % 1000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_condition_grade_from_alphanumeric_model() {
        assert_eq!(
            normalize_commerce_text_boundaries("A4PRO299新的，咱们说喜欢运动相机的可以看一下"),
            "A4PRO2 99新的，咱们说喜欢运动相机的可以看一下"
        );
    }

    #[test]
    fn leaves_plain_alphanumeric_model_unchanged() {
        assert_eq!(
            normalize_commerce_text_boundaries("影石A4PRO2运动相机"),
            "影石A4PRO2运动相机"
        );
    }

    #[test]
    fn serializes_learning_tables_inside_corpus() {
        let body = RequestBody {
            user: User {
                uid: "test-app".to_string(),
            },
            audio: Audio {
                data: "audio".to_string(),
                format: "mp3",
            },
            request: RequestOptions {
                model_name: "bigmodel",
                enable_itn: true,
                enable_punc: true,
                enable_ddc: false,
                corpus: Some(Corpus {
                    boosting_table_id: Some("hotword-id".to_string()),
                    correct_table_id: Some("replacement-id".to_string()),
                    context: Some("{\"context_type\":\"dialog_ctx\"}".to_string()),
                }),
            },
        };

        let json = serde_json::to_value(body).expect("request body should serialize");
        assert_eq!(json["request"]["corpus"]["boosting_table_id"], "hotword-id");
        assert_eq!(
            json["request"]["corpus"]["correct_table_id"],
            "replacement-id"
        );
        assert_eq!(
            json["request"]["corpus"]["context"],
            "{\"context_type\":\"dialog_ctx\"}"
        );
        assert!(json["request"].get("boosting_table_id").is_none());
        assert!(json["request"].get("correct_table_id").is_none());
    }

    #[test]
    fn splits_long_utterance_using_word_timestamps() {
        let words = (0..20)
            .map(|index| Word {
                start_time: index * 1000,
                end_time: (index + 1) * 1000,
                text: "词".to_string(),
            })
            .collect();
        let generated = to_generate_result(ApiResult {
            text: String::new(),
            utterances: vec![Utterance {
                start_time: 0,
                end_time: 20_000,
                text: "词".repeat(20),
                words,
            }],
        });
        assert_eq!(generated.subtitle_content.len(), 2);
        assert!(generated.subtitle_content.iter().all(|item| {
            item.end_time.into_duration().as_millis() - item.start_time.into_duration().as_millis()
                <= MAX_SEGMENT_MS as u128
        }));
    }
}
