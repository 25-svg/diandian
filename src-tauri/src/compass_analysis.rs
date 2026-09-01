use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;

use crate::compass_capture::{CompassCaptureCoverageItem, CompassCaptureSummary};
#[cfg(feature = "gui")]
use crate::state::State;
#[cfg(feature = "gui")]
use recorder::platforms::PlatformType;
#[cfg(feature = "gui")]
use std::str::FromStr;
#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassMetricPoint {
    pub time_label: String,
    pub sort_value: f64,
    pub value: f64,
    #[serde(default = "default_true")]
    pub valid: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassMetricAnalysis {
    pub label: String,
    pub unit: String,
    #[serde(default = "default_metric_source_kind")]
    pub source_kind: String,
    pub points: Vec<CompassMetricPoint>,
    pub minimum: f64,
    pub maximum: f64,
    pub average: f64,
    pub peak_time: String,
    pub trend_percent: f64,
    pub volatility: f64,
    pub source_endpoint: String,
}

fn default_metric_source_kind() -> String {
    "legacy".to_string()
}

#[derive(Debug, Clone)]
struct CompassNativeSeries {
    label: String,
    unit: String,
    points: Vec<CompassMetricPoint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassProductAnalysis {
    pub product_id: String,
    pub product_name: String,
    pub explain_count: i64,
    pub click_count: i64,
    pub payment_amount: f64,
    pub sold_count: i64,
    #[serde(default)]
    pub explain_start_time: String,
    #[serde(default)]
    pub explain_end_time: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassAudienceItem {
    pub dimension: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub source_endpoint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassQianchuanMetric {
    pub key: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub source_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassAnalysisFinding {
    pub level: String,
    pub title: String,
    pub summary: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassMetricChange {
    pub label: String,
    pub change_percent: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassAnalysisSource {
    pub kind: String,
    pub live_id: String,
    pub platform: String,
    pub room_id: String,
    pub video_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassDeclineEvent {
    pub id: String,
    pub metric_label: String,
    pub start_time: String,
    pub end_time: String,
    pub start_sort_value: f64,
    pub end_sort_value: f64,
    pub baseline_value: f64,
    pub lowest_value: f64,
    pub drop_percent: f64,
    pub seek_seconds: Option<f64>,
    pub transcript_start_seconds: Option<f64>,
    pub transcript_end_seconds: Option<f64>,
    pub transcript_excerpt: String,
    pub correlated_changes: Vec<CompassMetricChange>,
    pub possible_causes: Vec<String>,
    pub confidence: String,
    pub explanation: String,
    pub limitations: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassCaptureQuality {
    pub total_response_count: usize,
    pub business_response_count: usize,
    #[serde(default)]
    pub analysis_response_count: usize,
    #[serde(default)]
    pub navigation_response_count: usize,
    pub configuration_response_count: usize,
    #[serde(default)]
    pub http_response_count: usize,
    #[serde(default)]
    pub websocket_frame_count: usize,
    #[serde(default)]
    pub parse_failure_count: usize,
    pub metric_count: usize,
    pub product_count: usize,
    #[serde(default)]
    pub planned_section_count: usize,
    #[serde(default)]
    pub captured_section_count: usize,
    #[serde(default)]
    pub unavailable_section_count: usize,
    #[serde(default)]
    pub untriggered_section_count: usize,
    #[serde(default)]
    pub parse_failed_section_count: usize,
    #[serde(default)]
    pub permission_denied_section_count: usize,
    #[serde(default)]
    pub catalog_endpoint_count: usize,
    #[serde(default)]
    pub catalog_field_count: usize,
    pub complete: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassCaptureAnalysis {
    pub capture_id: String,
    pub target_date: String,
    pub target_shop_name: String,
    pub capture_status: String,
    pub raw_path: String,
    pub quality: CompassCaptureQuality,
    pub metrics: Vec<CompassMetricAnalysis>,
    pub products: Vec<CompassProductAnalysis>,
    #[serde(default)]
    pub audience: Vec<CompassAudienceItem>,
    #[serde(default)]
    pub qianchuan: Vec<CompassQianchuanMetric>,
    #[serde(default)]
    pub coverage: Vec<CompassCaptureCoverageItem>,
    pub findings: Vec<CompassAnalysisFinding>,
    #[serde(default)]
    pub decline_events: Vec<CompassDeclineEvent>,
    #[serde(default)]
    pub source: Option<CompassAnalysisSource>,
    pub ai_summary: Option<String>,
    pub generated_at: String,
}

const TIME_KEYS: [&str; 12] = [
    "time",
    "timestamp",
    "ts",
    "date",
    "datetime",
    "minute",
    "starttime",
    "start_time",
    "label",
    "x",
    "stat_time",
    "timepoint",
];
const VALUE_KEYS: [&str; 16] = [
    "value",
    "y",
    "count",
    "amount",
    "num",
    "total",
    "rate",
    "ratio",
    "payamount",
    "paymentamount",
    "gmv",
    "cost",
    "viewer",
    "online",
    "duration",
    "score",
];

fn normalized_key(value: &str) -> String {
    value.to_ascii_lowercase().replace(['-', '_'], "")
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| {
        value.as_str().and_then(|text| {
            let cleaned = text.trim().replace([',', '¥', '%'], "");
            cleaned.parse::<f64>().ok().map(|number| {
                if text.contains('%') {
                    number / 100.0
                } else {
                    number
                }
            })
        })
    })
}

fn value_as_label(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
}

fn sort_value_for_label(label: &str, index: usize) -> f64 {
    if let Ok(number) = label.parse::<f64>() {
        return if number >= 1_000_000_000_000.0 {
            number / 1000.0
        } else {
            number
        };
    }
    if let Ok(time) = chrono::DateTime::parse_from_rfc3339(label) {
        return time.timestamp_millis() as f64 / 1000.0;
    }
    if let Ok(time) = chrono::NaiveDateTime::parse_from_str(label, "%Y-%m-%d %H:%M:%S") {
        return time.and_utc().timestamp() as f64;
    }
    if let Some((hour, minute)) = label.split_once(':') {
        if let (Ok(hour), Ok(minute)) = (hour.parse::<f64>(), minute.parse::<f64>()) {
            return hour * 3600.0 + minute * 60.0;
        }
    }
    index as f64
}

fn display_time_label(label: &str) -> String {
    let Ok(number) = label.trim().parse::<f64>() else {
        return label.to_string();
    };
    let epoch_seconds = if number >= 1_000_000_000_000.0 {
        number / 1000.0
    } else {
        number
    };
    if !(1_000_000_000.0..=4_102_444_800.0).contains(&epoch_seconds) {
        return label.to_string();
    }
    let whole_seconds = epoch_seconds.floor() as i64;
    let nanoseconds = ((epoch_seconds.fract().abs()) * 1_000_000_000.0).round() as u32;
    let Some(timestamp) = chrono::DateTime::from_timestamp(whole_seconds, nanoseconds) else {
        return label.to_string();
    };
    let Some(china_offset) = chrono::FixedOffset::east_opt(8 * 60 * 60) else {
        return label.to_string();
    };
    timestamp
        .with_timezone(&china_offset)
        .format("%m/%d %H:%M")
        .to_string()
}

fn find_object_value<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Value> {
    keys.iter().find_map(|wanted| {
        object
            .iter()
            .find(|(key, _)| normalized_key(key) == normalized_key(wanted))
            .map(|(_, value)| value)
    })
}

fn array_to_points(items: &[Value]) -> Option<Vec<CompassMetricPoint>> {
    let mut points = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let object = item.as_object()?;
        let time = find_object_value(object, &TIME_KEYS).and_then(value_as_label)?;
        let explicit_value = find_object_value(object, &VALUE_KEYS);
        let metric_value = explicit_value.and_then(value_as_f64).or_else(|| {
            explicit_value
                .is_none()
                .then(|| {
                    object.iter().find_map(|(key, value)| {
                        (!TIME_KEYS
                            .iter()
                            .any(|candidate| normalized_key(key) == normalized_key(candidate)))
                        .then(|| value_as_f64(value))
                        .flatten()
                    })
                })
                .flatten()
        });
        if explicit_value.is_none() && metric_value.is_none() {
            return None;
        }
        points.push(CompassMetricPoint {
            sort_value: sort_value_for_label(&time, index),
            time_label: display_time_label(&time),
            value: metric_value.unwrap_or_default(),
            valid: metric_value.is_some(),
        });
    }
    (points.len() >= 2).then_some(points)
}

fn collect_point_arrays(value: &Value, output: &mut Vec<Vec<CompassMetricPoint>>) {
    match value {
        Value::Array(items) => {
            if let Some(points) = array_to_points(items) {
                output.push(points);
            }
            for item in items {
                collect_point_arrays(item, output);
            }
        }
        Value::Object(map) => {
            for child in map.values() {
                collect_point_arrays(child, output);
            }
        }
        _ => {}
    }
}

fn trend_array_to_native_series(
    items: &[Value],
    units: Option<&serde_json::Map<String, Value>>,
) -> Vec<CompassNativeSeries> {
    let mut groups = BTreeMap::<String, CompassNativeSeries>::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            continue;
        };
        let point_name = text_field(object, &["point_name", "pointName"]);
        let display_name = text_field(object, &["display_name", "displayName"]);
        if point_name.is_empty() || display_name.is_empty() {
            continue;
        }
        let time_label = text_field(
            object,
            &[
                "horizontal",
                "date_time_str",
                "dateTimeStr",
                "time",
                "timestamp",
            ],
        );
        let sort_label = text_field(object, &["date_time", "dateTime", "timestamp", "time"]);
        let Some(raw_value) = find_object_value(
            object,
            &["vertical", "right_vertical", "rightVertical", "value"],
        ) else {
            continue;
        };
        let value = value_as_f64(raw_value);
        let unit = units
            .and_then(|values| values.get(&point_name))
            .and_then(value_as_label)
            .unwrap_or_default();
        let group = groups
            .entry(point_name.clone())
            .or_insert_with(|| CompassNativeSeries {
                label: display_name.clone(),
                unit,
                points: Vec::new(),
            });
        let sort_value = if sort_label.is_empty() {
            index as f64
        } else {
            sort_value_for_label(&sort_label, index)
        };
        let display_label = if time_label.is_empty() {
            sort_label.clone()
        } else {
            time_label
        };
        group.points.push(CompassMetricPoint {
            time_label: display_time_label(&display_label),
            sort_value,
            value: value.unwrap_or_default(),
            valid: value.is_some(),
        });
    }
    groups
        .into_values()
        .filter(|series| series.points.len() >= 2)
        .collect()
}

fn collect_native_trend_series(value: &Value, output: &mut Vec<CompassNativeSeries>) {
    match value {
        Value::Object(object) => {
            if let Some(items) = object.get("trends").and_then(Value::as_array) {
                let units = object.get("unit").and_then(Value::as_object);
                output.extend(trend_array_to_native_series(items, units));
            }
            for (key, child) in object {
                if key != "trends" {
                    collect_native_trend_series(child, output);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_native_trend_series(item, output);
            }
        }
        _ => {}
    }
}

fn meta_endpoint(meta: &Value) -> String {
    meta.get("url")
        .and_then(Value::as_str)
        .and_then(|url| url::Url::parse(url).ok())
        .map(|url| url.path().to_string())
        .unwrap_or_default()
}

fn normalized_session_clock(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .take(12)
        .collect()
}

fn session_minute(value: &str) -> Option<i64> {
    let normalized = normalized_session_clock(value);
    (normalized.len() == 12)
        .then(|| normalized.parse::<i64>().ok())
        .flatten()
}

fn session_key_matches(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    let expected_parts = expected.split('|').collect::<Vec<_>>();
    let actual_parts = actual.split('|').collect::<Vec<_>>();
    if expected_parts.len() < 2
        || actual_parts.len() < 2
        || expected_parts[0].trim() != actual_parts[0].trim()
    {
        return false;
    }
    let Some(expected_start) = session_minute(expected_parts[1]) else {
        return false;
    };
    let Some(actual_start) = session_minute(actual_parts[1]) else {
        return false;
    };
    if expected_start == actual_start {
        return true;
    }
    actual_parts
        .get(2)
        .and_then(|value| session_minute(value))
        .is_some_and(|actual_end| actual_start <= expected_start && expected_start <= actual_end)
}

fn response_matches_session(meta: &Value, expected_session_key: &str) -> bool {
    let stored_session_key = meta
        .get("sessionKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if !stored_session_key.is_empty() {
        return session_key_matches(expected_session_key, stored_session_key);
    }

    // CDP responses can arrive before the page collector attaches a sessionKey.
    // Their capture target still identifies the requested shop and broadcast date.
    let expected_parts = expected_session_key.split('|').collect::<Vec<_>>();
    if expected_parts.len() < 2 {
        return false;
    }
    let expected_shop = expected_parts[0].trim();
    let expected_date = expected_parts[1].trim().get(..10).unwrap_or_default();
    let target_shop = meta
        .get("targetShopName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let target_date = meta
        .get("targetDate")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    !expected_shop.is_empty()
        && !expected_date.is_empty()
        && target_shop == expected_shop
        && target_date == expected_date
}

fn metric_label_from_meta(meta: &Value) -> String {
    if let Some(label) = meta
        .get("metricLabel")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    {
        return label.to_string();
    }
    let searchable = format!(
        "{} {}",
        meta.get("url").and_then(Value::as_str).unwrap_or_default(),
        meta.get("requestPostData")
            .and_then(Value::as_str)
            .unwrap_or_default()
    )
    .to_ascii_lowercase();
    [
        (
            "成交金额",
            &["成交金额", "pay_amount", "payment_amount", "gmv"][..],
        ),
        ("投放消耗", &["投放消耗", "ad_cost", "cost"][..]),
        ("在线人数", &["在线人数", "online_user", "online_count"][..]),
        ("互动率", &["互动率", "interaction_rate"][..]),
        ("关注率", &["关注率", "follow_rate"][..]),
        ("商品点击率", &["商品点击率", "product_click_rate"][..]),
    ]
    .iter()
    .find(|(_, aliases)| aliases.iter().any(|alias| searchable.contains(alias)))
    .map(|(label, _)| (*label).to_string())
    .unwrap_or_default()
}

fn text_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> String {
    find_object_value(object, keys)
        .and_then(value_as_label)
        .unwrap_or_default()
}

fn integer_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> i64 {
    find_object_value(object, keys)
        .and_then(value_as_f64)
        .unwrap_or_default()
        .round() as i64
}

fn number_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> f64 {
    find_object_value(object, keys)
        .and_then(value_as_f64)
        .unwrap_or_default()
}

fn collect_products(value: &Value, output: &mut HashMap<String, CompassProductAnalysis>) {
    match value {
        Value::Object(object) => {
            let product_id = text_field(
                object,
                &[
                    "product_id",
                    "productid",
                    "goods_id",
                    "goodsid",
                    "item_id",
                    "itemid",
                ],
            );
            let product_name = text_field(
                object,
                &[
                    "product_name",
                    "productname",
                    "goods_name",
                    "goodsname",
                    "item_name",
                    "itemname",
                    "title",
                ],
            );
            if !product_name.is_empty()
                && (!product_id.is_empty()
                    || object
                        .keys()
                        .any(|key| normalized_key(key).contains("product")))
            {
                let key = if product_id.is_empty() {
                    product_name.clone()
                } else {
                    product_id.clone()
                };
                let product = output.entry(key).or_default();
                if product.product_id.is_empty() {
                    product.product_id = product_id;
                }
                if product.product_name.is_empty() {
                    product.product_name = product_name;
                }
                product.explain_count = product.explain_count.max(integer_field(
                    object,
                    &["explain_count", "explaincount", "explanation_count"],
                ));
                product.click_count = product.click_count.max(integer_field(
                    object,
                    &["click_count", "clickcount", "product_click_count"],
                ));
                product.payment_amount = product.payment_amount.max(number_field(
                    object,
                    &["payment_amount", "pay_amount", "gmv"],
                ));
                product.sold_count = product.sold_count.max(integer_field(
                    object,
                    &["sold_count", "pay_order_count", "order_count"],
                ));
                if product.explain_start_time.is_empty() {
                    product.explain_start_time = text_field(
                        object,
                        &[
                            "explain_start_time",
                            "explainStartTime",
                            "start_time",
                            "startTime",
                        ],
                    );
                }
                if product.explain_end_time.is_empty() {
                    product.explain_end_time = text_field(
                        object,
                        &["explain_end_time", "explainEndTime", "end_time", "endTime"],
                    );
                }
            }
            for child in object.values() {
                collect_products(child, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_products(item, output);
            }
        }
        _ => {}
    }
}

fn response_section(meta: &Value, endpoint: &str, body: &Value) -> &'static str {
    let section_key = meta
        .get("sectionKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let searchable = format!("{section_key} {}", endpoint.to_ascii_lowercase());
    if searchable.contains("audience")
        || searchable.contains("portrait")
        || searchable.contains("crowd")
    {
        "audience"
    } else if searchable.contains("qianchuan")
        || searchable.contains("advertising")
        || searchable.contains("oceanengine")
        || searchable.contains("roi_info")
        || section_key == "module.qianchuan"
        || contains_qianchuan_metric(body)
    {
        "qianchuan"
    } else {
        ""
    }
}

fn audience_dimension(path: &[String], object: &serde_json::Map<String, Value>) -> String {
    let explicit = text_field(
        object,
        &["dimension_name", "dimensionName", "dimension", "group_name"],
    );
    if !explicit.is_empty() {
        return explicit;
    }
    let searchable = path.join(".").to_ascii_lowercase();
    for (label, aliases) in [
        ("性别", &["gender", "sex"][..]),
        ("年龄", &["age"][..]),
        ("地域", &["province", "city", "region", "area"][..]),
        (
            "手机价格",
            &["phone_price", "device_price", "phoneprice"][..],
        ),
        ("策略人群", &["strategy", "crowd", "tag"][..]),
    ] {
        if aliases.iter().any(|alias| searchable.contains(alias)) {
            return label.to_string();
        }
    }
    path.last()
        .cloned()
        .unwrap_or_else(|| "其他人群".to_string())
}

fn collect_audience_items(
    value: &Value,
    endpoint: &str,
    path: &mut Vec<String>,
    output: &mut HashMap<String, CompassAudienceItem>,
) {
    match value {
        Value::Object(object) => {
            let label = text_field(
                object,
                &["label", "name", "display_name", "displayName", "title"],
            );
            let raw_value = find_object_value(
                object,
                &["value", "count", "ratio", "percent", "percentage"],
            )
            .and_then(value_as_f64);
            if !label.is_empty() {
                if let Some(number) = raw_value {
                    let dimension = audience_dimension(path, object);
                    let unit = text_field(object, &["unit"]);
                    let key = format!("{dimension}|{label}");
                    output.entry(key).or_insert_with(|| CompassAudienceItem {
                        dimension,
                        label,
                        value: number,
                        unit,
                        source_endpoint: endpoint.to_string(),
                    });
                }
            }
            for (key, child) in object {
                path.push(key.clone());
                collect_audience_items(child, endpoint, path, output);
                path.pop();
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_audience_items(item, endpoint, path, output);
            }
        }
        _ => {}
    }
}

fn qianchuan_metric_definition(key: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let key = normalized_key(key);
    [
        (
            "投放消耗",
            "spend",
            "元",
            &[
                "spend",
                "adspend",
                "adcost",
                "qianchuancost",
                "cost",
                "statcost",
                "adcostedamt",
            ][..],
        ),
        (
            "投放成交金额",
            "payment",
            "元",
            &["paymentamount", "payamount", "adgmv", "gmv"][..],
        ),
        ("ROI", "roi", "", &["roi", "payroi", "roi2"][..]),
        (
            "曝光",
            "impressions",
            "次",
            &["impressions", "impression", "exposure", "showcount"][..],
        ),
        (
            "点击",
            "clicks",
            "次",
            &["clicks", "clickcount", "adclickcount"][..],
        ),
        (
            "成交",
            "conversions",
            "单",
            &[
                "conversions",
                "conversioncount",
                "payordercount",
                "ordercount",
            ][..],
        ),
    ]
    .into_iter()
    .find(|(_, _, _, aliases)| aliases.iter().any(|alias| normalized_key(alias) == key))
    .map(|(label, stable_key, unit, _)| (label, stable_key, unit))
}

fn contains_qianchuan_metric(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            qianchuan_metric_definition(key).is_some()
                || (normalized_key(key) == "indexname"
                    && child
                        .as_str()
                        .and_then(qianchuan_metric_definition)
                        .is_some())
                || contains_qianchuan_metric(child)
        }),
        Value::Array(items) => items.iter().any(contains_qianchuan_metric),
        _ => false,
    }
}

fn qianchuan_numeric_value(value: &Value) -> Option<f64> {
    if let Some(number) = value_as_f64(value) {
        return Some(number);
    }
    let object = value.as_object()?;
    for key in ["value", "index_values", "indexValues"] {
        if let Some(child) = object.get(key) {
            if let Some(mut number) = qianchuan_numeric_value(child) {
                let is_price = object
                    .get("unit")
                    .is_some_and(|unit| unit.as_str() == Some("price") || unit.as_i64() == Some(3));
                if is_price {
                    number /= 100.0;
                }
                return Some(number);
            }
        }
    }
    None
}

fn collect_qianchuan_metrics(
    value: &Value,
    endpoint: &str,
    output: &mut HashMap<String, CompassQianchuanMetric>,
) {
    match value {
        Value::Object(object) => {
            let indexed_definition = text_field(object, &["index_name", "indexName"]);
            if let Some((label, stable_key, unit)) =
                qianchuan_metric_definition(&indexed_definition)
            {
                if let Some(number) = find_object_value(object, &["value", "index_values"])
                    .and_then(qianchuan_numeric_value)
                {
                    output.entry(stable_key.to_string()).or_insert_with(|| {
                        CompassQianchuanMetric {
                            key: stable_key.to_string(),
                            label: label.to_string(),
                            value: number,
                            unit: unit.to_string(),
                            source_endpoint: endpoint.to_string(),
                        }
                    });
                }
            }
            for (key, child) in object {
                if let Some((label, stable_key, unit)) = qianchuan_metric_definition(key) {
                    if let Some(number) = qianchuan_numeric_value(child) {
                        output.entry(stable_key.to_string()).or_insert_with(|| {
                            CompassQianchuanMetric {
                                key: stable_key.to_string(),
                                label: label.to_string(),
                                value: number,
                                unit: unit.to_string(),
                                source_endpoint: endpoint.to_string(),
                            }
                        });
                    }
                }
                collect_qianchuan_metrics(child, endpoint, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_qianchuan_metrics(item, endpoint, output);
            }
        }
        _ => {}
    }
}

fn summarize_metric(
    label: String,
    unit: String,
    source_kind: String,
    source_endpoint: String,
    mut points: Vec<CompassMetricPoint>,
) -> CompassMetricAnalysis {
    points.sort_by(|left, right| left.sort_value.total_cmp(&right.sort_value));
    points.dedup_by(|left, right| left.time_label == right.time_label);
    let valid_points = points
        .iter()
        .filter(|point| point.valid)
        .collect::<Vec<_>>();
    let minimum = valid_points
        .iter()
        .map(|point| point.value)
        .reduce(f64::min)
        .unwrap_or_default();
    let maximum = valid_points
        .iter()
        .map(|point| point.value)
        .reduce(f64::max)
        .unwrap_or_default();
    let average = valid_points.iter().map(|point| point.value).sum::<f64>()
        / valid_points.len().max(1) as f64;
    let peak_time = valid_points
        .iter()
        .max_by(|left, right| left.value.total_cmp(&right.value))
        .map(|point| point.time_label.clone())
        .unwrap_or_default();
    let first = valid_points
        .first()
        .map(|point| point.value)
        .unwrap_or_default();
    let last = valid_points
        .last()
        .map(|point| point.value)
        .unwrap_or_default();
    let trend_percent = if first.abs() > f64::EPSILON {
        (last - first) / first.abs() * 100.0
    } else {
        0.0
    };
    let volatility = (valid_points
        .iter()
        .map(|point| (point.value - average).powi(2))
        .sum::<f64>()
        / valid_points.len().max(1) as f64)
        .sqrt();
    CompassMetricAnalysis {
        label,
        unit,
        source_kind,
        points,
        minimum,
        maximum,
        average,
        peak_time,
        trend_percent,
        volatility,
        source_endpoint,
    }
}

fn average_points(points: &[CompassMetricPoint]) -> f64 {
    let valid = points
        .iter()
        .filter(|point| point.valid)
        .collect::<Vec<_>>();
    valid.iter().map(|point| point.value).sum::<f64>() / valid.len().max(1) as f64
}

fn metric_is_decline_relevant(label: &str) -> bool {
    [
        "在线人数",
        "曝光-观看率",
        "互动率",
        "关注率",
        "商品点击率",
        "商品点击-成交率",
        "千次观看用户支付金额",
        "用户支付金额",
        "成交金额",
    ]
    .iter()
    .any(|candidate| label.contains(candidate))
}

fn change_for_window(
    metric: &CompassMetricAnalysis,
    start_sort_value: f64,
    end_sort_value: f64,
) -> Option<CompassMetricChange> {
    let start_index = metric
        .points
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (left.sort_value - start_sort_value)
                .abs()
                .total_cmp(&(right.sort_value - start_sort_value).abs())
        })?
        .0;
    let end_index = metric
        .points
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (left.sort_value - end_sort_value)
                .abs()
                .total_cmp(&(right.sort_value - end_sort_value).abs())
        })?
        .0;
    let baseline_start = start_index.saturating_sub(3);
    let baseline = average_points(&metric.points[baseline_start..start_index.max(1)]);
    let low_start = start_index.min(metric.points.len() - 1);
    let low_end = end_index.max(low_start).min(metric.points.len() - 1);
    let during = average_points(&metric.points[low_start..=low_end]);
    if baseline.abs() <= f64::EPSILON {
        return None;
    }
    Some(CompassMetricChange {
        label: metric.label.clone(),
        change_percent: (during - baseline) / baseline.abs() * 100.0,
    })
}

fn deterministic_causes(changes: &[CompassMetricChange], transcript_excerpt: &str) -> Vec<String> {
    if transcript_excerpt.trim().is_empty() {
        return Vec::new();
    }
    let mut causes = Vec::new();
    let change = |needle: &str| {
        changes
            .iter()
            .find(|item| item.label.contains(needle))
            .map(|item| item.change_percent)
    };
    if change("投放消耗").is_some_and(|value| value <= -20.0) {
        causes.push("同期投放消耗明显下降，存在时间相关；是否与投放收缩有关仍待核实".to_string());
    }
    if change("曝光-观看率").is_some_and(|value| value <= -15.0) {
        causes.push(
            "曝光进入直播间效率同步下降，存在时间相关；开场承接和画面因素仍待核实".to_string(),
        );
    }
    if change("互动率").is_some_and(|value| value <= -15.0) {
        causes.push("互动率同步下降，存在时间相关；主播互动方式是否相关仍待核实".to_string());
    }
    if change("商品点击率").is_some_and(|value| value <= -15.0) {
        causes.push("商品点击率同步下降，存在时间相关；商品利益点和购买指令仍待核实".to_string());
    }
    if causes.is_empty() {
        causes.push(
            "其他指标没有出现同幅度变化，需要结合该时间窗原话人工复核，不能直接认定是话术造成"
                .to_string(),
        );
    }
    causes
}

fn detect_decline_events(metrics: &[CompassMetricAnalysis]) -> Vec<CompassDeclineEvent> {
    let mut events = Vec::new();
    for metric in metrics
        .iter()
        .filter(|metric| metric_is_decline_relevant(&metric.label))
    {
        let points = &metric.points;
        if points.len() < 6 {
            continue;
        }
        let mut index = 3usize;
        while index + 2 < points.len() {
            if points[index - 3..=index + 2]
                .iter()
                .any(|point| !point.valid)
            {
                index += 1;
                continue;
            }
            let baseline = average_points(&points[index - 3..index]);
            if baseline <= f64::EPSILON {
                index += 1;
                continue;
            }
            let first_window_end = (index + 2).min(points.len() - 1);
            let first_window = &points[index..=first_window_end];
            let first_average = average_points(first_window);
            let first_lowest = first_window
                .iter()
                .map(|point| point.value)
                .fold(f64::INFINITY, f64::min);
            let sustained_drop = (baseline - first_average) / baseline >= 0.15;
            let significant_low = (baseline - first_lowest) / baseline >= 0.20;
            if !sustained_drop || !significant_low {
                index += 1;
                continue;
            }

            let mut end_index = first_window_end;
            let max_end = (index + 14).min(points.len() - 1);
            while end_index < max_end
                && points[end_index + 1].valid
                && points[end_index + 1].value < baseline * 0.90
            {
                end_index += 1;
            }
            let lowest = points[index..=end_index]
                .iter()
                .map(|point| point.value)
                .fold(f64::INFINITY, f64::min);
            let drop_percent = (baseline - lowest) / baseline * 100.0;
            let correlated_changes = metrics
                .iter()
                .filter(|candidate| candidate.label != metric.label)
                .filter_map(|candidate| {
                    change_for_window(
                        candidate,
                        points[index].sort_value,
                        points[end_index].sort_value,
                    )
                })
                .filter(|change| change.change_percent.abs() >= 8.0)
                .take(6)
                .collect::<Vec<_>>();
            events.push(CompassDeclineEvent {
                id: format!("decline-{}-{}", events.len() + 1, index),
                metric_label: metric.label.clone(),
                start_time: points[index].time_label.clone(),
                end_time: points[end_index].time_label.clone(),
                start_sort_value: points[index].sort_value,
                end_sort_value: points[end_index].sort_value,
                baseline_value: baseline,
                lowest_value: lowest,
                drop_percent,
                seek_seconds: None,
                transcript_start_seconds: None,
                transcript_end_seconds: None,
                transcript_excerpt: String::new(),
                possible_causes: deterministic_causes(&correlated_changes, ""),
                correlated_changes,
                confidence: "待核实".to_string(),
                explanation: "待核实：当前时间段没有逐字稿或节奏地图证据。".to_string(),
                limitations: "曲线变化只能证明时间相关，不能单独证明因果".to_string(),
            });
            index = end_index.saturating_add(1);
        }
    }
    events.sort_by(|left, right| {
        right
            .drop_percent
            .total_cmp(&left.drop_percent)
            .then(left.start_sort_value.total_cmp(&right.start_sort_value))
    });
    events.truncate(12);
    events
}

pub fn analyze_capture_directory(
    capture_id: &str,
    directory: &std::path::Path,
) -> Result<CompassCaptureAnalysis, String> {
    analyze_capture_directory_for_session(capture_id, directory, None)
}

pub fn analyze_capture_directory_for_session(
    capture_id: &str,
    directory: &std::path::Path,
    session_key: Option<&str>,
) -> Result<CompassCaptureAnalysis, String> {
    let manifest: CompassCaptureSummary = serde_json::from_str(
        &fs::read_to_string(directory.join("capture.json"))
            .map_err(|error| format!("无法读取采集清单：{error}"))?,
    )
    .map_err(|error| format!("采集清单格式错误：{error}"))?;
    let mut metric_groups =
        BTreeMap::<String, (String, String, String, String, Vec<CompassMetricPoint>)>::new();
    let mut products = HashMap::<String, CompassProductAnalysis>::new();
    let mut audience = HashMap::<String, CompassAudienceItem>::new();
    let mut qianchuan = HashMap::<String, CompassQianchuanMetric>::new();
    let responses = directory.join("responses");
    for entry in fs::read_dir(&responses)
        .map_err(|error| format!("无法读取采集响应：{error}"))?
        .flatten()
    {
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(stored) = fs::read_to_string(entry.path())
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .ok_or(())
        else {
            continue;
        };
        let meta = &stored["meta"];
        let body = &stored["body"];
        if let Some(expected_session_key) = session_key {
            if !response_matches_session(meta, expected_session_key) {
                continue;
            }
        }
        let endpoint = meta_endpoint(meta);
        if endpoint.contains("config_center")
            || endpoint.contains("/fe_dynamic/page/schema")
            || endpoint.contains("announce")
            || endpoint.contains("common_libra")
        {
            continue;
        }
        let mut native_series = Vec::new();
        collect_native_trend_series(body, &mut native_series);
        if native_series.is_empty() {
            let label = metric_label_from_meta(meta);
            let mut candidates = Vec::new();
            collect_point_arrays(body, &mut candidates);
            if let Some(selected) = candidates.into_iter().max_by_key(Vec::len) {
                let label = if label.is_empty() {
                    format!("未命名指标 {}", metric_groups.len() + 1)
                } else {
                    label
                };
                let group = metric_groups.entry(label.clone()).or_insert_with(|| {
                    (
                        label,
                        String::new(),
                        String::new(),
                        "legacy".to_string(),
                        Vec::new(),
                    )
                });
                if selected.len() > group.4.len() {
                    group.4 = selected;
                    group.2 = endpoint.clone();
                }
            }
        } else {
            for series in native_series {
                let group = metric_groups
                    .entry(series.label.clone())
                    .or_insert_with(|| {
                        (
                            series.label.clone(),
                            series.unit.clone(),
                            endpoint.clone(),
                            "compass_native".to_string(),
                            Vec::new(),
                        )
                    });
                if series.points.len() > group.4.len() {
                    group.0 = series.label;
                    group.1 = series.unit;
                    group.2 = endpoint.clone();
                    group.3 = "compass_native".to_string();
                    group.4 = series.points;
                }
                if group.2.is_empty() {
                    group.2 = endpoint.clone();
                }
            }
        }
        collect_products(body, &mut products);
        match response_section(meta, &endpoint, body) {
            "audience" => collect_audience_items(body, &endpoint, &mut Vec::new(), &mut audience),
            "qianchuan" => collect_qianchuan_metrics(body, &endpoint, &mut qianchuan),
            _ => {}
        }
    }
    let metrics = metric_groups
        .into_iter()
        .filter_map(|(_, (label, unit, endpoint, source_kind, points))| {
            (points.len() >= 2)
                .then(|| summarize_metric(label, unit, source_kind, endpoint, points))
        })
        .collect::<Vec<_>>();
    let mut products = products.into_values().collect::<Vec<_>>();
    products.sort_by(|left, right| {
        right
            .payment_amount
            .total_cmp(&left.payment_amount)
            .then(right.explain_count.cmp(&left.explain_count))
    });
    let mut audience = audience.into_values().collect::<Vec<_>>();
    audience.sort_by(|left, right| {
        left.dimension
            .cmp(&right.dimension)
            .then(right.value.total_cmp(&left.value))
            .then(left.label.cmp(&right.label))
    });
    let mut qianchuan = qianchuan.into_values().collect::<Vec<_>>();
    qianchuan.sort_by(|left, right| left.key.cmp(&right.key));
    let mut issues = Vec::new();
    if manifest.analysis_response_count == 0 {
        issues.push("尚未采集到曲线或商品接口；首页和直播列表响应不能生成动态分析".to_string());
    }
    if metrics.is_empty() {
        issues.push("尚未识别到可绘制的时间序列曲线".to_string());
    }
    if manifest.status == "running" {
        issues.push("采集仍在进行，当前分析可能不完整".to_string());
    }
    let pending_coverage = manifest
        .coverage
        .values()
        .filter(|item| matches!(item.status.as_str(), "pending" | "capturing"))
        .count();
    let unavailable_coverage = manifest
        .coverage
        .values()
        .filter(|item| matches!(item.status.as_str(), "unavailable" | "permission_denied"))
        .count();
    let untriggered_coverage = manifest
        .coverage
        .values()
        .filter(|item| item.status == "untriggered")
        .count();
    let parse_failed_coverage = manifest
        .coverage
        .values()
        .filter(|item| item.status == "parse_failed")
        .count();
    if pending_coverage > 0 {
        issues.push(format!(
            "仍有 {pending_coverage} 个页面或指标未完成覆盖确认"
        ));
    }
    if unavailable_coverage > 0 {
        issues.push(format!(
            "当前账号或当前场次未提供 {unavailable_coverage} 个计划页面/指标；已明确标记，不按 0 处理"
        ));
    }
    if untriggered_coverage > 0 {
        issues.push(format!(
            "有 {untriggered_coverage} 个页面或指标已点击，但没有捕获到对应响应"
        ));
    }
    if parse_failed_coverage > 0 || manifest.parse_failure_count > 0 {
        issues.push(format!(
            "有 {} 次响应未能解析为 JSON，其中 {parse_failed_coverage} 个覆盖目标尚无可用响应",
            manifest.parse_failure_count
        ));
    }
    let complete = manifest.analysis_response_count > 0
        && !metrics.is_empty()
        && manifest.status == "completed";
    let planned_section_count = manifest.coverage.len();
    let captured_section_count = manifest
        .coverage
        .values()
        .filter(|item| item.status == "captured")
        .count();
    let unavailable_section_count = manifest
        .coverage
        .values()
        .filter(|item| item.status == "unavailable")
        .count();
    let permission_denied_section_count = manifest
        .coverage
        .values()
        .filter(|item| item.status == "permission_denied")
        .count();
    let quality = CompassCaptureQuality {
        total_response_count: manifest.response_count,
        business_response_count: manifest.business_response_count,
        analysis_response_count: manifest.analysis_response_count,
        navigation_response_count: manifest.navigation_response_count.max(
            manifest
                .business_response_count
                .saturating_sub(manifest.analysis_response_count),
        ),
        configuration_response_count: manifest.configuration_response_count,
        http_response_count: manifest.http_response_count,
        websocket_frame_count: manifest.websocket_frame_count,
        parse_failure_count: manifest.parse_failure_count,
        metric_count: metrics.len(),
        product_count: products.len(),
        planned_section_count,
        captured_section_count,
        unavailable_section_count,
        untriggered_section_count: untriggered_coverage,
        parse_failed_section_count: parse_failed_coverage,
        permission_denied_section_count,
        catalog_endpoint_count: manifest.catalog_endpoint_count,
        catalog_field_count: manifest.catalog_field_count,
        complete,
        issues,
    };
    let decline_events = detect_decline_events(&metrics);
    let mut findings = Vec::new();
    for metric in metrics.iter().take(6) {
        findings.push(CompassAnalysisFinding {
            level: if metric.trend_percent < -20.0 {
                "warning".into()
            } else {
                "info".into()
            },
            title: format!("{}峰值", metric.label),
            summary: format!(
                "峰值 {:.2}，均值 {:.2}，首尾变化 {:+.1}%",
                metric.maximum, metric.average, metric.trend_percent
            ),
            evidence: format!(
                "峰值时间 {}；数据点 {} 个",
                metric.peak_time,
                metric.points.len()
            ),
        });
    }
    if findings.is_empty() {
        findings.push(CompassAnalysisFinding {
            level: "warning".into(),
            title: "暂无可分析曲线".into(),
            summary: "请重新执行完整采集，并等待页面指标接口返回后再结束。".into(),
            evidence: format!(
                "可分析响应 {} 条；页面业务响应 {} 条",
                manifest.analysis_response_count, manifest.business_response_count
            ),
        });
    }
    Ok(CompassCaptureAnalysis {
        capture_id: capture_id.to_string(),
        target_date: manifest.target_date,
        target_shop_name: manifest.target_shop_name,
        capture_status: manifest.status,
        raw_path: directory.to_string_lossy().to_string(),
        quality,
        metrics,
        products,
        audience,
        qianchuan,
        coverage: manifest.coverage.into_values().collect(),
        findings,
        decline_events,
        source: None,
        ai_summary: None,
        generated_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn srt_time_seconds(time: &srtparse::Time) -> f64 {
    (time.hours * 3600 + time.minutes * 60 + time.seconds) as f64
        + time.milliseconds as f64 / 1000.0
}

fn transcript_excerpt(srt: &str, start_seconds: f64, end_seconds: f64) -> String {
    let Ok(items) = srtparse::from_str(srt) else {
        return String::new();
    };
    let mut lines = Vec::new();
    let mut length = 0usize;
    for item in items {
        let start = srt_time_seconds(&item.start_time);
        let end = srt_time_seconds(&item.end_time);
        if end < start_seconds || start > end_seconds {
            continue;
        }
        let text = item.text.trim().replace(['\r', '\n'], " ");
        if text.is_empty() {
            continue;
        }
        let total = start.max(0.0).round() as u64;
        let line = format!(
            "[{:02}:{:02}:{:02}] {}",
            total / 3600,
            total % 3600 / 60,
            total % 60,
            text
        );
        length += line.chars().count();
        lines.push(line);
        if length >= 1_800 {
            break;
        }
    }
    lines.join("\n")
}

fn session_start_seconds_of_day(started_at: &str) -> Option<f64> {
    let clock = started_at
        .split(['T', ' '])
        .nth(1)?
        .split(['+', 'Z'])
        .next()?;
    let mut parts = clock.split(':');
    let hour = parts.next()?.parse::<f64>().ok()?;
    let minute = parts.next()?.parse::<f64>().ok()?;
    let second = parts
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();
    Some(hour * 3600.0 + minute * 60.0 + second)
}

fn relative_seconds_for_point(
    time_label: &str,
    sort_value: f64,
    session_started_at: &str,
) -> Option<f64> {
    if let (Ok(point), Ok(start)) = (
        chrono::DateTime::parse_from_rfc3339(time_label),
        chrono::DateTime::parse_from_rfc3339(session_started_at),
    ) {
        return Some((point - start).num_milliseconds() as f64 / 1000.0);
    }
    if let Some(start_day_seconds) = session_start_seconds_of_day(session_started_at) {
        let point_day_seconds = if time_label.contains(':') {
            let clock = time_label
                .split(['T', ' '])
                .next_back()
                .unwrap_or(time_label)
                .split(['+', 'Z'])
                .next()
                .unwrap_or(time_label);
            let mut parts = clock.split(':');
            let hour = parts.next()?.parse::<f64>().ok()?;
            let minute = parts.next()?.parse::<f64>().ok()?;
            let second = parts
                .next()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or_default();
            hour * 3600.0 + minute * 60.0 + second
        } else if sort_value < 86_400.0 {
            sort_value
        } else {
            return None;
        };
        let mut relative = point_day_seconds - start_day_seconds;
        if relative < -43_200.0 {
            relative += 86_400.0;
        }
        return Some(relative.max(0.0));
    }
    None
}

fn evidence_window(start_seconds: f64, end_seconds: f64) -> (f64, f64, f64) {
    (
        (start_seconds - 30.0).max(0.0),
        (start_seconds - 60.0).max(0.0),
        end_seconds.max(start_seconds) + 60.0,
    )
}

fn enrich_decline_events_with_transcript(
    analysis: &mut CompassCaptureAnalysis,
    session_started_at: &str,
    transcript: &str,
) {
    for event in &mut analysis.decline_events {
        let Some(start_seconds) = relative_seconds_for_point(
            &event.start_time,
            event.start_sort_value,
            session_started_at,
        ) else {
            continue;
        };
        let end_seconds =
            relative_seconds_for_point(&event.end_time, event.end_sort_value, session_started_at)
                .unwrap_or(start_seconds + 180.0)
                .max(start_seconds);
        let (seek_seconds, transcript_start, transcript_end) =
            evidence_window(start_seconds, end_seconds);
        let excerpt = transcript_excerpt(transcript, transcript_start, transcript_end);
        event.seek_seconds = Some(seek_seconds);
        event.transcript_start_seconds = Some(transcript_start);
        event.transcript_end_seconds = Some(transcript_end);
        event.transcript_excerpt = excerpt;
        event.possible_causes =
            deterministic_causes(&event.correlated_changes, &event.transcript_excerpt);
        if event.transcript_excerpt.trim().is_empty() {
            event.confidence = "待核实".to_string();
            event.explanation = "待核实：当前时间段没有逐字稿或节奏地图证据。".to_string();
        } else {
            event.confidence = "中".to_string();
            event.explanation =
                "下降区间已与主播原话及同期指标对齐；当前只确认时间相关，不代表因果".to_string();
        }
    }
}

#[derive(Debug, Deserialize)]
struct AiDeclineEventReview {
    id: String,
    #[serde(default)]
    explanation: String,
    #[serde(default)]
    confidence: String,
    #[serde(default)]
    possible_causes: Vec<String>,
    #[serde(default)]
    limitations: String,
}

#[derive(Debug, Deserialize)]
struct AiDeclineReviewBundle {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    events: Vec<AiDeclineEventReview>,
}

fn json_object_slice(value: &str) -> &str {
    match (value.find('{'), value.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &value[start..=end],
        _ => value,
    }
}

fn evidence_safe_claim(value: &str) -> bool {
    ![
        "因此导致",
        "直接导致",
        "造成了",
        "证明了",
        "可以确定",
        "确定是",
        "根本原因是",
    ]
    .iter()
    .any(|phrase| value.contains(phrase))
}

#[cfg(feature = "gui")]
async fn apply_ai_decline_review(
    state: &State,
    analysis: &mut CompassCaptureAnalysis,
) -> Result<(), String> {
    let reviewable = analysis
        .decline_events
        .iter()
        .filter(|event| !event.transcript_excerpt.trim().is_empty())
        .take(6)
        .map(|event| {
            serde_json::json!({
                "id": event.id,
                "metric": event.metric_label,
                "time": format!("{}-{}", event.start_time, event.end_time),
                "dropPercent": event.drop_percent,
                "correlatedChanges": event.correlated_changes,
                "transcript": event.transcript_excerpt,
            })
        })
        .collect::<Vec<_>>();
    if reviewable.is_empty()
        && analysis.metrics.is_empty()
        && analysis.products.is_empty()
        && analysis.audience.is_empty()
        && analysis.qianchuan.is_empty()
    {
        return Ok(());
    }
    let api_key = crate::handlers::ai::configured_minimax_api_key(state).await?;
    let full_context = serde_json::json!({
        "metrics": analysis.metrics.iter().map(|metric| serde_json::json!({
            "label": metric.label,
            "unit": metric.unit,
            "minimum": metric.minimum,
            "maximum": metric.maximum,
            "average": metric.average,
            "peakTime": metric.peak_time,
            "trendPercent": metric.trend_percent,
        })).collect::<Vec<_>>(),
        "products": analysis.products.iter().take(20).collect::<Vec<_>>(),
        "audience": analysis.audience.iter().take(50).collect::<Vec<_>>(),
        "qianchuan": analysis.qianchuan,
        "declineEvents": reviewable,
    });
    let request = serde_json::json!({
        "model": "MiniMax-M2.5",
        "max_tokens": 3000,
        "system": "你是直播运营数据分析师。输入包含官方罗盘曲线、商品、人群、千川和带时间戳主播原话。只能使用实际给出的字段；缺失模块必须写待核实。不得把相关性写成确定因果，不得补写未提供的话术。输出严格JSON：{summary:string,events:[{id:string,explanation:string,confidence:'高'|'中'|'低',possible_causes:string[],limitations:string}]}。summary需综合实际存在的全部模块；每个事件结论必须说明数据证据和原话证据；若投放、平台分发等外部信息缺失，必须写入limitations。",
        "messages": [{
            "role": "user",
            "content": format!("请综合分析本场数据并重点解释下降事件：{}", serde_json::to_string(&full_context).unwrap_or_default())
        }]
    });
    let response = crate::handlers::ai::request_minimax_payload(&api_key, &request).await?;
    let parsed: AiDeclineReviewBundle = serde_json::from_str(json_object_slice(&response))
        .map_err(|error| format!("AI下降原因结果格式无效：{error}"))?;
    analysis.ai_summary = (!parsed.summary.trim().is_empty()
        && evidence_safe_claim(&parsed.summary))
    .then_some(parsed.summary);
    for review in parsed.events {
        if let Some(event) = analysis
            .decline_events
            .iter_mut()
            .find(|event| event.id == review.id)
        {
            if !review.explanation.trim().is_empty() && evidence_safe_claim(&review.explanation) {
                event.explanation = review.explanation;
            }
            if matches!(review.confidence.as_str(), "高" | "中" | "低") {
                event.confidence = review.confidence;
            }
            let safe_causes = review
                .possible_causes
                .into_iter()
                .filter(|cause| evidence_safe_claim(cause))
                .map(|cause| {
                    if cause.contains("不代表因果") || cause.contains("待核实") {
                        cause
                    } else {
                        format!("{cause}（仅时间相关，不代表因果）")
                    }
                })
                .collect::<Vec<_>>();
            if !safe_causes.is_empty() {
                event.possible_causes = safe_causes;
            }
            if !review.limitations.trim().is_empty() {
                event.limitations = review.limitations;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "gui")]
async fn attach_session_evidence(
    state: &State,
    session_id: i64,
    analysis: &mut CompassCaptureAnalysis,
) -> Result<(), String> {
    let session = state
        .db
        .get_live_dashboard_session(session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "当前直播场次不存在，请刷新后重试".to_string())?;
    let live_ids = state
        .db
        .get_live_ids_bound_to_session(session_id)
        .await
        .map_err(|error| error.to_string())?;

    for live_id in live_ids {
        if let Some(video_id) = live_id
            .strip_prefix("import:")
            .and_then(|value| value.parse::<i64>().ok())
        {
            analysis.source = Some(CompassAnalysisSource {
                kind: "video".to_string(),
                live_id: live_id.clone(),
                platform: "imported".to_string(),
                room_id: String::new(),
                video_id: Some(video_id),
            });
            if let Ok(transcript) =
                crate::handlers::video::load_video_subtitle(state, video_id).await
            {
                if !transcript.trim().is_empty() {
                    enrich_decline_events_with_transcript(
                        analysis,
                        &session.started_at,
                        &transcript,
                    );
                    return Ok(());
                }
            }
            continue;
        }

        let Ok(record) = state.db.get_record_by_live_id(&live_id).await else {
            continue;
        };
        analysis.source = Some(CompassAnalysisSource {
            kind: "archive".to_string(),
            live_id: record.live_id.clone(),
            platform: record.platform.clone(),
            room_id: record.room_id.clone(),
            video_id: None,
        });
        let Ok(platform) = PlatformType::from_str(&record.platform) else {
            continue;
        };
        if let Ok(transcript) = state
            .recorder_manager
            .get_archive_subtitle(platform, &record.room_id, &record.live_id)
            .await
        {
            if !transcript.trim().is_empty() {
                enrich_decline_events_with_transcript(analysis, &session.started_at, &transcript);
                return Ok(());
            }
        }
    }

    if analysis.source.is_some() {
        enrich_decline_events_with_transcript(analysis, &session.started_at, "");
    }
    if !analysis.decline_events.is_empty() {
        analysis.quality.issues.push(
            "已识别下降区间，但当前场次尚未绑定带逐字稿的录播；暂时只能查看曲线证据".to_string(),
        );
    }
    Ok(())
}

#[cfg(feature = "gui")]
fn compass_analysis_storage_key(capture_id: &str, session_id: Option<i64>) -> String {
    session_id
        .map(|session_id| format!("{capture_id}::session:{session_id}"))
        .unwrap_or_else(|| capture_id.to_string())
}

#[cfg(feature = "gui")]
pub(crate) async fn analyze_compass_capture_state(
    state: &State,
    capture_id: String,
    session_id: Option<i64>,
) -> Result<CompassCaptureAnalysis, String> {
    let directory =
        crate::compass_capture::capture_directory_for_id(&state.app_handle, &capture_id)?;
    let session = if let Some(session_id) = session_id {
        state
            .db
            .get_live_dashboard_session(session_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "当前直播场次不存在，请刷新后重试".to_string())?
            .into()
    } else {
        None
    };
    let session_key = session.as_ref().map(
        |session: &crate::database::live_dashboard::LiveDashboardSessionRow| {
            format!(
                "{}|{}|{}",
                session.shop_name, session.started_at, session.ended_at
            )
        },
    );
    let capture_id_for_task = capture_id.clone();
    let mut analysis = tauri::async_runtime::spawn_blocking(move || {
        analyze_capture_directory_for_session(
            &capture_id_for_task,
            &directory,
            session_key.as_deref(),
        )
    })
    .await
    .map_err(|error| format!("分析任务异常结束：{error}"))??;
    if let Some(session_id) = session_id {
        attach_session_evidence(&state, session_id, &mut analysis).await?;
    }
    if let Err(error) = apply_ai_decline_review(&state, &mut analysis).await {
        log::warn!("AI compass review unavailable: {error}");
    }
    analysis.capture_id = compass_analysis_storage_key(&capture_id, session_id);
    state
        .db
        .replace_compass_analysis(&analysis)
        .await
        .map_err(String::from)?;
    Ok(analysis)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn analyze_compass_capture(
    state: crate::state_type!(),
    capture_id: String,
    session_id: Option<i64>,
) -> Result<CompassCaptureAnalysis, String> {
    analyze_compass_capture_state(&state, capture_id, session_id).await
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn get_compass_capture_analysis(
    state: crate::state_type!(),
    capture_id: String,
    session_id: Option<i64>,
) -> Result<Option<CompassCaptureAnalysis>, String> {
    let storage_key = compass_analysis_storage_key(&capture_id, session_id);
    let Some(json) = state
        .db
        .get_compass_analysis_json(&storage_key)
        .await
        .map_err(String::from)?
    else {
        return Ok(None);
    };
    serde_json::from_str(&json)
        .map(Some)
        .map_err(|error| format!("已保存分析格式错误：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_time_series_from_nested_response() {
        let body = serde_json::json!({"data":{"trend":[
            {"time":"10:00","value":10}, {"time":"10:01","value":18}, {"time":"10:02","value":12}
        ]}});
        let mut found = Vec::new();
        collect_point_arrays(&body, &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0][1].value, 18.0);
    }

    #[test]
    fn preserves_missing_curve_points_without_converting_them_to_zero() {
        let body = serde_json::json!({"data":{"trend":[
            {"time":"10:00","value":10}, {"time":"10:01","value":null}, {"time":"10:02","value":12}
        ]}});
        let mut found = Vec::new();
        collect_point_arrays(&body, &mut found);
        assert_eq!(found.len(), 1);
        assert!(found[0][0].valid);
        assert!(!found[0][1].valid);
        assert!(found[0][2].valid);
    }

    #[test]
    fn extracts_native_compass_trends_without_treating_key_points_as_metrics() {
        let body = serde_json::json!({"data":{
            "trends":[
                {"date_time":1786523400_i64,"horizontal":"2026/08/12 16:30","point_name":"stat_cost","display_name":"投放消耗","vertical":2},
                {"date_time":1786523400_i64,"horizontal":"2026/08/12 16:30","point_name":"new_pay_amt","display_name":"成交金额","vertical":0},
                {"date_time":1786523460_i64,"horizontal":"2026/08/12 16:31","point_name":"stat_cost","display_name":"投放消耗","vertical":6},
                {"date_time":1786523460_i64,"horizontal":"2026/08/12 16:31","point_name":"new_pay_amt","display_name":"成交金额","vertical":100}
            ],
            "key_points":[
                {"date_time":1786523400_i64,"display_name":"主播","point_name":"anchor","vertical":1},
                {"date_time":1786523460_i64,"display_name":"讲解","point_name":"product","vertical":1},
                {"date_time":1786523520_i64,"display_name":"投放","point_name":"ad","vertical":1}
            ]
        }});
        let mut found = Vec::new();
        collect_native_trend_series(&body, &mut found);
        assert_eq!(found.len(), 2);
        let payment = found
            .iter()
            .find(|series| series.label == "成交金额")
            .expect("payment series");
        assert_eq!(payment.points.len(), 2);
        assert_eq!(payment.points[1].value, 100.0);
        assert!(!found.iter().any(|series| series.label == "主播"));
    }

    #[test]
    fn converts_second_and_millisecond_epoch_labels_to_china_clock_text() {
        assert_eq!(display_time_label("1786523400"), "08/12 16:30");
        assert_eq!(display_time_label("1786523400000"), "08/12 16:30");
        assert_eq!(sort_value_for_label("1786523400", 0), 1_786_523_400.0);
        assert_eq!(sort_value_for_label("1786523400000", 0), 1_786_523_400.0);
        assert_eq!(display_time_label("16:30"), "16:30");
    }

    #[test]
    fn native_trends_never_expose_raw_epoch_as_axis_label() {
        let body = serde_json::json!({"data":{"trends":[
            {"date_time":1786523400_i64,"point_name":"online","display_name":"在线人数","vertical":12},
            {"date_time":1786523460000_i64,"point_name":"online","display_name":"在线人数","vertical":18}
        ]}});
        let mut found = Vec::new();
        collect_native_trend_series(&body, &mut found);
        assert_eq!(found[0].points[0].time_label, "08/12 16:30");
        assert_eq!(found[0].points[1].time_label, "08/12 16:31");
        assert_eq!(
            found[0].points[1].sort_value - found[0].points[0].sort_value,
            60.0
        );
    }

    #[test]
    #[ignore = "requires COMPASS_REAL_CAPTURE_DIR pointing to a local redacted capture"]
    fn validates_real_capture_data_trends_end_to_end() {
        let directory = std::env::var("COMPASS_REAL_CAPTURE_DIR")
            .expect("COMPASS_REAL_CAPTURE_DIR is required for this manual acceptance test");
        let analysis = analyze_capture_directory_for_session(
            "real-capture-acceptance",
            std::path::Path::new(&directory),
            None,
        )
        .expect("real capture should parse");
        let native_metrics = analysis
            .metrics
            .iter()
            .filter(|metric| metric.source_kind == "compass_native")
            .collect::<Vec<_>>();
        assert!(
            native_metrics.len() >= 2,
            "expected multiple native metrics"
        );
        assert!(native_metrics.iter().all(|metric| metric.points.len() >= 3));
        assert!(native_metrics.iter().all(|metric| {
            metric
                .points
                .windows(2)
                .all(|pair| pair[0].sort_value <= pair[1].sort_value)
        }));
        let labels = native_metrics
            .iter()
            .map(|metric| metric.label.as_str())
            .collect::<Vec<_>>();
        assert!(labels.contains(&"成交金额"));
        assert!(labels.contains(&"投放消耗"));
        assert!(labels.contains(&"在线人数"));
        assert!(!labels.contains(&"主播"));
        assert!(
            !analysis.products.is_empty(),
            "expected official Compass product responses to normalize"
        );
        assert!(
            !analysis.audience.is_empty(),
            "expected official Compass audience responses to normalize"
        );
        assert!(
            !analysis.qianchuan.is_empty(),
            "expected official Compass Qianchuan responses to normalize"
        );
    }

    #[test]
    fn extracts_product_without_order_data_dependency() {
        let body = serde_json::json!({"data":{"product_id":"42","product_name":"佳能相机","explain_count":5,"explain_start_time":"10:20:00","explain_end_time":"10:24:30"}});
        let mut products = HashMap::new();
        collect_products(&body, &mut products);
        assert_eq!(products["42"].product_name, "佳能相机");
        assert_eq!(products["42"].explain_count, 5);
        assert_eq!(products["42"].explain_start_time, "10:20:00");
        assert_eq!(products["42"].explain_end_time, "10:24:30");
    }

    #[test]
    fn normalizes_audience_response_into_exact_dimensions() {
        let body = serde_json::json!({
            "data": {
                "gender": [{"name":"女","ratio":0.68},{"name":"男","ratio":0.32}],
                "age": [{"label":"25-34岁","value":42,"unit":"%"}]
            }
        });
        let mut output = HashMap::new();
        collect_audience_items(
            &body,
            "/compass_api/live/audience_portrait",
            &mut Vec::new(),
            &mut output,
        );
        let mut items = output.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| left.label.cmp(&right.label));
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].dimension, "年龄");
        assert_eq!(items[0].label, "25-34岁");
        assert_eq!(items[0].value, 42.0);
        assert_eq!(items[1].dimension, "性别");
        assert_eq!(items[1].label, "女");
        assert_eq!(items[1].value, 0.68);
    }

    #[test]
    fn normalizes_qianchuan_response_without_guessing_unknown_fields() {
        let body = serde_json::json!({"data":{"ad_spend":24.01,"pay_roi":1.8,"impressions":1200,"mystery_score":999}});
        let mut output = HashMap::new();
        collect_qianchuan_metrics(&body, "/compass_api/live/qianchuan/summary", &mut output);
        assert_eq!(output.len(), 3);
        assert_eq!(output["spend"].label, "投放消耗");
        assert_eq!(output["spend"].value, 24.01);
        assert_eq!(output["roi"].value, 1.8);
        assert!(!output.contains_key("mystery_score"));
    }

    #[test]
    fn normalizes_official_core_data_stat_cost_as_yuan() {
        let body = serde_json::json!({
            "data": {
                "core_data": [{
                    "index_display": "投放消耗(店铺绑定)",
                    "index_name": "stat_cost",
                    "value": { "unit": "price", "value": 15799 }
                }]
            }
        });
        assert_eq!(
            response_section(
                &serde_json::json!({}),
                "/compass_api/shop/live/live_screen/core_data",
                &body
            ),
            "qianchuan"
        );
        let mut output = HashMap::new();
        collect_qianchuan_metrics(
            &body,
            "/compass_api/shop/live/live_screen/core_data",
            &mut output,
        );
        assert_eq!(output["spend"].value, 157.99);
        assert_eq!(output["spend"].unit, "元");
    }

    fn metric(label: &str, values: &[f64]) -> CompassMetricAnalysis {
        summarize_metric(
            label.to_string(),
            String::new(),
            "compass_native".to_string(),
            "/trend".to_string(),
            values
                .iter()
                .enumerate()
                .map(|(index, value)| CompassMetricPoint {
                    time_label: format!("10:{index:02}"),
                    sort_value: (10 * 3600 + index * 60) as f64,
                    value: *value,
                    valid: true,
                })
                .collect(),
        )
    }

    #[test]
    fn detects_sustained_decline_and_correlated_metric() {
        let online = metric(
            "在线人数",
            &[100.0, 102.0, 98.0, 78.0, 72.0, 70.0, 69.0, 95.0],
        );
        let interaction = metric("互动率", &[20.0, 20.0, 21.0, 16.0, 15.0, 14.0, 15.0, 20.0]);
        let events = detect_decline_events(&[online, interaction]);
        let event = events
            .iter()
            .find(|event| event.metric_label == "在线人数")
            .expect("online decline should be detected");
        assert!(event.drop_percent >= 25.0);
        assert!(event
            .correlated_changes
            .iter()
            .any(|change| change.label == "互动率" && change.change_percent < -20.0));
    }

    #[test]
    fn ignores_single_point_noise() {
        let online = metric(
            "在线人数",
            &[100.0, 100.0, 100.0, 60.0, 100.0, 100.0, 100.0],
        );
        assert!(detect_decline_events(&[online]).is_empty());
    }

    #[test]
    fn does_not_infer_causes_without_transcript_evidence() {
        let changes = vec![CompassMetricChange {
            label: "互动率".to_string(),
            change_percent: -30.0,
        }];
        assert!(deterministic_causes(&changes, "").is_empty());
        assert!(evidence_safe_claim("可能存在时间相关，仍待核实"));
        assert!(!evidence_safe_claim("可以确定主播话术直接导致人数下降"));
    }

    #[test]
    fn evidence_window_has_fixed_preroll_and_context() {
        assert_eq!(evidence_window(240.0, 300.0), (210.0, 180.0, 360.0));
        assert_eq!(evidence_window(20.0, 30.0), (0.0, 0.0, 90.0));
    }

    #[test]
    fn extracts_only_transcript_window() {
        let transcript = "1\n00:00:10,000 --> 00:00:12,000\n开场欢迎大家\n\n2\n00:04:00,000 --> 00:04:04,000\n现在把价格和赠品讲清楚\n\n3\n00:09:00,000 --> 00:09:03,000\n下一款商品\n";
        let excerpt = transcript_excerpt(transcript, 180.0, 300.0);
        assert!(excerpt.contains("价格和赠品"));
        assert!(!excerpt.contains("开场欢迎"));
        assert!(!excerpt.contains("下一款商品"));
    }

    #[test]
    fn matches_session_key_by_exact_or_contained_start_minute() {
        assert!(session_key_matches(
            "金典拍拍相机专卖店|2026-08-27T10:39:00+08:00|2026-08-27T16:45:00+08:00",
            "金典拍拍相机专卖店|2026-08-27T10:39:12+08:00|2026-08-27T16:45:11+08:00"
        ));
        assert!(!session_key_matches(
            "科创店|2026-08-27T10:39:00+08:00|2026-08-27T16:45:00+08:00",
            "相机店|2026-08-27T10:39:00+08:00|2026-08-27T16:45:00+08:00"
        ));
        assert!(session_key_matches(
            "金典拍拍相机专卖店|2026-08-29T10:24:15|2026-08-29T17:59:21",
            "金典拍拍相机专卖店|2026-08-29T08:37:00+08:00|2026-08-29T16:00:00+08:00"
        ));
        assert!(!session_key_matches(
            "金典拍拍相机专卖店|2026-08-29T16:01:00|2026-08-29T17:59:21",
            "金典拍拍相机专卖店|2026-08-29T08:37:00+08:00|2026-08-29T16:00:00+08:00"
        ));
    }

    #[test]
    fn matches_cdp_response_without_session_key_by_capture_target() {
        let expected = "金典拍拍相机专卖店|2026-08-12T16:30:00+08:00|2026-08-12T18:47:00+08:00";
        assert!(response_matches_session(
            &serde_json::json!({
                "targetDate": "2026-08-12",
                "targetShopName": "金典拍拍相机专卖店"
            }),
            expected
        ));
        assert!(!response_matches_session(
            &serde_json::json!({
                "targetDate": "2026-08-13",
                "targetShopName": "金典拍拍相机专卖店"
            }),
            expected
        ));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn stores_each_session_analysis_under_a_distinct_key() {
        assert_eq!(
            compass_analysis_storage_key("capture-a", Some(27)),
            "capture-a::session:27"
        );
        assert_eq!(
            compass_analysis_storage_key("capture-a", Some(28)),
            "capture-a::session:28"
        );
        assert_eq!(compass_analysis_storage_key("capture-a", None), "capture-a");
    }
}
