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

fn import_raw_order_timeline_value(
    payload: &Value,
    session: &LiveDashboardSessionRow,
    products: &[LiveDashboardProductRow],
    source_name: Option<&str>,
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

    for (shop_name, order) in iter_orders(payload) {
        raw_order_count += 1;
        let Some(pay_time) = positive_i64(order.get("pay_time")) else {
            continue;
        };
        if pay_time < started_at.timestamp() || pay_time > ended_at.timestamp() {
            continue;
        }

        let order_status = text(order.get("order_status_desc"))
            .or_else_non_empty(|| text(order.get("main_status_desc")));
        if is_non_deal_status(&order_status) {
            continue;
        }
        let order_id = text(order.get("order_id"));
        let sku_list = order
            .get("sku_order_list")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (sku_index, sku) in sku_list.iter().enumerate() {
            let room_id = text(sku.get("room_id"));
            if room_id.is_empty() {
                continue;
            }
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
            candidate.events.push(RawPaymentEvent {
                offset_sec: sku_pay_time - started_at.timestamp(),
                pay_amount_fen,
                product_name,
                product_id,
                order_id: event_order_id,
                order_status: order_status.clone(),
            });
        }
    }

    if candidates.is_empty() {
        return Err("订单文件中没有落在本场直播时间内、且带直播房间号的支付订单".to_string());
    }

    let expected_count = session.deal_item_count;
    let expected_amount = session.payment_amount_fen;
    let candidate_count = candidates.len();
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
    let mut ranked_indices = same_shop_indices;
    ranked_indices.sort_by_key(|index| std::cmp::Reverse(candidate_values[*index].events.len()));
    let matched_index = ranked_indices[0];
    if ranked_indices.get(1).is_some_and(|second| {
        candidate_values[*second].events.len() == candidate_values[matched_index].events.len()
    }) {
        return Err("同一店铺存在多个商品命中数相同的直播房间，无法唯一确定本场".to_string());
    }

    let mut matched = candidate_values.swap_remove(matched_index);
    matched.events.sort_by_key(|event| event.offset_sec);
    let total_pay_amount_fen = matched.total_pay_amount_fen();
    let parent_order_count = matched.parent_order_ids.len();
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

fn iter_orders(payload: &Value) -> Vec<(String, &Value)> {
    let mut output = Vec::new();
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

fn is_non_deal_status(value: &str) -> bool {
    ["已关闭", "已取消", "取消成功"]
        .into_iter()
        .any(|status| value.contains(status))
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
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("13800000000"));
        assert!(!serialized.contains("private"));
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
    }
}
