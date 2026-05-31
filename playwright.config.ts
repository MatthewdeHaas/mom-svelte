import { defineConfig, devices } from '@playwright/test';

/**
 * BASE_URL can be overridden by CI (e.g. if running against a built preview
 * server on a different port). Defaults to the SvelteKit dev server.
 */
const BASE_URL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';

export default defineConfig({
  testDir: 'tests/e2e',
  fullyParallel: false, // tests share a seeded DB; keep serial to avoid conflicts
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? 'github' : 'list',

  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  globalSetup: './tests/e2e/global-setup.ts',

  webServer: {
    command: 'pnpm run dev -- --port 5174',
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    env: {
      // Forward the same env vars the integration tests use
      POSTGRES_USER: process.env.POSTGRES_USER ?? 'postgres',
      POSTGRES_PASSWORD: process.env.POSTGRES_PASSWORD ?? 'postgres',
      POSTGRES_HOST: process.env.POSTGRES_HOST ?? 'localhost',
      POSTGRES_DB: process.env.POSTGRES_DB ?? 'meals_test',
      BETTER_AUTH_SECRET: process.env.BETTER_AUTH_SECRET ?? 'e2e-test-secret',
      BETTER_AUTH_URL: BASE_URL,
      ADMIN_EMAIL: process.env.ADMIN_EMAIL ?? 'admin@test.com',
      RESEND_API_KEY: process.env.RESEND_API_KEY ?? 're_placeholder',
      EMAIL_FROM: process.env.EMAIL_FROM ?? 'Test <test@test.com>',
    },
  },
});
