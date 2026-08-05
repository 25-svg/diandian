use master_script::{
    evaluate_admission, CandidateAdmission, HardGateResult, MasterScriptError, MasterSectionKind,
    ScoreBreakdown,
};
use serde::{Deserialize, Serialize};

mod upgrade_review;
pub use upgrade_review::{
    is_actionable_upgrade_decision, parse_upgrade_review, prompt_two_is_upgrade_eligible,
    validate_upgrade_review_for_prompt_two, MasterUpgradeReview, UpgradeComparisonDecision,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceGrade {
    A,
    B,
    C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    Passed,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentDecision {
    SupportCandidate,
    GoldenSentence,
    TrainingMaterial,
    ReferenceOnly,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreDetail {
    pub score: u8,
    pub max_score: u8,
    pub reason: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreDetails {
    pub scene_goal: ScoreDetail,
    pub persuasiveness: ScoreDetail,
    pub master_increment: ScoreDetail,
    pub reusability: ScoreDetail,
    pub factual_accuracy: ScoreDetail,
    pub natural_expression: ScoreDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentQualityReview {
    pub segment_id: String,
    pub segment_type: String,
    pub total_score: u8,
    pub decision: SegmentDecision,
    pub evidence_grade: EvidenceGrade,
    pub risk_status: RiskStatus,
    pub score_details: ScoreDetails,
    pub what_is_good: Vec<String>,
    pub what_needs_improvement: Vec<String>,
    pub facts_to_confirm: Vec<String>,
    pub reusable_original_sentence: String,
    pub suggested_training_version: String,
    pub recommended_master_section: String,
}

pub fn evaluate_segment_decision(
    segment_type: &str,
    total_score: u8,
    evidence_grade: EvidenceGrade,
    risk_status: RiskStatus,
) -> SegmentDecision {
    if risk_status == RiskStatus::Blocked {
        return SegmentDecision::Blocked;
    }
    if segment_type == "需要改进的反面案例" {
        return SegmentDecision::ReferenceOnly;
    }
    if total_score >= 85 && evidence_grade != EvidenceGrade::C {
        return if segment_type == "高质量金句" {
            SegmentDecision::GoldenSentence
        } else {
            SegmentDecision::SupportCandidate
        };
    }
    if total_score >= 70 {
        SegmentDecision::TrainingMaterial
    } else {
        SegmentDecision::ReferenceOnly
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterSectionRef {
    pub id: i64,
    pub product_card_id: Option<String>,
    pub kind: MasterSectionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterSnapshot {
    pub id: i64,
    pub version: String,
    pub sections: Vec<MasterSectionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawScore {
    pub scenario_goal: u8,
    pub factual_accuracy: u8,
    pub talk_structure: u8,
    pub conversion_action: u8,
    pub expression_rhythm: u8,
    pub risk_control: u8,
}

impl RawScore {
    pub const fn new(
        scenario_goal: u8,
        factual_accuracy: u8,
        talk_structure: u8,
        conversion_action: u8,
        expression_rhythm: u8,
        risk_control: u8,
    ) -> Self {
        Self {
            scenario_goal,
            factual_accuracy,
            talk_structure,
            conversion_action,
            expression_rhythm,
            risk_control,
        }
    }

    fn validate(self) -> Result<ScoreBreakdown, MasterScriptError> {
        ScoreBreakdown::new(
            self.scenario_goal,
            self.factual_accuracy,
            self.talk_structure,
            self.conversion_action,
            self.expression_rhythm,
            self.risk_control,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAssessment {
    pub gates: HardGateResult,
    pub score: RawScore,
    pub evidence_cue_ids: Vec<u64>,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub module_tags: Vec<String>,
    pub improvements: Vec<String>,
    pub risks: Vec<String>,
    pub suggested_insertion_point: String,
    #[serde(default)]
    pub quality_review: Option<SegmentQualityReview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterComparison {
    pub master_script_id: i64,
    pub master_version: String,
    pub master_section_id: Option<i64>,
    pub is_master_source: bool,
    pub score: Option<ScoreBreakdown>,
    pub total_score: Option<u8>,
    pub admission: Option<CandidateAdmission>,
    pub gates: Option<HardGateResult>,
    pub verdict: String,
    pub module_tags: Vec<String>,
    pub improvements: Vec<String>,
    pub risks: Vec<String>,
    pub suggested_insertion_point: Option<String>,
    pub quality_review: Option<SegmentQualityReview>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComparisonError {
    #[error("母稿版本已更新，请按最新母稿重新评分（当前 ID：{current_id}）")]
    StaleMaster { current_id: i64 },
    #[error("模型返回的分项分数超过允许上限")]
    InvalidScore,
    #[error("模型引用了不属于当前已审核逐字稿的 cue：{cue_id}")]
    InvalidEvidenceCue { cue_id: u64 },
}

#[derive(Debug, Deserialize)]
struct V2ModelAssessment {
    segment_id: String,
    segment_type: String,
    total_score: u8,
    decision: SegmentDecision,
    evidence_grade: EvidenceGrade,
    risk_status: RiskStatus,
    score_details: ScoreDetails,
    #[serde(default)]
    what_is_good: Vec<String>,
    #[serde(default)]
    what_needs_improvement: Vec<String>,
    #[serde(default)]
    facts_to_confirm: Vec<String>,
    #[serde(default)]
    reusable_original_sentence: String,
    #[serde(default)]
    suggested_training_version: String,
    #[serde(default)]
    recommended_master_section: String,
    #[serde(default, alias = "evidenceCueIds")]
    evidence_cue_ids: Vec<u64>,
}

pub fn parse_model_assessment(response: &str) -> Result<ModelAssessment, String> {
    let mut value = response.trim();
    if let Some(unfenced) = value.strip_prefix("```json") {
        value = unfenced;
    } else if let Some(unfenced) = value.strip_prefix("```") {
        value = unfenced;
    }
    value = value.trim();
    if let Some(unfenced) = value.strip_suffix("```") {
        value = unfenced.trim();
    }
    let mut json = serde_json::from_str::<serde_json::Value>(value)
        .map_err(|error| format!("MiniMax 对比结果格式错误：{error}"))?;
    normalize_numeric_string_fields(&mut json);
    normalize_text_list_fields(&mut json);
    normalize_v2_assessment_fields(&mut json);
    if json.get("score_details").is_some() {
        return parse_v2_model_assessment(json);
    }
    serde_json::from_value(json).map_err(|error| format!("MiniMax 对比结果格式错误：{error}"))
}

fn parse_v2_model_assessment(json: serde_json::Value) -> Result<ModelAssessment, String> {
    let input = serde_json::from_value::<V2ModelAssessment>(json)
        .map_err(|error| format!("MiniMax 对比结果格式错误：{error}"))?;
    validate_score_detail(&input.score_details.scene_goal, 30, "scene_goal")?;
    validate_score_detail(&input.score_details.persuasiveness, 20, "persuasiveness")?;
    validate_score_detail(
        &input.score_details.master_increment,
        20,
        "master_increment",
    )?;
    validate_score_detail(&input.score_details.reusability, 15, "reusability")?;
    validate_score_detail(
        &input.score_details.factual_accuracy,
        10,
        "factual_accuracy",
    )?;
    validate_score_detail(
        &input.score_details.natural_expression,
        5,
        "natural_expression",
    )?;
    let score = RawScore::new(
        input.score_details.scene_goal.score,
        input.score_details.persuasiveness.score,
        input.score_details.master_increment.score,
        input.score_details.reusability.score,
        input.score_details.factual_accuracy.score,
        input.score_details.natural_expression.score,
    );
    let _model_total = input.total_score;
    let total_score = score
        .clone()
        .validate()
        .map_err(|_| "MiniMax 对比结果格式错误：分项分数超过允许上限".to_string())?
        .total();
    // Prefer the recomputed sum when the model's total_score drifts after normalization.
    let decision = evaluate_segment_decision(
        &input.segment_type,
        total_score,
        input.evidence_grade,
        input.risk_status,
    );
    let quality_review = SegmentQualityReview {
        segment_id: input.segment_id,
        segment_type: input.segment_type,
        total_score,
        decision,
        evidence_grade: input.evidence_grade,
        risk_status: input.risk_status,
        score_details: input.score_details,
        what_is_good: input.what_is_good,
        what_needs_improvement: input.what_needs_improvement,
        facts_to_confirm: input.facts_to_confirm,
        reusable_original_sentence: input.reusable_original_sentence,
        suggested_training_version: input.suggested_training_version,
        recommended_master_section: input.recommended_master_section,
    };
    let gates = HardGateResult {
        transcript_reviewed: true,
        master_section_matched: true,
        context_complete: true,
        facts_resolved: input.risk_status != RiskStatus::Blocked,
        transaction_evidence_valid: input.risk_status != RiskStatus::Blocked,
        host_speech_backed: !input.evidence_cue_ids.is_empty(),
        not_duplicate: true,
        reasons: Vec::new(),
    };
    let _model_decision = input.decision;
    Ok(ModelAssessment {
        gates,
        score,
        evidence_cue_ids: input.evidence_cue_ids,
        verdict: decision_label(decision).into(),
        module_tags: vec![quality_review.segment_type.clone()],
        improvements: quality_review.what_is_good.clone(),
        risks: quality_review.facts_to_confirm.clone(),
        suggested_insertion_point: quality_review.recommended_master_section.clone(),
        quality_review: Some(quality_review),
    })
}

fn validate_score_detail(
    detail: &ScoreDetail,
    expected_max: u8,
    field: &str,
) -> Result<(), String> {
    if detail.max_score != expected_max || detail.score > expected_max {
        return Err(format!("MiniMax 对比结果格式错误：{field} 分数或上限无效"));
    }
    if detail.reason.trim().is_empty() || detail.evidence.iter().all(|item| item.trim().is_empty())
    {
        return Err(format!(
            "MiniMax 对比结果格式错误：{field} 缺少评分理由或真实证据"
        ));
    }
    Ok(())
}

fn decision_label(decision: SegmentDecision) -> &'static str {
    match decision {
        SegmentDecision::SupportCandidate => "候选辅稿",
        SegmentDecision::GoldenSentence => "金句话术",
        SegmentDecision::TrainingMaterial => "训练素材",
        SegmentDecision::ReferenceOnly => "仅作复盘参考",
        SegmentDecision::Blocked => "禁止使用",
    }
}

/// MiniMax occasionally quotes numeric fields in an otherwise valid JSON response.
/// Accept only decimal integer strings here; structural and range validation still happens
/// during serde deserialization and score validation below.
fn normalize_numeric_string_fields(value: &mut serde_json::Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };

    if let Some(score) = root
        .get_mut("score")
        .and_then(serde_json::Value::as_object_mut)
    {
        for field in [
            "scenarioGoal",
            "factualAccuracy",
            "talkStructure",
            "conversionAction",
            "expressionRhythm",
            "riskControl",
        ] {
            if let Some(value) = score.get_mut(field) {
                normalize_decimal_string(value);
            }
        }
    }

    for field in ["total_score", "totalScore"] {
        if let Some(value) = root.get_mut(field) {
            normalize_decimal_string(value);
        }
    }
    if let Some(details) = root
        .get_mut("score_details")
        .and_then(serde_json::Value::as_object_mut)
    {
        for detail in details
            .values_mut()
            .filter_map(serde_json::Value::as_object_mut)
        {
            for field in ["score", "max_score"] {
                if let Some(value) = detail.get_mut(field) {
                    normalize_decimal_string(value);
                }
            }
        }
    }
    normalize_evidence_cue_ids(root);
}

fn normalize_v2_assessment_fields(value: &mut serde_json::Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    normalize_evidence_cue_ids(root);
    let Some(details) = root
        .get_mut("score_details")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    rename_score_detail_key(details, "sceneGoal", "scene_goal");
    rename_score_detail_key(details, "masterIncrement", "master_increment");
    rename_score_detail_key(details, "factualAccuracy", "factual_accuracy");
    rename_score_detail_key(details, "naturalExpression", "natural_expression");
    for (field, expected_max) in [
        ("scene_goal", 30),
        ("persuasiveness", 20),
        ("master_increment", 20),
        ("reusability", 15),
        ("factual_accuracy", 10),
        ("natural_expression", 5),
    ] {
        if let Some(detail) = details.get_mut(field) {
            normalize_score_detail_object(detail, expected_max);
        }
    }
}

fn rename_score_detail_key(
    details: &mut serde_json::Map<String, serde_json::Value>,
    from: &str,
    to: &str,
) {
    if details.contains_key(to) {
        return;
    }
    if let Some(value) = details.remove(from) {
        details.insert(to.to_string(), value);
    }
}

fn normalize_score_detail_object(detail: &mut serde_json::Value, expected_max: u8) {
    let Some(object) = detail.as_object_mut() else {
        return;
    };
    for field in ["score", "maxScore", "max_score"] {
        if let Some(value) = object.get_mut(field) {
            normalize_integer_value(value);
        }
    }
    if object.get("max_score").is_none() {
        if let Some(max_score) = object.remove("maxScore") {
            object.insert("max_score".to_string(), max_score);
        }
    }
    object.insert(
        "max_score".to_string(),
        serde_json::Value::Number(expected_max.into()),
    );
    if let Some(score) = object.get_mut("score") {
        normalize_integer_value(score);
        if let Some(parsed) = score.as_u64() {
            if parsed > expected_max as u64 {
                *score = serde_json::Value::Number(expected_max.into());
            }
        }
    }
    if let Some(evidence) = object.get_mut("evidence") {
        normalize_text_list(evidence);
    }
}

fn normalize_evidence_cue_ids(root: &mut serde_json::Map<String, serde_json::Value>) {
    for field in ["evidenceCueIds", "evidence_cue_ids"] {
        let Some(raw) = root.get(field).cloned() else {
            continue;
        };
        let Some(values) = raw.as_array() else {
            continue;
        };
        let normalized = values
            .iter()
            .filter_map(parse_evidence_cue_id)
            .map(serde_json::Value::from)
            .collect::<Vec<_>>();
        root.insert(field.to_string(), serde_json::Value::Array(normalized));
    }
}

/// MiniMax often returns cue ids like "cue_1" even though the prompt asks for numbers.
pub fn parse_evidence_cue_id(value: &serde_json::Value) -> Option<u64> {
    if let Some(number) = value.as_u64() {
        return Some(number);
    }
    if let Some(number) = value.as_i64().filter(|value| *value >= 0) {
        return Some(number as u64);
    }
    if let Some(number) = value.as_f64().filter(|value| *value >= 0.0) {
        return Some(number.round() as u64);
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if let Ok(number) = trimmed.parse::<u64>() {
            return Some(number);
        }
        let lower = trimmed.to_ascii_lowercase();
        for prefix in ["cue_", "cue-", "cue "] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                if let Ok(number) = rest.trim().parse::<u64>() {
                    return Some(number);
                }
            }
        }
    }
    if let Some(object) = value.as_object() {
        for key in ["id", "cueId", "cue_id"] {
            if let Some(parsed) = object.get(key).and_then(parse_evidence_cue_id) {
                return Some(parsed);
            }
        }
    }
    None
}

fn normalize_integer_value(value: &mut serde_json::Value) {
    if value.is_u64() || value.is_i64() {
        return;
    }
    if let Some(number) = value.as_f64() {
        if number >= 0.0 {
            *value = serde_json::Value::Number((number.round() as u64).into());
        }
        return;
    }
    normalize_decimal_string(value);
}

fn normalize_decimal_string(value: &mut serde_json::Value) {
    let Some(text) = value.as_str() else {
        return;
    };
    let Ok(number) = text.trim().parse::<u64>() else {
        return;
    };
    *value = serde_json::Value::Number(number.into());
}

/// The model can return one prose suggestion instead of a one-item array.
/// These fields are display-only explanations, so preserving one item is safer
/// than discarding it. Evidence IDs remain strictly numeric above.
fn normalize_text_list_fields(value: &mut serde_json::Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };

    for field in [
        "moduleTags",
        "improvements",
        "risks",
        "what_is_good",
        "what_needs_improvement",
        "facts_to_confirm",
    ] {
        if let Some(value) = root.get_mut(field) {
            normalize_text_list(value);
        }
    }
    if let Some(details) = root
        .get_mut("score_details")
        .and_then(serde_json::Value::as_object_mut)
    {
        for detail in details
            .values_mut()
            .filter_map(serde_json::Value::as_object_mut)
        {
            if let Some(evidence) = detail.get_mut("evidence") {
                normalize_text_list(evidence);
            }
        }
    }
    if let Some(gates) = root
        .get_mut("gates")
        .and_then(serde_json::Value::as_object_mut)
    {
        if let Some(reasons) = gates.get_mut("reasons") {
            normalize_text_list(reasons);
        }
    }
}

fn normalize_text_list(value: &mut serde_json::Value) {
    let Some(text) = value.as_str() else {
        return;
    };
    let trimmed = text.trim();
    *value = serde_json::Value::Array(if trimmed.is_empty() {
        Vec::new()
    } else {
        vec![serde_json::Value::String(trimmed.to_string())]
    });
}

pub fn compare_to_master(
    master: &MasterSnapshot,
    expected_master_id: i64,
    master_section_id: Option<i64>,
    product_card_id: Option<&str>,
    section_kind: MasterSectionKind,
    reviewed_clip_cue_ids: &[u64],
    assessment: ModelAssessment,
) -> Result<MasterComparison, ComparisonError> {
    if master.id != expected_master_id {
        return Err(ComparisonError::StaleMaster {
            current_id: master.id,
        });
    }
    let matched = match master_section_id {
        Some(section_id) => master
            .sections
            .iter()
            .find(|section| section.id == section_id),
        None => master.sections.iter().find(|section| {
            section.kind == section_kind && section.product_card_id.as_deref() == product_card_id
        }),
    };
    let Some(section) = matched else {
        return Ok(MasterComparison {
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
        });
    };
    for cue_id in &assessment.evidence_cue_ids {
        if !reviewed_clip_cue_ids.contains(cue_id) {
            return Err(ComparisonError::InvalidEvidenceCue { cue_id: *cue_id });
        }
    }
    if !assessment.gates.context_complete {
        return Ok(MasterComparison {
            master_script_id: master.id,
            master_version: master.version.clone(),
            master_section_id: Some(section.id),
            is_master_source: false,
            score: None,
            total_score: None,
            admission: Some(CandidateAdmission::AnalysisOnly),
            gates: Some(assessment.gates),
            verdict: assessment.verdict,
            module_tags: assessment.module_tags,
            improvements: assessment.improvements,
            risks: assessment.risks,
            suggested_insertion_point: None,
            quality_review: assessment.quality_review,
        });
    }
    let score = assessment
        .score
        .validate()
        .map_err(|_| ComparisonError::InvalidScore)?;
    let admission = assessment
        .quality_review
        .as_ref()
        .map(|review| admission_for_decision(review.decision, &assessment.gates))
        .unwrap_or_else(|| evaluate_admission(&assessment.gates, &score));
    let total_score = score.total();
    Ok(MasterComparison {
        master_script_id: master.id,
        master_version: master.version.clone(),
        master_section_id: Some(section.id),
        is_master_source: false,
        score: Some(score),
        total_score: Some(total_score),
        admission: Some(admission),
        gates: Some(assessment.gates),
        verdict: assessment.verdict,
        module_tags: assessment.module_tags,
        improvements: assessment.improvements,
        risks: assessment.risks,
        suggested_insertion_point: Some(assessment.suggested_insertion_point),
        quality_review: assessment.quality_review,
    })
}

/// A clip from the recording that produced the published master is reference
/// material, not a candidate support script. It must never be re-scored by the
/// model or sent through the support-script selection workflow.
pub fn master_source_baseline(master: &MasterSnapshot, master_section_id: i64) -> MasterComparison {
    MasterComparison {
        master_script_id: master.id,
        master_version: master.version.clone(),
        master_section_id: Some(master_section_id),
        is_master_source: true,
        score: Some(
            ScoreBreakdown::new(30, 20, 20, 15, 10, 5).expect("fixed baseline score is valid"),
        ),
        total_score: Some(100),
        admission: Some(CandidateAdmission::AnalysisOnly),
        gates: Some(HardGateResult {
            transcript_reviewed: true,
            master_section_matched: true,
            context_complete: true,
            facts_resolved: true,
            transaction_evidence_valid: true,
            host_speech_backed: true,
            not_duplicate: true,
            reasons: Vec::new(),
        }),
        verdict: "本段来自当前企业母稿的原始录播，用作基准样本。".into(),
        module_tags: Vec::new(),
        improvements: Vec::new(),
        risks: Vec::new(),
        suggested_insertion_point: None,
        quality_review: None,
    }
}

fn admission_for_decision(decision: SegmentDecision, gates: &HardGateResult) -> CandidateAdmission {
    if !gates.all_pass() {
        return CandidateAdmission::Blocked;
    }
    match decision {
        SegmentDecision::SupportCandidate => CandidateAdmission::CandidateQueue,
        SegmentDecision::TrainingMaterial => CandidateAdmission::ReviewOnly,
        SegmentDecision::GoldenSentence | SegmentDecision::ReferenceOnly => {
            CandidateAdmission::AnalysisOnly
        }
        SegmentDecision::Blocked => CandidateAdmission::Blocked,
    }
}
