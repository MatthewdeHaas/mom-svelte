import { test, expect } from '../fixtures';

test('client with company lands on /client dashboard', async ({ clientPage }) => {
  await clientPage.goto('/client');
  await expect(clientPage).toHaveURL('/client');
  await expect(clientPage.getByRole('heading', { name: 'Dashboard' })).toBeVisible();
});

test('client without company is redirected to /client/pending', async ({ pendingPage }) => {
  await pendingPage.goto('/client');
  await expect(pendingPage).toHaveURL('/client/pending');
  await expect(pendingPage.getByRole('heading', { name: 'Access pending' })).toBeVisible();
});

test('client /client/pending is accessible without redirect loop', async ({ pendingPage }) => {
  // Verify direct navigation to /client/pending stays on that page
  await pendingPage.goto('/client/pending');
  await expect(pendingPage).toHaveURL('/client/pending');
});

test('client orders page loads', async ({ clientPage }) => {
  await clientPage.goto('/client/orders');
  await expect(clientPage).toHaveURL('/client/orders');
});
