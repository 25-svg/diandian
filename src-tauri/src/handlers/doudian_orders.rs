use crate::database::DatabaseError;
use crate::doudian_orders::{
    fetch_payment_events, resolve_doudian_order_config, resolve_doudian_order_config_for_shop,
    resolve_live_window, FetchDoudianPaymentEventsResult,
};
use crate::state::State;
use crate::state_type;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchDoudianPaymentEventsRequest {
    pub room_id: String,
    pub live_id: String,
    pub live_started_at: Option<String>,
    pub live_ended_at: Option<String>,
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn fetch_doudian_payment_events(
    state: state_type!(),
    room_id: String,
    live_id: String,
    live_started_at: Option<String>,
    live_ended_at: Option<String>,
) -> Result<FetchDoudianPaymentEventsResult, String> {
    fetch_doudian_payment_events_state(
        &state,
        FetchDoudianPaymentEventsRequest {
            room_id,
            live_id,
            live_started_at,
            live_ended_at,
        },
    )
    .await
}

pub async fn fetch_doudian_payment_events_state(
    state: &State,
    request: FetchDoudianPaymentEventsRequest,
) -> Result<FetchDoudianPaymentEventsResult, String> {
    let record = match state
        .db
        .get_record(&request.room_id, &request.live_id)
        .await
    {
        Ok(row) => Some(row),
        Err(DatabaseError::NotFound) => None,
        Err(error) => return Err(error.to_string()),
    };

    let dashboard_session = state
        .db
        .get_bound_live_dashboard_session(&request.live_id)
        .await
        .map_err(|error| error.to_string())?;

    if let (Some(record), Some(session)) = (record.as_ref(), dashboard_session.as_ref()) {
        let binding_input = crate::live_dashboard_binding::RecordForLiveDashboardBinding {
            platform: record.platform.clone(),
            room_id: record.room_id.clone(),
            title: record.title.clone(),
            created_at: record.created_at.clone(),
            length_secs: (record.length > 0.0).then_some(record.length.round() as i64),
        };
        if crate::live_dashboard_binding::bound_session_conflicts_with_record(
            &binding_input,
            session,
        ) {
            if state
                .db
                .get_live_dashboard_binding(&request.live_id)
                .await
                .map_err(|error| error.to_string())?
                .is_some_and(|binding| binding.match_method != "manual")
            {
                state
                    .db
                    .delete_live_dashboard_binding(&request.live_id)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            return Err(format!(
                "场次绑定日期不一致：当前录像是 {}，Excel 是 {}。已清除错误自动绑定，请导入并绑定同一天的整场 Excel",
                record.created_at.get(..10).unwrap_or(&record.created_at),
                session.started_at.get(..10).unwrap_or(&session.started_at)
            ));
        }
    }

    let (live_started_at, live_ended_at) = resolve_live_window(
        record.as_ref(),
        dashboard_session.as_ref(),
        request.live_started_at,
        request.live_ended_at,
    )?;

    let configured = state.config.read().await.doudian_order.clone();
    let resolved_config = if let Some(session) = dashboard_session.as_ref() {
        resolve_doudian_order_config_for_shop(&configured, &session.shop_name)?
    } else {
        resolve_doudian_order_config(&configured)
    };
    let mut result =
        fetch_payment_events(&resolved_config, &live_started_at, &live_ended_at).await?;
    if resolved_config.env_file != configured.env_file
        || resolved_config.token_file != configured.token_file
        || resolved_config.sdk_path != configured.sdk_path
        || resolved_config.shop_id != configured.shop_id
    {
        let mut app_config = state.config.write().await;
        app_config.doudian_order = resolved_config;
        app_config.save();
    }
    let orders_payload = result.orders_payload.take();
    if let (Some(session), Some(orders_payload)) = (dashboard_session.as_ref(), orders_payload) {
        let products = state
            .db
            .list_live_dashboard_products(session.id)
            .await
            .map_err(|error| error.to_string())?;
        let pull_summary = result.summary.clone();
        let imported = crate::raw_order_timeline::match_live_window_orders_to_session_for_room(
            &orders_payload,
            session,
            &products,
            Some("order.searchList"),
            Some(&request.room_id),
        )?;

        let candidate_event_count = pull_summary
            .get("event_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        let candidate_total_fen = pull_summary
            .get("total_pay_amount_yuan")
            .and_then(serde_json::Value::as_f64)
            .map(|yuan| (yuan * 100.0).round() as i64)
            .unwrap_or(0);
        let status_stats = result.stats.as_ref().and_then(|stats| stats.get("status"));
        let status_i64 = |name: &str| {
            status_stats
                .and_then(|stats| stats.get(name))
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
        };
        let attributed_closed = imported
            .events
            .iter()
            .filter(|event| is_closed_or_refunded_status(&event.order_status))
            .collect::<Vec<_>>();
        let attributed_closed_amount_fen = attributed_closed
            .iter()
            .filter_map(|event| event.pay_amount_fen)
            .sum::<i64>();
        let unmatched_event_count = candidate_event_count.saturating_sub(imported.events.len());
        let unmatched_amount_fen =
            (candidate_total_fen - imported.summary.total_pay_amount_fen).max(0);

        result.summary = serde_json::json!({
            "event_count": imported.summary.event_count,
            "total_pay_amount_yuan": (imported.summary.total_pay_amount_fen as f64) / 100.0,
            "live_started_at": session.started_at,
            "shop_id": pull_summary.get("shop_id").cloned().unwrap_or(serde_json::Value::Null),
            "shop_name": imported.summary.shop_name,
            "expected_event_count": imported.summary.expected_event_count,
            "expected_pay_amount_yuan": (imported.summary.expected_pay_amount_fen as f64) / 100.0,
            "candidate_event_count": candidate_event_count,
            "candidate_total_pay_amount_yuan": (candidate_total_fen as f64) / 100.0,
            "current_valid_event_count": status_i64("current_valid_event_count"),
            "current_valid_amount_yuan": (status_i64("current_valid_amount_fen") as f64) / 100.0,
            "closed_or_refunded_event_count": status_i64("closed_or_refunded_event_count"),
            "closed_or_refunded_amount_yuan": (status_i64("closed_or_refunded_amount_fen") as f64) / 100.0,
            "attributed_closed_or_refunded_event_count": attributed_closed.len(),
            "attributed_closed_or_refunded_amount_yuan": (attributed_closed_amount_fen as f64) / 100.0,
            "unmatched_event_count": unmatched_event_count,
            "unmatched_amount_yuan": (unmatched_amount_fen as f64) / 100.0,
            "confidence": imported.summary.confidence,
        });
        result.events =
            serde_json::to_value(&imported.events).map_err(|error| error.to_string())?;
        result.source_label = Some(imported.source_label);
    }
    Ok(result)
}

fn is_closed_or_refunded_status(status: &str) -> bool {
    let normalized = status.trim();
    normalized == "4"
        || ["关闭", "取消", "退款", "售后"]
            .into_iter()
            .any(|label| normalized.contains(label))
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn import_raw_doudian_payment_events(
    state: state_type!(),
    live_id: String,
    path: String,
) -> Result<crate::raw_order_timeline::RawOrderTimelineImportResult, String> {
    let session = state
        .db
        .get_bound_live_dashboard_session(&live_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "请先绑定对应的直播 Excel 场次，再导入原始订单 JSON".to_string())?;
    let products = state
        .db
        .list_live_dashboard_products(session.id)
        .await
        .map_err(|error| error.to_string())?;
    crate::raw_order_timeline::import_raw_order_timeline(
        std::path::Path::new(&path),
        &session,
        &products,
    )
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_doudian_order_config(
    state: state_type!(),
) -> Result<crate::config::DoudianOrderConfig, String> {
    Ok(state.config.read().await.doudian_order.clone())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn update_doudian_order_config(
    state: state_type!(),
    env_file: String,
    token_file: String,
    sdk_path: String,
    shop_id: String,
) -> Result<(), String> {
    let mut config = state.config.write().await;
    config.doudian_order = crate::config::DoudianOrderConfig {
        env_file,
        token_file,
        sdk_path,
        shop_id,
    };
    config.save();
    Ok(())
}
