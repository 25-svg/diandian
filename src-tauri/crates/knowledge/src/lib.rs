use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

const REQUIRED_DIRS: &[&str] = &[
    "01-产品事实",
    "02-别名与ASR纠错",
    "03-直播案例",
    "04-话术模块",
    "05-人群画像",
    "06-辅稿",
    "07-证据索引",
    "08-产品参数库",
];
const REVIEW_DIR_ALIASES: &[&str] = &["99-待审核", "09-审核逐字稿"];
const EXCLUDED_DIRS: &[&str] = &[".obsidian", "90-模板", "99-待处理冲突"];
const RESTRICTED_MESSAGE: &str = "检测到个人或受限信息，正文未进入索引";

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
    pub classification: String,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub relative_path: String,
    pub classification: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultScan {
    pub inspection: VaultInspection,
    pub documents: Vec<KnowledgeDocument>,
    pub issues: Vec<ScanIssue>,
}

pub fn inspect_vault(path: &Path) -> Result<VaultInspection, KnowledgeError> {
    if !path.is_dir() {
        return Err(KnowledgeError::InvalidVault);
    }

    let canonical_root = path.canonicalize()?;
    let has_obsidian_config = canonical_root.join(".obsidian").is_dir();
    let mut missing_directories = REQUIRED_DIRS
        .iter()
        .filter(|name| !canonical_root.join(name).is_dir())
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    if !REVIEW_DIR_ALIASES
        .iter()
        .any(|name| canonical_root.join(name).is_dir())
    {
        missing_directories.push("99-待审核（或 09-审核逐字稿）".to_string());
    }
    let has_known_directory = REQUIRED_DIRS
        .iter()
        .chain(REVIEW_DIR_ALIASES.iter())
        .any(|name| canonical_root.join(name).is_dir());
    if !has_obsidian_config && !has_known_directory {
        return Err(KnowledgeError::InvalidVault);
    }

    let markdown_count = markdown_entries(&canonical_root)?.len();
    Ok(VaultInspection {
        path: canonical_root.to_string_lossy().to_string(),
        valid: true,
        has_obsidian_config,
        markdown_count,
        missing_directories,
    })
}

pub fn parse_document(vault: &Path, path: &Path) -> KnowledgeDocument {
    let relative_path = relative_path(vault, path).unwrap_or_default();
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return invalid_document(
                relative_path,
                String::new(),
                format!("无法读取 Markdown：{error}"),
            )
        }
    };
    let content_hash = format!("{:x}", Sha256::digest(&bytes));
    let content = match std::str::from_utf8(&bytes) {
        Ok(content) => content,
        Err(_) => {
            return invalid_document(
                relative_path,
                content_hash,
                "Markdown 不是 UTF-8 编码".to_string(),
            )
        }
    };

    let (frontmatter, body) = match split_frontmatter(content) {
        Ok(parts) => parts,
        Err(message) if message == "缺少 YAML frontmatter" && is_ignored_note(&relative_path) => {
            return ignored_document(relative_path, content_hash)
        }
        Err(message) => return invalid_document(relative_path, content_hash, message),
    };
    let yaml = match serde_yaml::from_str::<Value>(frontmatter) {
        Ok(value) => value,
        Err(error) => {
            return invalid_document(
                relative_path,
                content_hash,
                format!("YAML frontmatter 无法解析：{error}"),
            )
        }
    };
    let metadata = match serde_json::to_value(&yaml) {
        Ok(value) => value,
        Err(error) => {
            return invalid_document(
                relative_path,
                content_hash,
                format!("YAML metadata 无法转换：{error}"),
            )
        }
    };

    let card_id = yaml_string(&yaml, "id");
    let title = yaml_string(&yaml, "title");
    let card_type = yaml_string(&yaml, "type");
    let status = yaml_string(&yaml, "status");
    let version = yaml_version(&yaml);
    let missing = [
        ("id", &card_id),
        ("title", &title),
        ("type", &card_type),
        ("status", &status),
    ]
    .into_iter()
    .filter(|(_, value)| value.is_empty())
    .map(|(name, _)| name)
    .collect::<Vec<_>>();

    let mut issue = if missing.is_empty() {
        None
    } else {
        Some(format!("缺少必填字段: {}", missing.join(", ")))
    };
    let restricted = is_restricted(&yaml, body);
    if restricted {
        issue = Some(RESTRICTED_MESSAGE.to_string());
    }

    let mut classification = if restricted {
        "restricted"
    } else if issue.is_some() {
        "invalid"
    } else if approved(&status) {
        "eligible"
    } else if pending_review(&status) {
        "pending_review"
    } else {
        issue = Some(format!("无法识别卡片状态: {status}"));
        "invalid"
    }
    .to_string();
    let eligible = classification == "eligible";

    if issue.is_some() && classification == "eligible" {
        classification = "invalid".to_string();
    }

    KnowledgeDocument {
        relative_path,
        card_id,
        title,
        card_type,
        status: status.clone(),
        version,
        content_hash,
        metadata,
        body: if restricted {
            String::new()
        } else {
            body.trim().to_string()
        },
        eligible,
        classification,
        issue,
    }
}

pub fn scan_vault(path: &Path) -> Result<VaultScan, KnowledgeError> {
    let inspection = inspect_vault(path)?;
    let canonical_root = PathBuf::from(&inspection.path).canonicalize()?;
    let mut documents = markdown_entries(&canonical_root)?
        .into_iter()
        .map(|entry| parse_document(&canonical_root, &entry))
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut id_counts = HashMap::<String, usize>::new();
    for document in &documents {
        if !document.card_id.is_empty() {
            *id_counts.entry(document.card_id.clone()).or_default() += 1;
        }
    }
    for document in &mut documents {
        if document.classification != "restricted"
            && id_counts
            .get(&document.card_id)
            .copied()
            .unwrap_or_default()
            > 1
        {
            document.issue = Some(format!("知识卡 ID 重复: {}", document.card_id));
            document.eligible = false;
            document.classification = "duplicate".to_string();
        }
    }

    let issues = documents
        .iter()
        .filter_map(|document| {
            document.issue.as_ref().map(|message| ScanIssue {
                relative_path: document.relative_path.clone(),
                classification: document.classification.clone(),
                message: message.clone(),
            })
        })
        .collect();

    Ok(VaultScan {
        inspection,
        documents,
        issues,
    })
}

fn markdown_entries(root: &Path) -> Result<Vec<PathBuf>, KnowledgeError> {
    let mut entries = Vec::new();
    for result in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_visit(root, entry))
    {
        let entry = result.map_err(|error| std::io::Error::other(error.to_string()))?;
        if !entry.file_type().is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let canonical_path = entry.path().canonicalize()?;
        if canonical_path.starts_with(root) {
            entries.push(canonical_path);
        }
    }
    entries.sort();
    Ok(entries)
}

fn should_visit(root: &Path, entry: &DirEntry) -> bool {
    entry.path() == root
        || entry
            .path()
            .strip_prefix(root)
            .is_ok_and(|relative| !is_excluded(relative))
}

fn is_excluded(relative: &Path) -> bool {
    relative.components().any(|part| {
        let value = part.as_os_str().to_string_lossy();
        value.starts_with('.') || EXCLUDED_DIRS.contains(&value.as_ref())
    })
}

fn relative_path(vault: &Path, path: &Path) -> Option<String> {
    let root = vault.canonicalize().ok()?;
    let file = path.canonicalize().ok()?;
    if !file.starts_with(&root) {
        return None;
    }
    Some(
        file.strip_prefix(root)
            .ok()?
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn split_frontmatter(content: &str) -> Result<(&str, &str), String> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    if let Some(rest) = normalized.strip_prefix("---\r\n") {
        let end = rest.find("\r\n---\r\n").ok_or("YAML frontmatter 未闭合")?;
        return Ok((&rest[..end], &rest[end + 7..]));
    }
    if let Some(rest) = normalized.strip_prefix("---\n") {
        let end = rest.find("\n---\n").ok_or("YAML frontmatter 未闭合")?;
        return Ok((&rest[..end], &rest[end + 5..]));
    }
    Err("缺少 YAML frontmatter".to_string())
}

fn yaml_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn yaml_version(value: &Value) -> String {
    value
        .get("version")
        .map(|item| match item {
            Value::String(value) => value.trim().to_string(),
            Value::Number(value) => value.to_string(),
            _ => "1".to_string(),
        })
        .unwrap_or_else(|| "1".to_string())
}

fn approved(status: &str) -> bool {
    matches!(status, "approved" | "imported" | "已审核" | "已入库")
}

fn pending_review(status: &str) -> bool {
    matches!(
        status,
        "pending_review" | "pending" | "draft" | "待审核" | "待确认"
    )
}

fn is_ignored_note(relative_path: &str) -> bool {
    let file_name = Path::new(relative_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let lower = file_name.to_ascii_lowercase();
    lower == "readme.md"
        || file_name == "来源说明.md"
        || file_name == "产品参数库.md"
        || file_name.ends_with("-品牌索引.md")
}

fn is_restricted(metadata: &Value, body: &str) -> bool {
    if matches!(
        yaml_string(metadata, "sensitivity").as_str(),
        "restricted" | "personal"
    ) {
        return true;
    }

    [
        r"(?:^|[^0-9])(1[3-9][0-9]{9})(?:$|[^0-9])",
        r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
        r"(?i)Authorization\s*:\s*Bearer",
        r"(?i)access_token\s*:",
        r"sk-[A-Za-z0-9]{20,}",
    ]
    .iter()
    .any(|pattern| Regex::new(pattern).is_ok_and(|regex| regex.is_match(body)))
}

fn invalid_document(
    relative_path: String,
    content_hash: String,
    message: String,
) -> KnowledgeDocument {
    KnowledgeDocument {
        relative_path,
        card_id: String::new(),
        title: String::new(),
        card_type: String::new(),
        status: String::new(),
        version: "1".to_string(),
        content_hash,
        metadata: serde_json::Value::Null,
        body: String::new(),
        eligible: false,
        classification: "invalid".to_string(),
        issue: Some(message),
    }
}

fn ignored_document(relative_path: String, content_hash: String) -> KnowledgeDocument {
    KnowledgeDocument {
        relative_path,
        card_id: String::new(),
        title: String::new(),
        card_type: String::new(),
        status: String::new(),
        version: "1".to_string(),
        content_hash,
        metadata: serde_json::Value::Null,
        body: String::new(),
        eligible: false,
        classification: "ignored".to_string(),
        issue: None,
    }
}
