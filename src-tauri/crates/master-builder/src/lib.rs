use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCue {
    pub id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterSectionKind {
    Opening,
    Product,
    Transition,
    Scenario,
    Closing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOrigin {
    HostSpeech,
    AiRewriteCandidate,
}

fn default_text_origin() -> TextOrigin {
    TextOrigin::HostSpeech
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicField {
    pub name: String,
    pub value: String,
    pub source_cue_ids: Vec<u64>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterSectionDraft {
    #[serde(default)]
    pub position: u32,
    #[serde(default)]
    pub section_key: String,
    pub kind: MasterSectionKind,
    pub product_card_id: Option<String>,
    pub source_cue_ids: Vec<u64>,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub host_text: String,
    pub master_text: String,
    #[serde(default = "default_text_origin")]
    pub text_origin: TextOrigin,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub dynamic_fields: Vec<DynamicField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterDraft {
    pub title: String,
    pub sections: Vec<MasterSectionDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterFactCard {
    pub card_id: String,
    pub static_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIssue {
    pub code: String,
    pub message: String,
    pub section_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterDraftValidation {
    pub publishable: bool,
    pub blocking_issues: Vec<BuilderIssue>,
}

pub fn validate_master_draft(
    draft: &MasterDraft,
    transcript: &[TranscriptCue],
    parameter_cards: &[ParameterFactCard],
) -> MasterDraftValidation {
    let cues = transcript
        .iter()
        .map(|cue| (cue.id, cue))
        .collect::<HashMap<_, _>>();
    let cards = parameter_cards
        .iter()
        .map(|card| (card.card_id.as_str(), card))
        .collect::<HashMap<_, _>>();
    let mut issues = Vec::new();
    let mut positions = HashSet::new();

    if draft.sections.is_empty() {
        push_issue(
            &mut issues,
            "missing_sections",
            "母稿至少需要一个有逐字稿证据的章节",
            None,
        );
    }

    for (index, section) in draft.sections.iter().enumerate() {
        let key = Some(section.section_key.clone());
        if !positions.insert(section.position) {
            push_issue(
                &mut issues,
                "duplicate_position",
                "章节位置重复",
                key.clone(),
            );
        }
        if section.position as usize != index + 1 {
            push_issue(
                &mut issues,
                "unordered_position",
                "章节必须按位置连续排列",
                key.clone(),
            );
        }
        if section.source_cue_ids.is_empty() {
            push_issue(
                &mut issues,
                "missing_cue_ids",
                "章节没有逐字稿证据",
                key.clone(),
            );
        }

        let selected = section
            .source_cue_ids
            .iter()
            .filter_map(|cue_id| match cues.get(cue_id) {
                Some(cue) => Some(*cue),
                None => {
                    push_issue(
                        &mut issues,
                        "unknown_cue_id",
                        format!("逐字稿 cue {cue_id} 不存在"),
                        key.clone(),
                    );
                    None
                }
            })
            .collect::<Vec<_>>();

        if !selected.is_empty() {
            let expected_start = selected.iter().map(|cue| cue.start_ms).min().unwrap();
            let expected_end = selected.iter().map(|cue| cue.end_ms).max().unwrap();
            if section.source_start_ms != expected_start || section.source_end_ms != expected_end {
                push_issue(
                    &mut issues,
                    "source_range_mismatch",
                    "章节时间范围与引用 cue 不一致",
                    key.clone(),
                );
            }
            let expected_host = selected
                .iter()
                .map(|cue| cue.text.trim())
                .collect::<Vec<_>>()
                .join("\n");
            if section.host_text.trim() != expected_host {
                push_issue(
                    &mut issues,
                    "host_text_mismatch",
                    "主播原话必须逐字来自引用 cue",
                    key.clone(),
                );
            }
        }

        if matches!(section.text_origin, TextOrigin::AiRewriteCandidate) {
            push_issue(
                &mut issues,
                "ai_rewrite_not_host_speech",
                "AI 改写只能作为候选，不能标记为主播原话",
                key.clone(),
            );
        }
        if section.master_text.trim() != section.host_text.trim() {
            push_issue(
                &mut issues,
                "unsupported_master_text",
                "首版母稿必须保留主播原话，不得润色或新增事实",
                key.clone(),
            );
        }
        if matches!(section.kind, MasterSectionKind::Product) {
            match section.product_card_id.as_deref() {
                Some(card_id) if cards.contains_key(card_id) => {}
                Some(_) => push_issue(
                    &mut issues,
                    "unknown_product_card",
                    "商品章节引用的参数卡不存在",
                    key.clone(),
                ),
                None => push_issue(
                    &mut issues,
                    "missing_product_card",
                    "商品章节必须关联产品参数卡",
                    key.clone(),
                ),
            }
        }
        for field in &section.dynamic_fields {
            let evidence_is_valid = !field.source_cue_ids.is_empty()
                && field.source_cue_ids.iter().all(|cue_id| {
                    cues.get(cue_id)
                        .is_some_and(|cue| cue.text.contains(&field.value))
                });
            if !field.confirmed || !evidence_is_valid {
                push_issue(
                    &mut issues,
                    "unconfirmed_dynamic_field",
                    format!("动态字段 {} 未经逐字稿证据确认", field.name),
                    key.clone(),
                );
            }
        }
    }

    MasterDraftValidation {
        publishable: issues.is_empty(),
        blocking_issues: issues,
    }
}

pub fn parse_model_sections(response: &str) -> Result<Vec<MasterSectionDraft>, String> {
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
    let parsed: serde_json::Value = serde_json::from_str(value)
        .map_err(|error| format!("MiniMax 母稿结果不是合法 JSON: {error}"))?;
    let mut sections = parsed.get("sections").cloned().unwrap_or(parsed);
    normalize_model_cue_ids(&mut sections);
    let mut sections: Vec<MasterSectionDraft> = serde_json::from_value(sections)
        .map_err(|error| format!("MiniMax 母稿章节格式错误: {error}"))?;
    for (index, section) in sections.iter_mut().enumerate() {
        section.position = u32::try_from(index + 1)
            .map_err(|error| format!("MiniMax 母稿章节数量异常: {error}"))?;
        if section.section_key.trim().is_empty() {
            section.section_key = format!("section-{:03}", index + 1);
        }
    }
    Ok(sections)
}

fn normalize_model_cue_ids(sections: &mut serde_json::Value) {
    let Some(sections) = sections.as_array_mut() else {
        return;
    };
    for section in sections {
        let Some(section) = section.as_object_mut() else {
            continue;
        };
        if let Some(cue_ids) = aliased_value_mut(section, "sourceCueIds", "source_cue_ids") {
            normalize_cue_id_array(cue_ids);
        }
        let dynamic_fields = aliased_value_mut(section, "dynamicFields", "dynamic_fields");
        if let Some(dynamic_fields) = dynamic_fields.and_then(serde_json::Value::as_array_mut) {
            for field in dynamic_fields {
                let Some(field) = field.as_object_mut() else {
                    continue;
                };
                if let Some(cue_ids) =
                    aliased_value_mut(field, "sourceCueIds", "source_cue_ids")
                {
                    normalize_cue_id_array(cue_ids);
                }
            }
        }
    }
}

fn aliased_value_mut<'a>(
    object: &'a mut serde_json::Map<String, serde_json::Value>,
    preferred: &str,
    fallback: &str,
) -> Option<&'a mut serde_json::Value> {
    let key = if object.contains_key(preferred) {
        preferred
    } else {
        fallback
    };
    object.get_mut(key)
}

fn normalize_cue_id_array(value: &mut serde_json::Value) {
    let Some(values) = value.as_array_mut() else {
        return;
    };
    *values = values
        .iter()
        .filter_map(model_cue_id)
        .map(serde_json::Value::from)
        .collect();
}

fn model_cue_id(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .or_else(|| {
            let object = value.as_object()?;
            ["id", "cueId", "cue_id"]
                .iter()
                .find_map(|key| object.get(*key).and_then(model_cue_id))
        })
}

fn push_issue(
    issues: &mut Vec<BuilderIssue>,
    code: impl Into<String>,
    message: impl Into<String>,
    section_key: Option<String>,
) {
    issues.push(BuilderIssue {
        code: code.into(),
        message: message.into(),
        section_key,
    });
}

pub fn render_master_index(script_key: &str, draft: &MasterDraft) -> String {
    let links = draft
        .sections
        .iter()
        .map(|section| {
            format!(
                "- [[{:02}-{}|{}]]",
                section.position,
                section.section_key,
                section.host_text.lines().next().unwrap_or("章节")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\nid: {script_key}\ntype: master_script\nversion: 1.0.0\nstatus: published\n---\n\n# {}\n\n{}\n",
        draft.title, links
    )
}

pub fn render_master_section(script_key: &str, section: &MasterSectionDraft) -> String {
    let cue_ids = section
        .source_cue_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "---\nmaster_id: {script_key}\nposition: {}\nkind: {}\nsource_start_ms: {}\nsource_end_ms: {}\nsource_cue_ids: [{}]\n---\n\n# {}\n\n{}\n",
        section.position,
        kind_name(&section.kind),
        section.source_start_ms,
        section.source_end_ms,
        cue_ids,
        section.host_text.lines().next().unwrap_or("章节"),
        section.master_text
    )
}

pub fn content_hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

pub fn safe_key(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_alphanumeric() || character == '-' || character == '_')
    {
        return Err("母稿编号只能包含字母、数字、短横线和下划线".into());
    }
    Ok(value.to_string())
}

pub fn master_system_prompt() -> &'static str {
    r#"你是企业直播母稿整理器，不是文案改写器。只能输出 JSON：{"sections":[...]}。
硬性规则：
1. hostText 与 masterText 必须逐字保留主播原话，不得润色、总结、补句或改变句式。
2. 每个章节必须引用真实 sourceCueIds，并填写这些 cue 的精确 sourceStartMs/sourceEndMs。
3. 参数卡只用于确认静态商品名称和型号，不得推断价格、库存、优惠、成色、链接号或成交状态。
4. 随机问答、突发互动单独标记 kind=scenario；AI 改写只能标记 textOrigin=ai_rewrite_candidate，不能混入主播原话。
5. 只输出本次指定 cueId 范围内的章节；kind 只能是 opening/product/transition/scenario/closing。"#
}

fn kind_name(kind: &MasterSectionKind) -> &'static str {
    match kind {
        MasterSectionKind::Opening => "opening",
        MasterSectionKind::Product => "product",
        MasterSectionKind::Transition => "transition",
        MasterSectionKind::Scenario => "scenario",
        MasterSectionKind::Closing => "closing",
    }
}
