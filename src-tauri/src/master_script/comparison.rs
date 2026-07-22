use crate::database::master_script::{MasterSectionRow, NewSupportCandidate, SupportCandidateRow};
use crate::handlers::ai::request_minimax_text;
use crate::handlers::transcript_review::load_transcript_audit;
use crate::state::State;
use crate::subtitle_generator::transcript_artifacts::TranscriptSource;
use master_comparison::{
    compare_to_master, parse_model_assessment, MasterComparison, MasterSectionRef, MasterSnapshot,
    ModelAssessment,
};
use master_script::{CandidateAdmission, MasterSectionKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareHighlightRequest {
    pub script_key: String,
    pub expected_master_script_id: i64,
    pub source: TranscriptSource,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub product_card_id: Option<String>,
    pub section_kind: MasterSectionKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareHighlightResult {
    pub comparison: MasterComparison,
    pub candidate: Option<SupportCandidateRow>,
}

pub async fn compare_highlight_to_master(
    state: &State,
    request: CompareHighlightRequest,
) -> Result<CompareHighlightResult, String> {
    if request.source_start_ms >= request.source_end_ms {
        return Err("候选片段时间范围无效".into());
    }
    let source_start_ms =
        i64::try_from(request.source_start_ms).map_err(|error| error.to_string())?;
    let source_end_ms = i64::try_from(request.source_end_ms).map_err(|error| error.to_string())?;
    let script_key = super::builder::safe_key(&request.script_key)?;
    let master = state
        .db
        .get_latest_published_master(&script_key)
        .await
        .map_err(String::from)?;
    let section_rows = state
        .db
        .list_master_sections(master.id)
        .await
        .map_err(String::from)?;
    let snapshot = snapshot_from_rows(&master, &section_rows)?;
    if snapshot.id != request.expected_master_script_id {
        return Err(format!(
            "母稿已更新为 {}，请按最新版本重新评分",
            snapshot.version
        ));
    }
    let matched = snapshot.sections.iter().any(|section| {
        section.kind == request.section_kind
            && section.product_card_id.as_deref() == request.product_card_id.as_deref()
    });
    if !matched {
        return Ok(CompareHighlightResult {
            comparison: unmatched_comparison(&snapshot),
            candidate: None,
        });
    }

    let audit = load_transcript_audit(state, request.source.clone()).await?;
    let all_cues = master_ingest::parse_srt_cues(&audit.corrected_srt)?;
    let clip_cues = all_cues
        .into_iter()
        .enumerate()
        .filter(|(_, cue)| {
            cue.end_ms >= request.source_start_ms && cue.start_ms <= request.source_end_ms
        })
        .map(|(index, cue)| master_builder::TranscriptCue {
            id: (index + 1) as u64,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.text,
        })
        .collect::<Vec<_>>();
    if clip_cues.is_empty() {
        return Err("候选片段内没有可用于评分的逐字稿".into());
    }
    let matched_section = section_rows
        .iter()
        .find(|section| {
            section.section_kind == request.section_kind.as_str()
                && section.product_card_id.as_deref() == request.product_card_id.as_deref()
        })
        .ok_or("母稿章节匹配状态不一致，请刷新后重试")?;
    let api_key = state.config.read().await.openai_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("MiniMax API Key 尚未配置，无法对比母稿".into());
    }
    let mut assessment = request_assessment(
        &api_key,
        matched_section,
        &clip_cues,
        audit.pending_critical_count,
    )
    .await?;
    assessment.gates.transcript_reviewed = audit.pending_critical_count == 0;
    assessment.gates.master_section_matched = true;
    assessment.gates.host_speech_backed = !assessment.evidence_cue_ids.is_empty();
    let source_key = source_key(&request.source);
    let clip_text = clip_cues
        .iter()
        .map(|cue| cue.text.trim())
        .collect::<Vec<_>>()
        .join("\n");
    let transcript_hash = super::builder::content_hash(&clip_text);
    let duplicate = state
        .db
        .list_support_candidates(None)
        .await
        .map_err(String::from)?
        .into_iter()
        .any(|candidate| {
            candidate.master_script_id == master.id
                && candidate.master_section_id == matched_section.id
                && candidate.source_key == source_key
                && candidate.source_start_ms == source_start_ms
                && candidate.source_end_ms == source_end_ms
                && candidate.transcript_hash == transcript_hash
        });
    assessment.gates.not_duplicate = !duplicate;
    if audit.pending_critical_count > 0 {
        assessment.gates.reasons.push(format!(
            "还有 {} 条关键逐字稿待确认",
            audit.pending_critical_count
        ));
    }
    if assessment.evidence_cue_ids.is_empty() {
        assessment
            .gates
            .reasons
            .push("模型没有引用当前片段的逐字稿证据".into());
    }
    if duplicate {
        assessment
            .gates
            .reasons
            .push("这个片段已经进入过候选辅稿，无需重复添加".into());
    }
    let cue_ids = clip_cues.iter().map(|cue| cue.id).collect::<Vec<_>>();
    let comparison = compare_to_master(
        &snapshot,
        request.expected_master_script_id,
        request.product_card_id.as_deref(),
        request.section_kind,
        &cue_ids,
        assessment,
    )
    .map_err(|error| error.to_string())?;
    let candidate = if comparison.admission == Some(CandidateAdmission::CandidateQueue) {
        Some(persist_candidate(state, &request, &clip_cues, &comparison).await?)
    } else {
        None
    };
    Ok(CompareHighlightResult {
        comparison,
        candidate,
    })
}

fn snapshot_from_rows(
    master: &crate::database::master_script::MasterScriptRow,
    sections: &[MasterSectionRow],
) -> Result<MasterSnapshot, String> {
    Ok(MasterSnapshot {
        id: master.id,
        version: master.version.clone(),
        sections: sections
            .iter()
            .map(|section| {
                Ok(MasterSectionRef {
                    id: section.id,
                    product_card_id: section.product_card_id.clone(),
                    kind: parse_section_kind(&section.section_kind)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn parse_section_kind(value: &str) -> Result<MasterSectionKind, String> {
    match value {
        "opening" => Ok(MasterSectionKind::Opening),
        "product" => Ok(MasterSectionKind::Product),
        "transition" => Ok(MasterSectionKind::Transition),
        "scenario" => Ok(MasterSectionKind::Scenario),
        "closing" => Ok(MasterSectionKind::Closing),
        _ => Err(format!("母稿章节类型无效：{value}")),
    }
}

fn unmatched_comparison(master: &MasterSnapshot) -> MasterComparison {
    MasterComparison {
        master_script_id: master.id,
        master_version: master.version.clone(),
        master_section_id: None,
        score: None,
        total_score: None,
        admission: None,
        gates: None,
        improvements: vec![],
        risks: vec![],
        suggested_insertion_point: None,
    }
}

async fn request_assessment(
    api_key: &str,
    section: &MasterSectionRow,
    clip_cues: &[master_builder::TranscriptCue],
    pending_critical_count: usize,
) -> Result<ModelAssessment, String> {
    let response = request_minimax_text(
        api_key,
        comparison_system_prompt(),
        vec![json!({
            "role": "user",
            "content": json!({
                "母稿章节": {
                    "sectionId": section.id,
                    "kind": section.section_kind,
                    "productCardId": section.product_card_id,
                    "masterText": section.master_text,
                },
                "候选片段逐字稿": clip_cues,
                "待确认关键纠错数": pending_critical_count,
            }).to_string(),
        })],
        4_096,
    )
    .await?;
    parse_model_assessment(&response)
}

async fn persist_candidate(
    state: &State,
    request: &CompareHighlightRequest,
    clip_cues: &[master_builder::TranscriptCue],
    comparison: &MasterComparison,
) -> Result<SupportCandidateRow, String> {
    let host_text = clip_cues
        .iter()
        .map(|cue| cue.text.trim())
        .collect::<Vec<_>>()
        .join("\n");
    let transcript_hash = super::builder::content_hash(&host_text);
    let source_key = source_key(&request.source);
    let master_section_id = comparison.master_section_id.ok_or("候选缺少母稿章节")?;
    let candidate_key = super::builder::content_hash(&format!(
        "{}:{}:{}:{}:{}:{}",
        source_key,
        request.source_start_ms,
        request.source_end_ms,
        transcript_hash,
        comparison.master_script_id,
        master_section_id
    ));
    let gates = comparison.gates.as_ref().ok_or("候选缺少硬门槛结果")?;
    let score = comparison.score.as_ref().ok_or("候选缺少分项分数")?;
    state
        .db
        .insert_support_candidate(NewSupportCandidate {
            candidate_key,
            master_script_id: comparison.master_script_id,
            master_section_id,
            source_key,
            source_start_ms: i64::try_from(request.source_start_ms)
                .map_err(|error| error.to_string())?,
            source_end_ms: i64::try_from(request.source_end_ms)
                .map_err(|error| error.to_string())?,
            transcript_hash,
            host_text,
            comparison_json: serde_json::to_string(comparison)
                .map_err(|error| error.to_string())?,
            gates_json: serde_json::to_string(gates).map_err(|error| error.to_string())?,
            score_json: serde_json::to_string(score).map_err(|error| error.to_string())?,
            admission: CandidateAdmission::CandidateQueue,
        })
        .await
        .map_err(String::from)
}

fn source_key(source: &TranscriptSource) -> String {
    match source {
        TranscriptSource::Video { video_id } => format!("video:{video_id}"),
        TranscriptSource::Archive {
            platform,
            room_id,
            live_id,
        } => format!("archive:{platform}:{room_id}:{live_id}"),
    }
}

fn comparison_system_prompt() -> &'static str {
    r#"你是企业直播片段对比评分器。只能输出一个合法 JSON 对象，字段严格为 gates、score、evidenceCueIds、improvements、risks、suggestedInsertionPoint。
gates 必须含 transcriptReviewed、masterSectionMatched、contextComplete、factsResolved、transactionEvidenceValid、hostSpeechBacked、notDuplicate、reasons。
score 必须含 transactionEvidence(0-25)、improvementOverMaster(0-25)、reusability(0-20)、completeness(0-15)、factualAccuracy(0-10)、scenarioClarity(0-5)，不要输出总分和录取结论。
所有判断必须引用候选片段中的真实 cue ID。不得推断价格、库存、优惠、成色、链接号或成交状态。候选片段优于母稿的部分只写 improvements；不得改写成主播原话。"#
}
