use crate::database::live_dashboard::LiveDashboardSessionRow;
use crate::live_dashboard_binding::{
    infer_shop_name_from_texts, match_dashboard_sessions, match_imported_video_by_duration,
    match_imported_video_sessions, session_matches_shop, LiveDashboardMatchCandidate,
    RecordForLiveDashboardBinding,
};
use crate::state::State;
use crate::state_type;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardBindingCandidate {
    pub session: LiveDashboardSessionRow,
    pub time_delta_seconds: i64,
    pub account_match: bool,
    pub shop_name_match: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLiveDashboardResult {
    pub session: Option<LiveDashboardSessionRow>,
    pub candidates: Vec<LiveDashboardBindingCandidate>,
    pub match_method: Option<String>,
}

impl From<LiveDashboardMatchCandidate> for LiveDashboardBindingCandidate {
    fn from(candidate: LiveDashboardMatchCandidate) -> Self {
        Self {
            session: candidate.session,
            time_delta_seconds: candidate.time_delta_seconds,
            account_match: candidate.account_match,
            shop_name_match: candidate.shop_name_match,
        }
    }
}

pub async fn resolve_live_dashboard_for_record_state(
    state: &State,
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<ResolveLiveDashboardResult, String> {
    if let Some(session) = state
        .db
        .get_bound_live_dashboard_session(&live_id)
        .await
        .map_err(|error| error.to_string())?
    {
        // A standardized imported filename is authoritative for shop identity.
        // Ignore an old cross-shop manual binding so resolution can repair it.
        let cross_shop_import_binding = if let Some(video_id) = live_id
            .strip_prefix("import:")
            .and_then(|value| value.parse::<i64>().ok())
        {
            let video = state
                .db
                .get_video(video_id)
                .await
                .map_err(|error| error.to_string())?;
            infer_shop_name_from_texts(&[&video.title, &video.file, &video.note])
                .is_some_and(|shop| !session_matches_shop(&session, shop))
        } else {
            false
        };
        if cross_shop_import_binding {
            // Continue into imported-video matching below. A successful match
            // overwrites the stale binding through the existing UPSERT.
        } else {
            let match_method = state
                .db
                .get_live_dashboard_binding(&live_id)
                .await
                .map_err(|error| error.to_string())?
                .map(|binding| binding.match_method);
            return Ok(ResolveLiveDashboardResult {
                session: Some(session),
                candidates: Vec::new(),
                match_method,
            });
        }
    }

    if platform.trim() == "imported" || live_id.starts_with("import:") {
        let video_id = live_id
            .strip_prefix("import:")
            .and_then(|value| value.parse::<i64>().ok());
        let Some(video_id) = video_id else {
            return Ok(ResolveLiveDashboardResult {
                session: None,
                candidates: Vec::new(),
                match_method: None,
            });
        };
        let video = state
            .db
            .get_video(video_id)
            .await
            .map_err(|error| error.to_string())?;
        let sessions = state
            .db
            .list_live_dashboard_sessions()
            .await
            .map_err(|error| error.to_string())?;
        let mut matched =
            match_imported_video_sessions(&video.title, &video.file, &video.note, &sessions);
        if matched.automatic.is_none() {
            let duration_matched = match_imported_video_by_duration(
                &video.title,
                &video.file,
                &video.note,
                &video.created_at,
                (video.length > 0).then_some(video.length),
                &sessions,
            );
            if duration_matched.automatic.is_some() || matched.candidates.is_empty() {
                matched = duration_matched;
            }
        }
        if let Some((session, method)) = matched.automatic {
            state
                .db
                .bind_live_dashboard_session(&live_id, session.id, method.as_str())
                .await
                .map_err(|error| error.to_string())?;
            return Ok(ResolveLiveDashboardResult {
                session: Some(session),
                candidates: Vec::new(),
                match_method: Some(method.as_str().to_string()),
            });
        }
        return Ok(ResolveLiveDashboardResult {
            session: None,
            candidates: matched.candidates.into_iter().map(Into::into).collect(),
            match_method: None,
        });
    }

    if platform.trim() != "douyin" {
        return Ok(ResolveLiveDashboardResult {
            session: None,
            candidates: Vec::new(),
            match_method: None,
        });
    }

    let record = state
        .db
        .get_record(&room_id, &live_id)
        .await
        .map_err(|error| error.to_string())?;
    let input = RecordForLiveDashboardBinding {
        platform: record.platform,
        room_id: record.room_id,
        title: record.title,
        created_at: record.created_at,
        length_secs: (record.length > 0.0).then_some(record.length.round() as i64),
    };
    let sessions = state
        .db
        .list_live_dashboard_sessions()
        .await
        .map_err(|error| error.to_string())?;
    let matched = match_dashboard_sessions(&input, &sessions);
    if let Some((session, method)) = matched.automatic {
        state
            .db
            .bind_live_dashboard_session(&live_id, session.id, method.as_str())
            .await
            .map_err(|error| error.to_string())?;
        return Ok(ResolveLiveDashboardResult {
            session: Some(session),
            candidates: Vec::new(),
            match_method: Some(method.as_str().to_string()),
        });
    }

    Ok(ResolveLiveDashboardResult {
        session: None,
        candidates: matched.candidates.into_iter().map(Into::into).collect(),
        match_method: None,
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn resolve_live_dashboard_for_record(
    state: state_type!(),
    platform: String,
    room_id: String,
    live_id: String,
) -> Result<ResolveLiveDashboardResult, String> {
    resolve_live_dashboard_for_record_state(&state, platform, room_id, live_id).await
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn bind_live_dashboard_session(
    state: state_type!(),
    live_id: String,
    session_id: i64,
) -> Result<ResolveLiveDashboardResult, String> {
    let session = state
        .db
        .get_live_dashboard_session(session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "直播数据场次不存在".to_string())?;
    if let Some(video_id) = live_id
        .strip_prefix("import:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let video = state
            .db
            .get_video(video_id)
            .await
            .map_err(|error| error.to_string())?;
        if let Some(shop) = infer_shop_name_from_texts(&[&video.title, &video.file, &video.note]) {
            if !session_matches_shop(&session, shop) {
                return Err(format!(
                    "店铺不一致：该视频属于{shop}，不能绑定到{}",
                    session.shop_name
                ));
            }
        }
    }
    state
        .db
        .bind_live_dashboard_session(&live_id, session_id, "manual")
        .await
        .map_err(|error| error.to_string())?;
    Ok(ResolveLiveDashboardResult {
        session: Some(session),
        candidates: Vec::new(),
        match_method: Some("manual".to_string()),
    })
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn list_live_dashboard_bindings_for_live_ids(
    state: state_type!(),
    live_ids: Vec<String>,
) -> Result<Vec<crate::database::live_dashboard_binding::LiveDashboardBindingSummaryRow>, String> {
    state
        .db
        .list_bound_live_dashboard_sessions_for_live_ids(&live_ids)
        .await
        .map_err(|error| error.to_string())
}
