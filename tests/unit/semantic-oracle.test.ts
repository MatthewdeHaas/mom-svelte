// Negative-control self-tests for the semantic oracle.
//
// The oracle lives in tests/integration/excel/_semantic.ts; these unit tests
// confirm that:
//   (a) hashing the same buffer twice yields the identical hash;
//   (b) mutations we EXPLICITLY DROP (workbook metadata, PDF CreationDate)
//       do NOT flip the hash — proving the drop is in effect;
//   (c) a logical content mutation (one xlsx cell, one PDF text run) DOES
//       flip the hash — proving the oracle isn't a tautology.
//
// Without (c), a future over-normalization could silently turn the oracle
// into a no-op that passes everything. Keep this file green.

import { describe, it, expect } from 'vitest';
import ExcelJS from 'exceljs';
import PDFDocument from 'pdfkit';
import {
  semanticHashXlsx,
  semanticHashPdf,
} from '../integration/excel/_semantic';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function buildWorkbook(opts: {
  cellA2?: string;
  creator?: string;
}): Promise<Buffer> {
  const wb = new ExcelJS.Workbook();
  wb.creator = opts.creator ?? 'original-creator';
  wb.created = new Date(0);
  wb.modified = new Date(0);
  const ws = wb.addWorksheet('Sheet1');
  ws.columns = [
    { header: 'Name', key: 'name', width: 20 },
    { header: 'Qty', key: 'qty', width: 10 },
  ];
  ws.addRow({ name: opts.cellA2 ?? 'A', qty: 1 });
  ws.addRow({ name: 'B', qty: 2 });
  const out = await wb.xlsx.writeBuffer();
  return Buffer.from(out as ArrayBuffer);
}

interface PdfOpts {
  text?: string;
  creationDate?: Date;
}

function buildPdf(opts: PdfOpts): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const doc = new PDFDocument({
      size: [612, 792],
      margin: 50,
      compress: false,
      autoFirstPage: false,
      info: {
        Title: 'Oracle Test',
        Subject: 'Negative control',
        Producer: 'motm-admin',
        Creator: 'motm-admin',
        CreationDate: opts.creationDate ?? new Date(0),
        ModDate: new Date(0),
      },
    });
    const chunks: Buffer[] = [];
    doc.on('data', (c) => chunks.push(c));
    doc.on('end', () => resolve(Buffer.concat(chunks)));
    doc.on('error', reject);

    doc.addPage();
    doc.fontSize(12).text(opts.text ?? 'Hello world');
    doc.end();
  });
}

// ---------------------------------------------------------------------------
// XLSX
// ---------------------------------------------------------------------------

describe('semanticHashXlsx', () => {
  it('is self-consistent — same buffer hashes identically', async () => {
    const buf = await buildWorkbook({});
    const h1 = await semanticHashXlsx(buf);
    const h2 = await semanticHashXlsx(buf);
    expect(h1).toBe(h2);
  });

  it('ignores workbook.creator (dropped metadata)', async () => {
    const a = await buildWorkbook({ creator: 'alice' });
    const b = await buildWorkbook({ creator: 'bob' });
    const ha = await semanticHashXlsx(a);
    const hb = await semanticHashXlsx(b);
    expect(ha).toBe(hb);
  });

  it('detects a single-cell content mutation', async () => {
    const a = await buildWorkbook({ cellA2: 'A' });
    const b = await buildWorkbook({ cellA2: 'B' });
    const ha = await semanticHashXlsx(a);
    const hb = await semanticHashXlsx(b);
    expect(ha).not.toBe(hb);
  });
});

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

describe('semanticHashPdf', () => {
  it('is self-consistent — same buffer hashes identically', async () => {
    const buf = await buildPdf({});
    const h1 = semanticHashPdf(buf);
    const h2 = semanticHashPdf(buf);
    expect(h1).toBe(h2);
  });

  it('ignores CreationDate drift (dropped from info dict)', async () => {
    const a = await buildPdf({ creationDate: new Date(0) });
    const b = await buildPdf({ creationDate: new Date(2_000_000_000_000) });
    const ha = semanticHashPdf(a);
    const hb = semanticHashPdf(b);
    expect(ha).toBe(hb);
  });

  it('detects a single-run text mutation', async () => {
    const a = await buildPdf({ text: 'Hello world' });
    const b = await buildPdf({ text: 'Hello WORLD' });
    const ha = semanticHashPdf(a);
    const hb = semanticHashPdf(b);
    expect(ha).not.toBe(hb);
  });
});
