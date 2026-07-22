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
fn admits_only_scores_above_85_after_all_gates_pass() {
    let score_69 = ScoreBreakdown::new(20, 20, 15, 10, 4, 0).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_69),
        CandidateAdmission::AnalysisOnly
    );

    let score_70 = ScoreBreakdown::new(20, 20, 15, 10, 5, 0).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_70),
        CandidateAdmission::ReviewOnly
    );

    let score_85 = ScoreBreakdown::new(20, 20, 18, 13, 9, 5).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_85),
        CandidateAdmission::ReviewOnly
    );

    let score_86 = ScoreBreakdown::new(21, 20, 18, 13, 9, 5).unwrap();
    assert_eq!(
        evaluate_admission(&passing_gates(), &score_86),
        CandidateAdmission::CandidateQueue
    );
}

#[test]
fn a_failed_gate_blocks_a_perfect_score() {
    let mut gates = passing_gates();
    gates.facts_resolved = false;
    gates.reasons.push("价格仍待确认".into());
    let score = ScoreBreakdown::new(25, 25, 20, 15, 10, 5).unwrap();
    assert_eq!(
        evaluate_admission(&gates, &score),
        CandidateAdmission::Blocked
    );
}

#[test]
fn gate_reasons_do_not_change_a_passing_gate_result() {
    let mut gates = passing_gates();
    gates.reasons.push("历史提示".into());
    let score = ScoreBreakdown::new(25, 25, 20, 15, 10, 5).unwrap();
    assert_eq!(
        evaluate_admission(&gates, &score),
        CandidateAdmission::CandidateQueue
    );
}

#[test]
fn validates_dimension_caps() {
    assert!(ScoreBreakdown::new(26, 25, 20, 15, 10, 5).is_err());
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
