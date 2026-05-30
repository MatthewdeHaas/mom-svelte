import { test, expect } from '../fixtures';

test('orders list page loads', async ({ adminPage }) => {
  await adminPage.goto('/admin/orders');
  await expect(adminPage).toHaveURL('/admin/orders');
  await expect(adminPage.getByRole('heading', { name: 'Orders' })).toBeVisible();
});
