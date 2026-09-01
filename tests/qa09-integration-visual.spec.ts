import path from "node:path";
import { mkdir } from "node:fs/promises";
import { expect, test, type Page, type TestInfo } from "@playwright/test";
import {
  CONTENT_ANALYSIS_FIXTURE,
  FULL_SKILL_RATING_HISTORY_FIXTURE,
  STREAMER_ARCHIVE_FIXTURE,
  STREAMER_RECORDER_FIXTURE,
} from "./fixtures/streamer-archive-fixture";
import type { RecordItem } from "../src/lib/db";

declare global {
  interface Window {
    __SCENARIO_TRAINING_TEST_FIXTURE__?: string;
  }
}

const artifactRoot = process.env.QA09_ARTIFACT_ROOT
  ? path.resolve(process.env.QA09_ARTIFACT_ROOT)
  : path.resolve("test-results/qa09");
const screenshotRoot = path.join(artifactRoot, "截图");
const viewports = [
  { width: 320, height: 800 },
  { width: 768, height: 900 },
  { width: 1440, height: 1000 },
] as const;

const browserErrors = new WeakMap<Page, string[]>();
const unexpectedCommands = new WeakMap<Page, string[]>();

test.beforeAll(async () => {
  await mkdir(screenshotRoot, { recursive: true });
});

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  browserErrors.set(page, errors);
  unexpectedCommands.set(page, []);
  page.on("pageerror", (error) => errors.push(`pageerror: ${error.message}`));
  page.on("console", (message) => {
    if (message.type() === "error") {
      const location = message.location();
      const source = location.url
        ? ` @ ${location.url}:${location.lineNumber ?? 0}:${location.columnNumber ?? 0}`
        : "";
      errors.push(`console.error: ${message.text()}${source}`);
    }
  });
});

test.afterEach(async ({ page }, testInfo) => {
  const errors = browserErrors.get(page) || [];
  const unknown = unexpectedCommands.get(page) || [];
  await testInfo.attach("browser-errors", {
    body: Buffer.from(JSON.stringify({ errors, unexpectedCommands: unknown }, null, 2)),
    contentType: "application/json",
  });
  expect(errors, "浏览器 console.error/pageerror 必须为零").toEqual([]);
  expect(unknown, "固定夹具不允许出现未建模的 Tauri 命令").toEqual([]);
});

async function screenshot(page: Page, testInfo: TestInfo, name: string): Promise<void> {
  const target = path.join(screenshotRoot, `${name}.png`);
  await page.screenshot({ path: target, fullPage: true });
  await testInfo.attach(name, { path: target, contentType: "image/png" });
}

async function inspectLayout(
  page: Page,
  testInfo: TestInfo,
  label: string,
  contentSelector: string,
  minimumCoverage: number,
): Promise<void> {
  const metrics = await page.evaluate(({ selector }) => {
    const documentElement = document.documentElement;
    const content = document.querySelector<HTMLElement>(selector);
    const sidebar = document.querySelector<HTMLElement>(".sidebar");
    if (!content) throw new Error(`未找到主内容容器：${selector}`);
    const contentRect = content.getBoundingClientRect();
    const sidebarRect = sidebar?.getBoundingClientRect();
    const availableWidth = Math.max(1, window.innerWidth - (sidebarRect?.width || 0));
    return {
      viewportWidth: window.innerWidth,
      documentClientWidth: documentElement.clientWidth,
      documentScrollWidth: documentElement.scrollWidth,
      noDocumentOverflow: documentElement.scrollWidth <= documentElement.clientWidth + 1,
      contentLeft: Number(contentRect.left.toFixed(1)),
      contentWidth: Number(contentRect.width.toFixed(1)),
      contentCoverage: Number((contentRect.width / window.innerWidth).toFixed(3)),
      availableCoverage: Number((contentRect.width / availableWidth).toFixed(3)),
      sidebarWidth: Number((sidebarRect?.width || 0).toFixed(1)),
    };
  }, { selector: contentSelector });
  await testInfo.attach(`${label}-layout`, {
    body: Buffer.from(JSON.stringify(metrics, null, 2)),
    contentType: "application/json",
  });
  expect(metrics.noDocumentOverflow, `${label} 不得出现页面级横向溢出`).toBe(true);
  expect(metrics.contentCoverage, `${label} 主内容覆盖率不得低于 ${minimumCoverage}`).toBeGreaterThanOrEqual(minimumCoverage);
}

async function expectReducedMotion(page: Page, locator: string): Promise<void> {
  const durations = await page.locator(locator).first().evaluate((element) =>
    getComputedStyle(element).transitionDuration.split(",").map((raw) => {
      const value = raw.trim();
      const number = Number.parseFloat(value) || 0;
      return value.endsWith("ms") ? number / 1000 : number;
    })
  );
  expect(durations.every((duration) => duration <= 0.01), `${locator} 应遵守减少动态效果`).toBe(true);
}

async function expectMinimumHeight(page: Page, locator: string, minimum = 44): Promise<void> {
  const height = await page.locator(locator).first().evaluate((element) => element.getBoundingClientRect().height);
  expect(height, `${locator} 交互高度不得低于 ${minimum}px`).toBeGreaterThanOrEqual(minimum);
}

async function installBrowserTauriEventMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.__BSR_DISABLE_SOCKET_IO_FOR_TESTS__ = true;
    let callbackId = 0;
    const eventInternals = {
      transformCallback: () => ++callbackId,
      unregisterCallback: () => undefined,
      invoke: async (command: string) => command === "plugin:event|listen" ? ++callbackId : null,
      convertFileSrc: (filePath: string) => filePath,
    };
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      get() {
        const stack = new Error().stack || "";
        return stack.includes("/src/lib/invoker.ts") ? undefined : eventInternals;
      },
    });
    Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {
      configurable: true,
      value: { unregisterListener: () => undefined },
    });
  });
}

async function installAppFixture(page: Page): Promise<void> {
  const mutableArchives = STREAMER_ARCHIVE_FIXTURE.map((item) => ({ ...item }));
  await page.addInitScript(({ analysis, ratings }) => {
    for (const [sourceKey, value] of Object.entries(analysis)) {
      localStorage.setItem(`bsr:content-analysis:v3:${sourceKey}`, JSON.stringify(value));
    }
    localStorage.setItem("bsr:streamer-skill-ratings:v1", JSON.stringify(ratings));
  }, { analysis: CONTENT_ANALYSIS_FIXTURE, ratings: FULL_SKILL_RATING_HISTORY_FIXTURE });
  await page.route("https://api.github.com/**", async (route) => {
    await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
  });
  await page.route("**/api/**", async (route) => {
    const command = new URL(route.request().url()).pathname.split("/").pop() || "";
    let body: Record<string, unknown> = {};
    try {
      body = (route.request().postDataJSON() || {}) as Record<string, unknown>;
    } catch {
      body = {};
    }
    let data: unknown = null;

    if (command === "get_recorder_list") {
      data = STREAMER_RECORDER_FIXTURE;
    } else if (command === "get_archives") {
      const roomId = String(body.roomId || "");
      const offset = Number(body.offset || 0);
      const limit = Number(body.limit || 100);
      data = mutableArchives.filter((item) => item.room_id === roomId).slice(offset, offset + limit);
    } else if (command === "get_all_videos") {
      data = [];
    } else if (command === "get_config") {
      data = {
        cache: "", output: "", primary_uid: 0, live_start_notify: true, live_end_notify: true,
        clip_notify: true, post_notify: true, auto_cleanup: true, auto_subtitle: false,
        subtitle_generator_type: "funasr", openai_api_endpoint: "", openai_api_key: "",
        volcengine_api_key: "", volcengine_app_id: "", volcengine_access_token: "",
        volcengine_resource_id: "volc.seedasr.auc", volcengine_boosting_table_id: "",
        volcengine_correct_table_id: "", admin_mode: false, powerlive_key: "",
        knowledge_vault_path: "", whisper_model: "", whisper_prompt: "", clip_name_format: "",
        auto_generate: { enabled: false, encode_danmu: false },
        nas_video_storage: { enabled: false, root_path: "", archive_recordings: true, archive_imports: true, delete_local_after_archive: true },
        autostart_enabled: true, startup_wizard_completed: true, status_check_interval: 30,
        whisper_language: "", webhook_url: "", danmu_ass_options: { font_size: 36, opacity: 0.8 },
      };
    } else if (command === "get_accounts") {
      data = { accounts: [] };
    } else if (["get_tasks", "get_recorder_health", "get_recent_record", "list_video_archives", "list_master_sample_batches"].includes(command)) {
      data = [];
    } else if (["get_archive_disk_usage", "get_account_count", "get_total_length", "get_today_record_count"].includes(command)) {
      data = 0;
    } else if (command === "get_disk_info") {
      data = { disk: "", total: 0, free: 0 };
    } else if (command === "get_storage_migration_status") {
      data = { cache: false, output: false };
    } else if (command === "get_storage_runtime_status") {
      data = {
        preferredCache: "",
        activeCache: "",
        usingFallback: false,
        preferredAvailable: true,
        detail: "fixture",
      };
    } else if (command === "get_minimax_setup_status") {
      data = { configured: true };
    } else if (command === "get_startup_readiness") {
      data = {
        wizardCompleted: true, autostartEnabled: true, accountCount: 0,
        recorderCount: STREAMER_RECORDER_FIXTURE.count, ffmpegOk: true, ffmpegDetail: "fixture",
        funasrOk: true, funasrDetail: "fixture", cacheOk: true, cacheDetail: "fixture",
        outputOk: true, outputDetail: "fixture", nasConfigured: false,
        doudianConfigured: false, readyForRecording: true,
      };
    } else if (command === "auto_classify_archive_kinds") {
      data = [];
    } else if (command === "list_live_dashboard_bindings_for_live_ids") {
      data = [];
    } else if (command === "resolve_live_dashboard_for_record") {
      data = { session: null, matchMethod: null };
    } else if (command === "detect_archive_anchor") {
      data = mutableArchives.find((item) => item.live_id === String(body.liveId || "")) || null;
    } else if (command === "set_archive_kind") {
      const liveId = String(body.liveId || "");
      const archiveKind = String(body.archiveKind || "") as RecordItem["archive_kind"];
      const index = mutableArchives.findIndex((item) => item.live_id === liveId);
      if (index >= 0) {
        mutableArchives[index] = { ...mutableArchives[index], archive_kind: archiveKind, classification_source: "manual" };
        data = mutableArchives[index];
      }
    } else if (command !== "console_log") {
      const commands = unexpectedCommands.get(page) || [];
      commands.push(command);
      unexpectedCommands.set(page, commands);
    }

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ code: 0, message: "ok", data }),
    });
  });
}

for (const viewport of viewports) {
  test(`05 曲线 ${viewport.width}px：指标、时间、分析、证据与截图`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/tests/fixtures/compass-curve-harness.html");
    await expect(page.getByRole("heading", { name: "罗盘原始动态曲线验收" })).toBeVisible();

    const online = page.getByRole("button", { name: "在线人数", exact: true });
    const spend = page.getByRole("button", { name: "投放消耗", exact: true });
    await spend.focus();
    await page.keyboard.press("Space");
    await expect(spend).toHaveAttribute("aria-pressed", "true");
    await expect(page.locator(".analysis-grid > article")).toHaveCount(2);

    const end = page.getByLabel("曲线分析结束时间");
    await end.focus();
    await page.keyboard.press("ArrowLeft");
    await expect(end).toHaveValue(String(10 * 3600 + 8 * 60));
    const seek = page.locator("button:enabled").filter({ hasText: "同步录播/文稿" }).first();
    await seek.click();
    await expect(page.getByText("已同步到 360 秒", { exact: true })).toBeVisible();

    await expectMinimumHeight(page, 'button[aria-pressed="true"]');
    await expectReducedMotion(page, ".curve-workbench button");
    await screenshot(page, testInfo, `05-curve-${viewport.width}-linked-evidence`);
    await page.getByRole("button", { name: "恢复整场" }).click();
    await expect(page.getByText(/待核实：当前时间段没有逐字稿或节奏地图证据/).first()).toBeVisible();
    await inspectLayout(page, testInfo, `05-curve-${viewport.width}`, ".curve-workbench", 0.9);
    await expect(online).toBeVisible();
  });

  test(`07 训练 ${viewport.width}px：角色、专项、对话、评分、证据与截图`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await installBrowserTauriEventMock(page);
    await installAppFixture(page);
    await page.addInitScript(() => {
      window.__SCENARIO_TRAINING_TEST_FIXTURE__ = "acceptance-v1";
    });
    await page.goto("/?trainingFixture=acceptance-v1", { waitUntil: "domcontentloaded" });
    await page.getByRole("button", { name: "情景训练", exact: true }).click();
    await expect(page.getByTestId("training-role-selection")).toBeVisible();
    await expectMinimumHeight(page, '[data-testid="role-card-ROLE-LUO"]');
    await expectReducedMotion(page, '[data-testid="role-visual-ROLE-LUO"]');
    await screenshot(page, testInfo, `07-training-${viewport.width}-role-selection`);

    const role = page.getByTestId("role-card-ROLE-LUO");
    await role.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("current-training-role")).toHaveText("当前训练主播：罗雨欣");
    await page.getByTestId("training-mode-specialized").click();
    await page.getByTestId("training-module-objection_handling").click();
    await page.getByTestId("start-training").click();
    await expect(page.getByTestId("training-question")).toContainText("价格偏高");
    await expect(page.getByTestId("training-question")).toContainText("不代表主播说过这句话");
    await page.getByTestId("training-answer").fill("【09 联调固定夹具】学员现场回答");
    await page.getByTestId("submit-answer").click();
    await expect(page.getByTestId("training-feedback")).toBeVisible();
    await expect(page.locator("[data-score-key]")).toHaveCount(7);
    await expect(page.getByTestId("training-feedback")).toContainText("当时真实回答（已审核）");
    await expect(page.getByTestId("training-feedback")).toContainText("AI练习建议，非主播原话");
    await expect(page.getByTestId("training-feedback")).toContainText("AUTO-EVIDENCE-");
    await screenshot(page, testInfo, `07-training-${viewport.width}-feedback-evidence`);
    await page.getByTestId("complete-training").click();
    await expect(page.getByTestId("training-summary")).toContainText("异议处理");
    await expect(page.getByTestId("training-summary")).toContainText("下次训练建议");
    await inspectLayout(page, testInfo, `07-training-${viewport.width}`, ".content", 0.65);
  });

  test(`08 档案 ${viewport.width}px：主播卡、场次、八维雷达与截图`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await installBrowserTauriEventMock(page);
    await installAppFixture(page);
    await page.goto("/", { waitUntil: "commit", timeout: 60_000 });
    await page.getByRole("button", { name: "录播", exact: true }).click({ timeout: 60_000 });
    await expect(page.getByRole("heading", { name: "主播档案", exact: true })).toBeVisible({ timeout: 60_000 });
    const luo = page.getByRole("button", { name: /打开罗雨欣的主播档案/ });
    await expect(luo).toContainText("2 场");
    await expectMinimumHeight(page, '[data-streamer-key="罗雨欣"]');
    await expectReducedMotion(page, '[data-streamer-key="罗雨欣"]');
    await screenshot(page, testInfo, `08-profile-${viewport.width}-directory`);

    await luo.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { name: "罗雨欣", exact: true })).toBeVisible();
    await expect(page.locator(".score-table-wrap tbody > tr")).toHaveCount(8);
    await expect(page.getByRole("img", { name: /罗雨欣八维能力雷达图/ })).toBeVisible();
    await expect(page.locator(".mac-table tbody > tr")).toHaveCount(2);
    await expect(page.getByRole("cell", { name: "罗雨欣", exact: true })).toHaveCount(2);
    await screenshot(page, testInfo, `08-profile-${viewport.width}-radar-sessions`);

    await page.getByLabel("商品筛选").fill("S9");
    await expect(page.locator(".mac-table tbody > tr")).toHaveCount(1);
    await page.getByRole("button", { name: "清空筛选" }).click();
    await expect(page.locator(".mac-table tbody > tr")).toHaveCount(2);
    await page.getByRole("button", { name: "返回主播列表" }).click();
    await expect(luo).toBeFocused();
    await inspectLayout(page, testInfo, `08-profile-${viewport.width}`, ".content", 0.65);
  });
}
