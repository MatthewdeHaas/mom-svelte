/**
 * Generates static .xlsx fixture files for parse.test.ts.
 * Run once: npx tsx scripts/generate-fixtures.ts
 * Commit the generated files — tests load them with fs.readFileSync.
 */

import ExcelJS from 'exceljs';
import fs from 'fs';
import path from 'path';

const OUT_DIR = path.resolve('tests/integration/excel/fixtures');
fs.mkdirSync(OUT_DIR, { recursive: true });

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

const HEADERS = [
  'Customer',
  'Serving Date',
  'Recipe',
  'Meal',
  'Day',
  'Portion Size',
  'Order Amount',
  'Client',
  'Storage',
  'Shipment Date',
];

/** A confirmed-valid row in display-header order */
const VALID_ROW_VALUES = [
  'Acme Corp',          // Customer
  new Date('2025-06-15T00:00:00Z'), // Serving Date  — Date object so ExcelJS writes a date cell
  'Pasta Primavera',    // Recipe
  'Lunch',              // Meal
  2,                    // Day
  100,                  // Portion Size
  10,                   // Order Amount
  null,                 // Client
  'c',                  // Storage (chilled)
  new Date('2025-06-10T00:00:00Z'), // Shipment Date
];

function addHeaderRow(sheet: ExcelJS.Worksheet, headers: string[]) {
  sheet.addRow(headers);
}

function addDataRow(sheet: ExcelJS.Worksheet, values: unknown[]) {
  const row = sheet.addRow(values);
  // Mark date columns (index 2=Serving Date, 10=Shipment Date, 1-based) as date cells
  // so ExcelJS stores them with the proper numFmt and returns them as Date objects on read-back.
  [2, 10].forEach((col1Based) => {
    const cell = row.getCell(col1Based);
    if (cell.value instanceof Date) {
      cell.numFmt = 'yyyy-mm-dd';
    }
  });
}

async function save(wb: ExcelJS.Workbook, name: string) {
  const dest = path.join(OUT_DIR, name);
  await wb.xlsx.writeFile(dest);
  console.log(`  wrote ${dest}`);
}

// ---------------------------------------------------------------------------
// valid-week.xlsx — happy path, 3 data rows, all columns, real date cells
// ---------------------------------------------------------------------------
async function genValidWeek() {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  addHeaderRow(ws, HEADERS);
  // Three rows with slightly different values
  const rows = [
    VALID_ROW_VALUES,
    [
      'Beta LLC',
      new Date('2025-06-16T00:00:00Z'),
      'Chicken Tikka',
      'Dinner',
      3,
      75,
      5,
      'gluten free',
      'c',
      new Date('2025-06-11T00:00:00Z'),
    ],
    [
      'Gamma Inc',
      new Date('2025-06-17T00:00:00Z'),
      'Caesar Salad',
      'Lunch',
      4,
      100,
      20,
      null,
      'f',
      new Date('2025-06-12T00:00:00Z'),
    ],
  ];
  for (const r of rows) addDataRow(ws, r);
  await save(wb, 'valid-week.xlsx');
}

// ---------------------------------------------------------------------------
// missing-columns.xlsx — only Company + Recipe headers present
// ---------------------------------------------------------------------------
async function genMissingColumns() {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  ws.addRow(['Company', 'Recipe']);
  ws.addRow(['Acme Corp', 'Pasta Primavera']);
  await save(wb, 'missing-columns.xlsx');
}

// ---------------------------------------------------------------------------
// extra-columns.xlsx — standard headers plus two unknown extras
// ---------------------------------------------------------------------------
async function genExtraColumns() {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  const extendedHeaders = [...HEADERS, 'Internal Code', 'Priority'];
  ws.addRow(extendedHeaders);
  const extendedRow = [...VALID_ROW_VALUES, 'IC-001', 'High'];
  addDataRow(ws, extendedRow);
  await save(wb, 'extra-columns.xlsx');
}

// ---------------------------------------------------------------------------
// empty-rows.xlsx — blank rows interspersed between valid rows
// ---------------------------------------------------------------------------
async function genEmptyRows() {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  addHeaderRow(ws, HEADERS);
  addDataRow(ws, VALID_ROW_VALUES);
  // Blank row
  ws.addRow([]);
  addDataRow(ws, [
    'Beta LLC',
    new Date('2025-06-16T00:00:00Z'),
    'Chicken Tikka',
    'Dinner',
    3,
    75,
    5,
    null,
    'c',
    new Date('2025-06-11T00:00:00Z'),
  ]);
  // Another blank row at the end
  ws.addRow([]);
  await save(wb, 'empty-rows.xlsx');
}

// ---------------------------------------------------------------------------
// date-as-date.xlsx — date columns as real Excel date cells (same as valid
// week but isolated so the test assertion is explicit)
// ---------------------------------------------------------------------------
async function genDateAsDate() {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  addHeaderRow(ws, HEADERS);
  addDataRow(ws, VALID_ROW_VALUES);
  await save(wb, 'date-as-date.xlsx');
}

// ---------------------------------------------------------------------------
// multiple-sheets.xlsx — two sheets; only the first should be parsed
// ---------------------------------------------------------------------------
async function genMultipleSheets() {
  const wb = new ExcelJS.Workbook();

  const ws1 = wb.addWorksheet('Sheet One');
  addHeaderRow(ws1, HEADERS);
  addDataRow(ws1, VALID_ROW_VALUES);

  // Second sheet has a single row with different data — must NOT appear in output
  const ws2 = wb.addWorksheet('Sheet Two');
  ws2.addRow(['Company', 'Recipe']);
  ws2.addRow(['Should Not Appear', 'Ignored Recipe']);

  await save(wb, 'multiple-sheets.xlsx');
}

// ---------------------------------------------------------------------------
// empty-sheet.xlsx — headers present but zero data rows
// ---------------------------------------------------------------------------
async function genEmptySheet() {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet('Orders');
  addHeaderRow(ws, HEADERS);
  // No data rows
  await save(wb, 'empty-sheet.xlsx');
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
(async () => {
  console.log('Generating fixtures...');
  await genValidWeek();
  await genMissingColumns();
  await genExtraColumns();
  await genEmptyRows();
  await genDateAsDate();
  await genMultipleSheets();
  await genEmptySheet();
  console.log('Done.');
})();
