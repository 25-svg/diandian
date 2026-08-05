use serde::{Deserialize, Serialize};

use crate::{EvidenceGrade, RiskStatus, SegmentDecision, SegmentQualityReview};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeComparisonDecision {
    AddAsSupport,
    AddAsGoldenSentence,
    ReplaceExisting,
    MergeWithExisting,
    Duplicate,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterUpgradeReview {
    pub comparison_decision: UpgradeComparisonDecision,
    pub matched_master_section: String,
    pub same_scene: bool,
    pub new_value: String,
    pub why_better: Vec<String>,
    pub duplicate_content: Vec<String>,
    pub risk_or_uncertainty: Vec<String>,
    pub original_host_words: String,
    pub training_suggestion: String,
    pub recommended_action: String,
}

#[derive(Debug, Deserialize)]
struct RawMasterUpgradeReview {
    comparison_decision: UpgradeComparisonDecision,
    matched_master_section: String,
    same_scene: bool,
    new_value: String,
    why_better: Vec<String>,
    duplicate_content: Vec<String>,
    risk_or_uncertainty: Vec<String>,
    original_host_words: String,
    training_suggestion: String,
    recommended_action: String,
}

pub fn parse_upgrade_review(
    raw: &str,
    expected_section: &str,
    candidate_original_text: &str,
) -> Result<MasterUpgradeReview, String> {
    let json_text = strip_code_fence(raw);
    let mut value = serde_json::from_str::<serde_json::Value>(&json_text)
        .map_err(|error| format!("MiniMax 母稿比较格式错误：{error}"))?;
    normalize_list_fields(&mut value);
    let parsed = serde_json::from_value::<RawMasterUpgradeReview>(value)
        .map_err(|error| format!("MiniMax 母稿比较格式错误：{error}"))?;

    let matched_master_section = parsed.matched_master_section.trim().to_string();
    if matched_master_section != expected_section.trim() {
        return Err("MiniMax 母稿比较章节与当前企业母稿章节不一致".into());
    }
    let original_host_words = parsed.original_host_words.trim().to_string();
    if original_host_words.is_empty() || !candidate_original_text.contains(&original_host_words) {
        return Err("MiniMax 母稿比较引用了候选片段中不存在的主播原话".into());
    }
    let new_value = required_text(parsed.new_value, "new_value")?;
    let training_suggestion = required_text(parsed.training_suggestion, "training_suggestion")?;
    if !training_suggestion.starts_with("[建议稿]") {
        return Err("MiniMax 母稿比较格式错误：training_suggestion 必须以 [建议稿] 开头".into());
    }
    let recommended_action = required_text(parsed.recommended_action, "recommended_action")?;

    Ok(MasterUpgradeReview {
        comparison_decision: parsed.comparison_decision,
        matched_master_section,
        same_scene: parsed.same_scene,
        new_value,
        why_better: clean_list(parsed.why_better),
        duplicate_content: clean_list(parsed.duplicate_content),
        risk_or_uncertainty: clean_list(parsed.risk_or_uncertainty),
        original_host_words,
        training_suggestion,
        recommended_action,
    })
}

pub fn prompt_two_is_upgrade_eligible(review: &SegmentQualityReview) -> bool {
    matches!(
        review.decision,
        SegmentDecision::SupportCandidate | SegmentDecision::GoldenSentence
    ) && review.evidence_grade != EvidenceGrade::C
        && review.risk_status != RiskStatus::Blocked
}

pub fn validate_upgrade_review_for_prompt_two(
    review: &MasterUpgradeReview,
    prompt_two: &SegmentQualityReview,
) -> Result<(), String> {
    match (review.comparison_decision, prompt_two.decision) {
        (UpgradeComparisonDecision::AddAsSupport, SegmentDecision::SupportCandidate)
        | (UpgradeComparisonDecision::AddAsGoldenSentence, SegmentDecision::GoldenSentence)
        | (
            UpgradeComparisonDecision::ReplaceExisting
            | UpgradeComparisonDecision::MergeWithExisting
            | UpgradeComparisonDecision::Duplicate
            | UpgradeComparisonDecision::Reject,
            _,
        ) => Ok(()),
        (UpgradeComparisonDecision::AddAsSupport, _) => {
            Err("母稿比较不能把金句话术改判为完整候选辅稿".into())
        }
        (UpgradeComparisonDecision::AddAsGoldenSentence, _) => {
            Err("母稿比较不能把完整候选辅稿改判为金句话术".into())
        }
    }
}

pub const fn is_actionable_upgrade_decision(decision: UpgradeComparisonDecision) -> bool {
    matches!(
        decision,
        UpgradeComparisonDecision::AddAsSupport
            | UpgradeComparisonDecision::AddAsGoldenSentence
            | UpgradeComparisonDecision::ReplaceExisting
            | UpgradeComparisonDecision::MergeWithExisting
    )
}

fn strip_code_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_open
        .strip_suffix("```")
        .unwrap_or(without_open)
        .trim()
        .to_string()
}

fn normalize_list_fields(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for field in ["why_better", "duplicate_content", "risk_or_uncertainty"] {
        let Some(current) = object.get_mut(field) else {
            continue;
        };
        if current.is_string() {
            *current = serde_json::Value::Array(vec![current.clone()]);
        }
    }
}

fn required_text(value: String, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("MiniMax 母稿比较格式错误：{field} 不能为空"));
    }
    Ok(value)
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}
