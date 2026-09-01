import { expect, test, type Page } from "@playwright/test";
import {
  TRAINING_ACCEPTANCE_MEDIA_PATH,
  TRAINING_ACCEPTANCE_SCORES,
} from "../fixtures/training-acceptance-v1";

declare global {
  interface Window {
    __SCENARIO_TRAINING_TEST_FIXTURE__?: string | Record<string, (...args: any[]) => Promise<any>>;
    __ABANDONED_TRAINING_SESSIONS__?: string[];
  }
}

const MODULE_LABELS = [
  "需求确认",
  "事实准确",
  "信任建立",
  "表达清晰",
  "成交推进",
  "风险合规",
  "直播节奏",
] as const;

async function openTraining(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.__SCENARIO_TRAINING_TEST_FIXTURE__ ||= "acceptance-v1";
  });
  await page.goto("/?trainingFixture=acceptance-v1", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "情景训练", exact: true }).click();
  await expect(page.getByTestId("training-role-selection")).toBeVisible();
}

async function openTrainingWithDelayedStart(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const modules = [
      "opening",
      "retention_interaction",
      "needs_discovery",
      "product_explanation",
      "objection_handling",
      "conversion",
      "transition",
      "incident",
    ];
    window.__ABANDONED_TRAINING_SESSIONS__ = [];
    window.__SCENARIO_TRAINING_TEST_FIXTURE__ = {
      async listTrainingRoles() {
        return [
          {
            id: "ROLE-LUO",
            displayName: "罗雨欣",
            description: "【自动测试夹具】延迟启动竞态角色。",
            availableCaseCount: 8,
            availableModules: modules,
            evidenceStatus: "ready",
          },
          {
            id: "ROLE-HOU",
            displayName: "侯梦娜",
            description: "【自动测试夹具】切换后的目标角色。",
            availableCaseCount: 2,
            availableModules: ["objection_handling", "incident"],
            evidenceStatus: "ready",
          },
        ];
      },
      async startTrainingSession(request: { roleId: string; mode: string }) {
        await new Promise((resolve) => setTimeout(resolve, 600));
        return {
          sessionId: `DELAYED-${request.roleId}`,
          roleId: request.roleId,
          displayName: request.roleId === "ROLE-LUO" ? "罗雨欣" : "侯梦娜",
          mode: request.mode,
          currentIndex: 0,
          totalQuestions: 1,
          question: {
            caseId: "DELAYED-CASE",
            module: "opening",
            prompt: "【自动测试夹具】延迟返回的问题不得污染新角色。",
          },
        };
      },
      async abandonTrainingSession(request: { sessionId: string }) {
        window.__ABANDONED_TRAINING_SESSIONS__?.push(request.sessionId);
      },
      async submitTrainingAnswer() { throw new Error("本测试不应提交回答"); },
      async nextTrainingQuestion() { throw new Error("本测试不应进入下一题"); },
      async completeTrainingSession() { throw new Error("本测试不应完成训练"); },
    };
  });
  await page.goto("/?trainingFixture=delayed-start", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "情景训练", exact: true }).click();
  await expect(page.getByTestId("training-role-selection")).toBeVisible();
}

async function selectRole(page: Page, roleId: string, displayName: string): Promise<void> {
  const role = page.getByTestId(`role-card-${roleId}`);
  await expect(role).toHaveAttribute("data-role-id", roleId);
  await role.click();
  await expect(page.getByTestId("current-training-role")).toHaveText(
    `当前训练主播：${displayName}`,
  );
  await page.getByTestId("experience-evidence").click();
}

async function startComprehensive(page: Page): Promise<void> {
  await page.getByTestId("training-mode-comprehensive").click();
  await page.getByTestId("start-training").click();
  await expect(page.getByTestId("training-question")).toBeVisible();
}

async function submitAnswer(page: Page, answer = "【自动测试夹具】学员现场回答"): Promise<void> {
  await page.getByTestId("training-answer").fill(answer);
  await page.getByTestId("submit-answer").click();
  await expect(page.getByTestId("training-feedback")).toBeVisible();
}

test("正式证据库为空时可显式加载标注清楚的测试场景", async ({ page }) => {
  await page.addInitScript(() => {
    const emptyRoles = [
      { id: "yu-qianhui", displayName: "于千惠", availableCaseCount: 0, availableModules: [], evidenceStatus: "empty" },
      { id: "hou-mengna", displayName: "侯梦娜", availableCaseCount: 0, availableModules: [], evidenceStatus: "empty" },
      { id: "xiao-e", displayName: "小鹅", availableCaseCount: 0, availableModules: [], evidenceStatus: "empty" },
      { id: "luo-yuxin", displayName: "罗雨欣", availableCaseCount: 0, availableModules: [], evidenceStatus: "empty" },
    ];
    const unavailable = async () => { throw new Error("本测试不应调用空库训练接口"); };
    window.__SCENARIO_TRAINING_TEST_FIXTURE__ = {
      async listTrainingRoles() { return emptyRoles; },
      startTrainingSession: unavailable,
      submitTrainingAnswer: unavailable,
      nextTrainingQuestion: unavailable,
      completeTrainingSession: unavailable,
      abandonTrainingSession: unavailable,
      startHumanMachineScenario: unavailable,
      submitHumanMachineTurn: unavailable,
      getActiveHumanMachineScenario: unavailable,
      abandonHumanMachineScenario: unavailable,
    };
  });

  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "情景训练", exact: true }).click();

  const callout = page.getByTestId("training-empty-test-action");
  await expect(callout).toContainText("正式证据库暂时为空");
  await expect(page.getByTestId("training-data-mode")).not.toContainText("测试数据模式");

  await callout.getByRole("button", { name: "加载测试场景" }).click();

  await expect(callout).toBeHidden();
  await expect(page.getByTestId("training-data-mode")).toContainText("测试数据模式");
  await expect(page.getByTestId("role-card-ROLE-LUO")).toHaveAttribute("aria-label", /8条已审核案例/);
  await expect(page.getByTestId("role-card-ROLE-HOU")).toHaveAttribute("aria-label", /2条已审核案例/);
  await expect(page.getByTestId("role-card-ROLE-XIAOE")).toHaveAttribute("aria-label", /2条已审核案例/);
});

test("先选择主播；选中卡清晰、其他卡变暗，进入正确主播", async ({ page }) => {
  await openTraining(page);

  await expect(page.getByTestId("training-question")).toHaveCount(0);
  await expect(page.getByTestId("role-card-ROLE-LUO")).toContainText("罗雨欣");
  await expect(page.getByTestId("role-card-ROLE-HOU")).toContainText("侯梦娜");
  await expect(page.getByTestId("role-card-ROLE-XIAOE")).toContainText("小鹅");
  await expect(page.getByTestId("role-card-ROLE-YU")).toContainText("于千惠");
  await expect(page.getByTestId("training-role-selection")).not.toContainText("小鸦");
  await expect(page.getByTestId("training-role-selection")).not.toContainText("天乐");

  const luo = page.getByTestId("role-card-ROLE-LUO");
  await expect(luo).toHaveAttribute("aria-pressed", "false");
  await luo.click();
  const selectionVisual = await page.evaluate(() => {
    const luoCard = document.querySelector<HTMLElement>('[data-testid="role-card-ROLE-LUO"]');
    const houCard = document.querySelector<HTMLElement>('[data-testid="role-card-ROLE-HOU"]');
    const houVisual = document.querySelector<HTMLElement>('[data-testid="role-visual-ROLE-HOU"]');
    const houName = document.querySelector<HTMLElement>('[data-testid="role-name-ROLE-HOU"]');
    return {
      luoPressed: luoCard?.getAttribute("aria-pressed"),
      luoSelected: luoCard?.dataset.selected,
      luoDimmed: luoCard?.dataset.dimmed,
      houDimmed: houCard?.dataset.dimmed,
      visualFilter: houVisual ? getComputedStyle(houVisual).filter : "missing",
      visualOpacity: houVisual ? Number(getComputedStyle(houVisual).opacity) : 1,
      nameFilter: houName ? getComputedStyle(houName).filter : "missing",
      nameOpacity: houName ? Number(getComputedStyle(houName).opacity) : 0,
    };
  });
  expect(selectionVisual).toMatchObject({
    luoPressed: "true",
    luoSelected: "true",
    luoDimmed: "false",
    houDimmed: "true",
    nameFilter: "none",
    nameOpacity: 1,
  });
  expect(selectionVisual.visualFilter).toContain("blur");
  expect(selectionVisual.visualOpacity).toBeLessThan(1);
  await expect(page.getByTestId("current-training-role")).toHaveText(
    "当前训练主播：罗雨欣",
  );
});

test("专项训练严格过滤主播和板块，并完成评分、证据与总结闭环", async ({ page }) => {
  test.setTimeout(120_000);
  await openTraining(page);
  const mediaResponse = await page.request.get(TRAINING_ACCEPTANCE_MEDIA_PATH, {
    timeout: 30_000,
  });
  expect(mediaResponse.ok()).toBe(true);
  expect(mediaResponse.headers()["content-type"]).toContain("video/webm");
  expect((await mediaResponse.body()).byteLength).toBeGreaterThan(0);
  await selectRole(page, "ROLE-LUO", "罗雨欣");
  await page.getByTestId("training-mode-specialized").click();
  await page.getByTestId("training-module-objection_handling").click();
  await page.getByTestId("start-training").click();

  await expect(page.getByTestId("training-question")).toContainText(
    "【自动测试夹具】我觉得测试价格偏高，你会怎样回应？",
  );
  await expect(page.getByTestId("training-question")).toContainText(
    "虚拟人物：【自动测试夹具】潜在买家",
  );
  await expect(page.getByTestId("training-question")).toContainText(
    "题目来源：已审核评论题库",
  );
  await submitAnswer(page);

  await expect(page.getByTestId("training-assessment-policy")).toContainText("本题判分依据");
  await expect(page.getByTestId("training-assessment-policy")).toContainText("七维量表");

  for (const [index, [key, score]] of Object.entries(TRAINING_ACCEPTANCE_SCORES).entries()) {
    const scoreCard = page.locator(`[data-score-key="${key}"]`);
    await expect(scoreCard).toContainText(MODULE_LABELS[index]);
    await expect(scoreCard.locator("strong")).toHaveText(String(score));
  }
  await expect(page.getByTestId("training-feedback")).toContainText("当时真实回答（已审核）");
  await expect(page.getByTestId("training-feedback")).toContainText("AUTO-EVIDENCE-");

  const video = page.getByTestId("training-feedback").locator("video");
  await expect(video).toHaveAttribute("src", new RegExp(TRAINING_ACCEPTANCE_MEDIA_PATH.replaceAll("/", "\\/")));
  const mediaState = await video.evaluate(async (element: HTMLVideoElement) => {
    if (element.readyState < HTMLMediaElement.HAVE_METADATA && !element.error) {
      await new Promise<void>((resolve) => {
        const done = () => resolve();
        element.addEventListener("loadedmetadata", done, { once: true });
        element.addEventListener("error", done, { once: true });
        setTimeout(done, 30_000);
      });
    }
    return {
      duration: element.duration,
      readyState: element.readyState,
      errorCode: element.error?.code || 0,
    };
  });
  expect(mediaState.errorCode, "测试证据视频不应触发媒体错误").toBe(0);
  expect(mediaState.readyState, "测试证据视频应加载到元数据阶段").toBeGreaterThanOrEqual(1);
  expect(mediaState.duration, "测试证据视频应可读取大于零的时长").toBeGreaterThan(0);

  await page.getByTestId("complete-training").click();
  await expect(page.getByTestId("training-summary")).toBeVisible();
  await expect(page.getByTestId("training-summary")).toContainText("异议处理");
  await expect(page.getByTestId("training-summary")).toContainText("证据");
  await expect(page.getByTestId("training-summary")).toContainText("反复出现的问题");
  await expect(page.getByTestId("training-summary")).toContainText("下次训练建议");
});

test("综合训练跨板块但不跨主播", async ({ page }) => {
  await openTraining(page);
  await selectRole(page, "ROLE-HOU", "侯梦娜");
  await startComprehensive(page);

  const allowedQuestions = [
    "【自动测试夹具】测试观众质疑成色，侯梦娜训练如何先核对事实？",
    "【自动测试夹具】测试链接异常，侯梦娜训练如何控场？",
  ];
  const seen: string[] = [];

  for (let index = 0; index < allowedQuestions.length; index += 1) {
    const question = (await page.getByTestId("training-question").textContent()) || "";
    const matched = allowedQuestions.find((item) => question.includes(item));
    expect(matched).toBeTruthy();
    seen.push(matched as string);
    expect(question).not.toContain("罗雨欣");
    expect(question).not.toContain("小鹅");

    await submitAnswer(page, `【自动测试夹具】侯梦娜综合训练回答${index + 1}`);
    if (index < allowedQuestions.length - 1) {
      await page.getByTestId("next-question").click();
      await expect(page.getByTestId("training-question")).toBeVisible();
    }
  }

  expect(new Set(seen).size).toBe(2);
  await page.getByTestId("complete-training").click();
  await expect(page.getByTestId("training-summary")).toContainText("侯梦娜");
  await expect(page.getByTestId("training-summary")).toContainText("异议处理");
  await expect(page.getByTestId("training-summary")).toContainText("突发情况");
});

test("人机情景训练完成1轮真实评论和2轮AI模拟追问闭环", async ({ page }) => {
  test.setTimeout(120_000);
  const featurePageErrors: string[] = [];
  const failedTrainingRequests: string[] = [];
  page.on("pageerror", (error) => {
    const detail = error.stack || error.message;
    if (/ScenarioTraining|HumanMachineTraining|trainingApi/i.test(detail)) {
      featurePageErrors.push(detail);
    }
  });
  page.on("requestfailed", (request) => {
    if (/training|evidence-1s\.webm/i.test(request.url())) {
      failedTrainingRequests.push(`${request.method()} ${request.url()}`);
    }
  });
  await page.setViewportSize({ width: 1440, height: 900 });
  await openTraining(page);
  await page.getByTestId("role-card-ROLE-LUO").click();
  await page.getByTestId("experience-human-machine").click();
  await page.getByTestId("human-machine-module-objection_handling").click();
  await page.getByTestId("start-human-machine").click();

  await expect(page.getByTestId("human-machine-training")).toContainText("真实评论 · 已审核");
  await page.screenshot({ path: "test-results/human-machine-training-1440x900.png", fullPage: true });
  await page.setViewportSize({ width: 320, height: 800 });
  await page.screenshot({ path: "test-results/human-machine-training-320x800.png", fullPage: true });
  const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(horizontalOverflow).toBeLessThanOrEqual(1);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.getByTestId("human-machine-answer").fill("请问你主要是什么用途和预算？我先帮你确认适合的选择。");
  await page.getByTestId("submit-human-machine-turn").click();
  await expect(page.getByTestId("human-machine-training")).toContainText("AI模拟追问 · 非真实原话");

  await page.getByTestId("human-machine-answer").fill("价格和库存我需要先核实，我先帮你看一下已确认信息。");
  await page.getByTestId("submit-human-machine-turn").click();
  await expect(page.getByTestId("human-machine-training")).toContainText("第 3 / 3 轮");
  await page.getByTestId("human-machine-answer").fill("确认后我会给你明确选择和下一步，不会先做未核实承诺。");
  await page.getByTestId("submit-human-machine-turn").click();

  const result = page.getByTestId("human-machine-result");
  await expect(result).toContainText("硬规则检查");
  await expect(result).toContainText("七维评分");
  await expect(result).toContainText("当时真实回答");
  await expect(result).toContainText("技能树写回预览");
  await expect(result).toContainText("只展示结果，不写入主播长期技能树");
  await expect(result.locator("video")).toHaveAttribute("src", new RegExp(TRAINING_ACCEPTANCE_MEDIA_PATH.replaceAll("/", "\\/")));
  await page.setViewportSize({ width: 1024, height: 768 });
  await result.getByText("硬规则检查", { exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: "test-results/human-machine-result-1024x768.png", fullPage: true });
  expect(featurePageErrors).toEqual([]);
  expect(failedTrainingRequests).toEqual([]);
});

test("五轮训练由真实评论起题并逐轮生成四个不同方向的追问", async ({ page }) => {
  await openTraining(page);
  await page.getByTestId("role-card-ROLE-LUO").click();
  await page.getByTestId("experience-human-machine").click();
  await page.getByTestId("human-machine-module-objection_handling").click();
  await page.getByTestId("human-machine-turns-5").click();
  await expect(page.getByTestId("start-human-machine")).toContainText("开始5轮");
  await page.getByTestId("start-human-machine").click();

  await expect(page.getByTestId("human-machine-training")).toContainText("第 1 / 5 轮");
  const answers = [
    "请问你主要拍什么、预算多少？我先确认需求。",
    "价格和库存需要核实，我先给你看已确认信息。",
    "我可以用原片和检测结果帮你判断，不做口头保证。",
    "你最犹豫的是成色、配置还是售后？",
    "信息确认后我给你一个明确、合规的下一步。",
  ];
  for (let index = 0; index < answers.length; index += 1) {
    await page.getByTestId("human-machine-answer").fill(answers[index]);
    await page.getByTestId("submit-human-machine-turn").click();
    if (index < answers.length - 1) {
      await expect(page.getByTestId("human-machine-training")).toContainText(`第 ${index + 2} / 5 轮`);
    }
  }

  const result = page.getByTestId("human-machine-result");
  await expect(result).toBeVisible();
  const viewerQuestions = await result.locator("article.viewer p").allTextContents();
  expect(viewerQuestions).toHaveLength(5);
  expect(new Set(viewerQuestions).size).toBe(5);
  await expect(result.getByText("真实评论 · 已审核", { exact: true })).toHaveCount(1);
  await expect(result.getByText("AI模拟追问 · 非真实原话", { exact: true })).toHaveCount(4);
  await expect(result.getByText(/追问方向：/)).toHaveCount(4);
});

test("AI追问失败时保留原回答并支持原样重试", async ({ page }) => {
  await page.addInitScript(() => {
    let attempts = 0;
    const baseScenario = {
      runId: "HM-RETRY-1",
      roleId: "ROLE-RETRY",
      displayName: "罗雨欣",
      module: "objection_handling",
      currentTurn: 1,
      totalTurns: 3,
      status: "active",
      turns: [{ turnIndex: 1, viewerMessage: "这个价格还能谈吗？", viewerSource: "approved_comment" }],
      factConstraints: { price: "待确认" },
      hardChecks: [],
      evaluationStatus: "pending",
      scores: null,
      realAnswer: "我先确认你的预算和使用需求。",
      clipPath: "",
      evidenceId: "AUTO-RETRY-EVIDENCE",
    };
    window.__SCENARIO_TRAINING_TEST_FIXTURE__ = {
      async listTrainingRoles() {
        return [{
          id: "ROLE-RETRY",
          displayName: "罗雨欣",
          description: "【自动测试夹具】AI失败重试场景。",
          availableCaseCount: 1,
          availableModules: ["objection_handling"],
          evidenceStatus: "ready",
        }];
      },
      async getActiveHumanMachineScenario() { return null; },
      async startHumanMachineScenario() { return structuredClone(baseScenario); },
      async submitHumanMachineTurn(request: { traineeAnswer: string }) {
        attempts += 1;
        if (attempts === 1) {
          throw new Error("AI模拟观众暂不可用；本轮回答已保存，可稍后原样重试。");
        }
        return {
          ...structuredClone(baseScenario),
          currentTurn: 2,
          turns: [
            { ...baseScenario.turns[0], traineeAnswer: request.traineeAnswer },
            { turnIndex: 2, viewerMessage: "你的预算范围大概是多少？", viewerSource: "ai_simulated_follow_up", followUpFocus: "needs_confirmation" },
          ],
        };
      },
      async abandonHumanMachineScenario() {},
    };
  });
  await page.goto("/?trainingFixture=acceptance-v1", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "情景训练", exact: true }).click();
  await page.getByTestId("role-card-ROLE-RETRY").click();
  await page.getByTestId("experience-human-machine").click();
  await page.getByTestId("human-machine-module-objection_handling").click();
  await page.getByTestId("start-human-machine").click();

  const answer = "我先确认你的预算和主要拍摄需求。";
  await page.getByTestId("human-machine-answer").fill(answer);
  await page.getByTestId("submit-human-machine-turn").click();
  await expect(page.getByTestId("human-machine-retry-error")).toContainText("本轮回答已保存");
  await expect(page.getByTestId("human-machine-answer")).toHaveValue(answer);
  await expect(page.getByTestId("submit-human-machine-turn")).toContainText("原样重试生成追问");

  await page.getByTestId("submit-human-machine-turn").click();
  await expect(page.getByTestId("human-machine-training")).toContainText("第 2 / 3 轮");
  await expect(page.getByTestId("human-machine-training")).toContainText("追问方向：需求确认");
  await expect(page.getByTestId("human-machine-answer")).toHaveValue("");
});

test("无可用证据不补题，也不发起训练或外部AI请求", async ({ page }) => {
  const trainingRequests: string[] = [];
  page.on("request", (request) => {
    if (/training|openai|anthropic|deepseek/i.test(request.url())) {
      trainingRequests.push(request.url());
    }
  });

  await openTraining(page);
  trainingRequests.length = 0;
  await page.getByTestId("role-card-ROLE-YU").click();
  await expect(page.getByTestId("training-empty-evidence")).toBeVisible();
  await expect(page.getByTestId("training-empty-evidence")).toContainText("暂无经审核训练证据");
  await expect(page.getByTestId("training-question")).toHaveCount(0);
  expect(trainingRequests).toEqual([]);
});

test("切换主播清空旧题、答案、反馈、板块和证据", async ({ page }) => {
  await openTraining(page);
  await selectRole(page, "ROLE-LUO", "罗雨欣");
  await page.getByTestId("training-mode-specialized").click();
  await page.getByTestId("training-module-objection_handling").click();
  await page.getByTestId("start-training").click();
  await submitAnswer(page, "【自动测试夹具】即将被清空的回答");

  await page.getByTestId("switch-role").click();
  await expect(page.getByTestId("training-role-selection")).toBeVisible();
  await expect(page.getByTestId("training-question")).toHaveCount(0);
  await expect(page.getByTestId("training-answer")).toHaveCount(0);
  await expect(page.getByTestId("training-feedback")).toHaveCount(0);
  await expect(page.getByTestId("current-training-role")).toHaveCount(0);

  await selectRole(page, "ROLE-HOU", "侯梦娜");
  await startComprehensive(page);
  await expect(page.getByTestId("training-question")).toContainText("侯梦娜");
  await expect(page.getByTestId("training-question")).not.toContainText("价格偏高");
});

test("角色卡支持Tab、Enter与Space，并保持清晰焦点", async ({ page }) => {
  await openTraining(page);
  const luo = page.getByTestId("role-card-ROLE-LUO");
  const hou = page.getByTestId("role-card-ROLE-HOU");
  await luo.focus();
  await expect(luo).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(hou).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("current-training-role")).toHaveText(
    "当前训练主播：侯梦娜",
  );

  await page.getByTestId("switch-role").click();
  const xiaoe = page.getByTestId("role-card-ROLE-XIAOE");
  await xiaoe.focus();
  await page.keyboard.press("Space");
  await expect(page.getByTestId("current-training-role")).toHaveText(
    "当前训练主播：小鹅",
  );
});

test("减少动态效果时选择状态不依赖动画完成", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openTraining(page);

  const visual = page.getByTestId("role-visual-ROLE-LUO");
  await expect(visual).toBeVisible();
  const transitionDuration = await visual.evaluate(
    (element) => getComputedStyle(element).transitionDuration,
  );
  expect(transitionDuration.split(",").every((value) => parseFloat(value) <= 0.01)).toBe(true);

  await page.getByTestId("role-card-ROLE-LUO").click();
  await expect(page.getByTestId("current-training-role")).toHaveText(
    "当前训练主播：罗雨欣",
  );
});

test("异常评分显示待复核，不自动补齐缺失维度", async ({ page }) => {
  await openTraining(page);
  await selectRole(page, "ROLE-LUO", "罗雨欣");
  await page.getByTestId("training-mode-specialized").click();
  await page.getByTestId("training-module-objection_handling").click();
  await page.getByTestId("start-training").click();
  await submitAnswer(page, "【自动测试夹具：异常评分】");

  await expect(page.getByTestId("training-feedback")).toContainText("待复核");
  await expect(page.locator('[data-score-key="livePacing"] strong')).toHaveText("待复核");
});

test("延迟启动返回时若已切换主播，会清理旧会话且不污染新角色", async ({ page }) => {
  await openTrainingWithDelayedStart(page);
  await selectRole(page, "ROLE-LUO", "罗雨欣");
  await page.getByTestId("start-training").click();

  // Switching remains available while start is pending. Do not wait for or
  // inspect the 240ms visual echo: business state must be independent of it.
  await page.getByTestId("switch-role").click();
  await expect(page.getByTestId("training-role-selection")).toBeVisible();
  await selectRole(page, "ROLE-HOU", "侯梦娜");

  await expect
    .poll(() => page.evaluate(() => window.__ABANDONED_TRAINING_SESSIONS__ || []))
    .toContain("DELAYED-ROLE-LUO");
  await expect(page.getByTestId("current-training-role")).toHaveText(
    "当前训练主播：侯梦娜",
  );
  await expect(page.getByTestId("training-question")).toHaveCount(0);
});
