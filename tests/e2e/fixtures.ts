import { test as base, type Page } from '@playwright/test';
import { readFileSync } from 'fs';
import { join } from 'path';

interface AuthState {
  adminToken: string;
  clientToken: string;
  pendingToken: string;
  companyId: string;
}

function loadState(): AuthState {
  return JSON.parse(
    readFileSync(join(process.cwd(), 'tests/e2e/.auth/state.json'), 'utf-8')
  );
}

/**
 * Inject a Better Auth session token as a cookie so the app treats the browser
 * as the given user — no actual login flow required.
 */
async function injectSession(page: Page, token: string) {
  await page.context().addCookies([
    {
      name: 'better-auth.session_token',
      value: token,
      domain: 'localhost',
      path: '/',
      httpOnly: true,
      secure: false,
      sameSite: 'Lax',
    },
  ]);
}

type Fixtures = {
  adminPage: Page;
  clientPage: Page;
  pendingPage: Page;
  authState: AuthState;
};

export const test = base.extend<Fixtures>({
  authState: async ({}, use) => {
    await use(loadState());
  },

  adminPage: async ({ page }, use) => {
    await injectSession(page, loadState().adminToken);
    await use(page);
  },

  clientPage: async ({ page }, use) => {
    await injectSession(page, loadState().clientToken);
    await use(page);
  },

  pendingPage: async ({ page }, use) => {
    await injectSession(page, loadState().pendingToken);
    await use(page);
  },
});

export { expect } from '@playwright/test';
