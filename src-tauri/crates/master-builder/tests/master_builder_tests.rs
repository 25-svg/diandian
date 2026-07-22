use master_builder::{
    master_system_prompt, parse_model_sections, render_master_index, render_master_section,
    safe_key, validate_master_draft, DynamicField, MasterDraft, MasterSectionDraft,
    MasterSectionKind, ParameterFactCard, TextOrigin, TranscriptCue,
};

#[test]
fn rejects_a_master_draft_without_sections() {
    let draft = MasterDraft {
        title: "整场母稿".into(),
        sections: vec![],
    };

    let result = validate_master_draft(&draft, &transcript(), &[]);

    assert!(!result.publishable);
    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "missing_sections"));
}

#[test]
fn parses_model_sections_with_or_without_markdown_fences() {
    let json = r#"{"sections":[{"position":1,"sectionKey":"opening","kind":"opening","productCardId":null,"sourceCueIds":[1],"sourceStartMs":0,"sourceEndMs":2000,"hostText":"欢迎来到直播间","masterText":"欢迎来到直播间","textOrigin":"host_speech","conditions":[],"dynamicFields":[]}]}"#;

    assert_eq!(parse_model_sections(json).unwrap().len(), 1);
    assert_eq!(
        parse_model_sections(&format!("```json\n{json}\n```\n"))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn fills_missing_model_positions_from_section_order() {
    let json = r#"{"sections":[{"sectionKey":"opening","kind":"opening","productCardId":null,"sourceCueIds":[1],"sourceStartMs":0,"sourceEndMs":2000,"hostText":"欢迎来到直播间","masterText":"欢迎来到直播间","textOrigin":"host_speech","conditions":[],"dynamicFields":[]},{"sectionKey":"closing","kind":"closing","productCardId":null,"sourceCueIds":[5],"sourceStartMs":12000,"sourceEndMs":14000,"hostText":"感谢大家今天的陪伴","masterText":"感谢大家今天的陪伴","textOrigin":"host_speech","conditions":[],"dynamicFields":[]}]}"#;

    let sections = parse_model_sections(json).unwrap();

    assert_eq!(sections[0].position, 1);
    assert_eq!(sections[1].position, 2);
}

#[test]
fn normalizes_model_cue_id_objects_and_strings() {
    let json = r#"{"sections":[{"sectionKey":"opening","kind":"opening","productCardId":null,"sourceCueIds":[{"id":1},"2"],"sourceStartMs":0,"sourceEndMs":6000,"hostText":"欢迎来到直播间\n这台佳能R50成色很好","masterText":"欢迎来到直播间\n这台佳能R50成色很好","textOrigin":"host_speech","conditions":[],"dynamicFields":[{"name":"价格","value":"R50","sourceCueIds":[{"cueId":2}],"confirmed":true}]}]}"#;

    let sections = parse_model_sections(json).unwrap();

    assert_eq!(sections[0].source_cue_ids, vec![1, 2]);
    assert_eq!(sections[0].dynamic_fields[0].source_cue_ids, vec![2]);
}

#[test]
fn fills_missing_non_business_structure_fields() {
    let json = r#"{"sections":[{"kind":"opening","sourceCueIds":[1],"sourceStartMs":0,"sourceEndMs":2000,"hostText":"欢迎来到直播间","masterText":"欢迎来到直播间"}]}"#;

    let sections = parse_model_sections(json).unwrap();

    assert_eq!(sections[0].position, 1);
    assert_eq!(sections[0].section_key, "section-001");
    assert_eq!(sections[0].text_origin, TextOrigin::HostSpeech);
    assert!(sections[0].conditions.is_empty());
    assert!(sections[0].dynamic_fields.is_empty());
}

#[test]
fn normalizes_model_text_origin_aliases() {
    let json = r#"{"sections":[{"kind":"opening","sourceCueIds":[1],"sourceStartMs":0,"sourceEndMs":2000,"hostText":"欢迎来到直播间","masterText":"欢迎来到直播间","textOrigin":"verbatim"}]}"#;

    let sections = parse_model_sections(json).unwrap();

    assert_eq!(sections[0].text_origin, TextOrigin::HostSpeech);
}

fn cue(id: u64, start_ms: u64, end_ms: u64, text: &str) -> TranscriptCue {
    TranscriptCue {
        id,
        start_ms,
        end_ms,
        text: text.into(),
    }
}

fn section(
    position: u32,
    kind: MasterSectionKind,
    cue_ids: Vec<u64>,
    host_text: &str,
) -> MasterSectionDraft {
    let (source_start_ms, source_end_ms) = match cue_ids.as_slice() {
        [1] => (0, 2_000),
        [2] => (2_000, 6_000),
        [3] => (6_000, 8_000),
        [4] => (8_000, 12_000),
        [5] => (12_000, 14_000),
        _ => (0, 0),
    };
    MasterSectionDraft {
        position,
        section_key: format!("section-{position}"),
        kind,
        product_card_id: None,
        source_cue_ids: cue_ids,
        source_start_ms,
        source_end_ms,
        host_text: host_text.into(),
        master_text: host_text.into(),
        text_origin: TextOrigin::HostSpeech,
        conditions: vec![],
        dynamic_fields: vec![],
    }
}

fn transcript() -> Vec<TranscriptCue> {
    vec![
        cue(1, 0, 2_000, "欢迎来到直播间"),
        cue(2, 2_000, 6_000, "这台佳能R50成色很好"),
        cue(3, 6_000, 8_000, "接下来看看镜头"),
        cue(4, 8_000, 12_000, "小白兔镜头对焦很快"),
        cue(5, 12_000, 14_000, "感谢大家今天的陪伴"),
    ]
}

#[test]
fn accepts_ordered_multi_product_host_speech() {
    let mut product_one = section(
        2,
        MasterSectionKind::Product,
        vec![2],
        "这台佳能R50成色很好",
    );
    product_one.product_card_id = Some("CANON-R50".into());
    product_one.source_start_ms = 2_000;
    product_one.source_end_ms = 6_000;
    let mut product_two = section(4, MasterSectionKind::Product, vec![4], "小白兔镜头对焦很快");
    product_two.product_card_id = Some("CANON-70-200".into());
    product_two.source_start_ms = 8_000;
    product_two.source_end_ms = 12_000;
    let draft = MasterDraft {
        title: "整场母稿".into(),
        sections: vec![
            section(1, MasterSectionKind::Opening, vec![1], "欢迎来到直播间"),
            product_one,
            section(3, MasterSectionKind::Transition, vec![3], "接下来看看镜头"),
            product_two,
            section(5, MasterSectionKind::Closing, vec![5], "感谢大家今天的陪伴"),
        ],
    };

    let result = validate_master_draft(
        &draft,
        &transcript(),
        &[
            ParameterFactCard {
                card_id: "CANON-R50".into(),
                static_terms: vec!["佳能R50".into()],
            },
            ParameterFactCard {
                card_id: "CANON-70-200".into(),
                static_terms: vec!["小白兔".into()],
            },
        ],
    );

    assert!(
        result.blocking_issues.is_empty(),
        "{:?}",
        result.blocking_issues
    );
    assert!(result.publishable);
}

#[test]
fn rejects_missing_cues_out_of_range_and_duplicate_positions() {
    let mut missing = section(1, MasterSectionKind::Opening, vec![], "欢迎来到直播间");
    missing.source_start_ms = 99_000;
    missing.source_end_ms = 100_000;
    let duplicate = section(1, MasterSectionKind::Closing, vec![99], "不存在");
    let draft = MasterDraft {
        title: "坏母稿".into(),
        sections: vec![missing, duplicate],
    };

    let result = validate_master_draft(&draft, &transcript(), &[]);

    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "missing_cue_ids"));
    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "unknown_cue_id"));
    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "duplicate_position"));
    assert!(!result.publishable);
}

#[test]
fn rejects_rewrites_and_unsupported_facts() {
    let mut rewritten = section(
        1,
        MasterSectionKind::Product,
        vec![2],
        "这台佳能R50成色很好",
    );
    rewritten.product_card_id = Some("CANON-R50".into());
    rewritten.source_start_ms = 2_000;
    rewritten.source_end_ms = 6_000;
    rewritten.master_text = "这台佳能R50全网最低价，闭眼入".into();
    rewritten.text_origin = TextOrigin::AiRewriteCandidate;
    rewritten.dynamic_fields.push(DynamicField {
        name: "price".into(),
        value: "3999".into(),
        source_cue_ids: vec![],
        confirmed: false,
    });
    let draft = MasterDraft {
        title: "坏母稿".into(),
        sections: vec![rewritten],
    };

    let result = validate_master_draft(
        &draft,
        &transcript(),
        &[ParameterFactCard {
            card_id: "CANON-R50".into(),
            static_terms: vec!["佳能R50".into()],
        }],
    );

    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "ai_rewrite_not_host_speech"));
    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "unsupported_master_text"));
    assert!(result
        .blocking_issues
        .iter()
        .any(|issue| issue.code == "unconfirmed_dynamic_field"));
}

#[test]
fn renders_only_valid_master_paths_and_preserves_host_text() {
    assert!(safe_key("MS-001").is_ok());
    assert!(safe_key("../escape").is_err());
    let draft = MasterDraft {
        title: "整场母稿".into(),
        sections: vec![section(
            1,
            MasterSectionKind::Opening,
            vec![1],
            "欢迎来到直播间",
        )],
    };

    let index = render_master_index("MS-001", &draft);
    let section_markdown = render_master_section("MS-001", &draft.sections[0]);

    assert!(index.contains("[[01-section-1|欢迎来到直播间]]"));
    assert!(section_markdown.contains("\n欢迎来到直播间\n"));
    assert!(!section_markdown.contains("AI"));
}

#[test]
fn model_prompt_forbids_rewriting_and_dynamic_fact_inference() {
    let prompt = master_system_prompt();
    for required in [
        "逐字保留主播原话",
        "不得润色",
        "sourceCueIds",
        "不得推断价格",
        "kind=scenario",
        "ai_rewrite_candidate",
    ] {
        assert!(prompt.contains(required), "missing prompt rule: {required}");
    }
}
