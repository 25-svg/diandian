use std::str::FromStr;

use crate::database::account::AccountRow;
use crate::security::{audit_tool_failure, audit_tool_success, require_sensitive_write};
use crate::state::State;
use crate::state_type;
use chrono::Utc;
use recorder::platforms::bilibili::api::{QrInfo, QrStatus};
use recorder::platforms::{bilibili, douyin, huya, kuaishou, tiktok, PlatformType};
use recorder::UserInfo;

use hyper::header::HeaderValue;
#[cfg(feature = "gui")]
use tauri::Manager;
#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_accounts(state: state_type!()) -> Result<super::AccountInfo, String> {
    let account_info = super::AccountInfo {
        accounts: state.db.get_accounts().await?,
    };
    Ok(account_info)
}

fn get_item_from_cookies(name: &str, cookies: &str) -> Result<String, String> {
    Ok(cookies
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix(format!("{name}=").as_str()))
        .ok_or_else(|| format!("Invalid cookies: missing {name}").to_string())?
        .to_string())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn add_account(
    state: state_type!(),
    platform: String,
    cookies: &str,
) -> Result<(), String> {
    // check if cookies is valid
    if let Err(e) = cookies.parse::<HeaderValue>() {
        return Err(format!("Invalid cookies: {e}"));
    }

    let platform = PlatformType::from_str(&platform).map_err(|_| "Invalid platform".to_string())?;

    let csrf = match platform {
        PlatformType::BiliBili => {
            cookies
                .split(';')
                .map(str::trim)
                .find_map(|cookie| -> Option<String> {
                    if cookie.starts_with("bili_jct=") {
                        let var_name = &"bili_jct=";
                        Some(cookie[var_name.len()..].to_string())
                    } else {
                        None
                    }
                })
        }
        _ => Some(String::new()),
    };

    // fetch basic account user info
    let client = reqwest::Client::new();
    let user_info = match platform {
        PlatformType::BiliBili => {
            // For Bilibili, extract numeric uid from cookies
            if csrf.is_none() {
                return Err("Invalid bilibili cookies".to_string());
            }
            let uid = get_item_from_cookies("DedeUserID", cookies)?;
            let tmp_account = AccountRow {
                platform: platform.as_str().to_string(),
                uid,
                name: String::new(),
                avatar: String::new(),
                csrf: csrf.clone().unwrap(),
                cookies: cookies.into(),
                created_at: Utc::now().to_rfc3339(),
            };
            match bilibili::api::get_user_info(&client, &tmp_account.to_account(), &tmp_account.uid)
                .await
            {
                Ok(user_info) => UserInfo {
                    user_id: user_info.user_id,
                    user_name: user_info.user_name,
                    user_avatar: user_info.user_avatar_url,
                },
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        }
        PlatformType::Douyin => {
            let tmp_account = AccountRow {
                platform: platform.as_str().to_string(),
                uid: "".into(),
                name: String::new(),
                avatar: String::new(),
                csrf: "".into(),
                cookies: cookies.into(),
                created_at: Utc::now().to_rfc3339(),
            };

            match douyin::api::get_user_info(&client, &tmp_account.to_account()).await {
                Ok(user_info) => {
                    // For Douyin, use sec_uid as the primary identifier in id_str field
                    let avatar_url = user_info
                        .avatar_thumb
                        .url_list
                        .first()
                        .cloned()
                        .unwrap_or_default();

                    UserInfo {
                        user_id: user_info.sec_uid,
                        user_name: user_info.nickname,
                        user_avatar: avatar_url,
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to get Douyin user info: {e}"));
                }
            }
        }
        PlatformType::Huya => {
            let user_id = get_item_from_cookies("yyuid", cookies)?;

            let tmp_account = AccountRow {
                platform: platform.as_str().to_string(),
                uid: user_id,
                name: String::new(),
                avatar: String::new(),
                csrf: "".into(),
                cookies: cookies.into(),
                created_at: Utc::now().to_rfc3339(),
            };

            match huya::api::get_user_info(&client, &tmp_account.to_account()).await {
                Ok(user_info) => UserInfo {
                    user_id: user_info.user_id,
                    user_name: user_info.user_name,
                    user_avatar: user_info.user_avatar,
                },
                Err(e) => {
                    return Err(format!("Failed to get Huya user info: {e}"));
                }
            }
        }
        PlatformType::Youtube => {
            // unsupported
            return Err("Unsupported platform".to_string());
        }
        PlatformType::Kuaishou => {
            let tmp_account = AccountRow {
                platform: platform.as_str().to_string(),
                uid: "".into(),
                name: String::new(),
                avatar: String::new(),
                csrf: "".into(),
                cookies: cookies.into(),
                created_at: Utc::now().to_rfc3339(),
            };
            match kuaishou::api::get_user_info(&client, &tmp_account.to_account()).await {
                Ok(user_info) => UserInfo {
                    user_id: user_info.user_id,
                    user_name: user_info.user_name,
                    user_avatar: user_info.user_avatar,
                },
                Err(e) => {
                    return Err(format!("Failed to get Kuaishou user info: {e}"));
                }
            }
        }
        PlatformType::Xiaohongshu => {
            return Err("Unsupported platform".to_string());
        }
        PlatformType::TikTok => {
            let tmp_account = AccountRow {
                platform: platform.as_str().to_string(),
                uid: "".into(),
                name: String::new(),
                avatar: String::new(),
                csrf: "".into(),
                cookies: cookies.into(),
                created_at: Utc::now().to_rfc3339(),
            };
            match tiktok::api::get_user_info(&client, &tmp_account.to_account()).await {
                Ok(user_info) => UserInfo {
                    user_id: user_info.user_id,
                    user_name: user_info.user_name,
                    user_avatar: user_info.user_avatar,
                },
                Err(e) => {
                    return Err(format!("Failed to get TikTok user info: {e}"));
                }
            }
        }
        PlatformType::Weibo => {
            return Err("Unsupported platform".to_string());
        }
    };

    let account = AccountRow {
        platform: platform.as_str().to_string(),
        uid: user_info.user_id,
        name: user_info.user_name,
        avatar: user_info.user_avatar,
        csrf: csrf.unwrap(),
        cookies: cookies.into(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.db.add_account(&account).await?;
    Ok(())
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn remove_account(
    state: state_type!(),
    platform: String,
    uid: String,
    idempotency_key: String,
    confirmation_token: String,
    trace_id: Option<String>,
) -> Result<(), String> {
    let audit = require_sensitive_write(
        "remove_account",
        &idempotency_key,
        &confirmation_token,
        trace_id.as_deref(),
        &format!("account:{platform}:{uid}"),
    )?;
    if platform == "bilibili" {
        let account = state.db.get_account(&platform, &uid).await?;
        let client = reqwest::Client::new();
        let _ = bilibili::api::logout(&client, &account.to_account()).await;
    }
    match state.db.remove_account(&platform, &uid).await {
        Ok(result) => {
            audit_tool_success(&audit);
            Ok(result)
        }
        Err(error) => {
            let error = error.to_string();
            audit_tool_failure(&audit, &error);
            Err(error)
        }
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_account_count(state: state_type!()) -> Result<u64, String> {
    Ok(state.db.get_accounts().await?.len() as u64)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_qr_status(_state: state_type!(), qrcode_key: &str) -> Result<QrStatus, ()> {
    let client = reqwest::Client::new();
    match bilibili::api::get_qr_status(&client, qrcode_key).await {
        Ok(qr_status) => Ok(qr_status),
        Err(_e) => Err(()),
    }
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_qr(_state: state_type!()) -> Result<QrInfo, ()> {
    let client = reqwest::Client::new();
    match bilibili::api::get_qr(&client).await {
        Ok(qr_info) => Ok(qr_info),
        Err(_e) => Err(()),
    }
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn open_douyin_login(state: state_type!()) -> Result<(), String> {
    const LABEL: &str = "douyin-login";

    if let Some(window) = state.app_handle.get_webview_window(LABEL) {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &state.app_handle,
        LABEL,
        tauri::WebviewUrl::External(
            "https://www.douyin.com/"
                .parse()
                .map_err(|e| format!("Invalid Douyin login URL: {e}"))?,
        ),
    )
    .title("抖音扫码登录")
    .inner_size(1100.0, 760.0)
    .center()
    .initialization_script(
        r#"
        (() => {
          const timer = setInterval(() => {
            const nodes = Array.from(document.querySelectorAll('button, [role="button"], span, div'));
            const label = nodes.find((node) =>
              node.textContent && node.textContent.trim() === '登录' && node.offsetParent !== null
            );
            const target = label && (label.closest('button, [role="button"]') || label);
            if (target) {
              target.click();
              clearInterval(timer);
            }
          }, 400);
          setTimeout(() => clearInterval(timer), 15000);
        })();
        "#,
    )
    .build()
    .map_err(|e| format!("打开抖音登录窗口失败: {e}"))?;

    Ok(())
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn get_douyin_login_cookies(state: state_type!()) -> Result<Option<String>, String> {
    let Some(window) = state.app_handle.get_webview_window("douyin-login") else {
        return Ok(None);
    };

    // This is an async command: WebView2 cookie reads must not run in a synchronous
    // event handler on Windows, otherwise the WebView callback can deadlock.
    let cookies = window
        .cookies_for_url(
            "https://www.douyin.com/"
                .parse()
                .map_err(|e| format!("Invalid Douyin URL: {e}"))?,
        )
        .map_err(|e| format!("读取抖音登录状态失败: {e}"))?;

    let logged_in = cookies.iter().any(|cookie| {
        matches!(
            cookie.name(),
            "sessionid" | "sessionid_ss" | "sid_guard" | "sid_tt"
        ) && !cookie.value().is_empty()
    });

    if !logged_in {
        return Ok(None);
    }

    let cookie_header = cookies
        .iter()
        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
        .collect::<Vec<_>>()
        .join("; ");

    Ok(Some(cookie_header))
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn close_douyin_login(state: state_type!()) -> Result<(), String> {
    if let Some(window) = state.app_handle.get_webview_window("douyin-login") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_item_from_cookies() {
        let cookies = "DedeUserID=1234567890; bili_jct=1234567890; yyuid=1234567890";
        let uid = get_item_from_cookies("DedeUserID", cookies).unwrap();
        assert_eq!(uid, "1234567890");
        let uid = get_item_from_cookies("yyuid", cookies).unwrap();
        assert_eq!(uid, "1234567890");
        let uid = get_item_from_cookies("bili_jct", cookies).unwrap();
        assert_eq!(uid, "1234567890");
        let uid = get_item_from_cookies("unknown", cookies).unwrap_err();
        assert_eq!(uid, "Invalid cookies: missing unknown");
    }

    #[test]
    fn test_get_item_from_cookies_empty() {
        let result = get_item_from_cookies("key", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_item_from_cookies_no_value() {
        let cookies = "key1=; key2=value2";
        let result = get_item_from_cookies("key1", cookies).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_get_item_from_cookies_special_chars() {
        let cookies = "token=abc%3Ddef; session=xyz123";
        let result = get_item_from_cookies("token", cookies).unwrap();
        assert_eq!(result, "abc%3Ddef");
    }

    #[test]
    fn test_get_item_from_cookies_whitespace_handling() {
        let cookies = "  key1=val1  ;  key2=val2  ";
        let result = get_item_from_cookies("key1", cookies).unwrap();
        assert_eq!(result, "val1");
        let result = get_item_from_cookies("key2", cookies).unwrap();
        assert_eq!(result, "val2");
    }
}
