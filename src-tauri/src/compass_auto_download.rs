const COMPASS_LIVE_OVERVIEW_URL: &str = "https://compass.jinritemai.com/shop/live-overview?from_page=%2Fshop";
const COMPASS_EVENT_URL_PREFIX: &str =
    "https://compass.jinritemai.com/__bsr_compass_event__?payload=";

#[cfg(feature = "gui")]
use crate::state::State;
#[cfg(feature = "gui")]
use tauri::State as TauriState;

pub fn validate_compass_date(value: &str) -> Result<String, String> {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err("日期必须使用 YYYY-MM-DD 格式".to_string());
    }
    let year = parts[0].parse::<i32>().map_err(|_| "年份无效")?;
    let month = parts[1].parse::<u32>().map_err(|_| "月份无效")?;
    let day = parts[2].parse::<u32>().map_err(|_| "日期无效")?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err("月份无效".to_string()),
    };
    if day == 0 || day > max_day {
        return Err("日期无效".to_string());
    }
    Ok(value.to_string())
}

pub fn is_allowed_compass_url(value: &str) -> bool {
    value == "https://compass.jinritemai.com"
        || value.starts_with("https://compass.jinritemai.com/")
}

pub fn compass_event_payload_from_url(value: &str) -> Option<&str> {
    value
        .strip_prefix(COMPASS_EVENT_URL_PREFIX)
        .filter(|payload| !payload.is_empty())
}

pub fn build_compass_automation_script(target_date: &str) -> Result<String, String> {
    let target_date = validate_compass_date(target_date)?;
    let target_json = format!("\"{target_date}\"");
    Ok(COMPASS_AUTOMATION_SCRIPT.replace("__TARGET_DATE_JSON__", &target_json))
}

pub fn build_compass_import_notification_script(started_at: &str) -> Result<String, String> {
    if started_at.is_empty()
        || !started_at
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | ':' | 'T' | '+' | 'Z' | '.' | ' '))
    {
        return Err("直播开始时间格式无效".to_string());
    }
    Ok(format!("window.__BSR_COMPASS_IMPORTED__?.({started_at:?});"))
}

#[cfg(feature = "gui")]
pub fn notify_compass_imported(app: &tauri::AppHandle, started_at: &str) {
    use tauri::Manager;
    let Ok(script) = build_compass_import_notification_script(started_at) else {
        return;
    };
    if let Some(window) = app.get_webview_window("compass-auto-download") {
        let _ = window.eval(&script);
    }
}

const COMPASS_AUTOMATION_SCRIPT: &str = r#"
(() => {
  const targetDate = __TARGET_DATE_JSON__;
  const storageKey = 'bsr:compass-auto-download:v2';
  const autoStart = new URLSearchParams(location.hash.slice(1)).get('bsrAutoStart') === '1';
  const emit = (payload) => {
    const enriched = { targetDate, ...payload };
    document.title = `BSR_COMPASS:${encodeURIComponent(JSON.stringify(enriched))}`;
    try {
      window.__TAURI_INTERNALS__?.invoke?.('plugin:event|emit', {
        event: 'compass-auto-download-progress', payload: enriched
      });
    } catch (_) {}
    try {
      location.assign(`https://compass.jinritemai.com/__bsr_compass_event__?payload=${encodeURIComponent(JSON.stringify(enriched))}`);
    } catch (_) {}
  };
  const visible = (node) => node && node.getClientRects().length > 0;
  const text = (node) => (node?.innerText || node?.textContent || '').replace(/\s+/g, ' ').trim();
  const exactText = (label, root = document) => Array.from(root.querySelectorAll('button,a,label,[role="button"],span,div'))
    .find((node) => visible(node) && text(node) === label);
  const clickTarget = (node) => {
    const target = node?.closest?.('button,a,label,[role="button"]') || node;
    if (!target) return false;
    target.click();
    return true;
  };
  const hoverTarget = (node) => {
    const target = node?.closest?.('label,button,a,[role="button"]') || node;
    if (!target) return false;
    target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, view: window }));
    target.dispatchEvent(new MouseEvent('mouseenter', { bubbles: false, view: window }));
    target.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, view: window }));
    return true;
  };
  const datePattern = new RegExp(`${targetDate.replaceAll('-', '[-/]')}\\s+(\\d{2}:\\d{2})\\s*[~～-]\\s*(?:${targetDate.replaceAll('-', '[-/]')}\\s+)?(\\d{2}:\\d{2})`);
  const normalizeTime = (clock) => `${targetDate}T${clock}:00+08:00`;
  const findSessionRows = () => {
    const candidates = Array.from(document.querySelectorAll('tr,[role="row"]')).filter((node) => datePattern.test(text(node)));
    const fallback = Array.from(document.querySelectorAll('*')).filter((node) => {
      const value = text(node);
      return visible(node) && datePattern.test(value) && value.includes('诊断') && value.length < 3000;
    });
    const rows = candidates.length ? candidates : fallback;
    const unique = new Map();
    for (const row of rows) {
      const value = text(row);
      const match = value.match(datePattern);
      if (!match) continue;
      const startedAt = normalizeTime(match[1]);
      const endedAt = normalizeTime(match[2]);
      const firstLine = (row.innerText || '').split('\n').map((part) => part.trim()).find(Boolean) || '抖音直播间';
      const key = `${firstLine}|${startedAt}|${endedAt}`;
      if (unique.has(key)) continue;
      const orderMatch = value.match(/(?:成交订单数)?\s*(\d+)\s*(?:净成交订单数|诊断|详情)/);
      const amountMatch = value.match(/¥[\d,.]+(?:万)?/g);
      unique.set(key, {
        key, row, shopName: firstLine, startedAt, endedAt,
        orderCount: orderMatch ? Number(orderMatch[1]) : null,
        paymentAmountText: amountMatch?.at(-1) || ''
      });
    }
    return [...unique.values()].sort((left, right) => left.startedAt.localeCompare(right.startedAt));
  };
  const state = (() => {
    try {
      const saved = JSON.parse(localStorage.getItem(storageKey) || '{}');
      if (saved.targetDate === targetDate) return saved;
    } catch (_) {}
    return { targetDate, completed: [], activeKey: null };
  })();
  const save = () => localStorage.setItem(storageKey, JSON.stringify(state));
  window.__BSR_COMPASS_IMPORTED__ = (startedAt) => {
    const activeStartedAt = state.activeKey?.split('|').at(-2) || '';
    if (!state.activeKey || activeStartedAt.slice(0, 16) !== String(startedAt).slice(0, 16)) return;
    if (!state.completed.includes(state.activeKey)) state.completed.push(state.activeKey);
    save();
    emit({ status: 'imported', sessionKey: state.activeKey, message: '下载并导入成功，准备处理下一场' });
    setTimeout(() => history.back(), 600);
  };
  const [targetYear, targetMonth, targetDay] = targetDate.split('-').map(Number);
  let filterApplied = false;
  let customOpened = false;
  let selectionPending = false;
  let navigationPending = false;
  let detailDownloadTriggered = false;
  let currentPageKind = '';
  const setInputValue = (input, value) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const monthHeaderPattern = /^(\d{4})年\s*(\d{1,2})月$/;
  const visibleMonthHeaders = () => Array.from(document.querySelectorAll('*'))
    .filter((node) => visible(node) && monthHeaderPattern.test(text(node)));
  const findMonthPanel = (header) => {
    let node = header?.parentElement;
    while (node && node !== document.body) {
      const headers = Array.from(node.querySelectorAll('*'))
        .filter((item) => visible(item) && monthHeaderPattern.test(text(item)));
      const days = Array.from(node.querySelectorAll('button,td,[role="gridcell"],span,div'))
        .filter((item) => visible(item) && /^(?:[1-9]|[12]\d|3[01])$/.test(text(item)));
      if (headers.length === 1 && days.length >= 20) return node;
      node = node.parentElement;
    }
    return header?.parentElement || null;
  };
  const findDayInPanel = (panel, day) => Array.from(panel?.querySelectorAll('button,td,[role="gridcell"],span,div') || [])
    .find((node) => visible(node)
      && text(node) === String(day)
      && node.getAttribute('aria-disabled') !== 'true'
      && !/(disabled|outside|other-month)/i.test(node.className || ''));
  const findExactDateCell = () => Array.from(document.querySelectorAll(`td[title="${targetDate}"]`))
    .find((node) => visible(node) && !/disabled/i.test(node.className || ''));
  const clickMonthNavigation = (direction, headers) => {
    const labelPattern = direction < 0 ? /(上一月|上个月|prev(?:ious)? month)/i : /(下一月|下个月|next month)/i;
    const buttons = Array.from(document.querySelectorAll('button,[role="button"]')).filter(visible);
    const labelled = buttons.find((button) => labelPattern.test(`${button.getAttribute('aria-label') || ''} ${button.getAttribute('title') || ''}`));
    if (labelled) return clickTarget(labelled);
    const anchor = direction < 0 ? headers[0] : headers.at(-1);
    if (!anchor) return false;
    const anchorRect = anchor.getBoundingClientRect();
    const nearby = buttons.filter((button) => {
      const rect = button.getBoundingClientRect();
      const sameRow = Math.abs((rect.top + rect.height / 2) - (anchorRect.top + anchorRect.height / 2)) < 45;
      return sameRow && (direction < 0 ? rect.right <= anchorRect.left + 20 : rect.left >= anchorRect.right - 20);
    }).sort((left, right) => {
      const leftRect = left.getBoundingClientRect();
      const rightRect = right.getBoundingClientRect();
      return direction < 0 ? rightRect.right - leftRect.right : leftRect.left - rightRect.left;
    });
    return clickTarget(nearby[0]);
  };
  const requestDateFilter = () => {
    if (filterApplied || selectionPending) return true;
    const headers = visibleMonthHeaders();
    if (!headers.length) {
      if (customOpened) return true;
      const customInput = document.querySelector('input[type="radio"][value="custom"]');
      const custom = customInput?.closest('label') || exactText('自定义');
      if (!custom) return false;
      customOpened = hoverTarget(custom);
      setTimeout(() => { customOpened = false; }, 1200);
      emit({ status: 'filtering', message: `正在打开日期面板，准备选择 ${targetDate}` });
      return true;
    }
    customOpened = false;
    const exactDateCell = findExactDateCell();
    if (exactDateCell) {
      selectionPending = true;
      const selectEndDate = () => {
        const endDateCell = findExactDateCell();
        if (!endDateCell) {
          selectionPending = false;
          emit({ status: 'manual-filter-needed', message: `日期面板已关闭，请重新悬停“自定义”并选择 ${targetDate}` });
          return;
        }
        clickTarget(endDateCell);
        setTimeout(() => {
          filterApplied = true;
          selectionPending = false;
          emit({ status: 'filtering', message: `已选择 ${targetDate}，等待场次列表刷新` });
        }, 800);
      };
      clickTarget(exactDateCell);
      setTimeout(selectEndDate, 700);
      return true;
    }
    const parsedHeaders = headers.map((header) => {
      const match = text(header).match(monthHeaderPattern);
      return { header, index: Number(match[1]) * 12 + Number(match[2]) - 1 };
    }).sort((left, right) => left.index - right.index);
    const targetIndex = targetYear * 12 + targetMonth - 1;
    const targetHeader = parsedHeaders.find((item) => item.index === targetIndex)?.header;
    if (!targetHeader) {
      const direction = targetIndex < parsedHeaders[0].index ? -1 : 1;
      if (!clickMonthNavigation(direction, headers)) {
        emit({ status: 'manual-filter-needed', message: `未找到日历的${direction < 0 ? '上一月' : '下一月'}按钮，请手动选择 ${targetDate}` });
      }
      return true;
    }
    const panel = findMonthPanel(targetHeader);
    const day = findDayInPanel(panel, targetDay);
    if (!day) {
      emit({ status: 'manual-filter-needed', message: `未找到 ${targetDate} 对应的日期单元格` });
      return true;
    }
    selectionPending = true;
    clickTarget(day);
    setTimeout(() => {
      const refreshedHeader = visibleMonthHeaders().find((header) => {
        const match = text(header).match(monthHeaderPattern);
        return Number(match?.[1]) === targetYear && Number(match?.[2]) === targetMonth;
      });
      const refreshedDay = findDayInPanel(findMonthPanel(refreshedHeader), targetDay);
      if (refreshedDay) clickTarget(refreshedDay);
      filterApplied = true;
      selectionPending = false;
      setTimeout(() => clickTarget(exactText('确定')) || clickTarget(exactText('查询')), 300);
      emit({ status: 'filtering', message: `已选择 ${targetDate}，等待场次列表刷新` });
    }, 350);
    return true;
  };
  const processList = () => {
    const sessions = findSessionRows();
    if (!sessions.length) {
      emit({ status: 'waiting-for-sessions', message: `尚未找到 ${targetDate} 的直播，请确认日期筛选`, sessions: [] });
      requestDateFilter();
      return;
    }
    emit({ status: 'sessions-found', message: `找到 ${sessions.length} 场直播`, sessions: sessions.map(({ row, ...session }) => session) });
    if (!autoStart) return;
    if (navigationPending) return;
    const next = sessions.find((session) => !state.completed.includes(session.key));
    if (!next) {
      emit({ status: 'batch-finished', message: `当天 ${sessions.length} 场直播已全部触发下载`, sessions: sessions.map(({ row, ...session }) => session) });
      return;
    }
    state.activeKey = next.key;
    save();
    const diagnose = exactText('诊断', next.row);
    navigationPending = true;
    const diagnosePath = diagnose?.closest?.('a')?.getAttribute('data-subway-href');
    if (diagnosePath) {
      const detailUrl = new URL(diagnosePath, location.origin);
      detailUrl.searchParams.set('tab', 'diagnosis');
      emit({ status: 'opening', sessionKey: next.key, message: '正在打开直播诊断详情' });
      setTimeout(() => location.assign(detailUrl.href), 120);
      return;
    } else if (!clickTarget(diagnose)) {
      navigationPending = false;
      emit({ status: 'failed', sessionKey: next.key, message: '未找到该场次的“诊断”按钮，请手动进入详情' });
      return;
    }
    emit({ status: 'opening', sessionKey: next.key, message: '正在打开直播诊断详情' });
  };
  const processDetail = () => {
    navigationPending = false;
    if (detailDownloadTriggered) return;
    const download = exactText('下载');
    if (!download) {
      emit({ status: 'waiting-for-download-button', sessionKey: state.activeKey, message: '等待详情页“下载”按钮' });
      return;
    }
    detailDownloadTriggered = true;
    clickTarget(download);
    emit({ status: 'downloading', sessionKey: state.activeKey, message: '已触发整场数据下载，等待文件下载并导入' });
    setTimeout(() => {
      if (detailDownloadTriggered) {
        emit({ status: 'downloading', sessionKey: state.activeKey, message: '仍在等待 XLSX 下载完成；完成后将自动继续下一场' });
      }
    }, 90000);
  };
  const run = () => {
    if (!location.hostname.endsWith('jinritemai.com')) return;
    const pageText = text(document.body);
    const pageKind = pageText.includes('直播间详情') ? 'detail' : pageText.includes('直播间列表') ? 'list' : 'other';
    if (pageKind !== currentPageKind) {
      const previousPageKind = currentPageKind;
      currentPageKind = pageKind;
      if (pageKind === 'list' && previousPageKind === 'detail') {
        navigationPending = false;
        detailDownloadTriggered = false;
      }
    }
    if (pageKind === 'detail') processDetail();
    else if (pageKind === 'list') processList();
    else emit({ status: 'login-required', message: '请扫码登录抖音电商罗盘，登录后将自动继续' });
  };
  let timer;
  const scheduleRun = () => {
    clearTimeout(timer);
    timer = setTimeout(run, 900);
  };
  const attachObserver = () => {
    if (!document.documentElement) {
      setTimeout(attachObserver, 100);
      return;
    }
    new MutationObserver(scheduleRun).observe(document.documentElement, {
      subtree: true,
      childList: true
    });
    scheduleRun();
    setInterval(run, 3000);
  };
  attachObserver();
})();
"#;

#[cfg(all(feature = "gui", target_os = "windows"))]
fn allow_compass_multiple_downloads(window: &tauri::WebviewWindow) {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_PERMISSION_KIND,
            COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS,
            COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        },
        PermissionRequestedEventHandler,
    };

    let _ = window.with_webview(|platform_webview| unsafe {
        let Ok(webview) = platform_webview.controller().CoreWebView2() else {
            return;
        };
        let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            args.PermissionKind(&mut kind)?;
            if kind == COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
            }
            Ok(())
        }));
        let mut token = 0;
        let _ = webview.add_PermissionRequested(&handler, &mut token);
    });
}

#[cfg(feature = "gui")]
fn open_compass_window(
    app: &tauri::AppHandle,
    target_date: &str,
    auto_start: bool,
) -> Result<(), String> {
    use tauri::{Emitter, Manager};

    const LABEL: &str = "compass-auto-download";
    if let Some(existing) = app.get_webview_window(LABEL) {
        existing.close().map_err(|error| error.to_string())?;
    }
    let script = build_compass_automation_script(target_date)?;
    let suffix = if auto_start { "#bsrAutoStart=1" } else { "#bsrAutoStart=0" };
    let url = format!("{COMPASS_LIVE_OVERVIEW_URL}{suffix}");
    if !is_allowed_compass_url(&url) {
        return Err("罗盘地址不在允许范围内".to_string());
    }
    let navigation_app = app.clone();
    let window = tauri::WebviewWindowBuilder::new(
        app,
        LABEL,
        tauri::WebviewUrl::External(url.parse().map_err(|error| format!("罗盘地址无效：{error}"))?),
    )
    .title("抖音罗盘 · 直播大屏自动下载")
    .inner_size(1380.0, 900.0)
    .center()
    .initialization_script(&script)
    .on_navigation(move |navigation_url| {
        if let Some(encoded) = compass_event_payload_from_url(navigation_url.as_str()) {
            if let Ok(decoded) = urlencoding::decode(encoded) {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&decoded) {
                    let _ = navigation_app.emit("compass-auto-download-progress", payload);
                }
            }
            return false;
        }
        is_allowed_compass_url(navigation_url.as_str())
    })
    .build()
    .map_err(|error| format!("打开抖音罗盘失败：{error}"))?;

    #[cfg(target_os = "windows")]
    allow_compass_multiple_downloads(&window);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_title = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(window) = app_handle.get_webview_window(LABEL) else { break };
            let Ok(title) = window.title() else { continue };
            if title == last_title { continue }
            let Some(encoded) = title.strip_prefix("BSR_COMPASS:") else { continue };
            let encoded = encoded.to_string();
            last_title = title;
            let Ok(decoded) = urlencoding::decode(&encoded) else { continue };
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&decoded) else { continue };
            let _ = app_handle.emit("compass-auto-download-progress", payload);
        }
    });
    Ok(())
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn query_compass_live_sessions(
    state: crate::state_type!(),
    target_date: String,
) -> Result<(), String> {
    open_compass_window(&state.app_handle, &target_date, false)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn start_compass_live_downloads(
    state: crate::state_type!(),
    target_date: String,
) -> Result<(), String> {
    open_compass_window(&state.app_handle, &target_date, true)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn close_compass_auto_download(state: crate::state_type!()) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = state.app_handle.get_webview_window("compass-auto-download") {
        window.close().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_calendar_dates() {
        assert_eq!(validate_compass_date("2026-07-28").unwrap(), "2026-07-28");
    }

    #[test]
    fn rejects_impossible_or_non_iso_dates() {
        assert!(validate_compass_date("2026-02-30").is_err());
        assert!(validate_compass_date("2026/07/28").is_err());
    }

    #[test]
    fn limits_navigation_to_compass() {
        assert!(is_allowed_compass_url(
            "https://compass.jinritemai.com/shop/live-overview?from_page=%2Fshop"
        ));
        assert!(!is_allowed_compass_url("https://example.com/shop/live-overview"));
        assert!(!is_allowed_compass_url("javascript:alert(1)"));
    }

    #[test]
    fn generated_script_contains_only_json_encoded_date() {
        let script = build_compass_automation_script("2026-07-28").unwrap();
        assert!(script.contains("2026-07-28"));
        assert!(script.contains("compass-auto-download-progress"));
        assert!(script.contains("if (!document.documentElement)"));
        assert!(script.contains("findMonthPanel"));
        assert!(script.contains("上一月"));
        assert!(script.contains("filterApplied"));
        assert!(script.contains("navigationPending"));
        assert!(script.contains("detailDownloadTriggered"));
        assert!(!script.contains("{{TARGET_DATE}}"));
    }

    #[test]
    fn generated_script_uses_hover_and_exact_date_cells() {
        let script = build_compass_automation_script("2026-07-29").unwrap();
        assert!(script.contains("mouseenter"));
        assert!(script.contains("td[title=\"${targetDate}\"]"));
        assert!(script.contains("setTimeout(selectEndDate, 700)"));
        assert!(script.contains("data-subway-href"));
        assert!(script.contains("setTimeout(() => location.assign(detailUrl.href), 120)"));
        assert!(script.contains("bsr:compass-auto-download:v2"));
        assert!(script.contains("window.__BSR_COMPASS_IMPORTED__"));
        assert!(!script.contains("setTimeout(() => history.back(), 4500)"));
    }

    #[test]
    fn builds_safe_import_notification_script() {
        assert_eq!(
            build_compass_import_notification_script("2026-07-29T08:19:03+08:00").unwrap(),
            "window.__BSR_COMPASS_IMPORTED__?.(\"2026-07-29T08:19:03+08:00\");"
        );
        assert!(build_compass_import_notification_script("');alert(1)//").is_err());
    }

    #[test]
    fn recognizes_compass_progress_navigation() {
        assert_eq!(
            compass_event_payload_from_url(
                "https://compass.jinritemai.com/__bsr_compass_event__?payload=%7B%22status%22%3A%22sessions-found%22%7D"
            ),
            Some("%7B%22status%22%3A%22sessions-found%22%7D")
        );
        assert_eq!(
            compass_event_payload_from_url("https://compass.jinritemai.com/shop/live-overview"),
            None
        );
        assert_eq!(
            compass_event_payload_from_url(
                "https://example.com/__bsr_compass_event__?payload=%7B%7D"
            ),
            None
        );
    }
}
