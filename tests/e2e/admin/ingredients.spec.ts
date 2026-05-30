import { test, expect } from '../fixtures';

// Unique name per run so parallel local runs don't collide
const NAME = `E2E-Ingredient-${Date.now()}`;
const EDITED_NAME = `${NAME}-edited`;

// These tests form a sequence: create → view → edit → deactivate.
// serial mode guarantees ordering and shared in-process state (NAME/EDITED_NAME).
test.describe.serial('Ingredients CRUD', () => {
  test('list page loads', async ({ adminPage }) => {
    await adminPage.goto('/admin/ingredients');
    await expect(adminPage.getByRole('heading', { name: 'Ingredients' })).toBeVisible();
    await expect(adminPage.getByPlaceholder('Search ingredients…')).toBeVisible();
    await expect(adminPage.getByRole('link', { name: 'New ingredient' })).toBeVisible();
  });

  test('create a new ingredient', async ({ adminPage }) => {
    await adminPage.goto('/admin/ingredients/new');
    await adminPage.getByLabel('Name').fill(NAME);
    await adminPage.getByRole('button', { name: 'Create ingredient' }).click();

    // Server redirects to the detail page after creation
    await expect(adminPage).toHaveURL(/\/admin\/ingredients\/.+/);
    await expect(adminPage.getByRole('heading', { name: NAME })).toBeVisible();
  });

  test('ingredient appears in the list', async ({ adminPage }) => {
    await adminPage.goto('/admin/ingredients');
    await expect(adminPage.locator('td', { hasText: NAME })).toBeVisible();
  });

  test('edit the ingredient name', async ({ adminPage }) => {
    await adminPage.goto('/admin/ingredients');
    // Wait for the table to be fully rendered (past hydration)
    await adminPage.waitForLoadState('networkidle');
    // Click the first cell (name column) of the row for this ingredient
    const nameCell = adminPage.locator('tbody td').filter({ hasText: NAME }).first();
    await nameCell.waitFor({ state: 'visible' });
    await nameCell.click();
    await expect(adminPage).toHaveURL(/\/admin\/ingredients\/.+/, { timeout: 10_000 });

    await adminPage.getByLabel('Name').fill(EDITED_NAME);
    await adminPage.getByRole('button', { name: 'Save changes' }).click();

    await expect(adminPage.getByText('Changes saved')).toBeVisible();
    await expect(adminPage.getByLabel('Name')).toHaveValue(EDITED_NAME);
  });

  test('deactivate the ingredient', async ({ adminPage }) => {
    await adminPage.goto('/admin/ingredients');
    await adminPage.waitForLoadState('networkidle');
    const nameCell = adminPage.locator('tbody td').filter({ hasText: EDITED_NAME }).first();
    await nameCell.waitFor({ state: 'visible' });
    await nameCell.click();
    await expect(adminPage).toHaveURL(/\/admin\/ingredients\/.+/, { timeout: 10_000 });

    await adminPage.getByRole('button', { name: 'Deactivate ingredient' }).click();

    await expect(adminPage.getByText('Ingredient deactivated')).toBeVisible();
    await expect(adminPage.locator('.inactive-chip')).toBeVisible();
  });
});
