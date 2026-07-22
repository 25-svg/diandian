use crate::config::Config;
use crate::database::knowledge::{KnowledgeStatus, KnowledgeSyncSummary};
use crate::database::Database;
use crate::state::State;
use crate::state_type;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

fn canonical_vault(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|_| "找不到所选知识库文件夹，请重新选择".to_string())?;
    if !canonical.is_dir() {
        return Err("请选择一个存在的知识库文件夹".to_string());
    }
    Ok(canonical)
}

async fn connect_in_state(
    db: &Database,
    config: &RwLock<Config>,
    path: &Path,
) -> Result<KnowledgeSyncSummary, String> {
    let canonical = canonical_vault(path)?;
    let scan = knowledge::scan_vault(&canonical).map_err(|error| error.to_string())?;
    let canonical_text = canonical.to_string_lossy().to_string();
    let summary = db
        .sync_knowledge_vault(&canonical_text, &scan)
        .await
        .map_err(String::from)?;
    let mut writable = config.write().await;
    writable.knowledge_vault_path = canonical_text;
    writable.save();
    Ok(summary)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn inspect_knowledge_vault(
    vault_path: String,
) -> Result<knowledge::VaultInspection, String> {
    let canonical = canonical_vault(Path::new(&vault_path))?;
    knowledge::inspect_vault(&canonical).map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn connect_knowledge_vault(
    state: state_type!(),
    vault_path: String,
) -> Result<KnowledgeSyncSummary, String> {
    connect_in_state(&state.db, &state.config, Path::new(&vault_path)).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn sync_knowledge_vault(state: state_type!()) -> Result<KnowledgeSyncSummary, String> {
    let configured = state.config.read().await.knowledge_vault_path.clone();
    if configured.trim().is_empty() {
        return Err("尚未连接 Obsidian 知识库，请先在设置中选择文件夹".to_string());
    }
    let canonical = canonical_vault(Path::new(&configured))?;
    let scan = knowledge::scan_vault(&canonical).map_err(|error| error.to_string())?;
    state
        .db
        .sync_knowledge_vault(&canonical.to_string_lossy(), &scan)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_knowledge_status(state: state_type!()) -> Result<KnowledgeStatus, String> {
    let configured = state.config.read().await.knowledge_vault_path.clone();
    let stored_path = (!configured.trim().is_empty()).then_some(configured.as_str());
    let mut status = state
        .db
        .get_knowledge_status(stored_path)
        .await
        .map_err(String::from)?;
    if stored_path.is_none() || !Path::new(&configured).is_dir() {
        status.connected = false;
        status.status = "disconnected".to_string();
    } else {
        status.connected = true;
        status.vault_path = configured;
    }
    Ok(status)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn open_knowledge_vault(state: state_type!()) -> Result<(), String> {
    let configured = state.config.read().await.knowledge_vault_path.clone();
    if configured.trim().is_empty() {
        return Err("尚未连接 Obsidian 知识库，请先在设置中选择文件夹".to_string());
    }
    let canonical = canonical_vault(Path::new(&configured))?;
    open::that_detached(canonical).map_err(|error| format!("无法打开知识库文件夹：{error}"))
}
