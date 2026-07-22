use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

const DEFAULT_MODEL_NAME: &str = "ggml-small-q5_1.bin";
const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin";
const MIN_MODEL_SIZE: u64 = 180_000_000;

pub fn managed_model_path(config_path: &str) -> Result<PathBuf, String> {
    let config_dir = Path::new(config_path)
        .parent()
        .ok_or_else(|| "无法确定应用配置目录".to_string())?;
    Ok(config_dir
        .join("models")
        .join("whisper")
        .join(DEFAULT_MODEL_NAME))
}

pub async fn ensure_model(config_path: &str, configured_path: &str) -> Result<PathBuf, String> {
    let configured = PathBuf::from(configured_path);
    if configured.is_file() {
        return Ok(configured);
    }

    let target = managed_model_path(config_path)?;
    if target
        .metadata()
        .map(|metadata| metadata.len() >= MIN_MODEL_SIZE)
        .unwrap_or(false)
    {
        return Ok(target);
    }

    tokio::task::spawn_blocking(move || download_model(&target))
        .await
        .map_err(|error| format!("Whisper 模型准备任务异常: {error}"))?
}

fn download_model(target: &Path) -> Result<PathBuf, String> {
    let parent = target
        .parent()
        .ok_or_else(|| "无法确定 Whisper 模型目录".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("创建 Whisper 模型目录失败: {error}"))?;

    let partial = target.with_extension("bin.part");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 30))
        .build()
        .map_err(|error| format!("创建模型下载连接失败: {error}"))?;
    let mut response = client
        .get(DEFAULT_MODEL_URL)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("下载 Whisper 模型失败: {error}"))?;

    let mut file = File::create(&partial)
        .map_err(|error| format!("创建 Whisper 临时模型文件失败: {error}"))?;
    std::io::copy(&mut response, &mut file)
        .map_err(|error| format!("保存 Whisper 模型失败: {error}"))?;
    file.flush()
        .map_err(|error| format!("写入 Whisper 模型失败: {error}"))?;

    let size = partial
        .metadata()
        .map_err(|error| format!("读取 Whisper 模型大小失败: {error}"))?
        .len();
    if size < MIN_MODEL_SIZE {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("Whisper 模型文件不完整，当前仅 {size} 字节"));
    }

    std::fs::rename(&partial, target).map_err(|error| format!("安装 Whisper 模型失败: {error}"))?;
    Ok(target.to_path_buf())
}
