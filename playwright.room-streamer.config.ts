import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: "room-streamer.spec.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 8_000 },
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:8055",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 8055 --strictPort",
    url: "http://127.0.0.1:8055",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
