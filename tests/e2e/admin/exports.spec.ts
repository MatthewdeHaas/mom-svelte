/**
 * Export tests — validate that the Excel and PDF export endpoints return
 * well-formed files with the expected content.
 *
 * Strategy: use page.request (same auth context as adminPage) to hit the
 * GET endpoints directly, then parse the response body in-process.
 *
 * Seed data (set up in global-setup.ts):
 *   - Recipe:     __e2e_test_recipe__
 *   - Ingredient: __e2e_test_ingredient__  (2 kg per recipe serving)
 *   - Allergen:   __e2e_test_allergen__
 *   - Order:      serving_date = shipment_date = 2099-01-15
 *                 amount = 3, meal_category = 'lunch', chilled = true, frozen = false
 *
 * Expected totals:
 *   Production order: 1 row — recipe=__e2e_test_recipe__, amount=3, chilled=✓
 *   Shopping list:    1 row — ingredient=__e2e_test_ingredient__, total=6 kg
 *   Packing slip:     1 PDF for __e2e_test_company__ (single-slip, not zipped)
 */

import ExcelJS from 'exceljs';
import type { Page } from '@playwright/test';
import { test, expect } from '../fixtures';
import {
  E2E_RECIPE_NAME,
  E2E_INGREDIENT_NAME,
  E2E_ALLERGEN_NAME,
  E2E_COMPANY_NAME,
  E2E_EXPORT_DATE,
} from '../global-setup';

// ─── helpers ─────────────────────────────────────────────────────────────────

async function fetchExport(adminPage: Page, path: string) {
  const response = await adminPage.request.get(path);
  expect(response.status(), `GET ${path} returned ${response.status()}`).toBe(200);
  return response;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function parseXlsx(buffer: any): Promise<ExcelJS.Worksheet> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(buffer);
  const sheet = workbook.worksheets[0];
  if (!sheet) throw new Error('No worksheet found in workbook');
  return sheet;
}

/** Return all cell values in a row as trimmed strings. */
function rowValues(row: ExcelJS.Row): string[] {
  const vals: string[] = [];
  row.eachCell({ includeEmpty: false }, (cell) => {
    vals.push(String(cell.value ?? '').trim());
  });
  return vals;
}

/** Find the first data row (after the header) whose first cell matches. */
function findDataRow(sheet: ExcelJS.Worksheet, text: string): ExcelJS.Row | undefined {
  for (let i = 2; i <= sheet.rowCount; i++) {
    const row = sheet.getRow(i);
    const first = String(row.getCell(1).value ?? '').trim();
    if (first === text) return row;
  }
  return undefined;
}

/** Find the first data row whose column `col` (1-based) matches. */
function findRowByColumn(
  sheet: ExcelJS.Worksheet,
  col: number,
  text: string
): ExcelJS.Row | undefined {
  for (let i = 2; i <= sheet.rowCount; i++) {
    const row = sheet.getRow(i);
    const val = String(row.getCell(col).value ?? '').trim();
    if (val === text) return row;
  }
  return undefined;
}

// ─── Production Order (.xlsx) ─────────────────────────────────────────────────

test.describe('Production Order export', () => {
  test('returns xlsx with correct Content-Type', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/production-order?shipment_date=${E2E_EXPORT_DATE}`
    );
    expect(res.headers()['content-type']).toContain(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
    );
    expect(res.headers()['content-disposition']).toContain('.xlsx');
  });

  test('worksheet is named "Production Order"', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/production-order?shipment_date=${E2E_EXPORT_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));
    expect(sheet.name).toBe('Production Order');
  });

  test('header row has correct column labels', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/production-order?shipment_date=${E2E_EXPORT_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));
    const headers = rowValues(sheet.getRow(1));
    expect(headers).toEqual([
      'Customer', 'Serving Date', 'Recipe', 'Meal', 'Day',
      'Portion Size', 'Order Amount', 'Quantity to Produce/Ship',
      'Client', 'Storage', 'Shipment Date',
    ]);
  });

  test('contains a row for the seeded recipe with correct values', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/production-order?shipment_date=${E2E_EXPORT_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));

    // Production sheet is now per-order; find row by recipe name (col 3)
    const row = findRowByColumn(sheet, 3, E2E_RECIPE_NAME);
    expect(row, `Row for "${E2E_RECIPE_NAME}" not found in sheet`).toBeDefined();

    // [Customer, ServingDate, Recipe, Meal, Day, PortionSize, OrderAmount,
    //  QtyProduce, Client, Storage, ShipmentDate]
    expect(String(row!.getCell(1).value ?? '').trim()).toBe(E2E_COMPANY_NAME);
    expect(String(row!.getCell(3).value ?? '').trim()).toBe(E2E_RECIPE_NAME);
    expect(String(row!.getCell(4).value ?? '').trim()).toBe('lunch');
    expect(Number(row!.getCell(7).value)).toBe(3);        // Order Amount
    expect(String(row!.getCell(10).value ?? '').trim()).toBe('c'); // Storage: chilled
    // Allergens are no longer carried on the production sheet — they live on
    // the shopping list; see the Shopping List tests below.
  });
});

// ─── Shopping List (.xlsx) ────────────────────────────────────────────────────

test.describe('Shopping List export', () => {
  test('returns xlsx with correct Content-Type', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/shopping-list?shipment_date=${E2E_EXPORT_DATE}`
    );
    expect(res.headers()['content-type']).toContain(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
    );
  });

  test('worksheet is named "Shopping List"', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/shopping-list?shipment_date=${E2E_EXPORT_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));
    expect(sheet.name).toBe('Shopping List');
  });

  test('header row has correct column labels', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/shopping-list?shipment_date=${E2E_EXPORT_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));
    const headers = rowValues(sheet.getRow(1));
    expect(headers).toEqual(['Ingredient', 'Total Amount', 'Unit', 'Allergens']);
  });

  test('contains the seeded ingredient with correct aggregated quantity', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/shopping-list?shipment_date=${E2E_EXPORT_DATE}`
    );
    const sheet = await parseXlsx(Buffer.from(await res.body()));

    const row = findDataRow(sheet, E2E_INGREDIENT_NAME);
    expect(row, `Row for "${E2E_INGREDIENT_NAME}" not found`).toBeDefined();

    // recipe_ingredient.amount (2) × order.amount (3) = 6 kg
    const totalAmount = Number(row!.getCell(2).value);
    expect(totalAmount).toBe(6);
    expect(String(row!.getCell(3).value ?? '').trim()).toBe('kg');
    expect(String(row!.getCell(4).value ?? '').trim()).toBe(E2E_ALLERGEN_NAME);
  });
});

// ─── Packing Slips (PDF) ──────────────────────────────────────────────────────

test.describe('Packing Slips export', () => {
  test('returns a PDF (single slip, not zipped) with correct Content-Type', async ({
    adminPage,
  }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/packing-slips?shipment_date=${E2E_EXPORT_DATE}`
    );
    expect(res.headers()['content-type']).toContain('application/pdf');
    expect(res.headers()['content-disposition']).toContain('.pdf');
  });

  test('PDF binary starts with %PDF magic bytes', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/packing-slips?shipment_date=${E2E_EXPORT_DATE}`
    );
    const buf = Buffer.from(await res.body());
    // All valid PDF files begin with %PDF-
    expect(buf.slice(0, 5).toString('ascii')).toBe('%PDF-');
  });

  test('PDF is non-trivially sized (contains rendered content)', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/packing-slips?shipment_date=${E2E_EXPORT_DATE}`
    );
    const buf = Buffer.from(await res.body());
    // A blank PDF with no content is ~1 KB; a rendered slip with header/table/rows should be > 2 KB
    expect(buf.length).toBeGreaterThan(2_000);
  });

  test('PDF contains the company name as text', async ({ adminPage }) => {
    const res = await fetchExport(
      adminPage,
      `/admin/exports/packing-slips?shipment_date=${E2E_EXPORT_DATE}`
    );
    const buf = Buffer.from(await res.body());
    // PDFKit encodes strings in PDF object streams as UTF-16BE (with BOM þÿ).
    // Convert company name to UTF-16BE and search the raw binary for it.
    const utf16 = Buffer.from(E2E_COMPANY_NAME, 'utf16le').swap16();
    expect(buf.indexOf(utf16)).toBeGreaterThan(-1);
  });
});
