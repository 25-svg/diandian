import { defineConfig } from "@playwright/test";

const trainingPort = Number(process.env.TRAINING_E2E_PORT || 8054);
const trainingBaseUrl = `http://127.0.0.1:${trainingPort}`;
const useProductionPreview = process.env.TRAINING_E2E_PREVIEW === "1";

export default defineConfig({
  testDir: "./tests/e2e",
  testMatch: "training-flow.spec.ts",
  fullyParallel: false,
  workers: 1,
  // The full desktop shell mounts every page; allow a cold Vite/Svelte compile
  // to finish on constrained Windows hosts while keeping assertions at 5s.
  timeout: 90_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: {
    baseURL: trainingBaseUrl,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: useProductionPreview
      ? `npm run preview -- --host 127.0.0.1 --port ${trainingPort} --strictPort`
      : `npm run dev -- --host 127.0.0.1 --port ${trainingPort} --strictPort`,
    url: trainingBaseUrl,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
