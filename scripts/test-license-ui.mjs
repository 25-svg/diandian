import assert from "node:assert/strict";
import { createServer } from "vite";
import { chromium } from "@playwright/test";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import sveltePreprocess from "svelte-preprocess";

const server = await createServer({ configFile: false, optimizeDeps: { noDiscovery: true, include: ["svelte", "svelte/internal"] }, server: { port: 0, strictPort: false, host: "127.0.0.1", watch: { ignored: ["**/src-tauri/target/**", "**/dist/**"] } }, plugins: [svelte({ preprocess: sveltePreprocess({ typescript: true }) }), {
  name: "license-ui-test",
  configureServer(server) {
    server.middlewares.use("/license-test", (_req, res) => {
      res.setHeader("content-type", "text/html");
      res.end('<html lang="zh-CN"><head><meta name="viewport" content="width=device-width,initial-scale=1"><style>body{margin:0}</style></head><body><script type="module">import Fixture from "/src/lib/components/LicenseGateFixture.svelte"; new Fixture({ target: document.body });</script></body></html>');
    });
  },
}] });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, reducedMotion: "reduce" });
  await page.addInitScript(() => {
    window.testLicense = { status: "unactivated", daysRemaining: null, message: "" };
    window.licenseCalls = [];
    window.__TAURI_INTERNALS__ = { invoke: (command, args) => {
      window.licenseCalls.push({ command, args });
      if (command === "get_license_status") return new Promise(resolve => { window.finishStatus = () => resolve(window.testLicense); });
      return new Promise(resolve => { window.finishOperation = () => resolve(window.testLicense); });
    }};
  });
  const url = `http://127.0.0.1:${server.httpServer.address().port}/license-test`;
  await page.goto(url);
  await page.getByText("正在验证设备授权…").waitFor({ timeout: 15000 });
  assert.equal(await page.getByTestId("protected-content").count(), 0);
  await page.evaluate(() => window.finishStatus());
  await page.getByLabel("激活码", { exact: true }).waitFor();
  if (process.env.LICENSE_SCREENSHOT_DIR) {
    await page.screenshot({ path: `${process.env.LICENSE_SCREENSHOT_DIR}/task-9-activation-mobile.png`, fullPage: true });
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.screenshot({ path: `${process.env.LICENSE_SCREENSHOT_DIR}/task-9-activation-desktop.png`, fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
  }
  for (const input of await page.locator("input").all()) assert.ok((await input.boundingBox()).height >= 44);
  await page.getByLabel("激活码", { exact: true }).focus();
  assert.equal(await page.getByLabel("激活码", { exact: true }).evaluate(el => getComputedStyle(el).outlineStyle), "solid");
  const code = page.getByLabel("激活码", { exact: true });
  assert.equal(await code.evaluate(el => !el.dispatchEvent(new Event("paste", { bubbles: true, cancelable: true }))), false);
  await code.fill("a".repeat(43));
  await page.getByLabel("电脑备注").fill("主播电脑");
  await page.getByRole("button", { name: "激活此电脑" }).click();
  assert.equal(await page.getByRole("button", { name: "正在激活…" }).isDisabled(), true);
  await page.evaluate(() => { window.testLicense = { status: "revoked", daysRemaining: null, message: "secret" }; window.finishOperation(); });
  await page.getByRole("alert").filter({ hasText: "撤销" }).waitFor();
  assert.equal(await page.getByTestId("protected-content").count(), 0);
  assert.ok(!(await page.textContent("body")).includes("secret"));
  await page.getByRole("button", { name: "联网重试" }).click();
  assert.equal(await page.getByRole("button", { name: "正在验证…" }).isDisabled(), true);
  await page.evaluate(() => { window.testLicense = { status: "offline_grace", daysRemaining: 2, message: "" }; window.finishOperation(); });
  await page.getByTestId("protected-content").waitFor();
  await page.getByRole("status").filter({ hasText: "剩余 2 天" }).waitFor();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.getByRole("button", { name: "联网续期" }).click();
  await page.evaluate(() => { window.testLicense = { status: "expired", daysRemaining: null, message: "" }; window.finishOperation(); });
  await page.getByRole("alert").filter({ hasText: "到期" }).waitFor();
  assert.equal(await page.getByTestId("protected-content").count(), 0);
  await page.getByRole("button", { name: "联网重试" }).click();
  await page.evaluate(() => { window.testLicense = { status: "clock_invalid", daysRemaining: null, message: "" }; window.finishOperation(); });
  await page.getByRole("alert").filter({ hasText: "系统时间" }).waitFor();
  assert.equal(await page.evaluate(() => Object.keys(localStorage).length + Object.keys(sessionStorage).length), 0);
  console.log("license UI passed: loading, labels, paste, disabled, alerts, offline, expiry, clock, narrow viewport");
} finally {
  await browser?.close();
  await server.close();
}
