use chrono::{DateTime, NaiveDateTime};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "gui")]
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

#[cfg(target_os = "windows")]
mod windows_idm {
    use std::path::PathBuf;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible,
        SendMessageW, WM_SETTEXT,
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
        if IsWindowVisible(hwnd).as_bool() {
            let title = window_text(hwnd);
            if title.contains("下载文件信息")
                || title.to_ascii_lowercase().contains("download file info")
            {
                windows.push(hwnd);
            }
        }
        BOOL(1)
    }

    unsafe extern "system" fn collect_edit_controls(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let controls = &mut *(lparam.0 as *mut Vec<HWND>);
        if class_name(hwnd).eq_ignore_ascii_case("Edit") {
            controls.push(hwnd);
        }
        BOOL(1)
    }

    fn looks_like_windows_file_path(value: &str) -> bool {
        let bytes = value.as_bytes();
        bytes.len() > 4
            && bytes.get(1) == Some(&b':')
            && matches!(bytes.get(2), Some(b'\\') | Some(b'/'))
            && !value.starts_with("http")
            && PathBuf::from(value).extension().is_some()
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
            let Some(target) = edits
                .into_iter()
                .find(|control| looks_like_windows_file_path(&window_text(*control)))
            else {
                continue;
            };
            let current = PathBuf::from(window_text(target));
            let extension = current
                .extension()
                .and_then(|value| value.to_str())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("ts");
            let file_name = format!("{base_name}.{extension}");
            let requested = current
                .parent()
                .map(|parent| parent.join(&file_name))
                .unwrap_or_else(|| PathBuf::from(&file_name));
            let mut wide = requested
                .to_string_lossy()
                .encode_utf16()
                .collect::<Vec<_>>();
            wide.push(0);
            unsafe {
                SendMessageW(
                    target,
                    WM_SETTEXT,
                    None,
                    Some(LPARAM(wide.as_ptr() as isize)),
                );
            }
            if window_text(target) == requested.to_string_lossy() {
                return Some(requested.to_string_lossy().to_string());
            }
        }
        None
    }
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
        let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::spawn(move || {
            for _ in 0..480 {
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
}
