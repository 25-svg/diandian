use async_trait::async_trait;
use master_ingest::{
    chunk_input_hash, merge_chunk_cues, plan_asr_chunks, select_parameter_cards, ArtifactBundle,
    ArtifactSink, CanonicalArtifactDirectory, Checkpoint, CheckpointStore, ChunkStatus,
    IngestRequest, IngestStatus, MasterIngestor, ParameterCard, SrtCue, Transcriber,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

fn card(id: &str, version: &str, name: &str, aliases: &[&str]) -> ParameterCard {
    ParameterCard {
        card_id: id.into(),
        version: version.into(),
        canonical_name: name.into(),
        aliases: aliases.iter().map(|value| (*value).into()).collect(),
        context: format!("{name} parameters"),
    }
}

#[test]
fn plans_ten_minute_chunks_without_empty_tail() {
    assert_eq!(
        plan_asr_chunks(1_200_001, 600_000),
        vec![(0, 600_000), (600_000, 1_200_000), (1_200_000, 1_200_001)]
    );
    assert!(plan_asr_chunks(0, 600_000).is_empty());
}

#[test]
fn offsets_to_absolute_time_and_dedupes_overlapping_cues() {
    let merged = merge_chunk_cues(vec![
        (0, vec![SrtCue::new(599_000, 600_500, "shared cue")]),
        (
            600_000,
            vec![
                SrtCue::new(0, 500, " shared   cue "),
                SrtCue::new(500, 2_000, "next cue"),
            ],
        ),
    ]);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0], SrtCue::new(599_000, 600_500, "shared cue"));
    assert_eq!(merged[1], SrtCue::new(600_500, 602_000, "next cue"));
}

#[test]
fn parameter_cards_follow_all_ranking_and_validation_rules() {
    let mut available = vec![
        card("CARD-Z", "1", "manual lens", &[]),
        card("CARD-B", "1", "Canon R5", &["R5 Mark II"]),
        card("CARD-A", "2", "Canon R5", &[]),
        card("CARD-C", "1", "Sony telephoto lens", &[]),
        card("CARD-D", "1", "Sony portrait lens", &[]),
        card("CARD-A", "1", "stale duplicate", &[]),
        card("", "1", "malformed", &[]),
        card("BAD-VERSION", "", "malformed", &[]),
        card("BAD-NAME", "1", "", &[]),
    ];
    for index in 0..40 {
        available.push(card(
            &format!("OVERFLOW-{index:02}"),
            "1",
            &format!("overflow token {index}"),
            &[],
        ));
    }

    let selected = select_parameter_cards(
        "R5 Mark II Sony telephoto lens overflow token",
        &["Canon R5".into()],
        &["CARD-Z".into(), "CARD-Z".into()],
        &available,
        30,
    );
    let ids = selected
        .cards
        .iter()
        .map(|item| item.card_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        &ids[..5],
        &["CARD-Z", "CARD-A", "CARD-B", "CARD-C", "CARD-D"]
    );
    assert_eq!(selected.cards[1].version, "2");
    assert_eq!(ids.len(), 30);
    assert_eq!(ids.iter().filter(|id| **id == "CARD-A").count(), 1);
    assert!(!ids.iter().any(|id| id.starts_with("BAD") || id.is_empty()));
}

#[test]
fn input_hash_covers_source_bounds_and_selected_card_versions() {
    let cards = vec![card("CARD-1", "1", "one", &[])];
    let base = chunk_input_hash("video:7", "source-hash", 0, 600_000, &cards);
    assert_ne!(
        base,
        chunk_input_hash("video:8", "source-hash", 0, 600_000, &cards)
    );
    assert_ne!(
        base,
        chunk_input_hash("video:7", "source-hash", 1, 600_000, &cards)
    );
    assert_ne!(
        base,
        chunk_input_hash("video:7", "source-hash", 0, 599_999, &cards)
    );
    assert_ne!(
        base,
        chunk_input_hash(
            "video:7",
            "source-hash",
            0,
            600_000,
            &[card("CARD-1", "2", "one", &[])],
        )
    );
}

#[tokio::test]
async fn canonical_artifacts_never_overwrite_raw_on_retry() {
    let dir = tempfile::tempdir().unwrap();
    let sink = CanonicalArtifactDirectory::new(dir.path());
    sink.persist(ArtifactBundle {
        raw_srt: "raw-v1".into(),
        corrected_srt: "corrected-v1".into(),
        review_json: "[]".into(),
    })
    .await
    .unwrap();
    sink.persist(ArtifactBundle {
        raw_srt: "raw-v2".into(),
        corrected_srt: "corrected-v2".into(),
        review_json: "[{\"id\":1}]".into(),
    })
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(dir.path().join("subtitle.raw.srt"))
            .await
            .unwrap(),
        "raw-v1"
    );
    assert_eq!(
        tokio::fs::read_to_string(dir.path().join("subtitle.corrected.srt"))
            .await
            .unwrap(),
        "corrected-v2"
    );
    assert_eq!(
        tokio::fs::read_to_string(dir.path().join("transcript.review.json"))
            .await
            .unwrap(),
        "[{\"id\":1}]"
    );
}

#[derive(Default, Clone)]
struct MemoryStore {
    rows: Arc<Mutex<BTreeMap<usize, Checkpoint>>>,
    transitions: Arc<Mutex<Vec<(usize, ChunkStatus)>>>,
}

#[async_trait]
impl CheckpointStore for MemoryStore {
    async fn list(&self, _source_id: i64) -> Result<Vec<Checkpoint>, String> {
        Ok(self.rows.lock().unwrap().values().cloned().collect())
    }

    async fn persist(&self, checkpoint: Checkpoint) -> Result<Checkpoint, String> {
        self.transitions
            .lock()
            .unwrap()
            .push((checkpoint.chunk_index, checkpoint.status.clone()));
        self.rows
            .lock()
            .unwrap()
            .insert(checkpoint.chunk_index, checkpoint.clone());
        Ok(checkpoint)
    }
}

#[derive(Clone)]
struct FakeTranscriber {
    outcomes: SharedOutcomes,
    calls: Arc<Mutex<Vec<usize>>>,
}

type SharedOutcomes = Arc<Mutex<VecDeque<Result<Vec<SrtCue>, String>>>>;

#[async_trait]
impl Transcriber for FakeTranscriber {
    async fn transcribe(
        &self,
        chunk_index: usize,
        _start_ms: u64,
        _end_ms: u64,
        _cards: &[ParameterCard],
    ) -> Result<Vec<SrtCue>, String> {
        self.calls.lock().unwrap().push(chunk_index);
        self.outcomes.lock().unwrap().pop_front().unwrap()
    }
}

#[derive(Default, Clone)]
struct MemoryArtifacts {
    writes: Arc<Mutex<Vec<ArtifactBundle>>>,
}

#[async_trait]
impl ArtifactSink for MemoryArtifacts {
    async fn persist(&self, artifacts: ArtifactBundle) -> Result<(), String> {
        let mut writes = self.writes.lock().unwrap();
        if let Some(first) = writes.first() {
            assert_eq!(
                first.raw_srt, artifacts.raw_srt,
                "raw must stay immutable on retry"
            );
        }
        writes.push(artifacts);
        Ok(())
    }
}

#[tokio::test]
async fn failed_second_chunk_resumes_only_remaining_chunks_and_preserves_first() {
    let store = MemoryStore::default();
    let artifacts = MemoryArtifacts::default();
    let transcriber = FakeTranscriber {
        outcomes: Arc::new(Mutex::new(VecDeque::from([
            Ok(vec![SrtCue::new(0, 1_000, "chunk one")]),
            Err("chunk two failed".into()),
            Ok(vec![SrtCue::new(0, 1_000, "chunk two")]),
            Ok(vec![SrtCue::new(0, 1_000, "chunk three")]),
        ]))),
        calls: Arc::new(Mutex::new(Vec::new())),
    };
    let ingestor = MasterIngestor::new(store.clone(), transcriber.clone(), artifacts.clone());
    let request = IngestRequest {
        source_id: 7,
        source_key: "video:7".into(),
        source_hash: "source-hash".into(),
        duration_ms: 1_800_000,
        chunk_duration_ms: 600_000,
        selected_cards: vec![card("CARD-1", "3", "Canon R5", &[])],
    };

    let failed = ingestor.run(request.clone()).await.unwrap();
    assert!(matches!(
        failed,
        IngestStatus::Failed {
            failed_chunk: 1,
            ..
        }
    ));
    let first_before = store.rows.lock().unwrap().get(&0).unwrap().clone();
    assert_eq!(first_before.status, ChunkStatus::Complete);
    assert_eq!(
        store.rows.lock().unwrap().get(&1).unwrap().status,
        ChunkStatus::Failed
    );

    let complete = ingestor.run(request.clone()).await.unwrap();
    assert_eq!(
        complete,
        IngestStatus::Complete {
            completed: 3,
            total: 3
        }
    );
    assert_eq!(*transcriber.calls.lock().unwrap(), vec![0, 1, 1, 2]);
    assert_eq!(store.rows.lock().unwrap().get(&0).unwrap(), &first_before);
    assert!(store
        .transitions
        .lock()
        .unwrap()
        .windows(2)
        .any(|pair| pair[0] == (1, ChunkStatus::Running) && pair[1] == (1, ChunkStatus::Failed)));

    let calls_before_retry = transcriber.calls.lock().unwrap().len();
    let complete_again = ingestor.run(request).await.unwrap();
    assert_eq!(
        complete_again,
        IngestStatus::Complete {
            completed: 3,
            total: 3
        }
    );
    assert_eq!(transcriber.calls.lock().unwrap().len(), calls_before_retry);
    let writes = artifacts.writes.lock().unwrap();
    assert_eq!(writes.len(), 2);
    assert!(writes[0].raw_srt.contains("00:10:00,000"));
    assert_eq!(writes[0].raw_srt, writes[0].corrected_srt);
    assert_eq!(writes[0].review_json, "[]");
}
