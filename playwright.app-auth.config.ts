import { defineConfig } from "@playwright/test";
const port = Number(process.env.APP_AUTH_E2E_PORT || 8093);
export default defineConfig({
  testDir: "./tests", testMatch: "app-auth.spec.ts", fullyParallel: false, workers: 1, timeout: 60_000,
  expect: { timeout: 8_000 }, reporter: [["list"]],
  outputDir: "C:/Users/10230/Documents/Codex/Workspace/01-Projects/Project-003-直播切片分析系统/05-测试与验收/2026-09-15-双角色登录",
  use: { baseURL: `http://127.0.0.1:${port}`, viewport: { width: 1440, height: 900 }, trace: "retain-on-failure", screenshot: "only-on-failure" },
  webServer: { command: `vite --config vite.app-auth.config.ts --host 127.0.0.1 --port ${port} --strictPort`, url: `http://127.0.0.1:${port}/tests/fixtures/app-auth-gate-harness.html`, reuseExistingServer: false, timeout: 120_000 },
});
