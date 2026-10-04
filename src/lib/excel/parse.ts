import ExcelJS from 'exceljs';

function toYMD(date: Date): string {
  const y = date.getUTCFullYear();
  const m = String(date.getUTCMonth() + 1).padStart(2, '0');
  const d = String(date.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

function coerceCell(value: ExcelJS.CellValue): string | number | null {
  if (value === null || value === undefined || value === '') return null;
  if (value instanceof Date) return toYMD(value);
  if (typeof value === 'number') return value;
  if (typeof value === 'string') return value;
  // RichText object
  if (typeof value === 'object' && 'richText' in (value as object)) {
    return (value as ExcelJS.CellRichTextValue).richText.map((r) => r.text).join('');
  }
  return String(value);
}

export function parseExcelBuffer(buffer: ArrayBuffer | Buffer): Record<string, unknown>[] {
  const workbook = new ExcelJS.Workbook();

  const data = buffer;

  throw new Error(
    'Use parseExcelBufferAsync instead — ExcelJS requires async parsing.',
  );
}

/**
 * Parses an ExcelJS workbook buffer and returns one record per data row.
 *
 * - Only the first sheet is read.
 * - Row 1 is treated as the header row.
 * - Fully empty rows are filtered out.
 * - Date cells are converted to YYYY-MM-DD strings via UTC methods.
 * - Headers are preserved exactly as they appear in the sheet.
 */
export async function parseExcelBufferAsync(
  buffer: ArrayBuffer | Buffer,
): Promise<Record<string, unknown>[]> {
  const { rows } = await parseExcelBufferAsyncWithMeta(buffer);
  return rows;
}

/**
 * Same as `parseExcelBufferAsync` but also returns workbook-level metadata
 * the importer needs to emit sheet-level warnings (notably: how many
 * non-data sheets were present and ignored).
 *
 * Only sheet 1 is read for data — the real client's weekly workbook has
 * 7 sheets but only sheet 1 is the production order; sheets 2–7 are
 * packing-slip templates and office layouts. `extraSheetCount` lets the
 * route surface a yellow `extra_sheets_ignored` warning so the operator
 * knows the trailing sheets were skipped on purpose.
 */
export async function parseExcelBufferAsyncWithMeta(
  buffer: ArrayBuffer | Buffer,
): Promise<{ rows: Record<string, unknown>[]; extraSheetCount: number; headers: string[] }> {
  const workbook = new ExcelJS.Workbook();
  // ExcelJS types declare load(buffer: Buffer) using the legacy non-generic Buffer.
  // Node 22+ types make Buffer<ArrayBufferLike> — cast away the mismatch.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await workbook.xlsx.load(buffer as any);

  const sheet = workbook.worksheets[0];
  if (!sheet) return { rows: [], extraSheetCount: 0, headers: [] };

  const extraSheetCount = Math.max(0, workbook.worksheets.length - 1);

  // Extract headers from row 1
  const headerRow = sheet.getRow(1);
  const headers: string[] = [];
  headerRow.eachCell({ includeEmpty: true }, (cell, colNumber) => {
    headers[colNumber - 1] = coerceCell(cell.value) as string ?? '';
  });

  const rows: Record<string, unknown>[] = [];

  sheet.eachRow((row, rowNumber) => {
    if (rowNumber === 1) return; // skip header

    const values: unknown[] = [];
    let hasValue = false;

    row.eachCell({ includeEmpty: true }, (cell, colNumber) => {
      const coerced = coerceCell(cell.value);
      values[colNumber - 1] = coerced;
      if (coerced !== null) hasValue = true;
    });

    // Filter fully empty rows
    if (!hasValue) return;

    const record: Record<string, unknown> = {};
    headers.forEach((header, i) => {
      if (header) record[header] = values[i] ?? null;
    });

    rows.push(record);
  });

  return { rows, extraSheetCount, headers };
}
