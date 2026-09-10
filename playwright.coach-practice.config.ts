import { defineConfig } from '@playwright/test';
export default defineConfig({testDir:'./tests',testMatch:'coach-practice.spec.ts',workers:1,timeout:60000,outputDir:'test-results-coach-practice',
  use:{baseURL:'http://127.0.0.1:8076',trace:'retain-on-failure',screenshot:'only-on-failure'},
  webServer:{command:'node node_modules/vite/bin/vite.js --config vite.coach-practice.config.ts --host 127.0.0.1 --port 8076 --strictPort',url:'http://127.0.0.1:8076/tests/fixtures/coach-practice-harness.html',timeout:120000}});
