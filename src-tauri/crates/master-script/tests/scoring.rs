use master_script::{
    evaluate_admission, next_patch_version, CandidateAdmission, HardGateResult, MasterSectionKind,
    ScoreBreakdown, SupportCandidateStatus,
};

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

#[test]
fn admits_scores_at_85_after_all_gates_pass() {
    let score_69 = ScoreBreakdown::new(20, 14, 14, 10, 7, 4).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_69),
        CandidateAdmission::AnalysisOnly
    );

    let score_70 = ScoreBreakdown::new(20, 14, 14, 10, 8, 4).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_70),
        CandidateAdmission::ReviewOnly
    );

    let score_84 = ScoreBreakdown::new(25, 17, 17, 12, 8, 5).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_84),
        CandidateAdmission::ReviewOnly
    );

    let score_85 = ScoreBreakdown::new(26, 17, 17, 12, 8, 5).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_85),
        CandidateAdmission::CandidateQueue
    );
}

#[test]
fn a_failed_gate_blocks_a_perfect_score() {
    let mut gates = passing_gates();
    gates.facts_resolved = false;
    gates.reasons.push("价格仍待确认".into());
    let score = ScoreBreakdown::new(30, 20, 20, 15, 10, 5).unwrap();
    assert_eq!(
        evaluate_admission(&gates, &score),
        CandidateAdmission::Blocked
    );
}

#[test]
fn pending_transcript_review_blocks_queue_admission_until_confirmed() {
    let mut gates = passing_gates();
    gates.transcript_reviewed = false;
    gates.reasons.push("有少量逐字稿待确认".into());
    let score = ScoreBreakdown::new(26, 17, 17, 12, 8, 5).unwrap();

    assert_eq!(
        evaluate_admission(&gates, &score),
        CandidateAdmission::Blocked
    );
}

#[test]
fn gate_reasons_do_not_change_a_passing_gate_result() {
    let mut gates = passing_gates();
    gates.reasons.push("历史提示".into());
    let score = ScoreBreakdown::new(30, 20, 20, 15, 10, 5).unwrap();
    assert_eq!(
        evaluate_admission(&gates, &score),
        CandidateAdmission::CandidateQueue
    );
}

#[test]
fn validates_dimension_caps() {
    assert!(ScoreBreakdown::new(31, 20, 20, 15, 10, 5).is_err());
    assert!(ScoreBreakdown::new(30, 21, 20, 15, 10, 5).is_err());
    assert!(ScoreBreakdown::new(30, 20, 21, 15, 10, 5).is_err());
    assert!(ScoreBreakdown::new(30, 20, 20, 16, 10, 5).is_err());
    assert!(ScoreBreakdown::new(30, 20, 20, 15, 11, 5).is_err());
    assert!(ScoreBreakdown::new(30, 20, 20, 15, 10, 6).is_err());
}

#[test]
fn tampered_serialized_total_cannot_admit_a_low_score() {
    let tampered = r#"{
        "sceneGoal": 0,
        "persuasiveness": 0,
        "masterIncrement": 0,
        "reusability": 0,
        "factualAccuracy": 0,
        "naturalExpression": 0,
        "total": 86
    }"#;
    let score: ScoreBreakdown = serde_json::from_str(tampered).unwrap();

    assert_eq!(score.total(), 0);
    assert_ne!(
        evaluate_admission(&passing_gates(), &score),
        CandidateAdmission::CandidateQueue
    );
    let serialized = serde_json::to_value(&score).unwrap();
    assert_eq!(
        serialized.get("total").and_then(serde_json::Value::as_u64),
        Some(0)
    );
}

#[test]
fn score_round_trip_exposes_read_only_dimensions_and_derived_total() {
    let score = ScoreBreakdown::new(30, 19, 19, 14, 9, 4).unwrap();
    let json = serde_json::to_value(&score).unwrap();
    let round_tripped: ScoreBreakdown = serde_json::from_value(json.clone()).unwrap();

    assert_eq!(round_tripped.scene_goal(), 30);
    assert_eq!(round_tripped.persuasiveness(), 19);
    assert_eq!(round_tripped.master_increment(), 19);
    assert_eq!(round_tripped.reusability(), 14);
    assert_eq!(round_tripped.factual_accuracy(), 9);
    assert_eq!(round_tripped.natural_expression(), 4);
    assert_eq!(round_tripped.total(), 95);
    assert_eq!(
        json,
        serde_json::json!({
            "sceneGoal": 30,
            "persuasiveness": 19,
            "masterIncrement": 19,
            "reusability": 14,
            "factualAccuracy": 9,
            "naturalExpression": 4,
            "total": 95
        })
    );
}

#[test]
fn serializes_master_section_kinds_and_support_statuses_as_snake_case() {
    let sections = [
        (MasterSectionKind::Opening, "opening"),
        (MasterSectionKind::Product, "product"),
        (MasterSectionKind::Transition, "transition"),
        (MasterSectionKind::Scenario, "scenario"),
        (MasterSectionKind::Closing, "closing"),
    ];
    for (section, value) in sections {
        assert_eq!(section.as_str(), value);
        assert_eq!(
            serde_json::to_string(&section).unwrap(),
            format!("\"{value}\"")
        );
        assert_eq!(
            serde_json::from_str::<MasterSectionKind>(&format!("\"{value}\"")).unwrap(),
            section
        );
    }

    let statuses = [
        (SupportCandidateStatus::PendingReview, "pending_review"),
        (SupportCandidateStatus::Approved, "approved"),
        (SupportCandidateStatus::Held, "held"),
        (SupportCandidateStatus::Returned, "returned"),
        (SupportCandidateStatus::Rejected, "rejected"),
        (SupportCandidateStatus::Merged, "merged"),
    ];
    for (status, value) in statuses {
        assert_eq!(status.as_str(), value);
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{value}\"")
        );
        assert_eq!(
            serde_json::from_str::<SupportCandidateStatus>(&format!("\"{value}\"")).unwrap(),
            status
        );
    }
}

#[test]
fn rejects_non_strict_patch_versions() {
    for input in [
        "",
        "1",
        "1.0",
        "1.0.0.0",
        "v1.0.0",
        "+1.0.0",
        "-1.0.0",
        " 1.0.0",
        "1.0.0 ",
        "1.0.0-alpha",
        "1.0.0+build",
        "1..0",
        "1.a.0",
    ] {
        assert!(matches!(
            next_patch_version(input),
            Err(master_script::MasterScriptError::InvalidVersion(value)) if value == input
        ));
    }
}

#[test]
fn increments_patch_and_reports_patch_overflow() {
    assert_eq!(next_patch_version("1.0.9").unwrap(), "1.0.10");
    assert!(matches!(
        next_patch_version("1.2.18446744073709551615"),
        Err(master_script::MasterScriptError::VersionOverflow(value)) if value == "1.2.18446744073709551615"
    ));
}
