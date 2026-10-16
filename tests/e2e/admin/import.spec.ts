/**
 * Import pipeline E2E tests.
 *
 * Strategy: seed minimal companies and recipes in beforeAll, then
 * drive the import form via the real browser (setInputFiles + click submit).
 * Assert on visible UI elements and the DB state.
 *
 * Three test cases:
 *   1. Happy path — 2 valid rows → success banner shows "2" + 2 DB orders
 *   2. Error path — 1 valid row + 1 with a non-existent company → error table
 *      shown, zero new orders inserted (all-or-nothing semantics)
 *   3. Template download — GET /admin/orders/import/template returns a valid
 *      XLSX with the correct 8 column headers and ≥2 example rows
 *
 * All seed data uses a unique underscore-wrapped prefix so it can be cleaned
 * up without touching data from global-setup.ts.
 */

import ExcelJS from 'exceljs';
import { Pool } from 'pg';
import { randomUUID } from 'crypto';
import type { Page } from '@playwright/test';
import { test, expect } from '../fixtures';

// ─── Seed constants ───────────────────────────────────────────────────────────

const IMPORT_COMPANY_A = '__import_test_company_a__';
const IMPORT_COMPANY_B = '__import_test_company_b__';
const IMPORT_RECIPE_A  = '__import_test_recipe_a__';
const IMPORT_RECIPE_B  = '__import_test_recipe_b__';

// Far-future dates — well outside the global-setup export window (2099-01-15)
const IMPORT_SERVING_DATE  = '2099-06-10';
const IMPORT_SHIPMENT_DATE = '2099-06-09';

// ─── Pool helper ──────────────────────────────────────────────────────────────

function makePool() {
  return new Pool({
    connectionString: `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST ?? 'localhost'}:5432/${process.env.POSTGRES_DB}`,
  });
}

// ─── XLSX builder ─────────────────────────────────────────────────────────────

/**
 * Build an import XLSX in-memory using the same column headers the import
 * parser expects (underscore names, added to COLUMN_MAP in imports.ts).
 */
async function buildImportXlsx(
  rows: Array<{
    company: string;
    recipe: string;
    serving_date: string;
    shipment_date: string;
    meal_category: string;
    amount: number;
    storage: string;
    notes: string;
  }>
): Promise<Buffer> {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  ws.columns = [
    { header: 'company',       key: 'company' },
    { header: 'recipe',        key: 'recipe' },
    { header: 'serving_date',  key: 'serving_date' },
    { header: 'shipment_date', key: 'shipment_date' },
    { header: 'meal_category', key: 'meal_category' },
    { header: 'amount',        key: 'amount' },
    { header: 'storage',       key: 'storage' },
    { header: 'notes',         key: 'notes' },
  ];
  for (const row of rows) {
    ws.addRow(row);
  }
  return Buffer.from(await wb.xlsx.writeBuffer());
}

// ─── Browser upload helper ────────────────────────────────────────────────────

/**
 * Navigate to the import page and upload an XLSX buffer via the file input.
 * Returns after networkidle — caller asserts on the resulting page state.
 */
async function uploadImportFile(
  adminPage: Page,
  xlsx: Buffer,
  filename = 'import.xlsx'
) {
  await adminPage.goto('/admin/orders/import');
  await adminPage.waitForLoadState('networkidle');

  await adminPage.locator('input[type="file"][name="file"]').setInputFiles({
    name: filename,
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    buffer: xlsx,
  });

  // Scope to the upload form — the admin sidebar now contains its own
  // `<form method="POST" action="/logout"><button type="submit">` for
  // the Sign Out button, so a bare `button[type="submit"]` selector hits
  // two elements and fails strict mode.
  await adminPage.locator('form:has(input[name="file"]) button[type="submit"]').click();
  await adminPage.waitForLoadState('networkidle');
}

// ─── Setup / Teardown ─────────────────────────────────────────────────────────

let pool: Pool;

test.beforeAll(async () => {
  pool = makePool();

  // Clean up any stale data from a previous interrupted run
  await pool.query(
    `DELETE FROM "order" WHERE company_id IN (
       SELECT id FROM company WHERE name = ANY($1)
     )`,
    [[IMPORT_COMPANY_A, IMPORT_COMPANY_B]]
  );
  await pool.query(`DELETE FROM recipe WHERE name = ANY($1)`, [[IMPORT_RECIPE_A, IMPORT_RECIPE_B]]);
  await pool.query(`DELETE FROM company WHERE name = ANY($1)`, [[IMPORT_COMPANY_A, IMPORT_COMPANY_B]]);

  // Seed companies
  await pool.query(
    `INSERT INTO company (id, name, active) VALUES ($1, $2, true), ($3, $4, true)`,
    [randomUUID(), IMPORT_COMPANY_A, randomUUID(), IMPORT_COMPANY_B]
  );

  // Seed recipes
  await pool.query(
    `INSERT INTO recipe (id, name, active, created_at, updated_at)
     VALUES ($1, $2, true, now(), now()), ($3, $4, true, now(), now())`,
    [randomUUID(), IMPORT_RECIPE_A, randomUUID(), IMPORT_RECIPE_B]
  );
});

test.afterAll(async () => {
  await pool.query(
    `DELETE FROM "order" WHERE company_id IN (
       SELECT id FROM company WHERE name = ANY($1)
     )`,
    [[IMPORT_COMPANY_A, IMPORT_COMPANY_B]]
  );
  await pool.query(`DELETE FROM recipe WHERE name = ANY($1)`, [[IMPORT_RECIPE_A, IMPORT_RECIPE_B]]);
  await pool.query(`DELETE FROM company WHERE name = ANY($1)`, [[IMPORT_COMPANY_A, IMPORT_COMPANY_B]]);
  await pool.end();
});

// ─── Tests ────────────────────────────────────────────────────────────────────

test.describe('Order import', () => {
  test('happy path — valid rows show success and are persisted', async ({ adminPage }) => {
    const xlsx = await buildImportXlsx([
      {
        company:       IMPORT_COMPANY_A,
        recipe:        IMPORT_RECIPE_A,
        serving_date:  IMPORT_SERVING_DATE,
        shipment_date: IMPORT_SHIPMENT_DATE,
        meal_category: 'Lunch',
        amount:        25,
        storage:       'chilled',
        notes:         '',
      },
      {
        company:       IMPORT_COMPANY_B,
        recipe:        IMPORT_RECIPE_B,
        serving_date:  IMPORT_SERVING_DATE,
        shipment_date: IMPORT_SHIPMENT_DATE,
        meal_category: 'Breakfast',
        amount:        10,
        storage:       'frozen',
        notes:         'Early delivery please',
      },
    ]);

    await uploadImportFile(adminPage, xlsx, 'happy-import.xlsx');

    // Success banner must be visible
    await expect(adminPage.locator('.success-header')).toBeVisible({ timeout: 15_000 });
    const insertedText = await adminPage.locator('.stat-value').first().textContent();
    expect(Number(insertedText?.trim())).toBe(2);

    // DB must have the 2 new orders
    const { rows } = await pool.query(
      `SELECT COUNT(*)::int AS n FROM "order"
       WHERE company_id IN (SELECT id FROM company WHERE name = ANY($1))
         AND serving_date = $2::date`,
      [[IMPORT_COMPANY_A, IMPORT_COMPANY_B], IMPORT_SERVING_DATE]
    );
    expect(rows[0].n).toBe(2);
  });

  test('error path — unknown company shows validation errors, zero inserts', async ({
    adminPage,
  }) => {
    const { rows: before } = await pool.query(
      `SELECT COUNT(*)::int AS n FROM "order"
       WHERE company_id IN (SELECT id FROM company WHERE name = ANY($1))`,
      [[IMPORT_COMPANY_A, IMPORT_COMPANY_B]]
    );

    const xlsx = await buildImportXlsx([
      {
        company:       IMPORT_COMPANY_A,
        recipe:        IMPORT_RECIPE_A,
        serving_date:  '2099-07-01',
        shipment_date: '2099-06-30',
        meal_category: 'Lunch',
        amount:        5,
        storage:       'chilled',
        notes:         '',
      },
      {
        company:       '__nonexistent_company_xyz__', // will fail reference resolution
        recipe:        IMPORT_RECIPE_A,
        serving_date:  '2099-07-01',
        shipment_date: '2099-06-30',
        meal_category: 'Lunch',
        amount:        5,
        storage:       'chilled',
        notes:         '',
      },
    ]);

    await uploadImportFile(adminPage, xlsx, 'bad-import.xlsx');

    // Validation error box must be visible
    await expect(adminPage.locator('.error-box')).toBeVisible({ timeout: 15_000 });

    // All-or-nothing: no new orders inserted
    const { rows: after } = await pool.query(
      `SELECT COUNT(*)::int AS n FROM "order"
       WHERE company_id IN (SELECT id FROM company WHERE name = ANY($1))`,
      [[IMPORT_COMPANY_A, IMPORT_COMPANY_B]]
    );
    expect(after[0].n).toBe(before[0].n);
  });

  test('template download returns xlsx with correct headers and example rows', async ({
    adminPage,
  }) => {
    const response = await adminPage.request.get('/admin/orders/import/template');
    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
    );
    expect(response.headers()['content-disposition']).toContain('order-import-template.xlsx');

    // Parse and verify header row
    const wb = new ExcelJS.Workbook();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await wb.xlsx.load(Buffer.from(await response.body()) as any);
    const ws = wb.worksheets[0];
    expect(ws).toBeDefined();

    const headers: string[] = [];
    ws.getRow(1).eachCell({ includeEmpty: false }, (cell) => {
      headers.push(String(cell.value ?? '').trim());
    });

    expect(headers).toEqual([
      'Customer', 'Serving Date', 'Recipe', 'Meal', 'Day',
      'Portion Size', 'Order Amount', 'Client', 'Storage', 'Shipment Date',
    ]);

    // Header row + at least 2 example rows
    expect(ws.rowCount).toBeGreaterThanOrEqual(3);
  });
});
