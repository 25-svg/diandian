use crate::database::master_script::NewMasterSource;
use crate::handlers::video::resolve_video_transcript_context;
use crate::master_script::{
    parameter_card_from_record, resume_master_ingest as run_resume_master_ingest,
    start_master_ingest as run_start_master_ingest, DatabaseCheckpointStore,
    TranscriptArtifactSink, VolcengineChunkTranscriber,
};
use crate::state::State;
use crate::state_type;
use master_ingest::{
    select_parameter_cards, validate_resume_source, IngestRequest, IngestStatus, SourceIdentity,
    MAX_PARAMETER_CARDS,
};
use serde::Deserialize;
use std::time::UNIX_EPOCH;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterVideoIngestRequest {
    pub video_id: i64,
    pub title: String,
    #[serde(default)]
    pub recognized_terms: Vec<String>,
    #[serde(default)]
    pub manually_selected_card_ids: Vec<String>,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn start_master_ingest(
    state: state_type!(),
    request: MasterVideoIngestRequest,
) -> Result<IngestStatus, String> {
    let prepared = prepare_video_ingest(&state, &request).await?;
    let source = state
        .db
        .create_master_source(NewMasterSource {
            source_kind: "video".into(),
            source_key: prepared.source_key.clone(),
            media_path: prepared.media_path.clone(),
            media_hash: prepared.media_hash.clone(),
            duration_ms: i64::try_from(prepared.duration_ms).map_err(|error| error.to_string())?,
        })
        .await
        .map_err(String::from)?;
    let ingest_request = prepared.request(source.id);
    run_start_master_ingest(
        ingest_request,
        DatabaseCheckpointStore::new(state.db.clone()),
        prepared.transcriber,
        prepared.artifacts,
    )
    .await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resume_master_ingest(
    state: state_type!(),
    source_id: i64,
    request: MasterVideoIngestRequest,
) -> Result<IngestStatus, String> {
    let prepared = prepare_video_ingest(&state, &request).await?;
    let persisted = state
        .db
        .get_master_source(source_id)
        .await
        .map_err(String::from)?;
    if persisted.source_kind != "video" {
        return Err("续跑来源不是视频母稿".into());
    }
    let persisted_identity = SourceIdentity {
        source_key: persisted.source_key,
        media_path: persisted.media_path,
        media_hash: persisted.media_hash,
        duration_ms: u64::try_from(persisted.duration_ms).map_err(|error| error.to_string())?,
    };
    validate_resume_source(&persisted_identity, &prepared.identity())?;
    let ingest_request = prepared.request(source_id);
    run_resume_master_ingest(
        source_id,
        ingest_request,
        DatabaseCheckpointStore::new(state.db.clone()),
        prepared.transcriber,
        prepared.artifacts,
    )
    .await
}

struct PreparedVideoIngest {
    source_key: String,
    media_path: String,
    media_hash: String,
    duration_ms: u64,
    selected_cards: Vec<master_ingest::ParameterCard>,
    transcriber: VolcengineChunkTranscriber,
    artifacts: TranscriptArtifactSink,
}

impl PreparedVideoIngest {
    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            source_key: self.source_key.clone(),
            media_path: self.media_path.clone(),
            media_hash: self.media_hash.clone(),
            duration_ms: self.duration_ms,
        }
    }

    fn request(&self, source_id: i64) -> IngestRequest {
        IngestRequest {
            source_id,
            source_key: self.source_key.clone(),
            source_hash: self.media_hash.clone(),
            duration_ms: self.duration_ms,
            chunk_duration_ms: 600_000,
            selected_cards: self.selected_cards.clone(),
        }
    }
}

async fn prepare_video_ingest(
    state: &State,
    request: &MasterVideoIngestRequest,
) -> Result<PreparedVideoIngest, String> {
    let context = resolve_video_transcript_context(state, request.video_id).await?;
    let video = state.db.get_video(context.video_id).await?;
    let duration_ms = if video.length > 0 {
        u64::try_from(video.length)
            .map_err(|error| error.to_string())?
            .saturating_mul(1000)
    } else {
        crate::ffmpeg::probe_media_duration_ms(&context.media_file).await?
    };
    let metadata = tokio::fs::metadata(&context.media_file)
        .await
        .map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let source_key = format!("video:{}", context.video_id);
    let media_hash = format!(
        "{:x}",
        md5::compute(format!(
            "{}\n{}\n{}\n{}",
            source_key,
            metadata.len(),
            modified,
            duration_ms
        ))
    );
    let available_cards = state
        .db
        .list_asr_parameter_cards()
        .await
        .map_err(String::from)?
        .into_iter()
        .filter_map(parameter_card_from_record)
        .collect::<Vec<_>>();
    let selected_cards = select_parameter_cards(
        &request.title,
        &request.recognized_terms,
        &request.manually_selected_card_ids,
        &available_cards,
        MAX_PARAMETER_CARDS,
    )
    .cards;
    let config = state.config.read().await;
    let transcriber = VolcengineChunkTranscriber::configured(
        &context.media_file,
        context.artifact_dir.join("volcengine-master-chunks"),
        &config,
    )?;
    drop(config);
    let artifacts = TranscriptArtifactSink::new(context.source, &context.artifact_dir);

    Ok(PreparedVideoIngest {
        source_key,
        media_path: context.media_file.to_string_lossy().to_string(),
        media_hash,
        duration_ms,
        selected_cards,
        transcriber,
        artifacts,
    })
}
