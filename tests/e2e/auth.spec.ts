import { test, expect } from './fixtures';

test('unauthenticated / stays at / (no redirect)', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL('/');
});

test('unauthenticated /admin/dashboard redirects to /login', async ({ page }) => {
  await page.goto('/admin/dashboard');
  await expect(page).toHaveURL('/login');
});

test('unauthenticated /client redirects to /login', async ({ page }) => {
  await page.goto('/client');
  await expect(page).toHaveURL('/login');
});

test('authenticated admin at / is redirected to /admin/dashboard', async ({ adminPage }) => {
  await adminPage.goto('/');
  await expect(adminPage).toHaveURL('/admin/dashboard');
});

test('authenticated admin at /admin/dashboard sees dashboard', async ({ adminPage }) => {
  await adminPage.goto('/admin/dashboard');
  await expect(adminPage).toHaveURL('/admin/dashboard');
  await expect(adminPage.getByRole('heading', { name: 'Dashboard' })).toBeVisible();
});

test('client user visiting /admin/dashboard is redirected to /client', async ({ clientPage }) => {
  await clientPage.goto('/admin/dashboard');
  await expect(clientPage).toHaveURL('/client');
});

test('admin user visiting /client is redirected to /admin/dashboard', async ({ adminPage }) => {
  await adminPage.goto('/client');
  await expect(adminPage).toHaveURL('/admin/dashboard');
});
