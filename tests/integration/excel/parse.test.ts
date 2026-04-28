import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';
import { parseExcelBufferAsync } from '$lib/excel/parse';

const FIXTURE_DIR = 'tests/integration/excel/fixtures';

function load(name: string): Buffer {
  return fs.readFileSync(path.join(FIXTURE_DIR, name));
}

// ---------------------------------------------------------------------------
// valid-week.xlsx
// ---------------------------------------------------------------------------
describe('valid-week.xlsx', () => {
  it('returns exactly 3 rows', async () => {
    const rows = await parseExcelBufferAsync(load('valid-week.xlsx'));
    expect(rows).toHaveLength(3);
  });

  it('preserves header strings exactly as written in the sheet', async () => {
    const rows = await parseExcelBufferAsync(load('valid-week.xlsx'));
    const keys = Object.keys(rows[0]);
    expect(keys).toContain('Company');
    expect(keys).toContain('Recipe');
    expect(keys).toContain('Serving Date');
    expect(keys).toContain('Shipment Date');
    expect(keys).toContain('Meal Category');
    expect(keys).toContain('Amount');
    expect(keys).toContain('Storage');
    expect(keys).toContain('Notes');
  });

  it('coerces Amount to a number', async () => {
    const rows = await parseExcelBufferAsync(load('valid-week.xlsx'));
    expect(typeof rows[0]['Amount']).toBe('number');
    expect(rows[0]['Amount']).toBe(10);
  });

  it('coerces string columns to strings', async () => {
    const rows = await parseExcelBufferAsync(load('valid-week.xlsx'));
    expect(typeof rows[0]['Company']).toBe('string');
    expect(rows[0]['Company']).toBe('Acme Corp');
    expect(rows[0]['Storage']).toBe('frozen');
  });

  it('maps null Notes to null', async () => {
    const rows = await parseExcelBufferAsync(load('valid-week.xlsx'));
    expect(rows[0]['Notes']).toBeNull();
  });

  it('maps non-null Notes to a string', async () => {
    const rows = await parseExcelBufferAsync(load('valid-week.xlsx'));
    // Row 2 has 'gluten free' as Notes
    expect(typeof rows[1]['Notes']).toBe('string');
    expect(rows[1]['Notes']).toBe('gluten free');
  });
});

// ---------------------------------------------------------------------------
// date-as-date.xlsx  (and implicitly valid-week.xlsx)
// ---------------------------------------------------------------------------
describe('date-as-date.xlsx', () => {
  it('converts Serving Date cell to a YYYY-MM-DD string', async () => {
    const rows = await parseExcelBufferAsync(load('date-as-date.xlsx'));
    expect(rows[0]['Serving Date']).toBe('2025-06-15');
  });

  it('converts Shipment Date cell to a YYYY-MM-DD string', async () => {
    const rows = await parseExcelBufferAsync(load('date-as-date.xlsx'));
    expect(rows[0]['Shipment Date']).toBe('2025-06-10');
  });

  it('does not return a Date object for date columns', async () => {
    const rows = await parseExcelBufferAsync(load('date-as-date.xlsx'));
    expect(rows[0]['Serving Date']).not.toBeInstanceOf(Date);
    expect(rows[0]['Shipment Date']).not.toBeInstanceOf(Date);
  });

  it('date strings match /^\\d{4}-\\d{2}-\\d{2}$/', async () => {
    const rows = await parseExcelBufferAsync(load('date-as-date.xlsx'));
    expect(rows[0]['Serving Date']).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(rows[0]['Shipment Date']).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

// ---------------------------------------------------------------------------
// empty-rows.xlsx
// ---------------------------------------------------------------------------
describe('empty-rows.xlsx', () => {
  it('filters out blank rows — returns only 2 data rows', async () => {
    const rows = await parseExcelBufferAsync(load('empty-rows.xlsx'));
    expect(rows).toHaveLength(2);
  });

  it('first row is the first valid data row', async () => {
    const rows = await parseExcelBufferAsync(load('empty-rows.xlsx'));
    expect(rows[0]['Company']).toBe('Acme Corp');
  });

  it('second row is the second valid data row', async () => {
    const rows = await parseExcelBufferAsync(load('empty-rows.xlsx'));
    expect(rows[1]['Company']).toBe('Beta LLC');
  });
});

// ---------------------------------------------------------------------------
// missing-columns.xlsx
// ---------------------------------------------------------------------------
describe('missing-columns.xlsx', () => {
  it('returns rows even when required columns are absent', async () => {
    const rows = await parseExcelBufferAsync(load('missing-columns.xlsx'));
    expect(rows).toHaveLength(1);
  });

  it('only contains the columns that are present in the sheet', async () => {
    const rows = await parseExcelBufferAsync(load('missing-columns.xlsx'));
    const keys = Object.keys(rows[0]);
    expect(keys).toEqual(['Company', 'Recipe']);
  });

  it('does not contain headers that were not in the sheet', async () => {
    const rows = await parseExcelBufferAsync(load('missing-columns.xlsx'));
    expect(rows[0]).not.toHaveProperty('Serving Date');
    expect(rows[0]).not.toHaveProperty('Amount');
  });
});

// ---------------------------------------------------------------------------
// extra-columns.xlsx
// ---------------------------------------------------------------------------
describe('extra-columns.xlsx', () => {
  it('includes extra columns in output without throwing', async () => {
    const rows = await parseExcelBufferAsync(load('extra-columns.xlsx'));
    expect(rows).toHaveLength(1);
  });

  it('preserves the extra column headers exactly', async () => {
    const rows = await parseExcelBufferAsync(load('extra-columns.xlsx'));
    expect(rows[0]).toHaveProperty('Internal Code', 'IC-001');
    expect(rows[0]).toHaveProperty('Priority', 'High');
  });

  it('still contains the standard columns', async () => {
    const rows = await parseExcelBufferAsync(load('extra-columns.xlsx'));
    expect(rows[0]).toHaveProperty('Company', 'Acme Corp');
    expect(rows[0]).toHaveProperty('Amount', 10);
  });
});

// ---------------------------------------------------------------------------
// multiple-sheets.xlsx
// ---------------------------------------------------------------------------
describe('multiple-sheets.xlsx', () => {
  it('returns only rows from the first sheet', async () => {
    const rows = await parseExcelBufferAsync(load('multiple-sheets.xlsx'));
    expect(rows).toHaveLength(1);
  });

  it('the single row is from sheet one', async () => {
    const rows = await parseExcelBufferAsync(load('multiple-sheets.xlsx'));
    expect(rows[0]['Company']).toBe('Acme Corp');
  });

  it('does not contain data from sheet two', async () => {
    const rows = await parseExcelBufferAsync(load('multiple-sheets.xlsx'));
    const companies = rows.map((r) => r['Company']);
    expect(companies).not.toContain('Should Not Appear');
  });
});

// ---------------------------------------------------------------------------
// empty-sheet.xlsx
// ---------------------------------------------------------------------------
describe('empty-sheet.xlsx', () => {
  it('returns an empty array when there are no data rows', async () => {
    const rows = await parseExcelBufferAsync(load('empty-sheet.xlsx'));
    expect(rows).toEqual([]);
  });

  it('does not throw for an empty data sheet', async () => {
    await expect(
      parseExcelBufferAsync(load('empty-sheet.xlsx')),
    ).resolves.not.toThrow();
  });
});
