use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
    time::sleep,
};

use super::{GenerateResult, SubtitleGeneratorType};

const ENDPOINT: &str = "http://127.0.0.1:18765";
const DEFAULT_HOTWORDS: &str = "小白兔 佳能小白兔 七零二百 70-200 R62 RF24-240 24-240 99新 在仓现货 前盖 后盖 遮光罩 脚架环 UV镜 小黄车 56号链接 优惠完价 5839";
static SERVICE_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

#[derive(Debug, Serialize)]
struct TranscribeRequest {
    audio_path: String,
    hotwords: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fact_card: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorrectionChange {
    #[serde(rename = "原文")]
    pub source: String,
    #[serde(rename = "校对稿")]
    pub corrected: String,
    #[serde(rename = "修改类型")]
    pub change_type: String,
    #[serde(rename = "依据")]
    pub evidence: String,
    #[serde(rename = "是否需要人工确认")]
    pub needs_review: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunAsrSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub recognized: String,
    pub context: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunAsrResponse {
    pub engine: String,
    pub raw_text: String,
    pub corrected_text: String,
    pub changes: Vec<CorrectionChange>,
    #[serde(default)]
    pub raw_segments: Vec<FunAsrSegment>,
    pub segments: Vec<FunAsrSegment>,
    #[serde(default)]
    pub review_items: Vec<ReviewItem>,
    pub model_load_seconds: f64,
    pub inference_seconds: f64,
}

pub fn segments_to_srt(segments: &[FunAsrSegment]) -> String {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let item = srtparse::Item {
                pos: index + 1,
                start_time: milliseconds_to_time(segment.start_ms),
                end_time: milliseconds_to_time(segment.end_ms),
                text: segment.text.clone(),
            };
            format!(
                "{}\n{:02}:{:02}:{:02},{:03} --> {:02}:{:02}:{:02},{:03}\n{}\n\n",
                item.pos,
                item.start_time.hours,
                item.start_time.minutes,
                item.start_time.seconds,
                item.start_time.milliseconds,
                item.end_time.hours,
                item.end_time.minutes,
                item.end_time.seconds,
                item.end_time.milliseconds,
                item.text
            )
        })
        .collect()
}

#[derive(Clone)]
pub struct FunAsr {
    client: Client,
}

impl FunAsr {
    pub async fn new() -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30 * 60))
            .build()
            .map_err(|e| format!("Failed to create FunASR HTTP client: {e}"))?;
        ensure_service(&client).await?;
        Ok(Self { client })
    }

    pub async fn transcribe(
        &self,
        audio_path: &Path,
        fact_card: Option<Value>,
    ) -> Result<FunAsrResponse, String> {
        self.transcribe_with_hotwords(audio_path, fact_card, DEFAULT_HOTWORDS)
            .await
    }

    pub async fn transcribe_with_hotwords(
        &self,
        audio_path: &Path,
        fact_card: Option<Value>,
        hotwords: &str,
    ) -> Result<FunAsrResponse, String> {
        let absolute = std::fs::canonicalize(audio_path)
            .map_err(|e| format!("Failed to resolve FunASR audio path: {e}"))?;
        let response = self
            .client
            .post(format!("{ENDPOINT}/transcribe"))
            .json(&TranscribeRequest {
                audio_path: absolute.to_string_lossy().to_string(),
                hotwords: hotwords.trim().to_string(),
                fact_card,
            })
            .send()
            .await
            .map_err(|e| format!("FunASR request failed: {e}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read FunASR response: {e}"))?;
        if !status.is_success() {
            return Err(format!("FunASR service returned {status}: {body}"));
        }
        serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse FunASR response: {e}; body={body}"))
    }
}

pub fn into_generate_result(response: &FunAsrResponse) -> GenerateResult {
    let subtitle_content = response
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| srtparse::Item {
            pos: index + 1,
            start_time: milliseconds_to_time(segment.start_ms),
            end_time: milliseconds_to_time(segment.end_ms),
            text: segment.text.clone(),
        })
        .collect();
    GenerateResult {
        generator_type: SubtitleGeneratorType::FunAsr,
        subtitle_id: String::new(),
        subtitle_content,
    }
}

fn milliseconds_to_time(total_ms: u64) -> srtparse::Time {
    let total_seconds = total_ms / 1000;
    srtparse::Time {
        hours: total_seconds / 3600,
        minutes: (total_seconds % 3600) / 60,
        seconds: total_seconds % 60,
        milliseconds: total_ms % 1000,
    }
}

async fn service_healthy(client: &Client) -> bool {
    client
        .get(format!("{ENDPOINT}/health"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn ensure_service(client: &Client) -> Result<(), String> {
    if service_healthy(client).await {
        return Ok(());
    }
    let mutex = SERVICE_CHILD.get_or_init(|| Mutex::new(None));
    let mut child_guard = mutex.lock().await;
    if service_healthy(client).await {
        return Ok(());
    }
    if let Some(child) = child_guard.as_mut() {
        if child.try_wait().ok().flatten().is_none() {
            return wait_until_ready(client, Some(child)).await;
        }
    }

    let (program, args, working_dir) = find_service_command()?;
    let log_path = service_log_path();
    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .map_err(|e| {
            format!(
                "Failed to open FunASR startup log {}: {e}",
                log_path.display()
            )
        })?;
    let log_stdout = log_file
        .try_clone()
        .map_err(|e| format!("Failed to clone FunASR startup log handle: {e}"))?;
    let mut command = Command::new(program);
    configure_bundled_environment(&mut command, &working_dir);
    command
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_stdout))
        .stderr(Stdio::from(log_file));
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    *child_guard = Some(
        command
            .spawn()
            .map_err(|e| format!("Failed to start FunASR service: {e}"))?,
    );
    wait_until_ready(client, child_guard.as_mut()).await
}

fn configure_bundled_environment(command: &mut Command, runtime_dir: &Path) {
    let model_root = runtime_dir.join("models");
    let asr_model = model_root
        .join("iic--speech_seaco_paraformer_large_asr_nat-zh-cn-16k-common-vocab8404-pytorch");
    let vad_model = model_root.join("iic--speech_fsmn_vad_zh-cn-16k-common-pytorch");
    if asr_model.is_dir() {
        command.env("BSR_FUNASR_ASR_MODEL", asr_model);
    }
    if vad_model.is_dir() {
        command.env("BSR_FUNASR_VAD_MODEL", vad_model);
    }

    // The offline package keeps ffmpeg.exe beside the desktop executable and
    // FunASR under app/funasr-runtime. Make the bundled media tools visible to
    // Python without requiring a system PATH change on the target computer.
    if let Some(app_dir) = runtime_dir.parent() {
        let mut paths = vec![app_dir.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(value) = std::env::join_paths(paths) {
            command.env("PATH", value);
        }
    }
}

async fn wait_until_ready(client: &Client, mut child: Option<&mut Child>) -> Result<(), String> {
    // A cold Windows start may spend several minutes importing PyTorch while
    // antivirus scans the environment. Keep the UI task alive and report the
    // actual startup failure instead of falling back during a healthy load.
    for _ in 0..300 {
        if service_healthy(client).await {
            return Ok(());
        }
        if let Some(process) = child.as_mut() {
            if let Ok(Some(status)) = process.try_wait() {
                return Err(format!(
                    "FunASR service exited before becoming ready ({status}). {}",
                    read_service_log_tail()
                ));
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
    Err(format!(
        "FunASR service did not become ready within 300 seconds. {}",
        read_service_log_tail()
    ))
}

fn service_log_path() -> PathBuf {
    std::env::temp_dir().join("bili-shadowreplay-funasr.log")
}

fn read_service_log_tail() -> String {
    let path = service_log_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return format!("Startup log unavailable: {}", path.display());
    };
    let tail = content
        .chars()
        .rev()
        .take(4000)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("Startup log {}: {}", path.display(), tail.trim())
}

fn find_service_command() -> Result<(PathBuf, Vec<String>, PathBuf), String> {
    let roots = candidate_roots();
    for root in &roots {
        for runtime in [
            root.clone(),
            root.join("funasr-runtime"),
            root.join("resources").join("funasr-runtime"),
        ] {
            let sidecar = runtime.join("funasr-service.exe");
            if sidecar.is_file() {
                return Ok((sidecar, vec!["--port".into(), "18765".into()], runtime));
            }
        }
    }
    for root in &roots {
        let python = root.join(".funasr-venv").join("Scripts").join("python.exe");
        let script = root.join("asr-benchmarks").join("funasr_service.py");
        if python.is_file() && script.is_file() {
            return Ok((
                python,
                vec![
                    script.to_string_lossy().to_string(),
                    "--port".into(),
                    "18765".into(),
                ],
                root.clone(),
            ));
        }
    }
    Err(
        "FunASR runtime not found. Expected funasr-service.exe or the development .funasr-venv"
            .to_string(),
    )
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(root) = std::env::var("BSR_FUNASR_ROOT") {
        roots.push(PathBuf::from(root));
    }
    if let Ok(current) = std::env::current_dir() {
        roots.extend(current.ancestors().map(Path::to_path_buf));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            roots.extend(parent.ancestors().map(Path::to_path_buf));
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_milliseconds_to_srt_time() {
        let value = milliseconds_to_time(3_661_234);
        assert_eq!(value.hours, 1);
        assert_eq!(value.minutes, 1);
        assert_eq!(value.seconds, 1);
        assert_eq!(value.milliseconds, 234);
    }
}
