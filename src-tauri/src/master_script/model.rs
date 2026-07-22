use crate::handlers::ai::request_minimax_text;
use master_builder::{MasterDraft, ParameterFactCard, TranscriptCue};
use serde_json::json;

const BLOCK_SIZE: usize = 80;
const CONTEXT_SIZE: usize = 5;

pub async fn generate_master_draft(
    api_key: &str,
    title: &str,
    transcript: &[TranscriptCue],
    parameter_cards: &[ParameterFactCard],
) -> Result<MasterDraft, String> {
    if transcript.is_empty() {
        return Err("规范逐字稿为空，无法生成母稿".into());
    }
    let mut sections = Vec::new();
    for core_start in (0..transcript.len()).step_by(BLOCK_SIZE) {
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
        sections.extend(master_builder::parse_model_sections(&response)?);
    }
    for (index, section) in sections.iter_mut().enumerate() {
        section.position = u32::try_from(index + 1).map_err(|error| error.to_string())?;
    }
    Ok(MasterDraft {
        title: title.to_string(),
        sections,
    })
}
