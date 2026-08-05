use crate::handlers::ai::request_minimax_text;
use crate::master_script::company_benchmark::CompanyDealBenchmark;
use futures::{stream, StreamExt, TryStreamExt};
use master_builder::{MasterDraft, ParameterFactCard, TranscriptCue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

// Keep model output comfortably below MiniMax's response token limit.
const BLOCK_SIZE: usize = 32;
const CONTEXT_SIZE: usize = 5;
const MAX_CONCURRENT_BLOCKS: usize = 4;

const BATCH_JSON_REPAIR_SYSTEM_PROMPT: &str = r#"你是严格的 JSON 格式修复器。
输入是一份母稿草稿的错误 JSON。只修复 JSON 语法，不得增删、改写、概括或翻译任何字段值。
只输出一个合法 JSON 对象，不要 Markdown，不要解释。
对象必须保留此结构：
{
  "title":"...",
  "patterns":[{"name":"...","observation":"...","evidenceIds":["..."]}],
  "sections":[{"title":"...","purpose":"...","evidenceIds":["..."],"conditions":["..."],"dynamicFields":["..."]}],
  "operatingRules":["..."]
}
evidenceIds 必须原样保留，不能创造新的证据编号。"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEvidence {
    pub evidence_id: String,
    pub video_id: i64,
    pub video_title: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPattern {
    pub name: String,
    pub observation: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchMasterSection {
    pub title: String,
    pub purpose: String,
    pub evidence_ids: Vec<String>,
    pub fixed_speech: String,
    pub conditions: Vec<String>,
    pub dynamic_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchMasterDraft {
    pub title: String,
    pub evidence: Vec<BatchEvidence>,
    pub patterns: Vec<BatchPattern>,
    pub sections: Vec<BatchMasterSection>,
    pub operating_rules: Vec<String>,
    #[serde(default)]
    pub company_benchmarks: Vec<CompanyDealBenchmark>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchModelDraft {
    title: String,
    patterns: Vec<BatchPattern>,
    sections: Vec<BatchModelSection>,
    #[serde(default)]
    operating_rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchModelSection {
    title: String,
    purpose: String,
    evidence_ids: Vec<String>,
    #[serde(default)]
    conditions: Vec<String>,
    #[serde(default)]
    dynamic_fields: Vec<String>,
}

pub fn batch_master_system_prompt() -> &'static str {
    r#"你是企业直播话术复盘分析官。根据多场直播的【证据片段】提炼一份待审核的成交片段型企业母稿草稿。

整场逐字稿只作为证据来源，不得把整场直播回放或整场逐字稿直接当作母稿。
优先识别完整成交链路：客户需求/疑问 → 产品匹配 → 卖点或价值说明 → 风险消除/售后承诺 → 价格/链接/优惠 → 引导下单 → 确认成交。
同一用户、同一商品在 5 分钟内恢复且上下文能够对应时，允许把被插话打断的内容作为同一链路分析；不得拼接不同商品、不同用户或已经形成的新成交链路。
链路不完整但某一句或某个模块值得学习时，只能归纳为优秀局部训练素材，不得作为完整成交范例，也不得据此声称已经成交。

只输出合法 JSON，不要 Markdown，不要解释。固定结构：
{
  "title":"...",
  "patterns":[{"name":"...","observation":"...","evidenceIds":["..."]}],
  "sections":[{"title":"...","purpose":"...","evidenceIds":["..."],"conditions":["..."],"dynamicFields":["..."]}],
  "operatingRules":["..."]
}

硬性规则：
1. evidenceIds 只能引用输入中已有的 evidenceId，不能编造。
2. 每个规律和每个章节至少引用 1 条证据；没有证据则不输出。
3. 不要改写、润色、拼接或总结成主播原话。系统会在后续将 evidenceIds 对应的原话原样展示。
4. 价格、库存、赠品、链接、具体商品型号均是动态事实，放到 conditions 或 dynamicFields，不得写成固定话术。
5. patterns 提炼企业成交规则；sections 按完整成交链路模块整理证据，并明确缺失环节。
6. operatingRules 必须包含“先完成当前用户关键动作，再用一句话排队承接其他用户”的防打断规则。
7. 结论必须是给直播运营新人可执行的简明规则。
8. 【公司成交结构基准】只用于补全成交链路和检查结构缺口，不能作为主播固定原话，也不是逐字稿证据。
9. 固定原话仍只能来自 evidenceIds 对应的证据片段；公司基准中的价格、赠品、链接和履约信息也必须视为当场变量。"#
}

fn batch_master_payload(
    title: &str,
    evidence: &[BatchEvidence],
    parameter_cards: &[ParameterFactCard],
    company_benchmarks: &[CompanyDealBenchmark],
) -> serde_json::Value {
    json!({
        "批次名称": title,
        "公司确认参数卡": parameter_cards,
        "公司成交结构基准": company_benchmarks,
        "证据片段": evidence,
    })
}

pub async fn generate_batch_master_draft(
    api_key: &str,
    title: &str,
    evidence: Vec<BatchEvidence>,
    parameter_cards: &[ParameterFactCard],
    company_benchmarks: &[CompanyDealBenchmark],
) -> Result<BatchMasterDraft, String> {
    if evidence.is_empty() {
        return Err("没有可用于跨场提炼的逐字稿证据".into());
    }
    let prompt = batch_master_system_prompt();
    let payload = batch_master_payload(title, &evidence, parameter_cards, company_benchmarks);
    let response = request_minimax_text(
        api_key,
        prompt,
        vec![json!({"role": "user", "content": payload.to_string()})],
        8_192,
    )
    .await?;
    let parsed = match parse_batch_model_draft(&response) {
        Ok(draft) => draft,
        Err(first_error) => {
            let repair_prompt = build_batch_json_repair_prompt(&response);
            let repaired_response = request_minimax_text(
                api_key,
                BATCH_JSON_REPAIR_SYSTEM_PROMPT,
                vec![json!({"role": "user", "content": repair_prompt})],
                8_192,
            )
            .await
            .map_err(|repair_error| {
                format!(
                    "MiniMax 跨场母稿草稿格式错误，且格式重整请求失败：初次错误：{first_error}；重整错误：{repair_error}"
                )
            })?;
            parse_batch_model_draft(&repaired_response).map_err(|repair_error| {
                format!(
                    "MiniMax 跨场母稿草稿格式错误，格式重整后仍无法解析：初次错误：{first_error}；重整错误：{repair_error}"
                )
            })?
        }
    };
    let evidence_by_id = evidence
        .iter()
        .map(|item| (item.evidence_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    if parsed.patterns.is_empty() || parsed.sections.is_empty() {
        return Err("MiniMax 没有返回可审核的跨场规律或母稿章节".into());
    }
    let patterns = parsed
        .patterns
        .into_iter()
        .map(|pattern| {
            validate_batch_evidence_ids(&pattern.evidence_ids, &evidence_by_id, "规律")?;
            Ok(pattern)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let sections = parsed
        .sections
        .into_iter()
        .map(|section| {
            validate_batch_evidence_ids(&section.evidence_ids, &evidence_by_id, "章节")?;
            let fixed_speech = section
                .evidence_ids
                .iter()
                .filter_map(|id| evidence_by_id.get(id.as_str()))
                .map(|item| item.quote.trim())
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(BatchMasterSection {
                title: section.title,
                purpose: section.purpose,
                evidence_ids: section.evidence_ids,
                fixed_speech,
                conditions: section.conditions,
                dynamic_fields: section.dynamic_fields,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(BatchMasterDraft {
        title: if parsed.title.trim().is_empty() {
            title.to_string()
        } else {
            parsed.title
        },
        evidence,
        patterns,
        sections,
        operating_rules: parsed.operating_rules,
        company_benchmarks: company_benchmarks.to_vec(),
    })
}

fn parse_batch_model_draft(response: &str) -> Result<BatchModelDraft, String> {
    let mut value = response.trim();
    if let Some(unfenced) = value.strip_prefix("```json") {
        value = unfenced;
    } else if let Some(unfenced) = value.strip_prefix("```") {
        value = unfenced;
    }
    if let Some(unfenced) = value.trim().strip_suffix("```") {
        value = unfenced.trim();
    }

    match serde_json::from_str(value) {
        Ok(draft) => Ok(draft),
        Err(original_error) => {
            let repaired = repair_missing_json_commas(value);
            serde_json::from_str(&repaired)
                .map_err(|_| format!("MiniMax 跨场母稿草稿格式错误：{original_error}"))
        }
    }
}

fn build_batch_json_repair_prompt(malformed: &str) -> String {
    format!(
        "Return valid JSON only. Preserve title, patterns, sections, operatingRules, and every evidenceIds value exactly.\n\nMalformed model output:\n{malformed}"
    )
}

#[derive(Clone, Copy)]
enum JsonContainer {
    Object(JsonObjectState),
    Array(JsonArrayState),
}

#[derive(Clone, Copy)]
enum JsonObjectState {
    KeyOrEnd,
    Colon,
    Value,
    CommaOrEnd,
}

#[derive(Clone, Copy)]
enum JsonArrayState {
    ValueOrEnd,
    CommaOrEnd,
}

/// Repairs the one JSON mistake MiniMax most commonly makes for this large
/// response: omitting a comma between two already-complete values. The state
/// machine never alters quoted content or field values; it only inserts a
/// comma where the surrounding JSON structure unambiguously requires one.
fn repair_missing_json_commas(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut repaired = String::with_capacity(value.len() + 16);
    let mut stack = Vec::<JsonContainer>::new();
    let mut index = 0;

    while index < chars.len() {
        match chars[index] {
            '"' => {
                let end = json_string_end(&chars, index);
                before_json_string(&mut stack, &mut repaired);
                repaired.extend(chars[index..end].iter());
                index = end;
            }
            '{' => {
                before_json_value(&mut stack, &mut repaired);
                repaired.push('{');
                stack.push(JsonContainer::Object(JsonObjectState::KeyOrEnd));
                index += 1;
            }
            '[' => {
                before_json_value(&mut stack, &mut repaired);
                repaired.push('[');
                stack.push(JsonContainer::Array(JsonArrayState::ValueOrEnd));
                index += 1;
            }
            '}' | ']' => {
                repaired.push(chars[index]);
                stack.pop();
                complete_json_value(&mut stack);
                index += 1;
            }
            ':' => {
                repaired.push(':');
                if let Some(JsonContainer::Object(state)) = stack.last_mut() {
                    if matches!(state, JsonObjectState::Colon) {
                        *state = JsonObjectState::Value;
                    }
                }
                index += 1;
            }
            ',' => {
                repaired.push(',');
                if let Some(container) = stack.last_mut() {
                    match container {
                        JsonContainer::Object(state) => *state = JsonObjectState::KeyOrEnd,
                        JsonContainer::Array(state) => *state = JsonArrayState::ValueOrEnd,
                    }
                }
                index += 1;
            }
            ';' | '；' => {
                // MiniMax occasionally uses a semicolon between completed JSON
                // fields. Convert it only when the enclosing structure expects
                // a separator; quoted content is handled by the string branch.
                if let Some(container) = stack.last_mut() {
                    match container {
                        JsonContainer::Object(state)
                            if matches!(state, JsonObjectState::CommaOrEnd) =>
                        {
                            repaired.push(',');
                            *state = JsonObjectState::KeyOrEnd;
                        }
                        JsonContainer::Array(state)
                            if matches!(state, JsonArrayState::CommaOrEnd) =>
                        {
                            repaired.push(',');
                            *state = JsonArrayState::ValueOrEnd;
                        }
                        _ => repaired.push(chars[index]),
                    }
                } else {
                    repaired.push(chars[index]);
                }
                index += 1;
            }
            character if character.is_whitespace() => {
                repaired.push(character);
                index += 1;
            }
            _ => {
                let end = json_scalar_end(&chars, index);
                before_json_value(&mut stack, &mut repaired);
                repaired.extend(chars[index..end].iter());
                complete_json_value(&mut stack);
                index = end;
            }
        }
    }
    repaired
}

fn json_string_end(chars: &[char], start: usize) -> usize {
    let mut index = start + 1;
    let mut escaped = false;
    while index < chars.len() {
        match chars[index] {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return index + 1,
            _ => escaped = false,
        }
        index += 1;
    }
    chars.len()
}

fn json_scalar_end(chars: &[char], start: usize) -> usize {
    let mut index = start;
    while index < chars.len() && !matches!(chars[index], '{' | '}' | '[' | ']' | ':' | ',' | '"') {
        if chars[index].is_whitespace() {
            break;
        }
        index += 1;
    }
    index.max(start + 1)
}

fn before_json_string(stack: &mut [JsonContainer], repaired: &mut String) {
    let Some(JsonContainer::Object(state)) = stack.last_mut() else {
        before_json_value(stack, repaired);
        complete_json_value(stack);
        return;
    };
    match state {
        JsonObjectState::KeyOrEnd => *state = JsonObjectState::Colon,
        JsonObjectState::Value => *state = JsonObjectState::CommaOrEnd,
        JsonObjectState::CommaOrEnd => {
            repaired.push(',');
            *state = JsonObjectState::Colon;
        }
        JsonObjectState::Colon => {}
    }
}

fn before_json_value(stack: &mut [JsonContainer], repaired: &mut String) {
    let Some(container) = stack.last_mut() else {
        return;
    };
    match container {
        JsonContainer::Object(state) if matches!(state, JsonObjectState::Value) => {}
        JsonContainer::Array(state) if matches!(state, JsonArrayState::ValueOrEnd) => {}
        JsonContainer::Array(state) if matches!(state, JsonArrayState::CommaOrEnd) => {
            repaired.push(',');
            *state = JsonArrayState::ValueOrEnd;
        }
        _ => {}
    }
}

fn complete_json_value(stack: &mut [JsonContainer]) {
    let Some(container) = stack.last_mut() else {
        return;
    };
    match container {
        JsonContainer::Object(state) if matches!(state, JsonObjectState::Value) => {
            *state = JsonObjectState::CommaOrEnd;
        }
        JsonContainer::Array(state) if matches!(state, JsonArrayState::ValueOrEnd) => {
            *state = JsonArrayState::CommaOrEnd;
        }
        _ => {}
    }
}

fn validate_batch_evidence_ids(
    ids: &[String],
    evidence: &HashMap<&str, &BatchEvidence>,
    subject: &str,
) -> Result<(), String> {
    if ids.is_empty() {
        return Err(format!("MiniMax 返回的{subject}没有引用逐字稿证据"));
    }
    if let Some(unknown) = ids.iter().find(|id| !evidence.contains_key(id.as_str())) {
        return Err(format!("MiniMax 返回了不存在的证据编号：{unknown}"));
    }
    Ok(())
}

pub async fn generate_master_draft(
    api_key: &str,
    title: &str,
    transcript: &[TranscriptCue],
    parameter_cards: &[ParameterFactCard],
) -> Result<MasterDraft, String> {
    if transcript.is_empty() {
        return Err("规范逐字稿为空，无法生成母稿".into());
    }
    let mut block_results = stream::iter((0..transcript.len()).step_by(BLOCK_SIZE).enumerate())
        .map(|(block_index, core_start)| async move {
            let core_end = (core_start + BLOCK_SIZE).min(transcript.len());
            let context_start = core_start.saturating_sub(CONTEXT_SIZE);
            let context_end = (core_end + CONTEXT_SIZE).min(transcript.len());
            let payload = json!({
                "直播标题": title,
                "仅为上下文的前后句": &transcript[context_start..context_end],
                "本次必须整理的cueId范围": [transcript[core_start].id, transcript[core_end - 1].id],
                "公司确认参数卡": parameter_cards,
            });
            let response = request_minimax_text(
                api_key,
                master_builder::master_system_prompt(),
                vec![json!({"role": "user", "content": payload.to_string()})],
                8_192,
            )
            .await?;
            let sections = master_builder::parse_model_sections(&response)?;
            Ok::<_, String>((block_index, sections))
        })
        .buffer_unordered(MAX_CONCURRENT_BLOCKS)
        .try_collect::<Vec<_>>()
        .await?;
    block_results.sort_by_key(|(block_index, _)| *block_index);
    let mut sections = block_results
        .into_iter()
        .flat_map(|(_, sections)| sections)
        .collect::<Vec<_>>();
    let cues = transcript
        .iter()
        .map(|cue| (cue.id, cue))
        .collect::<HashMap<_, _>>();
    for (index, section) in sections.iter_mut().enumerate() {
        section.position = u32::try_from(index + 1).map_err(|error| error.to_string())?;
        section.section_key = format!("section-{:04}", index + 1);
        let selected = section
            .source_cue_ids
            .iter()
            .filter_map(|cue_id| cues.get(cue_id).copied())
            .collect::<Vec<_>>();
        if !selected.is_empty() && selected.len() == section.source_cue_ids.len() {
            section.source_start_ms = selected.iter().map(|cue| cue.start_ms).min().unwrap();
            section.source_end_ms = selected.iter().map(|cue| cue.end_ms).max().unwrap();
            section.host_text = selected
                .iter()
                .map(|cue| cue.text.trim())
                .collect::<Vec<_>>()
                .join("\n");
            section.master_text = section.host_text.clone();
            section.text_origin = master_builder::TextOrigin::HostSpeech;
        }
    }
    Ok(MasterDraft {
        title: title.to_string(),
        sections,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        batch_master_payload, batch_master_system_prompt, build_batch_json_repair_prompt,
        parse_batch_model_draft, BatchEvidence,
    };
    use crate::master_script::company_benchmark::CompanyDealBenchmark;

    #[test]
    fn batch_prompt_uses_transaction_examples_instead_of_whole_replays() {
        let prompt = batch_master_system_prompt();
        assert!(prompt.contains("整场逐字稿只作为证据来源"));
        assert!(prompt.contains("客户需求/疑问"));
        assert!(prompt.contains("5 分钟内"));
        assert!(prompt.contains("优秀局部训练素材"));
        assert!(prompt.contains("不得作为完整成交范例"));
        assert!(prompt.contains("公司成交结构基准"));
        assert!(prompt.contains("不能作为主播固定原话"));
        assert!(prompt.contains("固定原话仍只能来自 evidenceIds"));
    }

    #[test]
    fn batch_payload_includes_company_benchmarks_separately_from_evidence() {
        let evidence = vec![BatchEvidence {
            evidence_id: "V1-01".into(),
            video_id: 1,
            video_title: "样本".into(),
            start_ms: 0,
            end_ms: 1,
            quote: "主播原话".into(),
        }];
        let benchmarks = vec![CompanyDealBenchmark {
            card_id: "CDB-001".into(),
            title: "公司基准".into(),
            version: "1.0.0".into(),
            product_key: "canon-xiaobaitu".into(),
            product_name: "佳能小白兔".into(),
            aliases: vec!["小白兔".into()],
            body: "成交结构".into(),
        }];
        let payload = batch_master_payload("批次", &evidence, &[], &benchmarks);

        assert_eq!(payload["公司成交结构基准"][0]["cardId"], "CDB-001");
        assert_eq!(payload["证据片段"][0]["evidenceId"], "V1-01");
    }

    #[test]
    fn json_repair_prompt_preserves_malformed_model_output_and_schema() {
        let malformed = r#"{"title":"draft" "patterns":[]}"#;
        let prompt = build_batch_json_repair_prompt(malformed);

        assert!(prompt.contains(malformed));
        assert!(prompt.contains("valid JSON"));
        assert!(prompt.contains("evidenceIds"));
    }

    #[test]
    fn repairs_an_omitted_comma_between_top_level_fields() {
        let response = r#"{
          "title":"主播母稿",
          "patterns":[{"name":"成交推进","observation":"先确认需求","evidenceIds":["V1-01"]}]
          "sections":[{"title":"需求确认","purpose":"确认预算","evidenceIds":["V1-01"],"conditions":[],"dynamicFields":[]}],
          "operatingRules":["先问需求"]
        }"#;

        let draft = parse_batch_model_draft(response).expect("the missing comma is repairable");

        assert_eq!(draft.title, "主播母稿");
        assert_eq!(draft.patterns[0].evidence_ids, ["V1-01"]);
        assert_eq!(draft.sections[0].title, "需求确认");
    }

    #[test]
    fn repairs_semicolon_used_as_a_json_field_separator() {
        let response = r#"{
          "title":"主播话术";
          "patterns":[{"name":"成交推进","observation":"先确认需求","evidenceIds":["V1-01"]}];
          "sections":[{"title":"需求确认","purpose":"确认预算","evidenceIds":["V1-01"],"conditions":[],"dynamicFields":[]}];
          "operatingRules":["先问需求"]
        }"#;

        let draft = parse_batch_model_draft(response).expect("a semicolon separator is repairable");

        assert_eq!(draft.title, "主播话术");
        assert_eq!(draft.sections.len(), 1);
    }

    #[test]
    fn keeps_quoted_text_unchanged_while_repairing_structure() {
        let response = r#"{"title":"R五二，主播说：\"99新\"","patterns":[{"name":"型号","observation":"R五二","evidenceIds":["V1-01"]}] "sections":[{"title":"型号","purpose":"确认","evidenceIds":["V1-01"]}]}"#;

        let draft = parse_batch_model_draft(response).expect("quoted text must not be rewritten");

        assert_eq!(draft.title, "R五二，主播说：\"99新\"");
        assert_eq!(draft.patterns[0].observation, "R五二");
    }
}
