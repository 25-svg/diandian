use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MasterScriptError {
    #[error("score exceeds one or more dimension caps")]
    InvalidScore,
    #[error("invalid version: {0}")]
    InvalidVersion(String),
    #[error("version patch overflow: {0}")]
    VersionOverflow(String),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBreakdown {
    transaction_evidence: u8,
    improvement_over_master: u8,
    reusability: u8,
    completeness: u8,
    factual_accuracy: u8,
    scenario_clarity: u8,
    total: u8,
}

impl ScoreBreakdown {
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Result<Self, MasterScriptError> {
        if a > 25 || b > 25 || c > 20 || d > 15 || e > 10 || f > 5 {
            return Err(MasterScriptError::InvalidScore);
        }

        Ok(Self {
            transaction_evidence: a,
            improvement_over_master: b,
            reusability: c,
            completeness: d,
            factual_accuracy: e,
            scenario_clarity: f,
            total: a + b + c + d + e + f,
        })
    }

    pub const fn transaction_evidence(&self) -> u8 {
        self.transaction_evidence
    }

    pub const fn improvement_over_master(&self) -> u8 {
        self.improvement_over_master
    }

    pub const fn reusability(&self) -> u8 {
        self.reusability
    }

    pub const fn completeness(&self) -> u8 {
        self.completeness
    }

    pub const fn factual_accuracy(&self) -> u8 {
        self.factual_accuracy
    }

    pub const fn scenario_clarity(&self) -> u8 {
        self.scenario_clarity
    }

    pub fn total(&self) -> u8 {
        self.total
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoreBreakdownInput {
    transaction_evidence: u8,
    improvement_over_master: u8,
    reusability: u8,
    completeness: u8,
    factual_accuracy: u8,
    scenario_clarity: u8,
}

impl<'de> Deserialize<'de> for ScoreBreakdown {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = ScoreBreakdownInput::deserialize(deserializer)?;
        Self::new(
            input.transaction_evidence,
            input.improvement_over_master,
            input.reusability,
            input.completeness,
            input.factual_accuracy,
            input.scenario_clarity,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardGateResult {
    pub transcript_reviewed: bool,
    pub master_section_matched: bool,
    pub context_complete: bool,
    pub facts_resolved: bool,
    pub transaction_evidence_valid: bool,
    pub host_speech_backed: bool,
    pub not_duplicate: bool,
    pub reasons: Vec<String>,
}

impl HardGateResult {
    pub fn all_pass(&self) -> bool {
        self.transcript_reviewed
            && self.master_section_matched
            && self.context_complete
            && self.facts_resolved
            && self.transaction_evidence_valid
            && self.host_speech_backed
            && self.not_duplicate
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAdmission {
    Blocked,
    AnalysisOnly,
    ReviewOnly,
    CandidateQueue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MasterSectionKind {
    Opening,
    Product,
    Transition,
    Scenario,
    Closing,
}

impl MasterSectionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Product => "product",
            Self::Transition => "transition",
            Self::Scenario => "scenario",
            Self::Closing => "closing",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportCandidateStatus {
    PendingReview,
    Approved,
    Held,
    Returned,
    Rejected,
    Merged,
}

impl SupportCandidateStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PendingReview => "pending_review",
            Self::Approved => "approved",
            Self::Held => "held",
            Self::Returned => "returned",
            Self::Rejected => "rejected",
            Self::Merged => "merged",
        }
    }
}

pub fn evaluate_admission(gates: &HardGateResult, score: &ScoreBreakdown) -> CandidateAdmission {
    if !gates.all_pass() {
        CandidateAdmission::Blocked
    } else if score.total() > 85 {
        CandidateAdmission::CandidateQueue
    } else if score.total() >= 70 {
        CandidateAdmission::ReviewOnly
    } else {
        CandidateAdmission::AnalysisOnly
    }
}

pub fn next_patch_version(current: &str) -> Result<String, MasterScriptError> {
    let components = current.split('.').collect::<Vec<_>>();
    if components.len() != 3
        || components.iter().any(|component| {
            component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(MasterScriptError::InvalidVersion(current.to_string()));
    }

    let major = components[0]
        .parse::<u64>()
        .map_err(|_| MasterScriptError::InvalidVersion(current.to_string()))?;
    let minor = components[1]
        .parse::<u64>()
        .map_err(|_| MasterScriptError::InvalidVersion(current.to_string()))?;
    let patch = components[2]
        .parse::<u64>()
        .map_err(|_| MasterScriptError::InvalidVersion(current.to_string()))?;
    let next_patch = patch
        .checked_add(1)
        .ok_or_else(|| MasterScriptError::VersionOverflow(current.to_string()))?;

    Ok(format!("{major}.{minor}.{next_patch}"))
}
