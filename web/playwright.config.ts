import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  testMatch: '**/*.spec.ts',
  reporter: 'list',
  use: {
    baseURL: process.env.FD_BASE_URL ?? 'http://localhost:8021',
    trace: 'retain-on-failure',
  },
});
