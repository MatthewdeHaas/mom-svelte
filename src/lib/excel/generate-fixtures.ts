/**
 * Synthetic fixture generator for the Excel importer tests.
 *
 * Each fixture pins ONE normalization behavior so the test suite stays
 * focused — the real-client workbook combines many of these in one file,
 * but the synthetic fixtures let `normalization.test.ts` isolate failures.
 *
 * Usage: pnpm exec tsx src/lib/excel/generate-fixtures.ts
 * Output: tests/integration/excel/fixtures/<name>.xlsx (deterministic bytes)
 */
import ExcelJS from 'exceljs';
import JSZip from 'jszip';
import { writeFileSync, mkdirSync } from 'fs';
import { resolve } from 'path';

/**
 * Local copy of the deleted `src/lib/excel/determinism.ts::pinXlsxDates`.
 * Inlined because this script is stand-alone build-time tooling for
 * fixture generation — the request-path code uses Rust's
 * `pin_xlsx_bytes` instead (see `admin-operations-rust/crates/excel/src/determinism.rs`).
 * Keep this in lock-step with `sanitize-real-fixture.ts`'s copy so both
 * fixture generators produce byte-identical output for the same input.
 */
async function pinXlsxDates(buf: Buffer): Promise<Buffer> {
  const EPOCH = new Date(0);
  const zip = await JSZip.loadAsync(buf);

  const coreFile = zip.file('docProps/core.xml');
  if (coreFile) {
    const xml = await coreFile.async('string');
    const scrubbed = xml
      .replace(/<dc:creator>Unknown<\/dc:creator>/g, '<dc:creator></dc:creator>')
      .replace(
        /<cp:lastModifiedBy>Unknown<\/cp:lastModifiedBy>/g,
        '<cp:lastModifiedBy></cp:lastModifiedBy>',
      );
    zip.file('docProps/core.xml', scrubbed);
  }

  zip.forEach((_path, file) => {
    file.date = EPOCH;
  });

  return zip.generateAsync({
    type: 'nodebuffer',
    compression: 'DEFLATE',
    compressionOptions: { level: 6 },
  });
}

const OUT_DIR = resolve(process.cwd(), 'tests/integration/excel/fixtures');

interface Column {
  header: string;
  key: string;
}

type RowValue = string | number | Date | null;

async function emitWorkbook(
  filename: string,
  sheetName: string,
  columns: Column[],
  rows: Record<string, RowValue>[],
): Promise<void> {
  const wb = new ExcelJS.Workbook();
  wb.created = new Date(0);
  wb.modified = new Date(0);
  wb.creator = '';
  wb.lastModifiedBy = '';
  wb.company = '';
  wb.title = '';
  wb.description = '';

  const ws = wb.addWorksheet(sheetName);
  ws.columns = columns.map((c) => ({ header: c.header, key: c.key, width: 16 }));

  for (const row of rows) {
    ws.addRow(row);
  }

  const buf = await wb.xlsx.writeBuffer();
  const pinned = await pinXlsxDates(Buffer.from(buf));
  mkdirSync(OUT_DIR, { recursive: true });
  const out = resolve(OUT_DIR, filename);
  writeFileSync(out, pinned);
  console.log(`✓ wrote ${filename} (${pinned.length} bytes)`);
}

// -------------------------------------------------------------
// clean-template.xlsx — the canonical format the importer accepts
// without any warnings. Used as the baseline against which messy
// fixtures must produce equal downstream exports.
// -------------------------------------------------------------
async function cleanTemplate() {
  const columns: Column[] = [
    { header: 'Customer',       key: 'customer' },
    { header: 'Recipe',         key: 'recipe' },
    { header: 'Serving Date',   key: 'serving_date' },
    { header: 'Shipment Date',  key: 'shipment_date' },
    { header: 'Meal',           key: 'meal' },
    { header: 'Order Amount',   key: 'amount' },
    { header: 'Portion Size',   key: 'portion_size' },
    { header: 'Day',            key: 'day' },
    { header: 'Storage',        key: 'storage' },
    { header: 'Client',         key: 'client' },
  ];

  const rows: Record<string, RowValue>[] = [
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Lunch',  amount: 10, portion_size: 1.5, day: 1, storage: 'c', client: 'Alice' },
    { customer: 'Acme Co', recipe: 'Beef Stew',     serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Dinner', amount: 5,  portion_size: 1.5, day: 1, storage: 'f', client: 'Bob'   },
  ];

  await emitWorkbook('clean-template.xlsx', 'Orders', columns, rows);
}

// -------------------------------------------------------------
// frozen-chilled-split.xlsx — the real client's shape. All four
// Frozen/Chilled combinations in 4 rows, no Storage column.
// Expected warnings: storage_both_set (1), storage_missing_defaulted_chilled (1).
// -------------------------------------------------------------
async function frozenChilledSplit() {
  const columns: Column[] = [
    { header: 'Customer',       key: 'customer' },
    { header: 'Recipe',         key: 'recipe' },
    { header: 'Serving Date',   key: 'serving_date' },
    { header: 'Shipment Date',  key: 'shipment_date' },
    { header: 'Meal',           key: 'meal' },
    { header: 'Order Amount',   key: 'amount' },
    { header: 'Portion Size',   key: 'portion_size' },
    { header: 'Day',            key: 'day' },
    { header: 'Frozen',         key: 'frozen' },
    { header: ' Chilled',       key: 'chilled' }, // leading-space header matches real file
    { header: 'Client',         key: 'client' },
  ];

  const rows: Record<string, RowValue>[] = [
    // row 2: Frozen only → storage='f'
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Lunch',     amount: 10, portion_size: 1.5, day: 1, frozen: 'x', chilled: '', client: 'A' },
    // row 3: Chilled only → storage='c'
    { customer: 'Acme Co', recipe: 'Beef Stew',     serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Dinner',    amount: 5,  portion_size: 1.5, day: 1, frozen: '',  chilled: 'x', client: 'B' },
    // row 4: Both set → storage='f' + storage_both_set warning
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Breakfast', amount: 3,  portion_size: 1.5, day: 1, frozen: 'x', chilled: 'x', client: 'C' },
    // row 5: Neither → storage='c' + storage_missing_defaulted_chilled warning
    { customer: 'Acme Co', recipe: 'Beef Stew',     serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Snack',     amount: 2,  portion_size: 1.5, day: 1, frozen: '',  chilled: '',  client: 'D' },
  ];

  await emitWorkbook('frozen-chilled-split.xlsx', 'Orders', columns, rows);
}

// -------------------------------------------------------------
// item-column-duplicate.xlsx — Item column present. On one row,
// Item disagrees with Recipe (real-client Melon-vs-Banana typo).
// Expected warnings: item_column_ignored (sheet), item_recipe_mismatch (row 3).
// -------------------------------------------------------------
async function itemColumnDuplicate() {
  const columns: Column[] = [
    { header: 'Customer',       key: 'customer' },
    { header: 'Item',           key: 'item' },
    { header: 'Recipe',         key: 'recipe' },
    { header: 'Serving Date',   key: 'serving_date' },
    { header: 'Shipment Date',  key: 'shipment_date' },
    { header: 'Meal',           key: 'meal' },
    { header: 'Order Amount',   key: 'amount' },
    { header: 'Portion Size',   key: 'portion_size' },
    { header: 'Day',            key: 'day' },
    { header: 'Storage',        key: 'storage' },
    { header: 'Client',         key: 'client' },
  ];

  const rows: Record<string, RowValue>[] = [
    // row 2: Item matches Recipe — no mismatch warning
    { customer: 'Acme Co', item: 'Chicken Curry', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Lunch',  amount: 10, portion_size: 1.5, day: 1, storage: 'c', client: 'A' },
    // row 3: Item disagrees with Recipe — mismatch warning (Recipe wins)
    { customer: 'Acme Co', item: 'Melon',         recipe: 'Beef Stew',     serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Dinner', amount: 5,  portion_size: 1.5, day: 1, storage: 'f', client: 'B' },
  ];

  await emitWorkbook('item-column-duplicate.xlsx', 'Orders', columns, rows);
}

// -------------------------------------------------------------
// boilerplate-rows.xlsx — two valid rows, then a trailer of
// office-use / packing-slip boilerplate rows that must be dropped.
// Expected: inserted=2, boilerplate_row_stopped warning naming row 4.
// -------------------------------------------------------------
async function boilerplateRows() {
  const columns: Column[] = [
    { header: 'Customer',       key: 'customer' },
    { header: 'Recipe',         key: 'recipe' },
    { header: 'Serving Date',   key: 'serving_date' },
    { header: 'Shipment Date',  key: 'shipment_date' },
    { header: 'Meal',           key: 'meal' },
    { header: 'Order Amount',   key: 'amount' },
    { header: 'Portion Size',   key: 'portion_size' },
    { header: 'Day',            key: 'day' },
    { header: 'Storage',        key: 'storage' },
    { header: 'Client',         key: 'client' },
  ];

  const rows: Record<string, RowValue>[] = [
    // row 2: real order
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Lunch',  amount: 10, portion_size: 1.5, day: 1, storage: 'c', client: 'A' },
    // row 3: real order
    { customer: 'Acme Co', recipe: 'Beef Stew',     serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Dinner', amount: 5,  portion_size: 1.5, day: 1, storage: 'f', client: 'B' },
    // row 4: boilerplate — EOF marker. Everything from here down is dropped.
    { customer: 'PACKING SLIP', recipe: '', serving_date: null, shipment_date: null, meal: null, amount: null, portion_size: null, day: null, storage: null, client: null },
    // row 5: ignored
    { customer: 'OFFICE USE ONLY', recipe: '', serving_date: null, shipment_date: null, meal: null, amount: null, portion_size: null, day: null, storage: null, client: null },
    // row 6: ignored
    { customer: 'Cambros: 3', recipe: '', serving_date: null, shipment_date: null, meal: null, amount: null, portion_size: null, day: null, storage: null, client: null },
  ];

  await emitWorkbook('boilerplate-rows.xlsx', 'Orders', columns, rows);
}

// -------------------------------------------------------------
// zero-amount.xlsx — single row with Order Amount = 0. Pins the
// current reject-on-zero behavior: Zod `.positive()` rejects it,
// surfaces as a regular error (not a warning).
// -------------------------------------------------------------
async function zeroAmount() {
  const columns: Column[] = [
    { header: 'Customer',       key: 'customer' },
    { header: 'Recipe',         key: 'recipe' },
    { header: 'Serving Date',   key: 'serving_date' },
    { header: 'Shipment Date',  key: 'shipment_date' },
    { header: 'Meal',           key: 'meal' },
    { header: 'Order Amount',   key: 'amount' },
    { header: 'Portion Size',   key: 'portion_size' },
    { header: 'Day',            key: 'day' },
    { header: 'Storage',        key: 'storage' },
    { header: 'Client',         key: 'client' },
  ];

  const rows: Record<string, RowValue>[] = [
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Lunch', amount: 0, portion_size: 1.5, day: 1, storage: 'c', client: 'A' },
  ];

  await emitWorkbook('zero-amount.xlsx', 'Orders', columns, rows);
}

// -------------------------------------------------------------
// meal-case-variants.xlsx — meal column values with varying case
// and whitespace. Pins the meal_case_normalized warning.
// -------------------------------------------------------------
async function mealCaseVariants() {
  const columns: Column[] = [
    { header: 'Customer',       key: 'customer' },
    { header: 'Recipe',         key: 'recipe' },
    { header: 'Serving Date',   key: 'serving_date' },
    { header: 'Shipment Date',  key: 'shipment_date' },
    { header: 'Meal',           key: 'meal' },
    { header: 'Order Amount',   key: 'amount' },
    { header: 'Portion Size',   key: 'portion_size' },
    { header: 'Day',            key: 'day' },
    { header: 'Storage',        key: 'storage' },
    { header: 'Client',         key: 'client' },
  ];

  const rows: Record<string, RowValue>[] = [
    // row 2: ' lunch' → 'Lunch' (meal_case_normalized)
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: ' lunch',  amount: 10, portion_size: 1.5, day: 1, storage: 'c', client: 'A' },
    // row 3: 'lunch r' → 'Lunch R' (meal_case_normalized)
    { customer: 'Acme Co', recipe: 'Beef Stew',     serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'lunch r', amount: 5,  portion_size: 1.5, day: 1, storage: 'f', client: 'B' },
    // row 4: 'Dinner' → unchanged (no warning)
    { customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Dinner',  amount: 3,  portion_size: 1.5, day: 1, storage: 'c', client: 'C' },
  ];

  await emitWorkbook('meal-case-variants.xlsx', 'Orders', columns, rows);
}

// -------------------------------------------------------------
// extra-sheets.xlsx — valid data on sheet 1, two non-data sheets
// after. Expected warning: extra_sheets_ignored (count=2).
// Built with raw ExcelJS calls so we can add multiple sheets.
// -------------------------------------------------------------
async function extraSheets() {
  const wb = new ExcelJS.Workbook();
  wb.created = new Date(0);
  wb.modified = new Date(0);
  wb.creator = '';
  wb.lastModifiedBy = '';
  wb.company = '';
  wb.title = '';
  wb.description = '';

  const ws1 = wb.addWorksheet('Orders');
  ws1.columns = [
    { header: 'Customer',       key: 'customer',      width: 16 },
    { header: 'Recipe',         key: 'recipe',        width: 20 },
    { header: 'Serving Date',   key: 'serving_date',  width: 14 },
    { header: 'Shipment Date',  key: 'shipment_date', width: 14 },
    { header: 'Meal',           key: 'meal',          width: 10 },
    { header: 'Order Amount',   key: 'amount',        width: 10 },
    { header: 'Portion Size',   key: 'portion_size',  width: 12 },
    { header: 'Day',            key: 'day',           width: 6 },
    { header: 'Storage',        key: 'storage',       width: 8 },
    { header: 'Client',         key: 'client',        width: 16 },
  ];
  ws1.addRow({ customer: 'Acme Co', recipe: 'Chicken Curry', serving_date: '2025-06-15', shipment_date: '2025-06-10', meal: 'Lunch', amount: 10, portion_size: 1.5, day: 1, storage: 'c', client: 'A' });

  wb.addWorksheet('Packing Slip Template').addRow(['boilerplate']);
  wb.addWorksheet('Office Layout').addRow(['boilerplate']);

  const buf = await wb.xlsx.writeBuffer();
  const pinned = await pinXlsxDates(Buffer.from(buf));
  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(resolve(OUT_DIR, 'extra-sheets.xlsx'), pinned);
  console.log(`✓ wrote extra-sheets.xlsx (${pinned.length} bytes)`);
}

async function main() {
  await cleanTemplate();
  await frozenChilledSplit();
  await itemColumnDuplicate();
  await boilerplateRows();
  await zeroAmount();
  await mealCaseVariants();
  await extraSheets();
  console.log('\nAll fixtures regenerated.');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
