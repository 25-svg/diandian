import path from "node:path";
import { defineConfig } from "@playwright/test";

const artifactRoot = process.env.QA09_ARTIFACT_ROOT
  ? path.resolve(process.env.QA09_ARTIFACT_ROOT)
  : path.resolve("test-results/qa09");

export default defineConfig({
  testDir: "./tests",
  testMatch: "qa09-integration-visual.spec.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  outputDir: path.join(artifactRoot, "Playwright"),
  reporter: [
    ["list"],
    ["json", { outputFile: path.join(artifactRoot, "日志", "playwright-qa09.json") }],
  ],
  use: {
    baseURL: "http://127.0.0.1:8093",
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 8093 --strictPort",
    url: "http://127.0.0.1:8093",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
