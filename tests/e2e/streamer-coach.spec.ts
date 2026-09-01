import { expect, test, type Page } from "@playwright/test";

const analysisFixture = {
  sourceTitle: "【自动测试夹具】罗雨欣 8月30日场",
  updatedAt: "2026-08-30T12:00:00.000Z",
  anchorName: "罗雨欣",
  anchorIdentityConfirmed: true,
  commentEvidenceStatus: "available",
  commentCount: 18,
  diagnosis: {
    headline: "异议回应有证据，但成交推进偏弱",
    highlights: ["先确认了观众用途"],
    issues: ["成交推进不够明确"],
    confirmations: ["价格待确认"],
    nextActions: ["确认需求后给出一个可执行下一步"],
    stats: {
      totalCandidates: 1,
      reviewedCount: 1,
      pendingCount: 0,
      matchedCount: 1,
      unmatchedCount: 0,
      highScoreCount: 0,
      riskCount: 1,
      averageScore: 82,
    },
  },
  candidates: [{
    id: "C1",
    start: 32,
    end: 49,
    scene: "成色异议",
    evidence: "观众询问成色，主播展示机身细节并说明待核实项。",
  }],
  reviews: {
    C1: {
      beginner: { summary: "事实边界清楚", checks: ["具体成色等级待确认"] },
      spokenScript: "先展示可确认细节，再说明以链接实拍和检测结果为准。",
    },
  },
};

async function openCoach(page: Page, width: number, height = 800): Promise<string[]> {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" && /StreamerCoach|streamerCoach|主播教练|coachFixture/i.test(message.text())) {
      errors.push(message.text());
    }
  });
  page.on("pageerror", (error) => {
    const detail = error.stack || error.message;
    if (/StreamerCoach|streamerCoach|主播教练|coachFixture/i.test(detail)) errors.push(detail);
  });
  await page.setViewportSize({ width, height });
  await page.addInitScript((fixture) => {
    localStorage.setItem("bsr:content-analysis:v3:fixture-luo-session", JSON.stringify(fixture));
    localStorage.setItem("bsr:content-analysis:v3:fixture-unassigned", JSON.stringify({
      ...fixture,
      sourceTitle: "【自动测试夹具】身份待确认导入视频",
      anchorIdentityConfirmed: false,
      commentEvidenceStatus: "missing",
    }));
  }, analysisFixture);
  await page.goto("/?coachFixture=acceptance-v1", { waitUntil: "domcontentloaded" });
  const coachNavigation = page.getByRole("button", { name: "主播教练", exact: true });
  await coachNavigation.click();
  await expect(coachNavigation).toHaveClass(/active/);
  await expect(page.getByRole("heading", { name: "AI 主播教练中心" })).toBeInViewport();
  return errors;
}

test("主播单端按证据完成五步使用闭环", async ({ page }) => {
  const errors = await openCoach(page, 1440, 900);
  await expect(page.getByText("主播使用版")).toBeVisible();
  await expect(page.getByRole("heading", { name: "按这 5 步使用主播教练" })).toBeVisible();
  await expect(page.getByRole("button", { name: /开播前.*看上场问题和本场准备清单/ })).toBeVisible();
  await expect(page.getByRole("tab", { name: "负责人端" })).toHaveCount(0);
  await expect(page.getByText("身份待确认导入视频")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /罗雨欣.*1 场证据/ })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByText("成交推进不够明确")).toBeVisible();

  await page.getByRole("tab", { name: "本场跟播" }).click();
  await expect(page.getByText("本场证据接入状态")).toBeVisible();
  await expect(page.getByText("导入视频若无评论，只分析口播/视频，不补造评论。")).toBeVisible();

  await page.getByRole("tab", { name: "下播复盘" }).click();
  await expect(page.getByText("【自动测试夹具】罗雨欣 8月30日场")).toBeVisible();
  await expect(page.getByText("评论记录存在 · 18 条")).toBeVisible();
  await page.getByText(/00:32 · 成色异议/).click();
  await expect(page.getByText(/AI复盘建议，需人工审核/)).toBeVisible();

  await page.getByRole("tab", { name: "训练任务" }).click();
  await expect(page.getByText("从真实评论起题")).toBeVisible();

  await page.getByRole("tab", { name: "和教练沟通" }).click();
  await page.evaluate(() => {
    let callbackId = 0;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        transformCallback: () => ++callbackId,
        unregisterCallback: () => undefined,
        convertFileSrc: (path: string) => path,
        invoke: async (command: string) => command === "minimax_chat"
          ? "【自动测试夹具｜AI教练建议，需人工审核】先确认观众用途和预算；具体价格、库存和成色仍需核实。"
          : null,
      },
    });
  });
  await page.getByLabel("给教练留言").fill("我上场最该改哪一点？");
  await page.getByRole("button", { name: "发送" }).click();
  await expect(page.getByText(/自动测试夹具｜AI教练建议/)).toBeVisible();
  await expect(page.getByText("已加载 1 场本主播证据")).toBeVisible();

  await page.screenshot({ path: "test-results/streamer-coach-1440-host-usage.png", fullPage: true });
  expect(errors).toEqual([]);
});

for (const width of [320, 768, 1440]) {
  test(`${width}px 无横向溢出且键盘可切换阶段`, async ({ page }) => {
    const errors = await openCoach(page, width);
    const preflight = page.getByRole("tab", { name: "开播前" });
    await preflight.focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("tab", { name: "本场跟播" })).toHaveAttribute("aria-selected", "true");
    const dimensions = await page.evaluate(() => ({
      body: document.body.scrollWidth,
      viewport: document.documentElement.clientWidth,
    }));
    expect(dimensions.body).toBeLessThanOrEqual(dimensions.viewport);
    await expect(page.getByText("本场证据接入状态")).toBeVisible();
    if (width === 320) {
      await page.screenshot({ path: "test-results/streamer-coach-320-live.png", fullPage: true });
    }
    expect(errors).toEqual([]);
  });
}

test("减少动态效果下保持最终状态和可操作性", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  const errors = await openCoach(page, 768);
  await page.getByRole("tab", { name: "下播复盘" }).click();
  await expect(page.getByText("异议回应有证据，但成交推进偏弱")).toBeVisible();
  expect(errors).toEqual([]);
});
