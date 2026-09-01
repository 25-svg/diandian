use chrono::{DateTime, NaiveDateTime};
use regex::Regex;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::state::State;
use crate::state_type;
#[cfg(feature = "gui")]
use tauri::State as TauriState;

static WATCH_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdmNamingPreparation {
    pub base_name: String,
    pub suggested_file_name: String,
    pub watcher_started: bool,
}

fn format_started_at(value: &str) -> Result<String, String> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.format("%Y-%m-%d_%H-%M-%S").to_string());
    }
    for pattern in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(value) = NaiveDateTime::parse_from_str(value, pattern) {
            return Ok(value.format("%Y-%m-%d_%H-%M-%S").to_string());
        }
    }
    Err("直播开始时间格式无效".to_string())
}

pub fn build_idm_video_base_name(shop_name: &str, started_at: &str) -> Result<String, String> {
    // Imported dashboard rows can carry the live-session title after the shop name
    // (for example `金典拍拍相机专卖店微单相机场`).  IDM naming only needs the
    // canonical shop identity, while Compass navigation deliberately keeps its
    // stricter exact-match validation.
    let canonical_shop = ["金典拍拍科创专卖店", "金典拍拍相机专卖店"]
        .into_iter()
        .find(|candidate| shop_name.contains(candidate))
        .ok_or_else(|| "下载店铺无效，请选择科创店或相机店".to_string())?;
    let shop_name = crate::compass_auto_download::validate_compass_shop(canonical_shop)?;
    let timestamp = format_started_at(started_at)?;
    Ok(format!("{shop_name}_{timestamp}"))
}

fn is_timestamp_stem(value: &str) -> bool {
    value.len() == 19
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'_')
}

fn is_canonical_idm_video_stem(stem: &str) -> bool {
    ["金典拍拍科创专卖店", "金典拍拍相机专卖店"]
        .into_iter()
        .any(|shop| {
            let Some(rest) = stem
                .strip_prefix(shop)
                .and_then(|value| value.strip_prefix('_'))
            else {
                return false;
            };
            if is_timestamp_stem(rest) {
                return true;
            }
            rest.find('_')
                .and_then(|index| rest.get(index + 1..))
                .is_some_and(is_timestamp_stem)
        })
}

const COMPASS_SHOPS: [&str; 2] = ["金典拍拍科创专卖店", "金典拍拍相机专卖店"];

pub fn parse_compass_page_base_name(text: &str) -> Option<String> {
    let timestamp = parse_compass_start_timestamp(text)?;
    let shop = pick_shop_near_start_time(text)?;
    match parse_compass_anchor_name(text, shop) {
        Some(anchor) => Some(format!("{shop}_{anchor}_{timestamp}")),
        None => Some(format!("{shop}_{timestamp}")),
    }
}

fn pick_shop_near_start_time(text: &str) -> Option<&'static str> {
    let time_pos = text.find("开播时间")?;
    let mut best: Option<(&'static str, usize)> = None;
    for shop in COMPASS_SHOPS {
        let mut from = 0;
        while let Some(relative) = text[from..].find(shop) {
            let pos = from + relative;
            let distance = pos.abs_diff(time_pos);
            if best.map_or(true, |(_, best_distance)| distance < best_distance) {
                best = Some((shop, distance));
            }
            from = pos + shop.len();
        }
    }
    best.map(|(shop, _)| shop)
}

fn parse_compass_start_timestamp(text: &str) -> Option<String> {
    let labeled = Regex::new(
        r"开播时间[:：]?\s*(20\d{2})[/-](\d{1,2})[/-](\d{1,2})\s+(\d{1,2}):(\d{2})(?:[:：](\d{1,2}))?",
    )
    .ok()?;
    let captures = labeled.captures(text)?;
    let year = captures.get(1)?.as_str();
    let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let day = captures.get(3)?.as_str().parse::<u32>().ok()?;
    let hour = captures.get(4)?.as_str().parse::<u32>().ok()?;
    let minute = captures.get(5)?.as_str().parse::<u32>().ok()?;
    let second = captures
        .get(6)
        .and_then(|value| value.as_str().parse::<u32>().ok())
        .unwrap_or(0);
    Some(format!(
        "{year}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}"
    ))
}

fn parse_compass_anchor_name(text: &str, shop: &str) -> Option<String> {
    let pattern = Regex::new(r"主播[:：]\s*([^\s，,。/|]{1,16})").ok()?;
    let name = pattern.captures(text)?.get(1)?.as_str().trim();
    if name.is_empty() || name.contains("分析") || shop.contains(name) {
        return None;
    }
    let sanitized = name
        .chars()
        .filter(|character| !r#"\/:*?"<>|"#.contains(*character))
        .collect::<String>();
    (!sanitized.is_empty()).then_some(sanitized)
}

fn is_idm_video_file(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if name.starts_with("~$") || name.contains(".td") || name.ends_with(".tmp") {
        return false;
    }
    matches!(
        path.extension().and_then(|value| value.to_str()).map(|value| value.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "ts" | "mp4" | "flv" | "mkv")
    )
}

#[cfg(target_os = "windows")]
mod windows_idm {
    use std::path::PathBuf;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible,
        SendMessageW, WM_GETOBJECT, WM_SETTEXT,
    };

    fn window_text(hwnd: HWND) -> String {
        let mut buffer = vec![0u16; 4096];
        let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        String::from_utf16_lossy(&buffer[..length.max(0) as usize])
    }

    fn class_name(hwnd: HWND) -> String {
        let mut buffer = vec![0u16; 256];
        let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
        String::from_utf16_lossy(&buffer[..length.max(0) as usize])
    }

    unsafe extern "system" fn collect_top_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = &mut *(lparam.0 as *mut Vec<HWND>);
        if IsWindowVisible(hwnd).as_bool() && looks_like_idm_dialog(hwnd) {
            windows.push(hwnd);
        }
        BOOL(1)
    }

    fn is_edit_like_class(class: &str) -> bool {
        let lowered = class.to_ascii_lowercase();
        lowered.contains("edit") || lowered.contains("combo")
    }

    unsafe extern "system" fn collect_edit_controls(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let controls = &mut *(lparam.0 as *mut Vec<HWND>);
        let class = class_name(hwnd);
        let text = window_text(hwnd);
        if is_edit_like_class(&class) || looks_like_windows_file_path(&text) {
            controls.push(hwnd);
        }
        BOOL(1)
    }

    unsafe extern "system" fn collect_visible_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = &mut *(lparam.0 as *mut Vec<HWND>);
        if IsWindowVisible(hwnd).as_bool() {
            windows.push(hwnd);
        }
        BOOL(1)
    }

    unsafe extern "system" fn collect_all_child_text(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let texts = &mut *(lparam.0 as *mut Vec<String>);
        let text = window_text(hwnd);
        if !text.trim().is_empty() {
            texts.push(text);
        }
        BOOL(1)
    }

    fn looks_like_compass_window(hwnd: HWND) -> bool {
        let title = window_text(hwnd);
        if title.contains("典典直播切片") || title.contains("bili-shadowreplay") {
            return false;
        }
        title.contains("罗盘")
            || title.contains("直播大屏")
            || title.contains("抖音电商")
            || title.contains("巨量引擎")
            || title.contains("BSR_COMPASS")
            || title.to_ascii_lowercase().contains("compass.jinritemai")
    }

    fn looks_like_idm_dialog(hwnd: HWND) -> bool {
        let blob = collect_win32_texts(hwnd).join("\n");
        let lowered = blob.to_ascii_lowercase();
        if blob.contains("下载文件设置")
            || blob.contains("下载文件信息")
            || blob.contains("另存为")
            || (blob.contains("开始下载") && blob.contains("前往下载"))
            || lowered.contains("download file info")
            || lowered.contains("internet download manager")
            || blob.contains("IDM")
        {
            return true;
        }
        let title = window_text(hwnd);
        let mut edits = Vec::<HWND>::new();
        unsafe {
            let _ = EnumChildWindows(
                Some(hwnd),
                Some(collect_edit_controls),
                LPARAM((&mut edits as *mut Vec<HWND>) as isize),
            );
        }
        edits
            .into_iter()
            .any(|control| looks_like_windows_file_path(&window_text(control)))
            && (title.contains("下载") || lowered.contains("download") || title.contains("保存"))
    }

    fn collect_win32_texts(hwnd: HWND) -> Vec<String> {
        let mut texts = vec![window_text(hwnd)];
        unsafe {
            let _ = EnumChildWindows(
                Some(hwnd),
                Some(collect_all_child_text),
                LPARAM((&mut texts as *mut Vec<String>) as isize),
            );
        }
        texts
    }

    fn collect_uia_texts(hwnd: HWND) -> Vec<String> {
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        };
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
            TreeScope_Descendants, UIA_TextPatternId, UIA_ValuePatternId,
        };

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            else {
                return Vec::new();
            };
            let Ok(element) = automation.ElementFromHandle(hwnd) else {
                return Vec::new();
            };
            let Ok(condition) = automation.CreateTrueCondition() else {
                return Vec::new();
            };
            let Ok(found) = element.FindAll(TreeScope_Descendants, &condition) else {
                return Vec::new();
            };
            let Ok(length) = found.Length() else {
                return Vec::new();
            };
            let mut texts = Vec::new();
            for index in 0..length.min(800) {
                let Ok(item) = found.GetElement(index) else {
                    continue;
                };
                if let Ok(name) = item.CurrentName() {
                    let value = name.to_string();
                    if !value.trim().is_empty() {
                        texts.push(value);
                    }
                }
                if let Ok(pattern) =
                    item.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                {
                    if let Ok(value) = pattern.CurrentValue() {
                        let value = value.to_string();
                        if !value.trim().is_empty() {
                            texts.push(value);
                        }
                    }
                }
                if let Ok(pattern) =
                    item.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                {
                    if let Ok(range) = pattern.DocumentRange() {
                        if let Ok(value) = range.GetText(-1) {
                            let value = value.to_string();
                            if !value.trim().is_empty() {
                                texts.push(value);
                            }
                        }
                    }
                }
            }
            texts
        }
    }

    fn compass_page_score(text: &str) -> i32 {
        let mut score = 0;
        if text.contains("开播时间") {
            score += 10;
        }
        if text.contains("下载该视频") || text.contains("下载视频") {
            score += 5;
        }
        if super::COMPASS_SHOPS.iter().any(|shop| text.contains(shop)) {
            score += 3;
        }
        score
    }

    pub(super) fn scrape_compass_page_text() -> String {
        let mut windows = Vec::<HWND>::new();
        unsafe {
            let _ = EnumWindows(
                Some(collect_visible_windows),
                LPARAM((&mut windows as *mut Vec<HWND>) as isize),
            );
        }
        let mut best = String::new();
        let mut best_score = 0;
        for hwnd in windows {
            if !looks_like_compass_window(hwnd) {
                continue;
            }
            unsafe {
                let _ = SendMessageW(hwnd, WM_GETOBJECT, None, Some(LPARAM(-4)));
            }
            let mut chunks = collect_win32_texts(hwnd);
            chunks.extend(collect_uia_texts(hwnd));
            let text = chunks.join("\n");
            let score = compass_page_score(&text);
            if score > best_score {
                best_score = score;
                best = text;
            }
        }
        best
    }

    pub fn apply_compass_page_name_to_idm_dialog() -> Option<String> {
        let mut dialogs = Vec::<HWND>::new();
        unsafe {
            let _ = EnumWindows(
                Some(collect_top_windows),
                LPARAM((&mut dialogs as *mut Vec<HWND>) as isize),
            );
        }
        if dialogs.is_empty() {
            return None;
        }
        let identity = super::parse_compass_page_base_name(&scrape_compass_page_text())?;
        apply_name_to_latest_dialog(&identity)
    }

    fn looks_like_windows_file_path(value: &str) -> bool {
        let bytes = value.as_bytes();
        bytes.len() > 4
            && bytes.get(1) == Some(&b':')
            && matches!(bytes.get(2), Some(b'\\') | Some(b'/'))
            && !value.starts_with("http")
            && PathBuf::from(value).extension().is_some()
    }

    fn preferred_video_extension(path: &std::path::Path) -> &'static str {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref()
        {
            Some("ts") => "ts",
            Some("mp4") => "mp4",
            Some("flv") => "flv",
            Some("mkv") => "mkv",
            _ => "mp4",
        }
    }

    fn requested_save_path(current: &std::path::Path, base_name: &str) -> PathBuf {
        let file_name = format!("{base_name}.{}", preferred_video_extension(current));
        current
            .parent()
            .map(|parent| parent.join(&file_name))
            .unwrap_or_else(|| PathBuf::from(&file_name))
    }

    fn set_window_text(hwnd: HWND, value: &str) -> bool {
        let mut wide = value.encode_utf16().collect::<Vec<_>>();
        wide.push(0);
        unsafe {
            SendMessageW(hwnd, WM_SETTEXT, None, Some(LPARAM(wide.as_ptr() as isize)));
        }
        window_text(hwnd) == value
    }

    fn apply_name_via_uia(dialog: HWND, base_name: &str) -> Option<String> {
        use windows::core::BSTR;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        };
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
            UIA_ValuePatternId,
        };

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            else {
                return None;
            };
            let Ok(element) = automation.ElementFromHandle(dialog) else {
                return None;
            };
            let Ok(condition) = automation.CreateTrueCondition() else {
                return None;
            };
            let Ok(found) = element.FindAll(TreeScope_Descendants, &condition) else {
                return None;
            };
            let Ok(length) = found.Length() else {
                return None;
            };
            for index in 0..length.min(400) {
                let Ok(item) = found.GetElement(index) else {
                    continue;
                };
                let Ok(pattern) =
                    item.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                else {
                    continue;
                };
                let current = pattern
                    .CurrentValue()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                if !looks_like_windows_file_path(&current) {
                    continue;
                }
                let requested = requested_save_path(std::path::Path::new(&current), base_name);
                let requested_text = requested.to_string_lossy().to_string();
                if pattern
                    .SetValue(&BSTR::from(requested_text.as_str()))
                    .is_ok()
                {
                    return Some(requested_text);
                }
            }
            None
        }
    }

    pub fn latest_idm_save_directory() -> Option<String> {
        let mut dialogs = Vec::<HWND>::new();
        unsafe {
            let _ = EnumWindows(
                Some(collect_top_windows),
                LPARAM((&mut dialogs as *mut Vec<HWND>) as isize),
            );
        }
        for dialog in dialogs {
            let mut edits = Vec::<HWND>::new();
            unsafe {
                let _ = EnumChildWindows(
                    Some(dialog),
                    Some(collect_edit_controls),
                    LPARAM((&mut edits as *mut Vec<HWND>) as isize),
                );
            }
            if let Some(current) = edits
                .into_iter()
                .map(|control| window_text(control))
                .find(|text| looks_like_windows_file_path(text))
            {
                return PathBuf::from(current)
                    .parent()
                    .map(|parent| parent.to_string_lossy().to_string());
            }
        }
        None
    }

    pub fn apply_name_to_latest_dialog(base_name: &str) -> Option<String> {
        let mut dialogs = Vec::<HWND>::new();
        unsafe {
            let _ = EnumWindows(
                Some(collect_top_windows),
                LPARAM((&mut dialogs as *mut Vec<HWND>) as isize),
            );
        }
        for dialog in dialogs {
            let mut edits = Vec::<HWND>::new();
            unsafe {
                let _ = EnumChildWindows(
                    Some(dialog),
                    Some(collect_edit_controls),
                    LPARAM((&mut edits as *mut Vec<HWND>) as isize),
                );
            }
            if let Some(target) = edits
                .into_iter()
                .find(|control| looks_like_windows_file_path(&window_text(*control)))
            {
                let current = PathBuf::from(window_text(target));
                let requested = requested_save_path(&current, base_name);
                let requested_text = requested.to_string_lossy().to_string();
                if set_window_text(target, &requested_text) {
                    return Some(requested_text);
                }
            }
            if let Some(path) = apply_name_via_uia(dialog, base_name) {
                return Some(path);
            }
        }
        None
    }
}

fn rename_new_download_to_canonical(
    directory: &str,
    base_name: &str,
    armed_at: std::time::SystemTime,
    last_sizes: &mut std::collections::HashMap<std::path::PathBuf, u64>,
) -> Option<String> {
    if directory.trim().is_empty() {
        return None;
    }
    let entries = std::fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_idm_video_file(&path) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if modified + std::time::Duration::from_secs(15) < armed_at {
            continue;
        }
        let size = metadata.len();
        if size == 0 {
            continue;
        }
        let unchanged = last_sizes.insert(path.clone(), size) == Some(size);
        if !unchanged {
            continue;
        }
        let stem = path.file_stem()?.to_str()?;
        if stem == base_name || is_canonical_idm_video_stem(stem) {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("ts");
        let mut target = path.with_file_name(format!("{base_name}.{extension}"));
        if target == path {
            continue;
        }
        if target.exists() {
            target = path.with_file_name(format!("{base_name} (1).{extension}"));
        }
        if std::fs::rename(&path, &target).is_ok() {
            return Some(target.to_string_lossy().to_string());
        }
    }
    None
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn prepare_idm_download_filename(
    state: state_type!(),
    shop_name: String,
    started_at: String,
) -> Result<IdmNamingPreparation, String> {
    let base_name = build_idm_video_base_name(&shop_name, &started_at)?;
    let result = IdmNamingPreparation {
        suggested_file_name: format!("{base_name}.ts"),
        base_name: base_name.clone(),
        watcher_started: cfg!(target_os = "windows"),
    };

    #[cfg(all(feature = "gui", target_os = "windows"))]
    {
        use tauri::Emitter;
        let app = state.app_handle.clone();
        let download_dir = state
            .config
            .read()
            .await
            .live_dashboard_download_dir
            .clone();
        let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let armed_at = std::time::SystemTime::now();
        std::thread::spawn(move || {
            let mut last_sizes = std::collections::HashMap::<std::path::PathBuf, u64>::new();
            for _ in 0..1200 {
                if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                if let Some(path) = windows_idm::apply_name_to_latest_dialog(&base_name) {
                    let _ = app.emit(
                        "idm-download-name-applied",
                        serde_json::json!({ "path": path, "baseName": base_name }),
                    );
                    return;
                }
                if let Some(path) = rename_new_download_to_canonical(
                    &download_dir,
                    &base_name,
                    armed_at,
                    &mut last_sizes,
                ) {
                    let _ = app.emit(
                        "idm-download-name-applied",
                        serde_json::json!({ "path": path, "baseName": base_name }),
                    );
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            let _ = app.emit(
                "idm-download-name-timeout",
                serde_json::json!({ "baseName": base_name }),
            );
        });
    }

    #[cfg(not(feature = "gui"))]
    let _ = state;

    Ok(result)
}

pub fn start_idm_compass_rename_watcher() {
    #[cfg(all(feature = "gui", target_os = "windows"))]
    std::thread::spawn(|| {
        let mut last_path = String::new();
        let mut last_base = String::new();
        let mut last_dir = String::new();
        let mut armed_at = std::time::SystemTime::now();
        let mut last_sizes = std::collections::HashMap::<std::path::PathBuf, u64>::new();
        loop {
            if let Some(path) = windows_idm::apply_compass_page_name_to_idm_dialog() {
                if path != last_path {
                    log::info!("IDM 已按罗盘页面改名：{path}");
                    last_path = path.clone();
                    if let Some(stem) = std::path::Path::new(&path)
                        .file_stem()
                        .and_then(|value| value.to_str())
                    {
                        last_base = stem.to_string();
                    }
                    if let Some(dir) = std::path::Path::new(&path)
                        .parent()
                        .map(|value| value.to_string_lossy().to_string())
                    {
                        last_dir = dir;
                    }
                    armed_at = std::time::SystemTime::now();
                    last_sizes.clear();
                }
            } else if let Some(dir) = windows_idm::latest_idm_save_directory() {
                last_dir = dir;
                if last_base.is_empty() {
                    if let Some(name) =
                        parse_compass_page_base_name(&windows_idm::scrape_compass_page_text())
                    {
                        last_base = name;
                        armed_at = std::time::SystemTime::now();
                    }
                }
            }
            if !last_dir.is_empty() && !last_base.is_empty() {
                if let Some(path) = rename_new_download_to_canonical(
                    &last_dir,
                    &last_base,
                    armed_at,
                    &mut last_sizes,
                ) {
                    log::info!("IDM 下载文件已按规范改名：{path}");
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_windows_safe_names_from_live_session_time() {
        assert_eq!(
            build_idm_video_base_name("金典拍拍科创专卖店", "2026-08-01T08:57:00+08:00").unwrap(),
            "金典拍拍科创专卖店_2026-08-01_08-57-00"
        );
        assert_eq!(
            build_idm_video_base_name("金典拍拍相机专卖店微单相机场", "2026-08-04 20:58:00")
                .unwrap(),
            "金典拍拍相机专卖店_2026-08-04_20-58-00"
        );
    }

    #[test]
    fn rejects_unknown_shops_and_invalid_times() {
        assert!(build_idm_video_base_name("其他店铺", "2026-08-01T08:57:00").is_err());
        assert!(build_idm_video_base_name("金典拍拍科创专卖店", "8月1日").is_err());
    }

    #[test]
    fn recognizes_shop_and_time_video_stems() {
        assert!(is_canonical_idm_video_stem(
            "金典拍拍相机专卖店_2026-08-12_09-00-00"
        ));
        assert!(is_canonical_idm_video_stem(
            "金典拍拍相机专卖店_豌豆_2026-08-12_09-00-00"
        ));
        assert!(!is_canonical_idm_video_stem("随机下载名"));
        assert!(!is_canonical_idm_video_stem("直播大屏-专业版_2"));
    }

    #[test]
    fn reads_shop_time_and_anchor_from_compass_page_text() {
        let page = "直播大屏 · 专业版\n金典拍拍科创专卖店\n开播时间 2024/08/06 17:25 开播，共 5 小时 52 分钟\n主播：小鱼\n下载视频";
        assert_eq!(
            parse_compass_page_base_name(page).as_deref(),
            Some("金典拍拍科创专卖店_小鱼_2024-08-06_17-25-00")
        );
        let no_anchor = "金典拍拍相机专卖店\n开播时间：2026/08/12 09:00 开播";
        assert_eq!(
            parse_compass_page_base_name(no_anchor).as_deref(),
            Some("金典拍拍相机专卖店_2026-08-12_09-00-00")
        );
    }

    #[test]
    fn uses_shop_next_to_start_time_instead_of_the_first_hardcoded_shop() {
        let page = "店铺切换\n金典拍拍科创专卖店\n金典拍拍相机专卖店微单相机专场\n开播时间 2026/08/04 20:58 开播\n下载该视频";
        assert_eq!(
            parse_compass_page_base_name(page).as_deref(),
            Some("金典拍拍相机专卖店_2026-08-04_20-58-00")
        );
    }

    #[test]
    fn ignores_calendar_dates_without_start_time_label() {
        let page = "金典拍拍相机专卖店\n2026/08/01 09:00\n直播列表";
        assert_eq!(parse_compass_page_base_name(page), None);
    }
}
