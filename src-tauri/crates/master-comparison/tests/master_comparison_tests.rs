use master_comparison::{
    compare_to_master, parse_model_assessment, ComparisonError, MasterSectionRef, MasterSnapshot,
    ModelAssessment, RawScore,
};
use master_script::{CandidateAdmission, HardGateResult, MasterSectionKind};

fn master() -> MasterSnapshot {
    MasterSnapshot {
        id: 10,
        version: "1.0.0".into(),
        sections: vec![MasterSectionRef {
            id: 20,
            product_card_id: Some("CANON-R50".into()),
            kind: MasterSectionKind::Product,
        }],
    }
}

fn passing_gates() -> HardGateResult {
    HardGateResult {
        transcript_reviewed: true,
        master_section_matched: true,
        context_complete: true,
        facts_resolved: true,
        transaction_evidence_valid: true,
        host_speech_backed: true,
        not_duplicate: true,
        reasons: vec![],
    }
}

fn assessment(score: RawScore) -> ModelAssessment {
    ModelAssessment {
        gates: passing_gates(),
        score,
        evidence_cue_ids: vec![7, 8],
        improvements: vec!["更清楚地处理了顾客异议".into()],
        risks: vec![],
        suggested_insertion_point: "产品异议处理后".into(),
    }
}

#[test]
fn exact_product_and_section_match_scores_against_the_expected_master() {
    let result = compare_to_master(
        &master(),
        10,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7, 8],
        assessment(RawScore::new(20, 20, 18, 13, 9, 5)),
    )
    .unwrap();

    assert_eq!(result.master_section_id, Some(20));
    assert_eq!(result.total_score, Some(85));
    assert_eq!(result.admission, Some(CandidateAdmission::CandidateQueue));
}

#[test]
fn no_matching_section_returns_without_a_score() {
    let result = compare_to_master(
        &master(),
        10,
        Some("OTHER-PRODUCT"),
        MasterSectionKind::Product,
        &[7, 8],
        assessment(RawScore::new(25, 25, 20, 15, 10, 5)),
    )
    .unwrap();

    assert_eq!(result.master_section_id, None);
    assert_eq!(result.total_score, None);
    assert_eq!(result.admission, None);
}

#[test]
fn stale_master_version_is_rejected_before_scoring() {
    let error = compare_to_master(
        &master(),
        9,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7, 8],
        assessment(RawScore::new(25, 25, 20, 15, 10, 5)),
    )
    .unwrap_err();

    assert_eq!(error, ComparisonError::StaleMaster { current_id: 10 });
}

#[test]
fn server_rejects_a_score_dimension_over_its_cap() {
    let error = compare_to_master(
        &master(),
        10,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7, 8],
        assessment(RawScore::new(26, 25, 20, 15, 10, 5)),
    )
    .unwrap_err();

    assert_eq!(error, ComparisonError::InvalidScore);
}

#[test]
fn a_failed_hard_gate_blocks_even_one_hundred_model_points() {
    let mut model = assessment(RawScore::new(25, 25, 20, 15, 10, 5));
    model.gates.facts_resolved = false;
    model.gates.reasons.push("价格仍待确认".into());

    let result = compare_to_master(
        &master(),
        10,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7, 8],
        model,
    )
    .unwrap();

    assert_eq!(result.total_score, Some(100));
    assert_eq!(result.admission, Some(CandidateAdmission::Blocked));
}

#[test]
fn model_evidence_must_cite_the_reviewed_clip_transcript() {
    let error = compare_to_master(
        &master(),
        10,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7],
        assessment(RawScore::new(20, 20, 18, 13, 9, 5)),
    )
    .unwrap_err();

    assert_eq!(error, ComparisonError::InvalidEvidenceCue { cue_id: 8 });
}

#[test]
fn parses_a_fenced_model_assessment_without_trusting_a_model_total() {
    let payload = r#"{"gates":{"transcriptReviewed":true,"masterSectionMatched":true,"contextComplete":true,"factsResolved":true,"transactionEvidenceValid":true,"hostSpeechBacked":true,"notDuplicate":true,"reasons":[]},"score":{"transactionEvidence":20,"improvementOverMaster":20,"reusability":18,"completeness":13,"factualAccuracy":9,"scenarioClarity":5},"evidenceCueIds":[7],"improvements":[],"risks":[],"suggestedInsertionPoint":"异议处理后"}"#;

    let parsed = parse_model_assessment(&format!("```json\n{payload}\n```"))
        .expect("fenced model JSON should parse");

    assert_eq!(parsed.evidence_cue_ids, vec![7]);
    assert_eq!(parsed.score.transaction_evidence, 20);
}
