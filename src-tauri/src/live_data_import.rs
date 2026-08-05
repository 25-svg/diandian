use calamine::{open_workbook_auto, Data, Range, Reader, Sheets};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use thiserror::Error;

const REQUIRED_SHEETS: [&str; 6] = [
    "基本信息",
    "整体看板",
    "流量分析-流量转化",
    "流量分析-渠道分析",
    "流量分析-短视频引流",
    "商品分析-商品明细",
];

#[derive(Debug, Error)]
pub enum LiveDashboardImportError {
    #[error("无法读取 XLSX：{0}")]
    Workbook(#[from] calamine::Error),
    #[error("官方整场数据下载缺少工作表：{0}")]
    MissingSheet(String),
    #[error("官方整场数据下载格式无效：{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveSessionImport {
    pub account_key: String,
    pub shop_name: String,
    pub started_at: String,
    pub payment_amount_fen: i64,
    pub per_thousand_payment_amount_fen: Option<i64>,
    pub viewer_count: Option<i64>,
    pub average_online: Option<i64>,
    pub average_watch_seconds: Option<i64>,
    pub viewer_conversion_rate: Option<f64>,
    pub deal_buyer_count: Option<i64>,
    pub deal_item_count: Option<i64>,
    pub product_click_conversion_rate: Option<f64>,
    pub exposure_viewer_rate: Option<f64>,
    pub qianchuan_spend_fen: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveChannelImport {
    pub name: String,
    pub viewer_count: Option<i64>,
    pub payment_amount_fen: Option<i64>,
    pub order_count: Option<i64>,
    pub qianchuan_spend_fen: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveShortVideoImport {
    pub title: String,
    pub published_at: String,
    pub exposure_count: Option<i64>,
    pub referral_count: Option<i64>,
    pub click_rate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveProductImport {
    pub product_id: String,
    pub name: String,
    pub payment_amount_fen: Option<i64>,
    pub sold_count: Option<i64>,
    pub buyer_count: Option<i64>,
    pub exposure_count: Option<i64>,
    pub click_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveDashboardImport {
    pub session: LiveSessionImport,
    pub channels: Vec<LiveChannelImport>,
    pub short_videos: Vec<LiveShortVideoImport>,
    pub products: Vec<LiveProductImport>,
}

pub fn parse_live_dashboard_xlsx(
    path: &Path,
) -> Result<LiveDashboardImport, LiveDashboardImportError> {
    let mut workbook = open_workbook_auto(path)?;
    for name in REQUIRED_SHEETS {
        if !workbook.sheet_names().iter().any(|sheet| sheet == name) {
            return Err(LiveDashboardImportError::MissingSheet(name.to_string()));
        }
    }

    let basic = horizontal_values(&worksheet(&mut workbook, "基本信息")?)?;
    let board = horizontal_values(&worksheet(&mut workbook, "整体看板")?)?;
    let conversion = vertical_values(&worksheet(&mut workbook, "流量分析-流量转化")?);
    let mut session = parse_live_session(&basic, &board, &conversion)
        .map_err(LiveDashboardImportError::Invalid)?;
    let channels = parse_channels(&worksheet(&mut workbook, "流量分析-渠道分析")?);
    let short_videos = parse_short_videos(&worksheet(&mut workbook, "流量分析-短视频引流")?);
    let products = parse_products(&worksheet(&mut workbook, "商品分析-商品明细")?);
    let product_item_counts = products
        .iter()
        .filter_map(|product| product.sold_count)
        .collect::<Vec<_>>();
    session.deal_item_count =
        (!product_item_counts.is_empty()).then(|| product_item_counts.into_iter().sum());
    session.qianchuan_spend_fen = channels
        .iter()
        .find(|channel| channel.name == "整体")
        .and_then(|channel| channel.qianchuan_spend_fen);

    Ok(LiveDashboardImport {
        session,
        channels,
        short_videos,
        products,
    })
}

fn worksheet(
    workbook: &mut Sheets<BufReader<File>>,
    name: &str,
) -> Result<Range<Data>, LiveDashboardImportError> {
    workbook
        .worksheet_range(name)
        .map_err(|_| LiveDashboardImportError::MissingSheet(name.to_string()))
}

fn horizontal_values(
    range: &Range<Data>,
) -> Result<HashMap<String, String>, LiveDashboardImportError> {
    let mut rows = range.rows();
    let headers = rows
        .next()
        .ok_or_else(|| LiveDashboardImportError::Invalid("缺少表头".to_string()))?;
    let values = rows
        .next()
        .ok_or_else(|| LiveDashboardImportError::Invalid("缺少数据行".to_string()))?;
    Ok(headers
        .iter()
        .zip(values.iter())
        .map(|(header, value)| (cell_text(header), cell_text(value)))
        .filter(|(header, _)| !header.is_empty())
        .collect())
}

fn vertical_values(range: &Range<Data>) -> HashMap<String, String> {
    range
        .rows()
        .filter_map(|row| {
            let key = row.first().map(cell_text)?;
            let value = row.get(1).map(cell_text)?;
            (!key.is_empty()).then_some((key, value))
        })
        .collect()
}

fn parse_channels(range: &Range<Data>) -> Vec<LiveChannelImport> {
    tabular_rows(range)
        .into_iter()
        .filter_map(|row| {
            let name = row.get("渠道名称")?.trim().to_string();
            (!name.is_empty()).then(|| LiveChannelImport {
                name,
                viewer_count: row.get("观看人数").and_then(|value| parse_integer(value)),
                payment_amount_fen: row
                    .get("用户支付金额")
                    .and_then(|value| parse_amount_to_fen(value)),
                order_count: row.get("成交订单数").and_then(|value| parse_integer(value)),
                qianchuan_spend_fen: row
                    .get("千川消耗")
                    .and_then(|value| parse_amount_to_fen(value)),
            })
        })
        .collect()
}

fn parse_short_videos(range: &Range<Data>) -> Vec<LiveShortVideoImport> {
    tabular_rows(range)
        .into_iter()
        .filter_map(|row| {
            let title = row.get("短视频名称")?.trim().to_string();
            (!title.is_empty()).then(|| LiveShortVideoImport {
                title,
                published_at: row.get("投稿时间").cloned().unwrap_or_default(),
                exposure_count: row
                    .get("短视频直播入口曝光次数")
                    .and_then(|value| parse_integer(value)),
                referral_count: row
                    .get("短视频引流直播间次数")
                    .and_then(|value| parse_integer(value)),
                click_rate: row
                    .get("短视频直播入口点击率(次数)")
                    .and_then(|value| parse_ratio(value)),
            })
        })
        .collect()
}

fn parse_products(range: &Range<Data>) -> Vec<LiveProductImport> {
    tabular_rows(range)
        .into_iter()
        .filter_map(|row| {
            let product_id = row.get("商品ID")?.trim().to_string();
            if product_id.is_empty() || product_id.starts_with('-') || product_id == "商品ID" {
                return None;
            }
            Some(LiveProductImport {
                product_id,
                name: row.get("商品名称").cloned().unwrap_or_default(),
                payment_amount_fen: row
                    .get("用户支付金额")
                    .and_then(|value| parse_amount_to_fen(value)),
                sold_count: row.get("成交件数").and_then(|value| parse_integer(value)),
                buyer_count: row.get("成交人数").and_then(|value| parse_integer(value)),
                exposure_count: row
                    .get("商品曝光人数")
                    .and_then(|value| parse_integer(value)),
                click_count: row
                    .get("商品点击人数")
                    .and_then(|value| parse_integer(value)),
            })
        })
        .collect()
}

fn tabular_rows(range: &Range<Data>) -> Vec<HashMap<String, String>> {
    let mut rows = range.rows();
    let headers = match rows.next() {
        Some(headers) => headers.iter().map(cell_text).collect::<Vec<_>>(),
        None => return Vec::new(),
    };
    rows.map(|row| {
        headers
            .iter()
            .zip(row.iter())
            .map(|(header, value)| (header.clone(), cell_text(value)))
            .collect()
    })
    .collect()
}

fn cell_text(value: &Data) -> String {
    value.to_string().trim().to_string()
}

fn parse_live_session(
    basic: &HashMap<String, String>,
    board: &HashMap<String, String>,
    conversion: &HashMap<String, String>,
) -> Result<LiveSessionImport, String> {
    let started_at = required_field(basic, "直播时间")?
        .split('-')
        .next()
        .ok_or_else(|| "官方导出直播时间格式无效".to_string())?
        .trim();
    let started_at = chrono::NaiveDateTime::parse_from_str(started_at, "%Y/%m/%d %H:%M:%S")
        .map_err(|_| "官方导出直播时间格式无效".to_string())?
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let payment_amount_fen = parse_amount_to_fen(required_field(basic, "直播间用户支付金额")?)
        .ok_or_else(|| "官方导出支付金额格式无效".to_string())?;

    Ok(LiveSessionImport {
        account_key: required_field(basic, "抖音号or火山号")?.trim().to_string(),
        shop_name: required_field(basic, "达人昵称")?.trim().to_string(),
        started_at,
        payment_amount_fen,
        per_thousand_payment_amount_fen: basic
            .get("千次观看用户支付金额")
            .and_then(|value| parse_amount_to_fen(value)),
        viewer_count: board
            .get("直播间观看人数")
            .and_then(|value| parse_integer(value)),
        average_online: board
            .get("平均在线人数")
            .and_then(|value| parse_integer(value)),
        average_watch_seconds: board
            .get("人均观看时长")
            .and_then(|value| parse_duration_seconds(value)),
        viewer_conversion_rate: board
            .get("直播间观看-成交率(人数)")
            .and_then(|value| parse_ratio(value)),
        deal_buyer_count: conversion
            .get("成交人数")
            .and_then(|value| parse_integer(value)),
        deal_item_count: None,
        product_click_conversion_rate: board
            .get("直播间商品点击-成交率(人数)")
            .and_then(|value| parse_ratio(value)),
        exposure_viewer_rate: board
            .get("直播间曝光-观看率(人数)")
            .and_then(|value| parse_ratio(value)),
        qianchuan_spend_fen: None,
    })
}

fn required_field<'a>(fields: &'a HashMap<String, String>, name: &str) -> Result<&'a str, String> {
    fields
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("官方导出缺少字段：{name}"))
}

fn parse_integer(value: &str) -> Option<i64> {
    value.trim().replace(',', "").parse().ok()
}

fn parse_amount_to_fen(value: &str) -> Option<i64> {
    let normalized = value.trim().trim_start_matches('¥').replace(',', "");
    if normalized.is_empty() || normalized == "—" {
        return None;
    }

    let (whole, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    let whole = whole.parse::<i64>().ok()?;
    let mut fraction = fraction.chars().take(2).collect::<String>();
    while fraction.len() < 2 {
        fraction.push('0');
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().ok()?
    };
    whole.checked_mul(100)?.checked_add(fraction)
}

fn parse_ratio(value: &str) -> Option<f64> {
    let normalized = value.trim().strip_suffix('%')?;
    normalized.parse::<f64>().ok().map(|ratio| ratio / 100.0)
}

fn parse_duration_seconds(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let minutes = value
        .split_once('分')
        .and_then(|(minutes, _)| minutes.parse::<i64>().ok())
        .unwrap_or(0);
    let seconds = value
        .split_once('分')
        .map(|(_, seconds)| seconds)
        .unwrap_or(value)
        .trim_end_matches('秒')
        .parse::<i64>()
        .unwrap_or(0);
    let total = minutes.checked_mul(60)?.checked_add(seconds)?;
    (total > 0).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parses_currency_percentage_and_duration_from_official_cells() {
        assert_eq!(parse_amount_to_fen("¥236,552"), Some(23_655_200));
        assert!((parse_ratio("0.68%").unwrap() - 0.0068).abs() < f64::EPSILON);
        assert_eq!(parse_duration_seconds("1分5秒"), Some(65));
    }

    #[test]
    fn rejects_non_numeric_metric_cells() {
        assert_eq!(parse_amount_to_fen("—"), None);
        assert_eq!(parse_ratio("官方导出未提供"), None);
        assert_eq!(parse_duration_seconds(""), None);
    }

    #[test]
    fn maps_official_dashboard_fields_to_a_live_session() {
        let basic = HashMap::from([
            ("达人昵称".to_string(), "金典拍拍相机专卖店".to_string()),
            ("抖音号or火山号".to_string(), "20296833869".to_string()),
            (
                "直播时间".to_string(),
                "2026/07/28 08:15:49-2026/07/28 15:44:39".to_string(),
            ),
            ("直播间用户支付金额".to_string(), "¥236,552".to_string()),
            ("千次观看用户支付金额".to_string(), "¥26,439.25".to_string()),
        ]);
        let board = HashMap::from([
            ("直播间观看人数".to_string(), "6782".to_string()),
            ("平均在线人数".to_string(), "22".to_string()),
            ("人均观看时长".to_string(), "1分5秒".to_string()),
            ("直播间观看-成交率(人数)".to_string(), "0.68%".to_string()),
        ]);

        let session = parse_live_session(&basic, &board, &HashMap::new()).unwrap();

        assert_eq!(session.account_key, "20296833869");
        assert_eq!(session.shop_name, "金典拍拍相机专卖店");
        assert_eq!(session.started_at, "2026-07-28T08:15:49");
        assert_eq!(session.payment_amount_fen, 23_655_200);
        assert_eq!(session.per_thousand_payment_amount_fen, Some(2_643_925));
        assert_eq!(session.viewer_count, Some(6782));
        assert_eq!(session.average_online, Some(22));
        assert_eq!(session.average_watch_seconds, Some(65));
    }

    #[test]
    fn parses_an_official_workbook_with_all_required_sheets() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/live-dashboard/valid-live-dashboard.xlsx"
        ));

        let imported = parse_live_dashboard_xlsx(path).unwrap();

        assert_eq!(imported.session.account_key, "20296833869");
        assert_eq!(imported.session.payment_amount_fen, 23_655_200);
    }

    #[test]
    fn parses_official_kpi_alignment_values_from_the_fixture() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/live-dashboard/valid-live-dashboard.xlsx"
        ));

        let imported = parse_live_dashboard_xlsx(path).unwrap();

        assert_eq!(imported.session.deal_buyer_count, Some(46));
        assert_eq!(imported.session.deal_item_count, Some(51));
        assert_eq!(imported.session.product_click_conversion_rate, Some(0.0308));
        assert_eq!(imported.session.exposure_viewer_rate, Some(0.1012));
        assert_eq!(imported.session.qianchuan_spend_fen, Some(220_151));
    }
}
