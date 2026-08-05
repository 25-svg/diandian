#[cfg(feature = "gui")]
use tauri::State as TauriState;

use crate::state::State;
use crate::{
    database::task::TaskRow,
    security::{audit_tool_failure, audit_tool_success, require_sensitive_write},
    state_type,
};

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_tasks(state: state_type!()) -> Result<Vec<TaskRow>, String> {
    Ok(state.db.get_tasks().await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn delete_task(
    state: state_type!(),
    id: &str,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<(), String> {
    let audit = require_sensitive_write(
        "delete_task",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("task:{id}"),
    )?;
    match state.db.delete_task(id).await {
        Ok(result) => {
            audit_tool_success(&audit);
            Ok(result)
        }
        Err(error) => {
            let error = error.to_string();
            audit_tool_failure(&audit, &error);
            Err(error)
        }
    }
}
