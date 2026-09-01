use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::database::anchor_knowledge::{
    AnchorKnowledgeAssetDetail, AnchorKnowledgeAssetSummary, AnchorKnowledgeProfile,
    CreateAnchorKnowledgeCandidateRequest, CreateAnchorKnowledgeProfileRequest,
    GetAnchorKnowledgeAssetRequest, ReviewAnchorKnowledgeAssetRequest,
    SearchAnchorKnowledgeRequest, SubmitAnchorKnowledgeAssetRequest,
};
use crate::knowledge_writer::{write_verified_markdown, WriteReceipt};
use crate::state::State;
use crate::state_type;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

fn configured_vault(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("尚未连接公司知识库；审核发布前请先在设置中连接 Vault".into());
    }
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| "公司知识库目录不可用，请重新连接".to_string())?;
    if !canonical.is_dir() {
        return Err("公司知识库目录不可用，请重新连接".into());
    }
    Ok(canonical)
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn asset_directory(asset_type: &str) -> Result<&'static str, String> {
    match asset_type {
        "speech" => Ok("已发布话术"),
        "deal_clip" => Ok("成交切片索引"),
        "analysis_advice" => Ok("分析建议"),
        _ => Err("知识资产类型不受支持".into()),
    }
}

fn published_relative_path(
    profile: &AnchorKnowledgeProfile,
    detail: &AnchorKnowledgeAssetDetail,
) -> Result<PathBuf, String> {
    Ok(PathBuf::from(&profile.vault_relative_root)
        .join(asset_directory(&detail.asset.asset_type)?)
        .join(format!(
            "{}-v{}.md",
            detail.asset.asset_id, detail.asset.version
        )))
}

fn render_published_markdown(
    detail: &AnchorKnowledgeAssetDetail,
    reviewer_id: &str,
    review_reason: &str,
) -> String {
    let source_ids = detail
        .sources
        .iter()
        .map(|source| yaml_string(&source.source_id))
        .collect::<Vec<_>>()
        .join(", ");
    let sources = detail
        .sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let time_range = match (source.start_ms, source.end_ms) {
                (Some(start), Some(end)) => format!("；时间码：{start}–{end} ms"),
                _ => String::new(),
            };
            let transcript = if source.transcript_version.trim().is_empty() {
                String::new()
            } else {
                format!(
                    "；逐字稿版本：{}；逐字稿哈希：{}",
                    source.transcript_version, source.transcript_hash
                )
            };
            let product_fact = if source.product_fact_id.trim().is_empty() {
                String::new()
            } else {
                format!(
                    "；商品事实：{}@{}",
                    source.product_fact_id, source.product_fact_version
                )
            };
            format!(
                "{}. `{}`（{}）{}{}{}；来源哈希：`{}`",
                index + 1,
                source.source_locator,
                source.source_kind,
                time_range,
                transcript,
                product_fact,
                source.content_hash
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\nid: {}\ntype: anchor_knowledge_asset\nstatus: anchor_published\nreview_status: published\nanchor_id: {}\nanchor_name: {}\nasset_type: {}\nversion: {}\nproduct_id: {}\ncontent_hash: {}\nreviewed_by: {}\nreview_reason: {}\nsource_ids: [{}]\n---\n\n# {}\n\n{}\n\n## 来源引用\n\n{}\n\n> 本资产经过主播人工审核。商品参数只引用公司已审核事实版本，本文件不能覆盖公司事实。\n",
        yaml_string(&detail.asset.asset_id),
        yaml_string(&detail.asset.anchor_id),
        yaml_string(&detail.asset.anchor_name),
        yaml_string(&detail.asset.asset_type),
        detail.asset.version,
        yaml_string(&detail.asset.product_id),
        yaml_string(&detail.asset.content_hash),
        yaml_string(reviewer_id.trim()),
        yaml_string(review_reason.trim()),
        source_ids,
        detail.asset.title.trim(),
        detail.asset.body.trim(),
        sources
    )
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

async fn write_or_verify_published_markdown(
    vault: &Path,
    relative_path: &Path,
    content: &str,
) -> Result<WriteReceipt, String> {
    match write_verified_markdown(vault, relative_path, true, content).await {
        Ok(receipt) => Ok(receipt),
        Err(original_error) => {
            let absolute = vault.join(relative_path);
            let existing = tokio::fs::read(&absolute)
                .await
                .map_err(|_| original_error.clone())?;
            if existing != content.as_bytes() {
                return Err("目标发布文件已存在不同内容，禁止覆盖".into());
            }
            Ok(WriteReceipt {
                relative_path: relative_path
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
                byte_len: existing.len() as u64,
                sha256: sha256(&existing),
            })
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn create_anchor_knowledge_profile(
    state: state_type!(),
    request: CreateAnchorKnowledgeProfileRequest,
) -> Result<AnchorKnowledgeProfile, String> {
    state
        .db
        .create_anchor_knowledge_profile(request)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_anchor_knowledge_profiles(
    state: state_type!(),
) -> Result<Vec<AnchorKnowledgeProfile>, String> {
    state
        .db
        .list_anchor_knowledge_profiles()
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn create_anchor_knowledge_candidate(
    state: state_type!(),
    request: CreateAnchorKnowledgeCandidateRequest,
) -> Result<AnchorKnowledgeAssetDetail, String> {
    state
        .db
        .create_anchor_knowledge_candidate(request)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn submit_anchor_knowledge_asset(
    state: state_type!(),
    request: SubmitAnchorKnowledgeAssetRequest,
) -> Result<AnchorKnowledgeAssetDetail, String> {
    state
        .db
        .submit_anchor_knowledge_asset(request)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn review_anchor_knowledge_asset(
    state: state_type!(),
    request: ReviewAnchorKnowledgeAssetRequest,
) -> Result<AnchorKnowledgeAssetDetail, String> {
    if request.decision.trim() == "reject" {
        return state
            .db
            .review_anchor_knowledge_asset(request, None, None)
            .await
            .map_err(String::from);
    }
    if request.decision.trim() != "publish" {
        return Err("审核决定必须是 publish 或 reject".into());
    }
    let detail = state
        .db
        .get_anchor_knowledge_asset(GetAnchorKnowledgeAssetRequest {
            requester_anchor_id: request.anchor_id.clone(),
            scope: "private".into(),
            asset_id: request.asset_id.clone(),
        })
        .await
        .map_err(String::from)?;
    if detail.asset.review_status != "pending_review" {
        return Err("仅待审核资产可以发布".into());
    }
    let profile = state
        .db
        .get_anchor_knowledge_profile(&request.anchor_id)
        .await
        .map_err(String::from)?;
    let vault_path = state.config.read().await.knowledge_vault_path.clone();
    let vault = configured_vault(&vault_path)?;
    let relative = published_relative_path(&profile, &detail)?;
    let content = render_published_markdown(&detail, &request.reviewer_id, &request.reason);
    let receipt = write_or_verify_published_markdown(&vault, &relative, &content).await?;
    state
        .db
        .review_anchor_knowledge_asset(request, Some(&receipt.relative_path), Some(&receipt.sha256))
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn search_anchor_knowledge(
    state: state_type!(),
    request: SearchAnchorKnowledgeRequest,
) -> Result<Vec<AnchorKnowledgeAssetSummary>, String> {
    state
        .db
        .search_anchor_knowledge(request)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_anchor_knowledge_asset(
    state: state_type!(),
    request: GetAnchorKnowledgeAssetRequest,
) -> Result<AnchorKnowledgeAssetDetail, String> {
    state
        .db
        .get_anchor_knowledge_asset(request)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn rebuild_anchor_knowledge_search_index(
    state: state_type!(),
    requester_anchor_id: String,
    owner_anchor_id: String,
) -> Result<i64, String> {
    state
        .db
        .rebuild_anchor_knowledge_search_index(&requester_anchor_id, &owner_anchor_id)
        .await
        .map_err(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::anchor_knowledge::{AnchorKnowledgeAssetSummary, AnchorKnowledgeSource};

    fn detail() -> AnchorKnowledgeAssetDetail {
        AnchorKnowledgeAssetDetail {
            asset: AnchorKnowledgeAssetSummary {
                asset_id: "asset-1".into(),
                anchor_id: "anchor-1".into(),
                anchor_name: "主播A".into(),
                asset_type: "speech".into(),
                title: "成交话术".into(),
                body: "先核对需求，再说明已确认事实。".into(),
                product_id: "product-1".into(),
                review_status: "pending_review".into(),
                version: 1,
                supersedes_asset_id: None,
                content_hash: "asset-hash".into(),
                is_current: false,
                published_relative_path: String::new(),
                published_file_hash: String::new(),
                created_at: "2026-08-31".into(),
                updated_at: "2026-08-31".into(),
                reviewed_at: None,
                reviewed_by: String::new(),
                review_reason: String::new(),
                published_at: None,
            },
            sources: vec![AnchorKnowledgeSource {
                source_id: "source-1".into(),
                source_kind: "transcript".into(),
                source_locator: "video:1".into(),
                video_id: Some(1),
                start_ms: Some(1000),
                end_ms: Some(3000),
                transcript_version: "v1".into(),
                transcript_hash: "transcript-hash".into(),
                product_fact_id: String::new(),
                product_fact_version: String::new(),
                analysis_version: String::new(),
                content_hash: "source-hash".into(),
            }],
        }
    }

    #[test]
    fn published_markdown_contains_auditable_citations_and_fact_boundary() {
        let markdown = render_published_markdown(&detail(), "reviewer-1", "checked");
        assert!(markdown.contains("review_status: published"));
        assert!(markdown.contains("video:1"));
        assert!(markdown.contains("1000–3000 ms"));
        assert!(markdown.contains("transcript-hash"));
        assert!(markdown.contains("不能覆盖公司事实"));
    }

    #[tokio::test]
    async fn existing_different_file_is_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let relative = Path::new("主播知识库/anchor-1/已发布话术/asset-1.md");
        let absolute = root.path().join(relative);
        std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        std::fs::write(&absolute, "old").unwrap();
        let error = write_or_verify_published_markdown(root.path(), relative, "new")
            .await
            .unwrap_err();
        assert!(error.contains("禁止覆盖"));
        assert_eq!(std::fs::read_to_string(absolute).unwrap(), "old");
    }
}
