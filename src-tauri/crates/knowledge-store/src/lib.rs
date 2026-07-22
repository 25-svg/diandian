use knowledge::VaultScan;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

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

pub const KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL: &str = r#"
ALTER TABLE knowledge_documents ADD COLUMN classification TEXT NOT NULL DEFAULT 'invalid';
CREATE INDEX idx_knowledge_documents_classification ON knowledge_documents(classification, active);
"#;

pub const KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL: &str = r#"
UPDATE knowledge_documents SET classification = CASE
    WHEN eligible=1 THEN 'eligible'
    WHEN issue='检测到个人或受限信息，正文未进入索引' THEN 'restricted'
    WHEN issue IS NOT NULL THEN 'invalid'
    WHEN status IN ('pending_review','pending','draft','待审核','待确认') THEN 'pending_review'
    ELSE 'invalid'
END;
"#;

pub const KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL: &str = r#"
ALTER TABLE knowledge_documents ADD COLUMN asr_eligible INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_knowledge_documents_asr_eligible ON knowledge_documents(asr_eligible, active);
"#;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentRecord {
    pub card_id: String,
    pub title: String,
    pub card_type: String,
    pub version: String,
    pub relative_path: String,
    pub metadata_json: String,
    pub body: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSyncSummary {
    pub vault_path: String,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub deactivated: usize,
    pub eligible_count: usize,
    pub asr_eligible_count: usize,
    pub pending_review_count: usize,
    pub ignored_count: usize,
    pub restricted_count: usize,
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
    pub asr_eligible_count: i64,
    pub pending_review_count: i64,
    pub ignored_count: i64,
    pub restricted_count: i64,
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
    let asr_eligible_count = scan
        .documents
        .iter()
        .filter(|item| item.asr_eligible)
        .count();
    let pending_review_count = classification_count(scan, "pending_review");
    let ignored_count = classification_count(scan, "ignored");
    let restricted_count = classification_count(scan, "restricted");
    let error_count = scan
        .documents
        .iter()
        .filter(|item| matches!(item.classification.as_str(), "invalid" | "duplicate"))
        .count();
    let source_status = if error_count == 0 {
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
                   content_hash, metadata_json, body, eligible, asr_eligible, classification, issue, active, last_seen_sync,
                   updated_at
               ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,1,?15,datetime('now'))
               ON CONFLICT(source_id, relative_path) DO UPDATE SET
                   card_id=excluded.card_id, title=excluded.title, card_type=excluded.card_type,
                   status=excluded.status, version=excluded.version, content_hash=excluded.content_hash,
                   metadata_json=excluded.metadata_json, body=excluded.body,
                   eligible=excluded.eligible, asr_eligible=excluded.asr_eligible,
                   classification=excluded.classification,
                   issue=excluded.issue, active=1,
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
        .bind(i64::from(document.asr_eligible))
        .bind(&document.classification)
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
        "UPDATE knowledge_documents SET active=0, eligible=0, asr_eligible=0, updated_at=datetime('now') WHERE source_id=?1 AND active=1 AND last_seen_sync<>?2",
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
        asr_eligible_count,
        pending_review_count,
        ignored_count,
        restricted_count,
        error_count,
        synced_at,
    })
}

pub async fn list_eligible_documents(
    pool: &SqlitePool,
    card_types: &[String],
) -> Result<Vec<KnowledgeDocumentRecord>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT card_id, title, card_type, version, relative_path, metadata_json, body \
         FROM knowledge_documents WHERE active=1 AND eligible=1",
    );
    if !card_types.is_empty() {
        query.push(" AND card_type IN (");
        let mut separated = query.separated(", ");
        for card_type in card_types {
            separated.push_bind(card_type);
        }
        separated.push_unseparated(")");
    }
    query
        .push(" ORDER BY card_id, relative_path")
        .build_query_as::<KnowledgeDocumentRecord>()
        .fetch_all(pool)
        .await
}

pub async fn list_asr_parameter_cards(
    pool: &SqlitePool,
) -> Result<Vec<KnowledgeDocumentRecord>, sqlx::Error> {
    sqlx::query_as::<_, KnowledgeDocumentRecord>(
        "SELECT card_id, title, card_type, version, relative_path, metadata_json, body \
         FROM knowledge_documents WHERE active=1 AND asr_eligible=1 \
         ORDER BY card_id, relative_path",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_knowledge_status(
    pool: &SqlitePool,
    vault_path: Option<&str>,
) -> Result<KnowledgeStatus, sqlx::Error> {
    let has_asr_eligibility = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('knowledge_documents') WHERE name='asr_eligible'",
    )
    .fetch_one(pool)
    .await?
        > 0;
    let asr_count = if has_asr_eligibility {
        "COALESCE(SUM(CASE WHEN document.active=1 AND document.asr_eligible=1 THEN 1 ELSE 0 END),0)"
    } else {
        "0"
    };
    let select = format!(
        r#"SELECT 1 AS connected, source.vault_path, source.status,
            source.last_synced_at,
            COALESCE(SUM(CASE WHEN document.active=1 THEN 1 ELSE 0 END),0) AS active_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.eligible=1 THEN 1 ELSE 0 END),0) AS eligible_count,
            {asr_count} AS asr_eligible_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.classification='pending_review' THEN 1 ELSE 0 END),0) AS pending_review_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.classification='ignored' THEN 1 ELSE 0 END),0) AS ignored_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.classification='restricted' THEN 1 ELSE 0 END),0) AS restricted_count,
            COALESCE(SUM(CASE WHEN document.active=1 AND document.classification IN ('invalid','duplicate') THEN 1 ELSE 0 END),0) AS error_count
        FROM knowledge_sources source
        LEFT JOIN knowledge_documents document ON document.source_id=source.id"#
    );
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
        asr_eligible_count: 0,
        pending_review_count: 0,
        ignored_count: 0,
        restricted_count: 0,
        error_count: 0,
    }))
}

fn classification_count(scan: &VaultScan, classification: &str) -> usize {
    scan.documents
        .iter()
        .filter(|item| item.classification == classification)
        .count()
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
        pool.execute(KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL)
            .await
            .unwrap();
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
            asr_eligible: false,
            classification: if eligible {
                "eligible"
            } else {
                "pending_review"
            }
            .into(),
            issue: None,
        }
    }

    #[tokio::test]
    async fn v20_adds_asr_eligibility_column_and_index() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL)
            .await
            .unwrap();

        let column: (String, i64, String) = sqlx::query_as(
            "SELECT name, [notnull], dflt_value FROM pragma_table_info('knowledge_documents') WHERE name='asr_eligible'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(column, ("asr_eligible".into(), 1, "0".into()));
        let index: String = sqlx::query_scalar(
            "SELECT name FROM pragma_index_list('knowledge_documents') WHERE name='idx_knowledge_documents_asr_eligible'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(index, "idx_knowledge_documents_asr_eligible");
    }

    #[tokio::test]
    async fn general_query_filters_active_eligible_rows_with_bound_card_types() {
        let pool = pool().await;
        let mut product = document("01-产品事实/PF-001.md", "PF-001", "hash-a", true);
        product.title = "产品事实".into();
        product.metadata = serde_json::json!({"source": "company"});
        product.body = "产品正文".into();
        let mut other = document("04-话术模块/TALK-001.md", "TALK-001", "hash-b", true);
        other.card_type = "talk_track".into();
        let inactive = document("01-产品事实/PF-OLD.md", "PF-OLD", "hash-c", true);
        sync_knowledge_vault(&pool, r"C:\Vault", &scan(vec![product, other, inactive]))
            .await
            .unwrap();
        sqlx::query("UPDATE knowledge_documents SET active=0 WHERE card_id='PF-OLD'")
            .execute(&pool)
            .await
            .unwrap();

        let rows = list_eligible_documents(&pool, &["product_fact".into()])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].card_id, "PF-001");
        assert_eq!(rows[0].title, "产品事实");
        assert_eq!(rows[0].card_type, "product_fact");
        assert_eq!(rows[0].version, "1");
        assert_eq!(rows[0].relative_path, "01-产品事实/PF-001.md");
        assert_eq!(rows[0].metadata_json, r#"{"source":"company"}"#);
        assert_eq!(rows[0].body, "产品正文");

        let injection = list_eligible_documents(&pool, &["product_fact' OR 1=1 --".into()])
            .await
            .unwrap();
        assert!(injection.is_empty());
    }

    #[tokio::test]
    async fn asr_query_is_independent_from_general_eligibility() {
        let pool = pool().await;
        let mut pending_parameter =
            document("08-产品参数库/PARAM-001.md", "PARAM-001", "hash-a", false);
        pending_parameter.card_type = "product_catalog".into();
        pending_parameter.asr_eligible = true;
        let mut pending_alias = document(
            "02-别名与ASR纠错/ALIAS-001.md",
            "ALIAS-001",
            "hash-b",
            false,
        );
        pending_alias.card_type = "alias".into();
        pending_alias.asr_eligible = true;
        let mut malformed = document("08-产品参数库/broken.md", "", "hash-c", false);
        malformed.classification = "invalid".into();
        malformed.issue = Some("missing id".into());
        let mut restricted = document("08-产品参数库/private.md", "PARAM-PRIVATE", "hash-d", false);
        restricted.classification = "restricted".into();
        restricted.issue = Some("restricted".into());

        let summary = sync_knowledge_vault(
            &pool,
            r"C:\Vault",
            &scan(vec![
                pending_parameter,
                pending_alias,
                malformed,
                restricted,
            ]),
        )
        .await
        .unwrap();

        assert_eq!(summary.asr_eligible_count, 2);
        let status = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();
        assert_eq!(status.asr_eligible_count, 2);

        assert!(list_eligible_documents(&pool, &[])
            .await
            .unwrap()
            .is_empty());
        let rows = list_asr_parameter_cards(&pool).await.unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.card_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ALIAS-001", "PARAM-001"]
        );
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
        invalid.classification = "invalid".into();
        invalid.issue = Some("YAML frontmatter 未闭合".into());
        sync_knowledge_vault(&pool, r"C:\Vault", &scan(vec![invalid]))
            .await
            .unwrap();
        let status = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();
        assert_eq!((status.error_count, status.eligible_count), (1, 0));
    }

    #[tokio::test]
    async fn reports_beginner_friendly_document_categories() {
        let pool = pool().await;
        let eligible = document("approved.md", "PF-001", "hash-a", true);
        let pending = document("pending.md", "PF-002", "hash-b", false);
        let mut ignored = document("README.md", "", "hash-c", false);
        ignored.classification = "ignored".into();
        ignored.status.clear();
        let mut restricted = document("restricted.md", "PF-003", "hash-d", false);
        restricted.classification = "restricted".into();
        restricted.issue = Some("检测到个人或受限信息，正文未进入索引".into());
        let mut invalid = document("broken.md", "", "hash-e", false);
        invalid.classification = "invalid".into();
        invalid.issue = Some("YAML frontmatter 未闭合".into());

        sync_knowledge_vault(
            &pool,
            r"C:\Vault",
            &scan(vec![eligible, pending, ignored, restricted, invalid]),
        )
        .await
        .unwrap();
        let status = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();

        assert_eq!(status.eligible_count, 1);
        assert_eq!(status.pending_review_count, 1);
        assert_eq!(status.ignored_count, 1);
        assert_eq!(status.restricted_count, 1);
        assert_eq!(status.error_count, 1);
    }

    #[tokio::test]
    async fn migration_backfills_existing_v16_documents() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        sqlx::query("INSERT INTO knowledge_sources (id, vault_path, status) VALUES (1, 'C:\\Vault', 'ready')")
            .execute(&pool)
            .await
            .unwrap();
        for (path, status, eligible, issue) in [
            ("approved.md", "approved", 1_i64, None),
            ("pending.md", "pending_review", 0_i64, None),
            ("broken.md", "", 0_i64, Some("YAML frontmatter 未闭合")),
        ] {
            sqlx::query(
                "INSERT INTO knowledge_documents (source_id, relative_path, status, content_hash, eligible, issue, last_seen_sync) VALUES (1, ?1, ?2, ?3, ?4, ?5, 'sync')",
            )
            .bind(path)
            .bind(status)
            .bind(path)
            .bind(eligible)
            .bind(issue)
            .execute(&pool)
            .await
            .unwrap();
        }

        pool.execute(KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL)
            .await
            .unwrap();
        let status = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();
        assert_eq!(status.eligible_count, 1);
        assert_eq!(status.pending_review_count, 1);
        assert_eq!(status.error_count, 1);
    }

    #[tokio::test]
    async fn rebuilds_snapshot_from_vault_scan() {
        let pool = pool().await;
        let mut invalid = document("broken.md", "", "hash-b", false);
        invalid.classification = "invalid".into();
        invalid.issue = Some("YAML frontmatter 未闭合".into());
        let source = scan(vec![
            document("PF-001.md", "PF-001", "hash-a", true),
            invalid,
        ]);
        sync_knowledge_vault(&pool, r"C:\Vault", &source)
            .await
            .unwrap();
        let before = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();

        sqlx::query("DROP TABLE knowledge_documents")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE knowledge_sources")
            .execute(&pool)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_CLASSIFICATION_BACKFILL_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(KNOWLEDGE_ASR_ELIGIBILITY_MIGRATION_SQL)
            .await
            .unwrap();
        sync_knowledge_vault(&pool, r"C:\Vault", &source)
            .await
            .unwrap();
        let after = get_knowledge_status(&pool, Some(r"C:\Vault"))
            .await
            .unwrap();

        assert_eq!(
            (
                before.active_count,
                before.eligible_count,
                before.error_count
            ),
            (after.active_count, after.eligible_count, after.error_count)
        );
    }
}
