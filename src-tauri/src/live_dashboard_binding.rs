use chrono::{DateTime, NaiveDateTime, Utc};

use crate::database::live_dashboard::LiveDashboardSessionRow;

pub const AUTO_MATCH_WINDOW_SECS: i64 = 15 * 60;
pub const CANDIDATE_WINDOW_SECS: i64 = 2 * 60 * 60;

#[derive(Debug, Clone)]
pub struct RecordForLiveDashboardBinding {
    pub platform: String,
    pub room_id: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveDashboardMatchMethod {
    AutoAccount,
    AutoShop,
}

impl LiveDashboardMatchMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoAccount => "auto_account",
            Self::AutoShop => "auto_shop",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveDashboardMatchCandidate {
    pub session: LiveDashboardSessionRow,
    pub time_delta_seconds: i64,
    pub account_match: bool,
    pub shop_name_match: bool,
}

#[derive(Debug, Clone)]
pub struct LiveDashboardMatchResult {
    pub automatic: Option<(LiveDashboardSessionRow, LiveDashboardMatchMethod)>,
    pub candidates: Vec<LiveDashboardMatchCandidate>,
}

pub fn match_dashboard_sessions(
    record: &RecordForLiveDashboardBinding,
    sessions: &[LiveDashboardSessionRow],
) -> LiveDashboardMatchResult {
    if record.platform.trim() != "douyin" {
        return LiveDashboardMatchResult {
            automatic: None,
            candidates: Vec::new(),
        };
    }

    let candidates = candidate_sessions(record, sessions, AUTO_MATCH_WINDOW_SECS);
    let account_matches = candidates
        .iter()
        .filter(|candidate| candidate.account_match)
        .collect::<Vec<_>>();

    if account_matches.len() == 1 {
        return LiveDashboardMatchResult {
            automatic: Some((
                account_matches[0].session.clone(),
                LiveDashboardMatchMethod::AutoAccount,
            )),
            candidates,
        };
    }

    if account_matches.is_empty() && candidates.len() == 1 && candidates[0].shop_name_match {
        return LiveDashboardMatchResult {
            automatic: Some((
                candidates[0].session.clone(),
                LiveDashboardMatchMethod::AutoShop,
            )),
            candidates,
        };
    }

    LiveDashboardMatchResult {
        automatic: None,
        candidates,
    }
}

pub fn candidate_sessions(
    record: &RecordForLiveDashboardBinding,
    sessions: &[LiveDashboardSessionRow],
    window_seconds: i64,
) -> Vec<LiveDashboardMatchCandidate> {
    let Some(record_time) = parse_timestamp(&record.created_at) else {
        return Vec::new();
    };
    let room_id = record.room_id.trim();
    let normalized_title = normalize_match_text(&record.title);

    let mut candidates = sessions
        .iter()
        .filter_map(|session| {
            let session_time = parse_timestamp(&session.started_at)?;
            let time_delta_seconds = (session_time - record_time).num_seconds().abs();
            if time_delta_seconds > window_seconds {
                return None;
            }
            let account_key = session.account_key.trim();
            let normalized_shop_name = normalize_match_text(&session.shop_name);
            let shop_name_match = !normalized_title.is_empty()
                && !normalized_shop_name.is_empty()
                && (normalized_title.contains(&normalized_shop_name)
                    || normalized_shop_name.contains(&normalized_title));
            Some(LiveDashboardMatchCandidate {
                session: session.clone(),
                time_delta_seconds,
                account_match: !room_id.is_empty() && room_id == account_key,
                shop_name_match,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.time_delta_seconds);
    candidates
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(|value| value.and_utc())
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|value| value.and_utc())
        })
}

fn normalize_match_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: i64, account_key: &str, started_at: &str) -> LiveDashboardSessionRow {
        LiveDashboardSessionRow {
            id,
            account_key: account_key.into(),
            shop_name: "金典拍拍相机专卖店".into(),
            started_at: started_at.into(),
            payment_amount_fen: 23_655_200,
            per_thousand_payment_amount_fen: None,
            viewer_count: None,
            average_online: None,
            average_watch_seconds: None,
            viewer_conversion_rate: None,
            deal_buyer_count: Some(46),
            deal_item_count: Some(51),
            product_click_conversion_rate: Some(0.0308),
            exposure_viewer_rate: Some(0.1012),
            qianchuan_spend_fen: Some(220_151),
            source_file: "valid-live-dashboard.xlsx".into(),
            imported_at: "2026-07-28T08:20:00Z".into(),
        }
    }

    #[test]
    fn binds_the_fixture_session_to_a_same_room_record_within_fifteen_minutes() {
        let record = RecordForLiveDashboardBinding {
            platform: "douyin".into(),
            room_id: "20296833869".into(),
            title: "金典拍拍主做二手精品相机镜头，直播间领取最高400优惠！".into(),
            created_at: "2026-07-28T08:16:00".into(),
        };

        let result =
            match_dashboard_sessions(&record, &[session(1, "20296833869", "2026-07-28T08:15:49")]);

        assert_eq!(
            result.automatic.as_ref().map(|(session, _)| session.id),
            Some(1)
        );
        assert_eq!(
            result.automatic.map(|(_, method)| method),
            Some(LiveDashboardMatchMethod::AutoAccount)
        );
    }

    #[test]
    fn leaves_ambiguous_account_matches_unbound() {
        let record = RecordForLiveDashboardBinding {
            platform: "douyin".into(),
            room_id: "20296833869".into(),
            title: "金典拍拍相机专卖店".into(),
            created_at: "2026-07-28T08:16:00".into(),
        };

        let result = match_dashboard_sessions(
            &record,
            &[
                session(1, "20296833869", "2026-07-28T08:15:49"),
                session(2, "20296833869", "2026-07-28T08:16:10"),
            ],
        );

        assert!(result.automatic.is_none());
        assert_eq!(result.candidates.len(), 2);
    }
}
