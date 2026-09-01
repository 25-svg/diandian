use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::database::live_dashboard::{LiveDashboardProductRow, LiveDashboardSessionRow};

const CN_OFFSET: FixedOffset = FixedOffset::east_opt(8 * 3600).expect("valid CN offset");

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawPaymentEvent {
    pub offset_sec: i64,
    pub pay_amount_fen: Option<i64>,
    pub product_name: String,
    pub product_id: String,
    pub order_id: String,
    pub order_status: String,
    /// Masked receiver name from Douyin export (e.g. "张*"). Safe for streamer review.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub buyer_label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawOrderMatchSummary {
    pub shop_name: String,
    pub room_id: String,
    pub parent_order_count: usize,
    pub event_count: usize,
    pub total_pay_amount_fen: i64,
    pub expected_event_count: Option<i64>,
    pub expected_pay_amount_fen: i64,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawOrderTimelineImportResult {
    pub summary: RawOrderMatchSummary,
    pub events: Vec<RawPaymentEvent>,
    pub source_label: String,
    pub raw_order_count: usize,
    pub candidate_count: usize,
}

#[derive(Debug)]
struct Candidate {
    shop_name: String,
    room_id: String,
    parent_order_ids: HashSet<String>,
    events: Vec<RawPaymentEvent>,
}

#[derive(Debug, Clone)]
struct ProductAttributionRule {
    product_id: String,
    normalized_name: String,
    expected_count: Option<usize>,
    expected_amount_fen: Option<i64>,
}

impl Candidate {
    fn total_pay_amount_fen(&self) -> i64 {
        self.events
            .iter()
            .filter_map(|event| event.pay_amount_fen)
            .sum()
    }
}

pub fn import_raw_order_timeline(
    path: &Path,
    session: &LiveDashboardSessionRow,
    products: &[LiveDashboardProductRow],
) -> Result<RawOrderTimelineImportResult, String> {
    let bytes = fs::read(path).map_err(|error| format!("无法读取订单 JSON：{error}"))?;
    let payload: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("订单 JSON 格式无效：{error}"))?;
    import_raw_order_timeline_value(
        &payload,
        session,
        products,
        path.file_name().and_then(|v| v.to_str()),
    )
}

pub fn import_raw_order_timeline_value(
    payload: &Value,
    session: &LiveDashboardSessionRow,
    products: &[LiveDashboardProductRow],
    source_name: Option<&str>,
) -> Result<RawOrderTimelineImportResult, String> {
    import_raw_order_timeline_value_for_room(payload, session, products, source_name, None)
}

fn import_raw_order_timeline_value_for_room(
    payload: &Value,
    session: &LiveDashboardSessionRow,
    products: &[LiveDashboardProductRow],
    source_name: Option<&str>,
    preferred_room_id: Option<&str>,
) -> Result<RawOrderTimelineImportResult, String> {
    let started_at = parse_session_time(&session.started_at)?;
    let ended_at = parse_session_time(&session.ended_at).map_err(|_| {
        "当前直播场次缺少结束时间。请重新导入对应的官方整场 Excel 后再导入订单".to_string()
    })?;
    if ended_at <= started_at {
        return Err("直播结束时间必须晚于开始时间".to_string());
    }

    let product_names = products
        .iter()
        .map(|product| {
            (
                product.product_id.trim().to_string(),
                product.name.trim().to_string(),
            )
        })
        .filter(|(id, _)| !id.is_empty())
        .collect::<HashMap<_, _>>();
    let normalized_product_names = products
        .iter()
        .map(|product| normalize_product_name(&product.name))
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();
    let mut candidates: HashMap<(String, String), Candidate> = HashMap::new();
    let mut raw_order_count = 0usize;
    let mut observed_shop_names = HashSet::new();

    for (shop_name, order) in iter_orders(payload) {
        raw_order_count += 1;
        if !shop_name.trim().is_empty() {
            observed_shop_names.insert(shop_name.clone());
        }
        let Some(pay_time) = positive_i64(order.get("pay_time")) else {
            continue;
        };
        if pay_time < started_at.timestamp() || pay_time > ended_at.timestamp() {
            continue;
        }

        let order_status = text(order.get("order_status_desc"))
            .or_else_non_empty(|| text(order.get("main_status_desc")));
        let order_id = text(order.get("order_id"));
        let sku_list = order
            .get("sku_order_list")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (sku_index, sku) in sku_list.iter().enumerate() {
            let room_id = text(sku.get("room_id_str"))
                .or_else_non_empty(|| text(sku.get("room_id")))
                .or_else_non_empty(|| text(order.get("room_id_str")))
                .or_else_non_empty(|| text(order.get("room_id")));
            let room_id = if room_id.is_empty() {
                "0".to_string()
            } else {
                room_id
            };
            let sku_pay_time = positive_i64(sku.get("pay_time")).unwrap_or(pay_time);
            if sku_pay_time < started_at.timestamp() || sku_pay_time > ended_at.timestamp() {
                continue;
            }
            let product_id =
                text(sku.get("product_id_str")).or_else_non_empty(|| text(sku.get("product_id")));
            let product_name = text(sku.get("product_name"))
                .or_else_non_empty(|| product_names.get(&product_id).cloned().unwrap_or_default());
            let product_id_match =
                !product_id.is_empty() && product_names.contains_key(&product_id);
            let normalized_order_product = normalize_product_name(&product_name);
            let product_name_match = !normalized_order_product.is_empty()
                && normalized_product_names.iter().any(|excel_name| {
                    excel_name == &normalized_order_product
                        || (excel_name.len() >= 8
                            && normalized_order_product.len() >= 8
                            && (excel_name.contains(&normalized_order_product)
                                || normalized_order_product.contains(excel_name)))
                });
            if !product_id_match && !product_name_match {
                continue;
            }
            let pay_amount_fen = payment_amount(sku).or_else(|| {
                (sku_list.len() == 1)
                    .then(|| payment_amount(order))
                    .flatten()
            });
            let event_order_id = if order_id.is_empty() {
                format!("anonymous-{sku_index}")
            } else {
                order_id.clone()
            };
            let key = (shop_name.clone(), room_id.clone());
            let candidate = candidates.entry(key).or_insert_with(|| Candidate {
                shop_name: shop_name.clone(),
                room_id: room_id.clone(),
                parent_order_ids: HashSet::new(),
                events: Vec::new(),
            });
            if !order_id.is_empty() {
                candidate.parent_order_ids.insert(order_id.clone());
            }
            let buyer_label = text(order.get("mask_post_receiver"))
                .or_else_non_empty(|| text(order.get("user_nick_name")));
            candidate.events.push(RawPaymentEvent {
                offset_sec: sku_pay_time - started_at.timestamp(),
                pay_amount_fen,
                product_name,
                product_id,
                order_id: event_order_id,
                order_status: order_status.clone(),
                buyer_label,
            });
        }
    }

    let matching_shop_observed = observed_shop_names
        .iter()
        .any(|shop_name| crate::live_dashboard_binding::session_matches_shop(session, shop_name));
    if !observed_shop_names.is_empty() && !matching_shop_observed {
        let mut observed = observed_shop_names.into_iter().collect::<Vec<_>>();
        observed.sort();
        return Err(format!(
            "店铺不匹配：Excel 是“{}”，订单凭证返回“{}”。请换绑正确抖店账号",
            session.shop_name,
            observed.join("、")
        ));
    }
    if candidates.is_empty() {
        return Err("订单文件中没有落在本场直播时间内、且带直播房间号的支付订单".to_string());
    }

    let expected_count = session.deal_item_count;
    let expected_amount = session.payment_amount_fen;
    let candidate_count = candidates.len();
    // Douyin occasionally returns room_id=0 for part of the same live room.
    // Merge that unknown bucket only when the shop has one unambiguous real
    // room in this Excel time window. This keeps valid orders without guessing
    // between two simultaneous rooms.
    let unknown_room_keys = candidates
        .keys()
        .filter(|(_, room_id)| room_id == "0")
        .cloned()
        .collect::<Vec<_>>();
    for unknown_key in unknown_room_keys {
        let target_keys = candidates
            .keys()
            .filter(|(shop_name, room_id)| {
                shop_name == &unknown_key.0
                    && room_id != "0"
                    && crate::live_dashboard_binding::session_matches_shop(session, shop_name)
            })
            .cloned()
            .collect::<Vec<_>>();
        if target_keys.len() == 1 {
            if let Some(unknown) = candidates.remove(&unknown_key) {
                if let Some(target) = candidates.get_mut(&target_keys[0]) {
                    target.parent_order_ids.extend(unknown.parent_order_ids);
                    target.events.extend(unknown.events);
                }
            }
        }
    }
    let mut candidate_values = candidates.into_values().collect::<Vec<_>>();
    let same_shop_indices = candidate_values
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            crate::live_dashboard_binding::session_matches_shop(session, &candidate.shop_name)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if same_shop_indices.is_empty() {
        return Err(format!(
            "原始订单中没有找到店铺“{}”在本场时段内的 Excel 商品订单",
            session.shop_name
        ));
    }
    let preferred_room_id = preferred_room_id.unwrap_or_default().trim();
    let preferred_indices = same_shop_indices
        .iter()
        .copied()
        .filter(|index| {
            !preferred_room_id.is_empty()
                && candidate_values[*index].room_id.trim() == preferred_room_id
        })
        .collect::<Vec<_>>();
    if preferred_indices.len() > 1 {
        return Err("当前直播房间号对应到多个订单候选，无法唯一确定本场".to_string());
    }

    let mut ranked_indices = if preferred_indices.len() == 1 {
        preferred_indices
    } else {
        same_shop_indices
    };
    let candidate_score = |index: usize| {
        let attributed =
            select_product_attributed_events(candidate_values[index].events.clone(), products);
        let event_count = attributed.len();
        let total_amount = attributed
            .iter()
            .filter_map(|event| event.pay_amount_fen)
            .sum::<i64>();
        let count_delta = expected_count
            .and_then(|count| usize::try_from(count).ok())
            .map(|expected| event_count.abs_diff(expected))
            .unwrap_or(0);
        let amount_delta = if expected_amount > 0 {
            total_amount.abs_diff(expected_amount)
        } else {
            0
        };
        (count_delta, amount_delta, std::cmp::Reverse(event_count))
    };
    ranked_indices.sort_by_key(|index| candidate_score(*index));
    let matched_index = ranked_indices[0];
    if ranked_indices
        .get(1)
        .is_some_and(|second| candidate_score(*second) == candidate_score(matched_index))
    {
        return Err(
            "同一店铺存在多个直播房间，且房间号、成交件数和金额均无法唯一确定本场".to_string(),
        );
    }

    let mut matched = candidate_values.swap_remove(matched_index);
    matched.events = select_product_attributed_events(matched.events, products);
    if matched.events.is_empty() {
        return Err("订单文件中没有可归因到 Excel 商品的支付订单".to_string());
    }
    matched.events.sort_by_key(|event| event.offset_sec);
    let total_pay_amount_fen = matched.total_pay_amount_fen();
    let parent_order_count = matched
        .events
        .iter()
        .map(|event| event.order_id.as_str())
        .filter(|order_id| !order_id.is_empty() && !order_id.starts_with("anonymous-"))
        .collect::<HashSet<_>>()
        .len();
    let event_count = matched.events.len();
    let summary = RawOrderMatchSummary {
        shop_name: matched.shop_name,
        room_id: matched.room_id,
        parent_order_count,
        event_count,
        total_pay_amount_fen,
        expected_event_count: expected_count,
        expected_pay_amount_fen: expected_amount,
        confidence: "shop_time_and_excel_products".to_string(),
    };
    Ok(RawOrderTimelineImportResult {
        summary,
        events: matched.events,
        source_label: source_name.unwrap_or("order.searchList").to_string(),
        raw_order_count,
        candidate_count,
    })
}

/// Auto-pull: keep all paid SKUs in the live window, require the token shop to
/// match the bound Excel shop, then attribute by Excel product id/name.  The
/// Excel per-product sold count limits duplicate candidates; per-product GMV
/// only breaks ties.  We never force the whole session to equal one total.
pub fn match_live_window_orders_to_session(
    payload: &Value,
    session: &LiveDashboardSessionRow,
    products: &[LiveDashboardProductRow],
    source_name: Option<&str>,
) -> Result<RawOrderTimelineImportResult, String> {
    match_live_window_orders_to_session_for_room(payload, session, products, source_name, None)
}

pub fn match_live_window_orders_to_session_for_room(
    payload: &Value,
    session: &LiveDashboardSessionRow,
    products: &[LiveDashboardProductRow],
    source_name: Option<&str>,
    preferred_room_id: Option<&str>,
) -> Result<RawOrderTimelineImportResult, String> {
    import_raw_order_timeline_value_for_room(
        payload,
        session,
        products,
        source_name,
        preferred_room_id,
    )
}

fn build_product_attribution_rules(
    products: &[LiveDashboardProductRow],
) -> Vec<ProductAttributionRule> {
    products
        .iter()
        .filter_map(|product| {
            let product_id = product.product_id.trim().to_string();
            let normalized_name = normalize_product_name(&product.name);
            if product_id.is_empty() && normalized_name.is_empty() {
                return None;
            }
            Some(ProductAttributionRule {
                product_id,
                normalized_name,
                expected_count: product
                    .sold_count
                    .and_then(|count| usize::try_from(count).ok())
                    .filter(|count| *count > 0),
                expected_amount_fen: product.payment_amount_fen.filter(|amount| *amount > 0),
            })
        })
        .collect()
}

fn product_rule_index(event: &RawPaymentEvent, rules: &[ProductAttributionRule]) -> Option<usize> {
    if !event.product_id.trim().is_empty() {
        if let Some(index) = rules.iter().position(|rule| {
            !rule.product_id.is_empty() && rule.product_id == event.product_id.trim()
        }) {
            return Some(index);
        }
    }
    let normalized_event = normalize_product_name(&event.product_name);
    if normalized_event.is_empty() {
        return None;
    }
    rules.iter().position(|rule| {
        let excel_name = &rule.normalized_name;
        !excel_name.is_empty()
            && (excel_name == &normalized_event
                || (excel_name.len() >= 8
                    && normalized_event.len() >= 8
                    && (excel_name.contains(&normalized_event)
                        || normalized_event.contains(excel_name))))
    })
}

fn select_product_attributed_events(
    events: Vec<RawPaymentEvent>,
    products: &[LiveDashboardProductRow],
) -> Vec<RawPaymentEvent> {
    let rules = build_product_attribution_rules(products);
    if rules.is_empty() {
        return Vec::new();
    }
    let mut grouped = vec![Vec::<RawPaymentEvent>::new(); rules.len()];
    for event in events {
        if let Some(index) = product_rule_index(&event, &rules) {
            grouped[index].push(event);
        }
    }

    let mut attributed = Vec::new();
    for (rule, mut product_events) in rules.iter().zip(grouped) {
        product_events.sort_by_key(|event| event.offset_sec);
        if let Some(expected_count) = rule.expected_count {
            while product_events.len() > expected_count {
                let total = product_events
                    .iter()
                    .filter_map(|event| event.pay_amount_fen)
                    .sum::<i64>();
                let target = rule.expected_amount_fen.unwrap_or(total);
                let remove_index = product_events
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, event)| {
                        let next_total = total - event.pay_amount_fen.unwrap_or(0);
                        (next_total - target).abs()
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(product_events.len() - 1);
                product_events.remove(remove_index);
            }
        }
        attributed.extend(product_events);
    }
    attributed.sort_by_key(|event| event.offset_sec);
    attributed
}

fn iter_orders(payload: &Value) -> Vec<(String, &Value)> {
    let mut output = Vec::new();
    if let Some(nested) = payload.get("orders_payload") {
        output.extend(iter_orders(nested));
    }
    if let Some(orders) = payload.get("shop_order_list").and_then(Value::as_array) {
        let shop_name = text(payload.get("shop_name"));
        output.extend(orders.iter().map(|order| (shop_name.clone(), order)));
    }
    if let Some(shops) = payload.get("shops").and_then(Value::as_array) {
        for shop in shops {
            let shop_name = text(shop.get("shop_name"));
            if let Some(orders) = shop.get("orders").and_then(Value::as_array) {
                output.extend(orders.iter().map(|order| (shop_name.clone(), order)));
            }
        }
    }
    output
}

fn parse_session_time(value: &str) -> Result<DateTime<FixedOffset>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("时间为空".to_string());
    }
    if let Ok(value) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(value.with_timezone(&CN_OFFSET));
    }
    for format in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
    ] {
        if let Ok(value) = NaiveDateTime::parse_from_str(trimmed, format) {
            return CN_OFFSET
                .from_local_datetime(&value)
                .single()
                .ok_or_else(|| format!("无法解析时间：{trimmed}"));
        }
    }
    Err(format!("无法解析时间：{trimmed}"))
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(|value| match value {
            Value::String(value) => Some(value.trim().to_string()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

trait NonEmptyString {
    fn or_else_non_empty(self, fallback: impl FnOnce() -> String) -> String;
}

impl NonEmptyString for String {
    fn or_else_non_empty(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

fn positive_i64(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        })
        .filter(|value| *value > 0)
}

fn payment_amount(value: &Value) -> Option<i64> {
    positive_i64(value.get("pay_amount"))
        .or_else(|| positive_i64(value.get("actual_receive_amount")))
        .or_else(|| {
            value
                .get("actual_receive_amount_info")
                .and_then(|info| positive_i64(info.get("actual_receive_amount")))
        })
}

fn normalize_product_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> LiveDashboardSessionRow {
        LiveDashboardSessionRow {
            id: 1,
            account_key: "whjdxjh".into(),
            shop_name: "金典拍拍相机专卖店微单相机专场".into(),
            started_at: "2026-07-29T08:37:34".into(),
            ended_at: "2026-07-29T17:26:30".into(),
            payment_amount_fen: 14_239_200,
            per_thousand_payment_amount_fen: None,
            viewer_count: None,
            average_online: None,
            average_watch_seconds: None,
            viewer_conversion_rate: None,
            deal_buyer_count: Some(15),
            deal_item_count: Some(2),
            product_click_conversion_rate: None,
            exposure_viewer_rate: None,
            qianchuan_spend_fen: None,
            source_file: "session.xlsx".into(),
            imported_at: "2026-08-05T00:00:00Z".into(),
        }
    }

    #[test]
    fn matches_room_by_exact_excel_amount_and_sku_count_without_exposing_buyer_fields() {
        let payload = serde_json::json!({
            "shops": [{
                "shop_name": "金典拍拍相机专卖店",
                "orders": [{
                    "order_id": "parent-1",
                    "pay_time": 1785292658_i64,
                    "order_status_desc": "已发货",
                    "pay_tel": "13800000000",
                    "post_addr": {"detail": "private"},
                    "mask_post_receiver": "张*",
                    "sku_order_list": [
                        {"room_id": "matched-room", "pay_time": 1785292658_i64, "pay_amount": 9_000_000, "product_id": "p1"},
                        {"room_id": "matched-room", "pay_time": 1785292718_i64, "pay_amount": 5_239_200, "product_id": "p2"},
                        {"room_id": "other-room", "pay_time": 1785292718_i64, "pay_amount": 1_000, "product_id": "p3"}
                    ]
                }]
            }]
        });
        let products = [("p1", "索尼 A7M4"), ("p2", "佳能 RF70-200")]
            .into_iter()
            .enumerate()
            .map(|(index, (product_id, name))| LiveDashboardProductRow {
                id: index as i64 + 1,
                session_id: 1,
                product_id: product_id.into(),
                name: name.into(),
                payment_amount_fen: None,
                sold_count: None,
                buyer_count: None,
                exposure_count: None,
                click_count: None,
            })
            .collect::<Vec<_>>();

        let result =
            import_raw_order_timeline_value(&payload, &session(), &products, Some("raw.json"))
                .expect("exact match");

        assert_eq!(result.summary.room_id, "matched-room");
        assert_eq!(result.summary.parent_order_count, 1);
        assert_eq!(result.summary.event_count, 2);
        assert_eq!(result.summary.total_pay_amount_fen, 14_239_200);
        assert_eq!(result.summary.confidence, "shop_time_and_excel_products");
        assert_eq!(result.events[0].product_name, "索尼 A7M4");
        assert_eq!(result.events[0].buyer_label, "张*");
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("13800000000"));
        assert!(!serialized.contains("private"));
        assert!(serialized.contains("张*"));
        assert!(serialized.contains("buyerLabel"));
    }

    #[test]
    fn drops_same_window_skus_that_are_not_on_the_excel_leaderboard() {
        let payload = serde_json::json!({
            "shops": [{
                "shop_name": "金典拍拍相机专卖店",
                "orders": [{
                    "order_id": "parent-1",
                    "pay_time": 1785292658_i64,
                    "order_status_desc": "已发货",
                    "sku_order_list": [
                        {"room_id_str": "matched-room", "pay_time": 1785292658_i64, "pay_amount": 9_000_000, "product_id": "p1"},
                        {"room_id_str": "matched-room", "pay_time": 1785292718_i64, "pay_amount": 5_239_200, "product_id": "p2"},
                        {"room_id_str": "matched-room", "pay_time": 1785292800_i64, "pay_amount": 1_222_900, "product_id": "p-extra"}
                    ]
                }]
            }]
        });
        let products = [("p1", "索尼 A7M4"), ("p2", "佳能 RF70-200")]
            .into_iter()
            .enumerate()
            .map(|(index, (product_id, name))| LiveDashboardProductRow {
                id: index as i64 + 1,
                session_id: 1,
                product_id: product_id.into(),
                name: name.into(),
                payment_amount_fen: None,
                sold_count: None,
                buyer_count: None,
                exposure_count: None,
                click_count: None,
            })
            .collect::<Vec<_>>();

        let result =
            import_raw_order_timeline_value(&payload, &session(), &products, Some("raw.json"))
                .expect("excel products only");
        assert_eq!(result.summary.event_count, 2);
        assert_eq!(result.summary.total_pay_amount_fen, 14_239_200);
        assert!(result
            .events
            .iter()
            .all(|event| event.product_id != "p-extra"));
    }

    #[test]
    fn auto_pull_attributes_excel_products_and_keeps_later_closed_orders() {
        let mut live = session();
        live.payment_amount_fen = 7_488_000;
        live.deal_item_count = Some(2);
        let payload = serde_json::json!({
            "shops": [{
                "shop_name": "金典拍拍相机专卖店",
                "orders": [
                    {
                        "order_id": "live-1",
                        "pay_time": 1785292658_i64,
                        "order_status_desc": "已关闭",
                        "sku_order_list": [{
                            "room_id": "live-room",
                            "pay_time": 1785292658_i64,
                            "pay_amount": 4_000_000,
                            "product_id": "p1"
                        }]
                    },
                    {
                        "order_id": "live-2",
                        "pay_time": 1785292718_i64,
                        "order_status_desc": "已发货",
                        "sku_order_list": [{
                            "room_id": "0",
                            "pay_time": 1785292718_i64,
                            "pay_amount": 3_488_000,
                            "product_id": "p2"
                        }]
                    },
                    {
                        "order_id": "extra-1",
                        "pay_time": 1785292800_i64,
                        "order_status_desc": "已发货",
                        "sku_order_list": [{
                            "room_id": "other-room",
                            "pay_time": 1785292800_i64,
                            "pay_amount": 1_222_900,
                            "product_id": "p-extra"
                        }]
                    }
                ]
            }]
        });
        let products = [
            ("p1", "商品一", 4_000_000, 1),
            ("p2", "商品二", 3_488_000, 1),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(index, (product_id, name, amount, sold_count))| LiveDashboardProductRow {
                id: index as i64 + 1,
                session_id: 1,
                product_id: product_id.into(),
                name: name.into(),
                payment_amount_fen: Some(amount),
                sold_count: Some(sold_count),
                buyer_count: None,
                exposure_count: None,
                click_count: None,
            },
        )
        .collect::<Vec<_>>();
        let result = match_live_window_orders_to_session(
            &payload,
            &live,
            &products,
            Some("order.searchList"),
        )
        .expect("attribute products");
        assert_eq!(result.summary.event_count, 2);
        assert_eq!(result.summary.total_pay_amount_fen, 7_488_000);
        assert_eq!(result.summary.confidence, "shop_time_and_excel_products");
        assert!(result
            .events
            .iter()
            .any(|event| event.order_status == "已关闭"));
        assert!(result
            .events
            .iter()
            .all(|event| event.order_id != "extra-1"));
    }

    #[test]
    fn auto_pull_rejects_token_shop_that_differs_from_excel() {
        let live = session();
        let payload = serde_json::json!({
            "shops": [{
                "shop_name": "金典拍拍科创专卖店",
                "orders": [{
                    "order_id": "wrong-shop",
                    "pay_time": 1785292658_i64,
                    "order_status_desc": "已完成",
                    "sku_order_list": [{
                        "pay_time": 1785292658_i64,
                        "pay_amount": 1_000,
                        "product_id": "p1"
                    }]
                }]
            }]
        });
        let products = vec![LiveDashboardProductRow {
            id: 1,
            session_id: 1,
            product_id: "p1".into(),
            name: "商品一".into(),
            payment_amount_fen: Some(1_000),
            sold_count: Some(1),
            buyer_count: None,
            exposure_count: None,
            click_count: None,
        }];
        let error = match_live_window_orders_to_session(
            &payload,
            &live,
            &products,
            Some("order.searchList"),
        )
        .expect_err("wrong shop must stop");
        assert!(error.contains("店铺不匹配"));
        assert!(error.contains("金典拍拍科创专卖店"));
    }

    #[test]
    fn current_room_id_breaks_same_shop_candidate_tie() {
        let mut live = session();
        live.payment_amount_fen = 1_000;
        live.deal_item_count = Some(1);
        let payload = serde_json::json!({
            "shops": [{
                "shop_name": "金典拍拍相机专卖店",
                "orders": [
                    {
                        "order_id": "other-order",
                        "pay_time": 1785292658_i64,
                        "sku_order_list": [{
                            "room_id": "other-room",
                            "pay_time": 1785292658_i64,
                            "pay_amount": 1_000,
                            "product_id": "p1"
                        }]
                    },
                    {
                        "order_id": "current-order",
                        "pay_time": 1785292718_i64,
                        "sku_order_list": [{
                            "room_id": "current-room",
                            "pay_time": 1785292718_i64,
                            "pay_amount": 1_000,
                            "product_id": "p1"
                        }]
                    }
                ]
            }]
        });
        let products = vec![LiveDashboardProductRow {
            id: 1,
            session_id: 1,
            product_id: "p1".into(),
            name: "商品一".into(),
            payment_amount_fen: Some(1_000),
            sold_count: Some(1),
            buyer_count: None,
            exposure_count: None,
            click_count: None,
        }];

        let result = match_live_window_orders_to_session_for_room(
            &payload,
            &live,
            &products,
            Some("order.searchList"),
            Some("current-room"),
        )
        .expect("current room should resolve the tie");

        assert_eq!(result.summary.room_id, "current-room");
        assert_eq!(result.events[0].order_id, "current-order");
    }

    #[test]
    fn excel_amount_breaks_same_count_candidate_tie_without_room_match() {
        let mut live = session();
        live.payment_amount_fen = 9_900;
        live.deal_item_count = Some(1);
        let payload = serde_json::json!({
            "shops": [{
                "shop_name": "金典拍拍相机专卖店",
                "orders": [
                    {
                        "order_id": "far-amount",
                        "pay_time": 1785292658_i64,
                        "sku_order_list": [{
                            "room_id": "room-a",
                            "pay_time": 1785292658_i64,
                            "pay_amount": 1_000,
                            "product_id": "p1"
                        }]
                    },
                    {
                        "order_id": "close-amount",
                        "pay_time": 1785292718_i64,
                        "sku_order_list": [{
                            "room_id": "room-b",
                            "pay_time": 1785292718_i64,
                            "pay_amount": 9_900,
                            "product_id": "p1"
                        }]
                    }
                ]
            }]
        });
        let products = vec![LiveDashboardProductRow {
            id: 1,
            session_id: 1,
            product_id: "p1".into(),
            name: "商品一".into(),
            payment_amount_fen: Some(9_900),
            sold_count: Some(1),
            buyer_count: None,
            exposure_count: None,
            click_count: None,
        }];

        let result = match_live_window_orders_to_session(
            &payload,
            &live,
            &products,
            Some("order.searchList"),
        )
        .expect("Excel amount should resolve the tie");

        assert_eq!(result.summary.room_id, "room-b");
        assert_eq!(result.events[0].order_id, "close-amount");
    }

    #[test]
    fn product_level_count_uses_product_amount_only_to_break_duplicate_ties() {
        let amounts = [1_475_900, 886_900, 1_248_900, 694_900, 831_900];
        let events = amounts
            .into_iter()
            .enumerate()
            .map(|(index, amount)| RawPaymentEvent {
                offset_sec: index as i64,
                pay_amount_fen: Some(amount),
                product_name: "佳能主商品".into(),
                product_id: "canon-main".into(),
                order_id: format!("order-{index}"),
                order_status: if index >= 2 { "已关闭" } else { "已完成" }.into(),
                buyer_label: String::new(),
            })
            .collect::<Vec<_>>();
        let products = vec![LiveDashboardProductRow {
            id: 1,
            session_id: 1,
            product_id: "canon-main".into(),
            name: "佳能主商品".into(),
            payment_amount_fen: Some(4_306_600),
            sold_count: Some(4),
            buyer_count: None,
            exposure_count: None,
            click_count: None,
        }];
        let selected = select_product_attributed_events(events, &products);
        assert_eq!(selected.len(), 4);
        assert_eq!(
            selected
                .iter()
                .filter_map(|event| event.pay_amount_fen)
                .sum::<i64>(),
            4_306_600
        );
        assert!(selected
            .iter()
            .all(|event| event.pay_amount_fen != Some(831_900)));
    }

    #[test]
    fn reconciles_real_exports_when_paths_are_supplied() {
        let Ok(json_path) = std::env::var("BSR_REAL_ORDER_JSON") else {
            return;
        };
        let Ok(xlsx_path) = std::env::var("BSR_REAL_DASHBOARD_XLSX") else {
            return;
        };
        let imported = crate::live_data_import::parse_live_dashboard_xlsx(Path::new(&xlsx_path))
            .expect("real dashboard workbook");
        let session = LiveDashboardSessionRow {
            id: 1,
            account_key: imported.session.account_key,
            shop_name: imported.session.shop_name,
            started_at: imported.session.started_at,
            ended_at: imported.session.ended_at,
            payment_amount_fen: imported.session.payment_amount_fen,
            per_thousand_payment_amount_fen: imported.session.per_thousand_payment_amount_fen,
            viewer_count: imported.session.viewer_count,
            average_online: imported.session.average_online,
            average_watch_seconds: imported.session.average_watch_seconds,
            viewer_conversion_rate: imported.session.viewer_conversion_rate,
            deal_buyer_count: imported.session.deal_buyer_count,
            deal_item_count: imported.session.deal_item_count,
            product_click_conversion_rate: imported.session.product_click_conversion_rate,
            exposure_viewer_rate: imported.session.exposure_viewer_rate,
            qianchuan_spend_fen: imported.session.qianchuan_spend_fen,
            source_file: xlsx_path,
            imported_at: String::new(),
        };
        let products = imported
            .products
            .into_iter()
            .enumerate()
            .map(|(index, product)| LiveDashboardProductRow {
                id: index as i64 + 1,
                session_id: 1,
                product_id: product.product_id,
                name: product.name,
                payment_amount_fen: product.payment_amount_fen,
                sold_count: product.sold_count,
                buyer_count: product.buyer_count,
                exposure_count: product.exposure_count,
                click_count: product.click_count,
            })
            .collect::<Vec<_>>();

        let result = import_raw_order_timeline(Path::new(&json_path), &session, &products)
            .expect("real order timeline match");

        println!("real order match: {:?}", result.summary);
        assert!(!result.events.is_empty());
        assert_eq!(result.summary.confidence, "shop_time_and_excel_products");
        assert!(result
            .events
            .iter()
            .all(|event| !event.product_name.is_empty()));

        let payload: Value =
            serde_json::from_slice(&fs::read(&json_path).expect("read real order JSON"))
                .expect("parse real order JSON");
        let auto = match_live_window_orders_to_session(
            &payload,
            &session,
            &products,
            Some("order.searchList"),
        )
        .expect("real automatic attribution");
        println!("real automatic match: {:?}", auto.summary);
        assert_eq!(
            auto.summary.event_count as i64,
            session.deal_item_count.unwrap_or(0)
        );
        assert_eq!(
            auto.summary.total_pay_amount_fen,
            session.payment_amount_fen
        );
    }
}
