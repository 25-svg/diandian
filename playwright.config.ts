import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: "streamer-profile.spec.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 7_500 },
  outputDir: "./test-results/streamer-profile",
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:8067",
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 8067 --strictPort",
    url: "http://127.0.0.1:8067",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
