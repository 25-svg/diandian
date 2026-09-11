use crate::private_distribution::{
    client::{ActivationResponse, DistributionClient, RenewalResponse},
    lease::{compiled_public_key, evaluate_lease_at, verify_lease},
    model::{valid_text, valid_time, LicenseError, LicenseStatus, StoredCredential},
    vault::CredentialVault,
};
use std::{
    collections::HashSet,
    future::Future,
    io::{Read, Write},
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

// Serialize read/activate/renew across awaits, including persistence and trusted time.
static LICENSE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// If the disk fails during revocation, this process must still never reauthorize.
static REVOKED_IN_PROCESS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatusDto {
    pub status: &'static str,
    pub days_remaining: Option<i64>,
    pub message: &'static str,
}
fn dto(status: LicenseStatus, days: Option<i64>) -> LicenseStatusDto {
    let (status, message) = match status {
        LicenseStatus::Unactivated => ("unactivated", "请输入管理员提供的激活码，授权此电脑。"),
        LicenseStatus::Valid => ("valid", "设备已授权。"),
        LicenseStatus::OfflineGrace => ("offline_grace", "当前使用离线授权，请联网续期。"),
        LicenseStatus::Expired => ("expired", "离线授权已到期，请联网重新验证。"),
        LicenseStatus::Revoked => ("revoked", "此设备授权已被撤销，请联系管理员。"),
        LicenseStatus::ClockInvalid => ("clock_invalid", "系统时间异常，请校准时间后联网验证。"),
    };
    LicenseStatusDto {
        status,
        days_remaining: days,
        message,
    }
}
fn safe_error(error: LicenseError) -> LicenseStatusDto {
    LicenseStatusDto {
        status: "blocked",
        days_remaining: None,
        message: match error {
            LicenseError::Configuration => "授权服务尚未配置，请联系管理员获取正式安装包。",
            LicenseError::InvalidInput | LicenseError::InvalidCode => {
                "激活信息无效或已到期，请核对后重试。"
            }
            LicenseError::CodeUsed | LicenseError::AlreadyActivated => {
                "激活码或此安装已绑定，请联系管理员处理。"
            }
            LicenseError::Unavailable => "授权操作未完成，请检查网络后重试。",
            _ => "无法安全验证授权，请联网重试或联系管理员。",
        },
    }
}

struct LicenseStore {
    root: PathBuf,
    vault: CredentialVault,
}
impl LicenseStore {
    fn at(root: PathBuf) -> Self {
        Self {
            vault: CredentialVault::at(root.join("device.dpapi")),
            root,
        }
    }
    fn install_id(&self, existing: Option<&StoredCredential>) -> Result<String, LicenseError> {
        if let Some(existing) = existing {
            return Ok(existing.install_id.clone());
        }
        std::fs::create_dir_all(&self.root).map_err(|_| LicenseError::Vault)?;
        let path = self.root.join("install-id");
        match std::fs::File::open(&path) {
            Ok(file) => {
                let mut id = String::new();
                file.take(37)
                    .read_to_string(&mut id)
                    .map_err(|_| LicenseError::Vault)?;
                if id.len() != 36 || uuid::Uuid::parse_str(&id).is_err() {
                    return Err(LicenseError::Vault);
                }
                Ok(id)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let id = uuid::Uuid::new_v4().to_string();
                let mut file =
                    tempfile::NamedTempFile::new_in(&self.root).map_err(|_| LicenseError::Vault)?;
                file.write_all(id.as_bytes())
                    .and_then(|_| file.as_file().sync_all())
                    .map_err(|_| LicenseError::Vault)?;
                // No overwrite: a concurrent process must reuse the winner's ID.
                match file.persist_noclobber(path) {
                    Ok(_) => Ok(id),
                    Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                        self.install_id(None)
                    }
                    Err(_) => Err(LicenseError::Vault),
                }
            }
            Err(_) => Err(LicenseError::Vault),
        }
    }
    fn revoked(&self) -> Result<bool, LicenseError> {
        if REVOKED_IN_PROCESS
            .lock()
            .map_err(|_| LicenseError::Vault)?
            .contains(&self.root)
        {
            return Ok(true);
        }
        match std::fs::metadata(self.root.join("revoked")) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(LicenseError::Vault),
        }
    }
    fn revoke(&self) -> Result<(), LicenseError> {
        self.revoke_with_marker(|| {
            std::fs::create_dir_all(&self.root).map_err(|_| LicenseError::Vault)?;
            let mut file =
                tempfile::NamedTempFile::new_in(&self.root).map_err(|_| LicenseError::Vault)?;
            file.write_all(b"revoked")
                .and_then(|_| file.as_file().sync_all())
                .map_err(|_| LicenseError::Vault)?;
            file.persist(self.root.join("revoked"))
                .map_err(|_| LicenseError::Vault)?;
            Ok(())
        })
    }
    // Keep every marker I/O step inside one failure boundary. The writer seam is
    // private to this module and never exposed through a Tauri command.
    fn revoke_with_marker(
        &self,
        write_marker: impl FnOnce() -> Result<(), LicenseError>,
    ) -> Result<(), LicenseError> {
        REVOKED_IN_PROCESS
            .lock()
            .map_err(|_| LicenseError::Vault)?
            .insert(self.root.clone());
        if write_marker().is_err() {
            // Best effort removal prevents a stale lease surviving a failed marker write.
            let _ = self.vault.clear();
            return Err(LicenseError::Vault);
        }
        Ok(())
    }
    fn accept(&self, credential: &StoredCredential) -> Result<(), LicenseError> {
        self.vault.store(credential)?;
        match std::fs::remove_file(self.root.join("revoked")) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(LicenseError::Vault),
        }
        REVOKED_IN_PROCESS
            .lock()
            .map_err(|_| LicenseError::Vault)?
            .remove(&self.root);
        Ok(())
    }
}
fn local_status(
    store: &LicenseStore,
    key: &[u8],
    now: i64,
) -> Result<LicenseStatusDto, LicenseError> {
    if store.revoked()? {
        return Ok(dto(LicenseStatus::Revoked, None));
    }
    let Some(credential) = store.vault.load()? else {
        return Ok(dto(LicenseStatus::Unactivated, None));
    };
    let status = evaluate_lease_at(&credential.lease, key, now, credential.last_server_time)?;
    let days = if status == LicenseStatus::OfflineGrace {
        Some((verify_lease(&credential.lease, key)?.expires_at - now + 86399) / 86400)
    } else {
        None
    };
    Ok(dto(status, days))
}
async fn activate_with<F, Fut>(
    store: &LicenseStore,
    key: &[u8],
    code: &str,
    label: &str,
    clock: impl Fn() -> Result<i64, LicenseError>,
    activate: F,
) -> Result<LicenseStatusDto, LicenseError>
where
    F: FnOnce(String, String, String) -> Fut,
    Fut: Future<Output = Result<ActivationResponse, LicenseError>>,
{
    if !valid_text(code, 256) || !valid_text(label, 256) {
        return Err(LicenseError::InvalidInput);
    }
    let old = store.vault.load()?;
    let install_id = store.install_id(old.as_ref())?;
    let response = activate(install_id.clone(), code.to_owned(), label.to_owned()).await?;
    // The wire response has no separate deviceId or serverTime. Both come only
    // from the verified signed claims; renewal pins the existing device identity.
    let claims = verify_lease(&response.lease, key)?;
    if old
        .as_ref()
        .is_some_and(|old| claims.server_time < old.last_server_time)
    {
        return Err(LicenseError::InvalidResponse);
    }
    let now = clock()?;
    let status = evaluate_lease_at(&response.lease, key, now, claims.server_time)?;
    if !matches!(status, LicenseStatus::Valid | LicenseStatus::OfflineGrace) {
        return Ok(dto(status, None));
    }
    store.accept(&StoredCredential {
        install_id,
        device_token: response.device_token,
        lease: response.lease,
        last_server_time: claims.server_time,
    })?;
    local_status(store, key, now)
}
async fn renew_with<F, Fut>(
    store: &LicenseStore,
    key: &[u8],
    clock: impl Fn() -> Result<i64, LicenseError>,
    renew: F,
) -> Result<LicenseStatusDto, LicenseError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<RenewalResponse, LicenseError>>,
{
    let Some(mut credential) = store.vault.load()? else {
        return local_status(store, key, clock()?);
    };
    let old_claims = verify_lease(&credential.lease, key)?;
    match renew(credential.device_token.clone()).await {
        Ok(response) => {
            let claims = verify_lease(&response.lease, key)?;
            if claims.device_id != old_claims.device_id
                || claims.server_time < credential.last_server_time.max(old_claims.server_time)
            {
                return Err(LicenseError::InvalidResponse);
            }
            let now = clock()?;
            let status = evaluate_lease_at(&response.lease, key, now, claims.server_time)?;
            if !matches!(status, LicenseStatus::Valid | LicenseStatus::OfflineGrace) {
                return Ok(dto(status, None));
            }
            credential.lease = response.lease;
            credential.last_server_time = claims.server_time;
            store.accept(&credential)?;
            local_status(store, key, now)
        }
        Err(LicenseError::Revoked | LicenseError::InvalidToken) => {
            store.revoke()?;
            Ok(dto(LicenseStatus::Revoked, None))
        }
        Err(LicenseError::Unavailable) => local_status(store, key, clock()?),
        Err(error) => Err(error),
    }
}
#[cfg(feature = "gui")]
fn app_store(app: &tauri::AppHandle) -> Result<LicenseStore, LicenseError> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| LicenseError::Vault)?
        .join("private-distribution");
    Ok(LicenseStore::at(root))
}
fn now() -> Result<i64, LicenseError> {
    let time = chrono::Utc::now().timestamp();
    if !valid_time(time) {
        return Err(LicenseError::InvalidInput);
    }
    Ok(time)
}
#[cfg(feature = "gui")]
#[tauri::command]
pub async fn get_license_status(app: tauri::AppHandle) -> LicenseStatusDto {
    let _guard = LICENSE_LOCK.lock().await;
    (|| {
        let store = app_store(&app)?;
        // A fresh installation can still show activation when build config is absent.
        if store.vault.load()?.is_none() {
            return local_status(&store, &[], now()?);
        }
        local_status(&store, &compiled_public_key()?, now()?)
    })()
    .unwrap_or_else(safe_error)
}
#[cfg(feature = "gui")]
#[tauri::command]
pub async fn activate_device(
    app: tauri::AppHandle,
    code: String,
    label: String,
) -> LicenseStatusDto {
    let _guard = LICENSE_LOCK.lock().await;
    async {
        let store = app_store(&app)?;
        let key = compiled_public_key()?;
        let client = DistributionClient::compiled()?;
        activate_with(
            &store,
            &key,
            code.trim(),
            label.trim(),
            now,
            |id, code, label| async move { client.activate(&code, &id, &label).await },
        )
        .await
    }
    .await
    .unwrap_or_else(safe_error)
}
#[cfg(feature = "gui")]
#[tauri::command]
pub async fn renew_device_license(app: tauri::AppHandle) -> LicenseStatusDto {
    let _guard = LICENSE_LOCK.lock().await;
    async {
        let store = app_store(&app)?;
        if store.vault.load()?.is_none() {
            return local_status(&store, &[], now()?);
        }
        let key = compiled_public_key()?;
        let client = DistributionClient::compiled()?;
        renew_with(&store, &key, now, |token| async move {
            client.renew(&token).await
        })
        .await
    }
    .await
    .unwrap_or_else(safe_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use ring::{
        rand::SystemRandom,
        signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING},
    };

    fn signer() -> EcdsaKeyPair {
        let rng = SystemRandom::new();
        let bytes = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, bytes.as_ref(), &rng).unwrap()
    }
    fn signed(key: &EcdsaKeyPair, device: &str, now: i64) -> String {
        let body = format!(
            r#"{{"deviceId":"{device}","issuedAt":{now},"expiresAt":{},"serverTime":{now}}}"#,
            now + 604800
        );
        let sig = key.sign(&SystemRandom::new(), body.as_bytes()).unwrap();
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(body),
            URL_SAFE_NO_PAD.encode(sig.as_ref())
        )
    }
    fn credential(key: &EcdsaKeyPair) -> StoredCredential {
        StoredCredential {
            install_id: "install".into(),
            device_token: "x".repeat(43),
            lease: signed(key, "device", 100),
            last_server_time: 100,
        }
    }
    #[tokio::test]
    async fn failed_activation_reuses_pending_install_and_never_overwrites_vault() {
        let dir = tempfile::tempdir().unwrap();
        let store = LicenseStore::at(dir.path().to_owned());
        let key = signer();
        let first = store.install_id(None).unwrap();
        assert_eq!(
            activate_with(
                &store,
                key.public_key().as_ref(),
                "code",
                "name",
                || Ok(100),
                |id, _, _| async move {
                    assert_eq!(id, first);
                    Err(LicenseError::Unavailable)
                }
            )
            .await
            .unwrap_err(),
            LicenseError::Unavailable
        );
        let second = store.install_id(None).unwrap();
        assert_eq!(store.install_id(None).unwrap(), second);
        let old = credential(&key);
        store.vault.store(&old).unwrap();
        let before = std::fs::read(store.vault.path()).unwrap();
        assert!(activate_with(
            &store,
            key.public_key().as_ref(),
            "code",
            "name",
            || Ok(100),
            |_, _, _| async {
                Ok(ActivationResponse {
                    device_token: "y".repeat(43),
                    lease: "invalid".into(),
                })
            }
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(store.vault.path()).unwrap(), before);
    }
    #[tokio::test]
    async fn renewal_rejects_device_substitution_and_time_rollback_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = LicenseStore::at(dir.path().to_owned());
        let key = signer();
        store.vault.store(&credential(&key)).unwrap();
        let before = std::fs::read(store.vault.path()).unwrap();
        for lease in [
            signed(&key, "other", 200),
            signed(&key, "device", 99),
            "invalid".into(),
        ] {
            assert!(renew_with(
                &store,
                key.public_key().as_ref(),
                || Ok(200),
                |_| async { Ok(RenewalResponse { lease }) }
            )
            .await
            .is_err());
            assert_eq!(std::fs::read(store.vault.path()).unwrap(), before);
        }
        let lease = signed(&key, "device", 200);
        assert_eq!(
            renew_with(
                &store,
                key.public_key().as_ref(),
                || Ok(200),
                |_| async { Ok(RenewalResponse { lease }) }
            )
            .await
            .unwrap()
            .status,
            "valid"
        );
        assert_eq!(store.vault.load().unwrap().unwrap().last_server_time, 200);
    }
    #[tokio::test]
    async fn expired_signed_online_response_cannot_temporarily_authorize() {
        let dir = tempfile::tempdir().unwrap();
        let store = LicenseStore::at(dir.path().to_owned());
        let key = signer();
        store.vault.store(&credential(&key)).unwrap();
        let lease = signed(&key, "device", 100);
        assert_eq!(
            renew_with(
                &store,
                key.public_key().as_ref(),
                || Ok(604900),
                |_| async { Ok(RenewalResponse { lease }) }
            )
            .await
            .unwrap()
            .status,
            "expired"
        );
    }
    #[tokio::test]
    async fn network_failure_only_uses_unexpired_local_lease_and_revocation_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let store = LicenseStore::at(dir.path().to_owned());
        let key = signer();
        store.vault.store(&credential(&key)).unwrap();
        for (now, expected) in [
            (101, "offline_grace"),
            (604900, "expired"),
            (99, "clock_invalid"),
        ] {
            assert_eq!(
                renew_with(
                    &store,
                    key.public_key().as_ref(),
                    || Ok(now),
                    |_| async { Err(LicenseError::Unavailable) }
                )
                .await
                .unwrap()
                .status,
                expected
            );
        }
        assert_eq!(
            renew_with(
                &store,
                key.public_key().as_ref(),
                || Ok(101),
                |_| async { Err(LicenseError::Revoked) }
            )
            .await
            .unwrap()
            .status,
            "revoked"
        );
        REVOKED_IN_PROCESS.lock().unwrap().remove(&store.root);
        let restarted = LicenseStore::at(dir.path().to_owned());
        assert_eq!(
            local_status(&restarted, key.public_key().as_ref(), 102)
                .unwrap()
                .status,
            "revoked"
        );
        assert_eq!(
            renew_with(
                &restarted,
                key.public_key().as_ref(),
                || Ok(103),
                |_| async { Err(LicenseError::Unavailable) }
            )
            .await
            .unwrap()
            .status,
            "revoked"
        );
    }
    #[test]
    fn marker_directory_failure_clears_old_vault_and_keeps_process_revoked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cannot-create-directory");
        std::fs::write(&root, b"occupied").unwrap();
        let store = LicenseStore {
            root,
            vault: CredentialVault::at(dir.path().join("device.dpapi")),
        };
        store.vault.store(&credential(&signer())).unwrap();
        assert_eq!(store.revoke().unwrap_err(), LicenseError::Vault);
        assert!(store.vault.load().unwrap().is_none(), "stale vault must be removed even when directory creation fails");
        assert!(store.revoked().unwrap());
        REVOKED_IN_PROCESS.lock().unwrap().remove(&store.root);
    }
    #[test]
    fn each_marker_writer_failure_clears_vault_and_keeps_process_revoked() {
        for failed_step in ["create", "temp", "write", "sync", "persist"] {
            let dir = tempfile::tempdir().unwrap();
            let store = LicenseStore::at(dir.path().to_owned());
            let key = signer();
            store.vault.store(&credential(&key)).unwrap();
            // Inject only the unavailable filesystem step, preserving earlier
            // real writes and the real DPAPI vault / revocation behavior.
            let step = |name| {
                if name == failed_step { Err(LicenseError::Vault) } else { Ok(()) }
            };
            let result = store.revoke_with_marker(|| {
                step("create")?;
                std::fs::create_dir_all(&store.root).unwrap();
                step("temp")?;
                let mut file = tempfile::NamedTempFile::new_in(&store.root).unwrap();
                step("write")?;
                file.write_all(b"revoked").unwrap();
                step("sync")?;
                file.as_file().sync_all().unwrap();
                step("persist")?;
                file.persist(store.root.join("revoked")).unwrap();
                Ok(())
            });
            assert_eq!(result.unwrap_err(), LicenseError::Vault, "{failed_step}");
            assert!(store.vault.load().unwrap().is_none(), "{failed_step}");
            assert_eq!(local_status(&store, key.public_key().as_ref(), 101).unwrap().status, "revoked", "{failed_step}");
            assert_eq!(safe_error(LicenseError::Vault).message, "无法安全验证授权，请联网重试或联系管理员。");
            REVOKED_IN_PROCESS.lock().unwrap().remove(&store.root);
            let restarted = LicenseStore::at(store.root.clone());
            assert_eq!(local_status(&restarted, key.public_key().as_ref(), 102).unwrap().status, "unactivated", "{failed_step}");
        }
    }
    #[test]
    fn corrupted_storage_and_missing_key_never_authorize_and_dto_has_no_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let store = LicenseStore::at(dir.path().to_owned());
        assert_eq!(
            local_status(&store, &[], 100).unwrap().status,
            "unactivated"
        );
        std::fs::write(store.vault.path(), b"corrupt").unwrap();
        assert!(local_status(&store, &[], 100).is_err());
        let key = signer();
        store.vault.store(&credential(&key)).unwrap();
        assert!(local_status(&store, &[], 100).is_err());
        let dto = local_status(&store, key.public_key().as_ref(), 101).unwrap();
        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3);
        assert_eq!(json["daysRemaining"], 7);
        for error in [
            LicenseError::Vault,
            LicenseError::Configuration,
            LicenseError::InvalidLease,
            LicenseError::Unavailable,
        ] {
            assert_eq!(safe_error(error).status, "blocked");
            assert!(!serde_json::to_string(&safe_error(error))
                .unwrap()
                .contains(&"x".repeat(43)));
        }
    }
}
