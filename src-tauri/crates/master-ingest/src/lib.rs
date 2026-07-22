use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MAX_PARAMETER_CARDS: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterCard {
    pub card_id: String,
    pub version: String,
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedParameterCards {
    pub cards: Vec<ParameterCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SrtCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

impl SrtCue {
    pub fn new(start_ms: u64, end_ms: u64, text: impl Into<String>) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkStatus {
    Pending,
    Running,
    Complete,
    Failed,
}

impl ChunkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub source_id: i64,
    pub chunk_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub status: ChunkStatus,
    pub input_hash: String,
    pub raw_srt: String,
    pub reviewed_srt: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestRequest {
    pub source_id: i64,
    pub source_key: String,
    pub source_hash: String,
    pub duration_ms: u64,
    pub chunk_duration_ms: u64,
    pub selected_cards: Vec<ParameterCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IngestStatus {
    Complete {
        completed: usize,
        total: usize,
    },
    Failed {
        completed: usize,
        total: usize,
        failed_chunk: usize,
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactBundle {
    pub raw_srt: String,
    pub corrected_srt: String,
    pub review_json: String,
}

#[async_trait]
pub trait CheckpointStore: Send + Sync {
    async fn list(&self, source_id: i64) -> Result<Vec<Checkpoint>, String>;
    async fn persist(&self, checkpoint: Checkpoint) -> Result<Checkpoint, String>;
}

#[async_trait]
pub trait Transcriber: Send + Sync {
    async fn transcribe(
        &self,
        chunk_index: usize,
        start_ms: u64,
        end_ms: u64,
        cards: &[ParameterCard],
    ) -> Result<Vec<SrtCue>, String>;
}

#[async_trait]
pub trait ArtifactSink: Send + Sync {
    async fn persist(&self, artifacts: ArtifactBundle) -> Result<(), String>;
}

pub struct MasterIngestor<S, T, A> {
    store: S,
    transcriber: T,
    artifacts: A,
}

impl<S, T, A> MasterIngestor<S, T, A>
where
    S: CheckpointStore,
    T: Transcriber,
    A: ArtifactSink,
{
    pub fn new(store: S, transcriber: T, artifacts: A) -> Self {
        Self {
            store,
            transcriber,
            artifacts,
        }
    }

    pub async fn run(&self, request: IngestRequest) -> Result<IngestStatus, String> {
        let plan = plan_asr_chunks(request.duration_ms, request.chunk_duration_ms);
        let total = plan.len();
        let mut checkpoints = self
            .store
            .list(request.source_id)
            .await?
            .into_iter()
            .map(|row| (row.chunk_index, row))
            .collect::<BTreeMap<_, _>>();

        for (chunk_index, (start_ms, end_ms)) in plan.iter().copied().enumerate() {
            let input_hash = chunk_input_hash(
                &request.source_key,
                &request.source_hash,
                start_ms,
                end_ms,
                &request.selected_cards,
            );
            let can_skip = checkpoints.get(&chunk_index).is_some_and(|row| {
                row.status == ChunkStatus::Complete
                    && row.input_hash == input_hash
                    && row.start_ms == start_ms
                    && row.end_ms == end_ms
            });
            if can_skip {
                continue;
            }

            let running = Checkpoint {
                source_id: request.source_id,
                chunk_index,
                start_ms,
                end_ms,
                status: ChunkStatus::Running,
                input_hash: input_hash.clone(),
                raw_srt: String::new(),
                reviewed_srt: String::new(),
                error: None,
            };
            let running = self.store.persist(running).await?;
            checkpoints.insert(chunk_index, running.clone());

            match self
                .transcriber
                .transcribe(chunk_index, start_ms, end_ms, &request.selected_cards)
                .await
            {
                Ok(cues) => {
                    let srt = render_srt(&cues);
                    let complete = Checkpoint {
                        status: ChunkStatus::Complete,
                        raw_srt: srt.clone(),
                        reviewed_srt: srt,
                        ..running
                    };
                    let complete = self.store.persist(complete).await?;
                    checkpoints.insert(chunk_index, complete);
                }
                Err(error) => {
                    let failed = Checkpoint {
                        status: ChunkStatus::Failed,
                        error: Some(error.clone()),
                        ..running
                    };
                    let failed = self.store.persist(failed).await?;
                    checkpoints.insert(chunk_index, failed);
                    return Ok(IngestStatus::Failed {
                        completed: matching_complete_count(&checkpoints, &request, &plan),
                        total,
                        failed_chunk: chunk_index,
                        error,
                    });
                }
            }
        }

        let raw_chunks = collect_chunk_cues(&checkpoints, &plan, false)?;
        let reviewed_chunks = collect_chunk_cues(&checkpoints, &plan, true)?;
        let raw_srt = render_srt(&merge_chunk_cues(raw_chunks));
        let corrected_srt = render_srt(&merge_chunk_cues(reviewed_chunks));
        self.artifacts
            .persist(ArtifactBundle {
                raw_srt,
                corrected_srt,
                review_json: "[]".into(),
            })
            .await?;

        Ok(IngestStatus::Complete {
            completed: total,
            total,
        })
    }
}

pub fn plan_asr_chunks(duration_ms: u64, chunk_duration_ms: u64) -> Vec<(u64, u64)> {
    if duration_ms == 0 || chunk_duration_ms == 0 {
        return Vec::new();
    }
    (0..duration_ms)
        .step_by(chunk_duration_ms.min(usize::MAX as u64) as usize)
        .map(|start| {
            (
                start,
                start.saturating_add(chunk_duration_ms).min(duration_ms),
            )
        })
        .collect()
}

pub fn select_parameter_cards(
    title: &str,
    recognized_terms: &[String],
    manually_selected_ids: &[String],
    available_cards: &[ParameterCard],
    limit: usize,
) -> SelectedParameterCards {
    let mut by_id = HashMap::<String, ParameterCard>::new();
    for card in available_cards.iter().filter(|card| valid_card(card)) {
        match by_id.get(&card.card_id) {
            Some(current)
                if compare_versions(&current.version, &card.version) != Ordering::Less => {}
            _ => {
                by_id.insert(card.card_id.clone(), card.clone());
            }
        }
    }

    let manual_positions = manually_selected_ids.iter().enumerate().fold(
        HashMap::new(),
        |mut positions, (index, id)| {
            positions.entry(id.as_str()).or_insert(index);
            positions
        },
    );
    let normalized_title = normalize_phrase(title);
    let normalized_terms = recognized_terms
        .iter()
        .map(|term| normalize_phrase(term))
        .collect::<Vec<_>>();
    let haystack_tokens = tokenize(
        &std::iter::once(title)
            .chain(recognized_terms.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" "),
    );

    let mut ranked = by_id.into_values().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        let left_rank = card_rank(
            left,
            manual_positions.get(left.card_id.as_str()).copied(),
            &normalized_title,
            &normalized_terms,
            &haystack_tokens,
        );
        let right_rank = card_rank(
            right,
            manual_positions.get(right.card_id.as_str()).copied(),
            &normalized_title,
            &normalized_terms,
            &haystack_tokens,
        );
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.card_id.cmp(&right.card_id))
    });
    ranked.truncate(limit.min(MAX_PARAMETER_CARDS));
    SelectedParameterCards { cards: ranked }
}

pub fn chunk_input_hash(
    source_key: &str,
    source_hash: &str,
    start_ms: u64,
    end_ms: u64,
    selected_cards: &[ParameterCard],
) -> String {
    let mut cards = selected_cards
        .iter()
        .map(|card| format!("{}@{}", card.card_id, card.version))
        .collect::<Vec<_>>();
    cards.sort();
    cards.dedup();
    let mut hasher = Sha256::new();
    for part in [
        source_key.to_string(),
        source_hash.to_string(),
        start_ms.to_string(),
        end_ms.to_string(),
        cards.join("\n"),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub fn parameter_card_context(selected_cards: &[ParameterCard]) -> Option<String> {
    if selected_cards.is_empty() {
        return None;
    }
    let mut cards = selected_cards.iter().collect::<Vec<_>>();
    cards.sort_by(|left, right| left.card_id.cmp(&right.card_id));
    cards.truncate(MAX_PARAMETER_CARDS);
    Some(
        serde_json::json!({
            "context_type": "dialog_ctx",
            "parameter_cards": cards.into_iter().map(|card| serde_json::json!({
                "card_id": card.card_id,
                "version": card.version,
                "canonical_name": card.canonical_name,
                "aliases": card.aliases,
                "context": card.context,
            })).collect::<Vec<_>>(),
        })
        .to_string(),
    )
}

pub fn merge_chunk_cues(chunks: Vec<(u64, Vec<SrtCue>)>) -> Vec<SrtCue> {
    let mut merged = Vec::<SrtCue>::new();
    for (offset_ms, cues) in chunks {
        for cue in cues {
            let absolute = SrtCue {
                start_ms: cue.start_ms.saturating_add(offset_ms),
                end_ms: cue.end_ms.saturating_add(offset_ms),
                text: cue.text.trim().to_string(),
            };
            let duplicate = merged.iter().rev().any(|current| {
                current.start_ms < absolute.end_ms
                    && absolute.start_ms < current.end_ms
                    && normalize_phrase(&current.text) == normalize_phrase(&absolute.text)
            });
            if !duplicate {
                merged.push(absolute);
            }
        }
    }
    merged.sort_by_key(|cue| (cue.start_ms, cue.end_ms));
    merged
}

pub fn render_srt(cues: &[SrtCue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(index, cue)| {
            format!(
                "{}\n{} --> {}\n{}\n\n",
                index + 1,
                format_timestamp(cue.start_ms),
                format_timestamp(cue.end_ms),
                cue.text.trim()
            )
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct CanonicalArtifactDirectory {
    directory: PathBuf,
}

impl CanonicalArtifactDirectory {
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl ArtifactSink for CanonicalArtifactDirectory {
    async fn persist(&self, artifacts: ArtifactBundle) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.directory)
            .await
            .map_err(|error| error.to_string())?;
        let raw_path = self.directory.join("subtitle.raw.srt");
        if !tokio::fs::try_exists(&raw_path)
            .await
            .map_err(|error| error.to_string())?
        {
            write_new(&raw_path, artifacts.raw_srt.as_bytes()).await?;
        }
        write_replace(
            &self.directory.join("subtitle.corrected.srt"),
            artifacts.corrected_srt.as_bytes(),
        )
        .await?;
        write_replace(
            &self.directory.join("transcript.review.json"),
            artifacts.review_json.as_bytes(),
        )
        .await
    }
}

fn valid_card(card: &ParameterCard) -> bool {
    !card.card_id.trim().is_empty()
        && !card.version.trim().is_empty()
        && !card.canonical_name.trim().is_empty()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let parts = |value: &str| {
        value
            .split(['.', '-'])
            .map(|part| part.parse::<u64>().ok())
            .collect::<Vec<_>>()
    };
    let left_parts = parts(left);
    let right_parts = parts(right);
    left_parts
        .iter()
        .zip(&right_parts)
        .find_map(|(left, right)| (left != right).then(|| left.cmp(right)))
        .unwrap_or_else(|| {
            left_parts
                .len()
                .cmp(&right_parts.len())
                .then_with(|| left.cmp(right))
        })
}

fn card_rank(
    card: &ParameterCard,
    manual_position: Option<usize>,
    title: &str,
    terms: &[String],
    haystack_tokens: &HashSet<String>,
) -> (u8, usize, usize) {
    if let Some(position) = manual_position {
        return (0, position, 0);
    }
    let names = std::iter::once(&card.canonical_name).chain(card.aliases.iter());
    let normalized_names = names
        .map(|name| normalize_phrase(name))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    let exact = normalized_names.iter().any(|name| {
        phrase_matches(title, name) || terms.iter().any(|term| phrase_matches(term, name))
    });
    if exact {
        return (1, 0, 0);
    }
    let overlap = normalized_names
        .iter()
        .flat_map(|name| tokenize(name))
        .filter(|token| haystack_tokens.contains(token))
        .collect::<HashSet<_>>()
        .len();
    (2, usize::MAX - overlap, 0)
}

fn normalize_phrase(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn phrase_matches(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    if needle.chars().any(is_cjk) {
        return haystack.contains(needle);
    }
    let haystack_tokens = haystack.split_whitespace().collect::<Vec<_>>();
    let needle_tokens = needle.split_whitespace().collect::<Vec<_>>();
    !needle_tokens.is_empty()
        && haystack_tokens
            .windows(needle_tokens.len())
            .any(|window| window == needle_tokens)
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

fn tokenize(value: &str) -> HashSet<String> {
    normalize_phrase(value)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn matching_complete_count(
    checkpoints: &BTreeMap<usize, Checkpoint>,
    request: &IngestRequest,
    plan: &[(u64, u64)],
) -> usize {
    plan.iter()
        .enumerate()
        .filter(|(index, (start_ms, end_ms))| {
            checkpoints.get(index).is_some_and(|row| {
                row.status == ChunkStatus::Complete
                    && row.input_hash
                        == chunk_input_hash(
                            &request.source_key,
                            &request.source_hash,
                            *start_ms,
                            *end_ms,
                            &request.selected_cards,
                        )
            })
        })
        .count()
}

fn collect_chunk_cues(
    checkpoints: &BTreeMap<usize, Checkpoint>,
    plan: &[(u64, u64)],
    reviewed: bool,
) -> Result<Vec<(u64, Vec<SrtCue>)>, String> {
    plan.iter()
        .enumerate()
        .map(|(index, (start_ms, _))| {
            let row = checkpoints
                .get(&index)
                .filter(|row| row.status == ChunkStatus::Complete)
                .ok_or_else(|| format!("chunk {index} is not complete"))?;
            let value = if reviewed {
                &row.reviewed_srt
            } else {
                &row.raw_srt
            };
            let cues = parse_srt(value)
                .map_err(|error| format!("checkpoint recovery error for chunk {index}: {error}"))?;
            Ok((*start_ms, cues))
        })
        .collect()
}

fn parse_srt(value: &str) -> Result<Vec<SrtCue>, String> {
    let normalized = value.replace("\r\n", "\n");
    normalized
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let mut lines = block.lines();
            let position = lines
                .next()
                .ok_or("missing cue position")?
                .trim()
                .parse::<u64>()
                .map_err(|_| "invalid cue position")?;
            if position == 0 {
                return Err("invalid cue position".to_string());
            }
            let times = lines.next().ok_or("missing cue timestamps")?;
            let (start, end) = times.split_once(" --> ").ok_or("invalid cue timestamps")?;
            let start_ms = parse_timestamp(start)?;
            let end_ms = parse_timestamp(end)?;
            if end_ms <= start_ms {
                return Err("cue end must be after start".to_string());
            }
            Ok(SrtCue::new(
                start_ms,
                end_ms,
                lines.collect::<Vec<_>>().join("\n"),
            ))
        })
        .collect()
}

fn parse_timestamp(value: &str) -> Result<u64, String> {
    let (time, milliseconds) = value
        .trim()
        .split_once(',')
        .ok_or_else(|| format!("invalid SRT timestamp: {value}"))?;
    let mut parts = time.split(':');
    let hours = parts.next().and_then(|part| part.parse::<u64>().ok());
    let minutes = parts.next().and_then(|part| part.parse::<u64>().ok());
    let seconds = parts.next().and_then(|part| part.parse::<u64>().ok());
    let milliseconds = milliseconds.parse::<u64>().ok();
    match (hours, minutes, seconds, milliseconds, parts.next()) {
        (Some(hours), Some(minutes), Some(seconds), Some(milliseconds), None)
            if minutes <= 59 && seconds <= 59 && milliseconds <= 999 =>
        {
            Ok(hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + milliseconds)
        }
        _ => Err(format!("invalid SRT timestamp: {value}")),
    }
}

fn format_timestamp(total_ms: u64) -> String {
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms % 3_600_000) / 60_000;
    let seconds = (total_ms % 60_000) / 1_000;
    let milliseconds = total_ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}")
}

async fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(path)
        .await
        .map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .await
        .map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())
}

async fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("artifact")
    ));
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| error.to_string())?;
    if tokio::fs::try_exists(path)
        .await
        .map_err(|error| error.to_string())?
    {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| error.to_string())?;
    }
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|error| error.to_string())
}
