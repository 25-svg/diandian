import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/compass-curve-harness.html");
  await expect(page.getByRole("heading", { name: "罗盘原始动态曲线验收" })).toBeVisible();
});

test("指标与时间范围切换同步刷新曲线、分析和证据", async ({ page }) => {
  const online = page.getByRole("button", { name: "在线人数", exact: true });
  const spend = page.getByRole("button", { name: "投放消耗", exact: true });
  const onlineCard = page.locator(".analysis-grid > article").filter({ hasText: "在线人数" });
  await expect(online).toHaveAttribute("aria-pressed", "true");
  await expect(onlineCard.locator("dd").nth(1)).toContainText("200人");

  await spend.focus();
  await page.keyboard.press("Space");
  await expect(spend).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByText(/不同量纲按当前窗口各自归一化/)).toBeVisible();
  await expect(page.locator(".analysis-grid > article")).toHaveCount(2);
  await expect(page.locator(".metric-line")).toHaveCount(2);
  await expect(page.locator('[stroke-dasharray="9 5"]')).toHaveCount(2);

  const end = page.getByLabel("曲线分析结束时间");
  await expect(end).toHaveValue(String(10 * 3600 + 9 * 60));
  await end.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(end).toHaveValue(String(10 * 3600 + 8 * 60));
  await expect(onlineCard.locator("dd").nth(1)).toContainText("150人");
  await expect(onlineCard.locator("dd").nth(1)).toContainText("10:04");

  await expect(page.getByText(/已关联对应直播时间段和逐字稿/).first()).toBeVisible();
  const seekButton = page.locator("button:enabled").filter({ hasText: "同步录播/文稿" }).first();
  await seekButton.click();
  await expect(page.getByText("已同步到 360 秒", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "恢复整场" }).click();
  await expect(page.getByText(/待核实：当前时间段没有逐字稿或节奏地图证据/).first()).toBeVisible();
});

test("每个曲线点可用鼠标或键盘选择并同步录播文稿", async ({ page }) => {
  const point = page.getByRole("button", { name: /在线人数，10:04，150人/ });
  await point.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("选中曲线点分析")).toContainText("00:04:00");
  await page.getByLabel("选中曲线点分析").getByRole("button", { name: "同步录播/文稿" }).click();
  await expect(page.getByText("已同步到 240 秒", { exact: true })).toBeVisible();
});

test("商品、人群、千川模块展示响应数据并可回看讲解", async ({ page }) => {
  await page.getByRole("button", { name: /商品 1/ }).click();
  await expect(page.getByRole("cell", { name: "佳能相机套装" })).toBeVisible();
  await page.getByRole("button", { name: "回看讲解" }).click();
  await expect(page.getByText("已同步到 240 秒", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: /人群 3/ }).click();
  await expect(page.getByText("25-34岁", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: /千川 2/ }).click();
  await expect(page.getByText("24.01元", { exact: true })).toBeVisible();
  await expect(page.getByText("ROI", { exact: true })).toBeVisible();
});

for (const viewport of [
  { width: 320, height: 800 },
  { width: 375, height: 812 },
  { width: 768, height: 900 },
  { width: 844, height: 390 },
  { width: 1440, height: 1000 },
]) {
  test(`${viewport.width}x${viewport.height}关键尺寸无页面级横向溢出且键盘控件可见`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await expect(page.locator(".curve-workbench")).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1)).toBe(true);
    expect(await page.getByRole("button", { name: "在线人数", exact: true }).evaluate((element) => element.getBoundingClientRect().height)).toBeGreaterThanOrEqual(44);
    const start = page.getByLabel("曲线分析开始时间");
    await start.focus();
    await page.keyboard.press("ArrowRight");
    await expect(start).toHaveValue(String(10 * 3600 + 60));
    await expect(start).toBeFocused();
  });
}
