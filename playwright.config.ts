import { defineConfig, devices } from '@playwright/test';

/**
 * BASE_URL can be overridden by CI (e.g. if running against a built preview
 * server on a different port). Defaults to the SvelteKit dev server.
 */
const BASE_URL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';

// Postgres connection — shared between the SvelteKit dev server (via the
// POSTGRES_* vars it already reads) and the Rust API (which wants a single
// DATABASE_URL). Keep both forms in sync here so there's one source of truth.
const POSTGRES_USER = process.env.POSTGRES_USER ?? 'postgres';
const POSTGRES_PASSWORD = process.env.POSTGRES_PASSWORD ?? 'postgres';
const POSTGRES_HOST = process.env.POSTGRES_HOST ?? 'localhost';
const POSTGRES_DB = process.env.POSTGRES_DB ?? 'meals_test';
const DATABASE_URL =
  process.env.DATABASE_URL ??
  `postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:5432/${POSTGRES_DB}`;

// Shared HMAC secret for the SvelteKit → Rust internal auth channel. Rust
// enforces >=32 bytes (see `admin-operations-rust/crates/api/src/main.rs`);
// the literal below is 46 bytes. Overridable so CI can inject a per-run
// secret if ever needed.
const INTERNAL_AUTH_SECRET =
  process.env.INTERNAL_AUTH_SECRET ?? 'e2e-internal-auth-secret-at-least-32-bytes-long';

// Rust API lives on 8081 (dev-prod uses 8080; keeping e2e distinct avoids
// clashes with a locally-running `cargo run` session).
const RUST_API_URL = 'http://localhost:8081';

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

  // Two services, started in parallel. Both must become ready before
  // specs run. The SvelteKit `+page.server.ts` layer calls `internalFetch`
  // against RUST_API_URL for every admin/client-admin route, so without
  // the Rust API every SSR load 500s.
  //
  // globalSetup pre-builds the Rust binary (first run ~2 min, cached
  // after) so the webServer start isn't a cold compile.
  webServer: [
    {
      command: './admin-operations-rust/target/release/api',
      url: `${RUST_API_URL}/healthz`,
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
      env: {
        PORT: '8081',
        DATABASE_URL,
        INTERNAL_AUTH_SECRET,
        // Verbose logs so CI shows what the Rust import handler
        // actually says when the E2E suite is red. Dial back to
        // `warn` once the import flow is green again.
        RUST_LOG: 'debug',
      },
    },
    {
      command: 'pnpm run dev -- --port 5174',
      url: BASE_URL,
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
      env: {
        // Forward the same env vars the integration tests use
        POSTGRES_USER,
        POSTGRES_PASSWORD,
        POSTGRES_HOST,
        POSTGRES_DB,
        BETTER_AUTH_SECRET: process.env.BETTER_AUTH_SECRET ?? 'e2e-test-secret',
        BETTER_AUTH_URL: BASE_URL,
        ADMIN_EMAIL: process.env.ADMIN_EMAIL ?? 'admin@test.com',
        RESEND_API_KEY: process.env.RESEND_API_KEY ?? 're_placeholder',
        EMAIL_FROM: process.env.EMAIL_FROM ?? 'Test <test@test.com>',
        // Point SSR at the Rust service and share the HMAC secret.
        RUST_API_URL,
        INTERNAL_AUTH_SECRET,
      },
    },
  ],
});
