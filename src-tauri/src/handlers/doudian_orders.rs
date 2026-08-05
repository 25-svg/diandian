use crate::doudian_orders::{
    fetch_payment_events, resolve_live_window, FetchDoudianPaymentEventsResult,
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
    let record = state
        .db
        .get_record(&request.room_id, &request.live_id)
        .await
        .map_err(|error| error.to_string())?;

    let dashboard_session = state
        .db
        .get_bound_live_dashboard_session(&request.live_id)
        .await
        .map_err(|error| error.to_string())?;

    let (live_started_at, live_ended_at) = resolve_live_window(
        &record,
        dashboard_session.as_ref(),
        request.live_started_at,
        request.live_ended_at,
    )?;

    let config = state.config.read().await.doudian_order.clone();
    fetch_payment_events(&config, &live_started_at, &live_ended_at).await
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
