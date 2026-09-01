use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
    time::sleep,
};

use super::{GenerateResult, SubtitleGeneratorType};

const BASE_PORT: u16 = 18765;
/// Default parallel FunASR processes. Each loads a full model (~GB RAM).
const DEFAULT_WORKERS: usize = 2;
const MAX_WORKERS: usize = 4;
const DEFAULT_HOTWORDS: &str = "小白兔 佳能小白兔 七零二百 70-200 R62 RF24-240 24-240 99新 在仓现货 前盖 后盖 遮光罩 脚架环 UV镜 小黄车 56号链接 优惠完价 5839";

static SERVICE_POOL: OnceLock<Mutex<ServicePool>> = OnceLock::new();
static RR_COUNTER: AtomicUsize = AtomicUsize::new(0);

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

struct WorkerSlot {
    port: u16,
    child: Option<Child>,
}

struct ServicePool {
    workers: Vec<WorkerSlot>,
}

/// How many FunASR OS processes to keep ready for parallel ASR
/// (deal windows and full-session chunked transcription).
pub fn worker_count() -> usize {
    if let Ok(raw) = std::env::var("BSR_FUNASR_WORKERS") {
        if let Ok(parsed) = raw.trim().parse::<usize>() {
            return parsed.clamp(1, MAX_WORKERS);
        }
    }
    DEFAULT_WORKERS.clamp(1, MAX_WORKERS)
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
        ensure_service_pool(&client).await?;
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
        let endpoint = pick_endpoint(&self.client).await?;
        let response = self
            .client
            .post(format!("{endpoint}/transcribe"))
            .json(&TranscribeRequest {
                audio_path: absolute.to_string_lossy().to_string(),
                hotwords: hotwords.trim().to_string(),
                fact_card,
            })
            .send()
            .await
            .map_err(|e| format!("FunASR request failed ({endpoint}): {e}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read FunASR response: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "FunASR service returned {status} from {endpoint}: {body}"
            ));
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

fn endpoint_for_port(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

async fn service_healthy(client: &Client, port: u16) -> bool {
    client
        .get(format!("{}/health", endpoint_for_port(port)))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn pick_endpoint(client: &Client) -> Result<String, String> {
    let count = worker_count();
    let start = RR_COUNTER.fetch_add(1, Ordering::Relaxed);
    for offset in 0..count {
        let port = BASE_PORT + ((start + offset) % count) as u16;
        if service_healthy(client, port).await {
            return Ok(endpoint_for_port(port));
        }
    }
    // Last resort: try to (re)start the pool, then pick again.
    ensure_service_pool(client).await?;
    let start = RR_COUNTER.fetch_add(1, Ordering::Relaxed);
    for offset in 0..count {
        let port = BASE_PORT + ((start + offset) % count) as u16;
        if service_healthy(client, port).await {
            return Ok(endpoint_for_port(port));
        }
    }
    Err(format!(
        "No healthy FunASR worker among {} process(es). {}",
        count,
        read_service_log_tail(BASE_PORT)
    ))
}

async fn ensure_service_pool(client: &Client) -> Result<(), String> {
    let count = worker_count();
    let mutex = SERVICE_POOL.get_or_init(|| {
        Mutex::new(ServicePool {
            workers: (0..count)
                .map(|index| WorkerSlot {
                    port: BASE_PORT + index as u16,
                    child: None,
                })
                .collect(),
        })
    });
    let mut pool = mutex.lock().await;

    // Resize pool if env changed after first init (rare).
    if pool.workers.len() != count {
        for worker in pool.workers.iter_mut() {
            if let Some(mut child) = worker.child.take() {
                let _ = child.start_kill();
            }
        }
        pool.workers = (0..count)
            .map(|index| WorkerSlot {
                port: BASE_PORT + index as u16,
                child: None,
            })
            .collect();
    }

    let mut pending = Vec::new();
    for worker in pool.workers.iter_mut() {
        if service_healthy(client, worker.port).await {
            continue;
        }
        if let Some(child) = worker.child.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                pending.push(worker.port);
                continue;
            }
        }
        worker.child = Some(spawn_worker(worker.port, count)?);
        pending.push(worker.port);
    }

    for port in pending {
        let child = pool
            .workers
            .iter_mut()
            .find(|worker| worker.port == port)
            .and_then(|worker| worker.child.as_mut());
        wait_until_ready(client, port, child).await?;
    }
    Ok(())
}

fn spawn_worker(port: u16, total_workers: usize) -> Result<Child, String> {
    let (program, mut args, working_dir) = find_service_command(port)?;
    // Replace trailing port arg if find_service_command already set one.
    if let Some(pos) = args.iter().position(|arg| arg == "--port") {
        if let Some(value) = args.get_mut(pos + 1) {
            *value = port.to_string();
        }
    } else {
        args.push("--port".into());
        args.push(port.to_string());
    }

    let log_path = service_log_path(port);
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
    configure_bundled_environment(&mut command, &working_dir, total_workers);
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
    command
        .spawn()
        .map_err(|e| format!("Failed to start FunASR service on port {port}: {e}"))
}

fn configure_bundled_environment(command: &mut Command, runtime_dir: &Path, total_workers: usize) {
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
    // Avoid oversubscribing CPU when several model processes share one machine.
    let threads = (4 / total_workers.max(1)).max(1);
    command.env("BSR_FUNASR_TORCH_THREADS", threads.to_string());

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

async fn wait_until_ready(
    client: &Client,
    port: u16,
    mut child: Option<&mut Child>,
) -> Result<(), String> {
    for _ in 0..300 {
        if service_healthy(client, port).await {
            return Ok(());
        }
        if let Some(process) = child.as_mut() {
            if let Ok(Some(status)) = process.try_wait() {
                return Err(format!(
                    "FunASR service on port {port} exited before becoming ready ({status}). {}",
                    read_service_log_tail(port)
                ));
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
    Err(format!(
        "FunASR service on port {port} did not become ready within 300 seconds. {}",
        read_service_log_tail(port)
    ))
}

fn service_log_path(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("bili-shadowreplay-funasr-{port}.log"))
}

fn read_service_log_tail(port: u16) -> String {
    let path = service_log_path(port);
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

fn find_service_command(port: u16) -> Result<(PathBuf, Vec<String>, PathBuf), String> {
    let roots = candidate_roots();
    for root in &roots {
        for runtime in [
            root.clone(),
            root.join("funasr-runtime"),
            root.join("resources").join("funasr-runtime"),
        ] {
            let sidecar = runtime.join("funasr-service.exe");
            if sidecar.is_file() {
                return Ok((sidecar, vec!["--port".into(), port.to_string()], runtime));
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
                    port.to_string(),
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

pub fn inspect_runtime() -> Result<String, String> {
    let (command, _, _) = find_service_command(BASE_PORT)?;
    Ok(command.to_string_lossy().to_string())
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

    #[test]
    fn worker_count_clamps_env() {
        let previous = std::env::var("BSR_FUNASR_WORKERS").ok();
        std::env::set_var("BSR_FUNASR_WORKERS", "9");
        assert_eq!(worker_count(), MAX_WORKERS);
        std::env::set_var("BSR_FUNASR_WORKERS", "0");
        assert_eq!(worker_count(), 1);
        match previous {
            Some(value) => std::env::set_var("BSR_FUNASR_WORKERS", value),
            None => std::env::remove_var("BSR_FUNASR_WORKERS"),
        }
    }
}
