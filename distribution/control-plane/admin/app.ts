type AdminView = "summary" | "devices" | "codes" | "releases" | "admins" | "audit";
type Identity = { id: string; email: string; role: "owner" | "operator" };
type Row = Record<string, unknown>;
type PageResult = { items: Row[]; nextCursor: string | null };
const names: Record<AdminView, string> = { summary: "总览", devices: "设备", codes: "激活码与下载链接", releases: "版本", admins: "管理员", audit: "操作记录" };
const descriptions: Record<AdminView, string> = {
  summary: "查看授权与版本分发概况。", devices: "管理设备授权、解绑与测试组。",
  codes: "生成限时凭证，管理初次安装入口。", releases: "查看版本，从小范围测试逐步发布。",
  admins: "管理后台访问权限，至少保留一位主管理员。", audit: "追溯后台操作与执行结果。"
};
const statuses: Record<string, string> = { active: "已启用", revoked: "已禁用", unused: "未使用", used: "已使用", draft: "草稿", testing: "测试中", production: "已发布", halted: "已停止", owner: "主管理员", operator: "普通管理员" };
const root = document.querySelector<HTMLDivElement>("#app")!;
let identity: Identity;
let epoch = 0;
let sensitiveGeneration = 0;
let main: HTMLElement;
let notice: HTMLElement;
const sensitiveResults = new Set<HTMLElement>();
let activeDialog: HTMLDialogElement | undefined;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); node.textContent = text; node.className = className; return node;
}
function button(label: string, action: () => void, className = "") {
  const node = el("button", label, className); node.type = "button"; node.addEventListener("click", action); return node;
}
function message(text: string, failed = false) {
  notice.textContent = text; notice.setAttribute("role", failed ? "alert" : "status"); notice.className = failed ? "notice error" : "notice";
}
async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api/admin${path}`, { credentials: "same-origin", cache: "no-store", ...init });
  if (!response.ok) {
    const labels: Record<number, string> = { 401: "登录已失效，请刷新页面重新登录。", 403: "没有执行此操作的权限。", 409: "当前状态不允许此操作，请刷新后重试。", 429: "请求过于频繁，请稍后重试。" };
    throw new Error(labels[response.status] ?? "请求失败，请稍后重试。");
  }
  return response.json() as Promise<T>;
}
function clearSensitive(invalidatePending = true) {
  if (invalidatePending) ++sensitiveGeneration;
  for (const result of sensitiveResults) { result.replaceChildren(); result.remove(); }
  sensitiveResults.clear();
}
function clearView() { ++epoch; clearSensitive(); activeDialog?.close(); activeDialog?.remove(); activeDialog = undefined; }
function date(value: unknown) {
  if (!value) return "—";
  const parsed = new Date(typeof value === "number" ? value * 1000 : String(value));
  return Number.isNaN(parsed.valueOf()) ? "—" : parsed.toLocaleString("zh-CN", { hour12: false });
}
function value(row: Row, key: string) { const raw = row[key]; return raw == null || raw === "" ? "—" : String(raw); }
function badge(raw: unknown) { return el("span", statuses[String(raw)] ?? String(raw ?? "—"), "badge"); }
function field(form: HTMLElement, label: string, type = "text", initial = "") {
  const wrapper = el("label", label), input = el("input"); input.type = type; input.value = initial;
  input.autocomplete = "off"; wrapper.append(input); form.append(wrapper); return input;
}
function select(form: HTMLElement, label: string, choices: [string, string][]) {
  const wrapper = el("label", label), control = el("select");
  for (const [id, name] of choices) { const option = el("option", name); option.value = id; control.append(option); }
  wrapper.append(control); form.append(wrapper); return control;
}
async function write(path: string, body: Row, source: HTMLButtonElement, secret = false, method = "POST") {
  const started = epoch; source.disabled = true; clearSensitive(false); const sensitiveStarted = sensitiveGeneration; message("正在提交…");
  try {
    const result = await api<Row>(path, { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
    if (started !== epoch || (secret && sensitiveStarted !== sensitiveGeneration)) return;
    if (secret) {
      const sensitive = el("section", "", "secret panel"); sensitive.setAttribute("aria-label", "一次性结果");
      sensitive.append(el("h2", "一次性显示，请立即妥善保存"), el("p", "离开此页后不会再次显示。"));
      const codes = Array.isArray(result.codes) ? result.codes as Row[] : [];
      for (const code of codes) sensitive.append(el("code", String(code.code)));
      if (typeof result.url === "string") sensitive.append(el("code", result.url));
      sensitive.append(button("隐藏结果", clearSensitive)); sensitiveResults.add(sensitive); main.append(sensitive);
    } else {
      await render();
      // A navigation while refreshing must not receive this operation's status.
      if (epoch !== started + 1) return;
    }
    message("操作成功");
  } catch (error) {
    if (started === epoch) message(`操作失败：${error instanceof Error ? error.message : "请稍后重试。"}`, true);
  } finally { source.disabled = false; }
}
function confirmAction(label: string, detail: string, path: string, body: Row = {}, method = "POST") {
  const dialog = el("dialog"); activeDialog = dialog;
  const title = el("h2", `确认${label}`); title.id = "confirm-title"; dialog.setAttribute("aria-labelledby", title.id);
  const actions = el("div", "", "actions");
  const cancel = button("取消", () => dialog.close());
  const confirm = button(`确认${label}`, () => { dialog.close(); void write(path, body, confirm, false, method); }, "danger");
  actions.append(cancel, confirm); dialog.append(title, el("p", detail), actions); document.body.append(dialog);
  dialog.addEventListener("close", () => { dialog.remove(); if (activeDialog === dialog) activeDialog = undefined; });
  dialog.showModal(); cancel.focus();
}
function action(label: string, path: string, detail: string, body: Row = {}, method = "POST") {
  return button(label, () => confirmAction(label, detail, path, body, method), "danger-text");
}
function details(row: Row, pairs: [string, string, boolean?][]) {
  const list = el("dl", "", "details");
  for (const [key, label, isDate] of pairs) { const group = el("div"); group.append(el("dt", label), el("dd", isDate ? date(row[key]) : value(row, key))); list.append(group); }
  return list;
}
function rowCard(row: Row, kind: string) {
  const card = el("article", "", "record"); const header = el("div", "", "record-header"); const actions = el("div", "", "actions");
  const id = encodeURIComponent(String(row.id));
  if (kind === "devices") {
    header.append(el("h3", value(row, "streamer_note") === "—" ? value(row, "id") : value(row, "streamer_note")), badge(row.status));
    card.append(header, details(row, [["id", "设备编号"], ["current_version", "当前版本"], ["last_seen_at", "最后在线", true]]));
    card.append(el("p", row.test_group ? "测试组：已加入" : "测试组：未加入", "muted"));
    if (row.status === "active") actions.append(action("禁用设备", `/devices/${id}/revoke`, "设备授权将停用。确认后生效。"));
    actions.append(action("解绑设备", `/devices/${id}/unbind`, "原设备令牌将失效，重新使用需再次激活。"));
    const enabled = !row.test_group;
    actions.append(action(enabled ? "加入测试组" : "移出测试组", `/devices/${id}/test-group`, "此变更会影响设备可接收的测试版本。", { enabled }));
  } else if (kind === "releases") {
    header.append(el("h3", value(row, "version")), badge(row.status)); card.append(header, el("p", value(row, "notes")), details(row, [["id", "版本编号"], ["pub_date", "发布时间", true], ["platform", "平台"], ["arch", "架构"], ["size", "安装包字节数"]]));
    if (identity.role === "owner") {
      if (row.status === "draft") actions.append(action("开始测试", `/releases/${id}/testing`, "版本将对测试组设备可用。"));
      if (row.status === "testing") actions.append(action("全量发布", `/releases/${id}/production`, "版本将面向全部获授权设备发布。请确认测试已完成。"));
      if (row.status === "testing" || row.status === "production") actions.append(action("停止发布", `/releases/${id}/halted`, "此版本将停止分发。"));
    }
  } else if (kind === "admins") {
    header.append(el("h3", value(row, "email")), badge(row.role)); card.append(header, details(row, [["id", "管理员编号"], ["created_at", "添加时间", true]]));
    actions.append(action("移除管理员", `/admins/${id}`, "将撤销此账号的后台访问权限。系统会保留至少一位主管理员。", {}, "DELETE"));
  } else if (kind === "audit") {
    header.append(el("h3", value(row, "action"))); card.append(header, details(row, [["created_at", "操作时间", true], ["admin_id", "操作人"], ["target_id", "操作对象"], ["details_json", "操作详情"]]));
  } else {
    header.append(el("h3", value(row, "id")), badge(kind === "activation-codes" ? row.status : row.revoked_at ? "revoked" : "active"));
    card.append(header, details(row, [["expires_at", "有效期至", true], ...(kind === "download-links" ? [["release_id", "版本编号"] as [string, string]] : [])]));
    if (kind === "activation-codes" ? row.status === "unused" : !row.revoked_at) actions.append(action(kind === "activation-codes" ? "撤销激活码" : "撤销下载链接", `/${kind}/${id}/revoke`, "凭证将立即失效，无法继续使用。"));
  }
  card.append(actions); return card;
}
async function listing(parent: HTMLElement, kind: string, started: number, cursor?: string) {
  const result = await api<PageResult>(`/${kind}${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`);
  if (started !== epoch) return;
  if (!result.items.length && !cursor) parent.append(el("p", "暂无记录", "empty"));
  for (const row of result.items) parent.append(rowCard(row, kind));
  if (result.nextCursor) {
    const more = button("加载更多", async () => {
      more.disabled = true;
      try { await listing(parent, kind, started, result.nextCursor!); more.remove(); }
      catch { if (started === epoch) message("加载失败，请稍后重试。", true); more.disabled = false; }
    }); parent.append(more);
  }
}
function createForms(view: AdminView, parent: HTMLElement) {
  if (view === "codes") {
    const form = el("form", "", "panel form"); form.append(el("h2", "生成激活码"));
    const count = field(form, "数量（1–20 个，有效期 7 天）", "number", "1"); count.min = "1"; count.max = "20"; count.required = true;
    const submit = el("button", "生成激活码", "primary"); form.append(submit);
    form.addEventListener("submit", (event) => { event.preventDefault(); void write("/activation-codes", { count: Number(count.value), days: 7 }, submit, true); }); parent.append(form);
    const downloads = el("form", "", "panel form"); downloads.append(el("h2", "生成下载链接"));
    const release = field(downloads, "已全量发布的版本编号"); release.required = true; release.maxLength = 256; release.pattern = "[A-Za-z0-9_-]+";
    downloads.append(el("p", "链接有效期 24 小时。版本编号可在版本页查看。", "muted"));
    const generate = el("button", "生成下载链接", "primary"); downloads.append(generate);
    downloads.addEventListener("submit", (event) => { event.preventDefault(); void write("/download-links", { releaseId: release.value.trim() }, generate, true); }); parent.append(downloads);
  }
  if (view === "admins" && identity.role === "owner") {
    const form = el("form", "", "panel form"); form.append(el("h2", "添加管理员"));
    const email = field(form, "邮箱", "email"); email.required = true; email.maxLength = 320;
    const role = select(form, "权限", [["operator", "普通管理员"], ["owner", "主管理员"]]);
    const submit = el("button", "添加管理员", "primary"); form.append(submit);
    form.addEventListener("submit", (event) => { event.preventDefault(); confirmAction("添加管理员", `将为 ${email.value} 授予${statuses[role.value]}权限。`, "/admins", { email: email.value.trim(), role: role.value }); }); parent.append(form);
  }
}
async function render() {
  clearView(); const started = epoch;
  const hash = location.hash.slice(1); const view: AdminView = Object.hasOwn(names, hash) ? hash as AdminView : "summary";
  for (const link of document.querySelectorAll<HTMLAnchorElement>("nav a")) {
    if (link.hash === `#${view}`) link.setAttribute("aria-current", "page"); else link.removeAttribute("aria-current");
  }
  main.replaceChildren(); const heading = el("h1", names[view]); heading.tabIndex = -1;
  const top = el("header", "", "page-header"); top.append(el("p", "私密分发 / 管理控制台", "eyebrow"), heading, el("p", descriptions[view], "muted"));
  main.append(top); notice = el("p", "", "notice"); notice.setAttribute("aria-live", "polite"); main.append(notice);
  if (identity.role !== "owner" && (view === "admins" || view === "audit")) { message("仅主管理员可访问此页面。", true); return; }
  const content = el("div", "", "content"); main.append(content); message("正在加载…");
  try {
    if (view === "summary") {
      const summary = await api<Record<string, number>>("/summary"); if (started !== epoch) return;
      const metrics = el("div", "", "metrics");
      for (const [key, label] of [["devices", "已授权设备"], ["releases", "分发中版本"], ["activationCodes", "可用激活码"]]) {
        const metric = el("section", "", "panel metric"); metric.append(el("h2", label), el("p", String(summary[key] ?? 0), "number")); metrics.append(metric);
      }
      content.append(metrics, el("p", "先在测试组验证版本，再由主管理员执行全量发布。", "panel"));
    } else {
      createForms(view, content);
      const kinds = view === "codes" ? ["activation-codes", "download-links"] : [view];
      await Promise.all(kinds.map(async (kind) => {
        const section = el("section", "", "records"); section.setAttribute("aria-label", kind === "activation-codes" ? "激活码记录" : kind === "download-links" ? "下载链接记录" : names[view]);
        if (view === "codes") section.append(el("h2", kind === "activation-codes" ? "激活码记录" : "下载链接记录")); content.append(section); await listing(section, kind, started);
      }));
    }
    if (started === epoch) message("加载成功");
  } catch (error) {
    if (started !== epoch) return;
    message(`加载失败：${error instanceof Error ? error.message : "请稍后重试。"}`, true);
    content.append(button("重试", () => { void render(); }));
  }
}
async function boot() {
  clearView();
  root.replaceChildren(el("p", "正在加载…", "boot"));
  try {
    identity = await api<Identity>("/me");
    if (identity.role !== "owner" && identity.role !== "operator") throw new Error("没有后台访问权限。");
    root.replaceChildren(); const aside = el("aside", "", "sidebar"); const brand = el("div", "", "brand");
    const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg"); icon.setAttribute("viewBox", "0 0 24 24"); icon.setAttribute("aria-hidden", "true");
    const path = document.createElementNS(icon.namespaceURI, "path"); path.setAttribute("d", "M4 4h7v7H4zM14 4h6v7h-6zM4 14h7v6H4zM14 14h6v6h-6z"); icon.append(path);
    brand.append(icon, el("strong", "点点 · 分发管理")); aside.append(brand, el("p", "工作空间", "eyebrow"));
    const nav = el("nav"); nav.setAttribute("aria-label", "后台导航");
    for (const [view, name] of Object.entries(names)) {
      if (identity.role !== "owner" && (view === "admins" || view === "audit")) continue;
      const link = el("a", name); link.href = `#${view}`; nav.append(link);
    }
    aside.append(nav); const user = el("div", "", "identity"); user.append(el("p", identity.email), badge(identity.role)); aside.append(user);
    main = el("main"); main.id = "main"; main.tabIndex = -1;
    const skip = el("a", "跳至主要内容", "skip"); skip.href = "#main";
    skip.addEventListener("click", (event) => { event.preventDefault(); main.focus(); });
    root.append(skip, aside, main); await render();
  } catch (error) {
    const failure = el("p", `加载失败：${error instanceof Error ? error.message : "请刷新重试。"}`); failure.setAttribute("role", "alert"); root.replaceChildren(failure, button("重试", () => { void boot(); }));
  }
}
window.addEventListener("hashchange", () => { if (identity) void render(); });
window.addEventListener("pagehide", clearView);
window.addEventListener("pageshow", (event) => { if (event.persisted) void boot(); });
void boot();
