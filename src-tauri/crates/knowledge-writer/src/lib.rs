use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WriteReceipt {
    pub relative_path: String,
    pub byte_len: u64,
    pub sha256: String,
}

pub async fn write_verified_markdown(
    vault: &Path,
    relative_path: &Path,
    expected_absent: bool,
    content: &str,
) -> Result<WriteReceipt, String> {
    let destination = checked_child(vault, relative_path)?;
    if expected_absent && destination.exists() {
        return Err("目标母稿版本已经存在".into());
    }

    let parent = destination.parent().ok_or("目标路径无效")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| error.to_string())?;
    let canonical_parent = parent.canonicalize().map_err(|error| error.to_string())?;
    let canonical_vault = vault.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_parent.starts_with(&canonical_vault) {
        return Err("目标路径超出知识库".into());
    }

    let temporary = canonical_parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    if let Err(error) = tokio::fs::write(&temporary, content.as_bytes()).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.to_string());
    }
    if expected_absent && destination.exists() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err("目标母稿版本已经存在".into());
    }
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.to_string());
    }

    let read_back = match tokio::fs::read(&destination).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error.to_string());
        }
    };
    if read_back != content.as_bytes() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err("母稿写入校验失败".into());
    }

    Ok(WriteReceipt {
        relative_path: normalized_relative_path(relative_path),
        byte_len: read_back.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&read_back)),
    })
}

fn checked_child(vault: &Path, relative_path: &Path) -> Result<PathBuf, String> {
    if !vault.is_dir() || relative_path.as_os_str().is_empty() || relative_path.is_absolute() {
        return Err("目标路径无效".into());
    }
    let root = vault.canonicalize().map_err(|error| error.to_string())?;
    let mut candidate = root.clone();
    for component in relative_path.components() {
        let Component::Normal(part) = component else {
            return Err("目标路径超出知识库".into());
        };
        candidate.push(part);
        if let Ok(metadata) = std::fs::symlink_metadata(&candidate) {
            if metadata.file_type().is_symlink() {
                return Err("目标路径包含符号链接".into());
            }
            let canonical = candidate
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !canonical.starts_with(&root) {
                return Err("目标路径超出知识库".into());
            }
        }
    }
    if !candidate.starts_with(&root) {
        return Err("目标路径超出知识库".into());
    }
    Ok(candidate)
}

fn normalized_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
