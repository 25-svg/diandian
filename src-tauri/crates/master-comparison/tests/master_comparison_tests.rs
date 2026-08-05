use master_comparison::{
    compare_to_master, evaluate_segment_decision, master_source_baseline, parse_model_assessment,
    ComparisonError, EvidenceGrade, MasterSectionRef, MasterSnapshot, ModelAssessment, RawScore,
    RiskStatus, SegmentDecision,
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
        verdict: "这段先确认需求，再给出对应推荐，适合作为产品讲解参考。".into(),
        module_tags: vec!["需求探询".into(), "产品讲解".into()],
        improvements: vec!["更清楚地处理了顾客异议".into()],
        risks: vec![],
        suggested_insertion_point: "产品异议处理后".into(),
        quality_review: None,
    }
}

#[test]
fn exact_product_and_section_match_scores_against_the_expected_master() {
    let result = compare_to_master(
        &master(),
        10,
        None,
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
fn master_source_is_a_baseline_not_a_candidate_support_script() {
    let result = master_source_baseline(&master(), 20);

    assert!(result.is_master_source);
    assert_eq!(result.total_score, Some(100));
    assert_eq!(result.admission, Some(CandidateAdmission::AnalysisOnly));
    assert!(result.improvements.is_empty());
}

#[test]
fn no_matching_section_returns_without_a_score() {
    let result = compare_to_master(
        &master(),
        10,
        None,
        Some("OTHER-PRODUCT"),
        MasterSectionKind::Product,
        &[7, 8],
        assessment(RawScore::new(30, 20, 20, 15, 10, 5)),
    )
    .unwrap();

    assert_eq!(result.master_section_id, None);
    assert_eq!(result.total_score, None);
    assert_eq!(result.admission, None);
}

#[test]
fn explicit_section_id_scores_the_selected_scene_when_product_cards_are_absent() {
    let mut snapshot = master();
    snapshot.sections.push(MasterSectionRef {
        id: 21,
        product_card_id: None,
        kind: MasterSectionKind::Scenario,
    });
    let result = compare_to_master(
        &snapshot,
        10,
        Some(21),
        None,
        MasterSectionKind::Scenario,
        &[7, 8],
        assessment(RawScore::new(20, 20, 18, 13, 9, 5)),
    )
    .unwrap();

    assert_eq!(result.master_section_id, Some(21));
    assert_eq!(result.total_score, Some(85));
}

#[test]
fn stale_master_version_is_rejected_before_scoring() {
    let error = compare_to_master(
        &master(),
        9,
        None,
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
        None,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7, 8],
        assessment(RawScore::new(31, 20, 20, 15, 10, 5)),
    )
    .unwrap_err();

    assert_eq!(error, ComparisonError::InvalidScore);
}

#[test]
fn a_failed_hard_gate_blocks_even_one_hundred_model_points() {
    let mut model = assessment(RawScore::new(30, 20, 20, 15, 10, 5));
    model.gates.facts_resolved = false;
    model.gates.reasons.push("价格仍待确认".into());

    let result = compare_to_master(
        &master(),
        10,
        None,
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
fn incomplete_context_returns_analysis_only_without_a_reusability_score() {
    let mut model = assessment(RawScore::new(30, 20, 20, 15, 10, 5));
    model.gates.context_complete = false;
    model
        .gates
        .reasons
        .push("缺少报价链接和成交确认，不能视为完整成交链路".into());

    let result = compare_to_master(
        &master(),
        10,
        None,
        Some("CANON-R50"),
        MasterSectionKind::Product,
        &[7, 8],
        model,
    )
    .unwrap();

    assert_eq!(result.total_score, None);
    assert_eq!(result.score, None);
    assert_eq!(result.admission, Some(CandidateAdmission::AnalysisOnly));
}

#[test]
fn model_evidence_must_cite_the_reviewed_clip_transcript() {
    let error = compare_to_master(
        &master(),
        10,
        None,
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
    let payload = r#"{"gates":{"transcriptReviewed":true,"masterSectionMatched":true,"contextComplete":true,"factsResolved":true,"transactionEvidenceValid":true,"hostSpeechBacked":true,"notDuplicate":true,"reasons":[]},"score":{"scenarioGoal":20,"factualAccuracy":20,"talkStructure":18,"conversionAction":13,"expressionRhythm":9,"riskControl":5},"evidenceCueIds":[7],"improvements":[],"risks":[],"suggestedInsertionPoint":"异议处理后"}"#;

    let parsed = parse_model_assessment(&format!("```json\n{payload}\n```"))
        .expect("fenced model JSON should parse");

    assert_eq!(parsed.evidence_cue_ids, vec![7]);
    assert_eq!(parsed.score.scenario_goal, 20);
}

#[test]
fn parses_quoted_model_numbers_without_relaxing_score_validation() {
    let payload = r#"{"gates":{"transcriptReviewed":true,"masterSectionMatched":true,"contextComplete":true,"factsResolved":true,"transactionEvidenceValid":true,"hostSpeechBacked":true,"notDuplicate":true,"reasons":[]},"score":{"scenarioGoal":"20","factualAccuracy":"20","talkStructure":"18","conversionAction":"13","expressionRhythm":"9","riskControl":"5"},"evidenceCueIds":["525"],"improvements":[],"risks":[],"suggestedInsertionPoint":"异议处理后"}"#;

    let parsed =
        parse_model_assessment(payload).expect("quoted decimal score and cue values should parse");

    assert_eq!(parsed.evidence_cue_ids, vec![525]);
    assert_eq!(parsed.score, RawScore::new(20, 20, 18, 13, 9, 5));
}

#[test]
fn parses_single_prose_explanations_as_one_item_lists() {
    let payload = r#"{"gates":{"transcriptReviewed":true,"masterSectionMatched":true,"contextComplete":true,"factsResolved":true,"transactionEvidenceValid":true,"hostSpeechBacked":true,"notDuplicate":true,"reasons":"价格已在片段内确认"},"score":{"scenarioGoal":20,"factualAccuracy":20,"talkStructure":18,"conversionAction":13,"expressionRhythm":9,"riskControl":5},"evidenceCueIds":[525],"verdict":"先问需求再推荐，适合作为产品介绍结构。","moduleTags":"需求探询","improvements":"成交引导清晰","risks":"未明确下一步动作","suggestedInsertionPoint":"异议处理后"}"#;

    let parsed = parse_model_assessment(payload)
        .expect("single prose explanations should be normalized into lists");

    assert_eq!(parsed.improvements, vec!["成交引导清晰"]);
    assert_eq!(parsed.risks, vec!["未明确下一步动作"]);
    assert_eq!(parsed.gates.reasons, vec!["价格已在片段内确认"]);
    assert_eq!(parsed.module_tags, vec!["需求探询"]);
    assert_eq!(parsed.verdict, "先问需求再推荐，适合作为产品介绍结构。");
}

#[test]
fn deterministically_decides_how_each_scored_segment_may_be_used() {
    assert_eq!(
        evaluate_segment_decision("异议处理", 85, EvidenceGrade::A, RiskStatus::Passed),
        SegmentDecision::SupportCandidate
    );
    assert_eq!(
        evaluate_segment_decision("高质量金句", 85, EvidenceGrade::B, RiskStatus::NeedsReview),
        SegmentDecision::GoldenSentence
    );
    assert_eq!(
        evaluate_segment_decision("产品讲解", 85, EvidenceGrade::C, RiskStatus::Passed),
        SegmentDecision::TrainingMaterial
    );
    assert_eq!(
        evaluate_segment_decision("异议处理", 84, EvidenceGrade::A, RiskStatus::Passed),
        SegmentDecision::TrainingMaterial
    );
    assert_eq!(
        evaluate_segment_decision("异议处理", 69, EvidenceGrade::A, RiskStatus::Passed),
        SegmentDecision::ReferenceOnly
    );
    assert_eq!(
        evaluate_segment_decision(
            "需要改进的反面案例",
            96,
            EvidenceGrade::A,
            RiskStatus::Passed
        ),
        SegmentDecision::ReferenceOnly
    );
    assert_eq!(
        evaluate_segment_decision("产品推荐", 96, EvidenceGrade::A, RiskStatus::Blocked),
        SegmentDecision::Blocked
    );
}

#[test]
fn parses_prompt_two_details_and_overrides_the_model_decision() {
    let payload = r#"{
      "segment_id":"SEG-001",
      "segment_type":"异议处理",
      "total_score":"85",
      "decision":"reference_only",
      "evidence_grade":"B",
      "risk_status":"needs_review",
      "score_details":{
        "scene_goal":{"score":"26","max_score":"30","reason":"回应了质量顾虑","evidence":"00:01:05 我们每台都会检测"},
        "persuasiveness":{"score":17,"max_score":20,"reason":"证据具体","evidence":["00:01:12 有售后保障"]},
        "master_increment":{"score":17,"max_score":20,"reason":"补充检测说明","evidence":["00:01:05 我们每台都会检测"]},
        "reusability":{"score":12,"max_score":15,"reason":"新人可以直接使用","evidence":["00:01:12 有售后保障"]},
        "factual_accuracy":{"score":8,"max_score":10,"reason":"主要事实有依据","evidence":["00:01:05 我们每台都会检测"]},
        "natural_expression":{"score":5,"max_score":5,"reason":"表达自然简洁","evidence":["00:01:12 有售后保障"]}
      },
      "what_is_good":"先回应顾虑，再给检测和售后证据",
      "what_needs_improvement":["补充明确的下一步动作"],
      "facts_to_confirm":["售后范围需要确认"],
      "reusable_original_sentence":"我们每台都会检测，有售后保障。",
      "suggested_training_version":"[建议稿] 先说明检测，再说明售后范围。",
      "recommended_master_section":"异议处理·质量顾虑",
      "evidence_cue_ids":["7"]
    }"#;

    let parsed = parse_model_assessment(&format!("```json\n{payload}\n```")).unwrap();
    let review = parsed.quality_review.unwrap();

    assert_eq!(review.total_score, 85);
    assert_eq!(review.decision, SegmentDecision::SupportCandidate);
    assert_eq!(review.evidence_grade, EvidenceGrade::B);
    assert_eq!(review.risk_status, RiskStatus::NeedsReview);
    assert_eq!(review.what_is_good, vec!["先回应顾虑，再给检测和售后证据"]);
    assert_eq!(parsed.evidence_cue_ids, vec![7]);
}

#[test]
fn parses_prompt_two_with_cue_prefix_and_wrong_natural_expression_cap() {
    let payload = r#"{
      "segment_id":"SEG-001",
      "segment_type":"产品讲解",
      "total_score":79,
      "decision":"training_material",
      "evidence_grade":"B",
      "risk_status":"needs_review",
      "score_details":{
        "sceneGoal":{"score":24,"max_score":30,"reason":"讲清了用途","evidence":["00:00:06 五零定焦"]},
        "persuasiveness":{"score":16,"max_score":20,"reason":"有说服力","evidence":["00:00:06 五零定焦"]},
        "masterIncrement":{"score":15,"max_score":20,"reason":"补充使用场景","evidence":["00:00:06 五零定焦"]},
        "reusability":{"score":12,"max_score":15,"reason":"新人可参考","evidence":["00:00:06 五零定焦"]},
        "factualAccuracy":{"score":8,"max_score":10,"reason":"事实清楚","evidence":["00:00:06 五零定焦"]},
        "naturalExpression":{"score":4,"max_score":10,"reason":"表达自然","evidence":["00:00:06 五零定焦"]}
      },
      "what_is_good":["用自身设备举例"],
      "what_needs_improvement":["缺少明确链接动作"],
      "facts_to_confirm":[],
      "reusable_original_sentence":"五零定焦",
      "suggested_training_version":"[建议稿] 先讲用途再讲价格",
      "recommended_master_section":"产品讲解与成色展示",
      "evidence_cue_ids":["cue_1"]
    }"#;

    let parsed =
        parse_model_assessment(payload).expect("cue prefix and max_score repair should parse");
    let review = parsed.quality_review.expect("quality review");

    assert_eq!(parsed.evidence_cue_ids, vec![1]);
    assert_eq!(review.score_details.natural_expression.max_score, 5);
    assert_eq!(review.score_details.natural_expression.score, 4);
    assert_eq!(review.total_score, 79);
}
