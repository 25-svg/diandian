use crate::database::anchor_knowledge::{
    AnchorKnowledgeSourceInput, PublishedAnchorKnowledgeAssetInput,
};
use crate::database::master_sample_batch::{
    MasterSampleBatchDetail, MasterSampleBatchItemRow, MasterSampleBatchSummary,
    NewMasterSampleBatch,
};
use crate::database::master_script::{
    MasterChunkRow, MasterScriptRow, MasterSourceRow, NewMasterSection, NewMasterSource,
    NewMasterVersion,
};
use crate::handlers::video::resolve_video_transcript_context;
use crate::master_script::company_benchmark::select_company_benchmarks;
use crate::master_script::{builder, model};
use crate::master_script::{
    parameter_card_from_record, resume_master_ingest as run_resume_master_ingest,
    start_master_ingest as run_start_master_ingest, DatabaseCheckpointStore,
    TranscriptArtifactSink, VolcengineChunkTranscriber,
};
use crate::state::State;
use crate::state_type;
use chrono::Utc;
use master_builder::{
    validate_master_draft, BuilderIssue, MasterDraft, MasterDraftValidation, MasterSectionKind,
    ParameterFactCard, TranscriptCue,
};
use master_ingest::{
    select_parameter_cards, validate_resume_source, IngestRequest, IngestStatus, SourceIdentity,
    MAX_PARAMETER_CARDS,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;
use tokio::sync::Mutex;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterVideoIngestRequest {
    pub video_id: i64,
    pub title: String,
    #[serde(default)]
    pub recognized_terms: Vec<String>,
    #[serde(default)]
    pub manually_selected_card_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMasterSampleBatchRequest {
    pub title: String,
    #[serde(default)]
    pub host_label: String,
    #[serde(default = "default_master_sample_batch_purpose")]
    pub purpose: String,
    pub target_sample_count: i64,
    pub video_ids: Vec<i64>,
}

fn default_master_sample_batch_purpose() -> String {
    "sample".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishMasterSampleBatchRequest {
    pub batch_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishCorrectedMasterSampleBatchRequest {
    pub batch_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterPreviewRequest {
    pub source_id: i64,
    pub script_key: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishMasterRequest {
    pub source_id: i64,
    pub script_key: String,
    pub draft: MasterDraft,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterPreview {
    pub source_id: i64,
    pub script_key: String,
    pub draft: MasterDraft,
    pub validation: MasterDraftValidation,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterScriptStatus {
    pub source: MasterSourceRow,
    pub chunks: Vec<MasterChunkRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterIngestResponse {
    pub source_id: i64,
    pub status: IngestStatus,
}

static MASTER_SAMPLE_BATCH_RUNS: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
static MASTER_SAMPLE_BATCH_SYNTHESIS_RUNS: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn create_master_sample_batch(
    state: state_type!(),
    request: CreateMasterSampleBatchRequest,
) -> Result<MasterSampleBatchDetail, String> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err("请填写母稿样本批次名称".into());
    }
    let batch_key = format!("MSB-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    state
        .db
        .create_master_sample_batch(NewMasterSampleBatch {
            batch_key,
            title: title.to_string(),
            host_label: request.host_label.trim().to_string(),
            purpose: request.purpose.trim().to_string(),
            target_sample_count: request.target_sample_count,
            video_ids: request.video_ids,
        })
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_master_sample_batches(
    state: state_type!(),
) -> Result<Vec<MasterSampleBatchSummary>, String> {
    state
        .db
        .list_master_sample_batches()
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_master_sample_batch(
    state: state_type!(),
    batch_id: i64,
) -> Result<MasterSampleBatchDetail, String> {
    let mut detail = state
        .db
        .get_master_sample_batch(batch_id)
        .await
        .map_err(String::from)?;
    attach_correctable_model_format_issue_count(&state, &mut detail).await;
    Ok(detail)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn start_master_sample_batch_processing(
    state: state_type!(),
    batch_id: i64,
) -> Result<MasterSampleBatchDetail, String> {
    #[cfg(feature = "gui")]
    let state = state.inner().clone();
    #[cfg(feature = "headless")]
    let state = state.clone();
    queue_master_sample_batch_processing(state, batch_id).await
}

pub async fn resume_processing_master_sample_batches(state: State) {
    let batches = match state.db.list_master_sample_batches().await {
        Ok(batches) => batches,
        Err(error) => {
            log::error!("无法读取待恢复的母稿样本批次: {error}");
            return;
        }
    };
    for summary in batches
        .into_iter()
        .filter(|summary| summary.batch.status == "processing")
    {
        if let Err(error) =
            queue_master_sample_batch_processing(state.clone(), summary.batch.id).await
        {
            log::error!("无法恢复母稿样本批次 {}: {error}", summary.batch.id);
        }
    }
}

async fn queue_master_sample_batch_processing(
    state: State,
    batch_id: i64,
) -> Result<MasterSampleBatchDetail, String> {
    let active_runs = MASTER_SAMPLE_BATCH_RUNS.get_or_init(|| Mutex::new(HashSet::new()));
    {
        let mut running = active_runs.lock().await;
        if !running.insert(batch_id) {
            return Err("这个母稿样本批次已在后台处理中".into());
        }
    }

    let detail = match state
        .db
        .begin_master_sample_batch_processing(batch_id)
        .await
    {
        Ok(detail) => detail,
        Err(error) => {
            active_runs.lock().await.remove(&batch_id);
            return Err(error.into());
        }
    };

    tokio::spawn(async move {
        if let Err(error) = process_master_sample_batch(state, batch_id).await {
            log::error!("母稿样本批次 {batch_id} 处理异常: {error}");
        }
        if let Some(active_runs) = MASTER_SAMPLE_BATCH_RUNS.get() {
            active_runs.lock().await.remove(&batch_id);
        }
    });
    Ok(detail)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn generate_master_sample_batch_draft(
    state: state_type!(),
    batch_id: i64,
) -> Result<MasterSampleBatchDetail, String> {
    let active_runs = MASTER_SAMPLE_BATCH_SYNTHESIS_RUNS.get_or_init(|| Mutex::new(HashSet::new()));
    {
        let mut running = active_runs.lock().await;
        if !running.insert(batch_id) {
            return Err("这个批次正在生成最终母稿草稿，请勿重复提交".into());
        }
    }
    let result = async {
        let detail = state
            .db
            .begin_master_sample_batch_synthesis(batch_id)
            .await
            .map_err(String::from)?;
        let api_key = crate::handlers::ai::configured_minimax_api_key(&state).await?;
        let parameter_cards = state
            .db
            .list_asr_parameter_cards()
            .await
            .map_err(String::from)?
            .into_iter()
            .map(|record| ParameterFactCard {
                card_id: record.card_id,
                static_terms: vec![record.title],
            })
            .collect::<Vec<_>>();
        let mut evidence = Vec::new();
        for item in &detail.items {
            let source_id = item
                .source_id
                .ok_or_else(|| format!("样本《{}》缺少已完成的转写来源", item.video_title))?;
            let truth = load_master_truth(&state, source_id).await?;
            if truth.pending_critical_count > 0 {
                return Err(format!(
                    "样本《{}》仍有 {} 条关键逐字稿待人工确认，请先完成校正",
                    item.video_title, truth.pending_critical_count
                ));
            }
            evidence.extend(select_batch_evidence(
                item.video_id,
                &item.video_title,
                &truth.transcript,
            ));
        }
        let benchmark_records = state
            .db
            .list_eligible_documents(&["company_deal_benchmark".to_string()])
            .await
            .map_err(String::from)?;
        let benchmark_corpus = format!(
            "{}\n{}",
            detail.batch.title,
            evidence
                .iter()
                .map(|item| item.quote.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let company_benchmarks = select_company_benchmarks(&benchmark_records, &benchmark_corpus);
        let draft = model::generate_batch_master_draft(
            &api_key,
            &format!("{} · {}", detail.batch.title, detail.batch.host_label),
            evidence,
            &parameter_cards,
            &company_benchmarks,
        )
        .await?;
        let draft_json = serde_json::to_string(&draft).map_err(|error| error.to_string())?;
        state
            .db
            .finish_master_sample_batch_synthesis(batch_id, &draft_json)
            .await
            .map_err(String::from)
    }
    .await;
    if let Err(error) = &result {
        let _ = state
            .db
            .fail_master_sample_batch_synthesis(batch_id, error)
            .await;
    }
    active_runs.lock().await.remove(&batch_id);
    result
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn publish_master_sample_batch_draft(
    state: state_type!(),
    request: PublishMasterSampleBatchRequest,
) -> Result<MasterScriptRow, String> {
    let detail = state
        .db
        .get_master_sample_batch(request.batch_id)
        .await
        .map_err(String::from)?;
    if detail.batch.status != "draft_ready"
        || detail.synthesis.as_ref().map(|item| item.status.as_str()) != Some("ready")
    {
        return Err("请先生成完成并核对最终母稿草稿，再发布企业母稿".into());
    }
    let host_label = resolve_batch_host_label(&state, &detail).await?;
    let anchor_profile = state
        .db
        .ensure_anchor_knowledge_profile(&host_label)
        .await
        .map_err(String::from)?;
    let kb_root = PathBuf::from(&anchor_profile.vault_relative_root);
    let draft_json = detail
        .synthesis
        .as_ref()
        .and_then(|item| item.draft_json.as_deref())
        .ok_or_else(|| "最终母稿草稿内容不存在，请重新生成".to_string())?;
    let draft = serde_json::from_str::<model::BatchMasterDraft>(draft_json)
        .map_err(|error| format!("最终母稿草稿无法读取：{error}"))?;
    if draft.sections.is_empty() {
        return Err("最终母稿草稿没有可发布的章节".into());
    }
    let source_id = detail
        .items
        .iter()
        .find_map(|item| item.source_id)
        .ok_or_else(|| "母稿样本缺少可追溯逐字稿来源".to_string())?;
    let script_key = builder::safe_key(&format!("MS-BATCH-{}", detail.batch.id))?;
    let vault = configured_vault(&state).await?;
    let speech_base = kb_root.join("话术").join("V1.0");
    let index_relative = speech_base.join("README.md");
    let index = render_batch_master_index(&script_key, &detail, &draft);
    let sections = batch_draft_sections(&detail, &draft)?;
    let mut files = vec![(index_relative.clone(), index.clone())];
    for (section, row) in draft.sections.iter().zip(&sections) {
        files.push((
            speech_base.join(format!("{:02}-{}.md", row.position, row.section_key)),
            render_batch_master_section(&script_key, row, section),
        ));
    }
    let analysis_relative = kb_root.join("分析建议").join("执行规则.md");
    let analysis_content = render_host_analysis_advice(&host_label, &detail, &draft);
    files.push((analysis_relative.clone(), analysis_content.clone()));
    let video_paths = load_batch_video_paths(&state, &draft).await;
    let deal_files = render_host_deal_video_refs(&kb_root, &host_label, &draft, &video_paths);
    files.extend(deal_files.iter().cloned());
    let mut created = Vec::<PathBuf>::new();
    for (relative, content) in files {
        if let Err(error) = write_new_master_file(&vault, &relative, &content, &mut created).await {
            cleanup_created_files(&vault, &created).await;
            return Err(error);
        }
    }
    let published = state
        .db
        .publish_master_version(NewMasterVersion {
            script_key: script_key.clone(),
            version: "1.0.0".into(),
            source_id,
            title: draft.title.clone(),
            index_relative_path: normalized_path(&index_relative),
            content_hash: builder::content_hash(&index),
            sections,
        })
        .await;
    let published = match published {
        Ok(value) => value,
        Err(error) => {
            cleanup_created_files(&vault, &created).await;
            return Err(error.to_string());
        }
    };
    state
        .db
        .mark_master_sample_batch_published(request.batch_id)
        .await
        .map_err(String::from)?;
    if let Err(error) = index_published_master_assets(
        &state,
        &anchor_profile.anchor_id,
        &detail,
        &draft,
        1,
        &index_relative,
        &index,
        &analysis_relative,
        &analysis_content,
        &deal_files,
    )
    .await
    {
        log::error!(
            "母稿批次 {} 已发布，但主播知识库索引写入失败: {}",
            request.batch_id,
            error
        );
    }
    Ok(published)
}

/// Produces a new immutable master version from the published version after
/// applying only deterministic transcript corrections. Ambiguous wording is
/// deliberately left untouched for a human reviewer.
#[cfg_attr(feature = "gui", tauri::command)]
pub async fn publish_corrected_master_sample_batch_version(
    state: state_type!(),
    request: PublishCorrectedMasterSampleBatchRequest,
) -> Result<MasterScriptRow, String> {
    let detail = state
        .db
        .get_master_sample_batch(request.batch_id)
        .await
        .map_err(String::from)?;
    if detail.batch.status != "published" {
        return Err("请先发布 V1.0 母稿，再生成校正版本".into());
    }
    let host_label = resolve_batch_host_label(&state, &detail).await?;
    let anchor_profile = state
        .db
        .ensure_anchor_knowledge_profile(&host_label)
        .await
        .map_err(String::from)?;
    let kb_root = PathBuf::from(&anchor_profile.vault_relative_root);
    let script_key = builder::safe_key(&format!("MS-BATCH-{}", detail.batch.id))?;
    let previous = state
        .db
        .get_latest_published_master(&script_key)
        .await
        .map_err(String::from)?;
    if previous.version != "1.0.0" {
        return Err("当前母稿已存在校正版本，请在新版本的审核流程中继续修订".into());
    }
    let existing_sections = state
        .db
        .list_master_sections(previous.id)
        .await
        .map_err(String::from)?;
    let mut correction_count = 0usize;
    let sections = existing_sections
        .iter()
        .map(|section| {
            let host_text = crate::subtitle_generator::transcript_artifacts::normalize_letter_prefixed_model_numbers(&section.host_text);
            let master_text = crate::subtitle_generator::transcript_artifacts::normalize_letter_prefixed_model_numbers(&section.master_text);
            correction_count += usize::from(has_deterministic_model_format_correction(
                &section.host_text,
                &section.master_text,
            ));
            Ok(NewMasterSection {
                section_key: section.section_key.clone(),
                position: section.position,
                section_kind: parse_stored_section_kind(&section.section_kind)?,
                product_card_id: section.product_card_id.clone(),
                title: section.title.clone(),
                source_start_ms: section.source_start_ms,
                source_end_ms: section.source_end_ms,
                host_text,
                master_text,
                metadata_json: corrected_section_metadata(&section.metadata_json, previous.id)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if correction_count == 0 {
        return Err(
            "未发现可确定的型号格式错误；不确定内容请在人审后修订，系统不会擅自改写主播原话".into(),
        );
    }

    let vault = configured_vault(&state).await?;
    let speech_base = kb_root.join("话术").join("V1.1");
    let index_relative = speech_base.join("README.md");
    let index =
        render_corrected_batch_master_index(&detail, &previous, &sections, correction_count);
    let mut files = vec![(index_relative.clone(), index.clone())];
    for section in &sections {
        files.push((
            speech_base.join(format!(
                "{:02}-{}.md",
                section.position, section.section_key
            )),
            render_corrected_batch_master_section(&script_key, section),
        ));
    }
    let analysis_relative = kb_root.join("分析建议").join("校正说明-V1.1.md");
    let analysis_content = format!(
            "# 分析建议（校正版）\n\n- 主播：{}\n- 样本批次：{}\n- 版本：V1.1\n\n## 本次校正\n\n- 已应用 {} 处可确定的型号格式校正。\n- 未能由规则验证的内容保持原样，避免擅自改写主播原话。\n",
            host_label,
            detail.batch.title.trim(),
            correction_count
        );
    files.push((analysis_relative.clone(), analysis_content.clone()));
    let mut created = Vec::<PathBuf>::new();
    for (relative, content) in files {
        if let Err(error) = write_new_master_file(&vault, &relative, &content, &mut created).await {
            cleanup_created_files(&vault, &created).await;
            return Err(error);
        }
    }
    let published = state
        .db
        .publish_master_version(NewMasterVersion {
            script_key,
            version: "1.1.0".into(),
            source_id: previous.source_id,
            title: format!("{}（逐字稿校正）", previous.title),
            index_relative_path: normalized_path(&index_relative),
            content_hash: builder::content_hash(&index),
            sections,
        })
        .await;
    match published {
        Ok(master) => {
            let source = master_version_source(previous.id, &previous.content_hash, "1.0.0");
            if let Err(error) = index_corrected_master_assets(
                &state,
                &anchor_profile.anchor_id,
                &detail,
                &index_relative,
                &index,
                &analysis_relative,
                &analysis_content,
                source,
            )
            .await
            {
                log::error!(
                    "母稿批次 {} 的校正版已发布，但主播知识库索引写入失败: {}",
                    request.batch_id,
                    error
                );
            }
            Ok(master)
        }
        Err(error) => {
            cleanup_created_files(&vault, &created).await;
            Err(error.to_string())
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_unbatched_master_source_video_ids(
    state: state_type!(),
) -> Result<Vec<i64>, String> {
    state
        .db
        .list_unbatched_master_source_video_ids()
        .await
        .map_err(String::from)
}

async fn process_master_sample_batch(state: State, batch_id: i64) -> Result<(), String> {
    let detail = state
        .db
        .get_master_sample_batch(batch_id)
        .await
        .map_err(String::from)?;
    for item in detail.items {
        if item.processing_status == "ready" {
            continue;
        }
        state
            .db
            .update_master_sample_batch_item(
                batch_id,
                item.id,
                item.source_id,
                "transcribing",
                None,
            )
            .await
            .map_err(String::from)?;
        match process_master_sample_batch_item(&state, batch_id, &item).await {
            Ok(source_id) => state
                .db
                .update_master_sample_batch_item(batch_id, item.id, Some(source_id), "ready", None)
                .await
                .map_err(String::from)?,
            Err(error) => {
                log::warn!(
                    "母稿样本批次 {batch_id} 的视频 {} 处理失败: {error}",
                    item.video_id
                );
                state
                    .db
                    .update_master_sample_batch_item(
                        batch_id,
                        item.id,
                        item.source_id,
                        "failed",
                        Some(&error),
                    )
                    .await
                    .map_err(String::from)?;
            }
        }
    }
    state
        .db
        .finish_master_sample_batch_processing(batch_id)
        .await
        .map_err(String::from)?;
    Ok(())
}

async fn process_master_sample_batch_item(
    state: &State,
    batch_id: i64,
    item: &MasterSampleBatchItemRow,
) -> Result<i64, String> {
    let request = MasterVideoIngestRequest {
        video_id: item.video_id,
        title: item.video_title.clone(),
        recognized_terms: Vec::new(),
        manually_selected_card_ids: Vec::new(),
    };
    let prepared = prepare_video_ingest(state, &request).await?;
    let existing_source_id = match item.source_id {
        Some(source_id) => Some(source_id),
        None => state
            .db
            .find_master_source_id_for_video(item.video_id)
            .await
            .map_err(String::from)?,
    };
    if let Some(source_id) = existing_source_id {
        let chunks = state
            .db
            .list_master_chunks(source_id)
            .await
            .map_err(String::from)?;
        if !chunks.is_empty() && chunks.iter().all(|chunk| chunk.status == "complete") {
            return Ok(source_id);
        }
        let source = state
            .db
            .get_master_source(source_id)
            .await
            .map_err(String::from)?;
        validate_resume_source(
            &SourceIdentity {
                source_key: source.source_key,
                media_path: source.media_path,
                media_hash: source.media_hash,
                duration_ms: u64::try_from(source.duration_ms)
                    .map_err(|error| error.to_string())?,
            },
            &prepared.identity(),
        )?;
        state
            .db
            .update_master_sample_batch_item(
                batch_id,
                item.id,
                Some(source_id),
                "transcribing",
                None,
            )
            .await
            .map_err(String::from)?;
        let status = run_resume_master_ingest(
            source_id,
            prepared.request(source_id),
            DatabaseCheckpointStore::new(state.db.clone()),
            prepared.transcriber,
            prepared.artifacts,
        )
        .await?;
        if let IngestStatus::Failed { error, .. } = status {
            return Err(error);
        }
        return Ok(source_id);
    }

    let source = state
        .db
        .create_master_source(NewMasterSource {
            source_kind: "video".into(),
            source_key: prepared.source_key.clone(),
            media_path: prepared.media_path.clone(),
            media_hash: prepared.media_hash.clone(),
            duration_ms: i64::try_from(prepared.duration_ms).map_err(|error| error.to_string())?,
        })
        .await
        .map_err(String::from)?;
    state
        .db
        .update_master_sample_batch_item(batch_id, item.id, Some(source.id), "transcribing", None)
        .await
        .map_err(String::from)?;
    let status = run_start_master_ingest(
        prepared.request(source.id),
        DatabaseCheckpointStore::new(state.db.clone()),
        prepared.transcriber,
        prepared.artifacts,
    )
    .await?;
    if let IngestStatus::Failed { error, .. } = status {
        return Err(error);
    }
    Ok(source.id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterBaseline {
    pub master: MasterScriptRow,
    pub sections: Vec<crate::database::master_script::MasterSectionRow>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDealRefinementCandidatesRequest {
    pub script_key: String,
    pub expected_master_script_id: i64,
    pub source: crate::subtitle_generator::transcript_artifacts::TranscriptSource,
    pub payload_json: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DealRefinementPayload {
    #[serde(default)]
    unique_by_product: Vec<DealRefinementCandidate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DealRefinementCandidate {
    #[serde(default)]
    order_id: String,
    #[serde(default)]
    product_name: String,
    #[serde(default)]
    verification_tier: String,
    #[serde(default)]
    speech_start_sec: f64,
    #[serde(default)]
    speech_end_sec: f64,
    #[serde(default)]
    speech_text: String,
    #[serde(default)]
    master_section_id: Option<i64>,
    #[serde(default)]
    master_product_card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDealRefinementCandidatesResult {
    pub imported: usize,
    pub skipped_weak: usize,
    pub skipped_invalid: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterUpgradeRequest {
    pub script_key: String,
    pub candidate_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterUpgradeSection {
    pub section_id: i64,
    pub section_key: String,
    pub current_text: String,
    pub next_text: String,
    pub candidate_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterUpgradePreview {
    pub base_master_id: i64,
    pub script_key: String,
    pub current_version: String,
    pub next_version: String,
    pub sections: Vec<MasterUpgradeSection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishMasterUpgradeRequest {
    pub script_key: String,
    pub candidate_ids: Vec<i64>,
    pub diff_confirmed: bool,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn start_master_ingest(
    state: state_type!(),
    request: MasterVideoIngestRequest,
) -> Result<MasterIngestResponse, String> {
    let prepared = prepare_video_ingest(&state, &request).await?;
    let source = state
        .db
        .create_master_source(NewMasterSource {
            source_kind: "video".into(),
            source_key: prepared.source_key.clone(),
            media_path: prepared.media_path.clone(),
            media_hash: prepared.media_hash.clone(),
            duration_ms: i64::try_from(prepared.duration_ms).map_err(|error| error.to_string())?,
        })
        .await
        .map_err(String::from)?;
    let ingest_request = prepared.request(source.id);
    let status = run_start_master_ingest(
        ingest_request,
        DatabaseCheckpointStore::new(state.db.clone()),
        prepared.transcriber,
        prepared.artifacts,
    )
    .await?;
    Ok(MasterIngestResponse {
        source_id: source.id,
        status,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resume_master_ingest(
    state: state_type!(),
    source_id: i64,
    request: MasterVideoIngestRequest,
) -> Result<MasterIngestResponse, String> {
    let prepared = prepare_video_ingest(&state, &request).await?;
    let persisted = state
        .db
        .get_master_source(source_id)
        .await
        .map_err(String::from)?;
    if persisted.source_kind != "video" {
        return Err("续跑来源不是视频母稿".into());
    }
    let persisted_identity = SourceIdentity {
        source_key: persisted.source_key,
        media_path: persisted.media_path,
        media_hash: persisted.media_hash,
        duration_ms: u64::try_from(persisted.duration_ms).map_err(|error| error.to_string())?,
    };
    validate_resume_source(&persisted_identity, &prepared.identity())?;
    let ingest_request = prepared.request(source_id);
    let status = run_resume_master_ingest(
        source_id,
        ingest_request,
        DatabaseCheckpointStore::new(state.db.clone()),
        prepared.transcriber,
        prepared.artifacts,
    )
    .await?;
    Ok(MasterIngestResponse { source_id, status })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn preview_master_script(
    state: state_type!(),
    request: MasterPreviewRequest,
) -> Result<MasterPreview, String> {
    let script_key = builder::safe_key(&request.script_key)?;
    let truth = load_master_truth(&state, request.source_id).await?;
    let api_key = crate::handlers::ai::configured_minimax_api_key(&state).await?;
    let draft = model::generate_master_draft(
        &api_key,
        &request.title,
        &truth.transcript,
        &truth.parameter_cards,
    )
    .await?;
    let validation = validate_with_review_gate(
        &draft,
        &truth.transcript,
        &truth.parameter_cards,
        truth.pending_critical_count,
    );
    Ok(MasterPreview {
        source_id: request.source_id,
        script_key,
        draft,
        validation,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn publish_master_script(
    state: state_type!(),
    request: PublishMasterRequest,
) -> Result<MasterScriptRow, String> {
    let script_key = builder::safe_key(&request.script_key)?;
    let truth = load_master_truth(&state, request.source_id).await?;
    let validation = validate_with_review_gate(
        &request.draft,
        &truth.transcript,
        &truth.parameter_cards,
        truth.pending_critical_count,
    );
    if !validation.publishable {
        return Err(format!(
            "母稿仍有 {} 项阻断问题，请先返回预览修正",
            validation.blocking_issues.len()
        ));
    }
    let vault = configured_vault(&state).await?;
    let base = PathBuf::from("10-企业母稿").join(&script_key).join("V1.0");
    let index_relative = base.join("README.md");
    let index = builder::render_master_index(&script_key, &request.draft);
    let mut files = vec![(index_relative.clone(), index.clone())];
    for section in &request.draft.sections {
        let section_key = builder::safe_key(&section.section_key)?;
        files.push((
            base.join(format!("{:02}-{section_key}.md", section.position)),
            builder::render_master_section(&script_key, section),
        ));
    }
    let sections = request
        .draft
        .sections
        .iter()
        .map(section_to_row)
        .collect::<Result<Vec<_>, _>>()?;
    let mut created = Vec::<PathBuf>::new();
    for (relative, content) in files {
        if let Err(error) = write_new_master_file(&vault, &relative, &content, &mut created).await {
            cleanup_created_files(&vault, &created).await;
            return Err(error);
        }
    }
    let published = state
        .db
        .publish_master_version(NewMasterVersion {
            script_key: script_key.clone(),
            version: "1.0.0".into(),
            source_id: request.source_id,
            title: request.draft.title.clone(),
            index_relative_path: normalized_path(&index_relative),
            content_hash: builder::content_hash(&index),
            sections,
        })
        .await;
    match published {
        Ok(row) => Ok(row),
        Err(error) => {
            cleanup_created_files(&vault, &created).await;
            Err(error.to_string())
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_master_script_status(
    state: state_type!(),
    source_id: i64,
) -> Result<MasterScriptStatus, String> {
    Ok(MasterScriptStatus {
        source: state
            .db
            .get_master_source(source_id)
            .await
            .map_err(String::from)?,
        chunks: state
            .db
            .list_master_chunks(source_id)
            .await
            .map_err(String::from)?,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_master_baseline(
    state: state_type!(),
    script_key: String,
) -> Result<MasterBaseline, String> {
    let script_key = builder::safe_key(&script_key)?;
    let master = state
        .db
        .get_latest_published_master(&script_key)
        .await
        .map_err(String::from)?;
    let sections = state
        .db
        .list_master_sections(master.id)
        .await
        .map_err(String::from)?;
    Ok(MasterBaseline { master, sections })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_master_section_index(
    state: state_type!(),
    script_key: String,
) -> Result<Vec<crate::master_script::section_index::MasterSectionIndexEntry>, String> {
    let script_key = builder::safe_key(&script_key)?;
    crate::master_script::section_index::get_section_index(&state, &script_key).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn import_deal_refinement_candidates(
    state: state_type!(),
    request: ImportDealRefinementCandidatesRequest,
) -> Result<ImportDealRefinementCandidatesResult, String> {
    let payload: DealRefinementPayload = serde_json::from_str(&request.payload_json)
        .map_err(|error| format!("成交候选 JSON 格式无效：{error}"))?;
    let script_key = builder::safe_key(&request.script_key)?;
    let master = state
        .db
        .get_latest_published_master(&script_key)
        .await
        .map_err(String::from)?;
    if master.id != request.expected_master_script_id {
        return Err("企业母稿已更新，请重新导出基线并生成成交候选".into());
    }
    let sections = state
        .db
        .list_master_sections(master.id)
        .await
        .map_err(String::from)?;
    let mut result = ImportDealRefinementCandidatesResult {
        imported: 0,
        skipped_weak: 0,
        skipped_invalid: 0,
        errors: Vec::new(),
    };
    for item in payload.unique_by_product {
        if item.verification_tier != "verified" {
            result.skipped_weak += 1;
            continue;
        }
        let Some(section_id) = item.master_section_id else {
            result.skipped_invalid += 1;
            continue;
        };
        let Some(section) = sections.iter().find(|section| section.id == section_id) else {
            result.skipped_invalid += 1;
            continue;
        };
        if item.speech_text.trim().is_empty() || item.speech_end_sec <= item.speech_start_sec {
            result.skipped_invalid += 1;
            continue;
        }
        let comparison = crate::master_script::comparison::compare_highlight_to_master(
            &state,
            crate::master_script::comparison::CompareHighlightRequest {
                script_key: script_key.clone(),
                expected_master_script_id: master.id,
                master_section_id: Some(section_id),
                source: request.source.clone(),
                source_start_ms: (item.speech_start_sec * 1000.0).round() as u64,
                source_end_ms: (item.speech_end_sec * 1000.0).round() as u64,
                candidate_type: "产品讲解".into(),
                chain_stages: vec!["订单锚定".into(), "商品讲解核验".into()],
                candidate_segment: crate::master_script::comparison::CandidateSegmentInput {
                    segment_id: format!("deal:{}:{}", item.order_id, item.product_name),
                    segment_type: "产品讲解".into(),
                    scene: "订单锚定商品讲解".into(),
                    customer_need: String::new(),
                    original_text: item.speech_text,
                    key_sentence: String::new(),
                    outcome: "confirmed_conversion".into(),
                    interrupted: false,
                    why_selected: "仅 verified 商品讲解片段可自动送入母稿对比；仍须人工审批。"
                        .into(),
                },
                product_card_id: item
                    .master_product_card_id
                    .or_else(|| section.product_card_id.clone()),
                section_kind: match section.section_kind.as_str() {
                    "opening" => master_script::MasterSectionKind::Opening,
                    "product" => master_script::MasterSectionKind::Product,
                    "transition" => master_script::MasterSectionKind::Transition,
                    "scenario" => master_script::MasterSectionKind::Scenario,
                    "closing" => master_script::MasterSectionKind::Closing,
                    _ => {
                        result.skipped_invalid += 1;
                        continue;
                    }
                },
                comparison_mode: "enterprise_upgrade".into(),
                competitor_name: None,
            },
        )
        .await;
        match comparison {
            Ok(_) => result.imported += 1,
            Err(error) => result
                .errors
                .push(format!("{}: {error}", item.product_name)),
        }
    }
    Ok(result)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_support_candidates(
    state: state_type!(),
    status: Option<String>,
) -> Result<Vec<crate::database::master_script::SupportCandidateRow>, String> {
    state
        .db
        .list_support_candidates(status.as_deref())
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn decide_support_candidate(
    state: state_type!(),
    id: i64,
    next_status: String,
) -> Result<crate::database::master_script::SupportCandidateRow, String> {
    state
        .db
        .decide_support_candidate(id, &next_status)
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_competitor_reference_candidates(
    state: state_type!(),
    status: Option<String>,
) -> Result<Vec<crate::database::master_script::CompetitorReferenceCandidateRow>, String> {
    state
        .db
        .list_competitor_reference_candidates(status.as_deref())
        .await
        .map_err(String::from)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn decide_competitor_reference_candidate(
    state: state_type!(),
    id: i64,
    next_status: String,
) -> Result<crate::database::master_script::CompetitorReferenceCandidateRow, String> {
    state
        .db
        .decide_competitor_reference_candidate(id, &next_status)
        .await
        .map_err(String::from)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishCompetitorReferenceRequest {
    pub candidate_id: i64,
    pub title: String,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn publish_competitor_reference(
    state: state_type!(),
    request: PublishCompetitorReferenceRequest,
) -> Result<String, String> {
    let candidate = state
        .db
        .list_competitor_reference_candidates(None)
        .await
        .map_err(String::from)?
        .into_iter()
        .find(|row| row.id == request.candidate_id)
        .ok_or("competitor reference candidate not found")?;
    if candidate.status != "approved_reference" {
        return Err("竞品参考必须先人工批准，才能写入案例库".into());
    }
    let vault = configured_vault(&state).await?;
    let master = state
        .db
        .get_master_version(candidate.master_script_id)
        .await
        .map_err(String::from)?;
    let section = state
        .db
        .list_master_sections(master.id)
        .await
        .map_err(String::from)?
        .into_iter()
        .find(|section| section.id == candidate.master_section_id)
        .ok_or("matched master section not found")?;
    let slug = candidate
        .competitor_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c.is_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    let title = if request.title.trim().is_empty() {
        section.title.clone()
    } else {
        request.title.trim().to_string()
    };
    let relative = PathBuf::from("03-直播案例")
        .join(if slug.is_empty() { "competitor" } else { slug })
        .join(format!(
            "{}-{}.md",
            Utc::now().format("%Y%m%d"),
            super_safe_filename(&title)
        ));
    let content = format!("---\nid: CASE-COMP-{}-{}\ntype: competitor_case_study\nstatus: approved_reference\ncompetitor_name: {}\nsource_recording: {}\nmaster_script_key: {}\nmatched_master_section: {}\nmigration_decision: {}\n---\n\n# {}\n\n## 竞品原话（仅供参考）\n\n{}\n\n## 对照企业母稿\n\n{}\n\n## 迁移提醒\n\n竞品原话不能直接作为企业标准稿。价格、库存、链接、型号、售后等动态事实必须替换为本场真实信息；仅可在人审后提炼结构或建议表达。\n\n## 对照结果\n\n```json\n{}\n```\n", Utc::now().format("%Y%m%d"), candidate.id, candidate.competitor_name, candidate.source_key, master.script_key, section.title, candidate.migration_decision, title, candidate.host_text, section.title, candidate.comparison_json);
    crate::knowledge_writer::write_verified_markdown(&vault, &relative, true, &content).await?;
    state
        .db
        .decide_competitor_reference_candidate(candidate.id, "merged_to_case_study")
        .await
        .map_err(String::from)?;
    Ok(relative.to_string_lossy().into_owned())
}

fn super_safe_filename(value: &str) -> String {
    let text = value
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '-'
            } else {
                c
            }
        })
        .collect::<String>();
    text.trim().trim_matches('.').chars().take(80).collect()
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn preview_master_upgrade(
    state: state_type!(),
    request: MasterUpgradeRequest,
) -> Result<MasterUpgradePreview, String> {
    build_upgrade_preview(&state, request).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn publish_master_upgrade(
    state: state_type!(),
    request: PublishMasterUpgradeRequest,
) -> Result<MasterScriptRow, String> {
    if !request.diff_confirmed {
        return Err("请先确认母稿差异".into());
    }
    let preview = build_upgrade_preview(
        &state,
        MasterUpgradeRequest {
            script_key: request.script_key,
            candidate_ids: request.candidate_ids,
        },
    )
    .await?;
    let base = state
        .db
        .get_master_version(preview.base_master_id)
        .await
        .map_err(String::from)?;
    let current_sections = state
        .db
        .list_master_sections(base.id)
        .await
        .map_err(String::from)?;
    let changes = preview
        .sections
        .iter()
        .map(|section| (section.section_id, section))
        .collect::<HashMap<_, _>>();
    let next_sections = current_sections
        .iter()
        .map(|section| {
            let change = changes.get(&section.id);
            Ok(NewMasterSection {
                section_key: section.section_key.clone(),
                position: section.position,
                section_kind: parse_stored_section_kind(&section.section_kind)?,
                product_card_id: section.product_card_id.clone(),
                title: section.title.clone(),
                source_start_ms: section.source_start_ms,
                source_end_ms: section.source_end_ms,
                host_text: section.host_text.clone(),
                master_text: change
                    .map(|value| value.next_text.clone())
                    .unwrap_or_else(|| section.master_text.clone()),
                metadata_json: if let Some(change) = change {
                    json!({"baseMetadata": section.metadata_json, "supportCandidateIds": change.candidate_ids}).to_string()
                } else {
                    section.metadata_json.clone()
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let vault = configured_vault(&state).await?;
    let base_path = PathBuf::from("10-企业母稿")
        .join(&preview.script_key)
        .join(format!("V{}", preview.next_version));
    let index_relative = base_path.join("README.md");
    let index = render_upgrade_index(&base, &preview.next_version, &next_sections);
    let mut files = vec![(index_relative.clone(), index.clone())];
    for section in &next_sections {
        let key = builder::safe_key(&section.section_key)?;
        files.push((
            base_path.join(format!("{:02}-{key}.md", section.position)),
            render_upgrade_section(&preview.script_key, &preview.next_version, section),
        ));
    }
    let mut created = Vec::new();
    for (relative, content) in files {
        if let Err(error) = write_new_master_file(&vault, &relative, &content, &mut created).await {
            cleanup_created_files(&vault, &created).await;
            return Err(error);
        }
    }
    let published = state
        .db
        .publish_master_version(NewMasterVersion {
            script_key: preview.script_key.clone(),
            version: preview.next_version.clone(),
            source_id: base.source_id,
            title: base.title,
            index_relative_path: normalized_path(&index_relative),
            content_hash: builder::content_hash(&index),
            sections: next_sections,
        })
        .await;
    let published = match published {
        Ok(value) => value,
        Err(error) => {
            cleanup_created_files(&vault, &created).await;
            return Err(error.to_string());
        }
    };
    for candidate_id in preview
        .sections
        .iter()
        .flat_map(|section| section.candidate_ids.iter())
    {
        state
            .db
            .decide_support_candidate(*candidate_id, "merged")
            .await
            .map_err(String::from)?;
    }
    Ok(published)
}

async fn build_upgrade_preview(
    state: &State,
    request: MasterUpgradeRequest,
) -> Result<MasterUpgradePreview, String> {
    let script_key = builder::safe_key(&request.script_key)?;
    let ids = request.candidate_ids.into_iter().collect::<HashSet<_>>();
    if ids.is_empty() {
        return Err("至少选择一条已通过的候选辅稿".into());
    }
    let base = state
        .db
        .get_latest_published_master(&script_key)
        .await
        .map_err(String::from)?;
    let candidates = state
        .db
        .list_support_candidates(Some("approved"))
        .await
        .map_err(String::from)?
        .into_iter()
        .filter(|candidate| ids.contains(&candidate.id))
        .collect::<Vec<_>>();
    if candidates.len() != ids.len() {
        return Err("所选候选辅稿尚未全部通过人工审核".into());
    }
    if candidates
        .iter()
        .any(|candidate| candidate.master_script_id != base.id)
    {
        return Err("母稿已更新，请先按最新版本重新评分".into());
    }
    let sections = state
        .db
        .list_master_sections(base.id)
        .await
        .map_err(String::from)?;
    let mut changes = Vec::new();
    for section in sections {
        let selected = candidates
            .iter()
            .filter(|candidate| candidate.master_section_id == section.id)
            .collect::<Vec<_>>();
        if selected.is_empty() {
            continue;
        }
        let mut ranked = selected
            .iter()
            .map(|candidate| {
                upgrade_decision_rank(&candidate.comparison_json).map(|rank| (rank, *candidate))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ranked.iter().filter(|(rank, _)| *rank == 0).count() > 1 {
            return Err("同一母稿章节不能同时批准两条替换建议，请只保留一条后重试".into());
        }
        ranked.sort_by_key(|(rank, candidate)| (*rank, candidate.id));
        let mut next_text = section.master_text.clone();
        for (_, candidate) in &ranked {
            next_text = apply_upgrade_candidate_text(
                &next_text,
                &candidate.host_text,
                &candidate.comparison_json,
            )?;
        }
        changes.push(MasterUpgradeSection {
            section_id: section.id,
            section_key: section.section_key,
            current_text: section.master_text.clone(),
            next_text,
            candidate_ids: selected.iter().map(|candidate| candidate.id).collect(),
        });
    }
    Ok(MasterUpgradePreview {
        base_master_id: base.id,
        script_key,
        current_version: base.version.clone(),
        next_version: master_script::next_patch_version(&base.version)
            .map_err(|error| error.to_string())?,
        sections: changes,
    })
}

fn apply_upgrade_candidate_text(
    current_text: &str,
    host_text: &str,
    comparison_json: &str,
) -> Result<String, String> {
    let parsed = serde_json::from_str::<serde_json::Value>(comparison_json)
        .map_err(|error| format!("候选话术的母稿比较结果无法读取：{error}"))?;
    let review = parsed
        .get("upgradeReview")
        .and_then(serde_json::Value::as_object)
        .ok_or("候选话术缺少提示词三比较结果")?;
    let decision = review
        .get("comparisonDecision")
        .and_then(serde_json::Value::as_str)
        .ok_or("候选话术缺少提示词三处理决策")?;
    let append = |heading: &str, text: &str| {
        format!("{}\n\n{heading}\n{}", current_text.trim(), text.trim())
    };
    match decision {
        "add_as_support" => Ok(append("## 候选辅稿", host_text)),
        "add_as_golden_sentence" => Ok(append("## 金句话术", host_text)),
        "merge_with_existing" => Ok(append("## 待合并话术", host_text)),
        "replace_existing" => {
            let suggestion = review
                .get("trainingSuggestion")
                .and_then(serde_json::Value::as_str)
                .filter(|value| value.trim().starts_with("[建议稿]"))
                .ok_or("建议替换缺少明确标记的建议稿")?;
            Ok(format!("## 建议替换稿\n{}", suggestion.trim()))
        }
        "duplicate" | "reject" => Err("重复或不采用的话术不能进入母稿升级".into()),
        _ => Err("候选话术包含未知的提示词三处理决策".into()),
    }
}

fn upgrade_decision_rank(comparison_json: &str) -> Result<u8, String> {
    let parsed = serde_json::from_str::<serde_json::Value>(comparison_json)
        .map_err(|error| format!("候选话术的母稿比较结果无法读取：{error}"))?;
    let decision = parsed
        .get("upgradeReview")
        .and_then(|review| review.get("comparisonDecision"))
        .and_then(serde_json::Value::as_str)
        .ok_or("候选话术缺少提示词三处理决策")?;
    match decision {
        "replace_existing" => Ok(0),
        "add_as_support" | "add_as_golden_sentence" | "merge_with_existing" => Ok(1),
        "duplicate" | "reject" => Err("重复或不采用的话术不能进入母稿升级".into()),
        _ => Err("候选话术包含未知的提示词三处理决策".into()),
    }
}

fn parse_stored_section_kind(value: &str) -> Result<master_script::MasterSectionKind, String> {
    match value {
        "opening" => Ok(master_script::MasterSectionKind::Opening),
        "product" => Ok(master_script::MasterSectionKind::Product),
        "transition" => Ok(master_script::MasterSectionKind::Transition),
        "scenario" => Ok(master_script::MasterSectionKind::Scenario),
        "closing" => Ok(master_script::MasterSectionKind::Closing),
        _ => Err(format!("母稿章节类型无效：{value}")),
    }
}

fn render_upgrade_index(
    base: &MasterScriptRow,
    version: &str,
    sections: &[NewMasterSection],
) -> String {
    let links = sections
        .iter()
        .map(|section| {
            format!(
                "- [[{:02}-{}|{}]]",
                section.position, section.section_key, section.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("---\nid: {}\ntype: master_script\nversion: {}\nbase_version: {}\nstatus: published\n---\n\n# {}\n\n{}\n", base.script_key, version, base.version, base.title, links)
}

fn render_upgrade_section(script_key: &str, version: &str, section: &NewMasterSection) -> String {
    format!("---\nmaster_id: {script_key}\nversion: {version}\nposition: {}\nkind: {}\nsource_start_ms: {}\nsource_end_ms: {}\n---\n\n# {}\n\n{}\n", section.position, section.section_kind.as_str(), section.source_start_ms, section.source_end_ms, section.title, section.master_text)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn compare_highlight_to_master(
    state: state_type!(),
    request: crate::master_script::comparison::CompareHighlightRequest,
) -> Result<crate::master_script::comparison::CompareHighlightResult, String> {
    crate::master_script::comparison::compare_highlight_to_master(&state, request).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn retry_master_upgrade_review(
    state: state_type!(),
    review_id: i64,
) -> Result<crate::master_script::comparison::RetryUpgradeReviewResult, String> {
    crate::master_script::comparison::retry_master_upgrade_review(&state, review_id).await
}

struct MasterTruth {
    transcript: Vec<TranscriptCue>,
    parameter_cards: Vec<ParameterFactCard>,
    pending_critical_count: usize,
}

const BATCH_EVIDENCE_PER_VIDEO: usize = 14;

fn select_batch_evidence(
    video_id: i64,
    video_title: &str,
    transcript: &[TranscriptCue],
) -> Vec<model::BatchEvidence> {
    if transcript.is_empty() {
        return Vec::new();
    }
    let mut candidate_indexes = (0..transcript.len()).collect::<Vec<_>>();
    candidate_indexes
        .sort_by_key(|index| std::cmp::Reverse(batch_evidence_score(&transcript[*index].text)));
    let mut selected = Vec::new();
    // Keep high-signal sales moments, while reserving evenly distributed evidence for the
    // full-session rhythm rather than only the most repetitive calls to action.
    for index in candidate_indexes {
        if selected.len() >= BATCH_EVIDENCE_PER_VIDEO.saturating_sub(5) {
            break;
        }
        let start_ms = transcript[index].start_ms;
        if selected
            .iter()
            .any(|chosen: &usize| transcript[*chosen].start_ms.abs_diff(start_ms) < 90_000)
        {
            continue;
        }
        selected.push(index);
    }
    for part in 0..BATCH_EVIDENCE_PER_VIDEO {
        if selected.len() >= BATCH_EVIDENCE_PER_VIDEO {
            break;
        }
        let index = part.saturating_mul(transcript.len().saturating_sub(1))
            / BATCH_EVIDENCE_PER_VIDEO.saturating_sub(1).max(1);
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    selected.sort_unstable();
    selected
        .into_iter()
        .enumerate()
        .map(|(rank, index)| {
            let start = index.saturating_sub(1);
            let end = (index + 2).min(transcript.len());
            let cues = &transcript[start..end];
            model::BatchEvidence {
                evidence_id: format!("V{video_id}-{:02}", rank + 1),
                video_id,
                video_title: video_title.to_string(),
                start_ms: cues.first().map(|cue| cue.start_ms).unwrap_or_default(),
                end_ms: cues.last().map(|cue| cue.end_ms).unwrap_or_default(),
                quote: cues
                    .iter()
                    .map(|cue| cue.text.trim())
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            }
        })
        .filter(|evidence| !evidence.quote.is_empty())
        .collect()
}

fn batch_evidence_score(text: &str) -> u32 {
    const SALES_TERMS: [&str; 20] = [
        "下单",
        "拍下",
        "链接",
        "小黄车",
        "价格",
        "优惠",
        "福利",
        "库存",
        "售后",
        "放心",
        "安排",
        "发货",
        "回收",
        "老师",
        "宝宝",
        "成交",
        "上车",
        "直接",
        "今天",
        "最后",
    ];
    let keyword_score = SALES_TERMS
        .iter()
        .filter(|term| text.contains(**term))
        .count() as u32;
    let digit_score = text
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count() as u32;
    keyword_score
        .saturating_mul(10)
        .saturating_add(digit_score.min(5))
}

async fn load_master_truth(state: &State, source_id: i64) -> Result<MasterTruth, String> {
    let source = state
        .db
        .get_master_source(source_id)
        .await
        .map_err(String::from)?;
    let video_id = source
        .source_key
        .strip_prefix("video:")
        .ok_or("当前只支持从已导入视频生成母稿")?
        .parse::<i64>()
        .map_err(|error| error.to_string())?;
    let context = resolve_video_transcript_context(state, video_id).await?;
    // Media files may be moved between disks after transcription. The immutable
    // source remains auditable through its stored hash, while draft generation
    // reads the corrected artifact associated with the stable video id.
    let bundle =
        crate::subtitle_generator::transcript_artifacts::TranscriptArtifactStore::load_from_dir(
            context.source,
            &context.artifact_dir,
        )
        .await
        .map_err(|error| error.to_string())?;
    let deal_window_path = context.artifact_dir.join("deal-windows.srt");
    let transcript_srt = match tokio::fs::read_to_string(deal_window_path).await {
        Ok(content) if !content.trim().is_empty() => Some(content),
        _ => None,
    };
    let transcript_cues = match &transcript_srt {
        Some(content) => master_ingest::parse_srt_cues(content)
            .or_else(|_| master_ingest::parse_srt_cues(&bundle.corrected_srt)),
        None => Ok(master_ingest::parse_srt_cues(&bundle.corrected_srt)?),
    }?;
    let transcript = transcript_cues
        .into_iter()
        .enumerate()
        .map(|(index, cue)| TranscriptCue {
            id: (index + 1) as u64,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: crate::subtitle_generator::transcript_artifacts::normalize_letter_prefixed_model_numbers(&cue.text),
        })
        .collect();
    let parameter_cards = state
        .db
        .list_asr_parameter_cards()
        .await
        .map_err(String::from)?
        .into_iter()
        .map(|record| ParameterFactCard {
            card_id: record.card_id,
            static_terms: vec![record.title],
        })
        .collect();
    Ok(MasterTruth {
        transcript,
        parameter_cards,
        pending_critical_count: bundle.pending_critical_count,
    })
}

fn validate_with_review_gate(
    draft: &MasterDraft,
    transcript: &[TranscriptCue],
    cards: &[ParameterFactCard],
    pending_critical_count: usize,
) -> MasterDraftValidation {
    let mut validation = validate_master_draft(draft, transcript, cards);
    if pending_critical_count > 0 {
        validation.blocking_issues.push(BuilderIssue {
            code: "pending_critical_transcript_review".into(),
            message: format!("还有 {pending_critical_count} 条关键逐字稿纠错待人工确认"),
            section_key: None,
        });
        validation.publishable = false;
    }
    validation
}

async fn configured_vault(state: &State) -> Result<PathBuf, String> {
    let path = state.config.read().await.knowledge_vault_path.clone();
    if path.trim().is_empty() {
        return Err("尚未连接 Obsidian 知识库".into());
    }
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err("Obsidian 知识库目录不可用".into());
    }
    Ok(path)
}

/// Resolves the host display name used for `{主播名}知识库`.
/// Prefers batch `host_label`; falls back to any non-empty video `anchor_name`.
async fn resolve_batch_host_label(
    state: &State,
    detail: &MasterSampleBatchDetail,
) -> Result<String, String> {
    let from_batch = detail.batch.host_label.trim();
    if !from_batch.is_empty() {
        return Ok(from_batch.to_string());
    }
    for item in &detail.items {
        if let Ok(video) = state.db.get_video(item.video_id).await {
            let anchor = video.anchor_name.trim();
            if !anchor.is_empty() {
                return Ok(anchor.to_string());
            }
        }
    }
    Err(
        "请先在批次填写样本来源/主播名（如：于千惠），再发布到「主播知识库」；无主播名无法创建知识库目录。"
            .into(),
    )
}

fn sanitize_host_knowledge_base_name(host_label: &str) -> Result<String, String> {
    let trimmed = host_label.trim();
    if trimmed.is_empty() {
        return Err("请先在批次填写样本来源/主播名（如：于千惠），再发布到「主播知识库」。".into());
    }
    let sanitized: String = trimmed
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
            ) && !character.is_control()
        })
        .collect::<String>()
        .trim()
        .trim_end_matches(['.', ' '])
        .to_string();
    if sanitized.is_empty() {
        return Err("主播名包含无效字符，请改用可识别的中文或字母名称后再发布。".into());
    }
    if sanitized.ends_with("知识库") {
        Ok(sanitized)
    } else {
        Ok(format!("{sanitized}知识库"))
    }
}

async fn load_batch_video_paths(
    state: &State,
    draft: &model::BatchMasterDraft,
) -> HashMap<i64, (String, String)> {
    let mut paths = HashMap::new();
    let mut seen = HashSet::new();
    for evidence in &draft.evidence {
        if !seen.insert(evidence.video_id) {
            continue;
        }
        if let Ok(video) = state.db.get_video(evidence.video_id).await {
            let title = if video.title.trim().is_empty() {
                evidence.video_title.clone()
            } else {
                video.title
            };
            paths.insert(evidence.video_id, (title, video.file));
        }
    }
    paths
}

fn render_host_analysis_advice(
    host_label: &str,
    detail: &MasterSampleBatchDetail,
    draft: &model::BatchMasterDraft,
) -> String {
    let rules = if draft.operating_rules.is_empty() {
        "暂无执行规则，待人工补充。".to_string()
    } else {
        draft
            .operating_rules
            .iter()
            .map(|rule| format!("- {}", rule.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let patterns = if draft.patterns.is_empty() {
        "暂无规律观察。".to_string()
    } else {
        draft
            .patterns
            .iter()
            .map(|pattern| {
                format!(
                    "### {}\n\n{}\n\n证据：{}",
                    pattern.name.trim(),
                    pattern.observation.trim(),
                    if pattern.evidence_ids.is_empty() {
                        "无".into()
                    } else {
                        pattern.evidence_ids.join("、")
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    format!(
        "# 分析建议\n\n- 主播：{}\n- 样本批次：{}\n- 版本：V1.0\n- 状态：已审核发布\n\n## 执行规则\n\n{}\n\n## 规律观察\n\n{}\n\n> AI 只给建议，不代替人拍板照念。\n",
        host_label.trim(),
        detail.batch.title.trim(),
        rules,
        patterns
    )
}

fn render_host_deal_video_refs(
    kb_root: &Path,
    host_label: &str,
    draft: &model::BatchMasterDraft,
    video_paths: &HashMap<i64, (String, String)>,
) -> Vec<(PathBuf, String)> {
    let video_dir = kb_root.join("视频").join("成交");
    if draft.evidence.is_empty() {
        return vec![(
            video_dir.join("README.md"),
            format!(
                "# 成交视频索引\n\n- 主播：{}\n- 类目：成交\n\n暂无带视频路径的证据条目。\n",
                host_label.trim()
            ),
        )];
    }
    draft
        .evidence
        .iter()
        .map(|evidence| {
            let (fallback_title, file_path) = video_paths
                .get(&evidence.video_id)
                .cloned()
                .unwrap_or_else(|| (evidence.video_title.clone(), String::new()));
            let title = if evidence.video_title.trim().is_empty() {
                fallback_title
            } else {
                evidence.video_title.clone()
            };
            let path_line = if file_path.trim().is_empty() {
                "（未找到本地/NAS 路径，请在视频库核对）".to_string()
            } else {
                file_path
            };
            let file_name = format!("{}.md", sanitize_evidence_file_stem(&evidence.evidence_id));
            let content = format!(
                "# {}\n\n- 主播：{}\n- 类目：成交\n- 证据编号：{}\n- 本地/NAS 路径：`{}`\n- 时间窗：{} — {}（毫秒）\n- 引用原话：{}\n\n> 本文件仅为路径引用，**未复制**视频文件进知识库。\n",
                title.trim(),
                host_label.trim(),
                evidence.evidence_id,
                path_line,
                evidence.start_ms,
                evidence.end_ms,
                evidence.quote.trim(),
            );
            (video_dir.join(file_name), content)
        })
        .collect()
}

fn transcript_sources(
    batch_id: i64,
    version: i64,
    evidence: &[model::BatchEvidence],
) -> Result<Vec<AnchorKnowledgeSourceInput>, String> {
    evidence
        .iter()
        .map(|item| {
            let start_ms = i64::try_from(item.start_ms)
                .map_err(|_| format!("证据 {} 的开始时间超出索引范围", item.evidence_id))?;
            let end_ms = i64::try_from(item.end_ms)
                .map_err(|_| format!("证据 {} 的结束时间超出索引范围", item.evidence_id))?;
            if end_ms <= start_ms {
                return Err(format!("证据 {} 的时间范围无效", item.evidence_id));
            }
            let transcript_hash = builder::content_hash(&item.quote);
            Ok(AnchorKnowledgeSourceInput {
                source_kind: "transcript".into(),
                source_locator: format!(
                    "video:{}#{}-{}:{}",
                    item.video_id, item.start_ms, item.end_ms, item.evidence_id
                ),
                video_id: Some(item.video_id),
                start_ms: Some(start_ms),
                end_ms: Some(end_ms),
                transcript_version: format!("master-batch-{batch_id}-v{version}"),
                transcript_hash: transcript_hash.clone(),
                product_fact_id: String::new(),
                product_fact_version: String::new(),
                analysis_version: String::new(),
                content_hash: transcript_hash,
            })
        })
        .collect()
}

fn master_version_source(
    previous_master_id: i64,
    previous_content_hash: &str,
    previous_version: &str,
) -> AnchorKnowledgeSourceInput {
    AnchorKnowledgeSourceInput {
        source_kind: "analysis".into(),
        source_locator: format!("master-script:{previous_master_id}"),
        video_id: None,
        start_ms: None,
        end_ms: None,
        transcript_version: String::new(),
        transcript_hash: String::new(),
        product_fact_id: String::new(),
        product_fact_version: String::new(),
        analysis_version: previous_version.into(),
        content_hash: previous_content_hash.into(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn index_published_master_assets(
    state: &State,
    anchor_id: &str,
    detail: &MasterSampleBatchDetail,
    draft: &model::BatchMasterDraft,
    version: i64,
    speech_relative: &Path,
    speech_content: &str,
    analysis_relative: &Path,
    analysis_content: &str,
    deal_files: &[(PathBuf, String)],
) -> Result<(), String> {
    let sources = transcript_sources(detail.batch.id, version, &draft.evidence)?;
    let reviewer_id = format!("master-sample-batch:{}", detail.batch.id);
    let reason = "母稿样本批次经人工确认发布";
    state
        .db
        .import_published_anchor_knowledge_asset(PublishedAnchorKnowledgeAssetInput {
            asset_id: format!("master-batch-{}-speech-v{version}", detail.batch.id),
            anchor_id: anchor_id.into(),
            asset_type: "speech".into(),
            title: draft.title.clone(),
            body: speech_content.into(),
            product_id: String::new(),
            version,
            supersedes_asset_id: (version > 1)
                .then(|| format!("master-batch-{}-speech-v{}", detail.batch.id, version - 1)),
            reviewer_id: reviewer_id.clone(),
            review_reason: reason.into(),
            published_relative_path: normalized_path(speech_relative),
            published_file_hash: builder::content_hash(speech_content),
            sources: sources.clone(),
        })
        .await
        .map_err(String::from)?;
    state
        .db
        .import_published_anchor_knowledge_asset(PublishedAnchorKnowledgeAssetInput {
            asset_id: format!("master-batch-{}-analysis-v{version}", detail.batch.id),
            anchor_id: anchor_id.into(),
            asset_type: "analysis_advice".into(),
            title: format!("{} · 分析建议", detail.batch.title.trim()),
            body: analysis_content.into(),
            product_id: String::new(),
            version,
            supersedes_asset_id: (version > 1)
                .then(|| format!("master-batch-{}-analysis-v{}", detail.batch.id, version - 1)),
            reviewer_id: reviewer_id.clone(),
            review_reason: reason.into(),
            published_relative_path: normalized_path(analysis_relative),
            published_file_hash: builder::content_hash(analysis_content),
            sources: sources.clone(),
        })
        .await
        .map_err(String::from)?;

    for (evidence, (relative, content)) in draft.evidence.iter().zip(deal_files) {
        let evidence_sources =
            transcript_sources(detail.batch.id, version, std::slice::from_ref(evidence))?;
        state
            .db
            .import_published_anchor_knowledge_asset(PublishedAnchorKnowledgeAssetInput {
                asset_id: format!(
                    "master-batch-{}-deal-{}",
                    detail.batch.id,
                    sanitize_evidence_file_stem(&evidence.evidence_id)
                ),
                anchor_id: anchor_id.into(),
                asset_type: "deal_clip".into(),
                title: evidence.video_title.clone(),
                body: content.clone(),
                product_id: String::new(),
                version: 1,
                supersedes_asset_id: None,
                reviewer_id: reviewer_id.clone(),
                review_reason: reason.into(),
                published_relative_path: normalized_path(relative),
                published_file_hash: builder::content_hash(content),
                sources: evidence_sources,
            })
            .await
            .map_err(String::from)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn index_corrected_master_assets(
    state: &State,
    anchor_id: &str,
    detail: &MasterSampleBatchDetail,
    speech_relative: &Path,
    speech_content: &str,
    analysis_relative: &Path,
    analysis_content: &str,
    source: AnchorKnowledgeSourceInput,
) -> Result<(), String> {
    let reviewer_id = format!("master-sample-batch:{}", detail.batch.id);
    let reason = "人工发布逐字稿确定性校正版";
    for input in [
        PublishedAnchorKnowledgeAssetInput {
            asset_id: format!("master-batch-{}-speech-v2", detail.batch.id),
            anchor_id: anchor_id.into(),
            asset_type: "speech".into(),
            title: format!("{}（逐字稿校正）", detail.batch.title.trim()),
            body: speech_content.into(),
            product_id: String::new(),
            version: 2,
            supersedes_asset_id: Some(format!("master-batch-{}-speech-v1", detail.batch.id)),
            reviewer_id: reviewer_id.clone(),
            review_reason: reason.into(),
            published_relative_path: normalized_path(speech_relative),
            published_file_hash: builder::content_hash(speech_content),
            sources: vec![source.clone()],
        },
        PublishedAnchorKnowledgeAssetInput {
            asset_id: format!("master-batch-{}-analysis-v2", detail.batch.id),
            anchor_id: anchor_id.into(),
            asset_type: "analysis_advice".into(),
            title: format!("{} · 校正说明", detail.batch.title.trim()),
            body: analysis_content.into(),
            product_id: String::new(),
            version: 2,
            supersedes_asset_id: Some(format!("master-batch-{}-analysis-v1", detail.batch.id)),
            reviewer_id: reviewer_id.clone(),
            review_reason: reason.into(),
            published_relative_path: normalized_path(analysis_relative),
            published_file_hash: builder::content_hash(analysis_content),
            sources: vec![source],
        },
    ] {
        state
            .db
            .import_published_anchor_knowledge_asset(input)
            .await
            .map_err(String::from)?;
    }
    Ok(())
}

fn sanitize_evidence_file_stem(evidence_id: &str) -> String {
    let stem: String = evidence_id
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "evidence".into()
    } else {
        stem
    }
}

async fn write_new_master_file(
    vault: &Path,
    relative: &Path,
    content: &str,
    created: &mut Vec<PathBuf>,
) -> Result<(), String> {
    crate::knowledge_writer::write_verified_markdown(vault, relative, true, content).await?;
    created.push(relative.to_path_buf());
    Ok(())
}

async fn cleanup_created_files(vault: &Path, created: &[PathBuf]) {
    for relative in created.iter().rev() {
        let _ = tokio::fs::remove_file(vault.join(relative)).await;
    }
}

fn section_to_row(
    section: &master_builder::MasterSectionDraft,
) -> Result<NewMasterSection, String> {
    Ok(NewMasterSection {
        section_key: section.section_key.clone(),
        position: i64::from(section.position),
        section_kind: match &section.kind {
            MasterSectionKind::Opening => master_script::MasterSectionKind::Opening,
            MasterSectionKind::Product => master_script::MasterSectionKind::Product,
            MasterSectionKind::Transition => master_script::MasterSectionKind::Transition,
            MasterSectionKind::Scenario => master_script::MasterSectionKind::Scenario,
            MasterSectionKind::Closing => master_script::MasterSectionKind::Closing,
        },
        product_card_id: section.product_card_id.clone(),
        title: section
            .host_text
            .lines()
            .next()
            .unwrap_or("章节")
            .to_string(),
        source_start_ms: i64::try_from(section.source_start_ms)
            .map_err(|error| error.to_string())?,
        source_end_ms: i64::try_from(section.source_end_ms).map_err(|error| error.to_string())?,
        host_text: section.host_text.clone(),
        master_text: section.master_text.clone(),
        metadata_json: json!({
            "sourceCueIds": section.source_cue_ids,
            "textOrigin": section.text_origin,
            "conditions": section.conditions,
            "dynamicFields": section.dynamic_fields,
        })
        .to_string(),
    })
}

fn batch_draft_sections(
    detail: &MasterSampleBatchDetail,
    draft: &model::BatchMasterDraft,
) -> Result<Vec<NewMasterSection>, String> {
    let evidence = draft
        .evidence
        .iter()
        .map(|item| (item.evidence_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let video_sources = detail
        .items
        .iter()
        .filter_map(|item| item.source_id.map(|source_id| (item.video_id, source_id)))
        .collect::<HashMap<_, _>>();

    draft
        .sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let selected = section
                .evidence_ids
                .iter()
                .filter_map(|id| evidence.get(id.as_str()).copied())
                .collect::<Vec<_>>();
            if selected.is_empty() {
                return Err(format!("章节“{}”没有有效样本证据", section.title));
            }
            let source_start_ms = selected.iter().map(|item| item.start_ms).min().unwrap_or(0);
            let source_end_ms = selected.iter().map(|item| item.end_ms).max().unwrap_or(0);
            let position = i64::try_from(index + 1).map_err(|error| error.to_string())?;
            Ok(NewMasterSection {
                section_key: format!("batch-section-{position:03}"),
                position,
                section_kind: batch_section_kind(&section.title, index, draft.sections.len()),
                product_card_id: None,
                title: section.title.trim().to_string(),
                source_start_ms: i64::try_from(source_start_ms)
                    .map_err(|error| error.to_string())?,
                source_end_ms: i64::try_from(source_end_ms).map_err(|error| error.to_string())?,
                host_text: section.fixed_speech.trim().to_string(),
                master_text: section.fixed_speech.trim().to_string(),
                metadata_json: json!({
                    "batchId": detail.batch.id,
                    "purpose": section.purpose,
                    "evidenceIds": section.evidence_ids,
                    "conditions": section.conditions,
                    "dynamicFields": section.dynamic_fields,
                    "evidence": selected.iter().map(|item| json!({
                        "evidenceId": item.evidence_id,
                        "videoId": item.video_id,
                        "sourceId": video_sources.get(&item.video_id),
                        "videoTitle": item.video_title,
                        "startMs": item.start_ms,
                        "endMs": item.end_ms,
                    })).collect::<Vec<_>>(),
                })
                .to_string(),
            })
        })
        .collect()
}

fn batch_section_kind(title: &str, index: usize, total: usize) -> master_script::MasterSectionKind {
    if index == 0 || title.contains("开场") {
        master_script::MasterSectionKind::Opening
    } else if index + 1 == total || title.contains("收尾") || title.contains("结束") {
        master_script::MasterSectionKind::Closing
    } else if title.contains("转场") || title.contains("衔接") {
        master_script::MasterSectionKind::Transition
    } else {
        master_script::MasterSectionKind::Scenario
    }
}

fn render_batch_master_index(
    script_key: &str,
    detail: &MasterSampleBatchDetail,
    draft: &model::BatchMasterDraft,
) -> String {
    let sections = draft
        .sections
        .iter()
        .enumerate()
        .map(|(index, section)| format!("- {:02}. {}", index + 1, section.title.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    let company_benchmark_reference = if draft.company_benchmarks.is_empty() {
        "未加载公司成交基准；本版本仅依据头牌主播录播证据整理。".to_string()
    } else {
        draft
            .company_benchmarks
            .iter()
            .map(|item| format!("- {} · V{} · {}", item.card_id, item.version, item.title))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "# {}\n\n- 母稿编号：{}\n- 版本：V1.0\n- 样本批次：{}\n- 样本场次：{}\n- 状态：已审核发布\n\n## 公司成交结构基准\n\n{}\n\n## 使用边界\n\n固定原话仅来自已校正的主播逐字稿；公司成交结构基准只用于结构检查，不能作为主播固定原话；价格、库存、赠品、链接和商品型号均需以当场事实为准。\n\n## 章节\n\n{}\n\n## 执行规则\n\n{}\n",
        draft.title.trim(),
        script_key,
        detail.batch.title.trim(),
        detail.items.len(),
        company_benchmark_reference,
        sections,
        draft
            .operating_rules
            .iter()
            .map(|rule| format!("- {}", rule.trim()))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn render_batch_master_section(
    script_key: &str,
    row: &NewMasterSection,
    section: &model::BatchMasterSection,
) -> String {
    format!(
        "# {}\n\n- 母稿编号：{}\n- 章节：{}\n- 证据编号：{}\n\n## 用途\n\n{}\n\n## 固定原话\n\n{}\n\n## 适用条件\n\n{}\n\n## 当场变量\n\n{}\n",
        row.title.trim(),
        script_key,
        row.position,
        section.evidence_ids.join("、"),
        section.purpose.trim(),
        row.master_text.trim(),
        if section.conditions.is_empty() { "- 无".into() } else { section.conditions.iter().map(|item| format!("- {}", item.trim())).collect::<Vec<_>>().join("\n") },
        if section.dynamic_fields.is_empty() { "- 无".into() } else { section.dynamic_fields.iter().map(|item| format!("- {}", item.trim())).collect::<Vec<_>>().join("\n") },
    )
}

fn corrected_section_metadata(
    metadata_json: &str,
    previous_master_id: i64,
) -> Result<String, String> {
    let mut metadata = serde_json::from_str::<serde_json::Value>(metadata_json)
        .unwrap_or_else(|_| json!({ "legacyMetadata": metadata_json }));
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| "母稿章节元数据不是对象，无法创建可追溯校正版本".to_string())?;
    object.insert("parentMasterScriptId".into(), json!(previous_master_id));
    object.insert(
        "correctionPolicy".into(),
        json!("deterministic_model_format_only"),
    );
    serde_json::to_string(&metadata).map_err(|error| error.to_string())
}

fn render_corrected_batch_master_index(
    detail: &MasterSampleBatchDetail,
    previous: &MasterScriptRow,
    sections: &[NewMasterSection],
    correction_count: usize,
) -> String {
    let section_index = sections
        .iter()
        .map(|section| format!("- {:02}. {}", section.position, section.title.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# {}\n\n- 母稿编号：{}\n- 版本：V1.1\n- 上一版本：V1.0\n- 样本批次：{}\n- 样本场次：{}\n- 状态：已审核发布\n\n## 本次校正\n\n- 已应用 {} 处可确定的型号格式校正，例如 `R七 → R7`、`叉 D → XD`。\n- 未能由公司参数卡或确定规则验证的型号、价格、成色和口语词保持原样，避免擅自改写主播原话。\n\n## 使用边界\n\n固定原话仅来自已校正的主播逐字稿；价格、库存、赠品、链接和商品型号均需以当场事实为准。\n\n## 章节\n\n{}\n",
        previous.title.trim(),
        previous.script_key,
        detail.batch.title.trim(),
        detail.items.len(),
        correction_count,
        section_index,
    )
}

fn render_corrected_batch_master_section(script_key: &str, section: &NewMasterSection) -> String {
    let metadata = serde_json::from_str::<serde_json::Value>(&section.metadata_json)
        .unwrap_or_else(|_| json!({}));
    let evidence_ids = metadata
        .get("evidenceIds")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join("、")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "沿用 V1.0 证据链".into());
    format!(
        "# {}\n\n- 母稿编号：{}\n- 版本：V1.1\n- 章节：{}\n- 证据编号：{}\n\n## 固定原话\n\n{}\n\n## 校正说明\n\n仅处理可确定的型号格式；未确认内容保留原转写，等待人工审核。\n",
        section.title.trim(),
        script_key,
        section.position,
        evidence_ids,
        section.master_text.trim(),
    )
}

fn normalized_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

async fn attach_correctable_model_format_issue_count(
    state: &State,
    detail: &mut MasterSampleBatchDetail,
) {
    if detail.batch.status != "published" {
        return;
    }
    let Ok(script_key) = builder::safe_key(&format!("MS-BATCH-{}", detail.batch.id)) else {
        return;
    };
    let Ok(previous) = state.db.get_latest_published_master(&script_key).await else {
        return;
    };
    if previous.version != "1.0.0" {
        detail.correctable_model_format_issue_count = Some(0);
        return;
    }
    let Ok(sections) = state.db.list_master_sections(previous.id).await else {
        return;
    };
    detail.correctable_model_format_issue_count = Some(
        sections
            .iter()
            .filter(|section| {
                has_deterministic_model_format_correction(&section.host_text, &section.master_text)
            })
            .count() as i64,
    );
}

fn has_deterministic_model_format_correction(host_text: &str, master_text: &str) -> bool {
    crate::subtitle_generator::transcript_artifacts::normalize_letter_prefixed_model_numbers(
        host_text,
    ) != host_text
        || crate::subtitle_generator::transcript_artifacts::normalize_letter_prefixed_model_numbers(
            master_text,
        ) != master_text
}

#[cfg(test)]
mod correction_status_tests {
    use super::{
        apply_upgrade_candidate_text, has_deterministic_model_format_correction,
        sanitize_host_knowledge_base_name, upgrade_decision_rank,
    };

    #[test]
    fn builds_host_knowledge_base_folder_name() {
        assert_eq!(
            sanitize_host_knowledge_base_name("于千惠").unwrap(),
            "于千惠知识库"
        );
        assert_eq!(
            sanitize_host_knowledge_base_name("于千惠知识库").unwrap(),
            "于千惠知识库"
        );
        assert!(sanitize_host_knowledge_base_name("  ")
            .unwrap_err()
            .contains("主播名"));
        assert!(sanitize_host_knowledge_base_name("a/b")
            .unwrap()
            .contains("知识库"));
    }

    #[test]
    fn recognizes_only_deterministic_model_format_corrections() {
        assert!(has_deterministic_model_format_correction(
            "R七的镜头",
            "正常内容"
        ));
        assert!(has_deterministic_model_format_correction(
            "正常内容",
            "这个叉D好成色"
        ));
        assert!(!has_deterministic_model_format_correction(
            "普通中文原话",
            "A二手相机"
        ));
    }

    fn comparison_json(decision: &str) -> String {
        format!(
            r#"{{
              "comparison": {{"totalScore": 88}},
              "upgradeReview": {{
                "comparisonDecision": "{decision}",
                "trainingSuggestion": "[建议稿] 更清楚的表达"
              }}
            }}"#
        )
    }

    #[test]
    fn upgrade_preview_uses_prompt_three_decision_instead_of_blind_append() {
        assert!(apply_upgrade_candidate_text(
            "母稿原文",
            "主播原话",
            &comparison_json("add_as_support")
        )
        .unwrap()
        .contains("## 候选辅稿\n主播原话"));
        assert!(apply_upgrade_candidate_text(
            "母稿原文",
            "主播金句",
            &comparison_json("add_as_golden_sentence")
        )
        .unwrap()
        .contains("## 金句话术\n主播金句"));
        assert!(apply_upgrade_candidate_text(
            "母稿原文",
            "互补原话",
            &comparison_json("merge_with_existing")
        )
        .unwrap()
        .contains("## 待合并话术\n互补原话"));
        assert!(apply_upgrade_candidate_text(
            "母稿原文",
            "主播原话",
            &comparison_json("replace_existing")
        )
        .unwrap()
        .contains("## 建议替换稿\n[建议稿] 更清楚的表达"));
        assert!(apply_upgrade_candidate_text(
            "母稿原文",
            "重复内容",
            &comparison_json("duplicate")
        )
        .is_err());
    }

    #[test]
    fn replacement_is_applied_before_additive_upgrade_decisions() {
        assert_eq!(
            upgrade_decision_rank(&comparison_json("replace_existing")).unwrap(),
            0
        );
        assert_eq!(
            upgrade_decision_rank(&comparison_json("add_as_support")).unwrap(),
            1
        );
        assert_eq!(
            upgrade_decision_rank(&comparison_json("add_as_golden_sentence")).unwrap(),
            1
        );
        assert_eq!(
            upgrade_decision_rank(&comparison_json("merge_with_existing")).unwrap(),
            1
        );
        assert!(upgrade_decision_rank(&comparison_json("duplicate")).is_err());
    }
}

struct PreparedVideoIngest {
    source_key: String,
    media_path: String,
    media_hash: String,
    duration_ms: u64,
    selected_cards: Vec<master_ingest::ParameterCard>,
    transcriber: VolcengineChunkTranscriber,
    artifacts: TranscriptArtifactSink,
}

impl PreparedVideoIngest {
    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            source_key: self.source_key.clone(),
            media_path: self.media_path.clone(),
            media_hash: self.media_hash.clone(),
            duration_ms: self.duration_ms,
        }
    }

    fn request(&self, source_id: i64) -> IngestRequest {
        IngestRequest {
            source_id,
            source_key: self.source_key.clone(),
            source_hash: self.media_hash.clone(),
            duration_ms: self.duration_ms,
            chunk_duration_ms: 600_000,
            selected_cards: self.selected_cards.clone(),
        }
    }
}

async fn prepare_video_ingest(
    state: &State,
    request: &MasterVideoIngestRequest,
) -> Result<PreparedVideoIngest, String> {
    let context = resolve_video_transcript_context(state, request.video_id).await?;
    let video = state.db.get_video(context.video_id).await?;
    let duration_ms = if video.length > 0 {
        u64::try_from(video.length)
            .map_err(|error| error.to_string())?
            .saturating_mul(1000)
    } else {
        crate::ffmpeg::probe_media_duration_ms(&context.media_file).await?
    };
    let metadata = tokio::fs::metadata(&context.media_file)
        .await
        .map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let source_key = format!("video:{}", context.video_id);
    let media_hash = format!(
        "{:x}",
        md5::compute(format!(
            "{}\n{}\n{}\n{}",
            source_key,
            metadata.len(),
            modified,
            duration_ms
        ))
    );
    let available_cards = state
        .db
        .list_asr_parameter_cards()
        .await
        .map_err(String::from)?
        .into_iter()
        .filter_map(parameter_card_from_record)
        .collect::<Vec<_>>();
    let selected_cards = select_parameter_cards(
        &request.title,
        &request.recognized_terms,
        &request.manually_selected_card_ids,
        &available_cards,
        MAX_PARAMETER_CARDS,
    )
    .cards;
    let config = state.config.read().await;
    let transcriber = VolcengineChunkTranscriber::configured(
        &context.media_file,
        context.artifact_dir.join("volcengine-master-chunks"),
        &config,
    )?;
    drop(config);
    let artifacts = TranscriptArtifactSink::new(context.source, &context.artifact_dir);

    Ok(PreparedVideoIngest {
        source_key,
        media_path: context.media_file.to_string_lossy().to_string(),
        media_hash,
        duration_ms,
        selected_cards,
        transcriber,
        artifacts,
    })
}
