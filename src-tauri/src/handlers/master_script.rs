use crate::database::master_script::{
    MasterChunkRow, MasterScriptRow, MasterSourceRow, NewMasterSection, NewMasterSource,
    NewMasterVersion,
};
use crate::handlers::video::resolve_video_transcript_context;
use crate::master_script::{builder, model};
use crate::master_script::{
    parameter_card_from_record, resume_master_ingest as run_resume_master_ingest,
    start_master_ingest as run_start_master_ingest, DatabaseCheckpointStore,
    TranscriptArtifactSink, VolcengineChunkTranscriber,
};
use crate::state::State;
use crate::state_type;
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
use std::time::UNIX_EPOCH;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterBaseline {
    pub master: MasterScriptRow,
    pub sections: Vec<crate::database::master_script::MasterSectionRow>,
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
    let api_key = state.config.read().await.openai_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("MiniMax API Key 尚未配置，无法整理母稿".into());
    }
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
        let support_text = selected
            .iter()
            .map(|candidate| candidate.host_text.trim())
            .collect::<Vec<_>>()
            .join("\n\n");
        changes.push(MasterUpgradeSection {
            section_id: section.id,
            section_key: section.section_key,
            current_text: section.master_text.clone(),
            next_text: format!(
                "{}\n\n## 候选辅稿\n{}",
                section.master_text.trim(),
                support_text
            ),
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

struct MasterTruth {
    transcript: Vec<TranscriptCue>,
    parameter_cards: Vec<ParameterFactCard>,
    pending_critical_count: usize,
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
    if context.media_file.to_string_lossy() != source.media_path {
        return Err("母稿来源视频路径已变化，请重新导入".into());
    }
    let bundle =
        crate::subtitle_generator::transcript_artifacts::TranscriptArtifactStore::load_from_dir(
            context.source,
            &context.artifact_dir,
        )
        .await
        .map_err(|error| error.to_string())?;
    let transcript = master_ingest::parse_srt_cues(&bundle.corrected_srt)?
        .into_iter()
        .enumerate()
        .map(|(index, cue)| TranscriptCue {
            id: (index + 1) as u64,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.text,
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

fn normalized_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
