use knowledge::VaultScan;
use sqlx::SqlitePool;

pub const KNOWLEDGE_MIGRATION_SQL: &str = r#"
CREATE TABLE knowledge_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_path TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'disconnected',
    last_synced_at TEXT,
    total_count INTEGER NOT NULL DEFAULT 0,
    eligible_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE knowledge_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id INTEGER NOT NULL,
    relative_path TEXT NOT NULL,
    card_id TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    card_type TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '1',
    content_hash TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    body TEXT NOT NULL DEFAULT '',
    eligible INTEGER NOT NULL DEFAULT 0,
    issue TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    last_seen_sync TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, relative_path),
    FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
);
CREATE INDEX idx_knowledge_documents_card_id ON knowledge_documents(card_id);
CREATE INDEX idx_knowledge_documents_type_status ON knowledge_documents(card_type, status);
CREATE INDEX idx_knowledge_documents_eligible ON knowledge_documents(eligible, active);
"#;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSyncSummary {
    pub vault_path: String,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub deactivated: usize,
    pub eligible_count: usize,
    pub error_count: usize,
    pub synced_at: String,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeStatus {
    pub connected: bool,
    pub vault_path: String,
    pub status: String,
    pub last_synced_at: Option<String>,
    pub active_count: i64,
    pub eligible_count: i64,
    pub error_count: i64,
}

pub async fn sync_knowledge_vault(
    pool: &SqlitePool,
    vault_path: &str,
    scan: &VaultScan,
) -> Result<KnowledgeSyncSummary, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let sync_token = uuid::Uuid::new_v4().to_string();
    let synced_at = chrono::Utc::now().to_rfc3339();
    let eligible_count = scan.documents.iter().filter(|item| item.eligible).count();
    let error_count = scan
        .documents
        .iter()
        .filter(|item| item.issue.is_some())
        .count();
    let source_status = if scan.issues.is_empty() {
        "ready"
    } else {
        "degraded"
    };

    let source_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO knowledge_sources (vault_path, status, updated_at)
           VALUES (?1, 'syncing', datetime('now'))
           ON CONFLICT(vault_path) DO UPDATE SET status='syncing', updated_at=datetime('now')
           RETURNING id"#,
    )
    .bind(vault_path)
    .fetch_one(&mut *transaction)
    .await?;

    let (mut inserted, mut updated, mut unchanged) = (0usize, 0usize, 0usize);
    for document in &scan.documents {
        let existing = sqlx::query_scalar::<_, String>(
            "SELECT content_hash FROM knowledge_documents WHERE source_id=?1 AND relative_path=?2",
        )
        .bind(source_id)
        .bind(&document.relative_path)
        .fetch_optional(&mut *transaction)
        .await?;
        match existing {
            None => inserted += 1,
            Some(hash) if hash == document.content_hash => unchanged += 1,
            Some(_) => updated += 1,
        }

        sqlx::query(
            r#"INSERT INTO knowledge_documents (
                   source_id, relative_path, card_id, title, card_type, status, version,
                   content_hash, metadata_json, body, eligible, issue, active, last_seen_sync,
                   updated_at
               ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,1,?13,datetime('now'))
               ON CONFLICT(source_id, relative_path) DO UPDATE SET
                   card_id=excluded.card_id, title=excluded.title, card_type=excluded.card_type,
                   status=excluded.status, version=excluded.version, content_hash=excluded.content_hash,
                   metadata_json=excluded.metadata_json, body=excluded.body,
                   eligible=excluded.eligible, issue=excluded.issue, active=1,
                   last_seen_sync=excluded.last_seen_sync, updated_at=datetime('now')"#,
        )
        .bind(source_id)
        .bind(&document.relative_path)
        .bind(&document.card_id)
        .bind(&document.title)
        .bind(&document.card_type)
        .bind(&document.status)
        .bind(&document.version)
        .bind(&document.content_hash)
        .bind(document.metadata.to_string())
        .bind(&document.body)
        .bind(i64::from(document.eligible))
        .bind(&document.issue)
        .bind(&sync_token)
        .execute(&mut *transaction)
        .await?;
    }

    let deactivated = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM knowledge_documents WHERE source_id=?1 AND active=1 AND last_seen_sync<>?2",
    )
    .bind(source_id)
    .bind(&sync_token)
    .fetch_one(&mut *transaction)
    .await? as usize;
    sqlx::query(
        "UPDATE knowledge_documents SET active=0, eligible=0, updated_at=datetime('now') WHERE source_id=?1 AND active=1 AND last_seen_sync<>?2",
    )
    .bind(source_id)
    .bind(&sync_token)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        r#"UPDATE knowledge_sources SET status=?2, last_synced_at=?3, total_count=?4,
               eligible_count=?5, error_count=?6, updated_at=datetime('now') WHERE id=?1"#,
    )
    .bind(source_id)
    .bind(source_status)
    .bind(&synced_at)
    .bind(scan.documents.len() as i64)
    .bind(eligible_count as i64)
    .bind(error_count as i64)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(KnowledgeSyncSummary {
        vault_path: vault_path.to_string(),
        inserted,
        updated,
        unchanged,
        deactivated,
        eligible_count,
        error_count,
        synced_at,
    })
}

pub async fn get_knowledge_status(
    pool: &SqlitePool,
    vault_path: Option<&str>,
) -> Result<KnowledgeStatus, sqlx::Error> {
    let select = r#"SELECT 1 AS connected, source.vault_path, source.status,
            source.last_synced_at,
            COALESCE(SUM(CASE WHEN document.active=1 THEN 1 ELSE 0 END),0) AS active_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.eligible=1 THEN 1 ELSE 0 END),0) AS eligible_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.issue IS NOT NULL THEN 1 ELSE 0 END),0) AS error_count
        FROM knowledge_sources source
        LEFT JOIN knowledge_documents document ON document.source_id=source.id"#;
    let result = if let Some(path) = vault_path {
        sqlx::query_as::<_, KnowledgeStatus>(&format!(
            "{select} WHERE source.vault_path=?1 GROUP BY source.id"
        ))
        .bind(path)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, KnowledgeStatus>(&format!(
            "{select} GROUP BY source.id ORDER BY source.updated_at DESC LIMIT 1"
        ))
        .fetch_optional(pool)
        .await?
    };
    Ok(result.unwrap_or(KnowledgeStatus {
        connected: false,
        vault_path: String::new(),
        status: "disconnected".to_string(),
        last_synced_at: None,
        active_count: 0,
        eligible_count: 0,
        error_count: 0,
    }))
}

#[cfg(test)]
mod knowledge_sync_tests {
    use super::*;
    use knowledge::{KnowledgeDocument, VaultInspection};
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool
    }

    fn document(path: &str, id: &str, hash: &str, eligible: bool) -> KnowledgeDocument {
        KnowledgeDocument {
            relative_path: path.into(),
            card_id: id.into(),
            title: "测试卡".into(),
            card_type: "product_fact".into(),
            status: if eligible {
                "approved"
            } else {
                "pending_review"
            }
            .into(),
            version: "1".into(),
            content_hash: hash.into(),
            metadata: serde_json::json!({"id": id}),
            body: "正文".into(),
            eligible,
            issue: None,
        }
    }

    fn scan(documents: Vec<KnowledgeDocument>) -> VaultScan {
        VaultScan {
            inspection: VaultInspection {
                path: r"C:\Vault".into(),
                valid: true,
                has_obsidian_config: true,
                markdown_count: documents.len(),
                missing_directories: vec![],
            },
            documents,
            issues: vec![],
        }
    }

    #[tokio::test]
    async fn sync_is_idempotent_and_deactivates_missing_documents() {
        let pool = pool().await;
        let first = scan(vec![document("PF-001.md", "PF-001", "hash-a", true)]);
        let first_result = sync_knowledge_vault(&pool, r"C:\Vault", &first)
            .await
            .unwrap();
        assert_eq!(
            (
                first_result.inserted,
                first_result.updated,
                first_result.unchanged,
                first_result.deactivated
            ),
            (1, 0, 0, 0)
        );
        let second = sync_knowledge_vault(&pool, r"C:\Vault", &first)
            .await
            .unwrap();
        assert_eq!(
            (second.inserted, second.updated, second.unchanged),
            (0, 0, 1)
        );
        let changed = scan(vec![document("PF-001.md", "PF-001", "hash-b", true)]);
        let changed_result = sync_knowledge_vault(&pool, r"C:\Vault", &changed)
            .await
            .unwrap();
        assert_eq!(
            (
                changed_result.inserted,
                changed_result.updated,
                changed_result.unchanged
            ),
            (0, 1, 0)
        );
        let empty = scan(vec![]);
        assert_eq!(
            sync_knowledge_vault(&pool, r"C:\Vault", &empty)
                .await
                .unwrap()
                .deactivated,
            1
        );
        assert_eq!(
            get_knowledge_status(&pool, Some(r"C:\Vault"))
                .await
                .unwrap()
                .active_count,
            0
        );
    }

    #[tokio::test]
    async fn parse_failures_are_indexed_but_never_eligible() {
        let pool = pool().await;
        let mut invalid = document("broken.md", "", "hash-b", false);
        invalid.issue = Some("YAML frontmatter 未闭合".into());
        sync_knowledge_vault(&pool, r"C:\Vault", &scan(vec![invalid]))
            .await
            .unwrap();
        let status = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();
        assert_eq!((status.error_count, status.eligible_count), (1, 0));
    }
}
