use crate::handlers::ai::request_minimax_text;
use futures::{stream, StreamExt, TryStreamExt};
use master_builder::{MasterDraft, ParameterFactCard, TranscriptCue};
use serde_json::json;

// Keep model output comfortably below MiniMax's response token limit.
const BLOCK_SIZE: usize = 32;
const CONTEXT_SIZE: usize = 5;
const MAX_CONCURRENT_BLOCKS: usize = 4;

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
    for (index, section) in sections.iter_mut().enumerate() {
        section.position = u32::try_from(index + 1).map_err(|error| error.to_string())?;
    }
    Ok(MasterDraft {
        title: title.to_string(),
        sections,
    })
}
