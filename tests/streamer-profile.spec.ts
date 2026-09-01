import { expect, test, type Page } from "@playwright/test";
import {
  CONTENT_ANALYSIS_FIXTURE,
  EMPTY_COMPANY_ARCHIVE_FIXTURE,
  FULL_SKILL_RATING_HISTORY_FIXTURE,
  SINGLE_SOURCE_SKILL_RATING_HISTORY_FIXTURE,
  STREAMER_ARCHIVE_FIXTURE,
  STREAMER_RECORDER_FIXTURE,
  STREAMER_RECORDER_MISMATCH_FIXTURE,
} from "./fixtures/streamer-archive-fixture";
import type { RecordItem } from "../src/lib/db";
import type { RecorderList } from "../src/lib/interface";
import type { SkillRatingVersion } from "../src/lib/streamerProfile";

const pageErrors = new WeakMap<Page, string[]>();
const unexpectedCommands = new WeakMap<Page, string[]>();
const LOCAL_API_ROUTE = /^http:\/\/127\.0\.0\.1:\d+\/api\/.*$/;

const WEB_CONFIG_FIXTURE = {
  cache: "",
  output: "",
  primary_uid: 0,
  live_start_notify: true,
  live_end_notify: true,
  clip_notify: true,
  post_notify: true,
  auto_cleanup: true,
  auto_subtitle: false,
  subtitle_generator_type: "funasr",
  openai_api_endpoint: "",
  openai_api_key: "",
  volcengine_api_key: "",
  volcengine_app_id: "",
  volcengine_access_token: "",
  volcengine_resource_id: "volc.seedasr.auc",
  volcengine_boosting_table_id: "",
  volcengine_correct_table_id: "",
  admin_mode: false,
  powerlive_key: "",
  knowledge_vault_path: "",
  whisper_model: "",
  whisper_prompt: "",
  clip_name_format: "",
  auto_generate: { enabled: false, encode_danmu: false },
  nas_video_storage: {
    enabled: false,
    root_path: "",
    archive_recordings: true,
    archive_imports: true,
    delete_local_after_archive: true,
  },
  autostart_enabled: true,
  startup_wizard_completed: true,
  status_check_interval: 30,
  whisper_language: "",
  webhook_url: "",
  danmu_ass_options: { font_size: 36, opacity: 0.8 },
};

async function installBrowserTauriEventMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    let callbackId = 0;
    const eventInternals = {
      transformCallback: () => ++callbackId,
      unregisterCallback: () => undefined,
      invoke: async (command: string) => command === "plugin:event|listen" ? ++callbackId : null,
      convertFileSrc: (path: string) => path,
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

async function installAnalysisCache(page: Page): Promise<void> {
  await page.addInitScript((entries) => {
    for (const [sourceKey, value] of Object.entries(entries)) {
      localStorage.setItem(`bsr:content-analysis:v3:${sourceKey}`, JSON.stringify(value));
    }
  }, CONTENT_ANALYSIS_FIXTURE);
}

async function installRatingHistory(page: Page, history: readonly SkillRatingVersion[]): Promise<void> {
  await page.addInitScript((ratings) => {
    localStorage.setItem("bsr:streamer-skill-ratings:v1", JSON.stringify(ratings));
  }, history);
}

async function mockArchiveApi(
  page: Page,
  fixture: RecordItem[] = STREAMER_ARCHIVE_FIXTURE,
  recorderFixture: RecorderList = STREAMER_RECORDER_FIXTURE,
): Promise<void> {
  const mutableFixture = fixture.map((item) => ({ ...item }));
  await page.route("https://api.github.com/**", async (route) => {
    await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
  });
  await page.route(LOCAL_API_ROUTE, async (route) => {
    const command = new URL(route.request().url()).pathname.split("/").pop() || "";
    const body = route.request().postDataJSON?.() || {};
    let data: unknown = null;

    if (command === "get_recorder_list") {
      data = recorderFixture;
    } else if (command === "get_archives") {
      const roomId = String(body.roomId || "");
      const offset = Number(body.offset || 0);
      const limit = Number(body.limit || 100);
      data = mutableFixture.filter((item) => item.room_id === roomId).slice(offset, offset + limit);
    } else if (command === "get_all_videos") {
      data = [];
    } else if (command === "get_config") {
      data = WEB_CONFIG_FIXTURE;
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
    } else if (command === "get_minimax_setup_status") {
      data = { configured: true };
    } else if (command === "get_archive_subtitle") {
      data = "";
    } else if (command === "refresh_archive_subtitle") {
      data = {
        subtitle: "1\n00:00:00,000 --> 00:00:02,000\n能力画像自动准备夹具\n",
        decision: "created",
        similarity: 0,
        oldLength: 0,
        newLength: 30,
      };
    } else if (command === "minimax_chat") {
      data = JSON.stringify({
        ratings: [
          {
            dimension: "needs_confirmation",
            score: 82,
            basis: "两场录播都出现了需求复述和对应证据，样本目前只覆盖相机答疑。",
            evidenceIds: [
              "archive:douyin:room-luo:live-luo-001#L1-C1",
              "archive:douyin:room-luo:live-luo-002#L2-C1",
            ],
          },
        ],
      });
    } else if (command === "get_startup_readiness") {
      data = {
        wizardCompleted: true,
        autostartEnabled: true,
        accountCount: 0,
        recorderCount: recorderFixture.count,
        ffmpegOk: true,
        ffmpegDetail: "fixture",
        funasrOk: true,
        funasrDetail: "fixture",
        cacheOk: true,
        cacheDetail: "fixture",
        outputOk: true,
        outputDetail: "fixture",
        nasConfigured: false,
        doudianConfigured: false,
        readyForRecording: true,
      };
    } else if (command === "auto_classify_archive_kinds") {
      data = [];
    } else if (command === "list_live_dashboard_bindings_for_live_ids") {
      data = [];
    } else if (command === "resolve_live_dashboard_for_record") {
      data = { session: null, matchMethod: null };
    } else if (command === "detect_archive_anchor") {
      data = mutableFixture.find((item) => item.live_id === String(body.liveId || "")) || null;
    } else if (command === "set_archive_kind") {
      const liveId = String(body.liveId || "");
      const archiveKind = String(body.archiveKind || "") as RecordItem["archive_kind"];
      const index = mutableFixture.findIndex((item) => item.live_id === liveId);
      if (index >= 0) {
        mutableFixture[index] = {
          ...mutableFixture[index],
          archive_kind: archiveKind,
          classification_source: "manual",
        };
        data = mutableFixture[index];
      }
    } else if (command !== "console_log") {
      const calls = unexpectedCommands.get(page) || [];
      calls.push(command);
      unexpectedCommands.set(page, calls);
    }

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ code: 0, message: "ok", data }),
    });
  });
}

async function openArchive(page: Page): Promise<void> {
  await page.goto("/", { waitUntil: "commit", timeout: 60_000 });
  await page.getByRole("button", { name: "录播", exact: true }).click({ timeout: 60_000 });
  await expect(page.getByRole("heading", { name: "主播档案", exact: true })).toBeVisible({ timeout: 60_000 });
  await expect.poll(async () => visiblePage(page).evaluate((element) => {
    const style = getComputedStyle(element);
    const parent = element.parentElement?.getBoundingClientRect();
    const bounds = element.getBoundingClientRect();
    return {
      opaque: Number.parseFloat(style.opacity) >= 0.999,
      aligned: Boolean(parent) && Math.abs(bounds.left - parent.left) <= 1,
    };
  })).toEqual({ opaque: true, aligned: true });
}

function visiblePage(page: Page) {
  return page.locator(".page.visible");
}

function visibleArchiveRows(page: Page) {
  return visiblePage(page).locator(".mac-table tbody > tr");
}

function archiveRowWithTitle(page: Page, title: string) {
  return visibleArchiveRows(page).filter({ hasText: title });
}

async function expectVisiblePageHasNoHorizontalOverflow(page: Page): Promise<void> {
  const pageScroller = page.locator(".page.visible .mac-page");
  await expect(pageScroller).toBeVisible();
  expect(await pageScroller.evaluate((element) =>
    element.scrollWidth <= element.clientWidth + 1
  )).toBe(true);
}

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  pageErrors.set(page, errors);
  unexpectedCommands.set(page, []);
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console.error: ${message.text()}`);
  });
  await installBrowserTauriEventMock(page);
  await installAnalysisCache(page);
  await mockArchiveApi(page);
});

test.afterEach(async ({ page }) => {
  expect(pageErrors.get(page) || []).toEqual([]);
  expect(unexpectedCommands.get(page) || []).toEqual([]);
});

test("公司录播默认按主播聚合，可靠直播状态与待确认均可读", async ({ page }) => {
  await openArchive(page);

  const luoCard = page.getByRole("button", { name: /打开罗雨欣的主播档案/ });
  await expect(luoCard).toContainText("2 场");
  await expect(luoCard).toContainText("最近直播");
  await expect(luoCard).toContainText("2026/08/27");
  await expect(luoCard).toContainText("分析完成");
  await expect(luoCard).toContainText("1/2");
  await expect(luoCard).toContainText("评分置信度");
  await expect(luoCard).toContainText("数据不足");
  await expect(luoCard).toContainText("正在直播");
  await expect(luoCard.locator('[role="img"][aria-label*="罗雨欣的虚拟角色"]')).toBeVisible();
  await expect(page.getByRole("button", { name: "打开张敏的主播档案" })).toContainText("1 场");
  await expect(page.getByRole("button", { name: /打开待确认主播列表/ })).toContainText("3");
  await expect(page.locator("[data-streamer-key]")).toHaveCount(3);
  await expect(visiblePage(page).getByText("竞品 S9 对比专场", { exact: true })).toHaveCount(0);
  await expect(visiblePage(page).getByText("room-competitor", { exact: true })).toHaveCount(0);
  await expect(visiblePage(page).locator(".mac-table")).toHaveCount(0);
});

test("人工确认别名小鸦归入小鹅，目录与详情统一显示规范姓名", async ({ page }) => {
  const aliasBase = STREAMER_ARCHIVE_FIXTURE[0];
  const aliasFixture: RecordItem[] = [
    ...STREAMER_ARCHIVE_FIXTURE,
    {
      ...aliasBase,
      live_id: "live-xiao-e-001",
      title: "小鹅 微单答疑",
      anchor_name: "小鹅",
      created_at: "2026-08-26T04:00:00.000Z",
    },
    {
      ...aliasBase,
      live_id: "live-xiao-ya-legacy-001",
      title: "历史小鸦标签场次",
      anchor_name: "小鸦",
      created_at: "2026-08-27T04:00:00.000Z",
    },
  ];
  await page.unroute(LOCAL_API_ROUTE);
  await mockArchiveApi(page, aliasFixture);
  await openArchive(page);

  const xiaoECard = page.getByRole("button", { name: /打开小鹅的主播档案/ });
  await expect(xiaoECard).toContainText("2 场");
  await expect(page.getByRole("button", { name: /打开小鸦的主播档案/ })).toHaveCount(0);
  await xiaoECard.click();

  await expect(page.getByRole("heading", { name: "小鹅", exact: true })).toBeVisible();
  await expect(visibleArchiveRows(page)).toHaveCount(2);
  await expect(archiveRowWithTitle(page, "小鹅 微单答疑")).toBeVisible();
  await expect(archiveRowWithTitle(page, "历史小鸦标签场次")).toBeVisible();
  await expect(visiblePage(page).getByRole("cell", { name: "小鹅", exact: true })).toHaveCount(2);
  await expect(visiblePage(page).getByRole("cell", { name: "小鸦", exact: true })).toHaveCount(0);
});

test("虚拟角色外观可按主播保存，且不改变能力证据", async ({ page }) => {
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  const avatarTrigger = page.getByRole("button", { name: "更换形象" });
  await avatarTrigger.click();
  const dialog = page.getByRole("dialog", { name: "选择你的虚拟角色" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("角色只代表个人外观，不会改变能力分数、直播归属或证据。", { exact: true })).toBeVisible();
  await dialog.getByRole("tab", { name: "虚拟人物" }).click();
  await dialog.getByRole("button", { name: /虚拟·青空/ }).click();
  await dialog.getByRole("button", { name: "使用此形象" }).click();

  await expect(page.getByText("角色：虚拟·青空", { exact: true })).toBeVisible();
  await expect(page.getByRole("row", { name: /需求确认/ }).getByText("数据不足", { exact: true }).first()).toBeVisible();
  await avatarTrigger.click();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(avatarTrigger).toBeVisible();

  await page.getByRole("button", { name: "返回主播列表" }).click();
  await expect(page.getByRole("img", { name: "罗雨欣的虚拟角色：虚拟·青空" })).toBeVisible();
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("bsr:streamer-virtual-avatar:v1") || "[]"))).toEqual([
    expect.objectContaining({ streamerKey: "罗雨欣", avatarId: "virtual-azure" }),
  ]);
});

test("同直播间但场次标识不匹配时不猜测正在直播", async ({ page }) => {
  await page.unroute(LOCAL_API_ROUTE);
  await mockArchiveApi(page, STREAMER_ARCHIVE_FIXTURE, STREAMER_RECORDER_MISMATCH_FIXTURE);
  await openArchive(page);
  await expect(page.getByRole("button", { name: /打开罗雨欣的主播档案/ })).not.toContainText("正在直播");
});

test("卡片下钻只显示当前主播，主播标签可见，筛选返回不串档", async ({ page }) => {
  const coverRequests: string[] = [];
  page.on("request", (request) => {
    const pathname = new URL(request.url()).pathname;
    if (pathname.includes("/cache/") && pathname.endsWith("/cover.jpg")) {
      coverRequests.push(request.url());
    }
  });
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  await expect(page.getByRole("heading", { name: "罗雨欣", exact: true })).toBeVisible();
  await expect(visibleArchiveRows(page)).toHaveCount(2);
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toBeVisible();
  await expect(archiveRowWithTitle(page, "张敏 S5M2 新人专场")).toHaveCount(0);
  await expect(visiblePage(page).getByRole("cell", { name: "罗雨欣", exact: true })).toHaveCount(2);
  await expect(visibleArchiveRows(page).getByRole("img", { name: /暂无封面：罗雨欣/ })).toHaveCount(2);
  await expect(visibleArchiveRows(page).locator('img[alt$="封面"]')).toHaveCount(0);
  expect(coverRequests, "空 cover 不应请求约定但不存在的 cover.jpg").toEqual([]);

  await page.getByLabel("商品筛选").fill("S9");
  await page.getByLabel("分析状态筛选").selectOption("complete");
  await expect(visibleArchiveRows(page)).toHaveCount(1);
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toHaveCount(0);

  await page.getByRole("button", { name: "清空筛选" }).click();
  await expect(visibleArchiveRows(page)).toHaveCount(2);
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toBeVisible();

  await page.getByLabel("开始日期").fill("2026-08-25");
  await expect(visibleArchiveRows(page)).toHaveCount(1);
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toHaveCount(0);
  await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toBeVisible();

  await page.getByRole("button", { name: "返回主播列表" }).click();
  await page.getByRole("button", { name: "打开张敏的主播档案" }).click();
  await expect(visibleArchiveRows(page)).toHaveCount(1);
  await expect(archiveRowWithTitle(page, "张敏 S5M2 新人专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toHaveCount(0);
  await expect(visiblePage(page).getByRole("cell", { name: "张敏", exact: true })).toHaveCount(1);
});

test("单来源评分显示数据不足并保留可读样本与证据版本", async ({ page }) => {
  await installRatingHistory(page, SINGLE_SOURCE_SKILL_RATING_HISTORY_FIXTURE);
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  const scoreRow = page.getByRole("row", { name: /需求确认/ });
  await expect(scoreRow.getByText("数据不足", { exact: true }).first()).toBeVisible();
  await expect(scoreRow.getByText("本期 1", { exact: true })).toBeVisible();
  await expect(page.locator("svg .current-polygon")).toHaveCount(0);
  await expect(page.locator("svg .current-marker")).toHaveCount(0);
  await expect(page.getByText(/0\/8 维达到每维 2 个来源门槛/)).toBeVisible();

  await scoreRow.getByRole("button", { name: /查看需求确认的 1 条证据/ }).click();
  await expect(page.getByText("需求确认 · 评分证据与版本", { exact: true })).toBeVisible();
  await expect(page.locator(".rating-version-card > header strong")).toContainText("当前周期 · 数据不足 · 原始人工记录 88 分 · v1");
  await expect(page.getByText("1 个来源", { exact: true })).toBeVisible();
});

test("能力画像建档向导由 AI 基于跨场次证据评分", async ({ page }) => {
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  await expect(page.getByText("罗雨欣的能力画像尚未建立", { exact: true })).toBeVisible();
  await expect(page.getByText("已归档", { exact: true })).toBeVisible();
  await expect(page.getByText("可审核证据", { exact: true })).toBeVisible();
  await expect(page.getByRole("img", { name: /罗雨欣八维能力雷达图/ })).toHaveCount(0);

  await page.getByRole("button", { name: "建立能力画像" }).click();
  await expect(page.getByText("建立能力画像 · 第 1/4 步", { exact: true })).toBeVisible();
  await expect(page.locator(".skill-setup-session input[type='checkbox']")).toHaveCount(2);
  await page.getByRole("button", { name: "建立画像并由 AI 评估" }).click();
  await expect(page.getByText("建立能力画像 · 第 4/4 步", { exact: true })).toBeVisible();
  const previewModal = page.getByRole("dialog", { name: "建立能力画像 · 第 4/4 步" });
  await expect(previewModal.getByText(/AI 已基于 2 条分析证据形成 1 个维度评分/)).toBeVisible();
  await expect(page.getByText(/当前已有 1\/8 个维度达到每维 2 个场次来源门槛/)).toBeVisible();
  await expect(page.getByText("82 分", { exact: true })).toBeVisible();
});

test("不足两个录播来源时仍可完成画像建档预览", async ({ page }) => {
  const oneSourceFixture = STREAMER_ARCHIVE_FIXTURE.filter((archive) => archive.live_id === "live-luo-001");
  await page.unroute(LOCAL_API_ROUTE);
  await mockArchiveApi(page, oneSourceFixture);
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  await page.getByRole("button", { name: "建立能力画像" }).click();
  await page.getByRole("button", { name: "继续由 AI 评估" }).click();

  await expect(page.getByText("建立能力画像 · 第 4/4 步", { exact: true })).toBeVisible();
  await expect(page.getByText(/当前只有 1 个可读取的录播来源/)).toBeVisible();
  await expect(page.getByText(/当前已有 0\/8 个维度达到每维 2 个场次来源门槛/)).toBeVisible();
});

test("长录播列表仍显示下一步操作", async ({ page }) => {
  const template = STREAMER_ARCHIVE_FIXTURE[0];
  const longLuoFixture = Array.from({ length: 12 }, (_, index) => ({
    ...template,
    live_id: `live-luo-long-${index + 1}`,
    title: `罗雨欣 长列表验收场次 ${index + 1}`,
    created_at: `2026-08-${String(28 - index).padStart(2, "0")}T06:00:00.000Z`,
  }));
  await page.unroute(LOCAL_API_ROUTE);
  await mockArchiveApi(page, longLuoFixture);
  await page.setViewportSize({ width: 768, height: 900 });
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();
  await page.getByRole("button", { name: "建立能力画像" }).click();

  const setupModal = page.getByRole("dialog", { name: "建立能力画像 · 第 1/4 步" });
  await expect(setupModal.getByText("已选择 5 场录播；系统会先建立基础档案，再自动准备可用文稿。", { exact: true })).toBeVisible();
  await expect(setupModal.getByRole("button", { name: "建立画像并后台准备" })).toBeVisible();
  expect(await setupModal.locator(".skill-setup-session-list").evaluate((element) =>
    element.scrollHeight > element.clientHeight
  )).toBe(true);
});

test("没有分析证据时先建立基础档案并后台准备录播文稿", async ({ page }) => {
  const preparationRequests: string[] = [];
  await page.addInitScript(() => {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith("bsr:content-analysis:v3:")) localStorage.removeItem(key);
    }
  });
  page.on("request", (request) => {
    const command = new URL(request.url()).pathname.split("/").pop();
    if (command === "refresh_archive_subtitle") preparationRequests.push(request.url());
  });

  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();
  await page.getByRole("button", { name: "建立能力画像" }).click();

  const setupModal = page.getByRole("dialog", { name: "建立能力画像 · 第 1/4 步" });
  await expect(setupModal.getByRole("button", { name: "建立画像并后台准备" })).toBeVisible();
  await setupModal.getByRole("button", { name: "建立画像并后台准备" }).click();

  await expect(page.getByText("罗雨欣的能力画像尚未建立", { exact: true })).toBeVisible();
  await expect(page.getByText(/基础档案已建立，(?:正在后台准备|\d+ 场录播文稿已准备完成)/)).toBeVisible();
  await expect(page.getByText("后台准备", { exact: true })).toBeVisible();
  await expect(page.getByText("1/1 场", { exact: true })).toBeVisible();
  expect(preparationRequests).toHaveLength(1);
});

test("双周期八维雷达、分数表、证据入口与页签键盘语义一致", async ({ page }) => {
  await installRatingHistory(page, FULL_SKILL_RATING_HISTORY_FIXTURE);
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  await expect(page.getByRole("img", { name: /罗雨欣八维能力雷达图/ })).toBeVisible();
  await expect(page.locator("svg .current-polygon")).toHaveCount(1);
  await expect(page.locator("svg .previous-polygon")).toHaveCount(1);
  await expect(page.locator(".score-table-wrap tbody > tr")).toHaveCount(8);
  await expect(page.locator(".axis-label")).toHaveCount(8);
  await expect(page.getByText("当前周期 · 8/8 维", { exact: true })).toBeVisible();
  await expect(page.getByText("上一周期 · 8/8 维", { exact: true })).toBeVisible();

  const scoreRow = page.getByRole("row", { name: /需求确认/ });
  await expect(scoreRow.getByText("88 分", { exact: true })).toBeVisible();
  await expect(scoreRow.getByText("76 分", { exact: true })).toBeVisible();
  await expect(scoreRow.getByText("本期 2", { exact: true })).toBeVisible();
  await expect(scoreRow.getByText("上期 2", { exact: true })).toBeVisible();
  await expect(scoreRow.getByRole("button", { name: /查看需求确认的 4 条证据/ })).toBeVisible();

  const streamsTab = page.getByRole("tab", { name: "全部直播" });
  await streamsTab.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "能力趋势" })).toBeFocused();
  await expect(page.getByRole("tab", { name: "能力趋势" })).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Home");
  await expect(streamsTab).toBeFocused();
  await expect(streamsTab).toHaveAttribute("aria-selected", "true");
});

test("七个有效维度已生成雷达时显示绿色 AI 分析成功状态", async ({ page }) => {
  const sevenDimensionHistory = FULL_SKILL_RATING_HISTORY_FIXTURE.filter((rating) =>
    rating.period !== "current" || rating.dimension !== "risk_compliance"
  );
  await installRatingHistory(page, sevenDimensionHistory);
  await openArchive(page);

  const luoCard = page.getByRole("button", { name: /打开罗雨欣的主播档案/ });
  const directoryStatus = luoCard.getByText("AI 分析已生成 · 7/8 维", { exact: true });
  await expect(directoryStatus).toBeVisible();
  await expect(directoryStatus).toHaveClass(/is-complete/);

  await luoCard.click();
  const profileStatus = page.locator(".skill-analysis-status");
  await expect(profileStatus).toHaveText("AI 分析已生成 · 7/8 维");
  await expect(profileStatus).toHaveClass(/is-complete/);
  await expect(profileStatus).toHaveCSS("color", "rgb(11, 107, 56)");
  await expect(page.getByRole("img", { name: /罗雨欣八维能力雷达图/ })).toBeVisible();
});

test("能力画像引用过的录播在列表中同步显示绿色已分析", async ({ page }) => {
  await installRatingHistory(page, FULL_SKILL_RATING_HISTORY_FIXTURE);
  await openArchive(page);

  const luoCard = page.getByRole("button", { name: /打开罗雨欣的主播档案/ });
  await expect(luoCard).toContainText("2/2");
  await luoCard.click();

  const analyzedRows = visibleArchiveRows(page).locator(".archive-analysis-status.is-complete");
  await expect(analyzedRows).toHaveCount(2);
  await expect(analyzedRows).toHaveText(["已分析", "已分析"]);
  await expect(analyzedRows.first()).toHaveCSS("color", "rgb(11, 107, 56)");
});

test("证据弹窗约束键盘焦点，Escape 关闭后恢复证据入口焦点", async ({ page }) => {
  await installRatingHistory(page, FULL_SKILL_RATING_HISTORY_FIXTURE);
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  const evidenceTrigger = page
    .getByRole("row", { name: /需求确认/ })
    .getByRole("button", { name: /查看需求确认的 4 条证据/ });
  await evidenceTrigger.click();

  const dialog = page.getByRole("dialog", { name: "需求确认 · 评分证据与版本" });
  const closeButton = dialog.getByRole("button", { name: "关闭" });
  const lastDialogButton = dialog.getByRole("button").last();
  await expect(closeButton).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(lastDialogButton).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(closeButton).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(evidenceTrigger).toBeFocused();
});

test("证据录播移至竞品后评分失效，最后场次移出后安全回到目录", async ({ page }) => {
  await installRatingHistory(page, FULL_SKILL_RATING_HISTORY_FIXTURE);
  await openArchive(page);
  await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();

  await archiveRowWithTitle(page, "罗雨欣 S9 相机专场")
    .getByRole("button", { name: "移至竞品" })
    .click();
  await expect(visibleArchiveRows(page)).toHaveCount(1);
  await expect(page.locator("svg .current-polygon")).toHaveCount(0);
  await expect(page.locator("svg .previous-polygon")).toHaveCount(0);
  await expect(page.getByText(/0\/8 维达到每维 2 个来源门槛/)).toBeVisible();

  const invalidatedScoreRow = page.getByRole("row", { name: /需求确认/ });
  await expect(invalidatedScoreRow.getByText("数据不足", { exact: true }).first()).toBeVisible();
  await invalidatedScoreRow
    .getByRole("button", { name: /查看需求确认的 0 条证据/ })
    .click();
  await expect(page.getByText(/已失效 · 原始人工记录 88 分/)).toBeVisible();
  await expect(page.getByText("已不计入当前评分：所绑定证据已不属于此主播的公司录播档案。", { exact: true })).toHaveCount(2);
  await page.keyboard.press("Escape");

  await archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")
    .getByRole("button", { name: "移至竞品" })
    .click();
  await expect(page.getByText("当前主播分组已变化，已安全返回主播列表。", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开罗雨欣的主播档案" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "打开张敏的主播档案" })).toBeFocused();

  await page.getByRole("tab", { name: "竞品录播" }).click();
  await expect(visibleArchiveRows(page)).toHaveCount(3);
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toBeVisible();
});

test("待确认不会误归档，键盘 Enter 下钻并在返回后恢复焦点", async ({ page }) => {
  await openArchive(page);

  const competitorTab = page.getByRole("tab", { name: "竞品录播" });
  const luoCard = page.getByRole("button", { name: /打开罗雨欣的主播档案/ });
  const zhangCard = page.getByRole("button", { name: /打开张敏的主播档案/ });
  const pendingCard = page.getByRole("button", { name: /打开待确认主播列表/ });
  await competitorTab.focus();
  await page.keyboard.press("Tab");
  await expect(luoCard).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(zhangCard).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(pendingCard).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "罗雨欣", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "返回主播列表" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(luoCard).toBeFocused();

  await pendingCard.press("Space");
  await expect(page.getByRole("heading", { name: "待确认录播" })).toBeVisible();
  await expect(page.getByRole("button", { name: "返回主播列表" })).toBeFocused();
  await expect(visibleArchiveRows(page)).toHaveCount(3);
  await expect(archiveRowWithTitle(page, "S9 晚间专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "G9 双机位专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "S5M2 午间专场")).toBeVisible();
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toHaveCount(0);
  await expect(page.getByText(/待确认 · 身份冲突 · 点击确认/)).toBeVisible();
});

test("竞品录播保持独立列表且不进入公司主播档案", async ({ page }) => {
  await openArchive(page);
  await page.getByRole("tab", { name: "竞品录播" }).click();
  await expect(visibleArchiveRows(page)).toHaveCount(1);
  await expect(archiveRowWithTitle(page, "竞品 S9 对比专场")).toBeVisible();
  await expect(page.getByRole("heading", { name: "主播档案", exact: true })).toHaveCount(0);
  await expect(archiveRowWithTitle(page, "罗雨欣 S9 相机专场")).toHaveCount(0);
});

test("损坏分析缓存局部降级但目录与档案仍可用", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("bsr:content-analysis:v3:archive:douyin:broken:json", "{broken");
    localStorage.setItem("bsr:content-analysis:v3:archive:douyin:broken:shape", JSON.stringify({ candidates: null }));
  });
  await openArchive(page);
  await page.getByRole("button", { name: /打开罗雨欣的主播档案/ }).click();
  await expect(page.getByRole("heading", { name: "八维能力画像" })).toBeVisible();
  await expect(page.getByText("数据不足", { exact: true }).first()).toBeVisible();
});

test("空公司数据显示明确空态", async ({ page }) => {
  await page.unroute(LOCAL_API_ROUTE);
  await mockArchiveApi(page, EMPTY_COMPANY_ARCHIVE_FIXTURE);
  await openArchive(page);
  await expect(page.getByText("暂无主播档案", { exact: true })).toBeVisible();
  await expect(page.getByText("主播身份确认后，档案会显示在这里。", { exact: true })).toBeVisible();
});

for (const viewport of [
  { width: 320, height: 900 },
  { width: 768, height: 900 },
  { width: 1440, height: 900 },
]) {
  test(`目录与档案在 ${viewport.width}px 宽度无页面级横向溢出`, async ({ page }, testInfo) => {
    await installRatingHistory(page, FULL_SKILL_RATING_HISTORY_FIXTURE);
    await page.setViewportSize(viewport);
    await openArchive(page);
    await expect(page.getByRole("button", { name: "打开罗雨欣的主播档案" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
    await expectVisiblePageHasNoHorizontalOverflow(page);
    await page.screenshot({
      path: testInfo.outputPath(`streamer-directory-${viewport.width}.png`),
      fullPage: true,
    });

    await page.getByRole("button", { name: "打开罗雨欣的主播档案" }).click();
    await expect(page.getByRole("heading", { name: "八维能力画像" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
    await expectVisiblePageHasNoHorizontalOverflow(page);
    await expect(page.locator(".score-table-wrap")).toHaveCSS("overflow-x", "auto");
    await expect(page.locator(".mac-table-wrap")).toHaveCSS("overflow-x", "auto");
    await expect(page.getByRole("button", { name: "返回主播列表" })).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath(`streamer-profile-${viewport.width}.png`),
      fullPage: true,
    });
    if (viewport.width === 320 || viewport.width === 1440) {
      await page.locator(".skill-card").screenshot({
        path: testInfo.outputPath(`streamer-skill-card-${viewport.width}.png`),
      });
      await page.getByLabel("商品筛选").fill("S9");
      await expect(visibleArchiveRows(page)).toHaveCount(1);
      await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toHaveCount(0);
      await page.getByRole("button", { name: "清空筛选" }).click();
      await expect(visibleArchiveRows(page)).toHaveCount(2);
      await expect(archiveRowWithTitle(page, "罗雨欣 G9 新品答疑")).toBeVisible();
      await page.getByRole("button", { name: "返回主播列表" }).click();
      await expect(page.getByRole("button", { name: /打开罗雨欣的主播档案/ })).toBeVisible();
      await expectVisiblePageHasNoHorizontalOverflow(page);
    }
  });
}
