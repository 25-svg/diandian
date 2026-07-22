use crate::config::Config;
use crate::database::knowledge::KnowledgeDocumentRecord;
use crate::database::master_script::{MasterChunkInput, MasterChunkRow};
use crate::database::Database;
use crate::ffmpeg;
use crate::subtitle_generator::transcript_artifacts::{TranscriptArtifactStore, TranscriptSource};
use crate::subtitle_generator::volcengine::VolcengineAsr;
use async_trait::async_trait;
use master_ingest::{
    parameter_card_context, ArtifactBundle, ArtifactSink, Checkpoint, CheckpointStore, ChunkStatus,
    IngestRequest, IngestStatus, MasterIngestor, ParameterCard, SrtCue, Transcriber,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type AnalysisSource = IngestRequest;

pub async fn start_master_ingest<S, T, A>(
    source: AnalysisSource,
    store: S,
    transcriber: T,
    artifacts: A,
) -> Result<IngestStatus, String>
where
    S: CheckpointStore,
    T: Transcriber,
    A: ArtifactSink,
{
    MasterIngestor::new(store, transcriber, artifacts)
        .run(source)
        .await
}

pub async fn resume_master_ingest<S, T, A>(
    source_id: i64,
    mut source: AnalysisSource,
    store: S,
    transcriber: T,
    artifacts: A,
) -> Result<IngestStatus, String>
where
    S: CheckpointStore,
    T: Transcriber,
    A: ArtifactSink,
{
    source.source_id = source_id;
    start_master_ingest(source, store, transcriber, artifacts).await
}

#[derive(Clone)]
pub struct VolcengineChunkTranscriber {
    media_file: PathBuf,
    chunk_directory: PathBuf,
    client: VolcengineAsr,
}

impl VolcengineChunkTranscriber {
    pub fn configured(
        media_file: impl AsRef<Path>,
        chunk_directory: impl AsRef<Path>,
        config: &Config,
    ) -> Result<Self, String> {
        Ok(Self {
            media_file: media_file.as_ref().to_path_buf(),
            chunk_directory: chunk_directory.as_ref().to_path_buf(),
            client: VolcengineAsr::new(
                &config.volcengine_api_key,
                &config.volcengine_app_id,
                &config.volcengine_access_token,
                &config.volcengine_resource_id,
                &config.volcengine_boosting_table_id,
                &config.volcengine_correct_table_id,
            )?,
        })
    }
}

#[async_trait]
impl Transcriber for VolcengineChunkTranscriber {
    async fn transcribe(
        &self,
        _chunk_index: usize,
        start_ms: u64,
        end_ms: u64,
        cards: &[ParameterCard],
    ) -> Result<Vec<SrtCue>, String> {
        let audio_path = ffmpeg::extract_volcengine_audio_segment(
            &self.media_file,
            start_ms,
            end_ms,
            &self.chunk_directory,
        )
        .await?;
        let context = parameter_card_context(cards);
        let result = self
            .client
            .recognize_file(&audio_path, context.as_deref())
            .await?;
        result
            .subtitle_content
            .into_iter()
            .map(|item| {
                let start_ms = time_to_ms(&item.start_time);
                let end_ms = time_to_ms(&item.end_time);
                if end_ms <= start_ms {
                    return Err("Volcengine returned an invalid cue range".to_string());
                }
                Ok(SrtCue::new(start_ms, end_ms, item.text))
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct DatabaseCheckpointStore {
    database: Arc<Database>,
}

impl DatabaseCheckpointStore {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl CheckpointStore for DatabaseCheckpointStore {
    async fn list(&self, source_id: i64) -> Result<Vec<Checkpoint>, String> {
        self.database
            .list_master_chunks(source_id)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(checkpoint_from_row)
            .collect()
    }

    async fn persist(&self, checkpoint: Checkpoint) -> Result<Checkpoint, String> {
        let input = MasterChunkInput {
            source_id: checkpoint.source_id,
            chunk_index: i64::try_from(checkpoint.chunk_index)
                .map_err(|error| error.to_string())?,
            start_ms: i64::try_from(checkpoint.start_ms).map_err(|error| error.to_string())?,
            end_ms: i64::try_from(checkpoint.end_ms).map_err(|error| error.to_string())?,
            status: checkpoint.status.as_str().into(),
            input_hash: checkpoint.input_hash,
            raw_srt: checkpoint.raw_srt,
            reviewed_srt: checkpoint.reviewed_srt,
            error: checkpoint.error,
        };
        checkpoint_from_row(
            self.database
                .upsert_master_chunk(input)
                .await
                .map_err(|error| error.to_string())?,
        )
    }
}

#[derive(Clone)]
pub struct TranscriptArtifactSink {
    source: TranscriptSource,
    directory: PathBuf,
}

impl TranscriptArtifactSink {
    pub fn new(source: TranscriptSource, directory: impl AsRef<Path>) -> Self {
        Self {
            source,
            directory: directory.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl ArtifactSink for TranscriptArtifactSink {
    async fn persist(&self, artifacts: ArtifactBundle) -> Result<(), String> {
        let raw_srt = if TranscriptArtifactStore::canonical_artifacts_exist(&self.directory)
            .await
            .map_err(|error| error.to_string())?
        {
            let existing =
                TranscriptArtifactStore::load_from_dir(self.source.clone(), &self.directory)
                    .await
                    .map_err(|error| error.to_string())?;
            if existing.raw_srt.is_empty() {
                artifacts.raw_srt
            } else {
                existing.raw_srt
            }
        } else {
            artifacts.raw_srt
        };
        TranscriptArtifactStore::initialize(
            self.source.clone(),
            &self.directory,
            &raw_srt,
            &artifacts.corrected_srt,
            Vec::new(),
        )
        .await
        .map_err(|error| error.to_string())?;
        write_review_json(&self.directory, &artifacts.review_json).await
    }
}

pub fn parameter_card_from_record(record: KnowledgeDocumentRecord) -> Option<ParameterCard> {
    let metadata = serde_json::from_str::<Value>(&record.metadata_json).ok()?;
    let canonical_name = ["standard_name", "canonical_name", "product_name"]
        .into_iter()
        .find_map(|key| metadata.get(key).and_then(Value::as_str))
        .unwrap_or(record.title.as_str())
        .trim()
        .to_string();
    let aliases = ["aliases", "recognized_terms"]
        .into_iter()
        .filter_map(|key| metadata.get(key).and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if record.card_id.trim().is_empty()
        || record.version.trim().is_empty()
        || canonical_name.is_empty()
    {
        return None;
    }
    Some(ParameterCard {
        card_id: record.card_id,
        version: record.version,
        canonical_name,
        aliases,
        context: record.body,
    })
}

fn checkpoint_from_row(row: MasterChunkRow) -> Result<Checkpoint, String> {
    Ok(Checkpoint {
        source_id: row.source_id,
        chunk_index: usize::try_from(row.chunk_index).map_err(|error| error.to_string())?,
        start_ms: u64::try_from(row.start_ms).map_err(|error| error.to_string())?,
        end_ms: u64::try_from(row.end_ms).map_err(|error| error.to_string())?,
        status: match row.status.as_str() {
            "pending" => ChunkStatus::Pending,
            "running" => ChunkStatus::Running,
            "complete" => ChunkStatus::Complete,
            "failed" => ChunkStatus::Failed,
            value => return Err(format!("unknown master chunk status: {value}")),
        },
        input_hash: row.input_hash,
        raw_srt: row.raw_srt,
        reviewed_srt: row.reviewed_srt,
        error: row.error,
    })
}

async fn write_review_json(directory: &Path, content: &str) -> Result<(), String> {
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| error.to_string())?;
    let destination = directory.join("transcript.review.json");
    let temporary = directory.join(".transcript.review.json.tmp");
    tokio::fs::write(&temporary, content.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    if tokio::fs::try_exists(&destination)
        .await
        .map_err(|error| error.to_string())?
    {
        tokio::fs::remove_file(&destination)
            .await
            .map_err(|error| error.to_string())?;
    }
    tokio::fs::rename(temporary, destination)
        .await
        .map_err(|error| error.to_string())
}

fn time_to_ms(time: &srtparse::Time) -> u64 {
    (((time.hours * 60 + time.minutes) * 60 + time.seconds) * 1000) + time.milliseconds
}
