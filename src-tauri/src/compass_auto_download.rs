const COMPASS_LIVE_OVERVIEW_URL: &str =
    "https://compass.jinritemai.com/shop/live-overview?from_page=%2Fshop";
const COMPASS_EVENT_URL_PREFIX: &str =
    "https://compass.jinritemai.com/__bsr_compass_event__?payload=";
const COMPASS_TARGET_SHOPS: [&str; 2] = ["金典拍拍科创专卖店", "金典拍拍相机专卖店"];

#[cfg(feature = "gui")]
use crate::state::State;
#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[cfg(feature = "gui")]
static COMPASS_WINDOW_OPEN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(feature = "gui")]
static COMPASS_EMBEDDED_OPEN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(feature = "gui")]
const COMPASS_EMBEDDED_LABEL: &str = "compass-embedded";

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

pub fn validate_compass_shop(value: &str) -> Result<String, String> {
    let compact = value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if COMPASS_TARGET_SHOPS.contains(&compact.as_str()) {
        return Ok(compact);
    }
    if compact.starts_with("金典拍拍") && compact.contains("科创") {
        return Ok(COMPASS_TARGET_SHOPS[0].to_string());
    }
    if compact.starts_with("金典拍拍") && (compact.contains("相机") || compact.contains("微单"))
    {
        return Ok(COMPASS_TARGET_SHOPS[1].to_string());
    }
    Err("店铺识别失败：请先在罗盘中切换到金典拍拍科创店或相机店".to_string())
}

pub fn build_compass_automation_script(
    target_date: &str,
    target_shop_name: &str,
    target_started_at: Option<&str>,
) -> Result<String, String> {
    build_compass_automation_script_with_mode(
        target_date,
        target_shop_name,
        target_started_at,
        false,
        false,
        None,
    )
}

fn build_compass_automation_script_with_mode(
    target_date: &str,
    target_shop_name: &str,
    target_started_at: Option<&str>,
    auto_start: bool,
    capture_mode: bool,
    capture_id: Option<&str>,
) -> Result<String, String> {
    let target_date = validate_compass_date(target_date)?;
    let target_shop_name = validate_compass_shop(target_shop_name)?;
    let target_json = format!("\"{target_date}\"");
    let shop_json = serde_json::to_string(&target_shop_name)
        .map_err(|error| format!("店铺名称编码失败：{error}"))?;
    let started_json = match target_started_at
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            serde_json::to_string(value).map_err(|error| format!("开播时间编码失败：{error}"))?
        }
        None => "null".to_string(),
    };
    let capture_id_json = serde_json::to_string(capture_id.unwrap_or_default())
        .map_err(|error| format!("采集标识编码失败：{error}"))?;
    Ok(COMPASS_AUTOMATION_SCRIPT
        .replace("__TARGET_DATE_JSON__", &target_json)
        .replace("__TARGET_SHOP_JSON__", &shop_json)
        .replace("__TARGET_STARTED_AT_JSON__", &started_json)
        .replace(
            "__AUTO_START_JSON__",
            if auto_start { "true" } else { "false" },
        )
        .replace(
            "__CAPTURE_MODE_JSON__",
            if capture_mode { "true" } else { "false" },
        )
        .replace("__CAPTURE_ID_JSON__", &capture_id_json))
}

pub fn build_compass_import_notification_script(started_at: &str) -> Result<String, String> {
    if started_at.is_empty()
        || !started_at
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | ':' | 'T' | '+' | 'Z' | '.' | ' '))
    {
        return Err("直播开始时间格式无效".to_string());
    }
    Ok(format!(
        "window.__BSR_COMPASS_IMPORTED__?.({started_at:?});"
    ))
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
  if (window.__BSR_COMPASS_AUTOMATION_INSTALLED__) return;
  window.__BSR_COMPASS_AUTOMATION_INSTALLED__ = true;
  const targetDate = __TARGET_DATE_JSON__;
  const targetShopName = __TARGET_SHOP_JSON__;
  const targetStartedAt = __TARGET_STARTED_AT_JSON__;
  const captureId = __CAPTURE_ID_JSON__;
  const captureMode = __CAPTURE_MODE_JSON__;
  const autoStart = __AUTO_START_JSON__;
  const allowedShopNames = ['金典拍拍科创专卖店', '金典拍拍相机专卖店'];
  const storageKey = `bsr:compass-auto-download:v4:${captureMode ? captureId : 'download'}`;
  const emit = (payload) => {
    const enriched = { targetDate, targetShopName, ...payload };
    document.title = `BSR_COMPASS:${encodeURIComponent(JSON.stringify(enriched))}`;
    try {
      window.__TAURI_INTERNALS__?.invoke?.('plugin:event|emit', {
        event: 'compass-auto-download-progress', payload: enriched
      });
    } catch (_) {}
    try {
      location.assign(`https://compass.jinritemai.com/__bsr_compass_event__?payload=${encodeURIComponent(JSON.stringify(enriched))}`);
    } catch (_) {}
    if (captureMode && ['failed', 'shop-unavailable', 'shop-mismatch'].includes(payload.status)) {
      setTimeout(() => emitCaptureEvent('compass-capture-control', {
        captureId, status: 'failed', message: payload.message || '罗盘完整采集失败',
        sessionKey: payload.sessionKey || '', targetDate, targetShopName
      }), 0);
    }
  };
  const emitCaptureEvent = (event, payload) => {
    if (!captureMode || !captureId) return;
    try {
      window.__TAURI_INTERNALS__?.invoke?.('plugin:event|emit', { event, payload });
    } catch (_) {}
  };
  const sensitiveKey = (key) => {
    const normalized = String(key || '').toLowerCase().replaceAll('-', '').replaceAll('_', '');
    return ['cookie', 'setcookie', 'authorization', 'accesstoken', 'refreshtoken', 'mstoken',
      'verifyfp', 'fp', 'abogus', 'signature', 'sign', 'mobile', 'phone', 'phonenumber',
      'address', 'receiver', 'receivername', 'receiverphone', 'idcard', 'openid'].includes(normalized)
      || normalized.endsWith('token');
  };
  const redactJson = (value, seen = new WeakSet()) => {
    if (!value || typeof value !== 'object') return value;
    if (seen.has(value)) return '[CIRCULAR]';
    seen.add(value);
    if (Array.isArray(value)) return value.map((item) => redactJson(item, seen));
    const result = {};
    for (const [key, child] of Object.entries(value)) {
      result[key] = sensitiveKey(key) ? '[REDACTED]' : redactJson(child, seen);
    }
    return result;
  };
  const sanitizeUrl = (value) => {
    try {
      const parsed = new URL(String(value), location.origin);
      for (const key of [...parsed.searchParams.keys()]) {
        if (sensitiveKey(key)) parsed.searchParams.delete(key);
      }
      parsed.hash = '';
      return parsed.href;
    } catch (_) {
      return String(value || '').split('?')[0];
    }
  };
  const capturePathAllowed = (value) => {
    try {
      const parsed = new URL(String(value), location.origin);
      if (parsed.hostname !== 'jinritemai.com' && !parsed.hostname.endsWith('.jinritemai.com')) return false;
      return ['/compass_api/', '/business_api/', '/compassapi/', '/compass_app/',
        '/strategy_api/', '/square_pc_api/', '/comment_api/']
        .some((prefix) => parsed.pathname.startsWith(prefix));
    } catch (_) { return false; }
  };
  let captureResponseSequence = 0;
  const captureResponse = async ({ url, method, status, contentType, bodyText, requestPostData }) => {
    if (window.__BSR_NATIVE_CDP_CAPTURE__ || !captureMode || !capturePathAllowed(url) || !bodyText) return;
    let body;
    try { body = JSON.parse(bodyText); } catch (_) { return; }
    const redacted = JSON.stringify(redactJson(body));
    const responseId = `${Date.now().toString(36)}-${(++captureResponseSequence).toString(36)}`;
    const chunkSize = 96 * 1024;
    const total = Math.max(1, Math.ceil(redacted.length / chunkSize));
    const meta = {
      url: sanitizeUrl(url), method: String(method || 'GET').toUpperCase(), status: Number(status || 0),
      contentType: String(contentType || ''), capturedAt: new Date().toISOString(),
      pagePath: `${location.pathname}${location.hash}`, sessionKey: state?.activeKey || '',
      targetDate, targetShopName, metricLabel: state?.captureMetricLabel || '',
      sectionKey: state?.captureSectionKey || '', sectionLabel: state?.captureSectionLabel || '',
      productPage: state?.capturePage || 0,
      requestPostData: typeof requestPostData === 'string' ? requestPostData : ''
    };
    for (let sequence = 0; sequence < total; sequence += 1) {
      emitCaptureEvent('compass-capture-chunk', {
        captureId, responseId, sequence, total,
        meta: sequence === 0 ? meta : {},
        chunk: redacted.slice(sequence * chunkSize, (sequence + 1) * chunkSize)
      });
      if (sequence > 0 && sequence % 8 === 0) await new Promise((resolve) => setTimeout(resolve, 20));
    }
  };
  if (captureMode && !window.__BSR_COMPASS_CAPTURE_INSTALLED__) {
    window.__BSR_COMPASS_CAPTURE_INSTALLED__ = true;
    const originalFetch = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await originalFetch(...args);
      try {
        const request = args[0];
        const url = typeof request === 'string' ? request : request?.url;
        const method = args[1]?.method || request?.method || 'GET';
        const requestPostData = typeof args[1]?.body === 'string' ? args[1].body : '';
        if (capturePathAllowed(url)) {
          const clone = response.clone();
          void clone.text().then((bodyText) => captureResponse({
            url, method, status: clone.status,
            contentType: clone.headers.get('content-type') || '', bodyText, requestPostData
          })).catch(() => {});
        }
      } catch (_) {}
      return response;
    };
    const OriginalXhr = window.XMLHttpRequest;
    const originalOpen = OriginalXhr.prototype.open;
    const originalSend = OriginalXhr.prototype.send;
    OriginalXhr.prototype.open = function(method, url, ...rest) {
      this.__bsrCaptureRequest = { method, url, requestPostData: '' };
      return originalOpen.call(this, method, url, ...rest);
    };
    OriginalXhr.prototype.send = function(...args) {
      if (this.__bsrCaptureRequest && typeof args[0] === 'string') {
        this.__bsrCaptureRequest.requestPostData = args[0];
      }
      if (capturePathAllowed(this.__bsrCaptureRequest?.url)) {
        this.addEventListener('load', () => {
          try {
            if (!this.responseType || this.responseType === 'text') {
              void captureResponse({
                url: this.__bsrCaptureRequest.url, method: this.__bsrCaptureRequest.method,
                status: this.status, contentType: this.getResponseHeader('content-type') || '',
                bodyText: this.responseText, requestPostData: this.__bsrCaptureRequest.requestPostData
              });
            } else if (this.responseType === 'json' && this.response) {
              void captureResponse({
                url: this.__bsrCaptureRequest.url, method: this.__bsrCaptureRequest.method,
                status: this.status, contentType: this.getResponseHeader('content-type') || '',
                bodyText: JSON.stringify(this.response), requestPostData: this.__bsrCaptureRequest.requestPostData
              });
            }
          } catch (_) {}
        }, { once: true });
      }
      return originalSend.apply(this, args);
    };
  }
  const visible = (node) => node && node.getClientRects().length > 0;
  const text = (node) => (node?.innerText || node?.textContent || '').replace(/\s+/g, ' ').trim();
  const exactTextNodes = (label, root = document) => Array.from(root.querySelectorAll('button,a,label,[role="button"],span,div'))
    .filter((node) => visible(node) && text(node) === label);
  const exactText = (label, root = document) => exactTextNodes(label, root)[0];
  const clickTarget = (node) => {
    const target = node?.closest?.('button,a,label,[role="button"]') || node;
    if (!target) return false;
    const eventInit = { bubbles: true, cancelable: true, view: window };
    try { target.dispatchEvent(new PointerEvent('pointerdown', eventInit)); } catch (_) {}
    target.dispatchEvent(new MouseEvent('mousedown', eventInit));
    try { target.dispatchEvent(new PointerEvent('pointerup', eventInit)); } catch (_) {}
    target.dispatchEvent(new MouseEvent('mouseup', eventInit));
    target.click();
    return true;
  };
  const findHeaderShopTrigger = () => {
    const accountTrigger = document.querySelector(
      '[class*="userSection-"] .ecom-dropdown-trigger[class*="userDropDown-"]'
    ) || document.querySelector('.ecom-dropdown-trigger [class*="userName-"]')?.closest('.ecom-dropdown-trigger');
    if (visible(accountTrigger)) return accountTrigger;
    const candidates = allowedShopNames.flatMap((shopName) => exactTextNodes(shopName));
    candidates.sort((left, right) => {
      const a = left.getBoundingClientRect();
      const b = right.getBoundingClientRect();
      return a.top - b.top || b.right - a.right || (a.width * a.height) - (b.width * b.height);
    });
    const label = candidates[0];
    if (!label) return null;
    let node = label;
    for (let depth = 0; node && depth < 6; depth += 1, node = node.parentElement) {
      const role = node.getAttribute?.('role') || '';
      const cursor = getComputedStyle(node).cursor;
      if (node.matches?.('button,a,label,[role="button"]') || role === 'button' || cursor === 'pointer') return node;
    }
    return label;
  };
  const findShopChoiceCard = (shopName) => {
    const labels = exactTextNodes(shopName);
    for (const label of labels) {
      let node = label;
      for (let depth = 0; node && depth < 6; depth += 1, node = node.parentElement) {
        const rect = node.getBoundingClientRect();
        if (rect.width >= 250 && rect.height >= 55 && rect.height <= 150) return node;
      }
    }
    return labels[0] || null;
  };
  const headerShowsShop = (shopName) => exactTextNodes(shopName)
    .some((node) => node.getBoundingClientRect().top < 120);
  const findTopNavigation = (label) => {
    const nodes = exactTextNodes(label)
      .filter((node) => node.getBoundingClientRect().top < 120)
      .sort((left, right) => left.getBoundingClientRect().top - right.getBoundingClientRect().top);
    const node = nodes[0];
    return node?.closest?.('button,a,label,[role="button"]') || node || null;
  };
  const hoverTarget = (node) => {
    const target = node?.closest?.('label,button,a,[role="button"]') || node;
    if (!target) return false;
    target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, view: window }));
    target.dispatchEvent(new MouseEvent('mouseenter', { bubbles: false, view: window }));
    target.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, view: window }));
    return true;
  };
  const datePattern = new RegExp(`${targetDate.replaceAll('-', '[-/]')}\\s+(\\d{2}:\\d{2})\\s*[~～-]\\s*(?:(20\\d{2}[-/]\\d{1,2}[-/]\\d{1,2})\\s+)?(\\d{2}:\\d{2})`);
  const normalizeDate = (value) => {
    const match = String(value || targetDate).match(/(20\\d{2})[-/](\\d{1,2})[-/](\\d{1,2})/);
    if (!match) return targetDate;
    return `${match[1]}-${String(match[2]).padStart(2, '0')}-${String(match[3]).padStart(2, '0')}`;
  };
  const normalizeTime = (clock, date = targetDate) => `${normalizeDate(date)}T${clock}:00+08:00`;
  const compassClock = (value) => {
    const match = String(value || '').match(/(20\d{2})[-/](\d{1,2})[-/](\d{1,2})[T ](\d{1,2}):(\d{2})/);
    if (!match) return '';
    return `${match[1]}-${String(match[2]).padStart(2, '0')}-${String(match[3]).padStart(2, '0')}T${String(match[4]).padStart(2, '0')}:${match[5]}`;
  };
  const sessionContainsStartedAt = (session, startedAt) => {
    const wanted = compassClock(startedAt);
    const sessionStart = compassClock(session?.startedAt);
    const sessionEnd = compassClock(session?.endedAt);
    return Boolean(wanted && sessionStart && sessionEnd
      && sessionStart <= wanted && wanted <= sessionEnd);
  };
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
      const endedAt = normalizeTime(match[3], match[2]);
      const firstLine = (row.innerText || '').split('\n').map((part) => part.trim()).find(Boolean) || '抖音直播间';
      const key = `${targetShopName}|${startedAt}|${endedAt}`;
      if (unique.has(key)) continue;
      const orderMatch = value.match(/(?:成交订单数)?\s*(\d+)\s*(?:净成交订单数|诊断|详情)/);
      const amountMatch = value.match(/¥[\d,.]+(?:万)?/g);
      unique.set(key, {
        key, row, shopName: targetShopName, title: firstLine, startedAt, endedAt,
        orderCount: orderMatch ? Number(orderMatch[1]) : null,
        paymentAmountText: amountMatch?.at(-1) || ''
      });
    }
    return [...unique.values()].sort((left, right) => left.startedAt.localeCompare(right.startedAt));
  };
  const findSessionAction = (row, labels = ['诊断', '详情']) => {
    for (const label of labels) {
      const inside = exactText(label, row);
      if (inside) return inside.closest?.('button,a,[role="button"]') || inside;
    }
    const identityAttributes = ['data-row-key', 'data-key', 'aria-rowindex'];
    const identities = identityAttributes
      .map((attribute) => [attribute, row?.getAttribute?.(attribute)])
      .filter(([, value]) => value != null && value !== '');
    if (identities.length) {
      const twins = Array.from(document.querySelectorAll('tr,[role="row"]'))
        .filter((candidate) => candidate !== row
          && identities.some(([attribute, value]) => candidate.getAttribute(attribute) === value));
      for (const twin of twins) {
        for (const label of labels) {
          const action = exactText(label, twin);
          if (action) return action.closest?.('button,a,[role="button"]') || action;
        }
      }
    }
    const rowRect = row?.getBoundingClientRect?.();
    if (!rowRect) return null;
    const actions = labels.flatMap((label) => exactTextNodes(label))
      .map((node) => node.closest?.('button,a,[role="button"]') || node)
      .filter((node, index, items) => visible(node) && items.indexOf(node) === index)
      .map((node) => ({ node, rect: node.getBoundingClientRect() }))
      .filter(({ rect }) => rect.bottom >= rowRect.top - 4 && rect.top <= rowRect.bottom + 4)
      .sort((left, right) => {
        const rowCenter = rowRect.top + rowRect.height / 2;
        const leftDistance = Math.abs(left.rect.top + left.rect.height / 2 - rowCenter);
        const rightDistance = Math.abs(right.rect.top + right.rect.height / 2 - rowCenter);
        return leftDistance - rightDistance || right.rect.right - left.rect.right;
      });
    return actions[0]?.node || null;
  };
  const state = (() => {
    try {
      const saved = JSON.parse(localStorage.getItem(storageKey) || '{}');
      if (saved.targetDate === targetDate && saved.targetShopName === targetShopName
        && saved.targetStartedAt === targetStartedAt
        && (!captureMode || saved.captureId === captureId)) return saved;
    } catch (_) {}
    return {
      targetDate, targetShopName, targetStartedAt, captureId, completed: [], activeKey: null, shopConfirmed: false,
      captureStarted: false, captureMetricIndex: 0, capturePage: 1, captureDetailIndex: 0,
      captureResponsesHint: 0, captureMetricLabel: '', captureSectionKey: '', captureSectionLabel: '',
      captureProductsCompleted: false, captureProductDetailFound: false,
      captureNavigationIndex: 0, capturePendingTarget: null, captureListUrl: ''
    };
  })();
  const save = () => localStorage.setItem(storageKey, JSON.stringify(state));
  const captureControl = (status, message, sessionKey = state.activeKey || '', section = null) => {
    if (section?.key) {
      state.captureSectionKey = section.key;
      state.captureSectionLabel = section.label || section.key;
      save();
    }
    emitCaptureEvent('compass-capture-control', {
      captureId, status, message, sessionKey, targetDate, targetShopName,
      metricLabel: state.captureMetricLabel || '', productPage: state.capturePage || 0,
      sectionKey: section?.key || state.captureSectionKey || '',
      sectionLabel: section?.label || state.captureSectionLabel || '',
      sectionKind: section?.kind || '', sectionStatus: section?.status || ''
    });
  };
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
  let customMenuOpened = false;
  let selectionPending = false;
  let navigationPending = false;
  let detailDownloadTriggered = false;
  let shopSelectionPending = false;
  let shopChoicePending = false;
  let liveNavigationPending = false;
  let captureActionPending = false;
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
  const findExactDateCell = () => {
    const titles = [targetDate, targetDate.replaceAll('-', '/')];
    return titles
      .flatMap((title) => Array.from(document.querySelectorAll(`td[title="${title}"]`)))
      .find((node) => visible(node) && !/disabled/i.test(node.className || ''));
  };
  const confirmDateFilter = () => clickTarget(exactText('确定')) || clickTarget(exactText('查询'));
  const markFilterApplied = () => {
    filterApplied = true;
    selectionPending = false;
    emit({ status: 'filtering', message: `已选择 ${targetDate}，等待场次列表刷新` });
    setTimeout(() => {
      if (!findSessionRows().length) {
        filterApplied = false;
        customMenuOpened = false;
        emit({ status: 'filtering', message: `未刷出 ${targetDate} 的场次，正在重试选日期` });
      }
    }, 5000);
  };
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
  const findCustomDateControl = () => {
    const dropdownTrigger = document.querySelector(
      '[data-node-key="custom"] .aurora-dropdown-trigger'
    );
    if (visible(dropdownTrigger)) return dropdownTrigger;
    const customTab = document.querySelector('[data-node-key="custom"] [role="tab"]');
    if (visible(customTab)) return customTab;
    const radio = document.querySelector('input[type="radio"][value="custom"]');
    if (radio) return radio.closest('label') || radio;
    return exactText('自定义');
  };
  const revealDateFilter = () => {
    try { window.scrollTo({ top: 0, left: 0, behavior: 'instant' }); } catch (_) { window.scrollTo(0, 0); }
    const scrollingElement = document.scrollingElement;
    if (scrollingElement) scrollingElement.scrollTop = 0;
    Array.from(document.querySelectorAll('main,section,div'))
      .filter((node) => node.scrollHeight > node.clientHeight + 80
        && /(auto|scroll)/.test(getComputedStyle(node).overflowY))
      .forEach((node) => { node.scrollTop = 0; });
  };
  const requestDateFilter = () => {
    if (filterApplied || selectionPending) return true;
    if (!customMenuOpened) {
      const custom = findCustomDateControl();
      if (!custom) {
        revealDateFilter();
        emit({ status: 'filtering', message: '正在回到列表顶部并寻找“自定义”日期入口' });
        return false;
      }
      hoverTarget(custom);
      customMenuOpened = true;
      setTimeout(() => {
        if (!visibleMonthHeaders().length) customMenuOpened = false;
      }, 1600);
      emit({ status: 'filtering', message: `正在悬停“自定义”，准备选择 ${targetDate}` });
      return true;
    }
    const headers = visibleMonthHeaders();
    if (!headers.length) {
      customMenuOpened = false;
      emit({ status: 'filtering', message: '自定义日期面板未打开，正在重试' });
      return true;
    }
    const exactDateCell = findExactDateCell();
    if (exactDateCell) {
      selectionPending = true;
      const selectEndDate = () => {
        const endDateCell = findExactDateCell();
        if (!endDateCell) {
          selectionPending = false;
          emit({ status: 'filtering', message: `日期面板已关闭，正在重试选择 ${targetDate}` });
          return;
        }
        clickTarget(endDateCell);
        setTimeout(() => {
          confirmDateFilter();
          markFilterApplied();
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
      setTimeout(() => {
        confirmDateFilter();
        markFilterApplied();
      }, 300);
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
    let matchingSessions = sessions;
    if (targetStartedAt) {
      const wanted = compassClock(targetStartedAt);
      matchingSessions = sessions.filter((session) => sessionContainsStartedAt(session, targetStartedAt));
      if (!matchingSessions.length) {
        emit({ status: 'failed', message: `未找到覆盖 ${targetShopName} ${wanted || targetStartedAt} 的整场直播`, sessions: sessions.map(({ row, ...session }) => session) });
        return;
      }
    }
    emit({ status: 'sessions-found', message: `${targetShopName}：找到 ${matchingSessions.length} 场直播`, sessions: matchingSessions.map(({ row, ...session }) => session) });
    if (!autoStart) return;
    if (navigationPending) return;
    const next = matchingSessions.find((session) => !state.completed.includes(session.key));
    if (!next) {
      const message = captureMode
        ? `${targetShopName} 当天 ${matchingSessions.length} 场直播已完成完整采集`
        : `${targetShopName} 当天 ${matchingSessions.length} 场直播已全部触发下载`;
      if (captureMode) captureControl('completed', message, '');
      emit({ status: 'batch-finished', message, captureId: captureMode ? captureId : undefined, sessions: matchingSessions.map(({ row, ...session }) => session) });
      return;
    }
    state.activeKey = next.key;
    state.captureListUrl = location.href;
    state.captureStarted = false;
    state.captureMetricIndex = 0;
    state.capturePage = 1;
    state.captureDetailIndex = 0;
    state.captureMetricLabel = '';
    state.captureSectionKey = '';
    state.captureSectionLabel = '';
    state.captureProductsCompleted = false;
    state.captureProductDetailFound = false;
    state.captureNavigationIndex = 0;
    state.capturePendingTarget = null;
    save();
    const diagnose = findSessionAction(next.row);
    navigationPending = true;
    const diagnoseLink = diagnose?.closest?.('a') || (diagnose?.matches?.('a') ? diagnose : null);
    const diagnosePath = diagnoseLink?.getAttribute('data-subway-href')
      || diagnoseLink?.getAttribute('href');
    if (diagnosePath) {
      const detailUrl = new URL(diagnosePath, location.origin);
      detailUrl.searchParams.set('tab', 'diagnosis');
      emit({ status: 'opening', sessionKey: next.key, message: captureMode ? '正在打开直播详情，准备完整采集' : '正在打开直播诊断详情' });
      setTimeout(() => location.assign(detailUrl.href), 120);
      return;
    } else if (!clickTarget(diagnose)) {
      navigationPending = false;
      emit({ status: 'failed', sessionKey: next.key, message: '未找到该场次右侧固定列的“诊断/详情”按钮，请手动进入详情' });
      return;
    }
    emit({ status: 'opening', sessionKey: next.key, message: captureMode ? '正在打开直播详情，准备完整采集' : '正在打开直播诊断详情' });
    const sourceUrl = location.href;
    setTimeout(() => {
      if (location.href !== sourceUrl || !document.documentElement.contains(diagnose)) return;
      navigationPending = false;
      emit({ status: 'opening', sessionKey: next.key, message: '详情按钮尚未响应，正在重新定位右侧固定操作列' });
    }, 3500);
  };
  const professionalScreenUrl = () => {
    const current = new URL(location.href);
    let roomId = current.searchParams.get('live_room_id') || current.searchParams.get('room_id');
    let appId = current.searchParams.get('live_app_id') || '1128';
    if (!roomId) {
      const linked = Array.from(document.querySelectorAll('a[href], [data-subway-href]'))
        .map((node) => node.getAttribute('href') || node.getAttribute('data-subway-href') || '')
        .find((value) => /live_room_id=\d+/.test(value));
      if (linked) {
        const parsed = new URL(linked, location.origin);
        roomId = parsed.searchParams.get('live_room_id');
        appId = parsed.searchParams.get('live_app_id') || appId;
      }
    }
    if (!roomId) return null;
    return `${location.origin}/screen/live/shop?live_room_id=${encodeURIComponent(roomId)}&live_app_id=${encodeURIComponent(appId)}&source=compass-live-detail#data`;
  };
  const captureMetricTargets = [
    { key: 'metric.deal_amount', label: '成交金额' },
    { key: 'metric.ad_spend', label: '投放消耗' },
    { key: 'metric.exposure_view', label: '曝光-观看率' },
    { key: 'metric.online', label: '在线人数' },
    { key: 'metric.watch_duration', label: '人均观看时长' },
    { key: 'metric.interaction', label: '互动率' },
    { key: 'metric.follow', label: '关注率' },
    { key: 'metric.negative_rate', label: '负反馈率' },
    { key: 'metric.negative_count', label: '负反馈次数' },
    { key: 'metric.gpm', label: '千次观看用户支付金额' },
    { key: 'metric.product_click', label: '商品点击率' },
    { key: 'metric.product_conversion', label: '商品点击-成交率' },
    { key: 'metric.user_payment', label: '用户支付金额' }
  ];
  const captureReadOnlyTargets = [
    { key: 'tab.traffic', label: '流量分析', kind: 'tab' },
    { key: 'tab.short_video', label: '引流短视频', kind: 'tab' },
    { key: 'tab.diagnosis', label: '流量诊断', kind: 'tab' },
    { key: 'tab.host', label: '主播分析', kind: 'tab' },
    { key: 'tab.violation', label: '违规情况', kind: 'tab' },
    { key: 'tab.trend', label: '综合趋势', kind: 'tab' },
    { key: 'module.product', label: '商品', kind: 'module' },
    { key: 'module.audience', label: '人群', kind: 'module' },
    { key: 'module.qianchuan', label: '千川', kind: 'module' },
    { key: 'module.data', label: '数据', kind: 'module' }
  ];
  const capturePacing = Object.freeze({
    settle: [2500, 4000],
    metric: [3000, 5000],
    product: [4000, 6000],
    page: [6000, 9000],
    navigation: [4500, 7000],
    session: [10000, 15000],
    unavailable: [800, 1400]
  });
  const pacedDelay = (kind) => {
    const [minimum, maximum] = capturePacing[kind] || capturePacing.settle;
    return Math.round(minimum + Math.random() * Math.max(0, maximum - minimum));
  };
  const preferredReadOnlyTarget = (target) => {
    const candidates = exactTextNodes(target.label)
      .map((node) => node.closest('button,a,[role="tab"],[role="menuitem"],[role="button"]') || node)
      .filter((node, index, items) => visible(node) && items.indexOf(node) === index);
    const score = (node) => {
      const role = node.getAttribute?.('role') || '';
      const ancestry = String(node.closest?.('aside,nav,[role="navigation"],[class*="sidebar"],[class*="sider"],[class*="menu"]')?.className || '');
      if (target.kind === 'tab') return (role === 'tab' ? 100 : 0) + (node.tagName === 'BUTTON' ? 30 : 0);
      return (ancestry ? 100 : 0) + (role === 'menuitem' ? 40 : 0) + (node.tagName === 'A' ? 20 : 0);
    };
    return candidates.sort((left, right) => score(right) - score(left))[0] || null;
  };
  const closeProductDetail = () => {
    const candidates = Array.from(document.querySelectorAll(
      'button[aria-label="Close"],button[aria-label="close"],.ecom-modal-close,.ecom-drawer-close,[class*="modal-close"],[class*="drawer-close"]'
    )).filter(visible);
    return clickTarget(candidates[0]);
  };
  const productRows = () => Array.from(document.querySelectorAll('tr[data-row-key]')).filter(visible);
  const explainedProductRows = () => productRows().filter((row) => /讲解\s*[1-9]\d*\s*次/.test(text(row)));
  const processProfessionalCapture = () => {
    if (!captureMode || captureActionPending) return;
    if (state.capturePendingTarget) {
      const finished = state.capturePendingTarget;
      state.capturePendingTarget = null;
      save();
      const permissionDenied = /暂无权限|没有权限|无访问权限|权限不足/.test(text(document.body));
      captureControl('running', permissionDenied ? `无权访问只读页面：${finished.label}` : `已触发只读页面：${finished.label}`, state.activeKey, {
        ...finished, status: permissionDenied ? 'permission_denied' : 'triggered'
      });
      emit({ status: 'capturing-section', sessionKey: state.activeKey, captureId, message: permissionDenied ? `当前账号无权访问：${finished.label}` : `已触发页面并等待响应：${finished.label}` });
      captureActionPending = true;
      setTimeout(() => { captureActionPending = false; run(); }, pacedDelay('settle'));
      return;
    }
    if (!state.captureStarted) {
      state.captureStarted = true;
      save();
      const section = { key: 'module.data', label: '数据', kind: 'module', status: 'capturing' };
      captureControl('running', `开始采集 ${targetShopName} ${targetDate} 的罗盘完整数据`, state.activeKey, section);
      emit({ status: 'capture-started', sessionKey: state.activeKey, captureId, message: '已进入直播大屏，正在抓取接口数据' });
      captureActionPending = true;
      setTimeout(() => {
        captureControl('running', '数据首页已触发，等待响应确认', state.activeKey, { ...section, status: 'triggered' });
        captureActionPending = false;
        run();
      }, pacedDelay('settle'));
      return;
    }
    if (state.captureMetricIndex < captureMetricTargets.length) {
      const target = captureMetricTargets[state.captureMetricIndex];
      const metric = exactText(target.label);
      state.captureMetricLabel = target.label;
      state.captureMetricIndex += 1;
      save();
      captureControl('running', `正在采集指标：${target.label}`, state.activeKey, {
        ...target, kind: 'metric', status: metric ? 'capturing' : 'unavailable'
      });
      if (metric) clickTarget(metric);
      emit({
        status: 'capturing-metric', sessionKey: state.activeKey, captureId,
        step: state.captureMetricIndex, totalSteps: captureMetricTargets.length + captureReadOnlyTargets.length,
        message: metric ? `正在采集指标：${target.label}` : `当前账号未提供指标：${target.label}`
      });
      captureActionPending = true;
      setTimeout(() => {
        if (metric) {
            captureControl('running', `已触发指标：${target.label}`, state.activeKey, {
              ...target, kind: 'metric', status: 'triggered'
          });
        }
        captureActionPending = false;
        run();
      }, pacedDelay(metric ? 'metric' : 'unavailable'));
      return;
    }

    if (!state.captureProductsCompleted) {
      const rows = explainedProductRows();
      state.captureMetricLabel = '';
      if (state.captureDetailIndex < rows.length) {
        closeProductDetail();
        const row = rows[state.captureDetailIndex];
        const detail = exactText('查看详情', row);
        state.captureDetailIndex += 1;
        state.captureProductDetailFound ||= Boolean(detail);
        save();
        captureControl('running', `正在采集第 ${state.capturePage} 页商品详情`, state.activeKey, {
          key: 'product.detail', label: '有讲解商品详情', kind: 'dataset',
          status: detail ? 'capturing' : 'unavailable'
        });
        if (detail) clickTarget(detail);
        emit({
          status: 'capturing-product-detail', sessionKey: state.activeKey, captureId,
          message: `正在采集第 ${state.capturePage} 页有讲解商品详情（${state.captureDetailIndex}/${rows.length}）`
        });
        captureActionPending = true;
        setTimeout(() => {
          closeProductDetail();
          if (detail) {
            captureControl('running', '已触发有讲解商品详情', state.activeKey, {
              key: 'product.detail', label: '有讲解商品详情', kind: 'dataset', status: 'triggered'
            });
          }
          captureActionPending = false;
          run();
        }, pacedDelay(detail ? 'product' : 'unavailable'));
        return;
      }

      const nextPage = document.querySelector('li[title="下一页"]');
      const canContinue = nextPage && visible(nextPage)
        && nextPage.getAttribute('aria-disabled') !== 'true'
        && !/disabled/i.test(nextPage.className || '');
      if (canContinue) {
        state.capturePage += 1;
        state.captureDetailIndex = 0;
        save();
        captureControl('running', `正在采集商品列表第 ${state.capturePage} 页`, state.activeKey, {
          key: 'product.list', label: '商品列表与分页', kind: 'dataset', status: 'capturing'
        });
        clickTarget(nextPage.querySelector('button,a') || nextPage);
        emit({
          status: 'capturing-products', sessionKey: state.activeKey, captureId,
          message: `正在采集商品列表第 ${state.capturePage} 页`
        });
        captureActionPending = true;
        setTimeout(() => { captureActionPending = false; run(); }, pacedDelay('page'));
        return;
      }

      state.captureProductsCompleted = true;
      save();
      captureControl('running', '商品列表和详情采集阶段完成', state.activeKey, {
        key: 'product.list', label: '商品列表与分页', kind: 'dataset',
        status: productRows().length || state.capturePage > 1 ? 'triggered' : 'unavailable'
      });
      captureControl('running', '商品详情采集阶段完成', state.activeKey, {
        key: 'product.detail', label: '有讲解商品详情', kind: 'dataset',
        status: state.captureProductDetailFound ? 'triggered' : 'unavailable'
      });
      captureActionPending = true;
      setTimeout(() => { captureActionPending = false; run(); }, pacedDelay('settle'));
      return;
    }

    if (state.captureNavigationIndex < captureReadOnlyTargets.length) {
      const target = captureReadOnlyTargets[state.captureNavigationIndex];
      const node = preferredReadOnlyTarget(target);
      state.captureNavigationIndex += 1;
      if (node) state.capturePendingTarget = target;
      save();
      captureControl('running', node ? `正在打开只读页面：${target.label}` : `当前账号未提供页面：${target.label}`, state.activeKey, {
        ...target, status: node ? 'capturing' : 'unavailable'
      });
      emit({
        status: 'capturing-section', sessionKey: state.activeKey, captureId,
        step: captureMetricTargets.length + state.captureNavigationIndex,
        totalSteps: captureMetricTargets.length + captureReadOnlyTargets.length,
        message: node ? `正在采集：${target.label}` : `当前账号未提供：${target.label}`
      });
      if (node) clickTarget(node);
      captureActionPending = true;
      setTimeout(() => { captureActionPending = false; run(); }, pacedDelay(node ? 'navigation' : 'unavailable'));
      return;
    }

    if (!state.completed.includes(state.activeKey)) state.completed.push(state.activeKey);
    const completedKey = state.activeKey;
    save();
    captureControl('running', '当前场次采集完成，准备处理下一场', completedKey);
    emit({ status: 'capture-session-finished', sessionKey: completedKey, captureId, message: '当前场次完整采集完成' });
    captureActionPending = true;
    setTimeout(() => {
      if (state.captureListUrl) location.assign(state.captureListUrl);
      else history.back();
    }, pacedDelay('session'));
  };
  const processDetail = () => {
    navigationPending = false;
    if (captureMode) {
      if (state.activeKey && state.completed.includes(state.activeKey)) {
        setTimeout(() => {
          if (state.captureListUrl) location.assign(state.captureListUrl);
          else history.back();
        }, 300);
        return;
      }
      const screenUrl = professionalScreenUrl();
      if (screenUrl) {
        emit({ status: 'opening-live-screen', sessionKey: state.activeKey, captureId, message: '正在进入直播大屏专业版' });
        setTimeout(() => location.assign(screenUrl), 150);
        return;
      }
      const entry = exactText('直播大屏') || exactText('进入直播大屏') || exactText('查看直播大屏');
      if (clickTarget(entry)) {
        emit({ status: 'opening-live-screen', sessionKey: state.activeKey, captureId, message: '正在打开直播大屏专业版' });
      } else {
        emit({ status: 'failed', sessionKey: state.activeKey, captureId, message: '未识别到直播大屏入口或直播间 ID，请在罗盘详情中打开直播大屏后重试' });
      }
      return;
    }
    if (detailDownloadTriggered) return;
    const activeStartedAt = state.activeKey?.split('|').at(-2) || '';
    const activeEndedAt = state.activeKey?.split('|').at(-1) || '';
    if (!activeStartedAt || (targetStartedAt && !sessionContainsStartedAt(
      { startedAt: activeStartedAt, endedAt: activeEndedAt }, targetStartedAt
    ))) {
      emit({ status: 'failed', sessionKey: state.activeKey, message: '当前详情不是目标场次，已停止下载，请返回列表重新匹配' });
      return;
    }
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
    if (location.hostname !== 'jinritemai.com' && !location.hostname.endsWith('.jinritemai.com')) return;
    const pageText = text(document.body);
    if (pageText.includes('请选择店铺')) {
      shopSelectionPending = false;
      const targetShop = findShopChoiceCard(targetShopName);
      if (!targetShop) {
        emit({ status: 'shop-unavailable', message: `当前账号无“${targetShopName}”店铺权限` });
        return;
      }
      if (!shopChoicePending) {
        shopChoicePending = true;
        state.shopConfirmed = true;
        save();
        clickTarget(targetShop);
        emit({ status: 'switching-shop', message: `正在进入${targetShopName}` });
        setTimeout(() => {
          shopChoicePending = false;
          run();
        }, 1000);
      }
      return;
    }
    const pageKind = location.pathname.startsWith('/screen/live/') || document.title.includes('直播大屏')
      || (captureMode && state.captureStarted && ['综合趋势', '流量分析', '引流短视频', '主播分析']
        .some((label) => pageText.includes(label)))
      ? 'professional'
      : pageText.includes('直播间详情') ? 'detail'
      : pageText.includes('直播间列表') ? 'list'
      : 'other';
    if (pageKind === 'professional') {
      processProfessionalCapture();
      return;
    }
    if (pageKind === 'other' && headerShowsShop(targetShopName)) {
      if (!state.shopConfirmed) {
        state.shopConfirmed = true;
        save();
      }
      if (liveNavigationPending) return;
      const liveList = exactText('直播间列表');
      const liveNavigation = findTopNavigation('直播');
      liveNavigationPending = true;
      if (liveList && clickTarget(liveList)) {
        emit({ status: 'opening', message: '正在进入直播间列表' });
      } else if (liveNavigation) {
        hoverTarget(liveNavigation);
        clickTarget(liveNavigation);
        emit({ status: 'opening', message: '正在打开顶部“直播”菜单' });
      } else {
        liveNavigationPending = false;
        emit({ status: 'shop-mismatch', message: '店铺已切换，但未找到顶部“直播”入口' });
        return;
      }
      setTimeout(() => {
        liveNavigationPending = false;
        run();
      }, 900);
      return;
    }
    if (pageKind === 'list' || pageKind === 'detail') {
      const visibleTargetShop = exactText(targetShopName);
      const visibleOtherShop = allowedShopNames
        .filter((shopName) => shopName !== targetShopName)
        .map((shopName) => ({ shopName, node: exactText(shopName) }))
        .find((item) => item.node);
      if (visibleTargetShop && !state.shopConfirmed) {
        state.shopConfirmed = true;
        save();
      }
      if (visibleOtherShop || !state.shopConfirmed) {
        const currentShop = visibleOtherShop?.shopName || '上次使用的店铺';
        if (shopSelectionPending) return;
        const switchDataView = exactText('切换数据视角');
        shopSelectionPending = true;
        if (switchDataView) {
          clickTarget(switchDataView);
          emit({ status: 'switching-shop', message: `账号菜单已打开，正在从${currentShop}切换到${targetShopName}` });
          setTimeout(() => { shopSelectionPending = false; run(); }, 1200);
          return;
        }
        const currentShopEntry = findHeaderShopTrigger();
        if (hoverTarget(currentShopEntry)) {
          emit({ status: 'switching-shop', message: '正在悬停当前账号，打开“切换数据视角”菜单' });
          setTimeout(() => { shopSelectionPending = false; run(); }, 900);
          return;
        }
        shopSelectionPending = false;
        emit({ status: 'shop-mismatch', message: `未找到“切换数据视角”，请确认右上角店铺菜单可用` });
        return;
      }
    }
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

#[cfg(all(feature = "gui", target_os = "windows"))]
#[derive(Debug, Clone)]
struct CompassCdpRequestContext {
    url: String,
    method: String,
    post_data: Option<String>,
}

#[cfg(all(feature = "gui", target_os = "windows"))]
fn compass_cdp_url_allowed(value: &str) -> bool {
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    if !parsed
        .host_str()
        .is_some_and(|host| host == "jinritemai.com" || host.ends_with(".jinritemai.com"))
    {
        return false;
    }
    [
        "/compass_api/",
        "/business_api/",
        "/compassapi/",
        "/compass_app/",
        "/strategy_api/",
        "/square_pc_api/",
        "/comment_api/",
    ]
    .iter()
    .any(|prefix| parsed.path().starts_with(prefix))
}

#[cfg(all(feature = "gui", target_os = "windows"))]
fn install_compass_cdp_capture(
    webview: &tauri::Webview,
    app: &tauri::AppHandle,
    capture_id: &str,
    target_date: &str,
    target_shop_name: &str,
) -> Result<(), String> {
    use base64::Engine as _;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use webview2_com::{
        CallDevToolsProtocolMethodCompletedHandler, CoTaskMemPWSTR,
        DevToolsProtocolEventReceivedEventHandler,
    };
    use windows::core::PWSTR;

    let capture_id = capture_id.to_string();
    let target_date = target_date.to_string();
    let target_shop_name = target_shop_name.to_string();
    let app = app.clone();
    let requests = Arc::new(Mutex::new(
        HashMap::<String, CompassCdpRequestContext>::new(),
    ));
    let unsupported_websockets = Arc::new(Mutex::new(HashSet::<String>::new()));
    let install_error = Arc::new(Mutex::new(None::<String>));
    let install_error_for_webview = install_error.clone();

    webview
        .with_webview(move |platform_webview| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let webview = platform_webview.controller().CoreWebView2()?;

                let request_event_name = CoTaskMemPWSTR::from("Network.requestWillBeSent");
                let request_receiver = webview.GetDevToolsProtocolEventReceiver(
                    *request_event_name.as_ref().as_pcwstr(),
                )?;
                let requests_for_event = requests.clone();
                let request_handler = DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                    move |_sender, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut raw = PWSTR::null();
                        args.ParameterObjectAsJson(&mut raw)?;
                        let payload = CoTaskMemPWSTR::from(raw).to_string();
                        let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                            return Ok(());
                        };
                        let request_id = value
                            .get("requestId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        let request = &value["request"];
                        let url = request
                            .get("url")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        if request_id.is_empty() || !compass_cdp_url_allowed(url) {
                            return Ok(());
                        }
                        let context = CompassCdpRequestContext {
                            url: url.to_string(),
                            method: request
                                .get("method")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("GET")
                                .to_string(),
                            post_data: request
                                .get("postData")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string),
                        };
                        if let Ok(mut pending) = requests_for_event.lock() {
                            pending.insert(request_id.to_string(), context);
                            if pending.len() > 1000 {
                                pending.clear();
                            }
                        }
                        Ok(())
                    },
                ));
                let mut request_token = 0;
                request_receiver.add_DevToolsProtocolEventReceived(
                    &request_handler,
                    &mut request_token,
                )?;

                let response_event_name = CoTaskMemPWSTR::from("Network.responseReceived");
                let response_receiver = webview.GetDevToolsProtocolEventReceiver(
                    *response_event_name.as_ref().as_pcwstr(),
                )?;
                let requests_for_response = requests.clone();
                let webview_for_response = webview.clone();
                let app_for_response = app.clone();
                let capture_id_for_response = capture_id.clone();
                let target_date_for_response = target_date.clone();
                let target_shop_for_response = target_shop_name.clone();
                let response_handler = DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                    move |_sender, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut raw = PWSTR::null();
                        args.ParameterObjectAsJson(&mut raw)?;
                        let payload = CoTaskMemPWSTR::from(raw).to_string();
                        let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                            return Ok(());
                        };
                        let request_id = value
                            .get("requestId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let response = &value["response"];
                        let response_url = response
                            .get("url")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        if request_id.is_empty() || !compass_cdp_url_allowed(response_url) {
                            return Ok(());
                        }
                        let context = requests_for_response
                            .lock()
                            .ok()
                            .and_then(|mut pending| pending.remove(&request_id))
                            .unwrap_or_else(|| CompassCdpRequestContext {
                                url: response_url.to_string(),
                                method: "GET".to_string(),
                                post_data: None,
                            });
                        let mut meta = serde_json::json!({
                            "source": "cdp",
                            "transport": "http",
                            "url": context.url,
                            "method": context.method,
                            "status": response.get("status").and_then(serde_json::Value::as_f64).unwrap_or_default(),
                            "contentType": response.get("mimeType").and_then(serde_json::Value::as_str).unwrap_or_default(),
                            "resourceType": value.get("type").and_then(serde_json::Value::as_str).unwrap_or_default(),
                            "requestId": request_id,
                            "capturedAt": chrono::Utc::now().to_rfc3339(),
                            "targetDate": target_date_for_response,
                            "targetShopName": target_shop_for_response,
                        });
                        if let Some(post_data) = context.post_data {
                            meta["requestPostData"] = serde_json::Value::String(post_data);
                        }
                        let method = CoTaskMemPWSTR::from("Network.getResponseBody");
                        let parameters = CoTaskMemPWSTR::from(
                            serde_json::json!({ "requestId": request_id }).to_string().as_str(),
                        );
                        let app_for_body = app_for_response.clone();
                        let capture_id_for_body = capture_id_for_response.clone();
                        let completed = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                            move |result, response_json| {
                                result?;
                                let Ok(body_result) = serde_json::from_str::<serde_json::Value>(&response_json) else {
                                    return Ok(());
                                };
                                let Some(body_text) = body_result.get("body").and_then(serde_json::Value::as_str) else {
                                    return Ok(());
                                };
                                let body_text = if body_result
                                    .get("base64Encoded")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(false)
                                {
                                    match base64::engine::general_purpose::STANDARD.decode(body_text) {
                                        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                                        Err(_) => return Ok(()),
                                    }
                                } else {
                                    body_text.to_string()
                                };
                                let app = app_for_body.clone();
                                let capture_id = capture_id_for_body.clone();
                                let meta = meta.clone();
                                tauri::async_runtime::spawn_blocking(move || {
                                    if let Err(error) = crate::compass_capture::ingest_cdp_response(
                                        &app,
                                        &capture_id,
                                        meta,
                                        &body_text,
                                    ) {
                                        log::debug!("Skipped compass CDP response: {error}");
                                    }
                                });
                                Ok(())
                            },
                        ));
                        webview_for_response.CallDevToolsProtocolMethod(
                            *method.as_ref().as_pcwstr(),
                            *parameters.as_ref().as_pcwstr(),
                            &completed,
                        )?;
                        Ok(())
                    },
                ));
                let mut response_token = 0;
                response_receiver.add_DevToolsProtocolEventReceived(
                    &response_handler,
                    &mut response_token,
                )?;

                let websocket_created_event_name =
                    CoTaskMemPWSTR::from("Network.webSocketCreated");
                let websocket_created_receiver = webview.GetDevToolsProtocolEventReceiver(
                    *websocket_created_event_name.as_ref().as_pcwstr(),
                )?;
                let requests_for_websocket = requests.clone();
                let websocket_created_handler =
                    DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                        move |_sender, args| {
                            let Some(args) = args else { return Ok(()) };
                            let mut raw = PWSTR::null();
                            args.ParameterObjectAsJson(&mut raw)?;
                            let payload = CoTaskMemPWSTR::from(raw).to_string();
                            let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload)
                            else {
                                return Ok(());
                            };
                            let request_id = value
                                .get("requestId")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default();
                            let url = value
                                .get("url")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default();
                            if request_id.is_empty() || !compass_cdp_url_allowed(url) {
                                return Ok(());
                            }
                            if let Ok(mut pending) = requests_for_websocket.lock() {
                                pending.insert(
                                    request_id.to_string(),
                                    CompassCdpRequestContext {
                                        url: url.to_string(),
                                        method: "WEBSOCKET".to_string(),
                                        post_data: None,
                                    },
                                );
                            }
                            Ok(())
                        },
                    ));
                let mut websocket_created_token = 0;
                websocket_created_receiver.add_DevToolsProtocolEventReceived(
                    &websocket_created_handler,
                    &mut websocket_created_token,
                )?;

                let websocket_frame_event_name =
                    CoTaskMemPWSTR::from("Network.webSocketFrameReceived");
                let websocket_frame_receiver = webview.GetDevToolsProtocolEventReceiver(
                    *websocket_frame_event_name.as_ref().as_pcwstr(),
                )?;
                let requests_for_frame = requests.clone();
                let unsupported_for_frame = unsupported_websockets.clone();
                let app_for_frame = app.clone();
                let capture_id_for_frame = capture_id.clone();
                let target_date_for_frame = target_date.clone();
                let target_shop_for_frame = target_shop_name.clone();
                let websocket_frame_handler =
                    DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                        move |_sender, args| {
                            let Some(args) = args else { return Ok(()) };
                            let mut raw = PWSTR::null();
                            args.ParameterObjectAsJson(&mut raw)?;
                            let payload = CoTaskMemPWSTR::from(raw).to_string();
                            let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload)
                            else {
                                return Ok(());
                            };
                            let request_id = value
                                .get("requestId")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let Some(context) = requests_for_frame
                                .lock()
                                .ok()
                                .and_then(|pending| pending.get(&request_id).cloned())
                            else {
                                return Ok(());
                            };
                            let frame = &value["response"];
                            let opcode = frame
                                .get("opcode")
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or_default() as u8;
                            let payload_data = frame
                                .get("payloadData")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let trimmed = payload_data.trim_start();
                            let mut meta = serde_json::json!({
                                "source": "cdp",
                                "transport": "websocket",
                                "url": context.url,
                                "method": context.method,
                                "status": 101,
                                "contentType": if opcode == 1 { "application/json" } else { "application/octet-stream" },
                                "resourceType": "WebSocket",
                                "requestId": request_id,
                                "capturedAt": chrono::Utc::now().to_rfc3339(),
                                "targetDate": target_date_for_frame,
                                "targetShopName": target_shop_for_frame,
                            });
                            if opcode == 1
                                && (trimmed.starts_with('{') || trimmed.starts_with('['))
                            {
                                let app = app_for_frame.clone();
                                let capture_id = capture_id_for_frame.clone();
                                tauri::async_runtime::spawn_blocking(move || {
                                    if let Err(error) =
                                        crate::compass_capture::ingest_cdp_response(
                                            &app,
                                            &capture_id,
                                            meta,
                                            &payload_data,
                                        )
                                    {
                                        log::debug!(
                                            "Skipped compass WebSocket frame: {error}"
                                        );
                                    }
                                });
                            } else {
                                let first_unsupported = unsupported_for_frame
                                    .lock()
                                    .map(|mut items| items.insert(request_id.clone()))
                                    .unwrap_or(false);
                                if first_unsupported {
                                    meta["associateActiveSection"] =
                                        serde_json::Value::Bool(false);
                                    let app = app_for_frame.clone();
                                    let capture_id = capture_id_for_frame.clone();
                                    tauri::async_runtime::spawn_blocking(move || {
                                        let _ = crate::compass_capture::record_cdp_capture_failure(
                                            &app,
                                            &capture_id,
                                            meta,
                                            "发现非 JSON WebSocket 数据帧，已记录接口但未保存正文",
                                        );
                                    });
                                }
                            }
                            Ok(())
                        },
                    ));
                let mut websocket_frame_token = 0;
                websocket_frame_receiver.add_DevToolsProtocolEventReceived(
                    &websocket_frame_handler,
                    &mut websocket_frame_token,
                )?;

                let method = CoTaskMemPWSTR::from("Network.enable");
                let parameters = CoTaskMemPWSTR::from(
                    r#"{"maxTotalBufferSize":104857600,"maxResourceBufferSize":10485760}"#,
                );
                let enabled = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                    move |result, _| result,
                ));
                webview.CallDevToolsProtocolMethod(
                    *method.as_ref().as_pcwstr(),
                    *parameters.as_ref().as_pcwstr(),
                    &enabled,
                )?;
                Ok(())
            })();
            if let Err(error) = result {
                if let Ok(mut slot) = install_error_for_webview.lock() {
                    *slot = Some(error.to_string());
                }
            }
        })
        .map_err(|error| format!("无法访问罗盘 WebView2：{error}"))?;

    if let Some(error) = install_error
        .lock()
        .map_err(|_| "CDP 安装状态锁已损坏".to_string())?
        .take()
    {
        return Err(format!("无法启用罗盘网络层采集：{error}"));
    }
    let _ = webview.eval("window.__BSR_NATIVE_CDP_CAPTURE__ = true;");
    Ok(())
}

#[cfg(feature = "gui")]
fn destroy_compass_auto_download_window(app: &tauri::AppHandle) {
    use tauri::Manager;

    let Some(existing) = app.get_webview_window("compass-auto-download") else {
        return;
    };
    if let Err(error) = existing.destroy() {
        log::warn!("Failed to destroy compass-auto-download window: {error}");
        let _ = existing.close();
    }
}

#[cfg(feature = "gui")]
async fn wait_compass_auto_download_window_gone(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;

    for _ in 0..40 {
        if app.get_webview_window("compass-auto-download").is_none() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    app.get_webview_window("compass-auto-download").is_none()
}

#[cfg(feature = "gui")]
async fn open_compass_window(
    app: &tauri::AppHandle,
    target_date: &str,
    target_shop_name: &str,
    auto_start: bool,
    target_started_at: Option<&str>,
    capture_mode: bool,
    capture_id: Option<&str>,
) -> Result<(), String> {
    use tauri::{Emitter, Manager};

    const LABEL: &str = "compass-auto-download";
    let _window_guard = COMPASS_WINDOW_OPEN_LOCK.lock().await;
    destroy_compass_auto_download_window(app);
    if !wait_compass_auto_download_window_gone(app).await {
        return Err("旧的抖音罗盘窗口仍在关闭中，请稍后重试".to_string());
    }
    let target_shop_name = validate_compass_shop(target_shop_name)?;
    let script = build_compass_automation_script_with_mode(
        target_date,
        &target_shop_name,
        target_started_at,
        auto_start,
        capture_mode,
        capture_id,
    )?;
    let suffix = if auto_start {
        "#bsrAutoStart=1"
    } else {
        "#bsrAutoStart=0"
    };
    let url = format!("{COMPASS_LIVE_OVERVIEW_URL}{suffix}");
    if !is_allowed_compass_url(&url) {
        return Err("罗盘地址不在允许范围内".to_string());
    }
    let navigation_app = app.clone();
    let window = tauri::WebviewWindowBuilder::new(
        app,
        LABEL,
        tauri::WebviewUrl::External(
            url.parse()
                .map_err(|error| format!("罗盘地址无效：{error}"))?,
        ),
    )
    .title(format!(
        "抖音罗盘 · {target_shop_name} · {}",
        if capture_mode {
            "完整采集"
        } else {
            "自动下载"
        }
    ))
    .inner_size(1380.0, 900.0)
    .center()
    .initialization_script(&script)
    .on_download(|_, _| {
        // 接管 WebView2 下载，保留后台下载但不显示浏览器下载侧栏。
        true
    })
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

    #[cfg(target_os = "windows")]
    if capture_mode {
        let capture_id = capture_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "缺少罗盘采集标识".to_string())?;
        if let Err(error) = install_compass_cdp_capture(
            window.as_ref(),
            app,
            capture_id,
            target_date,
            &target_shop_name,
        ) {
            let _ = window.close();
            return Err(error);
        }
    }

    // Initialization scripts run at document creation, while some Compass pages
    // replace their root immediately. Re-evaluate once after WebView2/CDP is ready;
    // the page-local guard keeps this idempotent.
    window
        .eval(&script)
        .map_err(|error| format!("无法启动罗盘自动采集脚本：{error}"))?;

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_title = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(window) = app_handle.get_webview_window(LABEL) else {
                break;
            };
            let Ok(title) = window.title() else { continue };
            if title == last_title {
                continue;
            }
            let Some(encoded) = title.strip_prefix("BSR_COMPASS:") else {
                continue;
            };
            let encoded = encoded.to_string();
            last_title = title;
            let Ok(decoded) = urlencoding::decode(&encoded) else {
                continue;
            };
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&decoded) else {
                continue;
            };
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
    target_shop_name: String,
) -> Result<(), String> {
    open_compass_window(
        &state.app_handle,
        &target_date,
        &target_shop_name,
        false,
        None,
        false,
        None,
    )
    .await
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn start_compass_live_downloads(
    state: crate::state_type!(),
    target_date: String,
    target_shop_name: String,
    target_started_at: Option<String>,
) -> Result<(), String> {
    open_compass_window(
        &state.app_handle,
        &target_date,
        &target_shop_name,
        true,
        target_started_at.as_deref(),
        false,
        None,
    )
    .await
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn start_compass_full_capture(
    state: crate::state_type!(),
    target_date: String,
    target_shop_name: String,
    target_started_at: Option<String>,
) -> Result<String, String> {
    let capture_id = format!("capture-{}", uuid::Uuid::new_v4().simple());
    let target_date = validate_compass_date(&target_date)?;
    let target_shop_name = validate_compass_shop(&target_shop_name)?;
    crate::compass_capture::initialize_compass_capture(
        &state.app_handle,
        &capture_id,
        &target_date,
        &target_shop_name,
    )?;
    if let Err(error) = open_compass_window(
        &state.app_handle,
        &target_date,
        &target_shop_name,
        true,
        target_started_at.as_deref(),
        true,
        Some(&capture_id),
    )
    .await
    {
        let _ = crate::compass_capture::fail_compass_capture(
            &state.app_handle,
            &capture_id,
            &format!("启动采集失败：{error}"),
        );
        return Err(error);
    }
    Ok(capture_id)
}

#[cfg(feature = "gui")]
fn close_embedded_compass_webview(app: &tauri::AppHandle) {
    use tauri::Manager;

    if let Some(webview) = app.get_webview(COMPASS_EMBEDDED_LABEL) {
        if let Err(error) = webview.close() {
            log::warn!("Failed to close embedded Compass webview: {error}");
        }
    }
}

#[cfg(feature = "gui")]
async fn wait_embedded_compass_webview_gone(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;

    for _ in 0..40 {
        if app.get_webview(COMPASS_EMBEDDED_LABEL).is_none() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    app.get_webview(COMPASS_EMBEDDED_LABEL).is_none()
}

#[cfg(feature = "gui")]
fn validate_embedded_bounds(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(tauri::LogicalPosition<f64>, tauri::LogicalSize<f64>), String> {
    if ![x, y, width, height].iter().all(|value| value.is_finite()) {
        return Err("罗盘嵌入区域坐标无效".to_string());
    }
    if width < 160.0 || height < 120.0 || width > 10_000.0 || height > 10_000.0 {
        return Err("罗盘嵌入区域尺寸无效".to_string());
    }
    Ok((
        tauri::LogicalPosition::new(x.max(0.0), y.max(0.0)),
        tauri::LogicalSize::new(width, height),
    ))
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn open_embedded_compass_capture(
    state: crate::state_type!(),
    target_date: String,
    target_shop_name: String,
    target_started_at: Option<String>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<String, String> {
    use tauri::{Emitter, Manager};

    let _guard = COMPASS_EMBEDDED_OPEN_LOCK.lock().await;
    let target_date = validate_compass_date(&target_date)?;
    let target_shop_name = validate_compass_shop(&target_shop_name)?;
    let (position, size) = validate_embedded_bounds(x, y, width, height)?;
    close_embedded_compass_webview(&state.app_handle);
    if !wait_embedded_compass_webview_gone(&state.app_handle).await {
        return Err("旧的内嵌罗盘仍在关闭中，请稍后重试".to_string());
    }

    let capture_id = format!("capture-{}", uuid::Uuid::new_v4().simple());
    crate::compass_capture::initialize_compass_capture(
        &state.app_handle,
        &capture_id,
        &target_date,
        &target_shop_name,
    )?;
    let script = build_compass_automation_script_with_mode(
        &target_date,
        &target_shop_name,
        target_started_at.as_deref(),
        true,
        true,
        Some(&capture_id),
    )?;
    let initial_url =
        format!("{COMPASS_LIVE_OVERVIEW_URL}#bsrAutoStart=1&bsrEmbedded=1&capture={capture_id}");
    if !is_allowed_compass_url(&initial_url) {
        return Err("罗盘地址不在允许范围内".to_string());
    }
    let main_window = state
        .app_handle
        .get_window("main")
        .ok_or_else(|| "主窗口不存在，无法嵌入罗盘".to_string())?;
    let navigation_app = state.app_handle.clone();
    let builder = tauri::webview::WebviewBuilder::new(
        COMPASS_EMBEDDED_LABEL,
        tauri::WebviewUrl::External(
            initial_url
                .parse()
                .map_err(|error| format!("罗盘地址无效：{error}"))?,
        ),
    )
    .initialization_script(&script)
    .on_download(|_, _| true)
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
    });

    let webview = match main_window.add_child(builder, position, size) {
        Ok(webview) => webview,
        Err(error) => {
            let message = format!("无法把官方罗盘嵌入数据看板：{error}");
            let _ = crate::compass_capture::fail_compass_capture(
                &state.app_handle,
                &capture_id,
                &message,
            );
            return Err(message);
        }
    };

    #[cfg(target_os = "windows")]
    if let Err(error) = install_compass_cdp_capture(
        &webview,
        &state.app_handle,
        &capture_id,
        &target_date,
        &target_shop_name,
    ) {
        let _ = webview.close();
        let _ = crate::compass_capture::fail_compass_capture(
            &state.app_handle,
            &capture_id,
            &format!("内嵌罗盘网络采集启动失败：{error}"),
        );
        return Err(error);
    }

    // CDP must be active before the business page issues its data requests.
    // Navigating once more with a unique hash guarantees a complete, observable load.
    let capture_url = format!("{initial_url}&cdpReady=1");
    webview
        .navigate(
            capture_url
                .parse()
                .map_err(|error| format!("罗盘采集地址无效：{error}"))?,
        )
        .map_err(|error| format!("无法加载内嵌罗盘：{error}"))?;
    webview
        .show()
        .map_err(|error| format!("无法显示内嵌罗盘：{error}"))?;
    Ok(capture_id)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn update_embedded_compass_bounds(
    state: crate::state_type!(),
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
) -> Result<bool, String> {
    use tauri::Manager;

    let Some(webview) = state.app_handle.get_webview(COMPASS_EMBEDDED_LABEL) else {
        return Ok(false);
    };
    if !visible {
        webview.hide().map_err(|error| error.to_string())?;
        return Ok(true);
    }
    let (position, size) = validate_embedded_bounds(x, y, width, height)?;
    webview
        .set_position(position)
        .map_err(|error| format!("无法移动内嵌罗盘：{error}"))?;
    webview
        .set_size(size)
        .map_err(|error| format!("无法调整内嵌罗盘：{error}"))?;
    webview
        .show()
        .map_err(|error| format!("无法显示内嵌罗盘：{error}"))?;
    Ok(true)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn close_embedded_compass(state: crate::state_type!()) -> Result<(), String> {
    let _guard = COMPASS_EMBEDDED_OPEN_LOCK.lock().await;
    close_embedded_compass_webview(&state.app_handle);
    if !wait_embedded_compass_webview_gone(&state.app_handle).await {
        return Err("内嵌罗盘仍在关闭中".to_string());
    }
    Ok(())
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn close_compass_auto_download(state: crate::state_type!()) -> Result<(), String> {
    let _window_guard = COMPASS_WINDOW_OPEN_LOCK.lock().await;
    destroy_compass_auto_download_window(&state.app_handle);
    if !wait_compass_auto_download_window_gone(&state.app_handle).await {
        return Err("抖音罗盘窗口仍在关闭中".to_string());
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
        assert!(!is_allowed_compass_url(
            "https://example.com/shop/live-overview"
        ));
        assert!(!is_allowed_compass_url("javascript:alert(1)"));
    }

    #[test]
    fn generated_script_contains_only_json_encoded_date() {
        let script =
            build_compass_automation_script("2026-07-28", "金典拍拍相机专卖店", None).unwrap();
        assert!(script.contains("2026-07-28"));
        assert!(script.contains("金典拍拍相机专卖店"));
        assert!(script.contains("compass-auto-download-progress"));
        assert!(script.contains("if (!document.documentElement)"));
        assert!(script.contains("findMonthPanel"));
        assert!(script.contains("revealDateFilter"));
        assert!(script.contains("正在回到列表顶部并寻找“自定义”日期入口"));
        assert!(script.contains("上一月"));
        assert!(script.contains("filterApplied"));
        assert!(script.contains("navigationPending"));
        assert!(script.contains("detailDownloadTriggered"));
        assert!(script.contains("切换数据视角"));
        assert!(script.contains("正在悬停当前账号，打开“切换数据视角”菜单"));
        assert!(script.contains("findHeaderShopTrigger"));
        assert!(script.contains("findShopChoiceCard"));
        assert!(script.contains("headerShowsShop"));
        assert!(script.contains("compassClock"));
        assert!(script.contains("sessionContainsStartedAt"));
        assert!(script.contains("findSessionAction"));
        assert!(script.contains("data-row-key"));
        assert!(script.contains("正在重新定位右侧固定操作列"));
        assert!(script.contains("capturePacing"));
        assert!(script.contains("metric: [3000, 5000]"));
        assert!(script.contains("product: [4000, 6000]"));
        assert!(script.contains("page: [6000, 9000]"));
        assert!(script.contains("navigation: [4500, 7000]"));
        assert!(script.contains("session: [10000, 15000]"));
        assert!(script.contains("sessionStart <= wanted && wanted <= sessionEnd"));
        assert!(script.contains("const normalizeDate = (value) =>"));
        assert!(script.contains("const endedAt = normalizeTime(match[3], match[2])"));
        assert!(script.contains("targetStartedAt"));
        assert!(script.contains("正在打开顶部“直播”菜单"));
        assert!(script.contains("PointerEvent('pointerdown'"));
        assert!(script.contains("shopConfirmed"));
        assert!(!script.contains("{{TARGET_DATE}}"));
    }

    #[test]
    fn accepts_only_the_two_configured_download_shops() {
        assert!(validate_compass_shop("金典拍拍科创专卖店").is_ok());
        assert!(validate_compass_shop("金典拍拍相机专卖店").is_ok());
        assert!(validate_compass_shop("其他店铺").is_err());
    }

    #[test]
    fn generated_script_uses_hover_and_exact_date_cells() {
        let script =
            build_compass_automation_script("2026-07-29", "金典拍拍科创专卖店", None).unwrap();
        assert!(script.contains("mouseenter"));
        assert!(script.contains("td[title=\"${title}\"]"));
        assert!(script.contains("confirmDateFilter"));
        assert!(script.contains("正在悬停“自定义”"));
        assert!(script.contains("findCustomDateControl"));
        assert!(script.contains("[data-node-key=\"custom\"] .aurora-dropdown-trigger"));
        assert!(!script.contains("clickTarget(custom)"));
        assert!(script.contains(".ecom-dropdown-trigger[class*=\"userDropDown-\"]"));
        assert!(script.contains(".ecom-dropdown-trigger [class*=\"userName-\"]"));
        assert!(script.contains("hoverTarget(currentShopEntry)"));
        assert!(!script.contains("clickTarget(currentShopEntry)"));
        assert!(script.contains("正在悬停当前账号"));
        assert!(script.contains("setTimeout(selectEndDate, 700)"));
        assert!(script.contains("data-subway-href"));
        assert!(script.contains("setTimeout(() => location.assign(detailUrl.href), 120)"));
        assert!(script.contains("saved.targetStartedAt === targetStartedAt"));
        assert!(script.contains("当前详情不是目标场次，已停止下载"));
        assert!(script.contains("bsr:compass-auto-download:v4"));
        assert!(script.contains("window.__BSR_COMPASS_IMPORTED__"));
        assert!(!script.contains("setTimeout(() => history.back(), 4500)"));
    }

    #[test]
    fn compass_window_reopen_destroys_and_waits_for_label_release() {
        let source = include_str!("compass_auto_download.rs");
        assert!(source.contains("existing.destroy()"));
        assert!(source.contains("wait_compass_auto_download_window_gone(app).await"));
        assert!(source.contains("COMPASS_WINDOW_OPEN_LOCK.lock().await"));
    }

    #[test]
    fn capture_mode_installs_redacted_chunked_network_capture() {
        let script = build_compass_automation_script_with_mode(
            "2026-08-27",
            "金典拍拍相机专卖店",
            None,
            true,
            true,
            Some("capture-safe-id"),
        )
        .unwrap();
        assert!(script.contains("const captureMode = true"));
        assert!(script.contains("const autoStart = true"));
        assert!(script.contains("capture-safe-id"));
        assert!(script.contains("compass-capture-chunk"));
        assert!(script.contains("compass-capture-control"));
        assert!(script.contains("window.fetch = async"));
        assert!(script.contains("OriginalXhr.prototype.open"));
        assert!(script.contains("chunkSize = 96 * 1024"));
        assert!(script.contains("captureMetricTargets"));
        assert!(script.contains("captureReadOnlyTargets"));
        assert!(script.contains("preferredReadOnlyTarget"));
        assert!(script.contains("sectionKey"));
        assert!(script.contains("module.qianchuan"));
        assert!(script.contains("tab.violation"));
        assert!(script.contains("state.captureListUrl"));
        assert!(script.contains("li[title=\"下一页\"]"));
        assert!(script.contains("status: 'triggered'"));
        assert!(script.contains("permission_denied"));
        assert!(script.contains("!parsed.hostname.endsWith('.jinritemai.com')"));
        assert!(script.contains("'[REDACTED]'"));
        assert!(!script.contains("label: '创建目标'"));
        assert!(!script.contains("label: '预警配置'"));
        assert!(!script.contains("__CAPTURE_ID_JSON__"));
    }

    #[cfg(all(feature = "gui", target_os = "windows"))]
    #[test]
    fn native_capture_rejects_hostname_suffix_spoofing() {
        assert!(compass_cdp_url_allowed(
            "https://compass.jinritemai.com/compass_api/live/trend"
        ));
        assert!(!compass_cdp_url_allowed(
            "https://eviljinritemai.com/compass_api/live/trend"
        ));
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
