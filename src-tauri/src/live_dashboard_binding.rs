use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use regex::Regex;

use crate::database::live_dashboard::LiveDashboardSessionRow;

pub const AUTO_MATCH_WINDOW_SECS: i64 = 15 * 60;
pub const CANDIDATE_WINDOW_SECS: i64 = 2 * 60 * 60;
pub const DURATION_AUTO_MATCH_SECS: i64 = 20 * 60;
pub const DURATION_CANDIDATE_SECS: i64 = 45 * 60;
const KNOWN_SHOPS: [&str; 2] = ["金典拍拍科创专卖店", "金典拍拍相机专卖店"];

#[derive(Debug, Clone)]
pub struct RecordForLiveDashboardBinding {
    pub platform: String,
    pub room_id: String,
    pub title: String,
    pub created_at: String,
    pub length_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveDashboardMatchMethod {
    AutoAccount,
    AutoShop,
    AutoFilename,
}

impl LiveDashboardMatchMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoAccount => "auto_account",
            Self::AutoShop => "auto_shop",
            Self::AutoFilename => "auto_filename",
        }
    }
}

pub fn infer_live_started_at_from_texts(values: &[&str]) -> Option<String> {
    let separated = Regex::new(
        r"(?x)(20\d{2})[-_/年\.](\d{1,2})[-_/月\.](\d{1,2})(?:日)?[T _-]+(\d{1,2})[-_:时](\d{1,2})(?:[-_:分](\d{1,2}))?",
    )
    .ok()?;
    let compact = Regex::new(r"(20\d{2})(\d{2})(\d{2})[T _-]?(\d{2})(\d{2})(\d{2})").ok()?;

    for value in values {
        let captures = separated
            .captures(value)
            .or_else(|| compact.captures(value));
        let Some(captures) = captures else { continue };
        let year = captures.get(1)?.as_str().parse::<i32>().ok()?;
        let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
        let day = captures.get(3)?.as_str().parse::<u32>().ok()?;
        let hour = captures.get(4)?.as_str().parse::<u32>().ok()?;
        let minute = captures.get(5)?.as_str().parse::<u32>().ok()?;
        let second = captures
            .get(6)
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let inferred =
            NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
        return Some(inferred.format("%Y-%m-%dT%H:%M:%S").to_string());
    }
    None
}

pub fn infer_shop_name_from_texts(values: &[&str]) -> Option<&'static str> {
    let identity = normalize_match_text(&values.join(" "));
    KNOWN_SHOPS
        .into_iter()
        .find(|shop| identity.contains(&normalize_match_text(shop)))
}

pub fn session_matches_shop(session: &LiveDashboardSessionRow, shop: &str) -> bool {
    normalize_match_text(&session.shop_name).contains(&normalize_match_text(shop))
}

pub fn imported_video_candidate_sessions(
    title: &str,
    file: &str,
    note: &str,
    sessions: &[LiveDashboardSessionRow],
    window_seconds: i64,
) -> Vec<LiveDashboardMatchCandidate> {
    let Some(inferred_started_at) = infer_live_started_at_from_texts(&[title, file, note]) else {
        return Vec::new();
    };
    let Some(video_time) = parse_timestamp(&inferred_started_at) else {
        return Vec::new();
    };
    let source_values = [title, file, note];
    let identity = normalize_match_text(&source_values.join(" "));
    let explicit_shop = infer_shop_name_from_texts(&source_values);
    let mut candidates = sessions
        .iter()
        .filter_map(|session| {
            if explicit_shop.is_some_and(|shop| !session_matches_shop(session, shop)) {
                return None;
            }
            let session_time = parse_timestamp(&session.started_at)?;
            let time_delta_seconds = (session_time - video_time).num_seconds().abs();
            if time_delta_seconds > window_seconds {
                return None;
            }
            let normalized_shop_name = normalize_match_text(&session.shop_name);
            Some(LiveDashboardMatchCandidate {
                session: session.clone(),
                time_delta_seconds,
                account_match: false,
                shop_name_match: explicit_shop.is_some()
                    || (!normalized_shop_name.is_empty()
                        && identity.contains(&normalized_shop_name)),
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.time_delta_seconds);
    candidates
}

pub fn match_imported_video_sessions(
    title: &str,
    file: &str,
    note: &str,
    sessions: &[LiveDashboardSessionRow],
) -> LiveDashboardMatchResult {
    let candidates =
        imported_video_candidate_sessions(title, file, note, sessions, AUTO_MATCH_WINDOW_SECS);
    let shop_matches = candidates
        .iter()
        .filter(|candidate| candidate.shop_name_match)
        .collect::<Vec<_>>();
    let automatic = if shop_matches.len() == 1 {
        Some((
            shop_matches[0].session.clone(),
            LiveDashboardMatchMethod::AutoShop,
        ))
    } else if shop_matches.is_empty() && candidates.len() == 1 {
        Some((
            candidates[0].session.clone(),
            LiveDashboardMatchMethod::AutoFilename,
        ))
    } else {
        None
    };
    LiveDashboardMatchResult {
        automatic,
        candidates,
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

    let mut candidates = candidate_sessions(record, sessions, AUTO_MATCH_WINDOW_SECS);
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

    if let Some(automatic) = unique_duration_match(
        parse_timestamp(&record.created_at),
        record.length_secs,
        &record.title,
        &record.room_id,
        sessions,
    ) {
        let mut ranked = rank_sessions_by_clock_and_duration(
            parse_timestamp(&record.created_at),
            record.length_secs,
            &record.title,
            &record.room_id,
            sessions,
        );
        if ranked.is_empty() {
            ranked = candidates;
        }
        return LiveDashboardMatchResult {
            automatic: Some(automatic),
            candidates: ranked,
        };
    }

    if candidates.is_empty() {
        candidates = rank_sessions_by_clock_and_duration(
            parse_timestamp(&record.created_at),
            record.length_secs,
            &record.title,
            &record.room_id,
            sessions,
        );
    }

    LiveDashboardMatchResult {
        automatic: None,
        candidates,
    }
}

pub fn match_imported_video_by_duration(
    title: &str,
    file: &str,
    note: &str,
    created_at: &str,
    length_secs: Option<i64>,
    sessions: &[LiveDashboardSessionRow],
) -> LiveDashboardMatchResult {
    let identity = [title, file, note].join(" ");
    let clock = infer_live_started_at_from_texts(&[title, file, note])
        .as_deref()
        .and_then(parse_timestamp)
        .or_else(|| parse_timestamp(created_at));
    if let Some(automatic) = unique_duration_match(clock, length_secs, &identity, "", sessions) {
        return LiveDashboardMatchResult {
            automatic: Some(automatic),
            candidates: rank_sessions_by_clock_and_duration(
                clock,
                length_secs,
                &identity,
                "",
                sessions,
            ),
        };
    }
    LiveDashboardMatchResult {
        automatic: None,
        candidates: rank_sessions_by_clock_and_duration(
            clock,
            length_secs,
            &identity,
            "",
            sessions,
        ),
    }
}

fn session_span_secs(session: &LiveDashboardSessionRow) -> Option<i64> {
    let start = parse_timestamp(&session.started_at)?;
    let end = parse_timestamp(&session.ended_at)?;
    let secs = (end - start).num_seconds();
    (secs > 60).then_some(secs)
}

fn unique_duration_match(
    clock: Option<DateTime<Utc>>,
    length_secs: Option<i64>,
    identity: &str,
    room_id: &str,
    sessions: &[LiveDashboardSessionRow],
) -> Option<(LiveDashboardSessionRow, LiveDashboardMatchMethod)> {
    let video_len = length_secs.filter(|value| *value >= 30 * 60)?;
    let explicit_shop = infer_shop_name_from_texts(&[identity]);
    let room_id = room_id.trim();
    let mut hits = sessions
        .iter()
        .filter(|session| {
            if explicit_shop.is_some_and(|shop| !session_matches_shop(session, shop)) {
                return false;
            }
            session_span_secs(session)
                .is_some_and(|span| (span - video_len).abs() <= DURATION_AUTO_MATCH_SECS)
        })
        .cloned()
        .collect::<Vec<_>>();
    if let Some(clock) = clock {
        let same_day = hits
            .iter()
            .filter(|session| {
                parse_timestamp(&session.started_at)
                    .is_some_and(|started| started.date_naive() == clock.date_naive())
            })
            .cloned()
            .collect::<Vec<_>>();
        if same_day.len() == 1 {
            let method = if !room_id.is_empty() && room_id == same_day[0].account_key.trim() {
                LiveDashboardMatchMethod::AutoAccount
            } else {
                LiveDashboardMatchMethod::AutoShop
            };
            return Some((same_day[0].clone(), method));
        }
        if !same_day.is_empty() {
            hits = same_day;
        }
    }
    if hits.len() == 1 {
        let method = if !room_id.is_empty() && room_id == hits[0].account_key.trim() {
            LiveDashboardMatchMethod::AutoAccount
        } else {
            LiveDashboardMatchMethod::AutoShop
        };
        return Some((hits[0].clone(), method));
    }
    None
}

fn rank_sessions_by_clock_and_duration(
    clock: Option<DateTime<Utc>>,
    length_secs: Option<i64>,
    identity: &str,
    room_id: &str,
    sessions: &[LiveDashboardSessionRow],
) -> Vec<LiveDashboardMatchCandidate> {
    let explicit_shop = infer_shop_name_from_texts(&[identity]);
    let normalized_identity = normalize_match_text(identity);
    let room_id = room_id.trim();
    let video_len = length_secs.filter(|value| *value >= 30 * 60);
    let mut candidates = sessions
        .iter()
        .filter_map(|session| {
            if explicit_shop.is_some_and(|shop| !session_matches_shop(session, shop)) {
                return None;
            }
            let started = parse_timestamp(&session.started_at)?;
            let time_delta_seconds = clock
                .map(|value| (started - value).num_seconds().abs())
                .unwrap_or(i64::MAX / 4);
            let duration_delta = video_len.and_then(|length| {
                session_span_secs(session).map(|span| (span - length).abs())
            });
            let same_day = clock.is_some_and(|value| started.date_naive() == value.date_naive());
            let duration_close =
                duration_delta.is_some_and(|delta| delta <= DURATION_CANDIDATE_SECS);
            if !same_day && !duration_close && time_delta_seconds > CANDIDATE_WINDOW_SECS {
                return None;
            }
            let normalized_shop_name = normalize_match_text(&session.shop_name);
            Some(LiveDashboardMatchCandidate {
                session: session.clone(),
                time_delta_seconds,
                account_match: !room_id.is_empty() && room_id == session.account_key.trim(),
                shop_name_match: explicit_shop.is_some()
                    || (!normalized_shop_name.is_empty()
                        && normalized_identity.contains(&normalized_shop_name)),
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_duration = video_len
            .and_then(|length| session_span_secs(&left.session).map(|span| (span - length).abs()))
            .unwrap_or(i64::MAX / 4);
        let right_duration = video_len
            .and_then(|length| session_span_secs(&right.session).map(|span| (span - length).abs()))
            .unwrap_or(i64::MAX / 4);
        left_duration
            .cmp(&right_duration)
            .then(left.time_delta_seconds.cmp(&right.time_delta_seconds))
    });
    candidates.truncate(12);
    candidates
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
            ended_at: "2026-07-28T15:44:39".into(),
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
            length_secs: None,
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
            length_secs: None,
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

    #[test]
    fn infers_live_time_from_windows_safe_video_names() {
        assert_eq!(
            infer_live_started_at_from_texts(&["金典拍拍科创专卖店_2026-08-01_08-57-00.ts"]),
            Some("2026-08-01T08:57:00".into())
        );
        assert_eq!(
            infer_live_started_at_from_texts(&["直播_20260801_085700.mp4"]),
            Some("2026-08-01T08:57:00".into())
        );
    }

    #[test]
    fn imported_video_uses_shop_and_filename_time_for_a_unique_match() {
        let camera = session(1, "camera", "2026-07-28T08:15:49");
        let mut innovation = session(2, "innovation", "2026-07-28T08:15:52");
        innovation.shop_name = "金典拍拍科创专卖店".into();

        let result = match_imported_video_sessions(
            "金典拍拍科创专卖店_2026-07-28_08-15-55",
            "imported.ts",
            "{}",
            &[camera, innovation],
        );

        assert_eq!(
            result.automatic.as_ref().map(|(session, _)| session.id),
            Some(2)
        );
        assert_eq!(
            result.automatic.map(|(_, method)| method),
            Some(LiveDashboardMatchMethod::AutoShop)
        );
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].session.shop_name, "金典拍拍科创专卖店");
    }

    #[test]
    fn explicit_shop_name_excludes_the_other_shop_from_manual_candidates() {
        let camera = session(1, "camera", "2026-08-01T08:57:02");
        let mut innovation = session(2, "innovation", "2026-08-01T08:58:00");
        innovation.shop_name = "金典拍拍科创专卖店".into();

        let candidates = imported_video_candidate_sessions(
            "金典拍拍科创专卖店_2026-08-01_08-57-01",
            "download.ts",
            "",
            &[camera, innovation],
            CANDIDATE_WINDOW_SECS,
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].session.id, 2);
    }

    #[test]
    fn imported_video_does_not_guess_when_two_time_matches_have_no_shop_hint() {
        let result = match_imported_video_sessions(
            "直播_2026-07-28_08-15-55",
            "imported.ts",
            "{}",
            &[
                session(1, "camera", "2026-07-28T08:15:49"),
                session(2, "camera-2", "2026-07-28T08:16:10"),
            ],
        );

        assert!(result.automatic.is_none());
        assert_eq!(result.candidates.len(), 2);
    }

    #[test]
    fn matches_old_video_to_excel_by_same_day_and_duration() {
        let mut same_day = session(1, "20296833869", "2026-08-12T10:00:00");
        same_day.ended_at = "2026-08-12T15:14:10".into();
        let mut other_day = session(2, "20296833869", "2026-07-01T10:00:00");
        other_day.ended_at = "2026-07-01T15:14:00".into();

        let record = RecordForLiveDashboardBinding {
            platform: "douyin".into(),
            room_id: "20296833869".into(),
            title: "金典拍拍相机专卖店".into(),
            created_at: "2026-08-12T10:02:00".into(),
            length_secs: Some(5 * 3600 + 14 * 60 + 10),
        };

        let result = match_dashboard_sessions(&record, &[same_day, other_day]);
        assert_eq!(
            result.automatic.as_ref().map(|(session, _)| session.id),
            Some(1)
        );
    }

    #[test]
    fn imported_video_finds_excel_by_duration_without_filename_clock() {
        let mut hit = session(1, "camera", "2026-08-12T09:00:00");
        hit.ended_at = "2026-08-12T14:14:00".into();
        let mut miss = session(2, "camera", "2026-08-12T09:05:00");
        miss.ended_at = "2026-08-12T10:00:00".into();

        let result = match_imported_video_by_duration(
            "金典拍拍相机专卖店",
            "download.mp4",
            "",
            "2026-08-12T09:01:00",
            Some(5 * 3600 + 14 * 60),
            &[hit, miss],
        );
        assert_eq!(
            result.automatic.as_ref().map(|(session, _)| session.id),
            Some(1)
        );
    }
}
