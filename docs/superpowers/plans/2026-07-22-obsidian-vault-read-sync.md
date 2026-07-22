# Obsidian Vault Connection and Read-only Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让桌面应用连接用户选择的 Obsidian Vault，将可解析 Markdown 增量同步为可重建的 SQLite 只读索引，并在设置页向直播负责人展示清晰状态。

**Architecture:** 新建独立 `knowledge` Rust crate 负责 Vault 校验、YAML frontmatter 解析和递归扫描；主程序数据库层负责事务式增量索引；Tauri handler 负责配置和命令边界；独立 Svelte 组件负责目录选择、同步、状态和错误展示。P1 不修改任何 Obsidian 文件，也不把索引接入 ASR 或高光模型。

**Tech Stack:** Rust 2021、Serde/serde_yaml、Walkdir、SHA-256、SQLx 0.8 SQLite、Tauri 2、Svelte 3、TypeScript 5、Node assert 测试。

## Global Constraints

- 唯一源码仓库是 `D:\git_work\bili-shadowreplay\bili-shadowreplay`。
- 执行前必须使用 `superpowers:using-git-worktrees`，从当前源码的不可变快照建立 `D:\git_work\bili-shadowreplay-worktrees\obsidian-vault-read-sync`；不得 reset、checkout 或覆盖主工作区未提交内容。
- P1 对 Vault 严格只读：不得创建目录、修改 Markdown、写入 `.obsidian` 或生成冲突文件。
- Obsidian Markdown 是唯一知识真源；SQLite 只是可删除、可重建快照。
- 只有 `status` 为 `approved`/`imported`（兼容既有 `已审核`/`已入库`）且包含稳定 `id` 的卡片可标记为 `eligible=true`；界面只显示中文状态。
- `.obsidian`、隐藏目录、`90-模板`、`99-待处理冲突` 不进入正式索引。
- 不记录 API Key、Cookie、访问令牌、买家姓名、电话或地址。
- `sensitivity: restricted/personal` 或正文包含明显手机号、邮箱地址的卡片只记录路径、哈希和阻塞原因，正文不得进入 SQLite。
- 所有文件路径在进入数据库前转换为 Vault 相对路径；禁止把 Vault 外路径写入索引。
- 测试必须遵循红、绿、重构；每个生产行为先有失败测试。
- P1 不修改 `ArchiveAnalysis.svelte`、ASR、片段分析提示词和母稿逻辑。

## File Map

### New files

- `src-tauri/crates/knowledge/Cargo.toml`：独立知识解析 crate 依赖。
- `src-tauri/crates/knowledge/src/lib.rs`：公开类型、Vault 检查、Markdown 解析和递归扫描。
- `src-tauri/crates/knowledge/tests/vault_scan.rs`：临时 Vault 行为测试和外部样板只读冒烟测试。
- `src-tauri/src/database/knowledge.rs`：迁移 SQL、索引行类型、事务同步和状态查询。
- `src-tauri/src/handlers/knowledge.rs`：Tauri 命令、路径校验、配置持久化和打开 Vault。
- `src/lib/knowledge.ts`：前端 DTO、状态文案和错误文案纯函数。
- `src/lib/knowledge.test.ts`：前端知识状态单元测试。
- `src/lib/components/settings/KnowledgeVaultSettings.svelte`：知识库设置区块。
- `scripts/check-knowledge-settings.mjs`：设置组件命令和文案契约检查。

### Modified files

- `src-tauri/Cargo.toml`：注册 `knowledge` workspace member 和依赖。
- `src-tauri/Cargo.lock`：Cargo 自动更新。
- `src-tauri/src/database/mod.rs`：导出知识索引模块。
- `src-tauri/src/config.rs`：持久化 `knowledge_vault_path`。
- `src-tauri/src/handlers/mod.rs`：导出 knowledge handler。
- `src-tauri/src/main.rs`：增加迁移 16 和四个 Tauri 命令。
- `src/lib/interface.ts`：Config 增加知识库路径。
- `src/page/Setting.svelte`：挂载独立设置组件。
- `package.json`：注册前端知识设置测试命令。
- `C:\Users\10230\Documents\Codex\Workspace\01-Projects\Project-003-直播切片分析系统\05-测试与验收\2026-07-22-Obsidian-P1验收记录.md`：保存验证证据。

---

### Task 1: Vault inspection and Markdown parsing crate

**Files:**
- Create: `src-tauri/crates/knowledge/Cargo.toml`
- Create: `src-tauri/crates/knowledge/src/lib.rs`
- Create: `src-tauri/crates/knowledge/tests/vault_scan.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: `inspect_vault(path: &Path) -> Result<VaultInspection, KnowledgeError>`
- Produces: `parse_document(vault: &Path, path: &Path) -> KnowledgeDocument`
- Produces: `scan_vault(path: &Path) -> Result<VaultScan, KnowledgeError>`
- Produces DTOs: `VaultInspection`, `KnowledgeDocument`, `VaultScan`, `ScanIssue`, `KnowledgeError`

- [ ] **Step 1: Add the crate manifest and workspace registration**

Create `src-tauri/crates/knowledge/Cargo.toml`:

```toml
[package]
name = "knowledge"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
regex = "1"
sha2 = "0.10"
thiserror = "2"
walkdir = "2"

[dev-dependencies]
tempfile = "3"
```

Append `"crates/knowledge"` to `workspace.members` in `src-tauri/Cargo.toml`.

- [ ] **Step 2: Write failing Vault tests**

Create `src-tauri/crates/knowledge/tests/vault_scan.rs` with these behaviors:

```rust
use knowledge::{inspect_vault, scan_vault};
use std::fs;

fn write(root: &std::path::Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn inspects_existing_vault_without_writing_to_it() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    let before = fs::read_dir(root.path()).unwrap().count();
    let result = inspect_vault(root.path()).unwrap();
    assert!(result.valid);
    assert_eq!(result.markdown_count, 0);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), before);
}

#[test]
fn indexes_only_approved_cards_and_reports_invalid_frontmatter() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    write(root.path(), "01-产品事实/PF-001.md", "---\nid: PF-001\ntitle: R7 产品事实\ntype: product_fact\nstatus: 已审核\nversion: 2\n---\n# 正文");
    write(root.path(), "03-直播案例/LC-001.md", "---\nid: LC-001\ntitle: 待审核案例\ntype: live_case\nstatus: 待确认\nversion: 1\n---\n正文");
    write(root.path(), "03-直播案例/broken.md", "---\nid: [\n---\n正文");
    write(root.path(), "03-直播案例/private.md", "---\nid: LC-PRIVATE\ntitle: 客户记录\ntype: live_case\nstatus: approved\nversion: 1\nsensitivity: personal\n---\n手机号 13800138000");
    write(root.path(), "90-模板/template.md", "---\nid: TEMPLATE\nstatus: 已审核\n---\n模板");

    let scan = scan_vault(root.path()).unwrap();
    assert_eq!(scan.documents.len(), 4);
    assert_eq!(scan.documents.iter().filter(|item| item.eligible).count(), 1);
    assert_eq!(scan.issues.len(), 2);
    let private = scan.documents.iter().find(|item| item.card_id == "LC-PRIVATE").unwrap();
    assert!(private.body.is_empty());
    assert_eq!(private.issue.as_deref(), Some("检测到个人或受限信息，正文未进入索引"));
    assert!(!root.path().join("99-待处理冲突").exists());
}

#[test]
fn rejects_paths_that_are_not_directories() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-vault.md");
    fs::write(&file, "x").unwrap();
    assert!(inspect_vault(&file).unwrap_err().to_string().contains("文件夹"));
}

#[test]
fn duplicate_card_ids_are_reported_and_never_eligible() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    for name in ["first.md", "second.md"] {
        write(root.path(), name, "---\nid: DUP-001\ntitle: 重复卡\ntype: product_fact\nstatus: approved\nversion: 1\n---\n正文");
    }
    let scan = scan_vault(root.path()).unwrap();
    assert_eq!(scan.issues.len(), 2);
    assert!(scan.documents.iter().all(|item| !item.eligible));
    assert!(scan.documents.iter().all(|item| item.issue.as_deref() == Some("知识卡 ID 重复: DUP-001")));
}
```

- [ ] **Step 3: Run the tests and verify RED**

Run:

```powershell
cd src-tauri
cargo test -p knowledge --test vault_scan
```

Expected: compilation fails because `knowledge::inspect_vault` and `knowledge::scan_vault` do not exist.

- [ ] **Step 4: Implement the public types and parser**

Create `src-tauri/crates/knowledge/src/lib.rs` with these exact public contracts:

```rust
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

const REQUIRED_DIRS: &[&str] = &[
    "01-产品事实", "02-别名与ASR纠错", "03-直播案例", "04-话术模块",
    "05-人群画像", "06-辅稿", "07-证据索引", "08-产品参数库", "09-审核逐字稿",
];
const EXCLUDED_DIRS: &[&str] = &[".obsidian", "90-模板", "99-待处理冲突"];

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("请选择一个存在的 Obsidian 知识库文件夹")]
    InvalidVault,
    #[error("无法读取知识库：{0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultInspection {
    pub path: String,
    pub valid: bool,
    pub has_obsidian_config: bool,
    pub markdown_count: usize,
    pub missing_directories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocument {
    pub relative_path: String,
    pub card_id: String,
    pub title: String,
    pub card_type: String,
    pub status: String,
    pub version: String,
    pub content_hash: String,
    pub metadata: serde_json::Value,
    pub body: String,
    pub eligible: bool,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub relative_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultScan {
    pub inspection: VaultInspection,
    pub documents: Vec<KnowledgeDocument>,
    pub issues: Vec<ScanIssue>,
}
```

Implement these private rules in the same file:

```rust
fn is_excluded(relative: &Path) -> bool {
    relative.components().any(|part| {
        let value = part.as_os_str().to_string_lossy();
        value.starts_with('.') || EXCLUDED_DIRS.contains(&value.as_ref())
    })
}

fn split_frontmatter(content: &str) -> Result<(&str, &str), String> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let rest = normalized.strip_prefix("---\n").ok_or("缺少 YAML frontmatter")?;
    let end = rest.find("\n---\n").ok_or("YAML frontmatter 未闭合")?;
    Ok((&rest[..end], &rest[end + 5..]))
}

fn yaml_string(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

fn yaml_version(value: &Value) -> String {
    value.get("version").map(|item| match item {
        Value::String(value) => value.trim().to_string(),
        Value::Number(value) => value.to_string(),
        _ => "1".to_string(),
    }).unwrap_or_else(|| "1".to_string())
}

fn approved(status: &str) -> bool {
    matches!(status, "approved" | "imported" | "已审核" | "已入库")
}
```

`parse_document` must always return a document row. Parse failures set `issue`, leave identifying fields and body empty, preserve the content hash, and set `eligible=false`. `scan_vault` sorts by `relative_path`, includes invalid Markdown in `documents`, mirrors each issue in `issues`, and never writes to disk.

Apply these exact validation rules:

- A valid Vault path is a directory containing `.obsidian` or at least one name from `REQUIRED_DIRS`.
- `id`, `title`, `type`, and `status` are required; missing values produce `issue="缺少必填字段: <逗号分隔字段>"`.
- `version` is optional and defaults to string `"1"`; both YAML numbers and strings are accepted.
- Relative paths use `/` separators on every platform.
- `content_hash` is lowercase SHA-256 of the complete UTF-8 Markdown bytes.
- YAML metadata is converted with `serde_json::to_value`; unsupported YAML map keys produce a parse issue rather than a panic.
- `sensitivity` 为 `restricted` 或 `personal` 时清空正文并设置阻塞原因。
- 正文匹配中国大陆手机号 `(?:^|[^0-9])(1[3-9][0-9]{9})(?:$|[^0-9])` 或邮箱地址 `[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}` 时清空正文并设置相同阻塞原因；测试数据使用虚构号码。
- 正文匹配 `Authorization: Bearer`、`access_token:` 或 `sk-` 加至少 20 个字母数字字符时按受限信息处理。
- 文件先按字节读取并计算哈希；非 UTF-8 文件生成 issue、正文留空，不得让整个扫描失败。
- `WalkDir` 不跟随符号链接；每个文件规范化后的路径必须以规范化 Vault 根目录开头。
- 扫描完成后按非空 `card_id` 分组；重复 ID 的全部卡片设置 `issue="知识卡 ID 重复: <id>"` 且 `eligible=false`。

- [ ] **Step 5: Run tests and verify GREEN**

Run:

```powershell
cd src-tauri
cargo fmt --all -- --check
cargo test -p knowledge --test vault_scan
```

Expected: 4 tests pass; formatting exits 0.

- [ ] **Step 6: Commit Task 1**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/crates/knowledge
git commit -m "feat: add read-only Obsidian vault scanner"
```

---

### Task 2: SQLite knowledge snapshot and idempotent sync

**Files:**
- Create: `src-tauri/src/database/knowledge.rs`
- Modify: `src-tauri/src/database/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `knowledge::VaultScan`
- Produces: `KNOWLEDGE_MIGRATION_SQL: &str`
- Produces: `Database::sync_knowledge_vault(vault_path: &str, scan: &VaultScan) -> Result<KnowledgeSyncSummary, DatabaseError>`
- Produces: `Database::get_knowledge_status(vault_path: Option<&str>) -> Result<KnowledgeStatus, DatabaseError>`

- [ ] **Step 1: Write failing database tests**

At the bottom of `src-tauri/src/database/knowledge.rs`, add `#[cfg(test)] mod tests` using a single-connection in-memory pool:

```rust
async fn database() -> Database {
    use sqlx::{sqlite::SqlitePoolOptions, Executor};
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
    let db = Database::new();
    db.set(pool).await;
    db
}

#[tokio::test]
async fn sync_is_idempotent_and_deactivates_missing_documents() {
    let db = database().await;
    let first = scan(vec![document("PF-001.md", "PF-001", "hash-a", true)]);
    let summary = db.sync_knowledge_vault(r"C:\Vault", &first).await.unwrap();
    assert_eq!((summary.inserted, summary.updated, summary.unchanged, summary.deactivated), (1, 0, 0, 0));

    let second = db.sync_knowledge_vault(r"C:\Vault", &first).await.unwrap();
    assert_eq!((second.inserted, second.updated, second.unchanged), (0, 0, 1));

    let changed = scan(vec![document("PF-001.md", "PF-001", "hash-b", true)]);
    let updated = db.sync_knowledge_vault(r"C:\Vault", &changed).await.unwrap();
    assert_eq!((updated.inserted, updated.updated, updated.unchanged), (0, 1, 0));

    let empty = scan(Vec::new());
    let third = db.sync_knowledge_vault(r"C:\Vault", &empty).await.unwrap();
    assert_eq!(third.deactivated, 1);
    assert_eq!(db.get_knowledge_status(Some(r"C:\Vault")).await.unwrap().active_count, 0);
}

#[tokio::test]
async fn parse_failures_are_indexed_but_never_eligible() {
    let db = database().await;
    let mut invalid = document("broken.md", "", "hash-b", false);
    invalid.issue = Some("YAML frontmatter 未闭合".to_string());
    db.sync_knowledge_vault(r"C:\Vault", &scan(vec![invalid])).await.unwrap();
    let status = db.get_knowledge_status(Some(r"C:\Vault")).await.unwrap();
    assert_eq!(status.error_count, 1);
    assert_eq!(status.eligible_count, 0);
}
```

Define local `document` and `scan` fixture functions returning the Task 1 DTOs; do not mock SQLx.

Use these exact fixtures:

```rust
fn document(path: &str, id: &str, hash: &str, eligible: bool) -> KnowledgeDocument {
    KnowledgeDocument {
        relative_path: path.to_string(),
        card_id: id.to_string(),
        title: "测试卡".to_string(),
        card_type: "product_fact".to_string(),
        status: if eligible { "approved" } else { "pending_review" }.to_string(),
        version: "1".to_string(),
        content_hash: hash.to_string(),
        metadata: serde_json::json!({"id": id}),
        body: "正文".to_string(),
        eligible,
        issue: None,
    }
}

fn scan(documents: Vec<KnowledgeDocument>) -> VaultScan {
    VaultScan {
        inspection: VaultInspection {
            path: r"C:\Vault".to_string(),
            valid: true,
            has_obsidian_config: true,
            markdown_count: documents.len(),
            missing_directories: Vec::new(),
        },
        documents,
        issues: Vec::new(),
    }
}
```

- [ ] **Step 2: Run the database tests and verify RED**

Run:

```powershell
cd src-tauri
cargo test knowledge_sync_tests
```

Expected: compilation fails because the migration constant and Database methods do not exist.

- [ ] **Step 3: Add migration 16**

Create `src-tauri/src/database/knowledge.rs` with:

```rust
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
```

Export `pub mod knowledge;` from `database/mod.rs`. Add migration version 16 in `get_migrations()`:

```rust
Migration {
    version: 16,
    description: "add_knowledge_snapshot_tables",
    sql: database::knowledge::KNOWLEDGE_MIGRATION_SQL,
    kind: MigrationKind::Up,
},
```

- [ ] **Step 4: Implement transaction sync and status DTOs**

Use these DTO contracts in `database/knowledge.rs`:

```rust
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
```

`sync_knowledge_vault` must:

1. Begin one SQLx transaction.
2. Upsert `knowledge_sources` and get `source_id`.
3. Use one UUID sync token for every document.
4. Compare existing `content_hash` before update and count inserted/updated/unchanged.
5. Store `metadata` with `document.metadata.to_string()` and booleans as `0/1`.
6. Mark previous rows not seen in the sync as `active=0, eligible=0`.
7. Set source status to `ready` when `issues` is empty, otherwise `degraded`.
8. Commit only after counts and source status update succeed.

- [ ] **Step 5: Run tests and verify GREEN**

```powershell
cd src-tauri
cargo fmt --all -- --check
cargo test knowledge_sync_tests
cargo test -p knowledge
```

Expected: both database tests and all scanner tests pass.

- [ ] **Step 6: Commit Task 2**

```powershell
git add src-tauri/src/database/knowledge.rs src-tauri/src/database/mod.rs src-tauri/src/main.rs
git commit -m "feat: add rebuildable knowledge snapshot index"
```

---

### Task 3: Configuration and Tauri command boundary

**Files:**
- Create: `src-tauri/src/handlers/knowledge.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/handlers/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interfaces:**
- Produces command: `inspect_knowledge_vault(vault_path: String) -> Result<VaultInspection, String>`
- Produces command: `connect_knowledge_vault(state, vault_path: String) -> Result<KnowledgeSyncSummary, String>`
- Produces command: `sync_knowledge_vault(state) -> Result<KnowledgeSyncSummary, String>`
- Produces command: `get_knowledge_status(state) -> Result<KnowledgeStatus, String>`
- Produces command: `open_knowledge_vault(state) -> Result<(), String>`

- [ ] **Step 1: Write failing handler tests**

In `handlers/knowledge.rs`, create tests around internal functions that accept `&Database` and `&RwLock<Config>` rather than constructing Tauri state:

```rust
#[tokio::test]
async fn connect_saves_path_only_after_successful_sync() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".obsidian")).unwrap();
    std::fs::write(root.path().join("PF.md"), "---\nid: PF-1\ntitle: 产品\ntype: product_fact\nstatus: 已审核\nversion: 1\n---\n正文").unwrap();
    let (_root, db, config) = state().await;
    let result = connect_in_state(&db, &config, root.path()).await.unwrap();
    assert_eq!(result.inserted, 1);
    assert_eq!(config.read().await.knowledge_vault_path, root.path().canonicalize().unwrap().to_string_lossy());
}

#[tokio::test]
async fn failed_connect_does_not_replace_previous_path() {
    let (_root, db, config) = state().await;
    config.write().await.knowledge_vault_path = r"C:\ExistingVault".to_string();
    let error = connect_in_state(&db, &config, std::path::Path::new(r"C:\MissingVault")).await.unwrap_err();
    assert!(error.contains("知识库文件夹"));
    assert_eq!(config.read().await.knowledge_vault_path, r"C:\ExistingVault");
}
```

Use this local fixture so the test never touches the real config or database:

```rust
async fn state() -> (tempfile::TempDir, Database, RwLock<Config>) {
    use sqlx::{sqlite::SqlitePoolOptions, Executor};
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("Conf.toml");
    let cache = root.path().join("cache");
    let output = root.path().join("output");
    let config = Config::load(&config_path, &cache, &output).unwrap();
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    pool.execute(KNOWLEDGE_MIGRATION_SQL).await.unwrap();
    let db = Database::new();
    db.set(pool).await;
    (root, db, RwLock::new(config))
}
```

Tests destructure this as `let (_root, db, config) = state().await;` so the temporary config directory remains alive for the full test.

- [ ] **Step 2: Run tests and verify RED**

```powershell
cd src-tauri
cargo test knowledge_handler_tests
```

Expected: compilation fails because `knowledge_vault_path` and `connect_in_state` do not exist.

- [ ] **Step 3: Add backward-compatible configuration**

Add to `Config`:

```rust
#[serde(default)]
pub knowledge_vault_path: String,
```

Initialize it to `String::new()` in every explicit `Config` test fixture/default constructor. Do not store scan results in TOML.

- [ ] **Step 4: Implement commands with user-facing errors**

Add `open = "5"` to root package dependencies in `src-tauri/Cargo.toml`. Implement these rules in `handlers/knowledge.rs`:

```rust
fn canonical_vault(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize().map_err(|_| "找不到所选知识库文件夹，请重新选择".to_string())
}

async fn connect_in_state(
    db: &Database,
    config: &RwLock<Config>,
    path: &Path,
) -> Result<KnowledgeSyncSummary, String> {
    let canonical = canonical_vault(path)?;
    let scan = knowledge::scan_vault(&canonical).map_err(|error| error.to_string())?;
    let summary = db.sync_knowledge_vault(&canonical.to_string_lossy(), &scan).await.map_err(String::from)?;
    let mut writable = config.write().await;
    writable.knowledge_vault_path = canonical.to_string_lossy().to_string();
    writable.save();
    Ok(summary)
}
```

`sync_knowledge_vault` must reject an empty configured path with `尚未连接 Obsidian 知识库，请先在设置中选择文件夹`. `open_knowledge_vault` must open only the canonical configured path using `open::that_detached`; it must not accept a frontend path parameter.

`get_knowledge_status` must read the configured path first. An empty or no-longer-existing path returns `{ connected: false, status: "disconnected" }` while preserving the last stored counts for diagnostics. A present configured path reads the Task 2 snapshot and returns `connected=true`; it must not trigger a scan implicitly.

Export `pub mod knowledge;` in `handlers/mod.rs` and register all five commands in `setup_invoke_handlers`.

- [ ] **Step 5: Run tests and verify GREEN**

```powershell
cd src-tauri
cargo fmt --all -- --check
cargo test knowledge_handler_tests
cargo test knowledge_sync_tests
cargo test -p knowledge
```

Expected: all focused tests pass.

- [ ] **Step 6: Commit Task 3**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/config.rs src-tauri/src/handlers/knowledge.rs src-tauri/src/handlers/mod.rs src-tauri/src/main.rs
git commit -m "feat: expose Obsidian vault connection commands"
```

---

### Task 4: Frontend knowledge status model

**Files:**
- Create: `src/lib/knowledge.ts`
- Create: `src/lib/knowledge.test.ts`
- Modify: `src/lib/interface.ts`
- Modify: `package.json`

**Interfaces:**
- Produces DTOs: `KnowledgeStatus`, `KnowledgeSyncSummary`, `VaultInspection`
- Produces: `knowledgeStatusPresentation(status: KnowledgeStatus) -> KnowledgeStatusPresentation`
- Produces: `knowledgeSyncSummaryText(summary: KnowledgeSyncSummary) -> string`
- Produces: `friendlyKnowledgeError(error: unknown) -> string`

- [ ] **Step 1: Write failing TypeScript tests**

Create `src/lib/knowledge.test.ts`:

```ts
import assert from "node:assert/strict";
import {
  friendlyKnowledgeError,
  knowledgeStatusPresentation,
  knowledgeSyncSummaryText,
} from "./knowledge.js";

assert.deepEqual(knowledgeStatusPresentation({
  connected: false, vaultPath: "", status: "disconnected", lastSyncedAt: null,
  activeCount: 0, eligibleCount: 0, errorCount: 0,
}), { tone: "neutral", label: "未连接", detail: "选择 Obsidian 知识库文件夹后即可同步" });

assert.deepEqual(knowledgeStatusPresentation({
  connected: true, vaultPath: "C:/Vault", status: "degraded", lastSyncedAt: "2026-07-22 10:00:00",
  activeCount: 12, eligibleCount: 8, errorCount: 2,
}), { tone: "warning", label: "有 2 个文件需要处理", detail: "已同步 12 张卡片，8 张可用于正式检索" });

assert.equal(knowledgeSyncSummaryText({
  vaultPath: "C:/Vault", inserted: 3, updated: 2, unchanged: 5,
  deactivated: 1, eligibleCount: 7, errorCount: 0, syncedAt: "2026-07-22 10:00:00",
}), "同步完成：新增 3，更新 2，未变化 5，停用 1");

assert.equal(friendlyKnowledgeError({ message: "permission denied" }), "知识库无法读取，请检查文件夹权限后重试");
console.log("knowledge frontend tests passed");
```

- [ ] **Step 2: Run test and verify RED**

```powershell
node --loader ts-node/esm src/lib/knowledge.test.ts
```

Expected: module or export not found.

- [ ] **Step 3: Implement DTOs and presentation functions**

Create `src/lib/knowledge.ts` with camelCase DTOs matching Rust serialization:

```ts
export type KnowledgeStatus = {
  connected: boolean; vaultPath: string; status: "disconnected" | "ready" | "degraded";
  lastSyncedAt: string | null; activeCount: number; eligibleCount: number; errorCount: number;
};
export type KnowledgeSyncSummary = {
  vaultPath: string; inserted: number; updated: number; unchanged: number; deactivated: number;
  eligibleCount: number; errorCount: number; syncedAt: string;
};
export type VaultInspection = {
  path: string; valid: boolean; hasObsidianConfig: boolean;
  markdownCount: number; missingDirectories: string[];
};
export type KnowledgeStatusPresentation = {
  tone: "neutral" | "success" | "warning"; label: string; detail: string;
};
```

Implement exact labels from the test. `friendlyKnowledgeError` must map permission errors, missing paths and generic objects without ever showing `[object Object]`.

Add `knowledge_vault_path: string` to `Config` in `src/lib/interface.ts` and initialize it to `""` in `Setting.svelte`'s local fallback model.

Add to `package.json`:

```json
"test:knowledge": "node --loader ts-node/esm src/lib/knowledge.test.ts"
```

- [ ] **Step 4: Run test and verify GREEN**

```powershell
npm run test:knowledge
```

Expected: `knowledge frontend tests passed` and exit 0.

- [ ] **Step 5: Commit Task 4**

```powershell
git add package.json src/lib/interface.ts src/lib/knowledge.ts src/lib/knowledge.test.ts src/page/Setting.svelte
git commit -m "feat: add knowledge connection status model"
```

---

### Task 5: Beginner-friendly Settings UI

**Files:**
- Create: `src/lib/components/settings/KnowledgeVaultSettings.svelte`
- Create: `scripts/check-knowledge-settings.mjs`
- Modify: `src/page/Setting.svelte`
- Modify: `package.json`

**Interfaces:**
- Consumes commands: `get_knowledge_status`, `inspect_knowledge_vault`, `connect_knowledge_vault`, `sync_knowledge_vault`, `open_knowledge_vault`
- Consumes helpers from `src/lib/knowledge.ts`
- Produces no global state; the component owns loading, status, error and last summary.

- [ ] **Step 1: Write a failing component contract check**

Create `scripts/check-knowledge-settings.mjs`:

```js
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const component = readFileSync("src/lib/components/settings/KnowledgeVaultSettings.svelte", "utf8");
const setting = readFileSync("src/page/Setting.svelte", "utf8");

for (const command of [
  "get_knowledge_status", "inspect_knowledge_vault", "connect_knowledge_vault",
  "sync_knowledge_vault", "open_knowledge_vault",
]) assert.match(component, new RegExp(`invoke(?:<[^>]+>)?\\(\\s*[\"']${command}[\"']`));

for (const label of ["Obsidian 知识库", "选择知识库", "重新同步", "打开文件夹"])
  assert.match(component, new RegExp(label));

assert.match(setting, /<KnowledgeVaultSettings\s*\/>/);
console.log("knowledge settings contract passed");
```

Add `"test:knowledge-settings": "node scripts/check-knowledge-settings.mjs"` to `package.json`.

- [ ] **Step 2: Run check and verify RED**

```powershell
npm run test:knowledge-settings
```

Expected: ENOENT because the component does not exist.

- [ ] **Step 3: Implement the settings component**

The component must use `open({ directory: true, multiple: false })` and this interaction order:

```ts
async function chooseVault(): Promise<void> {
  const selected = await open({ directory: true, multiple: false });
  const vaultPath = Array.isArray(selected) ? selected[0] : selected;
  if (!vaultPath) return;
  inspection = await invoke<VaultInspection>("inspect_knowledge_vault", { vaultPath });
  if (!inspection.valid) throw new Error("所选文件夹不是可用的知识库");
  lastSummary = await invoke<KnowledgeSyncSummary>("connect_knowledge_vault", { vaultPath });
  status = await invoke<KnowledgeStatus>("get_knowledge_status");
}

async function syncVault(): Promise<void> {
  lastSummary = await invoke<KnowledgeSyncSummary>("sync_knowledge_vault");
  status = await invoke<KnowledgeStatus>("get_knowledge_status");
}
```

Required UI states:

- Initial loading: fixed-height row with spinner and `正在读取知识库状态`.
- Disconnected: folder icon, explanation, primary `选择知识库` button.
- Ready: green status dot, Vault path, active/eligible counts, `重新同步` and folder icon button with `title="打开文件夹"`.
- Degraded: amber status dot, error count, `查看需要处理的文件` text; P1 displays count only and does not create a conflict screen.
- Inspection: when `missingDirectories` is non-empty, list the missing directory names and explain `当前阶段只读取，不会自动创建目录`; connection remains allowed.
- Action running: disable all buttons and keep dimensions stable.
- Success: display `knowledgeSyncSummaryText(lastSummary)` in a non-modal status line.
- Failure: display `friendlyKnowledgeError(error)` inline; do not call `alert`.

Use Lucide `Database`, `FolderOpen`, `RefreshCw`, `CheckCircle2`, `AlertTriangle`, and `Loader2`. Keep card radius at `8px`, avoid nested cards, and ensure paths wrap with `break-all`.

- [ ] **Step 4: Mount the component in Setting**

Import and place it immediately after the existing storage settings block:

```svelte
<script lang="ts">
  import KnowledgeVaultSettings from "../lib/components/settings/KnowledgeVaultSettings.svelte";
</script>

<KnowledgeVaultSettings />
```

Do not move unrelated settings or restyle the full page.

- [ ] **Step 5: Run focused checks and production build**

```powershell
npm run test:knowledge
npm run test:knowledge-settings
npm run build
```

Expected: both focused tests pass; Vite build exits 0. Existing unrelated accessibility/chunk warnings may remain, but no new warning may reference `KnowledgeVaultSettings.svelte`.

- [ ] **Step 6: Commit Task 5**

```powershell
git add package.json scripts/check-knowledge-settings.mjs src/lib/components/settings/KnowledgeVaultSettings.svelte src/page/Setting.svelte
git commit -m "feat: add Obsidian knowledge settings"
```

---

### Task 6: Real Vault read-only smoke test and recovery proof

**Files:**
- Modify: `src-tauri/crates/knowledge/tests/vault_scan.rs`
- Create: `C:\Users\10230\Documents\Codex\Workspace\01-Projects\Project-003-直播切片分析系统\05-测试与验收\2026-07-22-Obsidian-P1验收记录.md`
- Modify: `C:\Users\10230\Documents\Codex\Workspace\01-Projects\Project-003-直播切片分析系统\00-项目总览.md`

**Interfaces:**
- Consumes environment variable: `OBSIDIAN_TEST_VAULT`
- Proves the scanner can read the existing Project-001 sample Vault without modifying it.

- [ ] **Step 1: Add an ignored external Vault smoke test**

Add to `vault_scan.rs`:

```rust
#[test]
#[ignore = "requires OBSIDIAN_TEST_VAULT"]
fn scans_external_vault_without_modifying_files() {
    let path = std::env::var_os("OBSIDIAN_TEST_VAULT").expect("OBSIDIAN_TEST_VAULT");
    let root = std::path::PathBuf::from(path);
    let before = directory_fingerprint(&root);
    let scan = scan_vault(&root).unwrap();
    let after = directory_fingerprint(&root);
    assert!(!scan.documents.is_empty());
    assert!(scan.documents.iter().any(|item| item.status == "pending_review"));
    assert_eq!(before, after);
}
```

`directory_fingerprint` must recursively hash sorted `(relative_path, file_size, modified_time)` tuples without reading or writing outside the supplied root.

- [ ] **Step 2: Run the real sample Vault smoke test**

```powershell
$env:OBSIDIAN_TEST_VAULT='C:\Users\10230\Documents\Codex\Workspace\01-Projects\Project-001-直播电商知识库样板\06-交付物\佳能小白兔知识库样板'
cd src-tauri
cargo test -p knowledge scans_external_vault_without_modifying_files -- --ignored --nocapture
```

Expected: 1 ignored test is explicitly selected and passes; before/after fingerprints match.

- [ ] **Step 3: Prove SQLite can be rebuilt**

Add and run a focused database test that syncs a two-card scan, drops both knowledge tables, reruns `KNOWLEDGE_MIGRATION_SQL`, syncs the same scan, and asserts identical `active_count`, `eligible_count`, and `error_count`.

```powershell
cd src-tauri
cargo test rebuilds_snapshot_from_vault_scan
```

Expected: 1 test passes.

- [ ] **Step 4: Run full P1 verification**

```powershell
cd src-tauri
cargo fmt --all -- --check
cargo test -p knowledge
cargo test knowledge_sync_tests
cargo test knowledge_handler_tests
cd ..
npm run test:knowledge
npm run test:knowledge-settings
npm run build
git diff --check
```

Expected: all focused tests pass, production build exits 0, and `git diff --check` reports no whitespace errors. If full `cargo check --features gui` is blocked by the existing Windows V8 environment, record the exact command and failure separately; do not reinterpret it as a P1 test failure.

- [ ] **Step 5: Perform desktop visual verification**

Start the existing Tauri dev command, open Settings, and verify at 1280x720 and 1920x1080:

1. The knowledge section is visible without overlapping adjacent settings.
2. The disconnected, syncing, ready and degraded states preserve layout dimensions.
3. Long Chinese Vault paths wrap and buttons remain readable.
4. Selecting the real sample Vault shows active, eligible and error counts.
5. `打开文件夹` opens only the configured Vault.

Capture screenshots into the Project-003 `05-测试与验收` directory.

- [ ] **Step 6: Record acceptance evidence**

Create the acceptance record with these sections and actual command results:

```markdown
# Obsidian P1 验收记录

## 范围
Vault 连接、只读扫描、SQLite 增量索引、设置页状态。

## 自动测试
记录每条命令、退出码、通过数量和已知基线警告。

## 真实 Vault
记录扫描总数、可检索数量、解析失败数量和前后指纹一致性。

## 桌面验收
记录两个视口、状态切换、路径换行和打开文件夹结果。

## 结论
明确通过、未通过或带条件通过，不使用模糊表述。
```

Update Project-003 overview: last update `2026-07-22`, append milestone `Obsidian P1：Vault 连接与只读同步`, and record the unique source commit.

- [ ] **Step 7: Commit Task 6 source changes**

Commit only repository files; Workspace acceptance records remain under Project-003 governance:

```powershell
git add src-tauri/crates/knowledge/tests/vault_scan.rs
git commit -m "test: verify read-only Obsidian vault sync"
```

---

## P1 Exit Gate

P1 is complete only when all conditions hold:

- The user can connect and reopen a Vault from Settings.
- Scanning does not change any Vault file or directory.
- Repeated sync is idempotent and removed files become inactive in SQLite.
- Invalid Markdown is isolated and visible in counts without blocking valid cards.
- SQLite knowledge tables can be deleted and rebuilt from the same Vault scan.
- Only `approved`/`imported` cards (plus compatible Chinese values) with stable IDs are eligible.
- The desktop UI passes both target viewport checks.
- Verification evidence is stored under Project-003.

P1 intentionally reports missing standard directories without creating them. Directory creation occurs only in P2 after the user confirms the write-enabled connection step.

P2 planning starts only after this exit gate. P2 will add the persistent write queue,逐条勾选、入库预览、原子 Markdown 写入和冲突副本；P3 will connect only eligible indexed cards to ASR and high光 analysis with citation display.
