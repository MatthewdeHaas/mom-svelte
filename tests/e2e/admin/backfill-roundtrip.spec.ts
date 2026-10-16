/**
 * Backfill round-trip integration test.
 *
 * Strategy:
 *   1. Seed companies and recipes that mirror the backfill data
 *   2. Build an import XLSX in the app's canonical format (same as backfill output)
 *   3. Upload it via the admin import page
 *   4. Export via Production Order and Shopping List endpoints
 *   5. Verify exported XLSX contains the correct aggregated data
 *   6. Download both exports to ~/Downloads for manual inspection
 *
 * Uses a unique far-future date range (2099-03-01 week) to avoid collisions
 * with global-setup seed data.
 */

import ExcelJS from 'exceljs';
import { Pool } from 'pg';
import { randomUUID } from 'crypto';
import { writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
import os from 'os';
import type { Page } from '@playwright/test';
import { test, expect } from '../fixtures';

// ─── Seed constants ──────────────────────────────────────────────────────────

const PREFIX = '__backfill_roundtrip__';

const COMPANIES = [
  `${PREFIX}Aurora CHATS`,
  `${PREFIX}SLEC`,
  `${PREFIX}MIYM`,
];

const RECIPES = [
  { name: `${PREFIX}Butter Chicken`, ingredient: `${PREFIX}Diced Chicken`, amount: 0.1, unit: 'kg', allergen: null },
  { name: `${PREFIX}Lentil Soup`, ingredient: `${PREFIX}Red Lentils`, amount: 0.15, unit: 'kg', allergen: null },
  { name: `${PREFIX}Mac and Cheese`, ingredient: `${PREFIX}Cheddar Cheese`, amount: 0.08, unit: 'kg', allergen: `${PREFIX}Dairy` },
];

// Far-future dates to avoid collision with other tests
const SERVING_DATE = '2099-03-03';
const SHIPMENT_DATE = '2099-03-02';

// ─── Import rows — what we'll upload via the app ─────────────────────────────

const IMPORT_ROWS = [
  { company: COMPANIES[0], recipe: RECIPES[0].name, serving_date: SERVING_DATE, shipment_date: SHIPMENT_DATE, meal_category: 'Lunch', amount: 25, storage: 'chilled', notes: '' },
  { company: COMPANIES[0], recipe: RECIPES[1].name, serving_date: SERVING_DATE, shipment_date: SHIPMENT_DATE, meal_category: 'Soup', amount: 30, storage: 'chilled', notes: '' },
  { company: COMPANIES[1], recipe: RECIPES[0].name, serving_date: SERVING_DATE, shipment_date: SHIPMENT_DATE, meal_category: 'Lunch', amount: 50, storage: 'chilled', notes: '' },
  { company: COMPANIES[1], recipe: RECIPES[2].name, serving_date: SERVING_DATE, shipment_date: SHIPMENT_DATE, meal_category: 'Lunch', amount: 20, storage: 'frozen', notes: 'Extra cheesy' },
  { company: COMPANIES[2], recipe: RECIPES[0].name, serving_date: SERVING_DATE, shipment_date: SHIPMENT_DATE, meal_category: 'Lunch', amount: 15, storage: 'frozen', notes: '' },
  { company: COMPANIES[2], recipe: RECIPES[2].name, serving_date: SERVING_DATE, shipment_date: SHIPMENT_DATE, meal_category: 'Lunch', amount: 10, storage: 'frozen', notes: '' },
];

// ─── Expected aggregates ─────────────────────────────────────────────────────

// Production order groups by recipe_id, sums amount
// Butter Chicken: 25 + 50 + 15 = 90 (mixed storage — first order sets flags)
// Lentil Soup: 30
// Mac and Cheese: 20 + 10 = 30
const EXPECTED_PRODUCTION = [
  { recipe: RECIPES[0].name, amount: 90 },
  { recipe: RECIPES[1].name, amount: 30 },
  { recipe: RECIPES[2].name, amount: 30 },
];

// Shopping list: ingredient amount × order amount, grouped by ingredient
// Diced Chicken: 0.1 × 90 = 9 kg
// Red Lentils: 0.15 × 30 = 4.5 kg
// Cheddar Cheese: 0.08 × 30 = 2.4 kg
const EXPECTED_SHOPPING = [
  { ingredient: RECIPES[0].ingredient, total: 9, unit: 'kg' },
  { ingredient: RECIPES[1].ingredient, total: 4.5, unit: 'kg' },
  { ingredient: RECIPES[2].ingredient, total: 2.4, unit: 'kg' },
];

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makePool() {
  return new Pool({
    connectionString: `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST ?? 'localhost'}:5432/${process.env.POSTGRES_DB}`,
  });
}

async function buildImportXlsx(
  rows: typeof IMPORT_ROWS
): Promise<Buffer> {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  ws.columns = [
    { header: 'company', key: 'company' },
    { header: 'recipe', key: 'recipe' },
    { header: 'serving_date', key: 'serving_date' },
    { header: 'shipment_date', key: 'shipment_date' },
    { header: 'meal_category', key: 'meal_category' },
    { header: 'amount', key: 'amount' },
    { header: 'storage', key: 'storage' },
    { header: 'notes', key: 'notes' },
  ];
  for (const row of rows) ws.addRow(row);
  return Buffer.from(await wb.xlsx.writeBuffer());
}

async function uploadImportFile(adminPage: Page, xlsx: Buffer, filename = 'import.xlsx') {
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

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function parseXlsx(buffer: any): Promise<ExcelJS.Worksheet> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(buffer);
  const sheet = workbook.worksheets[0];
  if (!sheet) throw new Error('No worksheet found in workbook');
  return sheet;
}

function rowValues(row: ExcelJS.Row): string[] {
  const vals: string[] = [];
  row.eachCell({ includeEmpty: false }, (cell) => {
    vals.push(String(cell.value ?? '').trim());
  });
  return vals;
}

function findDataRow(sheet: ExcelJS.Worksheet, text: string): ExcelJS.Row | undefined {
  for (let i = 2; i <= sheet.rowCount; i++) {
    const row = sheet.getRow(i);
    const first = String(row.getCell(1).value ?? '').trim();
    if (first === text) return row;
  }
  return undefined;
}

/** Return every data row whose cell at `col` (1-based) matches. */
function findRowsByColumn(
  sheet: ExcelJS.Worksheet,
  col: number,
  text: string
): ExcelJS.Row[] {
  const matches: ExcelJS.Row[] = [];
  for (let i = 2; i <= sheet.rowCount; i++) {
    const row = sheet.getRow(i);
    const val = String(row.getCell(col).value ?? '').trim();
    if (val === text) matches.push(row);
  }
  return matches;
}

// ─── Setup / Teardown ────────────────────────────────────────────────────────

let pool: Pool;

test.beforeAll(async () => {
  pool = makePool();

  // Clean up any stale data
  await pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name = ANY($1))`,
    [COMPANIES]
  );
  await pool.query(
    `DELETE FROM recipe_ingredient WHERE recipe_id IN (SELECT id FROM recipe WHERE name = ANY($1))`,
    [RECIPES.map((r) => r.name)]
  );
  await pool.query(
    `DELETE FROM ingredient_allergen WHERE ingredient_id IN (SELECT id FROM ingredient WHERE name = ANY($1))`,
    [RECIPES.map((r) => r.ingredient)]
  );
  await pool.query(`DELETE FROM recipe WHERE name = ANY($1)`, [RECIPES.map((r) => r.name)]);
  await pool.query(`DELETE FROM ingredient WHERE name = ANY($1)`, [RECIPES.map((r) => r.ingredient)]);
  await pool.query(`DELETE FROM allergen WHERE name LIKE $1`, [`${PREFIX}%`]);
  await pool.query(`DELETE FROM company WHERE name = ANY($1)`, [COMPANIES]);

  // Seed companies
  for (const name of COMPANIES) {
    await pool.query(`INSERT INTO company (id, name, active) VALUES ($1, $2, true)`, [randomUUID(), name]);
  }

  // Seed allergen
  const dairyAllergenId = randomUUID();
  await pool.query(`INSERT INTO allergen (id, name) VALUES ($1, $2)`, [dairyAllergenId, `${PREFIX}Dairy`]);

  // Seed ingredients
  const ingredientIds: Record<string, string> = {};
  for (const r of RECIPES) {
    const id = randomUUID();
    await pool.query(`INSERT INTO ingredient (id, name, active) VALUES ($1, $2, true)`, [id, r.ingredient]);
    ingredientIds[r.ingredient] = id;
    if (r.allergen) {
      await pool.query(
        `INSERT INTO ingredient_allergen (ingredient_id, allergen_id) VALUES ($1, $2)`,
        [id, dairyAllergenId]
      );
    }
  }

  // Seed recipes with ingredients
  for (const r of RECIPES) {
    const recipeId = randomUUID();
    await pool.query(
      `INSERT INTO recipe (id, name, active, created_at, updated_at) VALUES ($1, $2, true, now(), now())`,
      [recipeId, r.name]
    );
    await pool.query(
      `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) VALUES ($1, $2, $3, $4)`,
      [recipeId, ingredientIds[r.ingredient], r.amount, r.unit]
    );
  }
});

test.afterAll(async () => {
  await pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name = ANY($1))`,
    [COMPANIES]
  );
  await pool.query(
    `DELETE FROM recipe_ingredient WHERE recipe_id IN (SELECT id FROM recipe WHERE name = ANY($1))`,
    [RECIPES.map((r) => r.name)]
  );
  await pool.query(
    `DELETE FROM ingredient_allergen WHERE ingredient_id IN (SELECT id FROM ingredient WHERE name = ANY($1))`,
    [RECIPES.map((r) => r.ingredient)]
  );
  await pool.query(`DELETE FROM recipe WHERE name = ANY($1)`, [RECIPES.map((r) => r.name)]);
  await pool.query(`DELETE FROM ingredient WHERE name = ANY($1)`, [RECIPES.map((r) => r.ingredient)]);
  await pool.query(`DELETE FROM allergen WHERE name LIKE $1`, [`${PREFIX}%`]);
  await pool.query(`DELETE FROM company WHERE name = ANY($1)`, [COMPANIES]);
  await pool.end();
});

// ─── Tests ───────────────────────────────────────────────────────────────────

test.describe('Backfill round-trip: import → export', () => {
  test('import canonical XLSX — all rows accepted', async ({ adminPage }) => {
    const xlsx = await buildImportXlsx(IMPORT_ROWS);
    await uploadImportFile(adminPage, xlsx, 'backfill-roundtrip.xlsx');

    // Success banner
    await expect(adminPage.locator('.success-header')).toBeVisible({ timeout: 15_000 });
    const insertedText = await adminPage.locator('.stat-value').first().textContent();
    expect(Number(insertedText?.trim())).toBe(IMPORT_ROWS.length);

    // Verify in DB
    const { rows } = await pool.query(
      `SELECT COUNT(*)::int AS n FROM "order"
       WHERE company_id IN (SELECT id FROM company WHERE name = ANY($1))
         AND serving_date = $2::date`,
      [COMPANIES, SERVING_DATE]
    );
    expect(rows[0].n).toBe(IMPORT_ROWS.length);
  });

  test('production order export contains correct per-order recipe amounts', async ({ adminPage }) => {
    const res = await adminPage.request.get(
      `/admin/exports/production-order?shipment_date=${SERVING_DATE}`
    );
    expect(res.status()).toBe(200);

    const buffer = Buffer.from(await res.body());
    const sheet = await parseXlsx(buffer);

    // Save to ~/Downloads for manual inspection (skip if dir missing in CI)
    try {
      const dlDir = join(os.homedir(), 'Downloads');
      mkdirSync(dlDir, { recursive: true });
      writeFileSync(join(dlDir, 'backfill-test-production-order.xlsx'), buffer);
    } catch { /* non-critical */ }

    // Verify header — production sheet is now per-order (one row per order)
    const headers = rowValues(sheet.getRow(1));
    expect(headers).toEqual([
      'Customer', 'Serving Date', 'Recipe', 'Meal', 'Day',
      'Portion Size', 'Order Amount', 'Quantity to Produce/Ship',
      'Client', 'Storage', 'Shipment Date',
    ]);

    // Verify the sum of Order Amount across all rows per recipe matches the
    // expected aggregate (3 recipes, 6 import rows → 6 output rows).
    for (const expected of EXPECTED_PRODUCTION) {
      const rows = findRowsByColumn(sheet, 3, expected.recipe);
      expect(rows.length, `Rows for "${expected.recipe}" not found in production order`).toBeGreaterThan(0);
      const sum = rows.reduce((acc, r) => acc + Number(r.getCell(7).value), 0);
      expect(sum).toBe(expected.amount);
    }
  });

  test('shopping list export contains correct ingredient totals', async ({ adminPage }) => {
    const res = await adminPage.request.get(
      `/admin/exports/shopping-list?shipment_date=${SERVING_DATE}`
    );
    expect(res.status()).toBe(200);

    const buffer = Buffer.from(await res.body());
    const sheet = await parseXlsx(buffer);

    // Save to ~/Downloads for manual inspection (skip if dir missing in CI)
    try {
      const dlDir = join(os.homedir(), 'Downloads');
      mkdirSync(dlDir, { recursive: true });
      writeFileSync(join(dlDir, 'backfill-test-shopping-list.xlsx'), buffer);
    } catch { /* non-critical */ }

    // Verify header
    const headers = rowValues(sheet.getRow(1));
    expect(headers).toEqual(['Ingredient', 'Total Amount', 'Unit', 'Allergens']);

    // Verify each expected ingredient row
    for (const expected of EXPECTED_SHOPPING) {
      const row = findDataRow(sheet, expected.ingredient);
      expect(row, `Row for "${expected.ingredient}" not found in shopping list`).toBeDefined();
      const total = Number(row!.getCell(2).value);
      expect(total).toBeCloseTo(expected.total, 2);
      expect(String(row!.getCell(3).value ?? '').trim()).toBe(expected.unit);
    }
  });

  test('allergen info is carried through to the shopping list export', async ({ adminPage }) => {
    // Mac and Cheese has Dairy allergen via Cheddar Cheese ingredient.
    // Allergens are only carried on the shopping list — the production sheet
    // is per-order and no longer surfaces allergens (ingredient-level data
    // doesn't belong on a production-planning row).
    const res = await adminPage.request.get(
      `/admin/exports/shopping-list?shipment_date=${SERVING_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));
    const ingRow = findDataRow(sheet, RECIPES[2].ingredient);
    expect(ingRow).toBeDefined();
    const ingAllergens = String(ingRow!.getCell(4).value ?? '').trim();
    expect(ingAllergens).toContain(`${PREFIX}Dairy`);
  });
});
