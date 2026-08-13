use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

const MINIMAX_SECRET_FILE: &str = "minimax-api-key.dpapi";
const MINIMAX_SECRET_MAGIC: &[u8] = b"BSR-MINIMAX-KEY-V1\0";

fn minimax_secret_path(config_path: &str) -> Result<PathBuf, String> {
    let parent = Path::new(config_path)
        .parent()
        .ok_or_else(|| "无法确定 MiniMax 安全配置目录".to_string())?;
    Ok(parent.join(MINIMAX_SECRET_FILE))
}

#[cfg(target_os = "windows")]
fn protect_for_current_windows_user(secret: &[u8]) -> Result<Vec<u8>, String> {
    use windows::core::w;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: secret
            .len()
            .try_into()
            .map_err(|_| "MiniMax API Key 长度无效".to_string())?,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            w!("典典直播切片 MiniMax API Key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| format!("Windows 加密 MiniMax API Key 失败：{error}"))?;
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(protected)
    }
}

#[cfg(target_os = "windows")]
fn unprotect_for_current_windows_user(protected: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: protected
            .len()
            .try_into()
            .map_err(|_| "MiniMax 安全配置长度无效".to_string())?,
        pbData: protected.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| format!("当前 Windows 用户无法解密 MiniMax API Key：{error}"))?;
        let secret = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(secret)
    }
}

#[cfg(not(target_os = "windows"))]
fn protect_for_current_windows_user(_secret: &[u8]) -> Result<Vec<u8>, String> {
    Err("当前版本仅支持在 Windows 安全保存 MiniMax API Key".to_string())
}

#[cfg(not(target_os = "windows"))]
fn unprotect_for_current_windows_user(_protected: &[u8]) -> Result<Vec<u8>, String> {
    Err("当前版本仅支持在 Windows 读取 MiniMax API Key".to_string())
}

pub fn store_minimax_api_key(config_path: &str, api_key: &str) -> Result<(), String> {
    let api_key = api_key.trim();
    if api_key.len() < 12 {
        return Err("MiniMax API Key 格式无效，请粘贴完整密钥".to_string());
    }
    let protected = protect_for_current_windows_user(api_key.as_bytes())?;
    let path = minimax_secret_path(config_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建 MiniMax 安全配置目录失败：{error}"))?;
    }
    let mut payload = Vec::with_capacity(MINIMAX_SECRET_MAGIC.len() + protected.len());
    payload.extend_from_slice(MINIMAX_SECRET_MAGIC);
    payload.extend_from_slice(&protected);
    let temporary = path.with_extension("dpapi.tmp");
    std::fs::write(&temporary, payload)
        .map_err(|error| format!("写入 MiniMax 安全配置失败：{error}"))?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| format!("更新 MiniMax 安全配置失败：{error}"))?;
    }
    std::fs::rename(&temporary, &path)
        .map_err(|error| format!("保存 MiniMax 安全配置失败：{error}"))
}

pub fn load_minimax_api_key(config_path: &str) -> Result<Option<String>, String> {
    let path = minimax_secret_path(config_path)?;
    if !path.is_file() {
        return Ok(None);
    }
    let payload =
        std::fs::read(&path).map_err(|error| format!("读取 MiniMax 安全配置失败：{error}"))?;
    let protected = payload
        .strip_prefix(MINIMAX_SECRET_MAGIC)
        .ok_or_else(|| "MiniMax 安全配置格式无效，请重新初始化".to_string())?;
    let secret = unprotect_for_current_windows_user(protected)?;
    let api_key = String::from_utf8(secret)
        .map_err(|_| "MiniMax 安全配置内容无效，请重新初始化".to_string())?;
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Ok(None);
    }
    Ok(Some(api_key))
}

static IDEMPOTENCY_KEYS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct ToolAuditContext {
    pub tool_name: &'static str,
    pub trace_id: String,
    pub idempotency_key: String,
    pub resource: String,
    started_at: Instant,
}

pub fn require_sensitive_write(
    tool_name: &'static str,
    idempotency_key: &str,
    confirmation_token: &str,
    trace_id: Option<&str>,
    resource: &str,
) -> Result<ToolAuditContext, String> {
    let trace_id = trace_id
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("tr_{}", uuid::Uuid::new_v4()));

    audit_tool_call(
        tool_name,
        &trace_id,
        "received",
        0,
        None,
        Some(idempotency_key),
        resource,
    );

    if idempotency_key.trim().len() < 8 || idempotency_key.len() > 128 {
        audit_tool_call(
            tool_name,
            &trace_id,
            "rejected",
            0,
            Some("VALIDATION_ERROR:idempotency_key"),
            Some(idempotency_key),
            resource,
        );
        return Err("VALIDATION_ERROR: idempotency_key length must be 8..128".to_string());
    }

    let expected_confirmation = format!("confirm:{tool_name}");
    if confirmation_token != expected_confirmation {
        audit_tool_call(
            tool_name,
            &trace_id,
            "rejected",
            0,
            Some("CONFIRMATION_REQUIRED"),
            Some(idempotency_key),
            resource,
        );
        return Err(format!(
            "CONFIRMATION_REQUIRED: confirmation_token must be {expected_confirmation}"
        ));
    }

    reserve_idempotency_key(tool_name, idempotency_key, resource, &trace_id)?;

    Ok(ToolAuditContext {
        tool_name,
        trace_id,
        idempotency_key: idempotency_key.to_string(),
        resource: resource.to_string(),
        started_at: Instant::now(),
    })
}

pub fn audit_tool_success(context: &ToolAuditContext) {
    audit_tool_call(
        context.tool_name,
        &context.trace_id,
        "succeeded",
        context.started_at.elapsed().as_millis(),
        None,
        Some(&context.idempotency_key),
        &context.resource,
    );
}

pub fn audit_tool_failure(context: &ToolAuditContext, error: &str) {
    audit_tool_call(
        context.tool_name,
        &context.trace_id,
        "failed",
        context.started_at.elapsed().as_millis(),
        Some(error),
        Some(&context.idempotency_key),
        &context.resource,
    );
}

pub fn audit_tool_call(
    tool_name: &str,
    trace_id: &str,
    status: &str,
    duration_ms: u128,
    error: Option<&str>,
    idempotency_key: Option<&str>,
    resource: &str,
) {
    log::info!(
        target: "tool_audit",
        "trace_id={} tool={} status={} duration_ms={} idempotency_key_present={} resource={} error={}",
        trace_id,
        tool_name,
        status,
        duration_ms,
        idempotency_key.is_some(),
        redact_resource(resource),
        error.unwrap_or("")
    );
}

fn reserve_idempotency_key(
    tool_name: &str,
    idempotency_key: &str,
    resource: &str,
    trace_id: &str,
) -> Result<(), String> {
    let key = format!("{tool_name}:{idempotency_key}");
    let fingerprint = format!("{tool_name}:{resource}");
    let store = IDEMPOTENCY_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut store = store
        .lock()
        .map_err(|_| "IDEMPOTENCY_ERROR: idempotency store lock failed".to_string())?;

    if let Some(existing) = store.get(&key) {
        let error = if existing == &fingerprint {
            "IDEMPOTENT_REPLAY: idempotency_key was already accepted for this operation"
        } else {
            "CONFLICT: idempotency_key was already used with different arguments"
        };
        audit_tool_call(
            tool_name,
            trace_id,
            "rejected",
            0,
            Some(error),
            Some(idempotency_key),
            resource,
        );
        return Err(error.to_string());
    }

    store.insert(key, fingerprint);
    Ok(())
}

fn redact_resource(resource: &str) -> String {
    if resource.chars().count() > 256 {
        format!("{}...", resource.chars().take(256).collect::<String>())
    } else {
        resource.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::require_sensitive_write;

    #[test]
    fn test_sensitive_write_requires_confirmation() {
        let result = require_sensitive_write(
            "delete_video",
            "idem-confirm-1",
            "wrong",
            Some("tr_test_confirm"),
            "video:1",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CONFIRMATION_REQUIRED"));
    }

    #[test]
    fn test_sensitive_write_rejects_short_idempotency_key() {
        let result = require_sensitive_write(
            "delete_video",
            "short",
            "confirm:delete_video",
            Some("tr_test_short"),
            "video:1",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("VALIDATION_ERROR"));
    }

    #[test]
    fn test_sensitive_write_rejects_idempotency_conflict() {
        let first = require_sensitive_write(
            "delete_archive",
            "idem-conflict-1",
            "confirm:delete_archive",
            Some("tr_test_first"),
            "archive:1",
        );
        assert!(first.is_ok());

        let second = require_sensitive_write(
            "delete_archive",
            "idem-conflict-1",
            "confirm:delete_archive",
            Some("tr_test_second"),
            "archive:2",
        );
        assert!(second.is_err());
        assert!(second.unwrap_err().contains("CONFLICT"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn minimax_api_key_round_trips_through_windows_dpapi() {
        let root = tempfile::tempdir().unwrap();
        let config_path = root.path().join("Conf.toml");
        let config_path = config_path.to_string_lossy();
        super::store_minimax_api_key(&config_path, "test-minimax-key-123456789").unwrap();
        let loaded = super::load_minimax_api_key(&config_path).unwrap();
        assert_eq!(loaded.as_deref(), Some("test-minimax-key-123456789"));
        let raw = std::fs::read(root.path().join(super::MINIMAX_SECRET_FILE)).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("test-minimax-key-123456789"));
    }
}
