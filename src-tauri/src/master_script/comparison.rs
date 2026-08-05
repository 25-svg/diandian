use crate::database::master_script::{
    MasterSectionRow, MasterUpgradeReviewRow, NewCompetitorReferenceCandidate,
    NewMasterUpgradeReview, NewSupportCandidate, SupportCandidateRow,
};
use crate::handlers::ai::request_minimax_text;
use crate::handlers::transcript_review::load_transcript_audit;
use crate::state::State;
use crate::subtitle_generator::transcript_artifacts::TranscriptSource;
use master_comparison::{
    compare_to_master, is_actionable_upgrade_decision, master_source_baseline,
    parse_model_assessment, parse_upgrade_review, prompt_two_is_upgrade_eligible,
    validate_upgrade_review_for_prompt_two, MasterComparison, MasterSectionRef, MasterSnapshot,
    MasterUpgradeReview, ModelAssessment, RiskStatus, SegmentDecision, SegmentQualityReview,
    UpgradeComparisonDecision,
};
use master_script::{CandidateAdmission, MasterSectionKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareHighlightRequest {
    pub script_key: String,
    pub expected_master_script_id: i64,
    #[serde(default)]
    pub master_section_id: Option<i64>,
    pub source: TranscriptSource,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    #[serde(default)]
    pub candidate_type: String,
    #[serde(default)]
    pub chain_stages: Vec<String>,
    pub candidate_segment: CandidateSegmentInput,
    pub product_card_id: Option<String>,
    pub section_kind: MasterSectionKind,
    #[serde(default = "default_comparison_mode")]
    pub comparison_mode: String,
    #[serde(default)]
    pub competitor_name: Option<String>,
}

fn default_comparison_mode() -> String {
    "enterprise_upgrade".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSegmentInput {
    pub segment_id: String,
    pub segment_type: String,
    #[serde(default)]
    pub scene: String,
    #[serde(default)]
    pub customer_need: String,
    #[serde(default)]
    pub original_text: String,
    #[serde(default)]
    pub key_sentence: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default)]
    pub why_selected: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareHighlightResult {
    pub comparison: MasterComparison,
    pub candidate: Option<SupportCandidateRow>,
    pub upgrade_review_id: Option<i64>,
    pub upgrade_review: Option<MasterUpgradeReview>,
    pub upgrade_review_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpgradeReviewInputSnapshot {
    request: CompareHighlightRequest,
    host_text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryUpgradeReviewResult {
    pub review_id: i64,
    pub upgrade_review: MasterUpgradeReview,
    pub candidate: Option<SupportCandidateRow>,
}

pub async fn compare_highlight_to_master(
    state: &State,
    request: CompareHighlightRequest,
) -> Result<CompareHighlightResult, String> {
    let competitor_benchmark = request.comparison_mode == "competitor_benchmark";
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
    let matched_section = match request.master_section_id {
        Some(section_id) => section_rows.iter().find(|section| section.id == section_id),
        None => section_rows.iter().find(|section| {
            section.section_kind == request.section_kind.as_str()
                && section.product_card_id.as_deref() == request.product_card_id.as_deref()
        }),
    };
    let Some(matched_section) = matched_section else {
        return Ok(CompareHighlightResult {
            comparison: unmatched_comparison(&snapshot),
            candidate: None,
            upgrade_review_id: None,
            upgrade_review: None,
            upgrade_review_error: None,
        });
    };

    let current_source_key = source_key(&request.source);
    let master_source = state
        .db
        .get_master_source(master.source_id)
        .await
        .map_err(String::from)?;
    if master_source.source_key == current_source_key {
        return Ok(CompareHighlightResult {
            comparison: master_source_baseline(&snapshot, matched_section.id),
            candidate: None,
            upgrade_review_id: None,
            upgrade_review: None,
            upgrade_review_error: None,
        });
    }

    if !is_scoreable_segment_type(&request.candidate_type) {
        return Err(format!("不支持的候选片段类型：{}", request.candidate_type));
    }
    if request.candidate_segment.segment_type != request.candidate_type {
        return Err("候选片段类型与评分请求不一致".into());
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
    let api_key = state.config.read().await.openai_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("MiniMax API Key 尚未配置，无法对比母稿".into());
    }
    let mut assessment = request_assessment(
        &api_key,
        matched_section,
        &clip_cues,
        audit.pending_critical_count,
        &request.candidate_segment,
        verified_product_facts(state, matched_section.product_card_id.as_deref()).await?,
    )
    .await?;
    if let Some(review) = assessment.quality_review.as_ref() {
        validate_model_review_identity(&request.candidate_segment.segment_id, review)?;
    }
    assessment.gates.transcript_reviewed = audit.pending_critical_count == 0;
    assessment.gates.master_section_matched = true;
    assessment.gates.host_speech_backed = !assessment.evidence_cue_ids.is_empty();
    if audit.pending_critical_count > 0 {
        if let Some(review) = assessment.quality_review.as_mut() {
            if review.risk_status == RiskStatus::Passed {
                review.risk_status = RiskStatus::NeedsReview;
            }
            if matches!(
                review.decision,
                SegmentDecision::SupportCandidate | SegmentDecision::GoldenSentence
            ) {
                review.decision = SegmentDecision::TrainingMaterial;
            }
            review
                .facts_to_confirm
                .push("逐字稿仍有关键内容待人工确认".into());
        }
    }
    if request.candidate_segment.outcome == "confirmed_conversion"
        && !has_confirmed_conversion_evidence(
            &clip_cues
                .iter()
                .map(|cue| cue.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    {
        if let Some(review) = assessment.quality_review.as_mut() {
            if review.risk_status == RiskStatus::Passed {
                review.risk_status = RiskStatus::NeedsReview;
            }
            if matches!(
                review.decision,
                SegmentDecision::SupportCandidate | SegmentDecision::GoldenSentence
            ) {
                review.decision = SegmentDecision::TrainingMaterial;
            }
            review
                .facts_to_confirm
                .push("逐字稿没有发现明确的下单、已拍或付款证据，成交状态需人工确认".into());
        }
    }
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
    // A duplicate must not create another support-candidate row, but it must
    // not erase the quality score when a user reopens an already reviewed clip.
    assessment.gates.not_duplicate = true;
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
        Some(matched_section.id),
        request.product_card_id.as_deref(),
        request.section_kind,
        &cue_ids,
        assessment,
    )
    .map_err(|error| error.to_string())?;
    if competitor_benchmark && !duplicate {
        persist_competitor_reference_candidate(
            state,
            &request,
            &clip_text,
            &transcript_hash,
            &comparison,
        )
        .await?;
    }
    let mut upgrade_review_id = None;
    let mut upgrade_review = None;
    let mut upgrade_review_error = None;
    if !competitor_benchmark
        && comparison
            .quality_review
            .as_ref()
            .is_some_and(prompt_two_is_upgrade_eligible)
    {
        let snapshot = UpgradeReviewInputSnapshot {
            request: request.clone(),
            host_text: clip_text.clone(),
        };
        let review_key = super::builder::content_hash(&format!(
            "{}:{}:{}:{}:{}:{}",
            master.id,
            matched_section.id,
            source_key,
            source_start_ms,
            source_end_ms,
            transcript_hash
        ));
        let job = state
            .db
            .create_or_get_master_upgrade_review(NewMasterUpgradeReview {
                review_key,
                master_script_id: master.id,
                master_section_id: matched_section.id,
                source_key: source_key.clone(),
                source_start_ms,
                source_end_ms,
                transcript_hash: transcript_hash.clone(),
                candidate_json: serde_json::to_string(&snapshot)
                    .map_err(|error| error.to_string())?,
                prompt_two_json: serde_json::to_string(&comparison)
                    .map_err(|error| error.to_string())?,
                master_section_json: serde_json::to_string(matched_section)
                    .map_err(|error| error.to_string())?,
            })
            .await
            .map_err(String::from)?;
        upgrade_review_id = Some(job.id);
        match resolve_upgrade_review_job(
            state,
            &api_key,
            job,
            comparison.quality_review.as_ref().expect("checked above"),
            matched_section,
            &request.candidate_segment.original_text,
        )
        .await
        {
            Ok(review) => upgrade_review = Some(review),
            Err(error) => upgrade_review_error = Some(error),
        }
    }
    // Competitor recordings may be scored against MS-BATCH-4, but can never
    // enter the enterprise support-candidate or master-upgrade pipeline.
    let candidate = if !competitor_benchmark
        && !duplicate
        && upgrade_review.as_ref().is_some_and(|review| {
            upgrade_decision_enters_candidate_queue(review.comparison_decision)
        }) {
        Some(
            persist_candidate(
                state,
                &request,
                &clip_text,
                &transcript_hash,
                &comparison,
                upgrade_review.as_ref(),
            )
            .await?,
        )
    } else {
        None
    };
    Ok(CompareHighlightResult {
        comparison,
        candidate,
        upgrade_review_id,
        upgrade_review,
        upgrade_review_error,
    })
}

pub async fn retry_master_upgrade_review(
    state: &State,
    review_id: i64,
) -> Result<RetryUpgradeReviewResult, String> {
    let job = state
        .db
        .get_master_upgrade_review(review_id)
        .await
        .map_err(String::from)?;
    let snapshot = serde_json::from_str::<UpgradeReviewInputSnapshot>(&job.candidate_json)
        .map_err(|error| format!("母稿比较候选快照无法读取：{error}"))?;
    let comparison = serde_json::from_str::<MasterComparison>(&job.prompt_two_json)
        .map_err(|error| format!("母稿比较评分快照无法读取：{error}"))?;
    let section = serde_json::from_str::<MasterSectionRow>(&job.master_section_json)
        .map_err(|error| format!("母稿比较章节快照无法读取：{error}"))?;
    let base = state
        .db
        .get_master_version(job.master_script_id)
        .await
        .map_err(String::from)?;
    let latest = state
        .db
        .get_latest_published_master(&base.script_key)
        .await
        .map_err(String::from)?;
    if latest.id != job.master_script_id {
        return Err("企业母稿已更新，这条旧版比较不能直接用于新版升级".into());
    }
    let prompt_two = comparison
        .quality_review
        .as_ref()
        .ok_or("母稿比较缺少提示词二评分结果")?;
    if !prompt_two_is_upgrade_eligible(prompt_two) {
        return Err("这条片段不满足提示词三的调用条件".into());
    }
    let api_key = state.config.read().await.openai_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("MiniMax API Key 尚未配置，无法比较企业母稿".into());
    }
    let review = resolve_upgrade_review_job(
        state,
        &api_key,
        job.clone(),
        prompt_two,
        &section,
        &snapshot.request.candidate_segment.original_text,
    )
    .await?;
    let candidate = if is_actionable_upgrade_decision(review.comparison_decision) {
        Some(
            persist_candidate(
                state,
                &snapshot.request,
                &snapshot.host_text,
                &job.transcript_hash,
                &comparison,
                Some(&review),
            )
            .await?,
        )
    } else {
        None
    };
    Ok(RetryUpgradeReviewResult {
        review_id,
        upgrade_review: review,
        candidate,
    })
}

const SCOREABLE_SEGMENT_TYPES: [&str; 12] = [
    "完整成交链路",
    "高质量金句",
    "需求判断",
    "产品推荐",
    "产品讲解",
    "异议处理",
    "留人钩子",
    "信任建立",
    "售后与风险消除",
    "价格、优惠或链接承接",
    "催单与成交确认",
    "需要改进的反面案例",
];

fn is_scoreable_segment_type(segment_type: &str) -> bool {
    SCOREABLE_SEGMENT_TYPES.contains(&segment_type)
}

fn validate_model_review_identity(
    expected_segment_id: &str,
    review: &SegmentQualityReview,
) -> Result<(), String> {
    if review.segment_id != expected_segment_id {
        return Err("模型返回的片段编号与当前候选片段不一致".into());
    }
    if !is_scoreable_segment_type(&review.segment_type) {
        return Err(format!(
            "模型返回了不支持的片段类型：{}",
            review.segment_type
        ));
    }
    Ok(())
}

fn upgrade_decision_enters_candidate_queue(
    decision: master_comparison::UpgradeComparisonDecision,
) -> bool {
    master_comparison::is_actionable_upgrade_decision(decision)
}

fn has_confirmed_conversion_evidence(text: &str) -> bool {
    [
        "已经下单",
        "已下单",
        "下单了",
        "已经拍了",
        "已经拍下",
        "已拍",
        "拍好了",
        "付款了",
        "已付款",
        "支付成功",
        "订单成功",
    ]
    .iter()
    .any(|signal| text.contains(signal))
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
        is_master_source: false,
        score: None,
        total_score: None,
        admission: None,
        gates: None,
        verdict: String::new(),
        module_tags: vec![],
        improvements: vec![],
        risks: vec![],
        suggested_insertion_point: None,
        quality_review: None,
    }
}

async fn verified_product_facts(
    state: &State,
    product_card_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let Some(product_card_id) = product_card_id else {
        return Ok(json!([]));
    };
    let cards = state
        .db
        .list_asr_parameter_cards()
        .await
        .map_err(String::from)?;
    let facts = cards
        .into_iter()
        .filter(|card| card.card_id == product_card_id)
        .map(|card| {
            json!({
                "cardId": card.card_id,
                "title": card.title,
                "version": card.version,
                "metadata": serde_json::from_str::<serde_json::Value>(&card.metadata_json)
                    .unwrap_or_else(|_| json!({})),
                "confirmedContent": card.body,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!(facts))
}

async fn request_assessment(
    api_key: &str,
    section: &MasterSectionRow,
    clip_cues: &[master_builder::TranscriptCue],
    pending_critical_count: usize,
    candidate_segment: &CandidateSegmentInput,
    verified_product_facts: serde_json::Value,
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
                "候选片段": candidate_segment,
                "候选片段逐字稿": clip_cues,
                "已确认商品信息": verified_product_facts,
                "待确认关键纠错数": pending_critical_count,
            }).to_string(),
        })],
        4_096,
    )
    .await?;
    match parse_model_assessment(&response) {
        Ok(assessment) => Ok(assessment),
        Err(first_error) => {
            let repaired = request_minimax_text(
                api_key,
                comparison_repair_prompt(),
                vec![json!({
                    "role": "user",
                    "content": json!({
                        "上一轮原始输出": response,
                    }).to_string(),
                })],
                1_200,
            )
            .await
            .map_err(|repair_error| {
                format!(
                    "MiniMax 对比结果格式错误，且自动格式修复失败：{first_error}；{repair_error}"
                )
            })?;
            parse_model_assessment(&repaired).map_err(|repair_error| format!(
                "MiniMax 对比结果格式错误，系统已自动重试一次：{first_error}；重试后仍无效：{repair_error}"
            ))
        }
    }
}

async fn resolve_upgrade_review_job(
    state: &State,
    api_key: &str,
    job: MasterUpgradeReviewRow,
    prompt_two: &master_comparison::SegmentQualityReview,
    section: &MasterSectionRow,
    candidate_original_text: &str,
) -> Result<MasterUpgradeReview, String> {
    if job.status == "complete" {
        return serde_json::from_str(&job.review_json)
            .map_err(|error| format!("已保存的母稿升级比较无法读取：{error}"));
    }
    let result =
        request_upgrade_review(api_key, prompt_two, section, candidate_original_text).await;
    match result {
        Ok(review) => {
            let review_json = serde_json::to_string(&review).map_err(|error| error.to_string())?;
            state
                .db
                .complete_master_upgrade_review(
                    job.id,
                    upgrade_decision_as_str(review.comparison_decision),
                    &review_json,
                )
                .await
                .map_err(String::from)?;
            Ok(review)
        }
        Err(error) => {
            state
                .db
                .fail_master_upgrade_review(job.id, &error)
                .await
                .map_err(String::from)?;
            Err(error)
        }
    }
}

async fn request_upgrade_review(
    api_key: &str,
    prompt_two: &master_comparison::SegmentQualityReview,
    section: &MasterSectionRow,
    candidate_original_text: &str,
) -> Result<MasterUpgradeReview, String> {
    let expected_section = section.title.trim();
    let response = request_minimax_text(
        api_key,
        upgrade_review_system_prompt(),
        vec![json!({
            "role": "user",
            "content": json!({
                "候选话术": {
                    "qualityReview": prompt_two,
                    "candidateOriginalText": candidate_original_text,
                },
                "企业母稿相关章节": {
                    "sectionId": section.id,
                    "sectionKey": section.section_key,
                    "sectionTitle": expected_section,
                    "sectionKind": section.section_kind,
                    "productCardId": section.product_card_id,
                    "masterText": section.master_text,
                },
            }).to_string(),
        })],
        2_500,
    )
    .await?;
    let parse_and_validate = |raw: &str| -> Result<MasterUpgradeReview, String> {
        let review = parse_upgrade_review(raw, expected_section, candidate_original_text)?;
        validate_upgrade_review_for_prompt_two(&review, prompt_two)?;
        Ok(review)
    };
    match parse_and_validate(&response) {
        Ok(review) => Ok(review),
        Err(first_error) => {
            let repaired = request_minimax_text(
                api_key,
                upgrade_review_repair_prompt(),
                vec![json!({
                    "role": "user",
                    "content": json!({
                        "上一轮原始输出": response,
                        "必须保持的母稿章节": expected_section,
                        "主播原话范围": candidate_original_text,
                    }).to_string(),
                })],
                1_200,
            )
            .await
            .map_err(|repair_error| {
                format!(
                    "MiniMax 母稿比较格式错误，且自动格式修复失败：{first_error}；{repair_error}"
                )
            })?;
            parse_and_validate(&repaired).map_err(|repair_error| {
                    format!(
                        "MiniMax 母稿比较格式错误，系统已自动重试一次：{first_error}；重试后仍无效：{repair_error}"
                    )
                })
        }
    }
}

fn upgrade_decision_as_str(decision: UpgradeComparisonDecision) -> &'static str {
    match decision {
        UpgradeComparisonDecision::AddAsSupport => "add_as_support",
        UpgradeComparisonDecision::AddAsGoldenSentence => "add_as_golden_sentence",
        UpgradeComparisonDecision::ReplaceExisting => "replace_existing",
        UpgradeComparisonDecision::MergeWithExisting => "merge_with_existing",
        UpgradeComparisonDecision::Duplicate => "duplicate",
        UpgradeComparisonDecision::Reject => "reject",
    }
}

async fn persist_candidate(
    state: &State,
    request: &CompareHighlightRequest,
    host_text: &str,
    transcript_hash: &str,
    comparison: &MasterComparison,
    upgrade_review: Option<&MasterUpgradeReview>,
) -> Result<SupportCandidateRow, String> {
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
            transcript_hash: transcript_hash.to_string(),
            host_text: host_text.to_string(),
            comparison_json: serde_json::to_string(&json!({
                "comparison": comparison,
                "upgradeReview": upgrade_review,
            }))
            .map_err(|error| error.to_string())?,
            gates_json: serde_json::to_string(gates).map_err(|error| error.to_string())?,
            score_json: serde_json::to_string(score).map_err(|error| error.to_string())?,
            admission: CandidateAdmission::CandidateQueue,
        })
        .await
        .map_err(String::from)
}

async fn persist_competitor_reference_candidate(
    state: &State,
    request: &CompareHighlightRequest,
    host_text: &str,
    transcript_hash: &str,
    comparison: &MasterComparison,
) -> Result<(), String> {
    let master_section_id = comparison
        .master_section_id
        .ok_or("competitor comparison has no master section")?;
    let score = comparison
        .score
        .as_ref()
        .ok_or("competitor comparison has no score")?;
    let candidate_key = super::builder::content_hash(&format!(
        "competitor:{}:{}:{}:{}:{}",
        source_key(&request.source),
        request.source_start_ms,
        request.source_end_ms,
        transcript_hash,
        master_section_id,
    ));
    let decision = match comparison
        .quality_review
        .as_ref()
        .map(|review| review.decision)
    {
        Some(SegmentDecision::SupportCandidate | SegmentDecision::GoldenSentence) => {
            "better_phrasing"
        }
        Some(SegmentDecision::TrainingMaterial) => "migratable_structure",
        Some(SegmentDecision::ReferenceOnly) => "reference_only",
        _ => "not_applicable",
    };
    state
        .db
        .insert_competitor_reference_candidate(NewCompetitorReferenceCandidate {
            candidate_key,
            master_script_id: comparison.master_script_id,
            master_section_id,
            source_key: source_key(&request.source),
            competitor_name: request.competitor_name.clone().unwrap_or_default(),
            source_start_ms: i64::try_from(request.source_start_ms)
                .map_err(|error| error.to_string())?,
            source_end_ms: i64::try_from(request.source_end_ms)
                .map_err(|error| error.to_string())?,
            transcript_hash: transcript_hash.to_string(),
            host_text: host_text.to_string(),
            comparison_json: serde_json::to_string(comparison)
                .map_err(|error| error.to_string())?,
            migration_decision: decision.to_string(),
            total_score: i64::from(score.total()),
        })
        .await
        .map_err(String::from)?;
    Ok(())
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
    r#"你是“金典拍拍企业直播话术质检官”。请严格依据候选片段、企业母稿相关章节、已确认商品信息和带编号逐字稿评分。不得因为语言听起来专业就给高分，不得补充输入中没有出现的事实。只返回一个合法 JSON 对象，不要 Markdown。

先判断片段类型，再使用对应目标评价：
- 完整成交链路：需求到成交确认是否完整、自然、可复用。
- 高质量金句：说服力、自然度和可直接复用程度；不得因链路不完整直接扣低分。
- 需求判断：是否准确识别预算、用途、偏好和顾虑。
- 产品推荐：推荐是否匹配需求，理由是否明确。
- 产品讲解：信息是否准确，是否把参数转化为用户价值。
- 异议处理：是否理解顾虑、提供可信证据并推动下一步。
- 留人钩子：是否给出继续停留的明确收益或期待。
- 信任建立：是否通过检测、成色、来源、售后和发货承诺降低风险。
- 售后与风险消除：承诺是否真实、具体并能降低顾虑。
- 价格、优惠或链接承接：价格、优惠、链接与商品是否明确对应。
- 催单与成交确认：是否有明确行动指令和真实成交证据。
- 需要改进的反面案例：识别具体问题和训练价值，不得建议进入辅稿。

身份字段规则：
- segment_id 必须原样复制输入「候选片段」的 segmentId，不得使用下方示例值。
- segment_type 是本轮重新判断后的类型，可以不同于输入初判，但只能使用上面列出的十二种类型之一。
- 下方 JSON 中的 SEG-001 和“异议处理”仅用于说明格式，不是固定答案。

六项分数固定为：scene_goal 0-30、persuasiveness 0-20、master_increment 0-20、reusability 0-15、factual_accuracy 0-10、natural_expression 0-5。每项必须包含 score、max_score、reason、evidence；evidence 必须引用当前逐字稿中的真实时间和原话。total_score 必须等于六项之和。

硬性规则：
- 没有订单、下单、已拍、付款等明确证据，不得写“确认成交”。
- 价格、型号、库存、成色、优惠、链接未经确认时，加入 facts_to_confirm。
- 存在虚假承诺、极限词或事实冲突时，risk_status=blocked。
- 单句金句只评价该句使用价值，不要求补齐完整成交链路。
- 不能因为与母稿文字相似就给高分，必须判断是否更清楚、更自然或补充新方法。
- 每项得分必须引用真实证据，不得评价主播身份。
- reusable_original_sentence 只能摘录主播原话；suggested_training_version 是建议稿，不得冒充主播原话。

证据等级只能为 A、B、C。风险状态只能为 passed、needs_review、blocked。decision 只能为 support_candidate、golden_sentence、training_material、reference_only、blocked；系统会重新核算 decision。

只返回以下 JSON 形状。数组字段即使只有一条也必须使用数组；分数和 evidence_cue_ids 必须是 JSON 数字。evidence_cue_ids 只能填写「候选片段逐字稿」数组里每条 cue 的 id 字段（例如 1、2、3），禁止写成 cue_1 这类字符串：
{"segment_id":"SEG-001","segment_type":"异议处理","total_score":0,"decision":"reference_only","evidence_grade":"B","risk_status":"needs_review","score_details":{"scene_goal":{"score":0,"max_score":30,"reason":"评分理由","evidence":["00:01:05 原话"]},"persuasiveness":{"score":0,"max_score":20,"reason":"评分理由","evidence":["00:01:05 原话"]},"master_increment":{"score":0,"max_score":20,"reason":"比母稿新增了什么","evidence":["00:01:05 原话"]},"reusability":{"score":0,"max_score":15,"reason":"新人能否直接使用","evidence":["00:01:05 原话"]},"factual_accuracy":{"score":0,"max_score":10,"reason":"事实是否有依据","evidence":["00:01:05 原话"]},"natural_expression":{"score":0,"max_score":5,"reason":"是否自然简洁","evidence":["00:01:05 原话"]}},"what_is_good":["小白能看懂的具体优点"],"what_needs_improvement":["缺少的动作或不清楚表达"],"facts_to_confirm":["待确认的动态事实"],"reusable_original_sentence":"主播原话","suggested_training_version":"[建议稿] 优化表达","recommended_master_section":"建议加入的企业母稿场景","evidence_cue_ids":[1]}"#
}

fn comparison_repair_prompt() -> &'static str {
    r#"你只负责修复上一轮片段评分的 JSON 格式。只返回合法 JSON，不要解释、不要 Markdown。不得改变原判断、分数、证据或原话，不得补造事实。

字段固定为 segment_id、segment_type、total_score、decision、evidence_grade、risk_status、score_details、what_is_good、what_needs_improvement、facts_to_confirm、reusable_original_sentence、suggested_training_version、recommended_master_section、evidence_cue_ids。score_details 必须包含 scene_goal、persuasiveness、master_increment、reusability、factual_accuracy、natural_expression，每项包含数字 score、数字 max_score、字符串 reason、数组 evidence。natural_expression 的 max_score 只能是 5。三个说明列表和 evidence_cue_ids 始终使用数组；evidence_cue_ids 只能是数字 id，不能是 cue_1 字符串。"#
}

fn upgrade_review_system_prompt() -> &'static str {
    r#"你是“企业母稿升级审核官”。

请比较候选话术和企业母稿。目标不是找文字相似度，而是判断候选话术是否为企业母稿带来了新的、可复用的价值。

【比较标准】
1. 候选内容解决的场景是否与母稿一致。
2. 候选内容是否比母稿更清楚、更自然、更有说服力。
3. 候选内容是否补充了母稿没有的方法、证据、金句或承接动作。
4. 是否只是母稿原话的重复或轻微改写。
5. 是否存在未经确认的商品、价格、库存、链接、售后或成交事实。
6. 是否适合普通新人主播直接理解和使用。
7. 保留主播原话与建议改写稿的边界，不得混淆。

【决策】
- add_as_support：新增为辅稿。
- add_as_golden_sentence：加入金句话术库。
- replace_existing：明显优于现有表达，建议人工审核后替换。
- merge_with_existing：与现有章节互补，建议合并。
- duplicate：与现有内容重复。
- reject：证据不足、风险过高或没有复用价值。

不得补写输入中没有出现的商品、价格、库存、链接、售后或成交事实。original_host_words 只能逐字摘录 candidateOriginalText；training_suggestion 必须明确标记为“[建议稿]”，不得冒充主播原话。matched_master_section 必须逐字返回输入的 sectionTitle。

只返回一个合法 JSON 对象，不要 Markdown。数组字段始终使用数组：
{"comparison_decision":"add_as_support","matched_master_section":"异议处理/质量担忧","same_scene":true,"new_value":"候选话术比母稿新增的具体价值","why_better":["具体理由"],"duplicate_content":["与母稿重复的部分"],"risk_or_uncertainty":["仍需人工确认的内容"],"original_host_words":"主播原话","training_suggestion":"[建议稿] 建议表达，不得冒充主播原话","recommended_action":"建议放入哪个章节以及如何使用"}"#
}

fn upgrade_review_repair_prompt() -> &'static str {
    r#"你只负责修复上一轮企业母稿比较结果的 JSON 格式。只返回合法 JSON，不要解释、不要 Markdown。不得改变决策、母稿章节、主播原话、风险或建议内容，不得补造事实。

字段固定为 comparison_decision、matched_master_section、same_scene、new_value、why_better、duplicate_content、risk_or_uncertainty、original_host_words、training_suggestion、recommended_action。三个说明列表始终使用数组。comparison_decision 只能是 add_as_support、add_as_golden_sentence、replace_existing、merge_with_existing、duplicate、reject。"#
}

#[allow(dead_code)]
fn legacy_comparison_system_prompt() -> &'static str {
    r#"你是企业直播片段对比评分器。只能输出一个合法 JSON 对象，字段严格为 gates、score、evidenceCueIds、improvements、risks、suggestedInsertionPoint。
gates 必须含 transcriptReviewed、masterSectionMatched、contextComplete、factsResolved、transactionEvidenceValid、hostSpeechBacked、notDuplicate、reasons。
score 必须含 transactionEvidence(0-25)、improvementOverMaster(0-25)、reusability(0-20)、completeness(0-15)、factualAccuracy(0-10)、scenarioClarity(0-5)，不要输出总分和录取结论。
所有判断必须引用候选片段中的真实 cue ID。不得推断价格、库存、优惠、成色、链接号或成交状态。候选片段优于母稿的部分只写 improvements；不得改写成主播原话。"#
}

#[cfg(test)]
mod tests {
    use super::{
        has_confirmed_conversion_evidence, is_scoreable_segment_type,
        upgrade_decision_enters_candidate_queue, validate_model_review_identity,
    };
    use master_comparison::{
        EvidenceGrade, RiskStatus, ScoreDetail, ScoreDetails, SegmentDecision,
        SegmentQualityReview, UpgradeComparisonDecision,
    };

    fn model_review(segment_id: &str, segment_type: &str) -> SegmentQualityReview {
        let detail = |max_score| ScoreDetail {
            score: 0,
            max_score,
            reason: String::new(),
            evidence: Vec::new(),
        };
        SegmentQualityReview {
            segment_id: segment_id.into(),
            segment_type: segment_type.into(),
            total_score: 0,
            decision: SegmentDecision::ReferenceOnly,
            evidence_grade: EvidenceGrade::C,
            risk_status: RiskStatus::NeedsReview,
            score_details: ScoreDetails {
                scene_goal: detail(30),
                persuasiveness: detail(20),
                master_increment: detail(20),
                reusability: detail(15),
                factual_accuracy: detail(10),
                natural_expression: detail(5),
            },
            what_is_good: Vec::new(),
            what_needs_improvement: Vec::new(),
            facts_to_confirm: Vec::new(),
            reusable_original_sentence: String::new(),
            suggested_training_version: String::new(),
            recommended_master_section: String::new(),
        }
    }

    #[test]
    fn all_prompt_one_segment_types_can_enter_type_specific_scoring() {
        for segment_type in [
            "完整成交链路",
            "高质量金句",
            "需求判断",
            "产品推荐",
            "产品讲解",
            "异议处理",
            "留人钩子",
            "信任建立",
            "售后与风险消除",
            "价格、优惠或链接承接",
            "催单与成交确认",
            "需要改进的反面案例",
        ] {
            assert!(is_scoreable_segment_type(segment_type), "{segment_type}");
        }
        assert!(!is_scoreable_segment_type("普通聊天"));
    }

    #[test]
    fn model_can_reclassify_the_candidate_but_cannot_change_its_identity() {
        assert!(validate_model_review_identity(
            "C08-3817-3933",
            &model_review("C08-3817-3933", "产品讲解"),
        )
        .is_ok());
        assert!(validate_model_review_identity(
            "C08-3817-3933",
            &model_review("C09-4000-4100", "产品讲解"),
        )
        .unwrap_err()
        .contains("片段编号"));
        assert!(validate_model_review_identity(
            "C08-3817-3933",
            &model_review("C08-3817-3933", "普通聊天"),
        )
        .unwrap_err()
        .contains("不支持的片段类型"));
    }

    #[test]
    fn confirmed_conversion_requires_explicit_order_or_payment_language() {
        assert!(has_confirmed_conversion_evidence(
            "宝宝已经下单了，我给你备注。"
        ));
        assert!(has_confirmed_conversion_evidence("这边显示已付款。"));
        assert!(!has_confirmed_conversion_evidence(
            "我把链接给你，你考虑好可以拍。"
        ));
        assert!(!has_confirmed_conversion_evidence("这个价格是 9999。"));
    }

    #[test]
    fn only_actionable_upgrade_decisions_enter_the_human_candidate_queue() {
        for decision in [
            UpgradeComparisonDecision::AddAsSupport,
            UpgradeComparisonDecision::AddAsGoldenSentence,
            UpgradeComparisonDecision::ReplaceExisting,
            UpgradeComparisonDecision::MergeWithExisting,
        ] {
            assert!(upgrade_decision_enters_candidate_queue(decision));
        }
        assert!(!upgrade_decision_enters_candidate_queue(
            UpgradeComparisonDecision::Duplicate
        ));
        assert!(!upgrade_decision_enters_candidate_queue(
            UpgradeComparisonDecision::Reject
        ));
    }
}
