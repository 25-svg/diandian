use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TranscriptSource {
    Archive {
        platform: String,
        room_id: String,
        live_id: String,
    },
    Video {
        video_id: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Pending,
    Approved,
    KeptOriginal,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    Approve,
    KeepOriginal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCorrection {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub original: String,
    pub proposed: String,
    pub category: String,
    pub evidence: Vec<String>,
    pub critical: bool,
    pub decision: ReviewDecision,
    pub decided_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAuditBundle {
    pub source: TranscriptSource,
    pub raw_srt: String,
    pub corrected_srt: String,
    pub corrections: Vec<TranscriptCorrection>,
    pub pending_critical_count: usize,
}

pub struct TranscriptArtifactStore;

struct ArtifactWrite {
    target: PathBuf,
    value: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct BatchManifest {
    entries: Vec<BatchManifestEntry>,
}

#[derive(Serialize, Deserialize)]
struct BatchManifestEntry {
    name: String,
    existed: bool,
}

#[derive(Deserialize)]
struct LegacyCorrectionChange {
    #[serde(rename = "原文")]
    source: String,
    #[serde(rename = "校对稿")]
    corrected: String,
    #[serde(rename = "修改类型", default)]
    change_type: String,
    #[serde(rename = "依据", default)]
    evidence: String,
    #[serde(rename = "是否需要人工确认", default)]
    needs_review: bool,
}

#[derive(Deserialize)]
struct VolcengineAudit {
    #[serde(default)]
    changes: Vec<VolcengineAuditChange>,
}

#[derive(Deserialize)]
struct VolcengineAuditChange {
    start_ms: u64,
    original: String,
    corrected: String,
    evidence: String,
    rule: String,
}

struct CorrectionProposal {
    source: String,
    corrected: String,
    category: String,
    evidence: Vec<String>,
    needs_review: bool,
    start_ms: Option<u64>,
    provider_critical: bool,
}

struct LocatedCorrection {
    proposal: CorrectionProposal,
    start_ms: u64,
    end_ms: u64,
    critical: bool,
    supported: bool,
    pending: bool,
}

#[derive(Clone)]
struct SrtCue {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

impl TranscriptArtifactStore {
    const RAW: &'static str = "subtitle.raw.srt";
    const CORRECTED: &'static str = "subtitle.corrected.srt";
    const CORRECTIONS: &'static str = "subtitle.corrections.json";
    const COMPATIBILITY_SUBTITLE: &'static str = "subtitle.srt";
    const SOURCE: &'static str = "subtitle.source.json";
    const BATCH_MARKER: &'static str = ".subtitle.artifacts.txn.json";
    const BATCH_MARKER_TEMP: &'static str = ".subtitle.artifacts.txn.tmp";

    pub fn video_artifact_dir(media_file: impl AsRef<Path>) -> PathBuf {
        let media_file = media_file.as_ref();
        match media_file.file_name() {
            Some(name) => {
                media_file.with_file_name(format!("{}.transcript", name.to_string_lossy()))
            }
            None => media_file.join("transcript"),
        }
    }

    pub fn asr_audit_path(media_file: impl AsRef<Path>) -> PathBuf {
        let media_file = media_file.as_ref();
        match media_file.file_name() {
            Some(file_name) => {
                let mut audit_name = file_name.to_os_string();
                audit_name.push(".asr.audit.json");
                media_file.with_file_name(audit_name)
            }
            None => media_file.join("asr.audit.json"),
        }
    }

    pub async fn initialize_from_asr_outputs(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
        media_file: impl AsRef<Path>,
        generated_srt: &str,
        provider: &str,
    ) -> io::Result<TranscriptAuditBundle> {
        let media_file = media_file.as_ref();
        let fact_card = read_optional_json(media_file.with_extension("facts.json")).await;
        let (raw_sidecar, deterministic_srt, proposals) = match provider {
            "funasr" => {
                let raw_sidecar = read_optional(media_file.with_extension("asr.raw.srt")).await?;
                let corrected_sidecar =
                    read_optional(media_file.with_extension("asr.corrected.srt")).await?;
                let deterministic_srt =
                    corrected_sidecar.unwrap_or_else(|| generated_srt.to_string());
                let proposals =
                    read_funasr_proposals(media_file.with_extension("asr.changes.json")).await?;
                (raw_sidecar, deterministic_srt, proposals)
            }
            "volcengine" => (
                None,
                generated_srt.to_string(),
                read_volcengine_proposals(media_file).await?,
            ),
            _ => (None, generated_srt.to_string(), Vec::new()),
        };
        let (raw_srt, safe_corrected_srt, corrections) = prepare_asr_artifacts(
            raw_sidecar,
            deterministic_srt,
            proposals,
            fact_card.as_ref(),
        )?;

        Self::initialize(source, dir, &raw_srt, &safe_corrected_srt, corrections).await
    }

    pub async fn load_from_dir(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
    ) -> io::Result<TranscriptAuditBundle> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        Self::load_from_dir_unlocked(source, &dir).await
    }

    pub async fn read_source_owner(dir: impl AsRef<Path>) -> io::Result<Option<TranscriptSource>> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        Self::recover_pending_batch(&dir).await?;
        Self::restore_backup_if_needed(&dir.join(Self::SOURCE)).await?;
        Self::read_source_owner_unlocked(&dir).await
    }

    pub async fn canonical_artifacts_exist(dir: impl AsRef<Path>) -> io::Result<bool> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        Self::recover_pending_batch(&dir).await?;
        for name in [Self::SOURCE, Self::RAW, Self::CORRECTED, Self::CORRECTIONS] {
            if tokio::fs::try_exists(dir.join(name)).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn read_source_owner_unlocked(dir: &Path) -> io::Result<Option<TranscriptSource>> {
        match tokio::fs::read_to_string(dir.join(Self::SOURCE)).await {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(invalid_correction_json),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn load_from_dir_unlocked(
        source: TranscriptSource,
        dir: &Path,
    ) -> io::Result<TranscriptAuditBundle> {
        Self::recover_pending_batch(dir).await?;
        Self::restore_backup_if_needed(&dir.join(Self::RAW)).await?;
        Self::restore_backup_if_needed(&dir.join(Self::CORRECTED)).await?;
        Self::restore_backup_if_needed(&dir.join(Self::CORRECTIONS)).await?;
        Self::restore_backup_if_needed(&dir.join(Self::COMPATIBILITY_SUBTITLE)).await?;
        Self::restore_backup_if_needed(&dir.join(Self::SOURCE)).await?;

        if Self::read_source_owner_unlocked(dir)
            .await?
            .is_some_and(|owner| owner != source)
        {
            return Err(invalid_input(
                "transcript artifacts are bound to a different source",
            ));
        }

        let raw_srt = Self::read_with_fallback(dir, Self::RAW, "transcript.raw.srt").await?;
        let corrected_srt =
            Self::read_with_fallback(dir, Self::CORRECTED, "transcript.corrected.srt").await?;
        let corrections = match tokio::fs::read_to_string(dir.join(Self::CORRECTIONS)).await {
            Ok(value) => serde_json::from_str(&value).map_err(invalid_correction_json)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };

        Ok(TranscriptAuditBundle {
            source,
            raw_srt,
            corrected_srt,
            pending_critical_count: pending_critical_count(&corrections),
            corrections,
        })
    }

    pub async fn initialize(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
        raw_srt: &str,
        corrected_srt: &str,
        corrections: Vec<TranscriptCorrection>,
    ) -> io::Result<TranscriptAuditBundle> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        Self::initialize_unlocked(source, &dir, raw_srt, corrected_srt, corrections).await
    }

    pub async fn ensure_legacy_corrections(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
        legacy_corrections: Vec<TranscriptCorrection>,
    ) -> io::Result<TranscriptAuditBundle> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        let mut bundle = Self::load_from_dir_unlocked(source, &dir).await?;
        let mut known_ids = bundle
            .corrections
            .iter()
            .map(|correction| correction.id.clone())
            .collect::<HashSet<_>>();
        let missing = legacy_corrections
            .into_iter()
            .filter(|correction| known_ids.insert(correction.id.clone()))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bundle.corrections.extend(missing);
            Self::write_mutable_artifacts(&dir, &bundle.corrected_srt, &bundle.corrections).await?;
        }
        bundle.pending_critical_count = pending_critical_count(&bundle.corrections);
        Ok(bundle)
    }

    async fn initialize_unlocked(
        source: TranscriptSource,
        dir: &Path,
        raw_srt: &str,
        corrected_srt: &str,
        corrections: Vec<TranscriptCorrection>,
    ) -> io::Result<TranscriptAuditBundle> {
        tokio::fs::create_dir_all(dir).await?;
        Self::recover_pending_batch(dir).await?;
        Self::restore_backup_if_needed(&dir.join(Self::SOURCE)).await?;
        let source_owner = Self::read_source_owner_unlocked(dir).await?;
        if source_owner.as_ref().is_some_and(|owner| owner != &source) {
            return Err(invalid_input(
                "transcript artifacts are bound to a different source",
            ));
        }
        let had_artifacts = Self::has_existing_artifacts(dir).await?;
        let raw_path = dir.join(Self::RAW);
        Self::restore_backup_if_needed(&raw_path).await?;
        let current_raw = Self::read_with_fallback(dir, Self::RAW, "transcript.raw.srt").await?;
        let raw_exists = tokio::fs::try_exists(&raw_path).await?;
        let replace_raw = !raw_srt.is_empty() && raw_srt != current_raw;
        if replace_raw && !current_raw.is_empty() {
            Self::archive_raw_generation(dir, &current_raw).await?;
        }

        let corrections_json =
            serde_json::to_vec_pretty(&corrections).map_err(invalid_correction_json)?;
        let mut writes = vec![
            ArtifactWrite {
                target: dir.join(Self::CORRECTED),
                value: corrected_srt.as_bytes().to_vec(),
            },
            ArtifactWrite {
                target: dir.join(Self::CORRECTIONS),
                value: corrections_json,
            },
            ArtifactWrite {
                target: dir.join(Self::COMPATIBILITY_SUBTITLE),
                value: corrected_srt.as_bytes().to_vec(),
            },
        ];
        if source_owner.is_none() && !had_artifacts {
            writes.push(ArtifactWrite {
                target: dir.join(Self::SOURCE),
                value: serde_json::to_vec_pretty(&source).map_err(invalid_correction_json)?,
            });
        }
        if !raw_exists || replace_raw {
            let active_raw = if raw_srt.is_empty() {
                current_raw
            } else {
                raw_srt.to_string()
            };
            writes.insert(
                0,
                ArtifactWrite {
                    target: raw_path,
                    value: active_raw.into_bytes(),
                },
            );
        }

        Self::write_artifact_batch(dir, writes).await?;
        Self::load_from_dir_unlocked(source, dir).await
    }

    async fn has_existing_artifacts(dir: &Path) -> io::Result<bool> {
        for name in [
            Self::RAW,
            Self::CORRECTED,
            Self::CORRECTIONS,
            Self::COMPATIBILITY_SUBTITLE,
            "transcript.raw.srt",
            "transcript.corrected.srt",
            "transcript.corrections.json",
        ] {
            if tokio::fs::try_exists(dir.join(name)).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn resolve(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
        correction_id: &str,
        action: ReviewAction,
        decided_text: Option<String>,
    ) -> io::Result<TranscriptAuditBundle> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        let mut bundle = Self::load_from_dir_unlocked(source, &dir).await?;
        let correction_index = bundle
            .corrections
            .iter()
            .position(|correction| correction.id == correction_id)
            .ok_or_else(|| invalid_input(format!("correction {correction_id} does not exist")))?;
        if bundle.corrections[correction_index].decision != ReviewDecision::Pending {
            return Err(invalid_input("correction has already been decided"));
        }

        match action {
            ReviewAction::Approve => {
                let decided_text = decided_text
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| invalid_input("approved corrections require decided text"))?;
                if is_pending_placeholder(decided_text) {
                    return Err(invalid_input(
                        "unresolved placeholders cannot be approved; confirm the audio or keep the original",
                    ));
                }
                let correction = &bundle.corrections[correction_index];
                bundle.corrected_srt = replace_cue_text(
                    &bundle.corrected_srt,
                    correction.start_ms,
                    correction.end_ms,
                    &correction.original,
                    decided_text,
                )?;
                let correction = &mut bundle.corrections[correction_index];
                correction.decision = ReviewDecision::Approved;
                correction.decided_text = Some(decided_text.to_string());
            }
            ReviewAction::KeepOriginal => {
                let correction = &mut bundle.corrections[correction_index];
                correction.decision = ReviewDecision::KeptOriginal;
                correction.decided_text = Some(correction.original.clone());
            }
        }

        Self::write_mutable_artifacts(&dir, &bundle.corrected_srt, &bundle.corrections).await?;
        bundle.pending_critical_count = pending_critical_count(&bundle.corrections);
        Ok(bundle)
    }

    pub async fn apply_manual_edit(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
        edited_srt: &str,
    ) -> io::Result<TranscriptAuditBundle> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        let mut bundle = Self::load_from_dir_unlocked(source.clone(), &dir).await?;
        if edited_srt == bundle.corrected_srt {
            return Ok(bundle);
        }
        if bundle
            .corrections
            .iter()
            .any(|correction| correction.decision == ReviewDecision::Pending)
        {
            return Err(invalid_input(
                "resolve pending transcript corrections before manual editing",
            ));
        }

        let correction =
            manual_correction(&source, &bundle.corrected_srt, edited_srt, "manual_edit")?;
        bundle.corrected_srt = edited_srt.to_string();
        bundle.corrections.push(correction);
        Self::write_mutable_artifacts(&dir, &bundle.corrected_srt, &bundle.corrections).await?;
        bundle.pending_critical_count = pending_critical_count(&bundle.corrections);
        Ok(bundle)
    }

    pub async fn initialize_manual_import(
        source: TranscriptSource,
        dir: impl AsRef<Path>,
        manual_srt: &str,
    ) -> io::Result<TranscriptAuditBundle> {
        let dir = dir.as_ref().to_path_buf();
        let _guard = lock_directory(&dir).await;
        Self::recover_pending_batch(&dir).await?;
        if Self::has_existing_artifacts(&dir).await? {
            return Err(invalid_input(
                "manual import requires a transcript directory without existing artifacts",
            ));
        }
        let correction = manual_correction(&source, "", manual_srt, "manual_import")?;
        Self::initialize_unlocked(source, &dir, "", manual_srt, vec![correction]).await
    }

    async fn write_mutable_artifacts(
        dir: &Path,
        corrected_srt: &str,
        corrections: &[TranscriptCorrection],
    ) -> io::Result<()> {
        let corrections =
            serde_json::to_vec_pretty(corrections).map_err(invalid_correction_json)?;
        Self::write_artifact_batch(
            dir,
            vec![
                ArtifactWrite {
                    target: dir.join(Self::CORRECTED),
                    value: corrected_srt.as_bytes().to_vec(),
                },
                ArtifactWrite {
                    target: dir.join(Self::CORRECTIONS),
                    value: corrections,
                },
                ArtifactWrite {
                    target: dir.join(Self::COMPATIBILITY_SUBTITLE),
                    value: corrected_srt.as_bytes().to_vec(),
                },
            ],
        )
        .await
    }

    async fn archive_raw_generation(dir: &Path, raw_srt: &str) -> io::Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?
            .as_millis();
        let mut sequence = 0;
        loop {
            let suffix = if sequence == 0 {
                timestamp.to_string()
            } else {
                format!("{timestamp}.{sequence}")
            };
            let path = dir.join(format!("subtitle.raw.{suffix}.srt"));
            if !tokio::fs::try_exists(&path).await? {
                return tokio::fs::write(path, raw_srt).await;
            }
            sequence += 1;
        }
    }

    async fn write_artifact_batch(dir: &Path, writes: Vec<ArtifactWrite>) -> io::Result<()> {
        let mut manifest = BatchManifest {
            entries: writes
                .iter()
                .map(|write| {
                    Ok(BatchManifestEntry {
                        name: write
                            .target
                            .file_name()
                            .and_then(|name| name.to_str())
                            .ok_or_else(|| invalid_input("artifact path has no valid file name"))?
                            .to_string(),
                        existed: false,
                    })
                })
                .collect::<io::Result<Vec<_>>>()?,
        };
        for (entry, write) in manifest.entries.iter_mut().zip(&writes) {
            entry.existed = tokio::fs::try_exists(&write.target).await?;
            if let Err(error) =
                tokio::fs::write(write.target.with_extension("tmp"), &write.value).await
            {
                Self::remove_temporary_files(&writes).await;
                return Err(error);
            }
        }

        if let Err(error) = Self::publish_batch_marker(dir, &manifest).await {
            Self::remove_temporary_files(&writes).await;
            return Err(error);
        }
        let marker = Self::marker_path(dir, Self::BATCH_MARKER)?;

        for write in &writes {
            let backup_result = async {
                let backup = write.target.with_extension("bak");
                if tokio::fs::try_exists(&backup).await? {
                    tokio::fs::remove_file(&backup).await?;
                }
                if tokio::fs::try_exists(&write.target).await? {
                    tokio::fs::rename(&write.target, &backup).await?;
                }
                Ok(())
            }
            .await;
            if let Err(error) = backup_result {
                return Self::rollback_batch(dir, &manifest, &writes, error).await;
            }
        }

        for (_publish_index, write) in writes.iter().enumerate() {
            #[cfg(test)]
            if Self::should_fail_batch_publish_for_test(dir, _publish_index) {
                return Self::rollback_batch(
                    dir,
                    &manifest,
                    &writes,
                    io::Error::new(io::ErrorKind::Other, "injected batch publish failure"),
                )
                .await;
            }
            if let Err(error) =
                tokio::fs::rename(write.target.with_extension("tmp"), &write.target).await
            {
                return Self::rollback_batch(dir, &manifest, &writes, error).await;
            }
        }

        if let Err(error) = tokio::fs::remove_file(&marker).await {
            return Self::rollback_batch(dir, &manifest, &writes, error).await;
        }
        for write in &writes {
            let backup = write.target.with_extension("bak");
            if tokio::fs::try_exists(&backup).await? {
                let _ = tokio::fs::remove_file(backup).await;
            }
        }
        Ok(())
    }

    async fn rollback_batch(
        dir: &Path,
        manifest: &BatchManifest,
        writes: &[ArtifactWrite],
        original_error: io::Error,
    ) -> io::Result<()> {
        match Self::restore_batch_from_manifest(dir, manifest).await {
            Ok(()) => {
                Self::remove_temporary_files(writes).await;
                tokio::fs::remove_file(dir.join(Self::BATCH_MARKER)).await?;
                Err(original_error)
            }
            Err(rollback_error) => Err(io::Error::new(
                original_error.kind(),
                format!("{original_error}; batch rollback failed: {rollback_error}"),
            )),
        }
    }

    async fn recover_pending_batch(dir: &Path) -> io::Result<()> {
        let marker = Self::marker_path(dir, Self::BATCH_MARKER)?;
        let marker_temp = Self::marker_path(dir, Self::BATCH_MARKER_TEMP)?;
        let manifest = match tokio::fs::read_to_string(&marker).await {
            Ok(value) => {
                let manifest = serde_json::from_str(&value).map_err(invalid_correction_json)?;
                validate_batch_manifest(&manifest)?;
                manifest
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Self::remove_orphan_marker_temp(&marker_temp).await?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        Self::remove_orphan_marker_temp(&marker_temp).await?;
        Self::restore_batch_from_manifest(dir, &manifest).await?;
        tokio::fs::remove_file(marker).await
    }

    async fn publish_batch_marker(dir: &Path, manifest: &BatchManifest) -> io::Result<()> {
        let marker = Self::marker_path(dir, Self::BATCH_MARKER)?;
        let marker_temp = Self::marker_path(dir, Self::BATCH_MARKER_TEMP)?;
        let marker_bytes = serde_json::to_vec(manifest).map_err(invalid_correction_json)?;
        let mut marker_file = tokio::fs::File::create(&marker_temp).await?;
        let write_result = async {
            marker_file.write_all(&marker_bytes).await?;
            marker_file.flush().await?;
            marker_file.sync_all().await
        }
        .await;
        if let Err(error) = write_result {
            drop(marker_file);
            let _ = tokio::fs::remove_file(marker_temp).await;
            return Err(error);
        }
        drop(marker_file);
        tokio::fs::rename(marker_temp, marker).await
    }

    async fn remove_orphan_marker_temp(marker_temp: &Path) -> io::Result<()> {
        if tokio::fs::try_exists(marker_temp).await? {
            tokio::fs::remove_file(marker_temp).await?;
        }
        Ok(())
    }

    fn marker_path(dir: &Path, name: &str) -> io::Result<PathBuf> {
        if !matches!(name, Self::BATCH_MARKER | Self::BATCH_MARKER_TEMP)
            || !is_single_normal_component(name)
        {
            return Err(invalid_manifest("invalid transaction marker path"));
        }
        Ok(dir.join(name))
    }

    async fn restore_batch_from_manifest(dir: &Path, manifest: &BatchManifest) -> io::Result<()> {
        for entry in &manifest.entries {
            let target = dir.join(&entry.name);
            let backup = target.with_extension("bak");
            if tokio::fs::try_exists(&backup).await? {
                if tokio::fs::try_exists(&target).await? {
                    tokio::fs::remove_file(&target).await?;
                }
                tokio::fs::rename(backup, target).await?;
            } else if !entry.existed && tokio::fs::try_exists(&target).await? {
                tokio::fs::remove_file(target).await?;
            }
            let temporary = dir.join(&entry.name).with_extension("tmp");
            if tokio::fs::try_exists(&temporary).await? {
                tokio::fs::remove_file(temporary).await?;
            }
        }
        Ok(())
    }

    async fn remove_temporary_files(writes: &[ArtifactWrite]) {
        for write in writes {
            let temporary = write.target.with_extension("tmp");
            if tokio::fs::try_exists(&temporary).await.unwrap_or(false) {
                let _ = tokio::fs::remove_file(temporary).await;
            }
        }
    }

    async fn read_with_fallback(dir: &Path, canonical: &str, legacy: &str) -> io::Result<String> {
        match tokio::fs::read_to_string(dir.join(canonical)).await {
            Ok(value) => Ok(value),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match tokio::fs::read_to_string(dir.join(legacy)).await {
                    Ok(value) => Ok(value),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn restore_backup_if_needed(path: &Path) -> io::Result<()> {
        let backup = path.with_extension("bak");
        if !tokio::fs::try_exists(path).await? && tokio::fs::try_exists(&backup).await? {
            tokio::fs::rename(backup, path).await?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn fail_batch_publish_for_test(dir: &Path, publish_index: usize) {
        batch_failures()
            .lock()
            .unwrap()
            .insert(dir.to_path_buf(), publish_index);
    }

    #[cfg(test)]
    fn should_fail_batch_publish_for_test(dir: &Path, publish_index: usize) -> bool {
        batch_failures()
            .lock()
            .unwrap()
            .get(dir)
            .is_some_and(|expected| *expected == publish_index)
    }
}

async fn read_optional(path: impl AsRef<Path>) -> io::Result<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

async fn read_optional_json(path: impl AsRef<Path>) -> Option<Value> {
    let value = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&value).ok()
}

async fn read_funasr_proposals(path: impl AsRef<Path>) -> io::Result<Vec<CorrectionProposal>> {
    let Some(value) = read_optional(path).await? else {
        return Ok(Vec::new());
    };
    let changes: Vec<LegacyCorrectionChange> =
        serde_json::from_str(&value).map_err(invalid_correction_json)?;
    Ok(changes
        .into_iter()
        .filter(|change| !change.source.trim().is_empty() && !change.corrected.trim().is_empty())
        .map(|change| CorrectionProposal {
            source: change.source,
            corrected: change.corrected,
            category: default_category(change.change_type),
            evidence: non_empty_evidence(change.evidence),
            needs_review: change.needs_review,
            start_ms: None,
            provider_critical: false,
        })
        .collect())
}

async fn read_volcengine_proposals(media_file: &Path) -> io::Result<Vec<CorrectionProposal>> {
    let audit_path = TranscriptArtifactStore::asr_audit_path(media_file);
    let value = match read_optional(&audit_path).await {
        Ok(Some(value)) => value,
        Ok(None) | Err(_) => return Ok(Vec::new()),
    };
    let audit: VolcengineAudit = match serde_json::from_str(&value) {
        Ok(audit) => audit,
        Err(_) => return Ok(Vec::new()),
    };
    Ok(audit
        .changes
        .into_iter()
        .filter(|change| !change.original.trim().is_empty() && !change.corrected.trim().is_empty())
        .map(|change| CorrectionProposal {
            source: change.original,
            corrected: change.corrected,
            category: default_category(change.rule),
            evidence: non_empty_evidence(change.evidence),
            needs_review: true,
            start_ms: Some(change.start_ms),
            provider_critical: true,
        })
        .collect())
}

fn prepare_asr_artifacts(
    raw_sidecar: Option<String>,
    deterministic_srt: String,
    proposals: Vec<CorrectionProposal>,
    fact_card: Option<&Value>,
) -> io::Result<(String, String, Vec<TranscriptCorrection>)> {
    let raw_sidecar = raw_sidecar.filter(|value| !value.trim().is_empty());
    let has_raw_sidecar = raw_sidecar.is_some();
    let mut raw_srt = raw_sidecar.unwrap_or_else(|| deterministic_srt.clone());
    let raw_cues = parse_srt_cues(&raw_srt);
    let corrected_cues = parse_srt_cues(&deterministic_srt);
    let validated_explicit_starts = proposals
        .iter()
        .filter_map(|proposal| {
            let start_ms = proposal.start_ms?;
            corrected_cues
                .iter()
                .any(|cue| cue.start_ms == start_ms && cue.text.trim() == proposal.corrected.trim())
                .then_some(start_ms)
        })
        .collect::<HashSet<_>>();
    let mut occurrences = HashMap::<(String, String), usize>::new();
    let mut derived_locations = HashMap::<String, Vec<(u64, u64)>>::new();
    let mut located = Vec::new();

    for proposal in proposals {
        if proposal
            .start_ms
            .is_some_and(|start_ms| !validated_explicit_starts.contains(&start_ms))
        {
            continue;
        }
        let key = (proposal.source.clone(), proposal.corrected.clone());
        let occurrence = occurrences.entry(key).or_default();
        let Some((start_ms, end_ms)) = locate_proposal(
            &proposal,
            &raw_cues,
            &corrected_cues,
            &derived_locations,
            *occurrence,
        ) else {
            continue;
        };
        *occurrence += 1;
        derived_locations
            .entry(proposal.corrected.clone())
            .or_default()
            .push((start_ms, end_ms));
        let supported =
            replacement_is_supported(&proposal.corrected, &proposal.evidence, fact_card);
        let formatting_only = is_deterministic_formatting_only(&proposal);
        let critical = proposal.provider_critical || !formatting_only;
        let pending = critical || proposal.needs_review || !supported;
        located.push(LocatedCorrection {
            proposal,
            start_ms,
            end_ms,
            critical,
            supported,
            pending,
        });
    }

    if !has_raw_sidecar {
        for correction in located.iter().rev() {
            raw_srt = replace_cue_text(
                &raw_srt,
                correction.start_ms,
                correction.end_ms,
                &correction.proposal.corrected,
                &correction.proposal.source,
            )?;
        }
    }

    let located = collapse_cue_corrections(located, &raw_srt, &deterministic_srt)?;

    let mut safe_corrected_srt = deterministic_srt;
    let raw_cues = parse_srt_cues(&raw_srt);
    let mut restored_cues = HashSet::new();
    for correction in located.iter().filter(|correction| correction.pending) {
        let timing = (correction.start_ms, correction.end_ms);
        if !restored_cues.insert(timing) {
            continue;
        }
        let raw_text = cue_text_at(&raw_cues, timing)
            .ok_or_else(|| invalid_input("pending correction has no matching raw cue"))?;
        let current_cues = parse_srt_cues(&safe_corrected_srt);
        let current_text = cue_text_at(&current_cues, timing)
            .ok_or_else(|| invalid_input("pending correction has no matching corrected cue"))?;
        safe_corrected_srt = replace_cue_text(
            &safe_corrected_srt,
            timing.0,
            timing.1,
            current_text,
            raw_text,
        )?;
    }

    let corrections = located
        .into_iter()
        .map(|correction| {
            let displayed_proposal =
                if correction.supported || is_pending_placeholder(&correction.proposal.corrected) {
                    correction.proposal.corrected.clone()
                } else {
                    "[待确认]".to_string()
                };
            let decision = if correction.pending {
                ReviewDecision::Pending
            } else {
                ReviewDecision::Approved
            };
            TranscriptCorrection {
                id: stable_correction_id(
                    correction.start_ms,
                    correction.end_ms,
                    &correction.proposal.source,
                    &correction.proposal.corrected,
                    &correction.proposal.category,
                ),
                start_ms: correction.start_ms,
                end_ms: correction.end_ms,
                original: correction.proposal.source,
                proposed: displayed_proposal.clone(),
                category: correction.proposal.category,
                evidence: correction.proposal.evidence,
                critical: correction.critical,
                decided_text: (decision == ReviewDecision::Approved).then_some(displayed_proposal),
                decision,
            }
        })
        .collect();

    Ok((raw_srt, safe_corrected_srt, corrections))
}

fn collapse_cue_corrections(
    located: Vec<LocatedCorrection>,
    raw_srt: &str,
    corrected_srt: &str,
) -> io::Result<Vec<LocatedCorrection>> {
    let raw_cues = parse_srt_cues(raw_srt);
    let corrected_cues = parse_srt_cues(corrected_srt);
    let mut group_indexes = HashMap::<(u64, u64), usize>::new();
    let mut groups = Vec::<Vec<LocatedCorrection>>::new();

    for correction in located {
        let timing = (correction.start_ms, correction.end_ms);
        let group_index = match group_indexes.get(&timing) {
            Some(index) => *index,
            None => {
                let index = groups.len();
                group_indexes.insert(timing, index);
                groups.push(Vec::new());
                index
            }
        };
        groups[group_index].push(correction);
    }

    groups
        .into_iter()
        .map(|group| {
            let timing = (group[0].start_ms, group[0].end_ms);
            let original = cue_text_at(&raw_cues, timing)
                .ok_or_else(|| invalid_input("correction chain has no matching raw cue"))?
                .to_string();
            let corrected = cue_text_at(&corrected_cues, timing)
                .ok_or_else(|| invalid_input("correction chain has no matching corrected cue"))?
                .to_string();
            let mut categories = Vec::new();
            let mut evidence = Vec::new();
            for correction in &group {
                if !categories.contains(&correction.proposal.category) {
                    categories.push(correction.proposal.category.clone());
                }
                for item in &correction.proposal.evidence {
                    if !evidence.contains(item) {
                        evidence.push(item.clone());
                    }
                }
            }
            let critical = group.iter().any(|correction| correction.critical);
            let supported = group.iter().all(|correction| correction.supported);
            let pending = group.iter().any(|correction| correction.pending);
            Ok(LocatedCorrection {
                proposal: CorrectionProposal {
                    source: original,
                    corrected,
                    category: categories.join(" + "),
                    evidence,
                    needs_review: group
                        .iter()
                        .any(|correction| correction.proposal.needs_review),
                    start_ms: Some(timing.0),
                    provider_critical: group
                        .iter()
                        .any(|correction| correction.proposal.provider_critical),
                },
                start_ms: timing.0,
                end_ms: timing.1,
                critical,
                supported,
                pending,
            })
        })
        .collect()
}

fn locate_proposal(
    proposal: &CorrectionProposal,
    raw_cues: &[SrtCue],
    corrected_cues: &[SrtCue],
    derived_locations: &HashMap<String, Vec<(u64, u64)>>,
    occurrence: usize,
) -> Option<(u64, u64)> {
    if let Some(start_ms) = proposal.start_ms {
        return corrected_cues
            .iter()
            .find(|cue| {
                cue.start_ms == start_ms
                    && (cue.text.contains(&proposal.corrected)
                        || cue.text.contains(&proposal.source))
            })
            .or_else(|| corrected_cues.iter().find(|cue| cue.start_ms == start_ms))
            .map(|cue| (cue.start_ms, cue.end_ms));
    }

    let raw_matches = raw_cues
        .iter()
        .filter(|cue| cue.text.contains(&proposal.source))
        .map(|cue| (cue.start_ms, cue.end_ms))
        .collect::<Vec<_>>();
    if let Some(timing) = raw_matches.get(occurrence) {
        return Some(*timing);
    }
    if let Some(timing) = derived_locations
        .get(&proposal.source)
        .and_then(|locations| locations.get(occurrence))
    {
        return Some(*timing);
    }

    corrected_cues
        .iter()
        .filter(|cue| cue.text.contains(&proposal.corrected))
        .nth(occurrence)
        .map(|cue| (cue.start_ms, cue.end_ms))
}

fn parse_srt_cues(srt: &str) -> Vec<SrtCue> {
    let lines = srt_line_ranges(srt);
    let mut cues = Vec::new();
    for (index, (line_start, line_end, _)) in lines.iter().copied().enumerate() {
        let Some((start_ms, end_ms)) = parse_srt_timing(&srt[line_start..line_end]) else {
            continue;
        };
        let Some((body_start, _, _)) = lines.get(index + 1).copied() else {
            continue;
        };
        let body_end = lines[index + 1..]
            .iter()
            .find(|(start, end, _)| srt[*start..*end].trim().is_empty())
            .map(|(start, _, _)| *start)
            .unwrap_or(srt.len());
        cues.push(SrtCue {
            start_ms,
            end_ms,
            text: srt[body_start..body_end]
                .trim_end_matches(['\r', '\n'])
                .to_string(),
        });
    }
    cues
}

fn cue_text_at(cues: &[SrtCue], timing: (u64, u64)) -> Option<&str> {
    cues.iter()
        .find(|cue| cue.start_ms == timing.0 && cue.end_ms == timing.1)
        .map(|cue| cue.text.as_str())
}

fn replacement_is_supported(
    replacement: &str,
    evidence: &[String],
    fact_card: Option<&Value>,
) -> bool {
    if is_pending_placeholder(replacement) {
        return false;
    }
    evidence.iter().any(|item| item.contains(replacement))
        || fact_card.is_some_and(|card| fact_card_contains(card, replacement))
}

fn fact_card_contains(value: &Value, replacement: &str) -> bool {
    match value {
        Value::String(value) => value.trim() == replacement || value.contains(replacement),
        Value::Number(value) => value.to_string() == replacement,
        Value::Array(values) => values
            .iter()
            .any(|value| fact_card_contains(value, replacement)),
        Value::Object(values) => values
            .values()
            .any(|value| fact_card_contains(value, replacement)),
        _ => false,
    }
}

fn is_pending_placeholder(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('[')
        && value.ends_with(']')
        && (value.contains("待确认") || value.contains("听不清") || value.contains("疑似"))
}

fn is_deterministic_formatting_only(proposal: &CorrectionProposal) -> bool {
    if proposal
        .source
        .chars()
        .chain(proposal.corrected.chars())
        .any(|character| character.is_ascii_alphabetic() || character.is_ascii_digit())
    {
        return false;
    }
    let category = proposal.category.trim().to_lowercase();
    let allowlisted_category = matches!(
        category.as_str(),
        "punctuation"
            | "whitespace"
            | "formatting"
            | "标点"
            | "标点符号"
            | "空格"
            | "空白"
            | "格式"
    );
    allowlisted_category
        && proposal.source != proposal.corrected
        && formatting_signature(&proposal.source) == formatting_signature(&proposal.corrected)
}

fn formatting_signature(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace() && !is_safe_sentence_punctuation(*character))
        .collect()
}

fn is_safe_sentence_punctuation(character: char) -> bool {
    matches!(character, '，' | '。' | '；' | '：' | '？' | '！')
}

fn stable_correction_id(
    start_ms: u64,
    end_ms: u64,
    source: &str,
    corrected: &str,
    category: &str,
) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in [
        start_ms.to_string(),
        end_ms.to_string(),
        source.to_string(),
        corrected.to_string(),
        category.to_string(),
    ] {
        for byte in value.bytes().chain(std::iter::once(0xff)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("correction-{start_ms}-{hash:016x}")
}

fn manual_correction(
    source: &TranscriptSource,
    original: &str,
    edited: &str,
    category: &str,
) -> io::Result<TranscriptCorrection> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?
        .as_millis();
    let source_evidence = match source {
        TranscriptSource::Video { video_id } => format!("source=video:{video_id}"),
        TranscriptSource::Archive {
            platform,
            room_id,
            live_id,
        } => format!("source=archive:{platform}:{room_id}:{live_id}"),
    };
    let end_ms = parse_srt_cues(edited)
        .last()
        .map(|cue| cue.end_ms)
        .unwrap_or(0);
    Ok(TranscriptCorrection {
        id: stable_correction_id(
            0,
            end_ms,
            original,
            edited,
            &format!("{category}:{timestamp}"),
        ),
        start_ms: 0,
        end_ms,
        original: original.to_string(),
        proposed: edited.to_string(),
        category: category.to_string(),
        evidence: vec![
            source_evidence,
            format!("timestamp_ms={timestamp}"),
            "decision=approved".to_string(),
            "editor=video_subtitle".to_string(),
        ],
        critical: false,
        decision: ReviewDecision::Approved,
        decided_text: Some(edited.to_string()),
    })
}

fn default_category(value: String) -> String {
    if value.trim().is_empty() {
        "provider_correction".to_string()
    } else {
        value
    }
}

fn non_empty_evidence(value: String) -> Vec<String> {
    if value.trim().is_empty() {
        Vec::new()
    } else {
        vec![value]
    }
}

fn directory_mutex(dir: &Path) -> Arc<AsyncMutex<()>> {
    static DIRECTORY_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> =
        OnceLock::new();
    let locks = DIRECTORY_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().unwrap();
    locks
        .entry(dir.to_path_buf())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

async fn lock_directory(dir: &Path) -> OwnedMutexGuard<()> {
    directory_mutex(dir).lock_owned().await
}

fn validate_batch_manifest(manifest: &BatchManifest) -> io::Result<()> {
    let mut names = HashSet::new();
    for entry in &manifest.entries {
        if !matches!(
            entry.name.as_str(),
            "subtitle.raw.srt"
                | "subtitle.corrected.srt"
                | "subtitle.corrections.json"
                | "subtitle.srt"
                | "subtitle.source.json"
        ) || !is_single_normal_component(&entry.name)
            || !names.insert(&entry.name)
        {
            return Err(invalid_manifest("invalid transaction manifest entry"));
        }
    }
    Ok(())
}

fn is_single_normal_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn replace_cue_text(
    srt: &str,
    start_ms: u64,
    end_ms: u64,
    original: &str,
    replacement: &str,
) -> io::Result<String> {
    if original.is_empty() {
        return Err(invalid_input("correction original text cannot be empty"));
    }

    let lines = srt_line_ranges(srt);
    let mut replacement_range = None;
    for (index, (line_start, line_end, content_end)) in lines.iter().copied().enumerate() {
        let line = &srt[line_start..content_end];
        if parse_srt_timing(line) != Some((start_ms, end_ms)) {
            continue;
        }
        if replacement_range.is_some() {
            return Err(invalid_input(
                "correction timing matches multiple subtitle cues",
            ));
        }

        let body_start = line_end;
        let body_end = lines[index + 1..]
            .iter()
            .find(|(next_start, _, next_content_end)| {
                srt[*next_start..*next_content_end].trim().is_empty()
            })
            .map_or(srt.len(), |(next_start, _, _)| *next_start);
        let body = &srt[body_start..body_end];
        let mut occurrences = overlapping_match_offsets(body, original);
        let offset = occurrences.next().ok_or_else(|| {
            invalid_input("correction original text is not present in the timestamped subtitle cue")
        })?;
        if occurrences.next().is_some() {
            return Err(invalid_input(
                "correction original text occurs more than once in the timestamped subtitle cue",
            ));
        }
        replacement_range = Some(body_start + offset..body_start + offset + original.len());
    }

    let replacement_range = replacement_range
        .ok_or_else(|| invalid_input("correction timing does not match a subtitle cue"))?;
    let mut updated = srt.to_string();
    updated.replace_range(replacement_range, replacement);
    Ok(updated)
}

fn overlapping_match_offsets<'a>(
    value: &'a str,
    needle: &'a str,
) -> impl Iterator<Item = usize> + 'a {
    let mut offsets = Vec::new();
    let mut search_from = 0;
    while let Some(offset) = value[search_from..].find(needle) {
        let match_start = search_from + offset;
        offsets.push(match_start);
        search_from = match_start
            + value[match_start..]
                .chars()
                .next()
                .expect("non-empty match start")
                .len_utf8();
    }
    offsets.into_iter()
}

fn srt_line_ranges(srt: &str) -> Vec<(usize, usize, usize)> {
    let mut lines = Vec::new();
    let mut start = 0;
    for line in srt.split_inclusive('\n') {
        let end = start + line.len();
        let content_end = if line.ends_with("\r\n") {
            end - 2
        } else if line.ends_with('\n') {
            end - 1
        } else {
            end
        };
        lines.push((start, end, content_end));
        start = end;
    }
    lines
}

fn parse_srt_timing(line: &str) -> Option<(u64, u64)> {
    let (start, end) = line.split_once("-->")?;
    Some((parse_srt_time(start.trim())?, parse_srt_time(end.trim())?))
}

fn parse_srt_time(value: &str) -> Option<u64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds_and_millis = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let (seconds, millis) = seconds_and_millis
        .split_once(',')
        .or_else(|| seconds_and_millis.split_once('.'))?;
    let seconds = seconds.parse::<u64>().ok()?;
    let millis = millis.parse::<u64>().ok()?;
    if minutes >= 60 || seconds >= 60 || millis >= 1_000 {
        return None;
    }
    Some(((hours * 60 + minutes) * 60 + seconds) * 1_000 + millis)
}

#[cfg(test)]
fn batch_failures() -> &'static Mutex<HashMap<PathBuf, usize>> {
    static BATCH_FAILURES: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
    BATCH_FAILURES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_critical_count(corrections: &[TranscriptCorrection]) -> usize {
    corrections
        .iter()
        .filter(|correction| correction.critical && correction.decision == ReviewDecision::Pending)
        .count()
}

fn invalid_correction_json(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn invalid_manifest(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn archive_and_video_initialization_write_the_same_artifact_set() {
        for source in [
            TranscriptSource::Archive {
                platform: "bilibili".into(),
                room_id: "1".into(),
                live_id: "2".into(),
            },
            TranscriptSource::Video { video_id: 7 },
        ] {
            let dir = tempfile::tempdir().unwrap();
            TranscriptArtifactStore::initialize(source, dir.path(), "raw", "corrected", vec![])
                .await
                .unwrap();

            for name in [
                "subtitle.raw.srt",
                "subtitle.corrected.srt",
                "subtitle.corrections.json",
                "subtitle.srt",
            ] {
                assert!(dir.path().join(name).exists(), "missing {name}");
            }
        }
    }

    #[tokio::test]
    async fn funasr_critical_change_is_pending_and_excluded_from_safe_srt() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let artifacts = root.path().join("artifacts");
        let raw = one_cue_srt("影石A4PRO299新");
        let corrected = one_cue_srt("影石A4PRO2 99新");
        tokio::fs::write(media.with_extension("asr.raw.srt"), &raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), &corrected)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([{
                "原文": "影石A4PRO299新",
                "校对稿": "影石A4PRO2 99新",
                "修改类型": "型号成色边界",
                "依据": "确定性规则和事实卡：影石A4PRO2 99新",
                "是否需要人工确认": false
            }]))
            .unwrap(),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 7 },
            &artifacts,
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, raw);
        assert_eq!(bundle.corrected_srt, raw);
        assert_eq!(bundle.pending_critical_count, 1);
        assert_eq!(bundle.corrections.len(), 1);
        let correction = &bundle.corrections[0];
        assert_eq!(correction.start_ms, 0);
        assert_eq!(correction.end_ms, 1_000);
        assert_eq!(correction.proposed, "影石A4PRO2 99新");
        assert!(correction.critical);
        assert_eq!(correction.decision, ReviewDecision::Pending);
        assert_eq!(
            correction.evidence,
            vec!["确定性规则和事实卡：影石A4PRO2 99新"]
        );
    }

    #[tokio::test]
    async fn unsupported_commercial_replacement_becomes_unapplied_placeholder() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let raw = one_cue_srt("优惠完价5830");
        let corrected = one_cue_srt("优惠完价5999");
        tokio::fs::write(media.with_extension("asr.raw.srt"), &raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), &corrected)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([{
                "原文": "5830",
                "校对稿": "5999",
                "修改类型": "价格",
                "依据": "MiniMax建议",
                "是否需要人工确认": false
            }]))
            .unwrap(),
        )
        .await
        .unwrap();
        tokio::fs::write(media.with_extension("facts.json"), br#"{"prices":[5839]}"#)
            .await
            .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 7 },
            root.path().join("artifacts"),
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrected_srt, raw);
        assert_eq!(bundle.corrections[0].proposed, "[待确认]");
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
        assert!(bundle.corrections[0].critical);
    }

    #[tokio::test]
    async fn supported_noncritical_change_stays_applied_with_a_stable_id() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let raw = one_cue_srt("大家好");
        let corrected = one_cue_srt("大家好，");
        tokio::fs::write(media.with_extension("asr.raw.srt"), &raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), &corrected)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([{
                "原文": "大家好",
                "校对稿": "大家好，",
                "修改类型": "标点",
                "依据": "确定性标点规则：大家好，",
                "是否需要人工确认": false
            }]))
            .unwrap(),
        )
        .await
        .unwrap();

        let first = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 7 },
            root.path().join("first"),
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();
        let second = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 7 },
            root.path().join("second"),
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();

        assert_eq!(first.corrected_srt, corrected);
        assert!(!first.corrections[0].critical);
        assert_eq!(first.corrections[0].decision, ReviewDecision::Approved);
        assert_eq!(
            first.corrections[0].decided_text.as_deref(),
            Some("大家好，")
        );
        assert_eq!(first.corrections[0].id, second.corrections[0].id);
    }

    #[tokio::test]
    async fn commercial_separator_model_hyphen_change_is_pending_and_unapplied() {
        assert_formatting_change_pending("R6-2", "R62").await;
    }

    #[tokio::test]
    async fn commercial_separator_decimal_change_is_pending_and_unapplied() {
        assert_formatting_change_pending("5839", "58.39").await;
    }

    #[tokio::test]
    async fn commercial_separator_numeric_hyphen_change_is_pending_and_unapplied() {
        assert_formatting_change_pending("5-6", "56").await;
    }

    #[tokio::test]
    async fn commercial_separator_slash_change_is_pending_and_unapplied() {
        assert_formatting_change_pending("1/2年", "12年").await;
    }

    #[tokio::test]
    async fn neutral_fact_card_product_name_change_is_pending_and_unapplied() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let raw = one_cue_srt("小白图");
        let corrected = one_cue_srt("小白兔");
        tokio::fs::write(media.with_extension("asr.raw.srt"), &raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), &corrected)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([{
                "原文": "小白图",
                "校对稿": "小白兔",
                "修改类型": "neutral",
                "依据": "事实卡",
                "是否需要人工确认": false
            }]))
            .unwrap(),
        )
        .await
        .unwrap();
        tokio::fs::write(
            media.with_extension("facts.json"),
            serde_json::to_vec(&serde_json::json!({"products": ["小白兔"]})).unwrap(),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 10 },
            root.path().join("artifacts"),
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrected_srt, raw);
        assert_eq!(bundle.corrections.len(), 1);
        assert_eq!(bundle.corrections[0].original, "小白图");
        assert_eq!(bundle.corrections[0].proposed, "小白兔");
        assert!(bundle.corrections[0].critical);
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
        assert_eq!(bundle.corrections[0].decided_text, None);
    }

    #[tokio::test]
    async fn chained_provider_and_model_changes_collapse_to_one_cue_correction() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let raw = one_cue_srt("佳能二六二");
        let corrected = one_cue_srt("佳能R62");
        tokio::fs::write(media.with_extension("asr.raw.srt"), &raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), &corrected)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([
                {
                    "原文": "佳能二六二",
                    "校对稿": "佳能R六二",
                    "修改类型": "商品型号",
                    "依据": "本地词表：佳能R六二",
                    "是否需要人工确认": false
                },
                {
                    "原文": "佳能R六二",
                    "校对稿": "佳能R62",
                    "修改类型": "MiniMax商品型号",
                    "依据": "事实卡：佳能R62",
                    "是否需要人工确认": false
                }
            ]))
            .unwrap(),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 7 },
            root.path().join("artifacts"),
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrected_srt, raw);
        assert_eq!(bundle.corrections.len(), 1);
        let correction = &bundle.corrections[0];
        assert_eq!(correction.start_ms, 0);
        assert_eq!(correction.end_ms, 1_000);
        assert_eq!(correction.original, "佳能二六二");
        assert_eq!(correction.proposed, "佳能R62");
        assert_eq!(
            correction.evidence,
            vec!["本地词表：佳能R六二", "事实卡：佳能R62"]
        );
        assert!(correction.category.contains("商品型号"));
        assert!(correction.category.contains("MiniMax商品型号"));
        assert_eq!(correction.decision, ReviewDecision::Pending);
    }

    #[tokio::test]
    async fn collapsed_chain_can_be_approved_without_predecessor_resolution() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let artifacts = root.path().join("artifacts");
        let raw = one_cue_srt("佳能二六二");
        let corrected = one_cue_srt("佳能R62");
        write_chained_funasr_fixture(&media, &raw, &corrected).await;

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 11 },
            &artifacts,
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();
        assert_eq!(bundle.corrections.len(), 1);

        let resolved = TranscriptArtifactStore::resolve(
            TranscriptSource::Video { video_id: 11 },
            &artifacts,
            &bundle.corrections[0].id,
            ReviewAction::Approve,
            Some("佳能R62".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(resolved.corrected_srt, corrected);
        assert_eq!(resolved.corrections[0].decision, ReviewDecision::Approved);
    }

    #[tokio::test]
    async fn collapsed_chain_can_keep_original_without_predecessor_resolution() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let artifacts = root.path().join("artifacts");
        let raw = one_cue_srt("佳能二六二");
        let corrected = one_cue_srt("佳能R62");
        write_chained_funasr_fixture(&media, &raw, &corrected).await;

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 12 },
            &artifacts,
            &media,
            &corrected,
            "funasr",
        )
        .await
        .unwrap();
        assert_eq!(bundle.corrections.len(), 1);

        let resolved = TranscriptArtifactStore::resolve(
            TranscriptSource::Video { video_id: 12 },
            &artifacts,
            &bundle.corrections[0].id,
            ReviewAction::KeepOriginal,
            None,
        )
        .await
        .unwrap();

        assert_eq!(resolved.corrected_srt, raw);
        assert_eq!(
            resolved.corrections[0].decision,
            ReviewDecision::KeptOriginal
        );
    }

    #[tokio::test]
    async fn volcengine_audit_reconstructs_raw_and_keeps_entity_change_pending() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let corrected = one_cue_srt("A7M4没有准新");
        tokio::fs::write(
            TranscriptArtifactStore::asr_audit_path(&media),
            serde_json::to_vec(&serde_json::json!({
                "version": "evidence-v1",
                "changes": [{
                    "subtitle_position": 1,
                    "start_ms": 0,
                    "original": "a 七 m 四没有准新",
                    "corrected": "A7M4没有准新",
                    "evidence": "A7M4有准新嘛",
                    "evidence_timestamp_ms": 1010,
                    "confidence": 0.99,
                    "rule": "nearby_danmu_entity_equivalence"
                }],
                "quality_flags": []
            }))
            .unwrap(),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 7 },
            root.path().join("artifacts"),
            &media,
            &corrected,
            "volcengine",
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, one_cue_srt("a 七 m 四没有准新"));
        assert_eq!(bundle.corrected_srt, bundle.raw_srt);
        assert_eq!(bundle.corrections[0].start_ms, 0);
        assert_eq!(bundle.corrections[0].end_ms, 1_000);
        assert_eq!(bundle.corrections[0].evidence, vec!["A7M4有准新嘛"]);
        assert!(bundle.corrections[0].critical);
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
    }

    #[tokio::test]
    async fn parent_wide_volcengine_audit_is_ignored_even_when_cue_matches() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let generated = one_cue_srt("A7M4没有准新");
        tokio::fs::write(
            root.path().join("subtitle.audit.json"),
            serde_json::to_vec(&serde_json::json!({
                "changes": [{
                    "start_ms": 0,
                    "original": "a 七 m 四没有准新",
                    "corrected": "A7M4没有准新",
                    "evidence": "A7M4有准新嘛",
                    "rule": "nearby_danmu_entity_equivalence"
                }]
            }))
            .unwrap(),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 8 },
            root.path().join("artifacts"),
            &media,
            &generated,
            "volcengine",
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, generated);
        assert_eq!(bundle.corrected_srt, generated);
        assert!(bundle.corrections.is_empty());
    }

    #[tokio::test]
    async fn source_bound_volcengine_audits_do_not_cross_identical_video_cues() {
        let root = tempfile::tempdir().unwrap();
        let first_media = root.path().join("first.mp4");
        let second_media = root.path().join("second.mp4");
        let generated = one_cue_srt("小白兔");
        for (media, original, evidence) in [
            (&first_media, "小白图", "第一路证据：小白兔"),
            (&second_media, "小白土", "第二路证据：小白兔"),
        ] {
            tokio::fs::write(
                TranscriptArtifactStore::asr_audit_path(media),
                serde_json::to_vec(&serde_json::json!({
                    "changes": [{
                        "start_ms": 0,
                        "original": original,
                        "corrected": "小白兔",
                        "evidence": evidence,
                        "rule": "neutral"
                    }]
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        }

        let first = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 1 },
            root.path().join("first-artifacts"),
            &first_media,
            &generated,
            "volcengine",
        )
        .await
        .unwrap();
        let second = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 2 },
            root.path().join("second-artifacts"),
            &second_media,
            &generated,
            "volcengine",
        )
        .await
        .unwrap();

        assert_eq!(first.corrections[0].original, "小白图");
        assert_eq!(first.corrections[0].evidence, vec!["第一路证据：小白兔"]);
        assert_eq!(second.corrections[0].original, "小白土");
        assert_eq!(second.corrections[0].evidence, vec!["第二路证据：小白兔"]);
    }

    #[tokio::test]
    async fn malformed_source_bound_volcengine_audit_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let generated = one_cue_srt("当前识别结果");
        tokio::fs::write(
            TranscriptArtifactStore::asr_audit_path(&media),
            b"{not-json",
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 3 },
            root.path().join("artifacts"),
            &media,
            &generated,
            "volcengine",
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, generated);
        assert_eq!(bundle.corrected_srt, generated);
        assert!(bundle.corrections.is_empty());
    }

    #[tokio::test]
    async fn unreadable_source_bound_volcengine_audit_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let generated = one_cue_srt("当前识别结果");
        tokio::fs::create_dir(TranscriptArtifactStore::asr_audit_path(&media))
            .await
            .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 4 },
            root.path().join("artifacts"),
            &media,
            &generated,
            "volcengine",
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, generated);
        assert_eq!(bundle.corrected_srt, generated);
        assert!(bundle.corrections.is_empty());
    }

    #[tokio::test]
    async fn non_funasr_generation_ignores_stale_funasr_sidecars() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let generated = one_cue_srt("当前识别结果");
        tokio::fs::write(media.with_extension("asr.raw.srt"), one_cue_srt("旧原稿"))
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.corrected.srt"),
            one_cue_srt("旧校对稿"),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 9 },
            root.path().join("artifacts"),
            &media,
            &generated,
            "whisper",
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, generated);
        assert_eq!(bundle.corrected_srt, generated);
        assert!(bundle.corrections.is_empty());
    }

    #[test]
    fn video_artifact_directories_are_source_specific() {
        let root = Path::new("C:/output");
        let first = TranscriptArtifactStore::video_artifact_dir(&root.join("one.mp4"));
        let second = TranscriptArtifactStore::video_artifact_dir(&root.join("two.mp4"));

        assert_eq!(first, root.join("one.mp4.transcript"));
        assert_eq!(second, root.join("two.mp4.transcript"));
        assert_ne!(first, second);
    }

    #[test]
    fn asr_audit_paths_include_the_complete_media_filename() {
        let root = Path::new("C:/output");
        let mp4 = TranscriptArtifactStore::asr_audit_path(root.join("clip.mp4"));
        let mov = TranscriptArtifactStore::asr_audit_path(root.join("clip.mov"));

        assert_eq!(mp4, root.join("clip.mp4.asr.audit.json"));
        assert_eq!(mov, root.join("clip.mov.asr.audit.json"));
        assert_ne!(mp4, mov);
    }

    #[tokio::test]
    async fn loads_legacy_transcript_files_when_canonical_files_are_absent() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("transcript.raw.srt"), "legacy raw")
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("transcript.corrected.srt"),
            "legacy corrected",
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, "legacy raw");
        assert_eq!(bundle.corrected_srt, "legacy corrected");
    }

    #[tokio::test]
    async fn canonical_files_take_precedence_over_legacy_files() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("subtitle.raw.srt"), "canonical raw")
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("subtitle.corrected.srt"),
            "canonical corrected",
        )
        .await
        .unwrap();
        tokio::fs::write(dir.path().join("transcript.raw.srt"), "legacy raw")
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("transcript.corrected.srt"),
            "legacy corrected",
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, "canonical raw");
        assert_eq!(bundle.corrected_srt, "canonical corrected");
    }

    #[tokio::test]
    async fn initialize_with_new_raw_archives_the_prior_generation() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let first = correction("first", ReviewDecision::Pending);
        let second = correction("second", ReviewDecision::Approved);

        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            "original raw",
            "first corrected",
            vec![first],
        )
        .await
        .unwrap();
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            "new raw",
            "second corrected",
            vec![second],
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
            .await
            .unwrap();
        assert_eq!(bundle.raw_srt, "new raw");
        assert_eq!(bundle.corrected_srt, "second corrected");
        assert_eq!(bundle.corrections[0].id, "second");
        let archived_raw = std::fs::read_dir(dir.path())
            .unwrap()
            .find_map(|entry| {
                let path = entry.unwrap().path();
                let is_archived_raw =
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("subtitle.raw.") && name.ends_with(".srt")
                        });
                is_archived_raw.then_some(path)
            })
            .expect("previous raw generation should be archived");
        assert_eq!(
            tokio::fs::read_to_string(archived_raw).await.unwrap(),
            "original raw"
        );
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.srt"))
                .await
                .unwrap(),
            "second corrected"
        );
        for file in [
            "subtitle.corrected.tmp",
            "subtitle.corrected.bak",
            "subtitle.corrections.tmp",
            "subtitle.corrections.bak",
            "subtitle.tmp",
            "subtitle.bak",
        ] {
            assert!(!dir.path().join(file).exists(), "left recovery file {file}");
        }
    }

    #[tokio::test]
    async fn fresh_initialization_persists_and_enforces_the_source_owner() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };

        TranscriptArtifactStore::initialize(source.clone(), dir.path(), "raw", "corrected", vec![])
            .await
            .unwrap();

        assert_eq!(
            TranscriptArtifactStore::read_source_owner(dir.path())
                .await
                .unwrap(),
            Some(source.clone())
        );
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.source.json"))
                .await
                .unwrap(),
            r#"{
  "kind": "video",
  "videoId": 7
}"#
        );

        let error = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 8 },
            dir.path(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            "transcript artifacts are bound to a different source"
        );
    }

    #[tokio::test]
    async fn initialization_does_not_silently_label_existing_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("transcript.raw.srt"), "legacy raw")
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("transcript.corrected.srt"),
            "legacy corrected",
        )
        .await
        .unwrap();

        TranscriptArtifactStore::initialize(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
            "legacy raw",
            "legacy corrected",
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(
            TranscriptArtifactStore::read_source_owner(dir.path())
                .await
                .unwrap(),
            None
        );
        assert!(!dir.path().join("subtitle.source.json").exists());
    }

    #[tokio::test]
    async fn failed_first_generation_rolls_back_the_source_owner() {
        let dir = tempfile::tempdir().unwrap();
        TranscriptArtifactStore::fail_batch_publish_for_test(dir.path(), 4);

        let error = TranscriptArtifactStore::initialize(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
            "raw",
            "corrected",
            vec![],
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        for name in [
            "subtitle.raw.srt",
            "subtitle.corrected.srt",
            "subtitle.corrections.json",
            "subtitle.srt",
            "subtitle.source.json",
        ] {
            assert!(!dir.path().join(name).exists(), "left artifact {name}");
        }
    }

    #[tokio::test]
    async fn load_restores_backup_when_canonical_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("subtitle.corrected.bak"),
            "recovered corrected",
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrected_srt, "recovered corrected");
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.corrected.srt"))
                .await
                .unwrap(),
            "recovered corrected"
        );
    }

    #[tokio::test]
    async fn rejects_malformed_correction_json() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("subtitle.corrections.json"), "not an array")
            .await
            .unwrap();

        let error = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn resolve_persists_an_explicit_review_decision() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = one_cue_srt("before");
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            &srt,
            &srt,
            vec![correction("change-1", ReviewDecision::Pending)],
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::resolve(
            source,
            dir.path(),
            "change-1",
            ReviewAction::Approve,
            Some("after".into()),
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrected_srt, one_cue_srt("after"));
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Approved);
        assert_eq!(bundle.corrections[0].decided_text.as_deref(), Some("after"));
        assert_eq!(bundle.pending_critical_count, 0);
    }

    #[tokio::test]
    async fn approval_rejects_unresolved_placeholders_without_mutation() {
        for placeholder in [
            "[待确认]",
            "  [待确认]  ",
            "[到手价待确认：识别为5830]",
            "[听不清]",
            "[疑似影石 Ace Pro 2]",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let source = TranscriptSource::Video { video_id: 7 };
            let srt = one_cue_srt("before");
            TranscriptArtifactStore::initialize(
                source.clone(),
                dir.path(),
                &srt,
                &srt,
                vec![correction("change-1", ReviewDecision::Pending)],
            )
            .await
            .unwrap();

            let error = TranscriptArtifactStore::resolve(
                source.clone(),
                dir.path(),
                "change-1",
                ReviewAction::Approve,
                Some(placeholder.into()),
            )
            .await
            .unwrap_err();

            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
                .await
                .unwrap();
            assert_eq!(bundle.corrected_srt, srt);
            assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
        }
    }

    #[tokio::test]
    async fn manual_edit_rejects_pending_critical_review_without_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = one_cue_srt("before");
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            &srt,
            &srt,
            vec![correction("change-1", ReviewDecision::Pending)],
        )
        .await
        .unwrap();

        let error = TranscriptArtifactStore::apply_manual_edit(
            source.clone(),
            dir.path(),
            &one_cue_srt("manual edit"),
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
            .await
            .unwrap();
        assert_eq!(bundle.raw_srt, srt);
        assert_eq!(bundle.corrected_srt, srt);
        assert_eq!(bundle.corrections.len(), 1);
    }

    #[tokio::test]
    async fn unchanged_manual_save_is_idempotent_while_review_is_pending() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = one_cue_srt("before");
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            &srt,
            &srt,
            vec![correction("change-1", ReviewDecision::Pending)],
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::apply_manual_edit(source, dir.path(), &srt)
            .await
            .unwrap();

        assert_eq!(bundle.corrected_srt, srt);
        assert_eq!(bundle.corrections.len(), 1);
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
    }

    #[tokio::test]
    async fn manual_edit_preserves_raw_and_appends_source_timestamped_audit() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let raw = one_cue_srt("raw");
        let corrected = one_cue_srt("reviewed");
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            &raw,
            &corrected,
            Vec::new(),
        )
        .await
        .unwrap();

        let edited = one_cue_srt("manual edit");
        let bundle = TranscriptArtifactStore::apply_manual_edit(source, dir.path(), &edited)
            .await
            .unwrap();

        assert_eq!(bundle.raw_srt, raw);
        assert_eq!(bundle.corrected_srt, edited);
        let audit = bundle.corrections.last().unwrap();
        assert_eq!(audit.category, "manual_edit");
        assert_eq!(audit.decision, ReviewDecision::Approved);
        assert_eq!(audit.decided_text.as_deref(), Some(edited.as_str()));
        assert!(audit.evidence.iter().any(|item| item == "source=video:7"));
        assert!(audit
            .evidence
            .iter()
            .any(|item| item.starts_with("timestamp_ms=")));
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.srt"))
                .await
                .unwrap(),
            edited
        );
    }

    #[tokio::test]
    async fn first_manual_import_uses_empty_raw_and_records_explicit_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let manual = one_cue_srt("manual first transcript");

        let bundle = TranscriptArtifactStore::initialize_manual_import(source, dir.path(), &manual)
            .await
            .unwrap();

        assert_eq!(bundle.raw_srt, "");
        assert_eq!(bundle.corrected_srt, manual);
        assert_eq!(bundle.source, TranscriptSource::Video { video_id: 7 });
        assert_eq!(bundle.corrections.len(), 1);
        let audit = &bundle.corrections[0];
        assert_eq!(audit.category, "manual_import");
        assert_eq!(audit.original, "");
        assert_eq!(audit.proposed, manual);
        assert_eq!(audit.decision, ReviewDecision::Approved);
        assert_eq!(audit.decided_text.as_deref(), Some(manual.as_str()));
        assert!(audit.evidence.iter().any(|item| item == "source=video:7"));
        assert!(audit
            .evidence
            .iter()
            .any(|item| item == "editor=video_subtitle"));
        assert!(audit
            .evidence
            .iter()
            .any(|item| item.starts_with("timestamp_ms=")));
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.raw.srt"))
                .await
                .unwrap(),
            ""
        );
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.srt"))
                .await
                .unwrap(),
            manual
        );

        let error = TranscriptArtifactStore::initialize_manual_import(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
            &one_cue_srt("replacement import"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        let reloaded = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(reloaded.raw_srt, "");
        assert_eq!(reloaded.corrected_srt, manual);
        assert_eq!(reloaded.corrections.len(), 1);
    }

    #[tokio::test]
    async fn approval_changes_only_the_timestamped_cue_when_text_repeats() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = concat!(
            "1\n00:00:00,000 --> 00:00:01,000\nbefore\n\n",
            "2\n00:00:01,000 --> 00:00:02,000\nbefore\n\n"
        );
        let mut correction = correction("second-cue", ReviewDecision::Pending);
        correction.start_ms = 1_000;
        correction.end_ms = 2_000;
        TranscriptArtifactStore::initialize(source.clone(), dir.path(), srt, srt, vec![correction])
            .await
            .unwrap();

        let bundle = TranscriptArtifactStore::resolve(
            source,
            dir.path(),
            "second-cue",
            ReviewAction::Approve,
            Some("after".into()),
        )
        .await
        .unwrap();

        assert_eq!(
            bundle.corrected_srt,
            concat!(
                "1\n00:00:00,000 --> 00:00:01,000\nbefore\n\n",
                "2\n00:00:01,000 --> 00:00:02,000\nafter\n\n"
            )
        );
    }

    #[tokio::test]
    async fn second_resolution_of_a_decided_correction_is_rejected_without_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = one_cue_srt("before");
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            &srt,
            &srt,
            vec![correction("change-1", ReviewDecision::Pending)],
        )
        .await
        .unwrap();
        TranscriptArtifactStore::resolve(
            source.clone(),
            dir.path(),
            "change-1",
            ReviewAction::Approve,
            Some("after".into()),
        )
        .await
        .unwrap();
        let before = tokio::fs::read(dir.path().join("subtitle.corrections.json"))
            .await
            .unwrap();

        let error = TranscriptArtifactStore::resolve(
            source.clone(),
            dir.path(),
            "change-1",
            ReviewAction::KeepOriginal,
            None,
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(
            tokio::fs::read(dir.path().join("subtitle.corrections.json"))
                .await
                .unwrap(),
            before
        );
        assert_eq!(
            TranscriptArtifactStore::load_from_dir(source, dir.path())
                .await
                .unwrap()
                .corrected_srt,
            one_cue_srt("after")
        );
    }

    #[tokio::test]
    async fn concurrent_decisions_are_serialized_without_lost_updates() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = concat!(
            "1\n00:00:00,000 --> 00:00:01,000\nbefore one\n\n",
            "2\n00:00:01,000 --> 00:00:02,000\nbefore two\n\n"
        );
        let mut first = correction("first", ReviewDecision::Pending);
        first.original = "before one".into();
        first.proposed = "after one".into();
        let mut second = correction("second", ReviewDecision::Pending);
        second.start_ms = 1_000;
        second.end_ms = 2_000;
        second.original = "before two".into();
        second.proposed = "after two".into();
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            srt,
            srt,
            vec![first, second],
        )
        .await
        .unwrap();

        let (first, second) = tokio::join!(
            TranscriptArtifactStore::resolve(
                source.clone(),
                dir.path(),
                "first",
                ReviewAction::Approve,
                Some("after one".into()),
            ),
            TranscriptArtifactStore::resolve(
                source.clone(),
                dir.path(),
                "second",
                ReviewAction::Approve,
                Some("after two".into()),
            )
        );
        first.unwrap();
        second.unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
            .await
            .unwrap();
        assert_eq!(bundle.pending_critical_count, 0);
        assert!(bundle
            .corrections
            .iter()
            .all(|correction| correction.decision == ReviewDecision::Approved));
        assert!(bundle.corrected_srt.contains("after one"));
        assert!(bundle.corrected_srt.contains("after two"));
    }

    #[tokio::test]
    async fn ensuring_legacy_corrections_preserves_a_concurrent_decision() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = concat!(
            "1\n00:00:00,000 --> 00:00:01,000\nbefore one\n\n",
            "2\n00:00:01,000 --> 00:00:02,000\nbefore two\n\n"
        );
        let mut existing = correction("existing", ReviewDecision::Pending);
        existing.original = "before one".into();
        existing.proposed = "after one".into();
        let mut legacy = correction("legacy-review-1", ReviewDecision::Pending);
        legacy.start_ms = 1_000;
        legacy.end_ms = 2_000;
        legacy.original = "before two".into();
        legacy.proposed = "before two".into();
        TranscriptArtifactStore::initialize(source.clone(), dir.path(), srt, srt, vec![existing])
            .await
            .unwrap();

        let (ensured, resolved) = tokio::join!(
            TranscriptArtifactStore::ensure_legacy_corrections(
                source.clone(),
                dir.path(),
                vec![legacy.clone()],
            ),
            TranscriptArtifactStore::resolve(
                source.clone(),
                dir.path(),
                "existing",
                ReviewAction::Approve,
                Some("after one".into()),
            )
        );
        ensured.unwrap();
        resolved.unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
            .await
            .unwrap();
        assert_eq!(
            bundle
                .corrections
                .iter()
                .find(|correction| correction.id == "existing")
                .unwrap()
                .decision,
            ReviewDecision::Approved
        );
        assert!(bundle.corrected_srt.contains("after one"));
        assert_eq!(
            bundle
                .corrections
                .iter()
                .filter(|correction| correction.id == legacy.id)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn failed_batch_publish_rolls_back_the_entire_generation() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            "old raw",
            "old corrected",
            vec![correction("old", ReviewDecision::Pending)],
        )
        .await
        .unwrap();
        TranscriptArtifactStore::fail_batch_publish_for_test(dir.path(), 2);

        let error = TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            "new raw",
            "new corrected",
            vec![correction("new", ReviewDecision::Pending)],
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
            .await
            .unwrap();
        assert_eq!(bundle.raw_srt, "old raw");
        assert_eq!(bundle.corrected_srt, "old corrected");
        assert_eq!(bundle.corrections[0].id, "old");
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.srt"))
                .await
                .unwrap(),
            "old corrected"
        );
    }

    #[tokio::test]
    async fn load_recovers_an_interrupted_batch_to_the_prior_generation() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            "old raw",
            "old corrected",
            vec![correction("old", ReviewDecision::Pending)],
        )
        .await
        .unwrap();
        for (name, new_value) in [
            ("subtitle.raw.srt", "new raw"),
            ("subtitle.corrected.srt", "new corrected"),
            ("subtitle.corrections.json", "[]"),
            ("subtitle.srt", "new corrected"),
        ] {
            let path = dir.path().join(name);
            tokio::fs::rename(&path, path.with_extension("bak"))
                .await
                .unwrap();
            tokio::fs::write(path, new_value).await.unwrap();
        }
        tokio::fs::write(
            dir.path().join(".subtitle.artifacts.txn.json"),
            r#"{"entries":[{"name":"subtitle.raw.srt","existed":true},{"name":"subtitle.corrected.srt","existed":true},{"name":"subtitle.corrections.json","existed":true},{"name":"subtitle.srt","existed":true}]}"#,
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(source, dir.path())
            .await
            .unwrap();

        assert_eq!(bundle.raw_srt, "old raw");
        assert_eq!(bundle.corrected_srt, "old corrected");
        assert_eq!(bundle.corrections[0].id, "old");
        assert!(!dir.path().join(".subtitle.artifacts.txn.json").exists());
    }

    #[tokio::test]
    async fn recovery_rejects_parent_directory_manifest_entry_without_touching_sentinel() {
        assert_malicious_manifest_is_rejected(vec!["../sentinel".into()]).await;
    }

    #[tokio::test]
    async fn recovery_rejects_absolute_manifest_entry_without_touching_sentinel() {
        let root = tempfile::tempdir().unwrap();
        let sentinel = root.path().join("sentinel");
        assert_malicious_manifest_is_rejected_at(root, vec![sentinel.to_string_lossy().into()])
            .await;
    }

    #[tokio::test]
    async fn recovery_rejects_duplicate_manifest_entries_without_touching_sentinel() {
        assert_malicious_manifest_is_rejected(vec![
            "subtitle.raw.srt".into(),
            "subtitle.raw.srt".into(),
        ])
        .await;
    }

    #[tokio::test]
    async fn recovery_rejects_unexpected_manifest_entry_without_touching_sentinel() {
        assert_malicious_manifest_is_rejected(vec!["not-an-artifact.txt".into()]).await;
    }

    #[tokio::test]
    async fn recovery_rejects_current_directory_manifest_entry_without_touching_sentinel() {
        assert_malicious_manifest_is_rejected(vec!["./subtitle.raw.srt".into()]).await;
    }

    #[tokio::test]
    async fn load_removes_an_orphan_temporary_marker_without_changing_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("subtitle.raw.srt"), "active raw")
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join(".subtitle.artifacts.txn.tmp"),
            "incomplete marker",
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(bundle.raw_srt, "active raw");
        assert!(!dir.path().join(".subtitle.artifacts.txn.tmp").exists());
    }

    #[tokio::test]
    async fn malformed_active_marker_leaves_artifacts_and_orphan_marker_untouched() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("subtitle.raw.srt"), "active raw")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join(".subtitle.artifacts.txn.json"), "{bad json")
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join(".subtitle.artifacts.txn.tmp"),
            "orphan marker",
        )
        .await
        .unwrap();

        let error = TranscriptArtifactStore::load_from_dir(
            TranscriptSource::Video { video_id: 7 },
            dir.path(),
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("subtitle.raw.srt"))
                .await
                .unwrap(),
            "active raw"
        );
        assert!(dir.path().join(".subtitle.artifacts.txn.json").exists());
        assert!(dir.path().join(".subtitle.artifacts.txn.tmp").exists());
    }

    #[tokio::test]
    async fn approval_rejects_duplicate_original_text_within_the_same_cue() {
        let dir = tempfile::tempdir().unwrap();
        let source = TranscriptSource::Video { video_id: 7 };
        let srt = one_cue_srt("before and before");
        TranscriptArtifactStore::initialize(
            source.clone(),
            dir.path(),
            &srt,
            &srt,
            vec![correction("change-1", ReviewDecision::Pending)],
        )
        .await
        .unwrap();
        let corrected_before = tokio::fs::read(dir.path().join("subtitle.corrected.srt"))
            .await
            .unwrap();
        let corrections_before = tokio::fs::read(dir.path().join("subtitle.corrections.json"))
            .await
            .unwrap();

        let error = TranscriptArtifactStore::resolve(
            source,
            dir.path(),
            "change-1",
            ReviewAction::Approve,
            Some("after".into()),
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(
            tokio::fs::read(dir.path().join("subtitle.corrected.srt"))
                .await
                .unwrap(),
            corrected_before
        );
        assert_eq!(
            tokio::fs::read(dir.path().join("subtitle.corrections.json"))
                .await
                .unwrap(),
            corrections_before
        );
    }

    async fn write_chained_funasr_fixture(media: &Path, raw: &str, corrected: &str) {
        tokio::fs::write(media.with_extension("asr.raw.srt"), raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), corrected)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([
                {
                    "原文": "佳能二六二",
                    "校对稿": "佳能R六二",
                    "修改类型": "商品型号",
                    "依据": "本地词表：佳能R六二",
                    "是否需要人工确认": false
                },
                {
                    "原文": "佳能R六二",
                    "校对稿": "佳能R62",
                    "修改类型": "MiniMax商品型号",
                    "依据": "事实卡：佳能R62",
                    "是否需要人工确认": false
                }
            ]))
            .unwrap(),
        )
        .await
        .unwrap();
    }

    async fn assert_formatting_change_pending(source: &str, corrected: &str) {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("video.mp4");
        let raw = one_cue_srt(source);
        let corrected_srt = one_cue_srt(corrected);
        tokio::fs::write(media.with_extension("asr.raw.srt"), &raw)
            .await
            .unwrap();
        tokio::fs::write(media.with_extension("asr.corrected.srt"), &corrected_srt)
            .await
            .unwrap();
        tokio::fs::write(
            media.with_extension("asr.changes.json"),
            serde_json::to_vec(&serde_json::json!([{
                "原文": source,
                "校对稿": corrected,
                "修改类型": "标点",
                "依据": format!("确定性标点规则：{corrected}"),
                "是否需要人工确认": false
            }]))
            .unwrap(),
        )
        .await
        .unwrap();

        let bundle = TranscriptArtifactStore::initialize_from_asr_outputs(
            TranscriptSource::Video { video_id: 13 },
            root.path().join("artifacts"),
            &media,
            &corrected_srt,
            "funasr",
        )
        .await
        .unwrap();

        assert_eq!(bundle.corrected_srt, raw, "unsafe auto-apply: {source}");
        assert_eq!(bundle.corrections.len(), 1);
        assert!(bundle.corrections[0].critical);
        assert_eq!(bundle.corrections[0].decision, ReviewDecision::Pending);
        assert_eq!(bundle.corrections[0].decided_text, None);
    }

    fn correction(id: &str, decision: ReviewDecision) -> TranscriptCorrection {
        TranscriptCorrection {
            id: id.into(),
            start_ms: 0,
            end_ms: 1_000,
            original: "before".into(),
            proposed: "after".into(),
            category: "model".into(),
            evidence: vec!["source fact".into()],
            critical: true,
            decision,
            decided_text: None,
        }
    }

    fn one_cue_srt(text: &str) -> String {
        format!("1\n00:00:00,000 --> 00:00:01,000\n{text}\n\n")
    }

    async fn assert_malicious_manifest_is_rejected(names: Vec<String>) {
        assert_malicious_manifest_is_rejected_at(tempfile::tempdir().unwrap(), names).await;
    }

    async fn assert_malicious_manifest_is_rejected_at(root: tempfile::TempDir, names: Vec<String>) {
        let dir = root.path().join("artifacts");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let sentinel = root.path().join("sentinel");
        let sentinel_backup = sentinel.with_extension("bak");
        tokio::fs::write(&sentinel, "outside sentinel")
            .await
            .unwrap();
        tokio::fs::write(&sentinel_backup, "outside backup")
            .await
            .unwrap();
        tokio::fs::write(dir.join("subtitle.raw.srt"), "active raw")
            .await
            .unwrap();
        let entries = names
            .into_iter()
            .map(|name| serde_json::json!({ "name": name, "existed": true }))
            .collect::<Vec<_>>();
        tokio::fs::write(
            dir.join(".subtitle.artifacts.txn.json"),
            serde_json::to_vec(&serde_json::json!({ "entries": entries })).unwrap(),
        )
        .await
        .unwrap();

        let error =
            TranscriptArtifactStore::load_from_dir(TranscriptSource::Video { video_id: 7 }, &dir)
                .await
                .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            tokio::fs::read_to_string(&sentinel).await.unwrap(),
            "outside sentinel"
        );
        assert_eq!(
            tokio::fs::read_to_string(&sentinel_backup).await.unwrap(),
            "outside backup"
        );
        assert_eq!(
            tokio::fs::read_to_string(dir.join("subtitle.raw.srt"))
                .await
                .unwrap(),
            "active raw"
        );
        assert!(dir.join(".subtitle.artifacts.txn.json").exists());
    }
}
