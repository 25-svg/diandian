import { createServer, type Server } from "node:http";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import { chromium, type Browser, type Page, expect as dom } from "@playwright/test";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";

let browser: Browser, server: Server, origin: string;
const pages: Page[] = [];
beforeAll(async () => {
  const script = await build({ entryPoints: [fileURLToPath(new URL("../admin/app.ts", import.meta.url))], bundle: true, write: false, format: "esm" });
  const html = readFileSync(new URL("../admin/index.html", import.meta.url));
  const css = readFileSync(new URL("../admin/styles.css", import.meta.url));
  server = createServer((req, res) => {
    const js = req.url === "/app.js", style = req.url === "/styles.css";
    res.setHeader("content-type", js ? "text/javascript" : style ? "text/css" : "text/html");
    res.end(js ? script.outputFiles[0]!.text : style ? css : html);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  browser = await chromium.launch({ headless: true });
}, 30_000);
afterEach(async () => { await Promise.all(pages.splice(0).map((page) => page.close())); });
afterAll(async () => { await browser?.close(); await new Promise<void>((resolve) => server ? server.close(() => resolve()) : resolve()); });

type Options = { role?: "owner" | "operator"; empty?: boolean; fail?: boolean; delay?: Promise<void>; items?: Record<string, unknown>[] };
async function open(options: Options = {}) {
  const page = await browser.newPage(); pages.push(page);
  const writes: { path: string; body: unknown; origin: string | undefined }[] = [];
  await page.route("**/api/admin/**", async (route) => {
    const request = route.request(), path = new URL(request.url()).pathname.slice("/api/admin".length);
    if (path === "/me") return route.fulfill({ json: { id: "a1", email: "admin@example.com", role: options.role ?? "owner" } });
    if (request.method() !== "GET") {
      writes.push({ path, body: request.postDataJSON(), origin: request.headers().origin });
      return route.fulfill({ json: path === "/activation-codes" ? { codes: [{ id: "c1", code: "ONE-TIME-CODE", expiresAt: 1_900_000_000 }] } : path === "/download-links" ? { id: "l1", url: "https://updates.example/v1/initial-download/SECRET-TICKET", expiresAt: 1_900_000_000 } : { ok: true } });
    }
    await options.delay;
    if (options.fail) return route.fulfill({ status: 500, json: { error: { code: "INTERNAL_ERROR", message: "private diagnostic" } } });
    if (path === "/summary") return route.fulfill({ json: { devices: 12, releases: 1, activationCodes: 3 } });
    const items: Record<string, unknown>[] = options.empty ? [] : options.items ?? (
      path === "/devices" ? [{ id: "d1", status: "active", streamer_note: "主播甲", current_version: "2.21.1", test_group: null, last_seen_at: 1_700_000_000 }] :
      path === "/releases" ? [{ id: "r1", version: "2.21.1", status: "testing", notes: "修复更新", pub_date: "2026-09-11T00:00:00Z", platform: "windows", arch: "x86_64", size: 100 }] :
      path === "/admins" ? [{ id: "a1", email: "admin@example.com", role: "owner" }] :
      path === "/audit" ? [{ id: "audit1", action: "device.revoke", admin_id: "a1", target_id: "d1", details_json: '{"result":"success"}', created_at: 1_700_000_000 }] :
      path === "/activation-codes" ? [{ id: "c1", status: "unused", expires_at: 1_900_000_000 }] : [{ id: "l1", release_id: "r1", revoked_at: null, expires_at: 1_900_000_000 }]);
    return route.fulfill({ json: { items, nextCursor: null } });
  });
  await page.goto(origin);
  return { page, writes };
}

describe("private admin UI in a real browser", () => {
  it("navigates all six owner pages with named controls and Chinese content", async () => {
    const { page } = await open();
    await dom(page.getByRole("navigation")).toBeVisible({ timeout: 1000 });
    for (const name of ["总览", "设备", "激活码与下载链接", "版本", "管理员", "操作记录"]) {
      await page.getByRole("navigation").getByRole("link", { name, exact: true }).click();
      await dom(page.getByRole("heading", { name, exact: true, level: 1 })).toBeVisible();
    }
    await page.getByRole("link", { name: "设备", exact: true }).click();
    await dom(page.getByRole("button", { name: "禁用设备", exact: true })).toBeVisible();
    await page.getByRole("link", { name: "版本", exact: true }).click();
    await dom(page.getByRole("button", { name: "全量发布", exact: true })).toBeVisible();
  });

  it("hides owner administration and release actions for operators including direct hashes", async () => {
    const { page } = await open({ role: "operator" });
    await dom(page.getByRole("heading", { name: "总览", exact: true })).toBeVisible();
    await dom(page.getByRole("navigation").getByRole("link", { name: "管理员", exact: true })).toHaveCount(0);
    await dom(page.getByRole("navigation").getByRole("link", { name: "操作记录", exact: true })).toHaveCount(0);
    await page.getByRole("link", { name: "版本", exact: true }).click();
    await dom(page.getByRole("heading", { name: "版本", exact: true })).toBeVisible();
    await dom(page.getByRole("button", { name: "全量发布", exact: true })).toHaveCount(0);
    await page.evaluate(() => { location.hash = "admins"; });
    await dom(page.getByText("仅主管理员可访问此页面。")).toBeVisible();
    await dom(page.getByRole("button", { name: "添加管理员" })).toHaveCount(0);
  });

  it("shows loading, empty, failure, and successful completion in Chinese", async () => {
    let release!: () => void;
    const delayed = await open({ delay: new Promise<void>((resolve) => { release = resolve; }) });
    await dom(delayed.page.getByText("正在加载…", { exact: true })).toBeVisible(); release();
    await dom(delayed.page.getByText("加载成功", { exact: true })).toBeVisible();
    const empty = await open({ empty: true });
    await empty.page.getByRole("link", { name: "设备", exact: true }).click();
    await dom(empty.page.getByText("暂无记录", { exact: true })).toBeVisible();
    const failed = await open({ fail: true });
    await dom(failed.page.getByRole("alert")).toContainText("加载失败");
    await dom(failed.page.getByRole("button", { name: "重试" })).toBeVisible();
    await dom(failed.page.getByText("private diagnostic")).toHaveCount(0);
  });

  it("requires a second confirmation and sends same-origin JSON writes", async () => {
    const { page, writes } = await open();
    await page.getByRole("link", { name: "设备", exact: true }).click();
    await page.getByRole("button", { name: "禁用设备", exact: true }).click();
    await dom(page.getByRole("dialog")).toBeVisible(); expect(writes).toHaveLength(0);
    await page.getByRole("button", { name: "取消", exact: true }).click(); expect(writes).toHaveLength(0);
    await page.getByRole("button", { name: "禁用设备", exact: true }).click();
    await page.getByRole("button", { name: "确认禁用设备", exact: true }).click();
    await dom(page.getByText("操作成功", { exact: true })).toBeVisible();
    expect(writes).toEqual([{ path: "/devices/d1/revoke", body: {}, origin }]);
  });

  it("renders API text safely and never persists identities or one-time secrets", async () => {
    const malicious = '<img src=x onerror="globalThis.pwned=1">';
    const { page } = await open({ items: [{ id: "d1", status: "active", streamer_note: malicious }] });
    await page.getByRole("link", { name: "设备", exact: true }).click();
    await dom(page.getByText(malicious, { exact: true })).toBeVisible();
    expect(await page.locator("main img").count()).toBe(0);
    await page.getByRole("link", { name: "激活码与下载链接", exact: true }).click();
    await page.getByRole("button", { name: "生成激活码", exact: true }).click();
    await dom(page.getByText("ONE-TIME-CODE", { exact: true })).toBeVisible();
    await dom(page.getByText(/一次性显示/)).toBeVisible();
    expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
    await page.getByRole("link", { name: "总览", exact: true }).click();
    await dom(page.getByText("ONE-TIME-CODE", { exact: true })).toHaveCount(0);
    await page.goBack();
    await dom(page.getByText("ONE-TIME-CODE", { exact: true })).toHaveCount(0);
  });

  it("keeps keyboard skip navigation on the current page", async () => {
    const { page } = await open();
    await page.getByRole("link", { name: "设备", exact: true }).click();
    const skip = page.getByRole("link", { name: "跳至主要内容" });
    await skip.focus(); await page.keyboard.press("Enter");
    await dom(page.getByRole("heading", { name: "设备", exact: true })).toBeVisible();
    await dom(page.locator("main")).toBeFocused();
  });

  it.each(["navigation", "pagehide", "restore"] as const)("generates download links from the entered release and discards late secrets after %s", async (cleanup) => {
    const { page, writes } = await open();
    await page.getByRole("link", { name: "激活码与下载链接", exact: true }).click();
    await page.getByLabel("已全量发布的版本编号").fill("r1");
    await page.getByRole("button", { name: "生成下载链接", exact: true }).click();
    await dom(page.getByText("https://updates.example/v1/initial-download/SECRET-TICKET", { exact: true })).toBeVisible();
    expect(writes.at(-1)?.body).toEqual({ releaseId: "r1" });
    expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
    await page.getByRole("button", { name: "隐藏结果", exact: true }).click();
    await dom(page.getByText(/SECRET-TICKET/)).toHaveCount(0);
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => { release = resolve; });
    await page.route("**/api/admin/activation-codes", async (route) => {
      if (route.request().method() === "GET") return route.fallback();
      await blocked; await route.fulfill({ json: { codes: [{ id: "late", code: "LATE-SECRET" }] } });
    });
    const requested = page.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/activation-codes"));
    await page.getByRole("button", { name: "生成激活码", exact: true }).click(); await requested;
    if (cleanup === "navigation") await page.getByRole("link", { name: "总览", exact: true }).click();
    else await page.evaluate((type) => dispatchEvent(new PageTransitionEvent(type, { persisted: true })), cleanup === "restore" ? "pageshow" : "pagehide");
    const responded = page.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/activation-codes"));
    release(); await responded;
    await dom(page.getByRole("heading", { name: cleanup === "navigation" ? "总览" : "激活码与下载链接", exact: true })).toBeVisible();
    if (cleanup !== "navigation") await dom(page.getByRole("button", { name: "生成激活码", exact: true })).toBeEnabled();
    await dom(page.getByText("LATE-SECRET", { exact: true })).toHaveCount(0);
    if (cleanup === "navigation") await page.goBack();
    await dom(page.getByText("LATE-SECRET", { exact: true })).toHaveCount(0);
  });

  it.each(["hide", "generate", "navigate", "pagehide", "restore"] as const)("clears all concurrent one-time secrets on %s", async (cleanup) => {
    const { page } = await open();
    let releaseFirst!: () => void, releaseSecond!: () => void;
    const first = new Promise<void>((resolve) => { releaseFirst = resolve; });
    const second = new Promise<void>((resolve) => { releaseSecond = resolve; });
    let codeRequests = 0;
    await page.route("**/api/admin/activation-codes", async (route) => {
      if (route.request().method() === "GET") return route.fallback();
      if (++codeRequests > 1) return route.fulfill({ status: 503, json: { error: {} } });
      await first;
      await route.fulfill({ json: { codes: [{ id: "first", code: "FIRST-SECRET", expiresAt: 1_900_000_000 }] } });
    });
    await page.route("**/api/admin/download-links", async (route) => {
      if (route.request().method() === "GET") return route.fallback();
      await second;
      await route.fulfill({ json: { id: "second", url: "https://updates.example/SECOND-SECRET", expiresAt: 1_900_000_000 } });
    });
    await page.getByRole("link", { name: "激活码与下载链接", exact: true }).click();
    await page.getByLabel("已全量发布的版本编号").fill("r1");
    const codeRequested = page.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/activation-codes"));
    await page.getByRole("button", { name: "生成激活码", exact: true }).click(); await codeRequested;
    const linkRequested = page.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/download-links"));
    await page.getByRole("button", { name: "生成下载链接", exact: true }).click(); await linkRequested;
    await dom(page.locator(".secret")).toHaveCount(0);
    releaseFirst();
    await dom(page.getByText("FIRST-SECRET", { exact: true })).toBeVisible();
    await dom(page.locator(".secret")).toHaveCount(1);
    releaseSecond();
    await dom(page.getByText("https://updates.example/SECOND-SECRET", { exact: true })).toBeVisible();
    await dom(page.locator(".secret")).toHaveCount(2);
    const results = await page.locator(".secret").elementHandles();
    if (cleanup === "hide") await page.getByRole("button", { name: "隐藏结果", exact: true }).first().click();
    else if (cleanup === "generate") await page.getByRole("button", { name: "生成激活码", exact: true }).click();
    else if (cleanup === "navigate") await page.getByRole("link", { name: "总览", exact: true }).click();
    else await page.evaluate((type) => dispatchEvent(new PageTransitionEvent(type, { persisted: true })), cleanup === "restore" ? "pageshow" : "pagehide");
    await dom(page.locator(".secret")).toHaveCount(0, { timeout: 1000 });
    expect(await page.locator("body").textContent()).not.toMatch(/FIRST-SECRET|SECOND-SECRET/);
    // Clear detached nodes too, so retained DOM references cannot keep the secret text.
    for (const result of results) { expect(await result.textContent()).toBe(""); await result.dispose(); }
    if (cleanup === "navigate") {
      await page.goBack();
      await dom(page.getByRole("heading", { name: "激活码与下载链接", exact: true })).toBeVisible();
      await dom(page.locator(".secret")).toHaveCount(0);
      expect(await page.locator("body").textContent()).not.toMatch(/FIRST-SECRET|SECOND-SECRET/);
    }
  });

  it("does not reveal a pending secret after the user hides existing results", async () => {
    const { page } = await open();
    let releaseFirst!: () => void, releaseSecond!: () => void;
    const first = new Promise<void>((resolve) => { releaseFirst = resolve; });
    const second = new Promise<void>((resolve) => { releaseSecond = resolve; });
    await page.route("**/api/admin/activation-codes", async (route) => {
      if (route.request().method() === "GET") return route.fallback();
      await first;
      await route.fulfill({ json: { codes: [{ id: "first", code: "FIRST-BEFORE-HIDE", expiresAt: 1_900_000_000 }] } });
    });
    await page.route("**/api/admin/download-links", async (route) => {
      if (route.request().method() === "GET") return route.fallback();
      await second;
      await route.fulfill({ json: { id: "late", url: "https://updates.example/LATE-AFTER-HIDE", expiresAt: 1_900_000_000 } });
    });
    await page.getByRole("link", { name: "激活码与下载链接", exact: true }).click();
    await page.getByLabel("已全量发布的版本编号").fill("r1");
    const codeRequested = page.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/activation-codes"));
    await page.getByRole("button", { name: "生成激活码", exact: true }).click();
    await codeRequested;
    const linkRequested = page.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/download-links"));
    await page.getByRole("button", { name: "生成下载链接", exact: true }).click();
    await linkRequested;
    releaseFirst();
    await dom(page.getByText("FIRST-BEFORE-HIDE", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "隐藏结果", exact: true }).click();
    await dom(page.locator(".secret")).toHaveCount(0);
    const responded = page.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/download-links"));
    releaseSecond();
    await responded;
    await dom(page.getByText("https://updates.example/LATE-AFTER-HIDE", { exact: true })).toHaveCount(0);
    await dom(page.locator(".secret")).toHaveCount(0);
  });

  it("uses the server cursor to load more devices without losing the first page", async () => {
    const { page } = await open();
    await page.route("**/api/admin/devices*", (route) => route.fulfill({ json: new URL(route.request().url()).searchParams.get("cursor") === "50"
      ? { items: [{ id: "d2", status: "active", streamer_note: "第二页设备" }], nextCursor: null }
      : { items: [{ id: "d1", status: "active", streamer_note: "第一页设备" }], nextCursor: "50" } }));
    await page.getByRole("link", { name: "设备", exact: true }).click();
    await page.getByRole("button", { name: "加载更多", exact: true }).click();
    await dom(page.getByText("第一页设备", { exact: true })).toBeVisible();
    await dom(page.getByText("第二页设备", { exact: true })).toBeVisible();
    await dom(page.getByRole("button", { name: "加载更多", exact: true })).toHaveCount(0);
  });

  it("confirms owner release and administrator writes with the correct routes", async () => {
    const { page, writes } = await open();
    await page.getByRole("link", { name: "版本", exact: true }).click();
    await page.getByRole("button", { name: "全量发布", exact: true }).click();
    expect(writes).toHaveLength(0);
    await page.getByRole("button", { name: "确认全量发布", exact: true }).click();
    await dom(page.getByText("操作成功", { exact: true })).toBeVisible();
    expect(writes.at(-1)?.path).toBe("/releases/r1/production");
    await page.getByRole("link", { name: "管理员", exact: true }).click();
    await page.getByLabel("邮箱", { exact: true }).fill("operator@example.com");
    await page.getByRole("button", { name: "添加管理员", exact: true }).click();
    await page.getByRole("button", { name: "确认添加管理员", exact: true }).click();
    await dom(page.getByText("操作成功", { exact: true })).toBeVisible();
    expect(writes.at(-1)?.body).toEqual({ email: "operator@example.com", role: "operator" });
    await page.getByRole("button", { name: "移除管理员", exact: true }).click();
    await page.getByRole("button", { name: "确认移除管理员", exact: true }).click();
    await dom(page.getByText("操作成功", { exact: true })).toBeVisible();
    expect(writes.at(-1)?.path).toBe("/admins/a1");
  });

  it("shows safe write failures and clears one-time results before a page can enter history cache", async () => {
    const { page } = await open();
    await page.getByRole("link", { name: "激活码与下载链接", exact: true }).click();
    await page.getByRole("button", { name: "生成激活码", exact: true }).click();
    await dom(page.getByText("ONE-TIME-CODE", { exact: true })).toBeVisible();
    await page.evaluate(() => dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
    await dom(page.getByText("ONE-TIME-CODE", { exact: true })).toHaveCount(0);
    await page.route("**/api/admin/activation-codes", (route) => route.request().method() === "POST"
      ? route.fulfill({ status: 403, json: { error: { message: "private diagnostic" } } }) : route.fallback());
    await page.getByRole("button", { name: "生成激活码", exact: true }).click();
    await dom(page.getByRole("alert")).toContainText("操作失败");
    await dom(page.getByText("private diagnostic")).toHaveCount(0);
    await dom(page.getByRole("button", { name: "生成激活码", exact: true })).toBeEnabled();
  });

  it.each([375, 768, 1024, 1440])("keeps layout contained at %i px with keyboard focus and 44px targets", async (width) => {
    const { page } = await open(); await page.setViewportSize({ width, height: 900 });
    await page.getByRole("link", { name: "设备", exact: true }).click();
    await dom(page.getByRole("button", { name: "禁用设备", exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const small = await page.locator("button:visible, nav a:visible, input:visible, select:visible").evaluateAll((nodes) => nodes.filter((node) => { const r = node.getBoundingClientRect(); return r.height < 44 || r.width < 44; }).length);
    expect(small).toBe(0);
    await page.getByRole("button", { name: "禁用设备", exact: true }).focus();
    await page.keyboard.press("Tab");
    expect(await page.evaluate(() => getComputedStyle(document.activeElement!).outlineStyle)).not.toBe("none");
    await page.emulateMedia({ reducedMotion: "reduce" });
    expect(await page.getByRole("button", { name: "禁用设备", exact: true }).evaluate((button) => getComputedStyle(button).transitionDuration)).toBe("0s");
  });
});
