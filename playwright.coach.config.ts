import { defineConfig } from "@playwright/test";

const port = Number(process.env.COACH_E2E_PORT || 8062);
const baseURL = `http://127.0.0.1:${port}`;
const useProductionPreview = process.env.COACH_E2E_PREVIEW === "1";

export default defineConfig({
  testDir: "./tests/e2e",
  testMatch: "streamer-coach.spec.ts",
  fullyParallel: false,
  workers: 1,
  // The desktop shell eagerly mounts every legacy page; allow one cold Svelte
  // compile on constrained Windows hosts without weakening assertion timeouts.
  timeout: 180_000,
  expect: { timeout: 7_000 },
  reporter: [["list"]],
  use: {
    baseURL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: useProductionPreview
      ? `npm run preview -- --host 127.0.0.1 --port ${port} --strictPort`
      : `npm run dev -- --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
