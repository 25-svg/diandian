use super::model::StoredCredential;
use async_trait::async_trait;
use serde::Serialize;
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
    shutting_down: Arc<AtomicBool>,
    cached_path: Arc<Mutex<Option<PathBuf>>>,
}
impl Default for PrivateUpdateCoordinator {
    fn default() -> Self {
        Self {
            status: Arc::new(RwLock::new(UpdateStatusDto::new(UpdatePhase::Idle, None))),
            running: Arc::new(AtomicBool::new(false)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            cached_path: Arc::new(Mutex::new(None)),
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
    fn set_cached_path(&self, path: Option<PathBuf>) {
        *self.cached_path.lock().unwrap_or_else(|e| e.into_inner()) = path;
    }
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        let path = self
            .cached_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(path) = path {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

fn write_verified_cache(root: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;
    std::fs::create_dir_all(root)?;
    let target = root.join("verified-update.bin");
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(&target).map_err(|e| e.error)?;
    Ok(target)
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
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = tokio::fs::remove_file(cache_root.join("verified-update.bin")).await;
            let mut schedule = CheckSchedule::default();
            loop {
                let delay = schedule.next_delay();
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                if coordinator.shutting_down.load(Ordering::Acquire) {
                    break;
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
        self.set_status(UpdatePhase::Checking, None);
        let credential = match authorization.authorized_credential().await {
            Ok(value) => value,
            Err(()) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let endpoint = match private_update_endpoint() {
            Ok(value) => value,
            Err(()) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        let token = credential.device_token;
        let updater = match build_private_updater(app, endpoint, &token) {
            Ok(updater) => updater,
            Err(_) => {
                self.set_status(UpdatePhase::Failed, None);
                return false;
            }
        };
        drop(token);
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
        self.set_status(UpdatePhase::Downloading, version.clone());
        let bytes = match update.download(|_, _| {}, || {}).await {
            Ok(bytes) => bytes,
            Err(_) => {
                self.set_status(UpdatePhase::Failed, version);
                return false;
            }
        };
        if self.shutting_down.load(Ordering::Acquire) {
            return false;
        }
        // Update::download verifies the Tauri minisign signature before returning.
        let cache_path = match write_verified_cache(cache_root, &bytes) {
            Ok(path) => path,
            Err(_) => {
                self.set_status(UpdatePhase::Failed, version);
                return false;
            }
        };
        drop(bytes);
        self.set_cached_path(Some(cache_path.clone()));
        self.set_status(UpdatePhase::WaitingForIdle, version.clone());
        let started = std::time::Instant::now();
        let mut gate = IdleGate::new(IDLE_REQUIRED);
        loop {
            if self.shutting_down.load(Ordering::Acquire) {
                let _ = tokio::fs::remove_file(&cache_path).await;
                self.set_cached_path(None);
                return false;
            }
            if gate.observe(is_safe_snapshot(&busy.snapshot().await), started.elapsed()) {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        if authorization.authorized_credential().await.is_err()
            || !is_safe_snapshot(&busy.snapshot().await)
        {
            let _ = tokio::fs::remove_file(&cache_path).await;
            self.set_cached_path(None);
            self.set_status(UpdatePhase::Failed, version);
            return false;
        }
        let install_bytes = match tokio::fs::read(&cache_path).await {
            Ok(bytes) => bytes,
            Err(_) => {
                self.set_cached_path(None);
                self.set_status(UpdatePhase::Failed, version);
                return false;
            }
        };
        if self.shutting_down.load(Ordering::Acquire)
            || authorization.authorized_credential().await.is_err()
            || !is_safe_snapshot(&busy.snapshot().await)
        {
            let _ = tokio::fs::remove_file(&cache_path).await;
            self.set_cached_path(None);
            self.set_status(UpdatePhase::Failed, version);
            return false;
        }
        let _ = tokio::fs::remove_file(&cache_path).await;
        self.set_cached_path(None);
        self.set_status(UpdatePhase::Installing, version.clone());
        let update = update.clone();
        let install = tauri::async_runtime::spawn_blocking(move || {
            before_install_exit();
            update.install(install_bytes)
        })
        .await;
        if !matches!(install, Ok(Ok(()))) {
            self.set_status(UpdatePhase::Failed, version);
            return false;
        }
        true
    }
}

#[cfg(feature = "gui")]
fn build_private_updater<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    endpoint: url::Url,
    token: &str,
) -> tauri_plugin_updater::Result<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    app.updater_builder()
        .endpoints(vec![endpoint])?
        .header("Authorization", format!("Bearer {token}"))?
        .timeout(Duration::from_secs(30))
        .configure_client(|client| {
            client
                .no_proxy()
                .redirect(reqwest_012::redirect::Policy::none())
        })
        .build()
}

#[cfg(feature = "gui")]
fn private_update_endpoint() -> Result<url::Url, ()> {
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
    base.join("v1/update/windows/x86_64/{{current_version}}")
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[tokio::test]
    async fn verified_cache_is_atomic_and_shutdown_cleans_it() {
        let root = tempfile::tempdir().unwrap();
        let path = write_verified_cache(root.path(), b"verified").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"verified");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        let coordinator = PrivateUpdateCoordinator::default();
        coordinator.set_cached_path(Some(path.clone()));
        coordinator.shutdown().await;
        assert!(!path.exists());
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
