use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};

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
}
