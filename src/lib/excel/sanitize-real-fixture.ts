/**
 * Sanitize the real client's weekly workbook into two committable fixtures:
 *
 *   tests/integration/excel/fixtures/real-client-week-3.xlsx       (messy, structurally faithful)
 *   tests/integration/excel/fixtures/real-client-week-3-clean.xlsx (same orders, clean template)
 *
 * REBUILD (do not mutate) — Excel workbooks leak PII through at least seven
 * surfaces beyond cell values: sharedStrings.xml from deleted cells, workbook
 * creator/company/lastModifiedBy, sheet comments, named ranges, print
 * headers/footers, docProps/custom.xml. Rebuilding from parsed cell values
 * eliminates all of them.
 *
 * Procedure:
 *   1. Load real workbook from argv[2] (default: ~/Downloads/WS 26...xlsx).
 *   2. For each of the N sheets, extract headers + rows via ws.eachRow
 *      reading cell.value — preserving {formula, result} for formula cells
 *      and Date objects for date cells.
 *   3. Build a fresh ExcelJS.Workbook with zeroed metadata.
 *   4. Re-emit every sheet cell-by-cell. For the Client column (sheet 1
 *      col 10 and sheet N variants) replace alphanumeric chars with 'X'
 *      preserving length + punctuation + whitespace. For sheet 1 r349
 *      (Order Amount = 0 — real client typo) override Order Amount = 1.
 *   5. Write via pinXlsxDates so the fixture bytes are deterministic.
 *   6. Build the clean counterpart: one sheet, 10-column clean template,
 *      Frozen/Chilled 'x' marks collapsed to a single Storage column
 *      ('c' default when neither was marked).
 *
 * Usage:
 *   pnpm exec tsx src/lib/excel/sanitize-real-fixture.ts \
 *     "$HOME/Downloads/WS 26 Master Prod Sheet Week 3 - June 8.xlsx"
 */
import ExcelJS from 'exceljs';
import JSZip from 'jszip';
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { resolve } from 'path';
import { homedir } from 'os';

/**
 * Local copy of the deleted `src/lib/excel/determinism.ts::pinXlsxDates`.
 * Inlined because this script is stand-alone build-time tooling for
 * fixture generation — the request-path code uses Rust's
 * `pin_xlsx_bytes` instead (see `admin-operations-rust/crates/excel/src/determinism.rs`).
 * Keep this in lock-step with `generate-fixtures.ts`'s copy so both
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
const DEFAULT_INPUT = resolve(
  homedir(),
  'Downloads/WS 26 Master Prod Sheet Week 3 - June 8.xlsx',
);

// Column names that may carry PII (person names). Sanitised by replacing
// every alphanumeric char with 'X' while preserving punctuation, spaces,
// and length. "Laura M - Banana, Potato" → "Xxxxx X - Xxxxxx, Xxxxxx".
const PII_HEADERS = new Set(['Client', 'client']);

function sanitizeClient(value: string): string {
  return value
    .split('')
    .map((ch) => {
      if (/[A-Z]/.test(ch)) return 'X';
      if (/[a-z]/.test(ch)) return 'x';
      if (/[0-9]/.test(ch)) return '0';
      return ch; // punctuation, whitespace, hyphens preserved
    })
    .join('');
}

// --- Cell preservation ------------------------------------------------------

type CellSnapshot = { value: ExcelJS.CellValue; numFmt?: string };

function snapshotCell(cell: ExcelJS.Cell): CellSnapshot {
  const snap: CellSnapshot = { value: cell.value };
  if (cell.numFmt) snap.numFmt = cell.numFmt;
  return snap;
}

function emitCell(target: ExcelJS.Cell, snap: CellSnapshot): void {
  target.value = snap.value;
  if (snap.numFmt) target.numFmt = snap.numFmt;
}

// --- Sheet shape ------------------------------------------------------------

interface SheetSnapshot {
  name: string;
  headers: string[]; // first-row cell text, EXACT (preserves " Chilled" leading space)
  rows: CellSnapshot[][];
}

function snapshotSheet(ws: ExcelJS.Worksheet): SheetSnapshot {
  const headerRow = ws.getRow(1);
  const headers: string[] = [];
  const columnCount = ws.actualColumnCount || ws.columnCount || 0;
  for (let c = 1; c <= columnCount; c++) {
    const cell = headerRow.getCell(c);
    const v = cell.value;
    headers.push(typeof v === 'string' ? v : v == null ? '' : String(v));
  }

  const rows: CellSnapshot[][] = [];
  const rowCount = ws.actualRowCount || ws.rowCount || 0;
  for (let r = 2; r <= rowCount; r++) {
    const row = ws.getRow(r);
    const snaps: CellSnapshot[] = [];
    let anyValue = false;
    for (let c = 1; c <= columnCount; c++) {
      const cell = row.getCell(c);
      const snap = snapshotCell(cell);
      if (snap.value !== null && snap.value !== undefined && snap.value !== '') {
        anyValue = true;
      }
      snaps.push(snap);
    }
    if (anyValue) rows.push(snaps);
  }

  return { name: ws.name, headers, rows };
}

function sanitizeSheet(sheet: SheetSnapshot): SheetSnapshot {
  const piiIdx = sheet.headers
    .map((h, i) => (PII_HEADERS.has(h.trim()) ? i : -1))
    .filter((i) => i >= 0);

  const rows = sheet.rows.map((row) => {
    const copy = row.map((c) => ({ ...c }));
    for (const idx of piiIdx) {
      const cell = copy[idx];
      if (typeof cell?.value === 'string' && cell.value.trim()) {
        cell.value = sanitizeClient(cell.value);
      }
    }
    return copy;
  });

  return { ...sheet, rows };
}

/**
 * Label and packing-slip sheets store client names as free-form cell values
 * (not under a Client header), e.g. "Afsar F - Pork, Beef" in sheet 2's
 * label column. The importer only reads sheet 1, so for sheets 2..N we
 * scrub EVERY non-empty string cell — the structural shape (row/col counts,
 * formulas, dates) is what matters for determinism tests, not the strings.
 *
 * Headers are preserved so visual inspection of the fixture still shows
 * the real column labels.
 */
function sanitizeNonDataSheet(sheet: SheetSnapshot): SheetSnapshot {
  const rows = sheet.rows.map((row) =>
    row.map((cell) => {
      if (typeof cell.value === 'string' && cell.value.trim()) {
        return { ...cell, value: sanitizeClient(cell.value) };
      }
      return { ...cell };
    }),
  );
  return { ...sheet, rows };
}

/**
 * Row 349 of the real workbook has Order Amount = 0 (operator typo).
 * The clean fixture needs that row to be importable so downstream export
 * tests get a stable row count. Patch sheet 1 only.
 */
function patchZeroAmount(sheet: SheetSnapshot): SheetSnapshot {
  const headerIdx = sheet.headers.findIndex(
    (h) => h.trim().toLowerCase() === 'order amount',
  );
  if (headerIdx < 0) return sheet;

  const rows = sheet.rows.map((row) => {
    if (row[headerIdx]?.value === 0) {
      const copy = row.map((c) => ({ ...c }));
      copy[headerIdx] = { ...copy[headerIdx], value: 1 };
      return copy;
    }
    return row;
  });

  return { ...sheet, rows };
}

// --- Writer: faithful messy fixture -----------------------------------------

async function writeMessy(sheets: SheetSnapshot[]): Promise<void> {
  const wb = new ExcelJS.Workbook();
  wb.creator = '';
  wb.lastModifiedBy = '';
  wb.company = '';
  wb.title = '';
  wb.description = '';
  wb.created = new Date(0);
  wb.modified = new Date(0);

  for (const sheet of sheets) {
    const ws = wb.addWorksheet(sheet.name);
    // Header row — preserve exact strings (leading-space " Chilled" matters).
    for (let c = 0; c < sheet.headers.length; c++) {
      ws.getCell(1, c + 1).value = sheet.headers[c];
    }
    for (let r = 0; r < sheet.rows.length; r++) {
      const row = sheet.rows[r];
      for (let c = 0; c < row.length; c++) {
        emitCell(ws.getCell(r + 2, c + 1), row[c]);
      }
    }
  }

  const buf = await wb.xlsx.writeBuffer();
  const pinned = await pinXlsxDates(Buffer.from(buf));
  mkdirSync(OUT_DIR, { recursive: true });
  const out = resolve(OUT_DIR, 'real-client-week-3.xlsx');
  writeFileSync(out, pinned);
  console.log(`✓ wrote ${out} (${pinned.length} bytes, ${sheets.length} sheets)`);
}

// --- Writer: clean counterpart (sheet 1 projected into clean template) -------

interface CleanRow {
  customer: string;
  recipe: string;
  serving_date: Date | string | null;
  shipment_date: Date | string | null;
  meal: string;
  amount: number;
  portion_size: number | null;
  day: number | null;
  storage: 'f' | 'c' | '';
  client: string;
}

function projectCleanRows(sheet1: SheetSnapshot): CleanRow[] {
  const h = sheet1.headers.map((x) => x.trim().toLowerCase());
  const col = (name: string) => h.indexOf(name);

  const iCustomer     = col('customer');
  const iRecipe       = col('recipe');
  const iServing      = col('serving date');
  const iShipment     = col('shipment date');
  const iMeal         = col('meal');
  const iAmount       = col('order amount');
  const iPortion      = col('portion size');
  const iDay          = col('day');
  const iFrozen       = col('frozen');
  const iChilled      = col('chilled');
  const iClient       = col('client');

  // Unwrap ExcelJS formula cells (`{formula, result}`) down to the computed
  // result — the real workbook's Portion Size and Qty columns are formulas
  // (`=100*1.3`), and `cell.value` returns the wrapper object, not the number.
  // Without this unwrap the clean fixture ends up with NaN portion_size.
  const unwrap = (v: ExcelJS.CellValue): ExcelJS.CellValue => {
    if (v && typeof v === 'object' && 'formula' in (v as object)) {
      const r = (v as ExcelJS.CellFormulaValue).result;
      if (r == null) return null;
      if (typeof r === 'object' && 'error' in (r as object)) return null;
      return r as ExcelJS.CellValue;
    }
    return v;
  };

  const rows: CleanRow[] = [];
  for (const row of sheet1.rows) {
    const get = (idx: number) => (idx >= 0 ? unwrap(row[idx]?.value ?? null) : null);

    const customer = String(get(iCustomer) ?? '').trim();
    const recipe   = String(get(iRecipe) ?? '').trim();
    // Skip rows that aren't real orders (boilerplate trailers, blanks).
    if (!customer || !recipe) continue;

    const servingRaw  = get(iServing);
    const shipmentRaw = get(iShipment);
    const amountRaw   = get(iAmount);
    const portionRaw  = get(iPortion);
    const dayRaw      = get(iDay);
    const frozenRaw   = get(iFrozen);
    const chilledRaw  = get(iChilled);

    const isX = (v: ExcelJS.CellValue): boolean =>
      typeof v === 'string' && v.trim().toLowerCase() === 'x';

    const storage: 'f' | 'c' = isX(frozenRaw) && !isX(chilledRaw) ? 'f' : 'c';

    rows.push({
      customer,
      recipe,
      serving_date:  servingRaw instanceof Date ? servingRaw : (typeof servingRaw === 'string' ? servingRaw : null),
      shipment_date: shipmentRaw instanceof Date ? shipmentRaw : (typeof shipmentRaw === 'string' ? shipmentRaw : null),
      meal:          String(get(iMeal) ?? '').trim(),
      amount:        typeof amountRaw  === 'number' ? amountRaw  : Number(amountRaw ?? 0),
      portion_size:  typeof portionRaw === 'number' ? portionRaw : (portionRaw == null ? null : Number(portionRaw)),
      day:           typeof dayRaw     === 'number' ? dayRaw     : (dayRaw == null ? null : Number(dayRaw)),
      storage,
      client:        String(get(iClient) ?? ''),
    });
  }
  return rows;
}

async function writeClean(cleanRows: CleanRow[]): Promise<void> {
  const wb = new ExcelJS.Workbook();
  wb.creator = '';
  wb.lastModifiedBy = '';
  wb.company = '';
  wb.title = '';
  wb.description = '';
  wb.created = new Date(0);
  wb.modified = new Date(0);

  const ws = wb.addWorksheet('Orders');
  ws.columns = [
    { header: 'Customer',       key: 'customer',      width: 24 },
    { header: 'Recipe',         key: 'recipe',        width: 32 },
    { header: 'Serving Date',   key: 'serving_date',  width: 14 },
    { header: 'Shipment Date',  key: 'shipment_date', width: 14 },
    { header: 'Meal',           key: 'meal',          width: 12 },
    { header: 'Order Amount',   key: 'amount',        width: 14 },
    { header: 'Portion Size',   key: 'portion_size',  width: 14 },
    { header: 'Day',            key: 'day',           width: 6  },
    { header: 'Storage',        key: 'storage',       width: 10 },
    { header: 'Client',         key: 'client',        width: 32 },
  ];

  for (const row of cleanRows) {
    ws.addRow({
      customer:      row.customer,
      recipe:        row.recipe,
      serving_date:  row.serving_date,
      shipment_date: row.shipment_date,
      meal:          row.meal,
      amount:        row.amount,
      portion_size:  row.portion_size,
      day:           row.day,
      storage:       row.storage,
      client:        row.client,
    });
  }

  const buf = await wb.xlsx.writeBuffer();
  const pinned = await pinXlsxDates(Buffer.from(buf));
  mkdirSync(OUT_DIR, { recursive: true });
  const out = resolve(OUT_DIR, 'real-client-week-3-clean.xlsx');
  writeFileSync(out, pinned);
  console.log(`✓ wrote ${out} (${pinned.length} bytes, ${cleanRows.length} rows)`);
}

// --- Entry point ------------------------------------------------------------

async function main() {
  const input = process.argv[2] ?? DEFAULT_INPUT;
  console.log(`→ reading ${input}`);

  const buf = readFileSync(input);
  const wb = new ExcelJS.Workbook();
  await wb.xlsx.load(buf as unknown as ArrayBuffer);

  const sheets = wb.worksheets.map(snapshotSheet);
  console.log(`  loaded ${sheets.length} sheet${sheets.length === 1 ? '' : 's'}: ${sheets.map((s) => `${s.name}(${s.rows.length}r)`).join(', ')}`);

  // Sheet 1 gets Client-column PII sanitization and the zero-amount patch.
  sheets[0] = patchZeroAmount(sanitizeSheet(sheets[0]));
  // Sheets 2..N are label/packing-slip templates that embed names as
  // free-form cell values. Scrub every string cell wholesale.
  for (let i = 1; i < sheets.length; i++) {
    sheets[i] = sanitizeNonDataSheet(sheets[i]);
  }

  await writeMessy(sheets);
  const cleanRows = projectCleanRows(sheets[0]);
  await writeClean(cleanRows);

  console.log('\nSanitizer finished.');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
