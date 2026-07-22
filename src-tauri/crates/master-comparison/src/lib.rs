use master_script::{
    evaluate_admission, CandidateAdmission, HardGateResult, MasterScriptError, MasterSectionKind,
    ScoreBreakdown,
};
use serde::{Deserialize, Serialize};

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
    pub transaction_evidence: u8,
    pub improvement_over_master: u8,
    pub reusability: u8,
    pub completeness: u8,
    pub factual_accuracy: u8,
    pub scenario_clarity: u8,
}

impl RawScore {
    pub const fn new(
        transaction_evidence: u8,
        improvement_over_master: u8,
        reusability: u8,
        completeness: u8,
        factual_accuracy: u8,
        scenario_clarity: u8,
    ) -> Self {
        Self {
            transaction_evidence,
            improvement_over_master,
            reusability,
            completeness,
            factual_accuracy,
            scenario_clarity,
        }
    }

    fn validate(self) -> Result<ScoreBreakdown, MasterScriptError> {
        ScoreBreakdown::new(
            self.transaction_evidence,
            self.improvement_over_master,
            self.reusability,
            self.completeness,
            self.factual_accuracy,
            self.scenario_clarity,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAssessment {
    pub gates: HardGateResult,
    pub score: RawScore,
    pub evidence_cue_ids: Vec<u64>,
    pub improvements: Vec<String>,
    pub risks: Vec<String>,
    pub suggested_insertion_point: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterComparison {
    pub master_script_id: i64,
    pub master_version: String,
    pub master_section_id: Option<i64>,
    pub score: Option<ScoreBreakdown>,
    pub total_score: Option<u8>,
    pub admission: Option<CandidateAdmission>,
    pub gates: Option<HardGateResult>,
    pub improvements: Vec<String>,
    pub risks: Vec<String>,
    pub suggested_insertion_point: Option<String>,
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
    serde_json::from_str(value).map_err(|error| format!("MiniMax 对比结果格式错误：{error}"))
}

pub fn compare_to_master(
    master: &MasterSnapshot,
    expected_master_id: i64,
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
    let matched = master.sections.iter().find(|section| {
        section.kind == section_kind && section.product_card_id.as_deref() == product_card_id
    });
    let Some(section) = matched else {
        return Ok(MasterComparison {
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
        });
    };
    for cue_id in &assessment.evidence_cue_ids {
        if !reviewed_clip_cue_ids.contains(cue_id) {
            return Err(ComparisonError::InvalidEvidenceCue { cue_id: *cue_id });
        }
    }
    let score = assessment
        .score
        .validate()
        .map_err(|_| ComparisonError::InvalidScore)?;
    let admission = evaluate_admission(&assessment.gates, &score);
    let total_score = score.total();
    Ok(MasterComparison {
        master_script_id: master.id,
        master_version: master.version.clone(),
        master_section_id: Some(section.id),
        score: Some(score),
        total_score: Some(total_score),
        admission: Some(admission),
        gates: Some(assessment.gates),
        improvements: assessment.improvements,
        risks: assessment.risks,
        suggested_insertion_point: Some(assessment.suggested_insertion_point),
    })
}
