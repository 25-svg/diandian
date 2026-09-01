import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: "compass-curve.spec.ts",
  fullyParallel: false,
  workers: 1,
  // Cold Vite/Svelte transformation of the production-sized shared app can exceed one minute on Windows.
  timeout: 120_000,
  expect: { timeout: 7_500 },
  outputDir: "./test-results/compass-curve",
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:8071",
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 8071 --strictPort",
    url: "http://127.0.0.1:8071/tests/fixtures/compass-curve-harness.html",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
