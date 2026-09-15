import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => { await page.goto("/tests/fixtures/app-auth-gate-harness.html"); });

test("管理员登录后固定进入负责人端，退出后回到登录页", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(page.getByRole("heading", { name: "登录工作台" })).toBeVisible();
  await page.screenshot({ path: "C:/Users/10230/Documents/Codex/Workspace/01-Projects/Project-003-直播切片分析系统/05-测试与验收/2026-09-15-双角色登录/login-desktop-1440.png", fullPage: true });
  await page.getByLabel("账号").fill("admin");
  await page.getByLabel("密码").fill("111");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("heading", { name: "首次登录，请修改密码" })).toBeVisible();
  await page.getByLabel("新密码", { exact: true }).fill("Changed-Password-2");
  await page.getByLabel("再次输入新密码").fill("Changed-Password-2");
  await page.getByRole("button", { name: "保存并进入" }).click();
  await expect(page.getByRole("heading", { name: "负责人端" })).toBeVisible();
  await expect(page.getByTestId("default-route")).toHaveText("运营总览");
  await expect(page.getByTestId("route-scope")).toHaveText("可访问运营总览");
  await page.getByRole("button", { name: "退出登录" }).click();
  await expect(page.getByRole("heading", { name: "登录工作台" })).toBeVisible();
});

test("主播首次登录必须改密，再进入主播端", async ({ page }) => {
  await page.getByLabel("账号").fill("anchor");
  await page.getByLabel("密码").fill("Correct-Password-1");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("heading", { name: "首次登录，请修改密码" })).toBeVisible();
  await page.getByLabel("新密码", { exact: true }).fill("Changed-Password-2");
  await page.getByLabel("再次输入新密码").fill("Changed-Password-2");
  await page.getByRole("button", { name: "保存并进入" }).click();
  await expect(page.getByRole("heading", { name: "主播端" })).toBeVisible();
  await expect(page.getByTestId("default-route")).toHaveText("总览");
  await expect(page.getByTestId("route-scope")).toHaveText("不可访问运营总览");
});

test("错误密码显示可读错误且密码输入允许粘贴", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 760 });
  await page.getByLabel("账号").fill("admin");
  await page.getByLabel("密码").fill("Wrong-Password-1");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("账号或密码错误");
  await expect(page.getByLabel("密码")).toHaveAttribute("autocomplete", "current-password");
  await page.screenshot({ path: "C:/Users/10230/Documents/Codex/Workspace/01-Projects/Project-003-直播切片分析系统/05-测试与验收/2026-09-15-双角色登录/login-error-mobile-375.png", fullPage: true });
});
