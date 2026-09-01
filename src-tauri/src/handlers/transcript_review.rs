use crate::database::transcript_dictionary_candidate::{
    export_transcript_dictionary_candidates_csv, export_transcript_dictionary_candidates_json,
    CandidateEvidence, CandidateEvidenceKind, CandidateSourceKind, CandidateSourceMetadata,
    NewTranscriptDictionaryCandidate, TranscriptDictionaryCandidateRow,
    TranscriptDictionaryCandidateType,
};
use crate::database::Database;
use crate::state::State;
use crate::state_type;
use crate::subtitle_generator::transcript_artifacts::{
    ReviewAction, ReviewDecision, TranscriptArtifactStore, TranscriptAuditBundle,
    TranscriptCorrection, TranscriptSource,
};
use recorder::platforms::PlatformType;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::future::Future;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveTranscriptCorrectionRequest {
    pub source: TranscriptSource,
    pub correction_id: String,
    pub action: ReviewAction,
    pub decided_text: Option<String>,
    pub add_to_dictionary_candidates: bool,
}

#[derive(Debug, Serialize)]
pub struct LegacyTranscriptAuditBundle {
    pub raw_srt: String,
    pub corrected_srt: String,
    pub changes_json: String,
    pub review_json: String,
    pub evidence_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateExportFormat {
    Json,
    Csv,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_transcript_audit(
    state: state_type!(),
    source: TranscriptSource,
) -> Result<TranscriptAuditBundle, String> {
    load_transcript_audit(&state, source).await
}

pub(crate) async fn load_transcript_audit(
    state: &State,
    source: TranscriptSource,
) -> Result<TranscriptAuditBundle, String> {
    let (source, dir) = resolve_transcript_source(&state, &source).await?;
    TranscriptArtifactStore::load_from_dir(source, dir)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn load_legacy_archive_transcript_audit(
    state: &State,
    source: TranscriptSource,
) -> Result<LegacyTranscriptAuditBundle, String> {
    let (source, dir) = resolve_transcript_source(state, &source).await?;
    load_legacy_transcript_audit_from_dir(source, dir).await
}

pub(crate) async fn resolve_legacy_archive_review_item(
    state: &State,
    source: TranscriptSource,
    index: usize,
    correction: String,
) -> Result<LegacyTranscriptAuditBundle, String> {
    let (source, dir) = resolve_transcript_source(state, &source).await?;
    resolve_legacy_review_item_from_dir(source, dir, index, correction).await
}

pub(crate) async fn load_legacy_transcript_audit_from_dir(
    source: TranscriptSource,
    dir: impl AsRef<Path>,
) -> Result<LegacyTranscriptAuditBundle, String> {
    let dir = dir.as_ref().to_path_buf();
    let (bundle, review_items) = load_or_migrate_legacy_review_items(source, &dir)
        .await
        .map_err(|error| error.to_string())?;
    legacy_audit_bundle(&dir, &bundle, review_items)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn resolve_legacy_review_item_from_dir(
    source: TranscriptSource,
    dir: impl AsRef<Path>,
    index: usize,
    correction: String,
) -> Result<LegacyTranscriptAuditBundle, String> {
    let dir = dir.as_ref().to_path_buf();
    let (_, review_items) = load_or_migrate_legacy_review_items(source.clone(), &dir)
        .await
        .map_err(|error| error.to_string())?;
    let correction_id = legacy_review_correction_id_at(&review_items, index)?;
    let bundle = TranscriptArtifactStore::resolve(
        source,
        &dir,
        &correction_id,
        ReviewAction::Approve,
        Some(correction),
    )
    .await
    .map_err(|error| error.to_string())?;
    let mut review_items = review_items;
    project_canonical_decisions(&mut review_items, &bundle.corrections);
    legacy_audit_bundle(&dir, &bundle, review_items)
        .await
        .map_err(|error| error.to_string())
}

fn legacy_review_correction_id_at(review_items: &[Value], index: usize) -> Result<String, String> {
    review_items
        .get(index)
        .and_then(|item| item.get("correction_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "review item does not exist; refresh and retry".to_string())
}

async fn load_or_migrate_legacy_review_items(
    source: TranscriptSource,
    dir: &Path,
) -> io::Result<(TranscriptAuditBundle, Vec<Value>)> {
    let initial_bundle = TranscriptArtifactStore::load_from_dir(source.clone(), dir).await?;
    let (mut review_items, review_exists) = read_legacy_review_items(dir).await?;
    if !review_exists && !initial_bundle.corrections.is_empty() {
        review_items = initial_bundle
            .corrections
            .iter()
            .map(legacy_review_item_from_correction)
            .collect();
    }

    let mut known_ids = initial_bundle
        .corrections
        .iter()
        .map(|correction| correction.id.clone())
        .collect::<HashSet<_>>();
    let mut assigned_ids = HashSet::new();
    let mut added_corrections = Vec::new();
    for (index, item) in review_items.iter_mut().enumerate() {
        let correction_id =
            legacy_review_item_id(item, index, &initial_bundle.corrections, &assigned_ids)?;
        assigned_ids.insert(correction_id.clone());
        if !known_ids.contains(&correction_id) {
            added_corrections.push(legacy_review_item_correction(item, correction_id.clone())?);
            known_ids.insert(correction_id);
        }
    }

    let bundle =
        TranscriptArtifactStore::ensure_legacy_corrections(source, dir, added_corrections).await?;
    project_canonical_decisions(&mut review_items, &bundle.corrections);

    Ok((bundle, review_items))
}

async fn read_legacy_review_items(dir: &Path) -> io::Result<(Vec<Value>, bool)> {
    match tokio::fs::read_to_string(dir.join("transcript.review.json")).await {
        Ok(value) => serde_json::from_str(&value)
            .map(|items| (items, true))
            .map_err(invalid_legacy_review_json),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((Vec::new(), false)),
        Err(error) => Err(error),
    }
}

fn legacy_review_item_id(
    item: &mut Value,
    index: usize,
    corrections: &[TranscriptCorrection],
    assigned_ids: &HashSet<String>,
) -> io::Result<String> {
    if let Some(id) = item
        .get("correction_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return Ok(id.to_string());
    }
    let id = corrections
        .iter()
        .find(|correction| {
            !assigned_ids.contains(&correction.id)
                && legacy_item_matches_correction(item, correction)
        })
        .map(|correction| correction.id.clone())
        .unwrap_or_else(|| stable_legacy_review_id(index, item));
    item["correction_id"] = Value::String(id.clone());
    Ok(id)
}

fn legacy_item_matches_correction(item: &Value, correction: &TranscriptCorrection) -> bool {
    item.get("recognized").and_then(Value::as_str) == Some(correction.original.as_str())
        && item.get("start_ms").and_then(Value::as_u64) == Some(correction.start_ms)
        && item.get("end_ms").and_then(Value::as_u64) == Some(correction.end_ms)
}

fn stable_legacy_review_id(index: usize, item: &Value) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in [
        index.to_string(),
        item.get("start_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .to_string(),
        item.get("end_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .to_string(),
        item.get("recognized")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        item.get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ] {
        for byte in value.bytes().chain(std::iter::once(0xff)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("legacy-review-{index}-{hash:016x}")
}

fn legacy_review_item_correction(item: &Value, id: String) -> io::Result<TranscriptCorrection> {
    let original = item
        .get("recognized")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_legacy_review("legacy review item is missing recognized text"))?
        .to_string();
    let start_ms = item
        .get("start_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_legacy_review("legacy review item is missing start_ms"))?;
    let end_ms = item
        .get("end_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_legacy_review("legacy review item is missing end_ms"))?;
    let decided_text = item
        .get("correction")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let decision = if item
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status.contains("已人工确认"))
        && decided_text.is_some()
    {
        ReviewDecision::Approved
    } else {
        ReviewDecision::Pending
    };
    let proposed = decided_text.clone().unwrap_or_else(|| original.clone());

    Ok(TranscriptCorrection {
        id,
        start_ms,
        end_ms,
        original,
        proposed,
        category: item
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("legacy_review")
            .to_string(),
        evidence: legacy_review_evidence(item),
        critical: true,
        decision,
        decided_text,
    })
}

fn legacy_review_evidence(item: &Value) -> Vec<String> {
    match item.get("evidence") {
        Some(Value::String(value)) if !value.trim().is_empty() => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn legacy_review_item_from_correction(correction: &TranscriptCorrection) -> Value {
    serde_json::json!({
        "start_ms": correction.start_ms,
        "end_ms": correction.end_ms,
        "recognized": correction.original,
        "type": correction.category,
        "status": legacy_status(correction),
        "correction": correction.decided_text,
        "correction_id": correction.id,
    })
}

fn project_canonical_decisions(
    review_items: &mut [Value],
    corrections: &[TranscriptCorrection],
) -> bool {
    let mut changed = false;
    for item in review_items {
        let Some(correction_id) = item.get("correction_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(correction) = corrections
            .iter()
            .find(|correction| correction.id == correction_id)
        else {
            continue;
        };
        if correction.decision == ReviewDecision::Pending {
            continue;
        }
        let status = legacy_status(correction);
        if item.get("status").and_then(Value::as_str) != Some(status) {
            item["status"] = Value::String(status.to_string());
            changed = true;
        }
        if let Some(decided_text) = correction.decided_text.as_ref() {
            if item.get("correction").and_then(Value::as_str) != Some(decided_text) {
                item["correction"] = Value::String(decided_text.clone());
                changed = true;
            }
        }
    }
    changed
}

fn legacy_status(correction: &TranscriptCorrection) -> &'static str {
    match correction.decision {
        ReviewDecision::Pending => "待确认",
        ReviewDecision::Approved => "已人工确认",
        ReviewDecision::KeptOriginal => "保留原文",
    }
}

async fn legacy_audit_bundle(
    dir: &Path,
    bundle: &TranscriptAuditBundle,
    review_items: Vec<Value>,
) -> io::Result<LegacyTranscriptAuditBundle> {
    let changes_json = read_optional_legacy_file(dir, &["transcript.changes.json"])
        .await?
        .unwrap_or_else(|| serde_json::to_string(&bundle.corrections).unwrap_or_default());
    let evidence_json = read_optional_legacy_file(
        dir,
        &[
            "transcript.audit.json",
            "tmp.mp4.asr.audit.json",
            "tmp.asr.audit.json",
        ],
    )
    .await?
    .unwrap_or_default();
    let review_json = serde_json::to_string(&review_items).map_err(invalid_legacy_review_json)?;

    Ok(LegacyTranscriptAuditBundle {
        raw_srt: bundle.raw_srt.clone(),
        corrected_srt: bundle.corrected_srt.clone(),
        changes_json,
        review_json,
        evidence_json,
    })
}

async fn read_optional_legacy_file(dir: &Path, names: &[&str]) -> io::Result<Option<String>> {
    for name in names {
        match tokio::fs::read_to_string(dir.join(name)).await {
            Ok(value) => return Ok(Some(value)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

fn invalid_legacy_review_json(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn invalid_legacy_review(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resolve_transcript_correction(
    state: state_type!(),
    request: ResolveTranscriptCorrectionRequest,
) -> Result<TranscriptAuditBundle, String> {
    let ResolveTranscriptCorrectionRequest {
        source,
        correction_id,
        action,
        decided_text,
        add_to_dictionary_candidates,
    } = request;
    let (source, dir) = resolve_transcript_source(&state, &source).await?;
    resolve_transcript_correction_in_dir(
        source,
        dir,
        &correction_id,
        action,
        decided_text,
        add_to_dictionary_candidates,
        |candidate| async {
            state
                .db
                .upsert_transcript_dictionary_candidate(candidate)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        },
    )
    .await
}

pub(crate) async fn resolve_transcript_correction_in_dir<P, PersistFuture>(
    source: TranscriptSource,
    dir: impl AsRef<Path>,
    correction_id: &str,
    action: ReviewAction,
    decided_text: Option<String>,
    add_to_dictionary_candidates: bool,
    persist_candidate: P,
) -> Result<TranscriptAuditBundle, String>
where
    P: FnOnce(NewTranscriptDictionaryCandidate) -> PersistFuture,
    PersistFuture: Future<Output = Result<(), String>>,
{
    let dir = dir.as_ref();
    let current = TranscriptArtifactStore::load_from_dir(source.clone(), dir)
        .await
        .map_err(|error| error.to_string())?;
    let Some(current_correction) = current
        .corrections
        .iter()
        .find(|correction| correction.id == correction_id)
    else {
        // Recognition or review regeneration can replace correction IDs while
        // an older WebView is still open. Treat that submission as stale and
        // return the authoritative bundle so the UI can move to the latest item.
        return Ok(current);
    };
    let was_pending = current_correction.decision == ReviewDecision::Pending;

    let (bundle, resolved_now) = if was_pending {
        match TranscriptArtifactStore::resolve(
            source.clone(),
            dir,
            correction_id,
            action.clone(),
            decided_text.clone(),
        )
        .await
        {
            Ok(bundle) => (bundle, true),
            Err(resolve_error) => {
                let latest = TranscriptArtifactStore::load_from_dir(source.clone(), dir)
                    .await
                    .map_err(|_| resolve_error.to_string())?;
                if correction_from_bundle(&latest, correction_id)?.decision
                    == ReviewDecision::Pending
                {
                    return Err(resolve_error.to_string());
                }
                (latest, false)
            }
        }
    } else {
        (current, false)
    };

    let correction = correction_from_bundle(&bundle, correction_id)?;
    if resolved_now && action == ReviewAction::KeepOriginal {
        return Ok(bundle);
    }
    if resolved_now && action == ReviewAction::Approve && !add_to_dictionary_candidates {
        return Ok(bundle);
    }
    if action != ReviewAction::Approve
        || !add_to_dictionary_candidates
        || correction.decision != ReviewDecision::Approved
        || normalized_decided_text(decided_text.as_deref()) != correction.decided_text.as_deref()
    {
        // The persisted decision is authoritative. A stale WebView can submit
        // an older choice after another click already saved the correction;
        // return the latest bundle so the UI refreshes instead of surfacing a
        // technical conflict to the reviewer.
        return Ok(bundle);
    }

    let candidate = build_replacement_candidate(&source, correction)?;
    persist_candidate(candidate).await.map_err(|error| {
        format!(
            "transcript review decision was saved, but dictionary candidate persistence failed: \
             {error}. Retry the identical approved request with \
             addToDictionaryCandidates=true"
        )
    })?;

    Ok(bundle)
}

fn correction_from_bundle<'a>(
    bundle: &'a TranscriptAuditBundle,
    correction_id: &str,
) -> Result<&'a TranscriptCorrection, String> {
    bundle
        .corrections
        .iter()
        .find(|correction| correction.id == correction_id)
        .ok_or_else(|| format!("correction {correction_id} does not exist"))
}

fn normalized_decided_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(crate) fn build_replacement_candidate(
    source: &TranscriptSource,
    correction: &TranscriptCorrection,
) -> Result<NewTranscriptDictionaryCandidate, String> {
    if correction.decision != ReviewDecision::Approved {
        return Err("dictionary candidates require an approved correction".to_string());
    }
    let target_text = correction
        .decided_text
        .as_deref()
        .and_then(|value| normalized_decided_text(Some(value)))
        .ok_or_else(|| "approved correction is missing decided text".to_string())?;
    let (source_kind, platform, room_id, live_id, video_id) = match source {
        TranscriptSource::Archive {
            platform,
            room_id,
            live_id,
        } => (
            CandidateSourceKind::Archive,
            Some(platform.clone()),
            Some(room_id.clone()),
            Some(live_id.clone()),
            None,
        ),
        TranscriptSource::Video { video_id } => (
            CandidateSourceKind::Video,
            None,
            None,
            None,
            Some(*video_id),
        ),
    };

    Ok(NewTranscriptDictionaryCandidate {
        candidate_type: TranscriptDictionaryCandidateType::Replacement,
        source_text: correction.original.clone(),
        target_text: target_text.to_string(),
        evidence: vec![CandidateEvidence {
            kind: CandidateEvidenceKind::HumanReview,
            provider: None,
            field: None,
        }],
        source: CandidateSourceMetadata {
            correction_id: correction.id.clone(),
            source_kind,
            platform,
            room_id,
            live_id,
            video_id,
            start_ms: Some(correction.start_ms),
            end_ms: Some(correction.end_ms),
        },
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_transcript_dictionary_candidates(
    state: state_type!(),
    status: Option<String>,
) -> Result<Vec<TranscriptDictionaryCandidateRow>, String> {
    state
        .db
        .list_transcript_dictionary_candidates(status.as_deref())
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn set_transcript_dictionary_candidate_status(
    state: state_type!(),
    id: i64,
    status: String,
) -> Result<TranscriptDictionaryCandidateRow, String> {
    state
        .db
        .set_transcript_dictionary_candidate_status(id, &status)
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn export_transcript_dictionary_candidates(
    state: state_type!(),
    format: String,
) -> Result<String, String> {
    let format = parse_candidate_export_format(&format)?;
    let candidates = state
        .db
        .list_transcript_dictionary_candidates(None)
        .await
        .map_err(|error| error.to_string())?;
    match format {
        CandidateExportFormat::Json => export_transcript_dictionary_candidates_json(&candidates)
            .map_err(|error| error.to_string()),
        CandidateExportFormat::Csv => Ok(export_transcript_dictionary_candidates_csv(&candidates)),
    }
}

pub(crate) fn parse_candidate_export_format(format: &str) -> Result<CandidateExportFormat, String> {
    match format {
        "json" => Ok(CandidateExportFormat::Json),
        "csv" => Ok(CandidateExportFormat::Csv),
        _ => Err("transcript dictionary candidate export format must be json or csv".to_string()),
    }
}

async fn resolve_transcript_source(
    state: &State,
    source: &TranscriptSource,
) -> Result<(TranscriptSource, PathBuf), String> {
    match source {
        TranscriptSource::Archive {
            platform,
            room_id,
            live_id,
        } => {
            let platform = PlatformType::from_str(platform)?;
            validate_archive_identifier("roomId", room_id)?;
            validate_archive_identifier("liveId", live_id)?;
            let archive_dir = state
                .recorder_manager
                .resolve_archive_dir(platform, room_id, live_id)
                .await;

            Ok((source.clone(), archive_dir))
        }
        TranscriptSource::Video { video_id } => {
            let output = state.config.read().await.output.clone();
            resolve_canonical_video_source(&state.db, Path::new(&output), *video_id).await
        }
    }
}

pub(crate) async fn resolve_canonical_video_source(
    db: &Database,
    output: &Path,
    requested_video_id: i64,
) -> Result<(TranscriptSource, PathBuf), String> {
    let (_, requested_file) = db
        .get_video_source(requested_video_id)
        .await
        .map_err(|error| error.to_string())?;
    let requested_media =
        database_video_media_path(db, output, requested_video_id, &requested_file).await?;
    let requested_identity = resolved_path_identity(&requested_media)?;
    let requested_dir =
        database_video_artifact_dir(db, output, requested_video_id, &requested_file).await?;

    let sources = db
        .list_video_sources()
        .await
        .map_err(|error| error.to_string())?;
    if let Some(owner) = TranscriptArtifactStore::read_source_owner(&requested_dir)
        .await
        .map_err(|error| error.to_string())?
    {
        let TranscriptSource::Video { video_id } = owner else {
            return Err("video transcript artifacts have a non-video source owner".to_string());
        };
        let owner_file = sources
            .iter()
            .find_map(|(id, file)| (*id == video_id).then_some(file))
            .ok_or_else(|| {
                format!("transcript artifact owner video id {video_id} does not exist")
            })?;
        let owner_media = database_video_media_path(db, output, video_id, owner_file).await?;
        if !same_path_identity(&requested_identity, &resolved_path_identity(&owner_media)?) {
            return Err(format!(
                "transcript artifacts are bound to video id {video_id}, which resolves to a different file"
            ));
        }
        return Ok((
            TranscriptSource::Video { video_id },
            database_video_artifact_dir(db, output, video_id, owner_file).await?,
        ));
    }

    let mut aliases = Vec::new();
    for (video_id, file) in sources {
        let Ok(media_file) = database_video_media_path(db, output, video_id, &file).await else {
            continue;
        };
        let Ok(identity) = resolved_path_identity(&media_file) else {
            continue;
        };
        if same_path_identity(&requested_identity, &identity) {
            aliases.push(video_id);
        }
    }
    if aliases.len() > 1 {
        return Err(format!(
            "transcript artifact source is ambiguous across video ids: {}",
            aliases
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok((
        TranscriptSource::Video {
            video_id: requested_video_id,
        },
        requested_dir,
    ))
}

async fn database_video_artifact_dir(
    db: &Database,
    output: &Path,
    video_id: i64,
    file: &str,
) -> Result<PathBuf, String> {
    if let Some(archive) = db
        .get_video_archive_by_video(video_id)
        .await
        .map_err(|error| error.to_string())?
    {
        if archive.status == "archived"
            && !archive.local_path.trim().is_empty()
            && archive.nas_path == file
        {
            let original_local_path = database_owned_video_path(output, &archive.local_path)?;
            return Ok(TranscriptArtifactStore::video_artifact_dir(
                original_local_path,
            ));
        }
    }
    Ok(TranscriptArtifactStore::video_artifact_dir(
        database_owned_video_path(output, file)?,
    ))
}

pub(crate) async fn database_video_media_path(
    db: &Database,
    output: &Path,
    video_id: i64,
    file: &str,
) -> Result<PathBuf, String> {
    if let Some(archive) = db
        .get_video_archive_by_video(video_id)
        .await
        .map_err(|error| error.to_string())?
    {
        if archive.status == "archived"
            && !archive.nas_path.trim().is_empty()
            && archive.nas_path == file
        {
            let path = PathBuf::from(file);
            if !path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
            {
                return Err("archived NAS video path is invalid".to_string());
            }
            return Ok(path);
        }
    }
    database_owned_video_path(output, file)
}

fn resolved_path_identity(path: &Path) -> Result<PathBuf, String> {
    let lexical = lexical_absolute_path(path)?;
    let Some(ancestor) = nearest_existing_ancestor(&lexical) else {
        return Ok(lexical);
    };
    let Ok(canonical_ancestor) = std::fs::canonicalize(ancestor) else {
        return Ok(lexical);
    };
    let suffix = lexical
        .strip_prefix(ancestor)
        .map_err(|error| error.to_string())?;
    Ok(canonical_ancestor.join(suffix))
}

#[cfg(windows)]
fn same_path_identity(left: &Path, right: &Path) -> bool {
    same_path_identity_with_case_semantics(left, right, true)
}

#[cfg(not(windows))]
fn same_path_identity(left: &Path, right: &Path) -> bool {
    same_path_identity_with_case_semantics(left, right, false)
}

fn same_path_identity_with_case_semantics(
    left: &Path,
    right: &Path,
    case_insensitive: bool,
) -> bool {
    if case_insensitive {
        left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
    } else {
        left == right
    }
}

fn validate_archive_identifier(name: &str, value: &str) -> Result<(), String> {
    if is_managed_path_component(value) {
        Ok(())
    } else {
        Err(format!("{name} must be a single managed path component"))
    }
}

fn is_managed_path_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

pub(crate) fn database_owned_video_path(output: &Path, file: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(file);
    if candidate.is_absolute() {
        validate_absolute_database_owned_file(output, candidate)
    } else {
        validate_database_owned_file(file)?;
        validate_absolute_database_owned_file(output, &output.join(candidate))
    }
}

fn validate_database_owned_file(file: &str) -> Result<(), String> {
    let mut has_component = false;
    for component in Path::new(file).components() {
        has_component = true;
        if !matches!(component, Component::Normal(_)) {
            return Err("video file must be a relative database-owned path".to_string());
        }
    }
    if !has_component {
        return Err("video file must be a relative database-owned path".to_string());
    }
    Ok(())
}

fn validate_absolute_database_owned_file(
    output: &Path,
    candidate: &Path,
) -> Result<PathBuf, String> {
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("video file contains invalid parent traversal".to_string());
    }
    let lexical_output = lexical_absolute_path(output)?;
    let lexical_candidate = lexical_absolute_path(candidate)?;
    if !path_starts_with(&lexical_candidate, &lexical_output) {
        return Err("video file must remain inside configured output".to_string());
    }

    if let Ok(canonical_output) = std::fs::canonicalize(&lexical_output) {
        let canonical_candidate = nearest_existing_ancestor(&lexical_candidate)
            .and_then(|path| std::fs::canonicalize(path).ok())
            .unwrap_or_else(|| lexical_candidate.clone());
        if !path_starts_with(&canonical_candidate, &canonical_output) {
            return Err("video file must remain inside configured output".to_string());
        }
    }

    Ok(lexical_candidate)
}

#[cfg(windows)]
fn path_starts_with(path: &Path, base: &Path) -> bool {
    let mut path_components = path.components();
    base.components().all(|base_component| {
        path_components.next().is_some_and(|path_component| {
            path_component.as_os_str().to_string_lossy().to_lowercase()
                == base_component.as_os_str().to_string_lossy().to_lowercase()
        })
    })
}

#[cfg(not(windows))]
fn path_starts_with(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

fn lexical_absolute_path(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err("video file contains invalid parent traversal".to_string());
                }
            }
        }
    }
    Ok(normalized)
}

fn nearest_existing_ancestor(path: &Path) -> Option<&Path> {
    let mut current = path;
    loop {
        if current.exists() {
            return Some(current);
        }
        current = current.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_replacement_candidate, is_managed_path_component, parse_candidate_export_format,
        resolve_canonical_video_source, resolve_transcript_correction_in_dir,
        same_path_identity_with_case_semantics, validate_database_owned_file,
        CandidateExportFormat, ResolveTranscriptCorrectionRequest,
    };
    use crate::database::transcript_dictionary_candidate::{
        CandidateEvidenceKind, TranscriptDictionaryCandidateType,
        TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL,
    };
    use crate::database::Database;
    use crate::subtitle_generator::transcript_artifacts::{
        ReviewAction, ReviewDecision, TranscriptArtifactStore, TranscriptCorrection,
        TranscriptSource,
    };
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;
    use std::sync::{Arc, Mutex};

    #[test]
    fn resolve_request_uses_camel_case_fields() {
        let request: ResolveTranscriptCorrectionRequest =
            serde_json::from_value(serde_json::json!({
                "source": { "kind": "video", "videoId": 7 },
                "correctionId": "correction-7",
                "action": "keep_original",
                "decidedText": null,
                "addToDictionaryCandidates": false
            }))
            .unwrap();

        assert_eq!(request.source, TranscriptSource::Video { video_id: 7 });
        assert_eq!(request.correction_id, "correction-7");
        assert_eq!(request.action, ReviewAction::KeepOriginal);
        assert_eq!(request.decided_text, None);
        assert!(!request.add_to_dictionary_candidates);
    }

    #[test]
    fn managed_path_component_rejects_path_like_source_identifiers() {
        assert!(is_managed_path_component("room-7"));
        assert!(!is_managed_path_component("../outside"));
        assert!(!is_managed_path_component("nested/archive"));
    }

    #[test]
    fn database_owned_video_file_must_remain_relative_to_output() {
        assert!(validate_database_owned_file("clips/recording.mp4").is_ok());
        assert!(validate_database_owned_file("../recording.mp4").is_err());
        assert!(validate_database_owned_file("C:/recording.mp4").is_err());
    }

    #[test]
    fn relative_video_path_remains_valid_before_output_root_is_created() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("not-created");

        assert_eq!(
            super::database_owned_video_path(&output, "clips/review.mp4").unwrap(),
            output.join("clips").join("review.mp4")
        );
    }

    #[test]
    fn path_identity_case_behavior_matches_windows_and_non_windows_contracts() {
        let lower = std::path::Path::new("output/clips/review.mp4");
        let upper = std::path::Path::new("OUTPUT/CLIPS/REVIEW.MP4");

        assert!(same_path_identity_with_case_semantics(lower, upper, true));
        assert!(!same_path_identity_with_case_semantics(lower, upper, false));
    }

    fn source() -> TranscriptSource {
        TranscriptSource::Video { video_id: 7 }
    }

    fn correction() -> TranscriptCorrection {
        TranscriptCorrection {
            id: "correction-7".to_string(),
            start_ms: 1_000,
            end_ms: 2_000,
            original: "A4PRO299元".to_string(),
            proposed: "A4PRO2 99元".to_string(),
            category: r#"fact_card:{"price":99}"#.to_string(),
            evidence: vec!["Authorization: Bearer raw-secret".to_string()],
            critical: true,
            decision: ReviewDecision::Pending,
            decided_text: None,
        }
    }

    async fn initialize_review(dir: &std::path::Path) {
        TranscriptArtifactStore::initialize(
            source(),
            dir,
            "1\n00:00:01,000 --> 00:00:02,000\nA4PRO299元\n\n",
            "1\n00:00:01,000 --> 00:00:02,000\nA4PRO299元\n\n",
            vec![correction()],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn keep_original_never_persists_a_dictionary_candidate() {
        let dir = tempfile::tempdir().unwrap();
        initialize_review(dir.path()).await;
        let persisted = Arc::new(Mutex::new(0));
        let persisted_for_call = persisted.clone();

        let bundle = resolve_transcript_correction_in_dir(
            source(),
            dir.path(),
            "correction-7",
            ReviewAction::KeepOriginal,
            None,
            true,
            move |_| {
                *persisted_for_call.lock().unwrap() += 1;
                async { Ok(()) }
            },
        )
        .await
        .unwrap();

        assert_eq!(*persisted.lock().unwrap(), 0);
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::KeptOriginal);
    }

    #[tokio::test]
    async fn first_approval_persists_one_replacement_candidate() {
        let dir = tempfile::tempdir().unwrap();
        initialize_review(dir.path()).await;
        let candidates = Arc::new(Mutex::new(Vec::new()));
        let candidates_for_call = candidates.clone();

        let bundle = resolve_transcript_correction_in_dir(
            source(),
            dir.path(),
            "correction-7",
            ReviewAction::Approve,
            Some("A4PRO2 99元".to_string()),
            true,
            move |candidate| {
                candidates_for_call.lock().unwrap().push(candidate);
                async { Ok(()) }
            },
        )
        .await
        .unwrap();

        let candidates = candidates.lock().unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].candidate_type,
            TranscriptDictionaryCandidateType::Replacement
        );
        assert_eq!(candidates[0].source_text, "A4PRO299元");
        assert_eq!(candidates[0].target_text, "A4PRO2 99元");
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Approved);
    }

    #[tokio::test]
    async fn identical_approved_retry_deduplicates_through_candidate_repository() {
        let dir = tempfile::tempdir().unwrap();
        initialize_review(dir.path()).await;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(TRANSCRIPT_DICTIONARY_CANDIDATES_MIGRATION_SQL)
            .await
            .unwrap();
        let db = Arc::new(Database::new());
        db.set(pool).await;

        for _ in 0..2 {
            let db_for_call = db.clone();
            resolve_transcript_correction_in_dir(
                source(),
                dir.path(),
                "correction-7",
                ReviewAction::Approve,
                Some("A4PRO2 99元".to_string()),
                true,
                move |candidate| async move {
                    db_for_call
                        .upsert_transcript_dictionary_candidate(candidate)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                },
            )
            .await
            .unwrap();
        }

        assert_eq!(
            db.list_transcript_dictionary_candidates(None)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn unowned_filesystem_equivalent_video_aliases_are_rejected_deterministically() {
        let output = tempfile::tempdir().unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("CREATE TABLE videos (id INTEGER PRIMARY KEY, file TEXT NOT NULL)")
            .await
            .unwrap();
        let absolute = output.path().join("clips").join("review.mp4");
        for (id, file) in [
            (9, absolute.to_string_lossy().into_owned()),
            (7, "clips/review.mp4".to_string()),
        ] {
            sqlx::query("INSERT INTO videos (id, file) VALUES ($1, $2)")
                .bind(id)
                .bind(file)
                .execute(&pool)
                .await
                .unwrap();
        }
        let db = Database::new();
        db.set(pool).await;

        let error = resolve_canonical_video_source(&db, output.path(), 9)
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "transcript artifact source is ambiguous across video ids: 7, 9"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn persisted_owner_binds_relative_absolute_separator_and_case_aliases_on_windows() {
        let output = tempfile::tempdir().unwrap();
        let media = output.path().join("clips").join("review.mp4");
        let artifact_dir = TranscriptArtifactStore::video_artifact_dir(&media);
        TranscriptArtifactStore::initialize(
            TranscriptSource::Video { video_id: 7 },
            &artifact_dir,
            "raw",
            "corrected",
            vec![],
        )
        .await
        .unwrap();

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("CREATE TABLE videos (id INTEGER PRIMARY KEY, file TEXT NOT NULL)")
            .await
            .unwrap();
        for (id, file) in [
            (7, "clips/review.mp4".to_string()),
            (8, media.to_string_lossy().to_uppercase()),
            (9, "CLIPS\\REVIEW.MP4".to_string()),
        ] {
            sqlx::query("INSERT INTO videos (id, file) VALUES ($1, $2)")
                .bind(id)
                .bind(file)
                .execute(&pool)
                .await
                .unwrap();
        }
        let db = Database::new();
        db.set(pool).await;

        for requested_id in [7, 8, 9] {
            let (source, dir) = resolve_canonical_video_source(&db, output.path(), requested_id)
                .await
                .unwrap();
            assert_eq!(source, TranscriptSource::Video { video_id: 7 });
            assert_eq!(dir, artifact_dir);
        }
    }

    #[tokio::test]
    async fn persisted_owner_path_change_is_rejected_without_relabeling() {
        let output = tempfile::tempdir().unwrap();
        let media = output.path().join("clips").join("review.mp4");
        let artifact_dir = TranscriptArtifactStore::video_artifact_dir(&media);
        TranscriptArtifactStore::initialize(
            TranscriptSource::Video { video_id: 7 },
            &artifact_dir,
            "raw",
            "corrected",
            vec![],
        )
        .await
        .unwrap();

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(
            "CREATE TABLE videos (id INTEGER PRIMARY KEY, file TEXT NOT NULL); \
             INSERT INTO videos (id, file) VALUES (7, 'clips/moved.mp4'); \
             INSERT INTO videos (id, file) VALUES (8, 'clips/review.mp4');",
        )
        .await
        .unwrap();
        let db = Database::new();
        db.set(pool).await;

        let error = resolve_canonical_video_source(&db, output.path(), 8)
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "transcript artifacts are bound to video id 7, which resolves to a different file"
        );
        assert_eq!(
            TranscriptArtifactStore::read_source_owner(&artifact_dir)
                .await
                .unwrap(),
            Some(TranscriptSource::Video { video_id: 7 })
        );
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn case_variants_remain_distinct_video_sources_off_windows() {
        let output = tempfile::tempdir().unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(
            "CREATE TABLE videos (id INTEGER PRIMARY KEY, file TEXT NOT NULL); \
             INSERT INTO videos (id, file) VALUES (7, 'clips/review.mp4'); \
             INSERT INTO videos (id, file) VALUES (8, 'clips/REVIEW.mp4');",
        )
        .await
        .unwrap();
        let db = Database::new();
        db.set(pool).await;

        let (source, dir) = resolve_canonical_video_source(&db, output.path(), 8)
            .await
            .unwrap();
        assert_eq!(source, TranscriptSource::Video { video_id: 8 });
        assert_eq!(
            dir,
            TranscriptArtifactStore::video_artifact_dir(
                output.path().join("clips").join("REVIEW.mp4")
            )
        );
    }

    #[tokio::test]
    async fn failed_candidate_write_can_be_retried_without_redeciding() {
        let dir = tempfile::tempdir().unwrap();
        initialize_review(dir.path()).await;

        let error = resolve_transcript_correction_in_dir(
            source(),
            dir.path(),
            "correction-7",
            ReviewAction::Approve,
            Some("A4PRO2 99元".to_string()),
            true,
            |_| async { Err("simulated database failure".to_string()) },
        )
        .await
        .unwrap_err();
        assert!(error.contains("decision was saved"));
        assert!(error.contains("simulated database failure"));

        let after_failure = TranscriptArtifactStore::load_from_dir(source(), dir.path())
            .await
            .unwrap();
        assert_eq!(
            after_failure.corrections[0].decision,
            ReviewDecision::Approved
        );

        let retried = resolve_transcript_correction_in_dir(
            source(),
            dir.path(),
            "correction-7",
            ReviewAction::Approve,
            Some(" A4PRO2 99元 ".to_string()),
            true,
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        assert_eq!(retried.corrections[0].decision, ReviewDecision::Approved);
    }

    #[tokio::test]
    async fn approved_retry_with_a_stale_choice_returns_the_persisted_decision() {
        let dir = tempfile::tempdir().unwrap();
        initialize_review(dir.path()).await;
        resolve_transcript_correction_in_dir(
            source(),
            dir.path(),
            "correction-7",
            ReviewAction::Approve,
            Some("A4PRO2 99元".to_string()),
            true,
            |_| async { Ok(()) },
        )
        .await
        .unwrap();

        for (action, decided_text) in [
            (ReviewAction::Approve, Some("different".to_string())),
            (ReviewAction::KeepOriginal, None),
        ] {
            let calls = Arc::new(Mutex::new(0));
            let calls_for_attempt = calls.clone();
            let bundle = resolve_transcript_correction_in_dir(
                source(),
                dir.path(),
                "correction-7",
                action,
                decided_text,
                true,
                move |_| {
                    *calls_for_attempt.lock().unwrap() += 1;
                    async { Ok(()) }
                },
            )
            .await
            .unwrap();
            assert_eq!(bundle.corrections[0].decision, ReviewDecision::Approved);
            assert_eq!(
                bundle.corrections[0].decided_text.as_deref(),
                Some("A4PRO2 99元")
            );
            assert_eq!(*calls.lock().unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn missing_correction_from_a_stale_webview_returns_the_latest_bundle() {
        let dir = tempfile::tempdir().unwrap();
        initialize_review(dir.path()).await;
        let persist_calls = Arc::new(Mutex::new(0));
        let persist_calls_for_attempt = persist_calls.clone();

        let bundle = resolve_transcript_correction_in_dir(
            source(),
            dir.path(),
            "correction-from-older-review",
            ReviewAction::KeepOriginal,
            None,
            false,
            move |_| {
                *persist_calls_for_attempt.lock().unwrap() += 1;
                async { Ok(()) }
            },
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrections[0].id, "correction-7");
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
        assert_eq!(*persist_calls.lock().unwrap(), 0);
    }

    #[test]
    fn replacement_candidate_serializes_only_safe_typed_metadata() {
        let mut approved = correction();
        approved.decision = ReviewDecision::Approved;
        approved.decided_text = Some("A4PRO2 99元".to_string());

        let candidate = build_replacement_candidate(&source(), &approved).unwrap();
        let serialized = serde_json::to_string(&candidate).unwrap();

        assert_eq!(candidate.evidence.len(), 1);
        assert_eq!(
            candidate.evidence[0].kind,
            CandidateEvidenceKind::HumanReview
        );
        assert!(serialized.contains(r#""correctionId":"correction-7""#));
        assert!(serialized.contains(r#""startMs":1000"#));
        assert!(!serialized.contains("Authorization"));
        assert!(!serialized.contains("raw-secret"));
        assert!(!serialized.contains("fact_card"));
    }

    #[test]
    fn candidate_export_format_accepts_only_json_and_csv() {
        assert_eq!(
            parse_candidate_export_format("json").unwrap(),
            CandidateExportFormat::Json
        );
        assert_eq!(
            parse_candidate_export_format("csv").unwrap(),
            CandidateExportFormat::Csv
        );
        assert_eq!(
            parse_candidate_export_format("xlsx").unwrap_err(),
            "transcript dictionary candidate export format must be json or csv"
        );
    }
}
