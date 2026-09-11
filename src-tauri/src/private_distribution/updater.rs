use super::model::StoredCredential;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

const IDLE_REQUIRED: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const SUCCESS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const FAILURE_DELAYS: [Duration; 3] = [
    Duration::from_secs(15 * 60),
    Duration::from_secs(30 * 60),
    Duration::from_secs(60 * 60),
];
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const RECEIPT_FILE: &str = "pending-install.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    WaitingForIdle,
    Installing,
    Failed,
    UpToDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusDto {
    pub status: UpdatePhase,
    pub version: Option<String>,
    pub message: &'static str,
}
impl UpdateStatusDto {
    fn new(status: UpdatePhase, version: Option<String>) -> Self {
        let message = match status {
            UpdatePhase::Idle => "更新服务待命。",
            UpdatePhase::Checking => "正在安全检查更新。",
            UpdatePhase::Downloading => "正在后台下载并校验更新。",
            UpdatePhase::WaitingForIdle => "更新已就绪，将在软件连续空闲两分钟后自动安装。",
            UpdatePhase::Installing => "正在安装更新，软件即将重启。",
            UpdatePhase::Failed => "本次更新未完成，稍后将自动重试。",
            UpdatePhase::UpToDate => "当前已是最新版本。",
        };
        Self {
            status,
            version,
            message,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BusySnapshot {
    pub recording: bool,
    pub queued: usize,
    pub running: usize,
    pub previews: usize,
}
pub fn is_safe_snapshot(snapshot: &BusySnapshot) -> bool {
    !snapshot.recording && snapshot.queued == 0 && snapshot.running == 0 && snapshot.previews == 0
}

#[async_trait]
pub trait BusyProbe: Send + Sync {
    async fn snapshot(&self) -> BusySnapshot;
}

#[async_trait]
pub trait UpdateAuthorization: Send + Sync {
    /// Returns a verified credential. It must stay inside Rust and never be logged or serialized.
    async fn authorized_credential(&self) -> Result<StoredCredential, ()>;
    /// Performs a real online renewal. Offline grace must never satisfy this boundary.
    async fn online_authorized_credential(&self) -> Result<StoredCredential, ()>;
}

#[derive(Debug)]
pub struct IdleGate {
    required: Duration,
    idle_since: Option<Duration>,
}
impl IdleGate {
    pub fn new(required: Duration) -> Self {
        Self {
            required,
            idle_since: None,
        }
    }
    pub fn observe(&mut self, safe: bool, now: Duration) -> bool {
        if !safe {
            self.idle_since = None;
            return false;
        }
        let since = self.idle_since.get_or_insert(now);
        now.saturating_sub(*since) >= self.required
    }
}

#[derive(Debug, Default)]
pub struct CheckSchedule {
    failures: usize,
    completed_once: bool,
}
impl CheckSchedule {
    pub fn next_delay(&self) -> Duration {
        if !self.completed_once {
            Duration::ZERO
        } else if self.failures == 0 {
            SUCCESS_INTERVAL
        } else {
            FAILURE_DELAYS[self
                .failures
                .saturating_sub(1)
                .min(FAILURE_DELAYS.len() - 1)]
        }
    }
    pub fn record_success(&mut self) {
        self.completed_once = true;
        self.failures = 0;
    }
    pub fn record_failure(&mut self) {
        self.completed_once = true;
        self.failures = self.failures.saturating_add(1);
    }
}

struct RunGuard(Arc<AtomicBool>);
impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
pub struct PrivateUpdateCoordinator {
    status: Arc<RwLock<UpdateStatusDto>>,
    running: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    shutting_down: Arc<AtomicBool>,
    cached_artifact: Arc<Mutex<Option<CachedArtifact>>>,
}
impl Default for PrivateUpdateCoordinator {
    fn default() -> Self {
        Self {
            status: Arc::new(RwLock::new(UpdateStatusDto::new(UpdatePhase::Idle, None))),
            running: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            cached_artifact: Arc::new(Mutex::new(None)),
        }
    }
}
impl PrivateUpdateCoordinator {
    pub fn status(&self) -> UpdateStatusDto {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn set_status(&self, status: UpdatePhase, version: Option<String>) {
        *self.status.write().unwrap_or_else(|e| e.into_inner()) =
            UpdateStatusDto::new(status, version);
    }
    fn begin_run(&self) -> Option<RunGuard> {
        self.running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| RunGuard(self.running.clone()))
    }
    fn set_cached_artifact(&self, artifact: Option<CachedArtifact>) {
        *self
            .cached_artifact
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = artifact;
    }
    fn cached_artifact(&self) -> Option<CachedArtifact> {
        self.cached_artifact
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn begin_start(&self) -> bool {
        self.started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    fn cleanup_cached_path_with(&self, remove: impl FnOnce(&Path) -> std::io::Result<()>) -> bool {
        let Some(artifact) = self.cached_artifact() else {
            return true;
        };
        let path = artifact.path;
        match remove(&path) {
            Ok(()) => {
                let mut owned = self
                    .cached_artifact
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if owned.as_ref().is_some_and(|owned| owned.path == path) {
                    *owned = None;
                }
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut owned = self
                    .cached_artifact
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if owned.as_ref().is_some_and(|owned| owned.path == path) {
                    *owned = None;
                }
                true
            }
            Err(_) => false,
        }
    }
    fn cleanup_cached_path(&self) -> bool {
        self.cleanup_cached_path_with(|path| std::fs::remove_file(path))
    }
    fn prepare_scheduler_entry_with(
        &self,
        remove: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> bool {
        self.cleanup_cached_path_with(remove)
    }
    fn prepare_scheduler_entry(&self) -> bool {
        self.prepare_scheduler_entry_with(|path| std::fs::remove_file(path))
    }
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        for _ in 0..3 {
            if self.cleanup_cached_path() {
                break;
            }
            tokio::task::yield_now().await;
        }
    }
}

async fn bounded_download<F, T, E>(future: F, timeout: Duration) -> Result<T, ()>
where
    F: Future<Output = Result<T, E>>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) | Err(_) => Err(()),
    }
}

async fn report_best_effort<F, T, E>(future: F)
where
    F: Future<Output = Result<T, E>>,
{
    let _ = future.await;
}

fn valid_release_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn release_id_from_download_url(origin: &url::Url, download: &url::Url) -> Result<String, ()> {
    if download.scheme() != origin.scheme()
        || download.host_str() != origin.host_str()
        || download.port_or_known_default() != origin.port_or_known_default()
        || !download.username().is_empty()
        || download.password().is_some()
        || download.query().is_some()
        || download.fragment().is_some()
    {
        return Err(());
    }
    let parts = download.path_segments().ok_or(())?.collect::<Vec<_>>();
    if parts.len() != 4
        || parts[0] != "v1"
        || parts[1] != "download"
        || !valid_release_id(parts[2])
        || parts[3].len() != 43
        || !parts[3]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(());
    }
    Ok(parts[2].to_owned())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingInstallReceipt {
    release_id: String,
    target_version: String,
    client_event_id: String,
    #[serde(default)]
    state: PendingInstallState,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PendingInstallState {
    #[default]
    AwaitingRelaunch,
    Acknowledged,
    CleanupRequired,
}

#[derive(Debug, PartialEq, Eq)]
enum PendingReceiptRead {
    Missing,
    Ready(PendingInstallReceipt),
    Blocked,
}

fn receipt_bytes(receipt: &PendingInstallReceipt) -> std::io::Result<Vec<u8>> {
    serde_json::to_vec(receipt).map_err(std::io::Error::other)
}

fn write_receipt_temp(
    root: &Path,
    receipt: &PendingInstallReceipt,
) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;
    std::fs::create_dir_all(root)?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.write_all(&receipt_bytes(receipt)?)?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn create_pending_receipt(root: &Path, receipt: &PendingInstallReceipt) -> std::io::Result<()> {
    let file = write_receipt_temp(root, receipt)?
        .persist_noclobber(root.join(RECEIPT_FILE))
        .map_err(|error| error.error)?;
    file.sync_all()
}

fn write_pending_receipt(root: &Path, receipt: &PendingInstallReceipt) -> std::io::Result<()> {
    let file = write_receipt_temp(root, receipt)?;
    let file = file.persist(root.join(RECEIPT_FILE)).map_err(|e| e.error)?;
    file.sync_all()
}

fn read_pending_receipt_with(
    path: &Path,
    read: impl FnOnce(&Path) -> std::io::Result<Vec<u8>>,
) -> PendingReceiptRead {
    match read(path) {
        Ok(bytes) => match serde_json::from_slice::<PendingInstallReceipt>(&bytes) {
            Ok(receipt) => PendingReceiptRead::Ready(receipt),
            Err(_) => PendingReceiptRead::Blocked,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => PendingReceiptRead::Missing,
        Err(_) => PendingReceiptRead::Blocked,
    }
}

fn read_pending_receipt(root: &Path) -> std::io::Result<Option<PendingInstallReceipt>> {
    match read_pending_receipt_with(&root.join(RECEIPT_FILE), |path| std::fs::read(path)) {
        PendingReceiptRead::Missing => Ok(None),
        PendingReceiptRead::Ready(receipt) => Ok(Some(receipt)),
        PendingReceiptRead::Blocked => Err(std::io::Error::other("pending receipt unavailable")),
    }
}

fn remove_acknowledged_receipt_with(
    path: &Path,
    remove: impl FnOnce(&Path) -> std::io::Result<()>,
) -> bool {
    match remove(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedArtifact {
    path: PathBuf,
    sha256: [u8; 32],
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut value = [0; 32];
    value.copy_from_slice(digest.as_ref());
    value
}

fn write_verified_cache(root: &Path, bytes: &[u8]) -> std::io::Result<CachedArtifact> {
    use std::io::Write;
    std::fs::create_dir_all(root)?;
    let target = root.join("verified-update.bin");
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(&target).map_err(|e| e.error)?;
    Ok(CachedArtifact {
        path: target,
        sha256: sha256(bytes),
    })
}

fn read_verified_cache(artifact: &CachedArtifact) -> std::io::Result<Vec<u8>> {
    let bytes = std::fs::read(&artifact.path)?;
    if sha256(&bytes) != artifact.sha256 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "verified update cache digest mismatch",
        ));
    }
    Ok(bytes)
}

/// Security-sensitive final boundary. `verified_bytes` must be the single
/// read-and-digested cache snapshot and the durable receipt must already exist.
/// After the busy snapshot resolves, shutdown is synchronously rechecked and
/// the installer is invoked without another await, file operation or telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalInstallError {
    AuthorizationOrSafety,
    Receipt,
    Install,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalInstallDisposition {
    Succeeded,
    Failed {
        cleanup_receipt: bool,
        report_install_failed: bool,
    },
}

fn final_install_disposition(result: Result<(), FinalInstallError>) -> FinalInstallDisposition {
    match result {
        Ok(()) => FinalInstallDisposition::Succeeded,
        Err(FinalInstallError::AuthorizationOrSafety) => FinalInstallDisposition::Failed {
            cleanup_receipt: true,
            report_install_failed: false,
        },
        Err(FinalInstallError::Receipt) => FinalInstallDisposition::Failed {
            cleanup_receipt: false,
            report_install_failed: false,
        },
        Err(FinalInstallError::Install) => FinalInstallDisposition::Failed {
            cleanup_receipt: true,
            report_install_failed: true,
        },
    }
}

async fn final_install_gate<Renew, RenewFuture, Busy, BusyFuture, Install>(
    verified_bytes: Vec<u8>,
    renew: Renew,
    busy_is_safe: Busy,
    shutting_down: impl FnOnce() -> bool,
    install: Install,
) -> Result<(), FinalInstallError>
where
    Renew: FnOnce() -> RenewFuture,
    RenewFuture: Future<Output = Result<(), ()>>,
    Busy: FnOnce() -> BusyFuture,
    BusyFuture: Future<Output = bool>,
    Install: FnOnce(Vec<u8>) -> Result<(), ()>,
{
    renew()
        .await
        .map_err(|_| FinalInstallError::AuthorizationOrSafety)?;
    if !busy_is_safe().await || shutting_down() {
        return Err(FinalInstallError::AuthorizationOrSafety);
    }
    install(verified_bytes).map_err(|_| FinalInstallError::Install)
}

async fn final_install_with_receipt<Write, Renew, RenewFuture, Busy, BusyFuture, Install>(
    verified_bytes: Vec<u8>,
    write_receipt: Write,
    renew: Renew,
    busy_is_safe: Busy,
    shutting_down: impl FnOnce() -> bool,
    install: Install,
) -> Result<(), FinalInstallError>
where
    Write: FnOnce() -> std::io::Result<()>,
    Renew: FnOnce() -> RenewFuture,
    RenewFuture: Future<Output = Result<(), ()>>,
    Busy: FnOnce() -> BusyFuture,
    BusyFuture: Future<Output = bool>,
    Install: FnOnce(Vec<u8>) -> Result<(), ()>,
{
    write_receipt().map_err(|_| FinalInstallError::Receipt)?;
    final_install_gate(verified_bytes, renew, busy_is_safe, shutting_down, install).await
}

fn cleanup_failed_receipt_with<Write, Remove>(
    root: &Path,
    receipt: &PendingInstallReceipt,
    write: Write,
    remove: Remove,
) -> bool
where
    Write: FnOnce(&Path, &PendingInstallReceipt) -> std::io::Result<()>,
    Remove: FnOnce(&Path) -> std::io::Result<()>,
{
    let mut cleanup_receipt = receipt.clone();
    cleanup_receipt.state = PendingInstallState::CleanupRequired;
    if write(root, &cleanup_receipt).is_err() {
        return false;
    }
    remove_acknowledged_receipt_with(&root.join(RECEIPT_FILE), remove)
}

fn cleanup_failed_receipt(root: &Path, receipt: &PendingInstallReceipt) -> bool {
    cleanup_failed_receipt_with(root, receipt, write_pending_receipt, |path| {
        std::fs::remove_file(path)
    })
}

#[cfg(feature = "gui")]
impl PrivateUpdateCoordinator {
    pub fn start(
        &self,
        app: tauri::AppHandle,
        cache_root: PathBuf,
        authorization: Arc<dyn UpdateAuthorization>,
        busy: Arc<dyn BusyProbe>,
        before_install_exit: Arc<dyn Fn() + Send + Sync>,
    ) {
        if !self.begin_start() {
            return;
        }
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let stale_cache = cache_root.join("verified-update.bin");
            if stale_cache.exists() {
                coordinator.set_cached_artifact(Some(CachedArtifact {
                    path: stale_cache,
                    sha256: [0; 32],
                }));
            }
            let mut schedule = CheckSchedule::default();
            loop {
                let delay = schedule.next_delay();
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                if coordinator.shutting_down.load(Ordering::Acquire) {
                    break;
                }
                let receipt_reconciled = coordinator
                    .reconcile_pending_install(&app, &cache_root, authorization.as_ref())
                    .await;
                if !receipt_reconciled {
                    schedule.record_failure();
                    continue;
                }
                if !coordinator.prepare_scheduler_entry() {
                    schedule.record_failure();
                    continue;
                }
                let success = coordinator
                    .run_once(
                        &app,
                        &cache_root,
                        authorization.as_ref(),
                        busy.as_ref(),
                        before_install_exit.clone(),
                    )
                    .await;
                if success {
                    schedule.record_success();
                } else {
                    schedule.record_failure();
                }
            }
        });
    }

    async fn run_once(
        &self,
        app: &tauri::AppHandle,
        cache_root: &Path,
        authorization: &dyn UpdateAuthorization,
        busy: &dyn BusyProbe,
        before_install_exit: Arc<dyn Fn() + Send + Sync>,
    ) -> bool {
        let Some(_run) = self.begin_run() else {
            return false;
        };
        if !self.prepare_scheduler_entry() {
            self.set_status(UpdatePhase::Failed, None);
            return false;
        }
        self.set_status(UpdatePhase::Checking, None);
        let credential = match authorization.authorized_credential().await {
            Ok(value) => value,
            Err(()) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let origin = match private_update_origin() {
            Ok(value) => value,
            Err(()) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let endpoint = match origin.join("v1/update/windows/x86_64/{{current_version}}") {
            Ok(value) => value,
            Err(_) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let token = credential.device_token;
        let client = match super::client::DistributionClient::compiled() {
            Ok(client) => client,
            Err(_) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let current_version = app.package_info().version.to_string();
        report_best_effort(client.report_event(&token, None, &current_version, "check")).await;
        let updater = match build_private_updater(app, endpoint, &token, before_install_exit) {
            Ok(updater) => updater,
            Err(_) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let update = match updater.check().await {
            Ok(Some(update)) => update,
            Ok(None) => {
                self.set_status(UpdatePhase::UpToDate, None);
                return true;
            }
            Err(_) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let version = Some(update.version.clone());
        let release_id = match release_id_from_download_url(&origin, &update.download_url) {
            Ok(value) => value,
            Err(()) => {
                self.set_status(UpdatePhase::Failed, version);
                return false;
            }
        };
        report_best_effort(client.report_event(
            &token,
            Some(&release_id),
            &current_version,
            "download_started",
        ))
        .await;
        self.set_status(UpdatePhase::Downloading, version.clone());
        let bytes =
            match bounded_download(update.download(|_, _| {}, || {}), DOWNLOAD_TIMEOUT).await {
                Ok(bytes) => bytes,
                Err(()) => {
                    let _ = client
                        .report_event(
                            &token,
                            Some(&release_id),
                            &current_version,
                            "download_failed",
                        )
                        .await;
                    self.set_status(UpdatePhase::Failed, version);
                    return false;
                }
            };
        if self.shutting_down.load(Ordering::Acquire) {
            let _ = client
                .report_event(
                    &token,
                    Some(&release_id),
                    &current_version,
                    "download_failed",
                )
                .await;
            return false;
        }
        // Update::download verifies the Tauri minisign signature before returning.
        let cache = match write_verified_cache(cache_root, &bytes) {
            Ok(cache) => cache,
            Err(_) => {
                let _ = client
                    .report_event(
                        &token,
                        Some(&release_id),
                        &current_version,
                        "download_failed",
                    )
                    .await;
                self.set_status(UpdatePhase::Failed, version);
                return false;
            }
        };
        drop(bytes);
        self.set_cached_artifact(Some(cache.clone()));
        self.set_status(UpdatePhase::WaitingForIdle, version.clone());
        let started = std::time::Instant::now();
        let mut gate = IdleGate::new(IDLE_REQUIRED);
        loop {
            if self.shutting_down.load(Ordering::Acquire) {
                self.cleanup_cached_path();
                return false;
            }
            if gate.observe(is_safe_snapshot(&busy.snapshot().await), started.elapsed()) {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        // Read and digest exactly once before the online authorization gate.
        // The resulting Vec is the only payload that can reach Update::install.
        let install_bytes = match tokio::task::spawn_blocking({
            let cache = cache.clone();
            move || read_verified_cache(&cache)
        })
        .await
        {
            Ok(Ok(bytes)) => bytes,
            _ => {
                self.cleanup_cached_path();
                let _ = client
                    .report_event(
                        &token,
                        Some(&release_id),
                        &current_version,
                        "install_failed",
                    )
                    .await;
                self.set_status(UpdatePhase::Failed, version);
                return false;
            }
        };
        // Cache cleanup is deliberately before online renewal. From renewal to
        // receipt/install there is no additional file read or unrelated await.
        if !self.cleanup_cached_path() {
            let _ = client
                .report_event(
                    &token,
                    Some(&release_id),
                    &current_version,
                    "install_failed",
                )
                .await;
            self.set_status(UpdatePhase::Failed, version);
            return false;
        }
        let receipt = PendingInstallReceipt {
            release_id: release_id.clone(),
            target_version: update.version.clone(),
            client_event_id: uuid::Uuid::new_v4().to_string(),
            state: PendingInstallState::AwaitingRelaunch,
        };
        let update = update.clone();
        self.set_status(UpdatePhase::Installing, version.clone());
        let install_result = final_install_with_receipt(
            install_bytes,
            || create_pending_receipt(cache_root, &receipt),
            || async {
                authorization
                    .online_authorized_credential()
                    .await
                    .map(|_| ())
            },
            || async { is_safe_snapshot(&busy.snapshot().await) },
            || self.shutting_down.load(Ordering::Acquire),
            move |bytes| update.install(bytes).map_err(|_| ()),
        )
        .await;
        match final_install_disposition(install_result) {
            FinalInstallDisposition::Succeeded => true,
            FinalInstallDisposition::Failed {
                cleanup_receipt,
                report_install_failed,
            } => {
                if cleanup_receipt {
                    let _ = cleanup_failed_receipt(cache_root, &receipt);
                }
                if report_install_failed {
                    let _ = client
                        .report_event(
                            &token,
                            Some(&release_id),
                            &current_version,
                            "install_failed",
                        )
                        .await;
                }
                self.set_status(UpdatePhase::Failed, version);
                false
            }
        }
    }

    async fn reconcile_pending_install(
        &self,
        app: &tauri::AppHandle,
        root: &Path,
        authorization: &dyn UpdateAuthorization,
    ) -> bool {
        let path = root.join(RECEIPT_FILE);
        let mut receipt = match read_pending_receipt_with(&path, |path| std::fs::read(path)) {
            PendingReceiptRead::Missing => return true,
            PendingReceiptRead::Ready(receipt) => receipt,
            PendingReceiptRead::Blocked => return false,
        };
        if !valid_release_id(&receipt.release_id)
            || uuid::Uuid::parse_str(&receipt.client_event_id).is_err()
        {
            return false;
        }
        match receipt.state {
            PendingInstallState::Acknowledged | PendingInstallState::CleanupRequired => {
                return remove_acknowledged_receipt_with(&path, |path| std::fs::remove_file(path));
            }
            PendingInstallState::AwaitingRelaunch => {}
        }
        if receipt.target_version != app.package_info().version.to_string() {
            // Relaunching on the old version means installation did not take
            // effect. Persist cleanup ownership before allowing another run.
            receipt.state = PendingInstallState::CleanupRequired;
            if write_pending_receipt(root, &receipt).is_err() {
                return false;
            }
            return remove_acknowledged_receipt_with(&path, |path| std::fs::remove_file(path));
        }
        let Ok(credential) = authorization.online_authorized_credential().await else {
            return false;
        };
        let Ok(client) = super::client::DistributionClient::compiled() else {
            return false;
        };
        if client
            .report_event_idempotent(
                &credential.device_token,
                Some(&receipt.release_id),
                &receipt.target_version,
                "install_succeeded",
                &receipt.client_event_id,
            )
            .await
            .is_ok()
        {
            receipt.state = PendingInstallState::Acknowledged;
            if write_pending_receipt(root, &receipt).is_err() {
                return false;
            }
            if !remove_acknowledged_receipt_with(&path, |path| std::fs::remove_file(path)) {
                // Persisted acknowledgement means the next scheduler iteration
                // retries deletion without reporting the same event again.
                return false;
            }
            return true;
        }
        false
    }
}

#[cfg(feature = "gui")]
fn build_private_updater<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    endpoint: url::Url,
    token: &str,
    before_install_exit: Arc<dyn Fn() + Send + Sync>,
) -> tauri_plugin_updater::Result<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    let app_handle = app.clone();
    app.updater_builder()
        .endpoints(vec![endpoint])?
        .header("Authorization", format!("Bearer {token}"))?
        // The plugin invokes this only after successful extraction, immediately
        // before launching the installer. Extraction failures leave the app live.
        .on_before_exit(move || {
            before_install_exit();
            app_handle.cleanup_before_exit();
        })
        .timeout(Duration::from_secs(30))
        .configure_client(|client| {
            client
                .no_proxy()
                .redirect(reqwest_012::redirect::Policy::none())
        })
        .build()
}

#[cfg(feature = "gui")]
fn private_update_origin() -> Result<url::Url, ()> {
    if !cfg!(target_os = "windows") {
        return Err(());
    }
    let base =
        url::Url::parse(option_env!("DIANDIAN_UPDATE_ENDPOINT").ok_or(())?).map_err(|_| ())?;
    if base.scheme() != "https"
        || base.host_str().is_none()
        || !base.username().is_empty()
        || base.password().is_some()
        || base.path() != "/"
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(());
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[test]
    fn every_busy_source_blocks_install() {
        for busy in [
            BusySnapshot {
                recording: true,
                queued: 0,
                running: 0,
                previews: 0,
            },
            BusySnapshot {
                recording: false,
                queued: 1,
                running: 0,
                previews: 0,
            },
            BusySnapshot {
                recording: false,
                queued: 0,
                running: 1,
                previews: 0,
            },
            BusySnapshot {
                recording: false,
                queued: 0,
                running: 0,
                previews: 1,
            },
        ] {
            assert!(!is_safe_snapshot(&busy));
        }
        assert!(is_safe_snapshot(&BusySnapshot::default()));
    }
    #[test]
    fn downloaded_update_waits_until_two_minutes_continuously_idle() {
        let mut gate = IdleGate::new(Duration::from_secs(120));
        assert!(!gate.observe(true, Duration::from_secs(0)));
        assert!(!gate.observe(true, Duration::from_secs(119)));
        assert!(!gate.observe(false, Duration::from_secs(120)));
        assert!(!gate.observe(true, Duration::from_secs(121)));
        assert!(gate.observe(true, Duration::from_secs(241)));
    }
    #[test]
    fn scheduler_starts_immediately_resets_after_success_and_caps_backoff() {
        let mut schedule = CheckSchedule::default();
        assert_eq!(schedule.next_delay(), Duration::ZERO);
        schedule.record_failure();
        assert_eq!(schedule.next_delay(), Duration::from_secs(15 * 60));
        schedule.record_failure();
        assert_eq!(schedule.next_delay(), Duration::from_secs(30 * 60));
        schedule.record_failure();
        assert_eq!(schedule.next_delay(), Duration::from_secs(60 * 60));
        schedule.record_failure();
        assert_eq!(schedule.next_delay(), Duration::from_secs(60 * 60));
        schedule.record_success();
        assert_eq!(schedule.next_delay(), Duration::from_secs(6 * 60 * 60));
    }
    #[test]
    fn coordinator_prevents_reentry() {
        let coordinator = PrivateUpdateCoordinator::default();
        let run = coordinator.begin_run().unwrap();
        assert!(coordinator.begin_run().is_none());
        drop(run);
        assert!(coordinator.begin_run().is_some());
    }
    #[test]
    fn coordinator_start_is_atomic_and_idempotent() {
        let coordinator = PrivateUpdateCoordinator::default();
        assert!(coordinator.begin_start());
        assert!(!coordinator.begin_start());
    }
    #[tokio::test]
    async fn bounded_download_drops_the_underlying_future_on_timeout() {
        struct DropSignal(Arc<AtomicBool>);
        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let held = DropSignal(dropped.clone());
        let result = bounded_download(
            async move {
                let _held = held;
                std::future::pending::<Result<Vec<u8>, ()>>().await
            },
            Duration::from_millis(10),
        )
        .await;
        assert!(result.is_err());
        assert!(
            dropped.load(Ordering::Acquire),
            "timeout must cancel the real awaited future"
        );
    }
    #[tokio::test]
    async fn telemetry_failure_never_skips_following_security_gate() {
        let gate_reached = Arc::new(AtomicBool::new(false));
        report_best_effort(async { Err::<(), _>("telemetry unavailable") }).await;
        gate_reached.store(true, Ordering::Release);
        assert!(gate_reached.load(Ordering::Acquire));
    }
    #[test]
    fn failed_cleanup_retains_cache_ownership_until_retry_succeeds() {
        let coordinator = PrivateUpdateCoordinator::default();
        let path = PathBuf::from("private-update.bin");
        let artifact = CachedArtifact {
            path: path.clone(),
            sha256: [0; 32],
        };
        coordinator.set_cached_artifact(Some(artifact.clone()));
        let attempts = AtomicUsize::new(0);
        assert!(!coordinator.cleanup_cached_path_with(|owned| {
            assert_eq!(owned, path.as_path());
            attempts.fetch_add(1, Ordering::Relaxed);
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "busy",
            ))
        }));
        assert_eq!(coordinator.cached_artifact(), Some(artifact));
        assert!(coordinator.cleanup_cached_path_with(|owned| {
            assert_eq!(owned, path.as_path());
            attempts.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        assert_eq!(coordinator.cached_artifact(), None);
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn release_id_is_only_accepted_from_exact_private_download_path() {
        let origin = url::Url::parse("https://updates.example/").unwrap();
        assert_eq!(release_id_from_download_url(&origin, &url::Url::parse("https://updates.example/v1/download/release_1/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap()).unwrap(), "release_1");
        for value in [
            "https://evil.example/v1/download/release_1/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "https://updates.example/v1/download/release_1/short",
            "https://updates.example/v1/download/bad%2Fid/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "https://updates.example/v1/download/release_1/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA?leak=1",
        ] { assert!(release_id_from_download_url(&origin, &url::Url::parse(value).unwrap()).is_err()); }
    }
    #[test]
    fn pending_receipt_is_atomic_non_secret_and_strict() {
        let root = tempfile::tempdir().unwrap();
        let receipt = PendingInstallReceipt {
            release_id: "release_1".into(),
            target_version: "2.22.0".into(),
            client_event_id: "123e4567-e89b-42d3-a456-426614174000".into(),
            state: PendingInstallState::AwaitingRelaunch,
        };
        write_pending_receipt(root.path(), &receipt).unwrap();
        write_pending_receipt(root.path(), &receipt).unwrap();
        let bytes = std::fs::read(root.path().join(RECEIPT_FILE)).unwrap();
        assert_eq!(
            serde_json::from_slice::<PendingInstallReceipt>(&bytes).unwrap(),
            receipt
        );
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            !text.contains("Bearer") && !text.contains("deviceToken") && !text.contains("ticket")
        );
        assert!(serde_json::from_str::<PendingInstallReceipt>(
            r#"{"releaseId":"release_1","targetVersion":"2.22.0","extra":true}"#
        )
        .is_err());
    }
    #[test]
    fn acknowledged_receipt_delete_failure_is_retried_and_retained_until_success() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(RECEIPT_FILE);
        std::fs::write(&path, b"receipt").unwrap();
        assert!(!remove_acknowledged_receipt_with(&path, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "busy",
            ))
        }));
        assert!(path.exists());
        assert!(remove_acknowledged_receipt_with(&path, |path| {
            std::fs::remove_file(path)
        }));
        assert!(!path.exists());
    }
    #[test]
    fn receipt_read_errors_block_the_scheduler_until_the_same_receipt_can_be_recovered() {
        let path = PathBuf::from(RECEIPT_FILE);
        let denied = read_pending_receipt_with(&path, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "locked",
            ))
        });
        assert!(matches!(denied, PendingReceiptRead::Blocked));
        let missing = read_pending_receipt_with(&path, |_| {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"))
        });
        assert!(matches!(missing, PendingReceiptRead::Missing));
        let recovered = read_pending_receipt_with(&path, |_| {
            Ok(br#"{"releaseId":"release_1","targetVersion":"2.22.0","clientEventId":"123e4567-e89b-42d3-a456-426614174000","state":"acknowledged"}"#.to_vec())
        });
        assert!(matches!(
            recovered,
            PendingReceiptRead::Ready(PendingInstallReceipt {
                state: PendingInstallState::Acknowledged,
                ..
            })
        ));
    }
    #[test]
    fn acknowledged_and_failed_receipts_keep_cleanup_ownership_until_delete_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let mut receipt = PendingInstallReceipt {
            release_id: "release_1".into(),
            target_version: "2.22.0".into(),
            client_event_id: "123e4567-e89b-42d3-a456-426614174000".into(),
            state: PendingInstallState::AwaitingRelaunch,
        };
        create_pending_receipt(root.path(), &receipt).unwrap();
        receipt.state = PendingInstallState::Acknowledged;
        write_pending_receipt(root.path(), &receipt).unwrap();
        assert_eq!(
            read_pending_receipt(root.path()).unwrap().unwrap().state,
            PendingInstallState::Acknowledged
        );
        let path = root.path().join(RECEIPT_FILE);
        assert!(!remove_acknowledged_receipt_with(&path, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "locked",
            ))
        }));
        assert!(
            create_pending_receipt(
                root.path(),
                &PendingInstallReceipt {
                    release_id: "release_2".into(),
                    target_version: "2.23.0".into(),
                    client_event_id: "223e4567-e89b-42d3-a456-426614174000".into(),
                    state: PendingInstallState::AwaitingRelaunch,
                }
            )
            .is_err(),
            "an owned receipt must never be overwritten"
        );

        receipt.state = PendingInstallState::CleanupRequired;
        write_pending_receipt(root.path(), &receipt).unwrap();
        assert_eq!(
            read_pending_receipt(root.path()).unwrap().unwrap().state,
            PendingInstallState::CleanupRequired
        );
        assert!(remove_acknowledged_receipt_with(&path, |path| {
            std::fs::remove_file(path)
        }));
        assert!(
            create_pending_receipt(
                root.path(),
                &PendingInstallReceipt {
                    release_id: "release_2".into(),
                    target_version: "2.23.0".into(),
                    client_event_id: "223e4567-e89b-42d3-a456-426614174000".into(),
                    state: PendingInstallState::AwaitingRelaunch,
                }
            )
            .is_ok(),
            "the next scheduler iteration may create only after cleanup succeeds"
        );
    }
    #[tokio::test]
    async fn final_install_gate_never_calls_installer_when_revoked_shutting_down_or_busy() {
        for case in ["revoked", "shutdown", "busy"] {
            let installs = Arc::new(AtomicUsize::new(0));
            let install_spy = installs.clone();
            let shutting_down = case == "shutdown";
            let result = final_install_with_receipt(
                vec![1, 2, 3],
                || Ok(()),
                || async move {
                    if case == "revoked" {
                        Err(())
                    } else {
                        Ok(())
                    }
                },
                || async move { case != "busy" },
                || shutting_down,
                move |_| {
                    install_spy.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
            )
            .await;
            assert!(result.is_err());
            assert_eq!(installs.load(Ordering::Relaxed), 0, "{case}");
        }
    }
    #[tokio::test]
    async fn final_install_gate_does_not_install_when_receipt_creation_is_blocked() {
        let installs = Arc::new(AtomicUsize::new(0));
        let install_spy = installs.clone();
        let result = final_install_with_receipt(
            vec![1, 2, 3],
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "owned",
                ))
            },
            || async { Ok(()) },
            || async { true },
            || false,
            move |_| {
                install_spy.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .await;
        assert_eq!(result, Err(FinalInstallError::Receipt));
        assert_eq!(installs.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn final_install_gate_rechecks_shutdown_after_busy_snapshot_await() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let flip_shutdown = shutdown.clone();
        let read_shutdown = shutdown.clone();
        let installs = Arc::new(AtomicUsize::new(0));
        let install_spy = installs.clone();
        let result = final_install_with_receipt(
            vec![1, 2, 3],
            || Ok(()),
            || async { Ok(()) },
            move || async move {
                tokio::task::yield_now().await;
                flip_shutdown.store(true, Ordering::Release);
                true
            },
            move || read_shutdown.load(Ordering::Acquire),
            move |_| {
                install_spy.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .await;
        assert_eq!(result, Err(FinalInstallError::AuthorizationOrSafety));
        assert_eq!(installs.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn final_install_gate_rechecks_shutdown_changed_during_receipt_fsync() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let flip_shutdown = shutdown.clone();
        let read_shutdown = shutdown.clone();
        let installs = Arc::new(AtomicUsize::new(0));
        let install_spy = installs.clone();
        let result = final_install_with_receipt(
            vec![1, 2, 3],
            move || {
                flip_shutdown.store(true, Ordering::Release);
                Ok(())
            },
            || async { Ok(()) },
            || async { true },
            move || read_shutdown.load(Ordering::Acquire),
            move |_| {
                install_spy.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .await;
        assert_eq!(result, Err(FinalInstallError::AuthorizationOrSafety));
        assert_eq!(installs.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn final_install_gate_observes_busy_started_during_receipt_fsync() {
        let busy = Arc::new(AtomicBool::new(false));
        let start_busy = busy.clone();
        let read_busy = busy.clone();
        let installs = Arc::new(AtomicUsize::new(0));
        let install_spy = installs.clone();
        let result = final_install_with_receipt(
            vec![1, 2, 3],
            move || {
                start_busy.store(true, Ordering::Release);
                Ok(())
            },
            || async { Ok(()) },
            move || async move { !read_busy.load(Ordering::Acquire) },
            || false,
            move |_| {
                install_spy.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .await;
        assert_eq!(result, Err(FinalInstallError::AuthorizationOrSafety));
        assert_eq!(installs.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn every_final_install_error_fails_and_uses_the_expected_cleanup_policy() {
        assert_eq!(
            final_install_disposition(Ok(())),
            FinalInstallDisposition::Succeeded
        );
        for (error, cleanup_receipt, report_install_failed) in [
            (FinalInstallError::AuthorizationOrSafety, true, false),
            (FinalInstallError::Receipt, false, false),
            (FinalInstallError::Install, true, true),
        ] {
            let disposition = final_install_disposition(Err(error));
            assert_eq!(
                disposition,
                FinalInstallDisposition::Failed {
                    cleanup_receipt,
                    report_install_failed,
                }
            );
            let mut schedule = CheckSchedule::default();
            schedule.record_failure();
            assert_eq!(schedule.next_delay(), Duration::from_secs(15 * 60));
        }
    }
    #[test]
    fn failed_gate_receipt_keeps_cleanup_ownership_until_next_scheduler_retry() {
        let root = tempfile::tempdir().unwrap();
        let receipt = PendingInstallReceipt {
            release_id: "release_1".into(),
            target_version: "2.22.0".into(),
            client_event_id: "123e4567-e89b-42d3-a456-426614174000".into(),
            state: PendingInstallState::AwaitingRelaunch,
        };
        create_pending_receipt(root.path(), &receipt).unwrap();
        assert!(!cleanup_failed_receipt_with(
            root.path(),
            &receipt,
            write_pending_receipt,
            |_| Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "locked"
            )),
        ));
        assert_eq!(
            read_pending_receipt(root.path()).unwrap().unwrap().state,
            PendingInstallState::CleanupRequired
        );
        let path = root.path().join(RECEIPT_FILE);
        assert!(remove_acknowledged_receipt_with(&path, |path| {
            std::fs::remove_file(path)
        }));
        assert!(!path.exists());
    }
    #[test]
    fn cached_digest_rejects_actual_file_tampering_before_install() {
        let root = tempfile::tempdir().unwrap();
        let artifact = write_verified_cache(root.path(), b"signed exact bytes").unwrap();
        assert_eq!(
            read_verified_cache(&artifact).unwrap(),
            b"signed exact bytes"
        );
        std::fs::write(&artifact.path, b"tampered after verification").unwrap();
        assert!(read_verified_cache(&artifact).is_err());
    }
    #[test]
    fn next_scheduler_entry_retries_owned_cleanup_without_manual_helper() {
        let coordinator = PrivateUpdateCoordinator::default();
        let path = PathBuf::from("private-update.bin");
        let artifact = CachedArtifact {
            path: path.clone(),
            sha256: [7; 32],
        };
        coordinator.set_cached_artifact(Some(artifact));
        let attempts = AtomicUsize::new(0);
        let entry = || {
            coordinator.prepare_scheduler_entry_with(|owned| {
                assert_eq!(owned, path.as_path());
                if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "busy",
                    ))
                } else {
                    Ok(())
                }
            })
        };
        assert!(
            !entry(),
            "first scheduler iteration must not overwrite owned cache"
        );
        assert!(
            entry(),
            "next scheduler iteration must retry cleanup automatically"
        );
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
        assert!(coordinator.cached_artifact().is_none());
    }
    #[tokio::test]
    async fn verified_cache_is_atomic_and_shutdown_cleans_it() {
        let root = tempfile::tempdir().unwrap();
        let artifact = write_verified_cache(root.path(), b"verified").unwrap();
        assert_eq!(std::fs::read(&artifact.path).unwrap(), b"verified");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        let coordinator = PrivateUpdateCoordinator::default();
        coordinator.set_cached_artifact(Some(artifact.clone()));
        coordinator.shutdown().await;
        assert!(!artifact.path.exists());
    }
    #[test]
    fn status_wire_is_read_only_and_contains_no_credentials() {
        let value = serde_json::to_value(UpdateStatusDto::new(
            UpdatePhase::WaitingForIdle,
            Some("2.21.1".into()),
        ))
        .unwrap();
        assert_eq!(value["status"], "waiting_for_idle");
        assert_eq!(value["version"], "2.21.1");
        assert_eq!(value.as_object().unwrap().len(), 3);
    }
    #[cfg(feature = "gui")]
    #[test]
    fn private_endpoint_and_bearer_are_runtime_only_and_redirects_are_disabled() {
        let source = include_str!("updater.rs");
        let config = include_str!("../../tauri.conf.json");
        assert!(source.contains("DIANDIAN_UPDATE_ENDPOINT"));
        assert!(source.contains("Authorization"));
        assert!(source.contains("reqwest_012::redirect::Policy::none()"));
        assert!(!config.contains("github.com/25-svg/diandian"));
    }
}
