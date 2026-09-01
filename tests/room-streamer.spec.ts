import { expect, test, type Page } from "@playwright/test";

const pageErrors = new WeakMap<Page, string[]>();
const LOCAL_API_ROUTE = /^http:\/\/127\.0\.0\.1:\d+\/api\/.*$/;

const room = {
  room_info: {
    platform: "douyin",
    room_id: "room-luo",
    room_title: "金典拍拍专做二手精品相机镜头，直播间领取最高400优惠！",
    room_cover: "",
    status: true,
  },
  user_info: {
    user_id: "shop-account",
    user_name: "金典拍拍相机专卖店",
    user_avatar: "",
  },
  platform_live_id: "douyin-live-001",
  live_id: "live-luo-001",
  recording: true,
  enabled: true,
  current_streamer: "罗雨欣",
  current_streamer_source: "manual",
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

async function mockApi(page: Page): Promise<void> {
  let currentStreamer = room.current_streamer;
  let currentSource = room.current_streamer_source;

  await page.route("https://api.github.com/**", async (route) => {
    await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
  });
  await page.route(LOCAL_API_ROUTE, async (route) => {
    const command = new URL(route.request().url()).pathname.split("/").pop() || "";
    const body = route.request().postDataJSON?.() || {};
    let data: unknown = null;

    if (command === "get_recorder_list") {
      data = {
        count: 1,
        recorders: [{ ...room, current_streamer: currentStreamer, current_streamer_source: currentSource }],
      };
    } else if (command === "get_known_streamer_names") {
      data = ["罗雨欣", "侯梦娜", "张敏"];
    } else if (command === "set_room_streamer") {
      currentStreamer = String(body.streamerName || "");
      currentSource = currentStreamer ? "manual" : "";
    } else if (command === "get_config") {
      data = {
        cache: "", output: "", primary_uid: 0, live_start_notify: true,
        live_end_notify: true, clip_notify: true, post_notify: true,
        auto_cleanup: true, auto_subtitle: false, subtitle_generator_type: "funasr",
        openai_api_endpoint: "", openai_api_key: "", volcengine_api_key: "",
        volcengine_app_id: "", volcengine_access_token: "",
        volcengine_resource_id: "volc.seedasr.auc", volcengine_boosting_table_id: "",
        volcengine_correct_table_id: "", admin_mode: false, powerlive_key: "",
        knowledge_vault_path: "", whisper_model: "", whisper_prompt: "",
        clip_name_format: "", auto_generate: { enabled: false, encode_danmu: false },
        nas_video_storage: { enabled: false, root_path: "", archive_recordings: true,
          archive_imports: true, delete_local_after_archive: true },
        autostart_enabled: true, startup_wizard_completed: true,
        status_check_interval: 30, whisper_language: "", webhook_url: "",
        danmu_ass_options: { font_size: 36, opacity: 0.8 },
      };
    } else if ([
      "get_accounts", "get_tasks", "get_recorder_health", "get_recent_record",
      "list_video_archives", "list_master_sample_batches", "get_all_videos",
      "get_messages", "list_live_dashboard_sessions", "list_scenario_training_sessions",
    ].includes(command)) {
      data = command === "get_accounts" ? { accounts: [] } : [];
    } else if ([
      "get_archive_disk_usage", "get_account_count", "get_total_length", "get_today_record_count",
    ].includes(command)) {
      data = 0;
    } else if (command === "get_disk_info") {
      data = { disk: "", total: 0, free: 0 };
    } else if (command === "get_storage_migration_status") {
      data = { cache: false, output: false };
    } else if (command === "get_minimax_setup_status") {
      data = { configured: true };
    } else if (command === "get_startup_readiness") {
      data = {
        wizardCompleted: true, autostartEnabled: true, accountCount: 0, recorderCount: 1,
        ffmpegOk: true, ffmpegDetail: "fixture", funasrOk: true, funasrDetail: "fixture",
        cacheOk: true, cacheDetail: "fixture", outputOk: true, outputDetail: "fixture",
        nasConfigured: false, doudianConfigured: false, readyForRecording: true,
      };
    }

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ code: 0, message: "ok", data }),
    });
  });
}

async function openRoomPage(page: Page): Promise<void> {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "直播间", exact: true }).click();
  const visiblePage = page.locator(".page.visible");
  await expect(visiblePage.getByRole("heading", { name: "直播间", exact: true })).toBeVisible();
  await expect.poll(async () => visiblePage.evaluate((element) => {
    const style = getComputedStyle(element);
    const parent = element.parentElement?.getBoundingClientRect();
    const bounds = element.getBoundingClientRect();
    return {
      opaque: Number.parseFloat(style.opacity) >= 0.999,
      aligned: Boolean(parent) && Math.abs(bounds.left - parent.left) <= 1,
    };
  })).toEqual({ opaque: true, aligned: true });
}

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  pageErrors.set(page, errors);
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console.error: ${message.text()}`);
  });
  await installBrowserTauriEventMock(page);
  await mockApi(page);
});

test.afterEach(async ({ page }) => {
  expect(pageErrors.get(page) || []).toEqual([]);
});

test("直播间卡片显示当前主播，修改后重新进入仍保留", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openRoomPage(page);

  const assignment = page.getByTestId("room-streamer-douyin-room-luo");
  await expect(assignment).toContainText("当前主播");
  await expect(assignment).toContainText("罗雨欣");
  await page.getByRole("button", { name: /管理直播间/ }).click();
  await page.getByText("修改当前主播", { exact: true }).click();
  await expect(page.getByRole("dialog", { name: "设置当前主播" })).toBeVisible();
  await expect(page.getByTestId("known-streamer-options")).toContainText("侯梦娜");

  await page.getByTestId("room-streamer-input").fill("侯梦娜");
  await page.getByTestId("save-room-streamer").click();
  await expect(assignment).toContainText("侯梦娜");

  await page.getByRole("button", { name: "总览", exact: true }).click();
  await page.getByRole("button", { name: "直播间", exact: true }).click();
  await expect(page.getByTestId("room-streamer-douyin-room-luo")).toContainText("侯梦娜");
});

test("非法姓名不会保存，恢复自动识别会清除人工指定", async ({ page }) => {
  await openRoomPage(page);
  await page.getByTestId("room-streamer-douyin-room-luo").click();
  await page.getByTestId("room-streamer-input").fill("罗雨欣，侯梦娜");
  await page.getByTestId("save-room-streamer").click();
  await expect(page.getByText(/不能包含标点或换行/)).toBeVisible();

  await page.getByRole("button", { name: "恢复自动识别" }).click();
  await expect(page.getByTestId("room-streamer-douyin-room-luo")).toContainText("待指定");
});

test("标题、当前主播和直播账号保持整齐且长文字不会挤乱操作区", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 720 });
  await openRoomPage(page);

  const title = page.getByTestId("room-title-douyin-room-luo");
  const streamer = page.getByTestId("room-streamer-douyin-room-luo");
  const account = page.getByTestId("room-account-douyin-room-luo");
  await expect(title).toBeVisible();
  await expect(streamer).toBeVisible();
  await expect(account).toHaveAttribute("title", room.user_info.user_name);

  const layout = await page.getByTestId("room-card-metadata-douyin-room-luo").evaluate((metadata) => {
    const titleRow = metadata.querySelector<HTMLElement>('[data-testid^="room-title-"]');
    const streamerRow = metadata.querySelector<HTMLElement>('[data-testid^="room-streamer-"]');
    const accountName = metadata.querySelector<HTMLElement>('[data-testid^="room-account-"]');
    if (!titleRow || !streamerRow || !accountName) throw new Error("卡片信息区不完整");
    const metadataLeft = metadata.getBoundingClientRect().left;
    const accountStyle = getComputedStyle(accountName);
    return {
      titleAligned: Math.abs(titleRow.getBoundingClientRect().left - metadataLeft) <= 1,
      streamerAligned: Math.abs(streamerRow.getBoundingClientRect().left - metadataLeft) <= 1,
      accountSingleLine: accountName.clientHeight <= Number.parseFloat(accountStyle.lineHeight) + 1,
      accountContained: accountName.getBoundingClientRect().right <= metadata.getBoundingClientRect().right + 1,
    };
  });

  expect(layout).toEqual({
    titleAligned: true,
    streamerAligned: true,
    accountSingleLine: true,
    accountContained: true,
  });
});

for (const viewport of [
  { width: 320, height: 720 },
  { width: 768, height: 480 },
  { width: 1440, height: 900 },
]) {
  test(`关键分辨率 ${viewport.width}px 无横向溢出`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openRoomPage(page);
    const scroller = page.locator(".page.visible .mac-page");
    await expect(scroller).toBeVisible();
    expect(await scroller.evaluate((element) => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath(`room-${viewport.width}.png`), fullPage: true });
  });
}

test("深色模式下当前主播和弹窗保持可读", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openRoomPage(page);
  await page.evaluate(() => document.documentElement.classList.add("dark"));
  const assignment = page.getByTestId("room-streamer-douyin-room-luo");
  await expect(assignment).toContainText("罗雨欣");
  await assignment.click();
  const dialog = page.getByRole("dialog", { name: "设置当前主播" });
  await expect(dialog).toBeVisible();
  await expect.poll(async () => dialog.evaluate((element) =>
    Number.parseFloat(getComputedStyle(element).opacity)
  )).toBeGreaterThanOrEqual(0.999);
  const input = page.getByTestId("room-streamer-input");
  await expect(input).toBeVisible();
  await expect(input).toBeFocused();
  await page.screenshot({ path: testInfo.outputPath("room-dark-modal.png"), fullPage: true });
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});
