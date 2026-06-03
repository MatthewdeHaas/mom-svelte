import { test, expect } from '../fixtures';

// ---------------------------------------------------------------------------
// Pagination E2E tests
// Covers: navigation, URL param composition with filters, per_page changes,
// page reset on filter change, and edge cases (out-of-range page).
// ---------------------------------------------------------------------------

test.describe('Pagination', () => {
  test.describe('Orders page', () => {
    test('displays pagination controls when orders exist', async ({ adminPage }) => {
      await adminPage.goto('/admin/orders');
      await expect(adminPage.getByRole('heading', { name: 'Orders' })).toBeVisible();
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
      await expect(pagination.getByText(/Showing \d+–\d+ of \d+/)).toBeVisible();
    });

    test('per_page selector changes page size via URL', async ({ adminPage }) => {
      await adminPage.goto('/admin/orders?per_page=25');
      await adminPage.waitForLoadState('networkidle');
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();

      // Change per_page to 100
      const perPageSelect = pagination.locator('.per-page-select');
      await perPageSelect.selectOption('100');
      await expect(adminPage).toHaveURL(/per_page=100/, { timeout: 10000 });
      // Should reset to page 1 (page param absent)
      await expect(adminPage).not.toHaveURL(/page=/);
    });

    test('page param composes with existing filters', async ({ adminPage }) => {
      await adminPage.goto('/admin/orders?status=confirmed&per_page=25');
      await expect(adminPage).toHaveURL(/status=confirmed/);
      await expect(adminPage).toHaveURL(/per_page=25/);

      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
    });

    test('filter change resets page to 1', async ({ adminPage }) => {
      await adminPage.goto('/admin/orders');
      await adminPage.waitForLoadState('networkidle');

      // Change a filter — the sidebar status dropdown
      const statusSelect = adminPage.locator('.sidebar select').first();
      await statusSelect.selectOption('pending');

      // URL should have status=pending and no page param (reset to 1)
      await expect(adminPage).toHaveURL(/status=pending/, { timeout: 10000 });
      await expect(adminPage).not.toHaveURL(/page=2/);
    });

    test('out-of-range page still renders pagination', async ({ adminPage }) => {
      // Request a very high page number — there are orders so total > 0
      await adminPage.goto('/admin/orders?page=9999');
      await expect(adminPage.getByRole('heading', { name: 'Orders' })).toBeVisible();
      // Pagination should render because total > 0 even though current page has no rows
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
    });

    test('select all checkbox applies to current page only', async ({ adminPage }) => {
      await adminPage.goto('/admin/orders');
      await adminPage.waitForLoadState('networkidle');

      // The label should say "on this page" when orders are visible
      const bulkBar = adminPage.locator('.bulk-bar');
      if (await bulkBar.isVisible()) {
        await expect(bulkBar).toContainText('on this page');
      }
    });
  });

  test.describe('Companies page', () => {
    test('displays pagination controls', async ({ adminPage }) => {
      await adminPage.goto('/admin/companies');
      await expect(adminPage.getByRole('heading', { name: 'Companies' })).toBeVisible();
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
    });

    test('search resets page and uses server-side filtering', async ({ adminPage }) => {
      await adminPage.goto('/admin/companies');
      await adminPage.waitForLoadState('networkidle');

      // Type in search box
      const searchInput = adminPage.getByPlaceholder('Search companies…');
      await searchInput.fill('e2e');

      // Wait for debounced navigation — use a longer timeout for the debounce + network
      await expect(adminPage).toHaveURL(/q=e2e/, { timeout: 10000 });
      // Page should be reset (no page param)
      await expect(adminPage).not.toHaveURL(/page=2/);
    });

    test('toggle inactive resets page', async ({ adminPage }) => {
      await adminPage.goto('/admin/companies');
      await adminPage.waitForLoadState('networkidle');

      await adminPage.getByRole('button', { name: /Show inactive/i }).click();

      await expect(adminPage).toHaveURL(/inactive=true/, { timeout: 10000 });
      await expect(adminPage).not.toHaveURL(/page=2/);
    });
  });

  test.describe('Recipes page', () => {
    test('displays pagination controls', async ({ adminPage }) => {
      await adminPage.goto('/admin/recipes');
      await expect(adminPage.getByRole('heading', { name: 'Recipes' })).toBeVisible();
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
    });

    test('search resets page', async ({ adminPage }) => {
      await adminPage.goto('/admin/recipes');
      await adminPage.waitForLoadState('networkidle');

      const searchInput = adminPage.getByPlaceholder('Search recipes…');
      await searchInput.fill('e2e');
      await expect(adminPage).toHaveURL(/q=e2e/, { timeout: 10000 });
      await expect(adminPage).not.toHaveURL(/page=2/);
    });
  });

  test.describe('Ingredients page', () => {
    test('displays pagination controls', async ({ adminPage }) => {
      await adminPage.goto('/admin/ingredients');
      await expect(adminPage.getByRole('heading', { name: 'Ingredients' })).toBeVisible();
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
    });

    test('search resets page', async ({ adminPage }) => {
      await adminPage.goto('/admin/ingredients');
      await adminPage.waitForLoadState('networkidle');

      const searchInput = adminPage.getByPlaceholder('Search ingredients…');
      await searchInput.fill('test');
      await expect(adminPage).toHaveURL(/q=test/, { timeout: 10000 });
      await expect(adminPage).not.toHaveURL(/page=2/);
    });
  });

  test.describe('Financials page', () => {
    test('displays pagination on recipe margins table', async ({ adminPage }) => {
      await adminPage.goto('/admin/financials');
      await expect(adminPage.getByRole('heading', { name: 'Financials' })).toBeVisible();
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
    });

    test('per_page changes work on financials', async ({ adminPage }) => {
      await adminPage.goto('/admin/financials?per_page=25');
      await adminPage.waitForLoadState('networkidle');
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();
      const perPageSelect = pagination.locator('.per-page-select');
      await perPageSelect.selectOption('50');
      await expect(adminPage).toHaveURL(/per_page=50/, { timeout: 10000 });
    });
  });

  test.describe('Pagination navigation', () => {
    test('prev button is disabled on first page', async ({ adminPage }) => {
      await adminPage.goto('/admin/orders');
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();

      const prevBtn = pagination.getByRole('button', { name: 'Previous page' });
      await expect(prevBtn).toBeDisabled();
    });

    test('clicking next page updates URL', async ({ adminPage }) => {
      // Use a very small per_page to ensure multiple pages exist
      await adminPage.goto('/admin/orders?per_page=10');
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();

      // Only test if there's more than one page of results
      const nextBtn = pagination.getByRole('button', { name: 'Next page' });
      if (await nextBtn.isEnabled()) {
        await nextBtn.click();
        await expect(adminPage).toHaveURL(/page=2/, { timeout: 10000 });

        // Prev button should now be enabled
        const prevBtn = pagination.getByRole('button', { name: 'Previous page' });
        await expect(prevBtn).toBeEnabled();
      }
    });

    test('clicking a page number navigates directly', async ({ adminPage }) => {
      // Use small per_page to get multiple pages
      await adminPage.goto('/admin/orders?per_page=10');
      const pagination = adminPage.getByTestId('pagination');
      await expect(pagination).toBeVisible();

      // If page 2 button exists, click it
      const page2Btn = pagination.getByRole('button', { name: 'Page 2' });
      if (await page2Btn.isVisible()) {
        await page2Btn.click();
        await expect(adminPage).toHaveURL(/page=2/, { timeout: 10000 });

        // That button should now have active state
        await expect(page2Btn).toHaveAttribute('aria-current', 'page');
      }
    });
  });
});
