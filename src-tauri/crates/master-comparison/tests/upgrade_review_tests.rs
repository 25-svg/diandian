use master_comparison::{
    is_actionable_upgrade_decision, parse_upgrade_review, prompt_two_is_upgrade_eligible,
    validate_upgrade_review_for_prompt_two, EvidenceGrade, MasterUpgradeReview, RiskStatus,
    ScoreDetail, ScoreDetails, SegmentDecision, SegmentQualityReview, UpgradeComparisonDecision,
};

fn prompt_two_review(
    decision: SegmentDecision,
    evidence_grade: EvidenceGrade,
    risk_status: RiskStatus,
) -> SegmentQualityReview {
    let detail = |score, max_score| ScoreDetail {
        score,
        max_score,
        reason: "有真实证据".into(),
        evidence: vec!["00:00:10 原话".into()],
    };
    SegmentQualityReview {
        segment_id: "SEG-001".into(),
        segment_type: "异议处理".into(),
        total_score: 88,
        decision,
        evidence_grade,
        risk_status,
        score_details: ScoreDetails {
            scene_goal: detail(27, 30),
            persuasiveness: detail(18, 20),
            master_increment: detail(18, 20),
            reusability: detail(13, 15),
            factual_accuracy: detail(8, 10),
            natural_expression: detail(4, 5),
        },
        what_is_good: vec!["先回应顾虑".into()],
        what_needs_improvement: vec![],
        facts_to_confirm: vec![],
        reusable_original_sentence: "我们每台机器都经过检测。".into(),
        suggested_training_version: "[建议稿] 先说明检测，再邀请客户确认。".into(),
        recommended_master_section: "异议处理/质量担忧".into(),
    }
}

#[test]
fn parses_all_six_upgrade_decisions_and_normalizes_single_lists() {
    for (raw_decision, expected) in [
        ("add_as_support", UpgradeComparisonDecision::AddAsSupport),
        (
            "add_as_golden_sentence",
            UpgradeComparisonDecision::AddAsGoldenSentence,
        ),
        (
            "replace_existing",
            UpgradeComparisonDecision::ReplaceExisting,
        ),
        (
            "merge_with_existing",
            UpgradeComparisonDecision::MergeWithExisting,
        ),
        ("duplicate", UpgradeComparisonDecision::Duplicate),
        ("reject", UpgradeComparisonDecision::Reject),
    ] {
        let raw = format!(
            r#"```json
{{
  "comparison_decision": "{raw_decision}",
  "matched_master_section": "异议处理/质量担忧",
  "same_scene": true,
  "new_value": "新增了可复用的检测说明",
  "why_better": "表达更具体",
  "duplicate_content": [],
  "risk_or_uncertainty": "检测范围仍需确认",
  "original_host_words": "我们每台机器都经过检测。",
  "training_suggestion": "[建议稿] 先说明检测，再邀请客户确认。",
  "recommended_action": "提交到异议处理章节人工审核"
}}
```"#
        );
        let review = parse_upgrade_review(
            &raw,
            "异议处理/质量担忧",
            "客户担心质量。我们每台机器都经过检测。",
        )
        .unwrap();
        assert_eq!(review.comparison_decision, expected);
        assert_eq!(review.why_better, vec!["表达更具体"]);
        assert_eq!(review.risk_or_uncertainty, vec!["检测范围仍需确认"]);
    }
}

#[test]
fn rejects_unknown_decision_mismatched_section_and_invented_host_words() {
    let valid = r#"{
      "comparison_decision": "add_as_support",
      "matched_master_section": "异议处理/质量担忧",
      "same_scene": true,
      "new_value": "新增检测说明",
      "why_better": ["更具体"],
      "duplicate_content": [],
      "risk_or_uncertainty": [],
      "original_host_words": "我们每台机器都经过检测。",
      "training_suggestion": "[建议稿] 补充检测范围。",
      "recommended_action": "人工审核后新增辅稿"
    }"#;
    assert!(parse_upgrade_review(
        &valid.replace("add_as_support", "auto_publish"),
        "异议处理/质量担忧",
        "我们每台机器都经过检测。"
    )
    .is_err());
    assert!(parse_upgrade_review(
        &valid.replace("异议处理/质量担忧", "价格承接/优惠"),
        "异议处理/质量担忧",
        "我们每台机器都经过检测。"
    )
    .is_err());
    assert!(parse_upgrade_review(
        &valid.replace("我们每台机器都经过检测。", "我们承诺终身保修。"),
        "异议处理/质量担忧",
        "我们每台机器都经过检测。"
    )
    .is_err());
    assert!(parse_upgrade_review(
        &valid.replace("[建议稿] 补充检测范围。", "补充检测范围。"),
        "异议处理/质量担忧",
        "我们每台机器都经过检测。"
    )
    .is_err());
}

#[test]
fn only_qualified_prompt_two_results_enter_upgrade_review() {
    assert!(prompt_two_is_upgrade_eligible(&prompt_two_review(
        SegmentDecision::SupportCandidate,
        EvidenceGrade::A,
        RiskStatus::Passed,
    )));
    assert!(prompt_two_is_upgrade_eligible(&prompt_two_review(
        SegmentDecision::GoldenSentence,
        EvidenceGrade::B,
        RiskStatus::NeedsReview,
    )));
    assert!(!prompt_two_is_upgrade_eligible(&prompt_two_review(
        SegmentDecision::TrainingMaterial,
        EvidenceGrade::A,
        RiskStatus::Passed,
    )));
    assert!(!prompt_two_is_upgrade_eligible(&prompt_two_review(
        SegmentDecision::SupportCandidate,
        EvidenceGrade::C,
        RiskStatus::Passed,
    )));
    assert!(!prompt_two_is_upgrade_eligible(&prompt_two_review(
        SegmentDecision::SupportCandidate,
        EvidenceGrade::A,
        RiskStatus::Blocked,
    )));
}

#[test]
fn duplicate_and_reject_are_audit_only() {
    for decision in [
        UpgradeComparisonDecision::AddAsSupport,
        UpgradeComparisonDecision::AddAsGoldenSentence,
        UpgradeComparisonDecision::ReplaceExisting,
        UpgradeComparisonDecision::MergeWithExisting,
    ] {
        assert!(is_actionable_upgrade_decision(decision));
    }
    assert!(!is_actionable_upgrade_decision(
        UpgradeComparisonDecision::Duplicate
    ));
    assert!(!is_actionable_upgrade_decision(
        UpgradeComparisonDecision::Reject
    ));
}

#[test]
fn add_decision_must_preserve_prompt_two_candidate_type() {
    let mut review = MasterUpgradeReview {
        comparison_decision: UpgradeComparisonDecision::AddAsGoldenSentence,
        matched_master_section: "异议处理/质量担忧".into(),
        same_scene: true,
        new_value: "新增可复用表达".into(),
        why_better: vec![],
        duplicate_content: vec![],
        risk_or_uncertainty: vec![],
        original_host_words: "我们每台机器都经过检测。".into(),
        training_suggestion: "[建议稿] 先说明检测范围。".into(),
        recommended_action: "人工审核".into(),
    };
    assert!(validate_upgrade_review_for_prompt_two(
        &review,
        &prompt_two_review(
            SegmentDecision::SupportCandidate,
            EvidenceGrade::A,
            RiskStatus::Passed,
        )
    )
    .is_err());

    review.comparison_decision = UpgradeComparisonDecision::AddAsSupport;
    assert!(validate_upgrade_review_for_prompt_two(
        &review,
        &prompt_two_review(
            SegmentDecision::GoldenSentence,
            EvidenceGrade::A,
            RiskStatus::Passed,
        )
    )
    .is_err());
}
