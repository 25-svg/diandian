use crate::config::Config;
use crate::database::knowledge::{KnowledgeStatus, KnowledgeSyncSummary};
use crate::database::Database;
use crate::state::State;
use crate::state_type;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

const PRODUCT_DICTIONARY_MAX_CARDS: usize = 32;
const PRODUCT_DICTIONARY_MAX_CHARS: usize = 16_000;

fn product_dictionary_text(
    cards: &[crate::database::knowledge::KnowledgeDocumentRecord],
) -> String {
    let mut output = String::new();
    for card in cards.iter().take(PRODUCT_DICTIONARY_MAX_CARDS) {
        let entry = format!(
            "- 编号：{}；名称：{}\n{}\n",
            card.card_id,
            card.title,
            card.body.trim()
        );
        let remaining = PRODUCT_DICTIONARY_MAX_CHARS.saturating_sub(output.chars().count());
        if remaining == 0 {
            break;
        }
        output.extend(entry.chars().take(remaining));
    }
    output.trim().to_string()
}

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
pub async fn get_enterprise_product_dictionary(state: state_type!()) -> Result<String, String> {
    let cards = state
        .db
        .list_asr_parameter_cards()
        .await
        .map_err(String::from)?;
    Ok(product_dictionary_text(&cards))
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

#[cfg(test)]
mod knowledge_handler_tests {
    use super::*;
    use crate::database::knowledge::{
        KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL, KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL,
        KNOWLEDGE_MIGRATION_SQL,
    };
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    fn product_card(
        id: &str,
        title: &str,
        body: &str,
    ) -> crate::database::knowledge::KnowledgeDocumentRecord {
        crate::database::knowledge::KnowledgeDocumentRecord {
            card_id: id.to_string(),
            title: title.to_string(),
            card_type: "product_fact".to_string(),
            version: "1".to_string(),
            relative_path: format!("{id}.md"),
            metadata_json: "{}".to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn product_dictionary_contains_verified_card_identity_and_body() {
        let text = product_dictionary_text(&[
            product_card("CANON-R7", "佳能 R7", "别名：R七\n类型：APS-C 相机"),
            product_card("CANON-R52", "佳能 R5 Mark II", "别名：R五二、R52"),
        ]);

        assert!(text.contains("CANON-R7"));
        assert!(text.contains("佳能 R7"));
        assert!(text.contains("R五二、R52"));
    }

    async fn state() -> (tempfile::TempDir, Database, RwLock<Config>) {
        let root = tempfile::tempdir().unwrap();
        let config_path = root.path().join("Conf.toml");
        let cache = root.path().join("cache");
        let output = root.path().join("output");
        let config = Config::load(&config_path, &cache, &output).unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL)
            .await
            .unwrap();
        let db = Database::new();
        db.set(pool).await;
        (root, db, RwLock::new(config))
    }

    #[tokio::test]
    async fn connect_saves_path_only_after_successful_sync() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".obsidian")).unwrap();
        std::fs::write(
            root.path().join("PF.md"),
            "---\nid: PF-1\ntitle: Product\ntype: product_fact\nstatus: approved\nversion: 1\n---\nBody",
        )
        .unwrap();
        let (_state_root, db, config) = state().await;

        let result = connect_in_state(&db, &config, root.path()).await.unwrap();

        assert_eq!(result.inserted, 1);
        assert_eq!(
            config.read().await.knowledge_vault_path,
            root.path().canonicalize().unwrap().to_string_lossy()
        );
    }

    #[tokio::test]
    async fn failed_connect_does_not_replace_previous_path() {
        let (_root, db, config) = state().await;
        config.write().await.knowledge_vault_path = r"C:\ExistingVault".to_string();

        let error = connect_in_state(&db, &config, Path::new(r"C:\MissingVault"))
            .await
            .unwrap_err();

        assert!(error.contains("知识库文件夹"));
        assert_eq!(
            config.read().await.knowledge_vault_path,
            r"C:\ExistingVault"
        );
    }
}
