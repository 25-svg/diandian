#[cfg(feature = "gui")]
use crate::state::State;
#[cfg(feature = "gui")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "gui")]
use serde_json::{json, Value};
#[cfg(feature = "gui")]
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(feature = "gui")]
use std::fs;
#[cfg(feature = "gui")]
use std::path::{Path, PathBuf};
#[cfg(feature = "gui")]
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[cfg(feature = "gui")]
const CAPTURE_CHUNK_EVENT: &str = "compass-capture-chunk";
#[cfg(feature = "gui")]
const CAPTURE_CONTROL_EVENT: &str = "compass-capture-control";
#[cfg(feature = "gui")]
const MAX_CAPTURE_PARTS: usize = 512;

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureChunk {
    capture_id: String,
    response_id: String,
    sequence: usize,
    total: usize,
    #[serde(default)]
    meta: Value,
    chunk: String,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureControl {
    capture_id: String,
    status: String,
    #[serde(default)]
    target_date: String,
    #[serde(default)]
    target_shop_name: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    metric_label: String,
    #[serde(default)]
    product_page: usize,
    #[serde(default)]
    section_key: String,
    #[serde(default)]
    section_label: String,
    #[serde(default)]
    section_kind: String,
    #[serde(default)]
    section_status: String,
}

#[cfg(feature = "gui")]
#[derive(Debug)]
struct PartialResponse {
    meta: Value,
    chunks: Vec<Option<String>>,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassCaptureCoverageItem {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub status: String,
    pub response_count: usize,
    #[serde(default)]
    pub attempt_count: usize,
    #[serde(default)]
    pub failure_count: usize,
    pub last_endpoint: Option<String>,
    #[serde(default)]
    pub last_transport: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    pub updated_at: Option<String>,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassDataCatalogEntry {
    pub endpoint: String,
    pub methods: Vec<String>,
    pub domains: Vec<String>,
    pub section_keys: Vec<String>,
    pub response_count: usize,
    #[serde(default)]
    pub transports: Vec<String>,
    #[serde(default)]
    pub content_types: Vec<String>,
    #[serde(default)]
    pub status_codes: Vec<u16>,
    #[serde(default)]
    pub total_body_bytes: usize,
    #[serde(default)]
    pub max_array_items: usize,
    pub field_paths: Vec<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassDataCatalog {
    pub version: usize,
    pub capture_id: String,
    pub generated_at: String,
    pub entries: BTreeMap<String, CompassDataCatalogEntry>,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompassCaptureSummary {
    pub capture_id: String,
    pub target_date: String,
    pub target_shop_name: String,
    pub status: String,
    pub message: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub response_count: usize,
    #[serde(default)]
    pub business_response_count: usize,
    #[serde(default)]
    pub analysis_response_count: usize,
    #[serde(default)]
    pub navigation_response_count: usize,
    #[serde(default)]
    pub configuration_response_count: usize,
    #[serde(default)]
    pub unknown_response_count: usize,
    #[serde(default)]
    pub http_response_count: usize,
    #[serde(default)]
    pub websocket_frame_count: usize,
    #[serde(default)]
    pub parse_failure_count: usize,
    pub endpoint_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub business_endpoint_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub analysis_endpoint_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub metric_labels: Vec<String>,
    #[serde(default)]
    pub has_product_data: bool,
    #[serde(default)]
    pub has_explanation_data: bool,
    #[serde(default)]
    pub active_metric_label: String,
    #[serde(default)]
    pub current_product_page: usize,
    #[serde(default)]
    pub active_section_key: String,
    #[serde(default)]
    pub coverage: BTreeMap<String, CompassCaptureCoverageItem>,
    #[serde(default)]
    pub catalog_endpoint_count: usize,
    #[serde(default)]
    pub catalog_field_count: usize,
    #[serde(default)]
    pub catalog_path: String,
    #[serde(default)]
    pub last_response_at: Option<String>,
    pub session_keys: Vec<String>,
    pub storage_path: String,
}

#[cfg(feature = "gui")]
impl CompassCaptureSummary {
    fn new(capture_id: &str, storage_path: &Path) -> Self {
        Self {
            capture_id: capture_id.to_string(),
            target_date: String::new(),
            target_shop_name: String::new(),
            status: "running".to_string(),
            message: "正在等待罗盘数据".to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            response_count: 0,
            business_response_count: 0,
            analysis_response_count: 0,
            navigation_response_count: 0,
            configuration_response_count: 0,
            unknown_response_count: 0,
            http_response_count: 0,
            websocket_frame_count: 0,
            parse_failure_count: 0,
            endpoint_counts: BTreeMap::new(),
            business_endpoint_counts: BTreeMap::new(),
            analysis_endpoint_counts: BTreeMap::new(),
            metric_labels: Vec::new(),
            has_product_data: false,
            has_explanation_data: false,
            active_metric_label: String::new(),
            current_product_page: 0,
            active_section_key: String::new(),
            coverage: default_capture_coverage(),
            catalog_endpoint_count: 0,
            catalog_field_count: 0,
            catalog_path: storage_path
                .join("data-catalog.json")
                .to_string_lossy()
                .to_string(),
            last_response_at: None,
            session_keys: Vec::new(),
            storage_path: storage_path.to_string_lossy().to_string(),
        }
    }
}

#[cfg(feature = "gui")]
fn coverage_item(key: &str, label: &str, kind: &str) -> CompassCaptureCoverageItem {
    CompassCaptureCoverageItem {
        key: key.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        status: "pending".to_string(),
        response_count: 0,
        attempt_count: 0,
        failure_count: 0,
        last_endpoint: None,
        last_transport: None,
        last_error: None,
        updated_at: None,
    }
}

#[cfg(feature = "gui")]
fn default_capture_coverage() -> BTreeMap<String, CompassCaptureCoverageItem> {
    let mut result = BTreeMap::new();
    let targets = [
        ("module.data", "数据", "module"),
        ("module.product", "商品", "module"),
        ("module.audience", "人群", "module"),
        ("module.qianchuan", "千川", "module"),
        ("tab.trend", "综合趋势", "tab"),
        ("tab.traffic", "流量分析", "tab"),
        ("tab.short_video", "引流短视频", "tab"),
        ("tab.diagnosis", "流量诊断", "tab"),
        ("tab.host", "主播分析", "tab"),
        ("tab.violation", "违规情况", "tab"),
        ("product.list", "商品列表与分页", "dataset"),
        ("product.detail", "有讲解商品详情", "dataset"),
        ("metric.deal_amount", "成交金额", "metric"),
        ("metric.ad_spend", "投放消耗", "metric"),
        ("metric.exposure_view", "曝光-观看率", "metric"),
        ("metric.online", "在线人数", "metric"),
        ("metric.watch_duration", "人均观看时长", "metric"),
        ("metric.interaction", "互动率", "metric"),
        ("metric.follow", "关注率", "metric"),
        ("metric.negative_rate", "负反馈率", "metric"),
        ("metric.negative_count", "负反馈次数", "metric"),
        ("metric.gpm", "千次观看用户支付金额", "metric"),
        ("metric.product_click", "商品点击率", "metric"),
        ("metric.product_conversion", "商品点击-成交率", "metric"),
        ("metric.user_payment", "用户支付金额", "metric"),
    ];
    for (key, label, kind) in targets {
        result.insert(key.to_string(), coverage_item(key, label, kind));
    }
    result
}

#[cfg(feature = "gui")]
fn assemblies() -> &'static Mutex<HashMap<String, PartialResponse>> {
    static ASSEMBLIES: OnceLock<Mutex<HashMap<String, PartialResponse>>> = OnceLock::new();
    ASSEMBLIES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(feature = "gui")]
fn capture_io_lock() -> &'static Mutex<()> {
    static IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    IO_LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(feature = "gui")]
fn active_capture_ids() -> &'static Mutex<HashSet<String>> {
    static ACTIVE_CAPTURE_IDS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ACTIVE_CAPTURE_IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

#[cfg(feature = "gui")]
fn reserve_capture_id(active: &mut HashSet<String>, capture_id: &str) -> Result<(), String> {
    if !active.is_empty() && !active.contains(capture_id) {
        return Err("已有罗盘完整采集任务正在运行，请等待完成或先取消当前任务".to_string());
    }
    active.insert(capture_id.to_string());
    Ok(())
}

#[cfg(feature = "gui")]
fn retain_running_capture_ids(active: &mut HashSet<String>, running: &HashSet<String>) {
    active.retain(|capture_id| running.contains(capture_id));
}

#[cfg(feature = "gui")]
fn safe_identifier(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 100
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err("采集标识无效".to_string());
    }
    Ok(value.to_string())
}

#[cfg(feature = "gui")]
fn capture_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?
        .join("compass-captures");
    fs::create_dir_all(&root).map_err(|error| format!("无法创建罗盘采集目录：{error}"))?;
    Ok(root)
}

#[cfg(feature = "gui")]
fn capture_directory(root: &Path, capture_id: &str) -> Result<PathBuf, String> {
    let capture_id = safe_identifier(capture_id)?;
    let directory = root.join(capture_id);
    fs::create_dir_all(directory.join("responses"))
        .map_err(|error| format!("无法创建罗盘响应目录：{error}"))?;
    Ok(directory)
}

#[cfg(feature = "gui")]
pub(crate) fn capture_directory_for_id(
    app: &tauri::AppHandle,
    capture_id: &str,
) -> Result<PathBuf, String> {
    let root = capture_root(app)?;
    capture_directory(&root, capture_id)
}

#[cfg(feature = "gui")]
pub(crate) fn initialize_compass_capture(
    app: &tauri::AppHandle,
    capture_id: &str,
    target_date: &str,
    target_shop_name: &str,
) -> Result<(), String> {
    let capture_id = safe_identifier(capture_id)?;
    let root = capture_root(app)?;
    {
        let mut active = active_capture_ids()
            .lock()
            .map_err(|_| "罗盘活动采集锁已损坏".to_string())?;
        let running = active
            .iter()
            .filter(|active_id| {
                let directory = root.join(active_id.as_str());
                read_manifest(&directory, active_id).status == "running"
            })
            .cloned()
            .collect::<HashSet<_>>();
        retain_running_capture_ids(&mut active, &running);
        reserve_capture_id(&mut active, &capture_id)?;
    }
    let _guard = capture_io_lock()
        .lock()
        .map_err(|_| "罗盘采集文件锁已损坏".to_string())?;
    let directory = capture_directory(&root, &capture_id)?;
    let mut manifest = read_manifest(&directory, &capture_id);
    manifest.target_date = target_date.to_string();
    manifest.target_shop_name = target_shop_name.to_string();
    manifest.status = "running".to_string();
    manifest.message = "正在打开罗盘，等待经营数据".to_string();
    if let Err(error) = write_manifest(&directory, &manifest) {
        if let Ok(mut active) = active_capture_ids().lock() {
            active.remove(&capture_id);
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(feature = "gui")]
pub(crate) fn fail_compass_capture(
    app: &tauri::AppHandle,
    capture_id: &str,
    message: &str,
) -> Result<(), String> {
    let capture_id = safe_identifier(capture_id)?;
    let _guard = capture_io_lock()
        .lock()
        .map_err(|_| "罗盘采集文件锁已损坏".to_string())?;
    let root = capture_root(app)?;
    let directory = capture_directory(&root, &capture_id)?;
    let mut manifest = read_manifest(&directory, &capture_id);
    manifest.status = "failed".to_string();
    manifest.message = message.to_string();
    manifest.finished_at = Some(chrono::Utc::now().to_rfc3339());
    write_manifest(&directory, &manifest)?;
    active_capture_ids()
        .lock()
        .map_err(|_| "罗盘活动采集锁已损坏".to_string())?
        .remove(&capture_id);
    Ok(())
}

#[cfg(feature = "gui")]
fn manifest_path(directory: &Path) -> PathBuf {
    directory.join("capture.json")
}

#[cfg(feature = "gui")]
fn read_manifest(directory: &Path, capture_id: &str) -> CompassCaptureSummary {
    let mut manifest = fs::read_to_string(manifest_path(directory))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| CompassCaptureSummary::new(capture_id, directory));
    for (key, item) in default_capture_coverage() {
        manifest.coverage.entry(key).or_insert(item);
    }
    if manifest.catalog_path.is_empty() {
        manifest.catalog_path = directory
            .join("data-catalog.json")
            .to_string_lossy()
            .to_string();
    }
    manifest
}

#[cfg(feature = "gui")]
fn write_manifest(directory: &Path, manifest: &CompassCaptureSummary) -> Result<(), String> {
    let output = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("无法编码罗盘采集清单：{error}"))?;
    let target = manifest_path(directory);
    let temporary = directory.join("capture.json.tmp");
    fs::write(&temporary, output).map_err(|error| format!("无法写入采集清单：{error}"))?;
    fs::rename(&temporary, &target).map_err(|error| format!("无法保存采集清单：{error}"))
}

#[cfg(feature = "gui")]
fn recover_orphaned_compass_captures(app: &tauri::AppHandle) -> Result<usize, String> {
    let _guard = capture_io_lock()
        .lock()
        .map_err(|_| "罗盘采集文件锁已损坏".to_string())?;
    let active_ids = active_capture_ids()
        .lock()
        .map_err(|_| "罗盘活动采集锁已损坏".to_string())?
        .clone();
    let root = capture_root(app)?;
    let mut recovered = 0;
    let entries = fs::read_dir(&root).map_err(|error| format!("无法读取罗盘采集目录：{error}"))?;

    for entry in entries.flatten() {
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }
        let Ok(content) = fs::read_to_string(manifest_path(&directory)) else {
            continue;
        };
        let Ok(mut manifest) = serde_json::from_str::<CompassCaptureSummary>(&content) else {
            continue;
        };
        if manifest.status != "running" || active_ids.contains(&manifest.capture_id) {
            continue;
        }

        manifest.status = "failed".to_string();
        manifest.finished_at = Some(chrono::Utc::now().to_rfc3339());
        manifest.message = if manifest.analysis_response_count == 0 {
            "上次采集已随程序退出而停止：未收到曲线或商品分析接口，请重新采集".to_string()
        } else {
            "上次采集已随程序退出而停止：已保留收到的可分析数据".to_string()
        };
        write_manifest(&directory, &manifest)?;
        recovered += 1;
    }

    Ok(recovered)
}

#[cfg(feature = "gui")]
fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
    matches!(
        normalized.as_str(),
        "cookie"
            | "setcookie"
            | "authorization"
            | "accesstoken"
            | "refreshtoken"
            | "mstoken"
            | "verifyfp"
            | "fp"
            | "abogus"
            | "signature"
            | "sign"
            | "mobile"
            | "phone"
            | "phonenumber"
            | "address"
            | "receiver"
            | "receivername"
            | "receiverphone"
            | "idcard"
            | "openid"
    ) || normalized.ends_with("token")
}

#[cfg(feature = "gui")]
fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String("[REDACTED]".to_string());
                } else {
                    redact_json(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json(item);
            }
        }
        _ => {}
    }
}

#[cfg(feature = "gui")]
fn sanitize_url(value: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(value) else {
        return value.split('?').next().unwrap_or(value).to_string();
    };
    let retained = parsed
        .query_pairs()
        .filter(|(key, _)| !is_sensitive_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    parsed.set_query(None);
    if !retained.is_empty() {
        parsed.query_pairs_mut().extend_pairs(retained);
    }
    parsed.set_fragment(None);
    parsed.to_string()
}

#[cfg(feature = "gui")]
fn endpoint_from_meta(meta: &Value) -> String {
    let value = meta.get("url").and_then(Value::as_str).unwrap_or("unknown");
    url::Url::parse(value)
        .ok()
        .map(|url| url.path().to_string())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(feature = "gui")]
fn data_catalog_path(directory: &Path) -> PathBuf {
    directory.join("data-catalog.json")
}

#[cfg(feature = "gui")]
fn read_data_catalog(directory: &Path, capture_id: &str) -> CompassDataCatalog {
    let mut catalog = fs::read_to_string(data_catalog_path(directory))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| CompassDataCatalog {
            version: 2,
            capture_id: capture_id.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            entries: BTreeMap::new(),
        });
    catalog.version = 2;
    catalog
}

#[cfg(feature = "gui")]
fn write_data_catalog(directory: &Path, catalog: &CompassDataCatalog) -> Result<(), String> {
    let output = serde_json::to_vec_pretty(catalog)
        .map_err(|error| format!("无法编码罗盘数据目录：{error}"))?;
    let target = data_catalog_path(directory);
    let temporary = directory.join("data-catalog.json.tmp");
    fs::write(&temporary, output).map_err(|error| format!("无法写入罗盘数据目录：{error}"))?;
    fs::rename(&temporary, &target).map_err(|error| format!("无法保存罗盘数据目录：{error}"))
}

#[cfg(feature = "gui")]
fn collect_field_paths(value: &Value) -> Vec<String> {
    fn visit(
        value: &Value,
        prefix: &str,
        depth: usize,
        output: &mut std::collections::BTreeSet<String>,
    ) {
        if depth > 7 || output.len() >= 2_000 {
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.to_string()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    if matches!(child, Value::Object(_) | Value::Array(_)) {
                        visit(child, &path, depth + 1, output);
                    } else {
                        output.insert(path);
                    }
                }
            }
            Value::Array(items) => {
                let path = if prefix.is_empty() {
                    "[]".to_string()
                } else {
                    format!("{prefix}[]")
                };
                for child in items.iter().take(5) {
                    if matches!(child, Value::Object(_) | Value::Array(_)) {
                        visit(child, &path, depth + 1, output);
                    } else {
                        output.insert(path.clone());
                    }
                }
            }
            _ if !prefix.is_empty() => {
                output.insert(prefix.to_string());
            }
            _ => {}
        }
    }

    let mut result = std::collections::BTreeSet::new();
    visit(value, "", 0, &mut result);
    result.into_iter().collect()
}

#[cfg(feature = "gui")]
fn max_array_items(value: &Value) -> usize {
    match value {
        Value::Array(items) => items
            .iter()
            .map(max_array_items)
            .max()
            .unwrap_or(0)
            .max(items.len()),
        Value::Object(map) => map.values().map(max_array_items).max().unwrap_or(0),
        _ => 0,
    }
}

#[cfg(feature = "gui")]
fn detect_data_domains(endpoint: &str, body: &Value) -> Vec<String> {
    let endpoint = endpoint.to_ascii_lowercase();
    let mut domains = std::collections::BTreeSet::new();
    let rules: [(&str, &[&str]); 9] = [
        ("trend", &["trend", "timeline", "curve", "metric"]),
        ("product", &["product", "goods", "item", "explain"]),
        (
            "traffic",
            &["traffic", "channel", "source", "exposure", "viewer"],
        ),
        (
            "audience",
            &["audience", "portrait", "gender", "province", "city"],
        ),
        (
            "advertising",
            &["qianchuan", "ad_spend", "cost", "spend", "roi"],
        ),
        ("comment", &["comment", "danmu", "chat"]),
        ("host", &["anchor", "host", "主播"]),
        ("event", &["explain", "lottery", "福袋", "场记", "讲解"]),
        ("violation", &["violation", "违规"]),
    ];
    for (domain, hints) in rules {
        if hints.iter().any(|hint| endpoint.contains(hint)) || payload_contains_any_key(body, hints)
        {
            domains.insert(domain.to_string());
        }
    }
    if domains.is_empty() {
        domains.insert("summary".to_string());
    }
    domains.into_iter().collect()
}

#[cfg(feature = "gui")]
fn inferred_module_section_keys(
    endpoint: &str,
    body: &Value,
    class: ResponseClass,
) -> Vec<&'static str> {
    if class != ResponseClass::Analysis {
        return Vec::new();
    }
    let domains = detect_data_domains(endpoint, body);
    let contains = |expected: &str| domains.iter().any(|domain| domain == expected);
    let mut sections = Vec::new();
    if contains("trend") || contains("traffic") {
        sections.push("module.data");
    }
    if contains("product") {
        sections.push("module.product");
    }
    if contains("audience") {
        sections.push("module.audience");
    }
    if contains("advertising") {
        sections.push("module.qianchuan");
    }
    sections
}

#[cfg(feature = "gui")]
fn mark_coverage_response(
    manifest: &mut CompassCaptureSummary,
    section_key: &str,
    endpoint: &str,
    transport: &str,
) {
    if let Some(item) = manifest.coverage.get_mut(section_key) {
        item.response_count += 1;
        item.last_endpoint = Some(endpoint.to_string());
        item.last_transport = Some(transport.to_string());
        item.last_error = None;
        item.updated_at = Some(chrono::Utc::now().to_rfc3339());
        if matches!(
            item.status.as_str(),
            "capturing" | "pending" | "triggered" | "untriggered" | "parse_failed"
        ) {
            item.status = "captured".to_string();
        }
    }
}

#[cfg(feature = "gui")]
fn update_data_catalog(
    directory: &Path,
    manifest: &mut CompassCaptureSummary,
    meta: &Value,
    body: &Value,
    endpoint: &str,
) -> Result<(), String> {
    let mut catalog = read_data_catalog(directory, &manifest.capture_id);
    let now = chrono::Utc::now().to_rfc3339();
    let method = meta
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let section_key = meta
        .get("sectionKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let fields = collect_field_paths(body);
    let domains = detect_data_domains(endpoint, body);
    let transport = meta
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("http")
        .to_string();
    let content_type = meta
        .get("contentType")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let status_code = meta
        .get("status")
        .and_then(Value::as_f64)
        .filter(|value| *value >= 0.0 && *value <= u16::MAX as f64)
        .map(|value| value as u16);
    let body_bytes = serde_json::to_vec(body)
        .map(|value| value.len())
        .unwrap_or(0);
    let observed_array_items = max_array_items(body);
    let key = format!("{method} {endpoint}");
    let entry = catalog
        .entries
        .entry(key)
        .or_insert_with(|| CompassDataCatalogEntry {
            endpoint: endpoint.to_string(),
            methods: Vec::new(),
            domains: Vec::new(),
            section_keys: Vec::new(),
            response_count: 0,
            transports: Vec::new(),
            content_types: Vec::new(),
            status_codes: Vec::new(),
            total_body_bytes: 0,
            max_array_items: 0,
            field_paths: Vec::new(),
            first_seen_at: now.clone(),
            last_seen_at: now.clone(),
        });
    entry.response_count += 1;
    entry.total_body_bytes = entry.total_body_bytes.saturating_add(body_bytes);
    entry.max_array_items = entry.max_array_items.max(observed_array_items);
    entry.last_seen_at = now.clone();
    if !entry.methods.contains(&method) {
        entry.methods.push(method);
        entry.methods.sort();
        if !entry.transports.contains(&transport) {
            entry.transports.push(transport);
            entry.transports.sort();
        }
        if !content_type.is_empty() && !entry.content_types.contains(&content_type) {
            entry.content_types.push(content_type);
            entry.content_types.sort();
        }
        if let Some(status_code) = status_code {
            if !entry.status_codes.contains(&status_code) {
                entry.status_codes.push(status_code);
                entry.status_codes.sort();
            }
        }
    }
    for domain in domains {
        if !entry.domains.contains(&domain) {
            entry.domains.push(domain);
        }
    }
    entry.domains.sort();
    if !section_key.is_empty() && !entry.section_keys.contains(&section_key) {
        entry.section_keys.push(section_key);
        entry.section_keys.sort();
    }
    let mut merged_fields = entry
        .field_paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    merged_fields.extend(fields);
    entry.field_paths = merged_fields.into_iter().take(4_000).collect();
    catalog.generated_at = now;
    manifest.catalog_endpoint_count = catalog.entries.len();
    manifest.catalog_field_count = catalog
        .entries
        .values()
        .flat_map(|item| item.field_paths.iter())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    manifest.catalog_path = data_catalog_path(directory).to_string_lossy().to_string();
    write_data_catalog(directory, &catalog)
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseClass {
    Analysis,
    Navigation,
    Configuration,
    Unknown,
}

#[cfg(feature = "gui")]
fn classify_response(endpoint: &str, body: &Value) -> ResponseClass {
    let endpoint = endpoint.to_ascii_lowercase();
    if endpoint == "unknown" {
        return ResponseClass::Unknown;
    }
    if endpoint.contains("/config_center/")
        || endpoint.contains("/fe_dynamic/page/schema")
        || endpoint.contains("announce")
        || endpoint.contains("common_libra")
    {
        return ResponseClass::Configuration;
    }
    let has_payload = match body {
        Value::Object(map) => {
            map.contains_key("data") || map.contains_key("value") || map.len() > 2
        }
        Value::Array(items) => !items.is_empty(),
        _ => false,
    };
    if has_payload && response_has_analysis_payload(&endpoint, body) {
        ResponseClass::Analysis
    } else if has_payload {
        ResponseClass::Navigation
    } else {
        ResponseClass::Unknown
    }
}

#[cfg(feature = "gui")]
fn response_has_analysis_payload(endpoint: &str, body: &Value) -> bool {
    let endpoint_hint = [
        "trend",
        "timeline",
        "time_line",
        "curve",
        "metric",
        "live_screen",
        "product_list",
        "product_detail",
        "goods_list",
        "explain",
        "audience",
        "portrait",
        "crowd",
        "qianchuan",
        "advertising",
        "oceanengine",
        "ad_spend",
        "ad_cost",
    ]
    .iter()
    .any(|hint| endpoint.contains(hint));
    let has_time_axis = payload_contains_any_key(
        body,
        &[
            "timestamp",
            "stat_time",
            "timepoint",
            "time_point",
            "minute",
        ],
    );
    let has_metric_value = payload_contains_any_key(
        body,
        &["value", "amount", "count", "rate", "ratio", "duration"],
    );
    let has_product = payload_contains_any_key(
        body,
        &["productid", "productname", "goodsid", "goodsname", "itemid"],
    );
    let has_audience = payload_contains_any_key(
        body,
        &["gender", "age", "province", "city", "portrait", "audience"],
    );
    let has_qianchuan = payload_contains_any_key(
        body,
        &[
            "adspend",
            "adcost",
            "qianchuancost",
            "roi",
            "conversioncost",
        ],
    );
    endpoint_hint
        || (has_time_axis && has_metric_value)
        || has_product
        || has_audience
        || has_qianchuan
}

#[cfg(feature = "gui")]
fn payload_contains_any_key(value: &Value, needles: &[&str]) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
            needles.iter().any(|needle| normalized.contains(needle))
                || payload_contains_any_key(child, needles)
        }),
        Value::Array(items) => items
            .iter()
            .any(|item| payload_contains_any_key(item, needles)),
        _ => false,
    }
}

#[cfg(feature = "gui")]
fn sanitize_request_post_data(value: &str) -> String {
    if let Ok(mut json) = serde_json::from_str::<Value>(value) {
        redact_json(&mut json);
        return serde_json::to_string(&json).unwrap_or_else(|_| "[UNAVAILABLE]".to_string());
    }
    let pairs = url::form_urlencoded::parse(value.as_bytes())
        .map(|(key, value)| {
            let value = if is_sensitive_key(&key) {
                "[REDACTED]".to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect::<Vec<_>>();
    if pairs.is_empty() {
        return "[UNPARSED]".to_string();
    }
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

#[cfg(feature = "gui")]
fn sanitize_meta(meta: &mut Value) {
    redact_json(meta);
    if let Some(url) = meta.get_mut("url") {
        if let Some(value) = url.as_str() {
            *url = Value::String(sanitize_url(value));
        }
    }
    if let Some(post_data) = meta.get_mut("requestPostData") {
        if let Some(value) = post_data.as_str() {
            *post_data = Value::String(sanitize_request_post_data(value));
        }
    }
}

#[cfg(feature = "gui")]
fn persist_response(
    app: &tauri::AppHandle,
    capture_id: &str,
    response_id: &str,
    mut meta: Value,
    mut body: Value,
) -> Result<(), String> {
    use tauri::Emitter;

    let capture_id = safe_identifier(capture_id)?;
    let response_id = safe_identifier(response_id)?;
    sanitize_meta(&mut meta);
    redact_json(&mut body);

    let _guard = capture_io_lock()
        .lock()
        .map_err(|_| "罗盘采集文件锁已损坏".to_string())?;
    let root = capture_root(app)?;
    let directory = capture_directory(&root, &capture_id)?;
    let mut manifest = read_manifest(&directory, &capture_id);
    if meta
        .get("sessionKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty()
    {
        if let Some(session_key) = manifest.session_keys.last() {
            meta["sessionKey"] = Value::String(session_key.clone());
        }
    }
    if meta
        .get("metricLabel")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty()
        && !manifest.active_metric_label.is_empty()
    {
        meta["metricLabel"] = Value::String(manifest.active_metric_label.clone());
    }
    if meta.get("productPage").is_none() && manifest.current_product_page > 0 {
        meta["productPage"] = json!(manifest.current_product_page);
    }
    if meta
        .get("sectionKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty()
        && !manifest.active_section_key.is_empty()
    {
        meta["sectionKey"] = Value::String(manifest.active_section_key.clone());
    }

    let endpoint = endpoint_from_meta(&meta);
    let transport = meta
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("http")
        .to_string();
    let class = classify_response(&endpoint, &body);
    let response_path = directory
        .join("responses")
        .join(format!("{response_id}.json"));
    let stored = json!({ "meta": meta, "body": body });
    fs::write(
        &response_path,
        serde_json::to_vec(&stored).map_err(|error| format!("无法编码罗盘响应：{error}"))?,
    )
    .map_err(|error| format!("无法保存罗盘响应：{error}"))?;

    manifest.response_count += 1;
    if transport == "websocket" {
        manifest.websocket_frame_count += 1;
    } else {
        manifest.http_response_count += 1;
    }
    *manifest
        .endpoint_counts
        .entry(endpoint.clone())
        .or_insert(0) += 1;
    match class {
        ResponseClass::Analysis => {
            manifest.business_response_count += 1;
            manifest.analysis_response_count += 1;
            *manifest
                .business_endpoint_counts
                .entry(endpoint.clone())
                .or_insert(0) += 1;
            *manifest
                .analysis_endpoint_counts
                .entry(endpoint.clone())
                .or_insert(0) += 1;
            if let Some(metric_label) = stored["meta"].get("metricLabel").and_then(Value::as_str) {
                if !metric_label.is_empty()
                    && !manifest
                        .metric_labels
                        .iter()
                        .any(|item| item == metric_label)
                {
                    manifest.metric_labels.push(metric_label.to_string());
                }
            }
            manifest.has_product_data |= payload_contains_any_key(
                &stored["body"],
                &["productid", "productname", "goodsid", "itemid"],
            );
            manifest.has_explanation_data |= payload_contains_any_key(
                &stored["body"],
                &["explaincount", "explanation", "explainstart", "讲解"],
            );
        }
        ResponseClass::Navigation => {
            manifest.business_response_count += 1;
            manifest.navigation_response_count += 1;
            *manifest
                .business_endpoint_counts
                .entry(endpoint.clone())
                .or_insert(0) += 1;
        }
        ResponseClass::Configuration => manifest.configuration_response_count += 1,
        ResponseClass::Unknown => manifest.unknown_response_count += 1,
    }
    let explicit_section = stored["meta"]
        .get("sectionKey")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut section_keys = inferred_module_section_keys(&endpoint, &stored["body"], class)
        .into_iter()
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(section_key) = explicit_section {
        section_keys.insert(section_key);
    }
    for section_key in section_keys {
        mark_coverage_response(&mut manifest, &section_key, &endpoint, &transport);
    }
    update_data_catalog(
        &directory,
        &mut manifest,
        &stored["meta"],
        &stored["body"],
        &endpoint,
    )?;
    manifest.last_response_at = Some(chrono::Utc::now().to_rfc3339());
    manifest.message = format!(
        "已保存 {} 条响应，目录已识别 {} 个接口、{} 个字段",
        manifest.response_count, manifest.catalog_endpoint_count, manifest.catalog_field_count
    );
    write_manifest(&directory, &manifest)?;
    let _ = app.emit(
        "compass-capture-progress",
        json!({
            "captureId": capture_id,
            "status": manifest.status,
            "message": manifest.message,
            "responseCount": manifest.response_count,
            "businessResponseCount": manifest.business_response_count,
            "analysisResponseCount": manifest.analysis_response_count,
            "navigationResponseCount": manifest.navigation_response_count,
            "configurationResponseCount": manifest.configuration_response_count,
            "httpResponseCount": manifest.http_response_count,
            "websocketFrameCount": manifest.websocket_frame_count,
            "parseFailureCount": manifest.parse_failure_count,
            "catalogEndpointCount": manifest.catalog_endpoint_count,
            "catalogFieldCount": manifest.catalog_field_count,
            "coverage": manifest.coverage,
            "endpoint": endpoint,
        }),
    );
    Ok(())
}

#[cfg(feature = "gui")]
pub(crate) fn ingest_cdp_response(
    app: &tauri::AppHandle,
    capture_id: &str,
    mut meta: Value,
    body_text: &str,
) -> Result<(), String> {
    if meta.get("transport").is_none() {
        meta["transport"] = Value::String("http".to_string());
    }
    let body = match serde_json::from_str::<Value>(body_text) {
        Ok(body) => body,
        Err(error) => {
            let message = format!("罗盘 CDP 响应不是有效 JSON：{error}");
            let _ = record_cdp_capture_failure(app, capture_id, meta, &message);
            return Err(message);
        }
    };
    let response_id = format!("cdp-{}", uuid::Uuid::new_v4().simple());
    persist_response(app, capture_id, &response_id, meta, body)
}

#[cfg(feature = "gui")]
pub(crate) fn record_cdp_capture_failure(
    app: &tauri::AppHandle,
    capture_id: &str,
    mut meta: Value,
    error: &str,
) -> Result<(), String> {
    use tauri::Emitter;

    let capture_id = safe_identifier(capture_id)?;
    sanitize_meta(&mut meta);
    let _guard = capture_io_lock()
        .lock()
        .map_err(|_| "罗盘采集文件锁已损坏".to_string())?;
    let root = capture_root(app)?;
    let directory = capture_directory(&root, &capture_id)?;
    let mut manifest = read_manifest(&directory, &capture_id);
    let transport = meta
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("http")
        .to_string();
    let associate_active_section = meta
        .get("associateActiveSection")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let section_key = meta
        .get("sectionKey")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            (associate_active_section && !manifest.active_section_key.is_empty())
                .then(|| manifest.active_section_key.clone())
        });
    manifest.parse_failure_count += 1;
    if let Some(section_key) = section_key {
        if let Some(item) = manifest.coverage.get_mut(&section_key) {
            item.failure_count += 1;
            item.last_transport = Some(transport);
            item.last_error = Some(error.chars().take(240).collect());
            item.updated_at = Some(chrono::Utc::now().to_rfc3339());
            if item.response_count == 0 {
                item.status = "parse_failed".to_string();
            }
        }
    }
    manifest.last_response_at = Some(chrono::Utc::now().to_rfc3339());
    manifest.message = format!(
        "已记录 {} 次响应解析失败；原始凭证和失败响应正文未落盘",
        manifest.parse_failure_count
    );
    write_manifest(&directory, &manifest)?;
    let _ = app.emit("compass-capture-progress", &manifest);
    Ok(())
}

#[cfg(feature = "gui")]
fn ingest_chunk(app: &tauri::AppHandle, mut chunk: CaptureChunk) -> Result<(), String> {
    let capture_id = safe_identifier(&chunk.capture_id)?;
    let response_id = safe_identifier(&chunk.response_id)?;
    if chunk.total == 0 || chunk.total > MAX_CAPTURE_PARTS || chunk.sequence >= chunk.total {
        return Err("罗盘响应分块序号无效".to_string());
    }
    sanitize_meta(&mut chunk.meta);

    let assembly_key = format!("{capture_id}:{response_id}");
    let completed = {
        let mut pending = assemblies()
            .lock()
            .map_err(|_| "罗盘采集分块锁已损坏".to_string())?;
        let response = pending
            .entry(assembly_key.clone())
            .or_insert_with(|| PartialResponse {
                meta: chunk.meta.clone(),
                chunks: vec![None; chunk.total],
            });
        if response.chunks.len() != chunk.total {
            pending.remove(&assembly_key);
            return Err("同一罗盘响应的分块总数不一致".to_string());
        }
        if !chunk.meta.is_null() && chunk.meta != json!({}) {
            response.meta = chunk.meta;
        }
        response.chunks[chunk.sequence] = Some(chunk.chunk);
        if response.chunks.iter().all(Option::is_some) {
            pending.remove(&assembly_key)
        } else {
            None
        }
    };

    let Some(response) = completed else {
        return Ok(());
    };
    let body_text = response
        .chunks
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("");
    let body: Value = serde_json::from_str(&body_text)
        .map_err(|error| format!("罗盘响应不是有效 JSON：{error}"))?;
    persist_response(app, &capture_id, &response_id, response.meta, body)
}

#[cfg(feature = "gui")]
fn apply_control(app: &tauri::AppHandle, control: CaptureControl) -> Result<(), String> {
    use tauri::Emitter;

    let capture_id = safe_identifier(&control.capture_id)?;
    let _guard = capture_io_lock()
        .lock()
        .map_err(|_| "罗盘采集文件锁已损坏".to_string())?;
    let root = capture_root(app)?;
    let directory = capture_directory(&root, &capture_id)?;
    let mut manifest = read_manifest(&directory, &capture_id);
    if !control.target_date.is_empty() {
        manifest.target_date = control.target_date;
    }
    if !control.target_shop_name.is_empty() {
        manifest.target_shop_name = control.target_shop_name;
    }
    if !control.session_key.is_empty() && !manifest.session_keys.contains(&control.session_key) {
        manifest.session_keys.push(control.session_key);
    }
    if !control.metric_label.is_empty() {
        manifest.active_metric_label = control.metric_label;
    }
    if control.product_page > 0 {
        manifest.current_product_page = control.product_page;
    }
    if !control.section_key.is_empty() {
        manifest.active_section_key = control.section_key.clone();
        let item = manifest
            .coverage
            .entry(control.section_key.clone())
            .or_insert_with(|| {
                coverage_item(
                    &control.section_key,
                    if control.section_label.is_empty() {
                        &control.section_key
                    } else {
                        &control.section_label
                    },
                    if control.section_kind.is_empty() {
                        "section"
                    } else {
                        &control.section_kind
                    },
                )
            });
        if !control.section_label.is_empty() {
            item.label = control.section_label;
        }
        if !control.section_kind.is_empty() {
            item.kind = control.section_kind;
        }
        if !control.section_status.is_empty() {
            if control.section_status == "capturing" {
                item.attempt_count += 1;
            }
            item.status = if control.section_status == "triggered" {
                if item.response_count > 0 {
                    "captured".to_string()
                } else if item.failure_count > 0 {
                    "parse_failed".to_string()
                } else {
                    "untriggered".to_string()
                }
            } else {
                control.section_status
            };
        }
        item.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }
    manifest.status = control.status;
    if !control.message.is_empty() {
        manifest.message = control.message;
    }
    if manifest.status == "completed" {
        if manifest.analysis_response_count == 0 {
            manifest.status = "failed".to_string();
            manifest.message = "采集未完成：尚未收到曲线或商品分析接口".to_string();
        } else if manifest.coverage.values().any(|item| {
            matches!(
                item.status.as_str(),
                "pending" | "capturing" | "untriggered" | "parse_failed"
            )
        }) {
            let unresolved = manifest
                .coverage
                .values()
                .filter(|item| {
                    matches!(
                        item.status.as_str(),
                        "pending" | "capturing" | "untriggered" | "parse_failed"
                    )
                })
                .count();
            manifest.status = "partial".to_string();
            manifest.message = format!("采集部分完成：仍有 {unresolved} 个页面或指标未确认");
        } else if manifest.metric_labels.len() < 13 {
            manifest.status = "partial".to_string();
            manifest.message = format!(
                "采集部分完成：已识别 {} 项曲线，目标为 13 项",
                manifest.metric_labels.len()
            );
        }
    }
    if matches!(
        manifest.status.as_str(),
        "completed" | "partial" | "failed" | "cancelled"
    ) {
        manifest.finished_at = Some(chrono::Utc::now().to_rfc3339());
        if let Ok(mut active) = active_capture_ids().lock() {
            active.remove(&capture_id);
        }
    }
    write_manifest(&directory, &manifest)?;
    let _ = app.emit("compass-capture-progress", &manifest);
    Ok(())
}

#[cfg(feature = "gui")]
pub fn register_compass_capture_listeners(app: &tauri::AppHandle) {
    use tauri::Listener;

    let chunk_app = app.clone();
    app.listen(CAPTURE_CHUNK_EVENT, move |event| {
        let Ok(chunk) = serde_json::from_str::<CaptureChunk>(event.payload()) else {
            log::warn!("Ignored malformed compass capture chunk");
            return;
        };
        let app = chunk_app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = ingest_chunk(&app, chunk) {
                log::warn!("Unable to store compass capture chunk: {error}");
            }
        });
    });

    let control_app = app.clone();
    app.listen(CAPTURE_CONTROL_EVENT, move |event| {
        let Ok(control) = serde_json::from_str::<CaptureControl>(event.payload()) else {
            log::warn!("Ignored malformed compass capture control event");
            return;
        };
        let app = control_app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = apply_control(&app, control) {
                log::warn!("Unable to update compass capture manifest: {error}");
            }
        });
    });

    let recovery_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match recover_orphaned_compass_captures(&recovery_app) {
            Ok(count) if count > 0 => {
                log::info!("Recovered {count} orphaned compass capture(s)")
            }
            Ok(_) => {}
            Err(error) => log::warn!("Unable to recover orphaned compass captures: {error}"),
        }
    });
}

#[cfg(feature = "gui")]
pub(crate) async fn list_compass_captures_state(
    state: &crate::state::State,
) -> Result<Vec<CompassCaptureSummary>, String> {
    use tauri::Manager;

    let root = capture_root(&state.app_handle)?;
    let capture_window_open = state
        .app_handle
        .get_webview_window("compass-auto-download")
        .is_some();
    let active_ids = active_capture_ids()
        .lock()
        .map_err(|_| "罗盘活动采集锁已损坏".to_string())?
        .clone();
    let mut captures = Vec::new();
    let entries = fs::read_dir(&root).map_err(|error| format!("无法读取罗盘采集目录：{error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(content) = fs::read_to_string(manifest_path(&path)) else {
            continue;
        };
        if let Ok(mut manifest) = serde_json::from_str::<CompassCaptureSummary>(&content) {
            let has_analysis_signals = manifest.analysis_response_count > 0
                || !manifest.metric_labels.is_empty()
                || manifest.has_product_data
                || manifest.has_explanation_data;
            if manifest.navigation_response_count == 0
                && manifest.business_response_count > 0
                && !has_analysis_signals
            {
                manifest.navigation_response_count = manifest.business_response_count;
            }
            let last_activity = manifest
                .last_response_at
                .as_deref()
                .unwrap_or(&manifest.started_at);
            let stale = chrono::DateTime::parse_from_rfc3339(last_activity)
                .ok()
                .map(|value| {
                    chrono::Utc::now().signed_duration_since(value.with_timezone(&chrono::Utc))
                        > chrono::Duration::minutes(10)
                })
                .unwrap_or(false);
            let not_active_in_this_process = !active_ids.contains(&manifest.capture_id);
            if manifest.status == "running"
                && (stale || !capture_window_open || not_active_in_this_process)
            {
                manifest.status = "failed".to_string();
                manifest.finished_at = Some(chrono::Utc::now().to_rfc3339());
                manifest.message = if manifest.analysis_response_count == 0 {
                    "采集已停止：未进入专业大屏或未收到曲线、商品接口，请重新采集".to_string()
                } else {
                    "采集中断：已保留收到的可分析数据，可先生成部分分析".to_string()
                };
                let _ = write_manifest(&path, &manifest);
            } else if !has_analysis_signals && manifest.status != "running" {
                manifest.message =
                    "本次只采集到首页或直播列表接口，未获得曲线和商品数据，请重新采集专业大屏"
                        .to_string();
            }
            captures.push(manifest);
        }
    }
    captures.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(captures)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn list_compass_captures(
    state: crate::state_type!(),
) -> Result<Vec<CompassCaptureSummary>, String> {
    list_compass_captures_state(&state).await
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn get_compass_capture_catalog(
    state: crate::state_type!(),
    capture_id: String,
) -> Result<CompassDataCatalog, String> {
    let capture_id = safe_identifier(&capture_id)?;
    let directory = capture_directory_for_id(&state.app_handle, &capture_id)?;
    Ok(read_data_catalog(&directory, &capture_id))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gui")]
    use super::*;

    #[cfg(feature = "gui")]
    #[test]
    fn sanitizes_sensitive_query_parameters_without_removing_business_identity() {
        let sanitized = sanitize_url(
            "https://compass.jinritemai.com/compass_api/live?room_id=123&msToken=secret&a_bogus=signed&product_id=456",
        );
        assert!(sanitized.contains("room_id=123"));
        assert!(sanitized.contains("product_id=456"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("signed"));
        assert!(!sanitized.contains("msToken"));
        assert!(!sanitized.contains("a_bogus"));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn redacts_credentials_and_pii_but_keeps_product_names() {
        let mut value = json!({
            "access_token": "secret",
            "phone": "13800000000",
            "product_name": "佳能相机",
            "nested": { "authorization": "Bearer secret", "product_id": "42" }
        });
        redact_json(&mut value);
        assert_eq!(value["access_token"], "[REDACTED]");
        assert_eq!(value["phone"], "[REDACTED]");
        assert_eq!(value["nested"]["authorization"], "[REDACTED]");
        assert_eq!(value["product_name"], "佳能相机");
        assert_eq!(value["nested"]["product_id"], "42");
    }

    #[cfg(feature = "gui")]
    #[test]
    fn rejects_path_traversal_capture_ids() {
        assert!(safe_identifier("capture-20260827").is_ok());
        assert!(safe_identifier("../escape").is_err());
        assert!(safe_identifier("capture/escape").is_err());
    }

    #[cfg(feature = "gui")]
    #[test]
    fn allows_only_one_active_full_capture() {
        let mut active = HashSet::new();
        reserve_capture_id(&mut active, "capture-first").unwrap();
        assert!(reserve_capture_id(&mut active, "capture-first").is_ok());
        let error = reserve_capture_id(&mut active, "capture-second").unwrap_err();
        assert!(error.contains("已有罗盘完整采集任务正在运行"));
        assert_eq!(active.len(), 1);
    }

    #[cfg(feature = "gui")]
    #[test]
    fn removes_stale_active_capture_ids_before_reserving() {
        let mut active =
            HashSet::from(["capture-running".to_string(), "capture-failed".to_string()]);
        let running = HashSet::from(["capture-running".to_string()]);
        retain_running_capture_ids(&mut active, &running);
        assert_eq!(active, running);
    }

    #[cfg(feature = "gui")]
    #[test]
    fn separates_configuration_from_business_responses() {
        assert_eq!(
            classify_response(
                "/compass_api/config_center/compass_announce",
                &json!({"data": {"message": "notice"}}),
            ),
            ResponseClass::Configuration
        );
        assert_eq!(
            classify_response(
                "/compass_api/live/trend",
                &json!({"data": [{"time": "10:00", "value": 12}]}),
            ),
            ResponseClass::Analysis
        );
        assert_eq!(
            classify_response(
                "/compass_api/shop/live/live_overview/live_room_detail_v2",
                &json!({"data": {"room_id": "42", "status": 2}}),
            ),
            ResponseClass::Navigation
        );
        assert_eq!(
            classify_response(
                "/compass_api/live/audience_portrait",
                &json!({"data": {"gender": [{"name": "女", "ratio": 0.68}]}}),
            ),
            ResponseClass::Analysis
        );
        assert_eq!(
            classify_response(
                "/compass_api/live/qianchuan/summary",
                &json!({"data": {"ad_spend": 128.5, "roi": 4.2}}),
            ),
            ResponseClass::Analysis
        );
    }

    #[cfg(feature = "gui")]
    #[test]
    fn field_catalog_flattens_nested_objects_and_arrays() {
        let fields = collect_field_paths(&json!({
            "data": {
                "points": [
                    { "timestamp": 1, "value": 12.5 },
                    { "timestamp": 2, "value": 8.0, "product": { "id": "42" } }
                ]
            }
        }));
        assert!(fields.contains(&"data.points[].timestamp".to_string()));
        assert!(fields.contains(&"data.points[].value".to_string()));
        assert!(fields.contains(&"data.points[].product.id".to_string()));
        assert_eq!(
            max_array_items(&json!({"data": {"points": [1, 2, 3], "groups": [[1, 2, 3, 4]]}})),
            4
        );
    }

    #[cfg(feature = "gui")]
    #[test]
    fn coverage_items_remain_backward_compatible_and_track_real_attempts() {
        let item: CompassCaptureCoverageItem = serde_json::from_value(json!({
            "key": "metric.online",
            "label": "在线人数",
            "kind": "metric",
            "status": "pending",
            "responseCount": 0,
            "lastEndpoint": null,
            "updatedAt": null
        }))
        .unwrap();
        assert_eq!(item.attempt_count, 0);
        assert_eq!(item.failure_count, 0);
        assert!(item.last_transport.is_none());
        assert!(item.last_error.is_none());
    }

    #[cfg(feature = "gui")]
    #[test]
    fn detects_ai_query_domains_from_endpoint_and_payload() {
        let domains = detect_data_domains(
            "/compass_api/live/audience_portrait",
            &json!({ "data": { "gender": [], "province": [] } }),
        );
        assert!(domains.contains(&"audience".to_string()));
        let domains = detect_data_domains(
            "/compass_api/live/trend",
            &json!({ "data": [{ "timestamp": 1, "value": 3 }] }),
        );
        assert!(domains.contains(&"trend".to_string()));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn infers_module_coverage_from_cdp_business_responses() {
        assert_eq!(
            inferred_module_section_keys(
                "/compass_api/content_live/shop/live_screen/blend_trend_v3",
                &json!({ "data": { "trends": [{ "timestamp": 1, "value": 3 }] } }),
                ResponseClass::Analysis,
            ),
            vec!["module.data"]
        );
        assert_eq!(
            inferred_module_section_keys(
                "/compass_api/shop/live/live_screen/product",
                &json!({ "data": { "product_list": [{ "product_id": "42" }] } }),
                ResponseClass::Analysis,
            ),
            vec!["module.product"]
        );
        assert_eq!(
            inferred_module_section_keys(
                "/compass_api/content_live/shop/live_screen/portrait_info",
                &json!({ "data": { "gender": [{ "name": "女" }] } }),
                ResponseClass::Analysis,
            ),
            vec!["module.audience"]
        );
        assert_eq!(
            inferred_module_section_keys(
                "/compass_api/content_live/shop/live_room_detail/roi_info",
                &json!({ "data": { "ad_spend": 128.5, "roi": 4.2 } }),
                ResponseClass::Analysis,
            ),
            vec!["module.qianchuan"]
        );
        assert!(inferred_module_section_keys(
            "/compass_api/config_center/compass_announce",
            &json!({ "data": { "product_id": "42" } }),
            ResponseClass::Configuration,
        )
        .is_empty());
    }

    #[cfg(feature = "gui")]
    #[test]
    fn initializes_all_read_only_capture_targets() {
        let coverage = default_capture_coverage();
        assert_eq!(coverage.get("module.data").unwrap().status, "pending");
        assert_eq!(coverage.get("module.qianchuan").unwrap().label, "千川");
        assert_eq!(coverage.get("tab.violation").unwrap().kind, "tab");
        assert_eq!(coverage.get("metric.user_payment").unwrap().kind, "metric");
        assert!(!coverage.contains_key("action.create_goal"));
    }
}
