use super::{Database, DatabaseError};
use knowledge::VaultScan;

pub use knowledge_store::{
    KnowledgeStatus, KnowledgeSyncSummary, KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL,
    KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL, KNOWLEDGE_MIGRATION_SQL,
};

impl Database {
    pub async fn sync_knowledge_vault(
        &self,
        vault_path: &str,
        scan: &VaultScan,
    ) -> Result<KnowledgeSyncSummary, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(knowledge_store::sync_knowledge_vault(&pool, vault_path, scan).await?)
    }

    pub async fn get_knowledge_status(
        &self,
        vault_path: Option<&str>,
    ) -> Result<KnowledgeStatus, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(knowledge_store::get_knowledge_status(&pool, vault_path).await?)
    }
}
