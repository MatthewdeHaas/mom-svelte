import PDFDocument from 'pdfkit';
import fs from 'fs';
import path from 'path';
import type { OrderDetail } from '$lib/types';
import { getOrders } from '$lib/server/queries/orders';
import { getOrderById } from '$lib/server/queries/orders';
import type { Pool } from 'pg';

// ─── Page geometry (Letter) ───────────────────────────────────────────────────

const PAGE_W   = 612;   // pts  (8.5 in)
const PAGE_H   = 792;   // pts  (11 in)
const ML       = 10;    // left margin
const MR       = 10;    // right margin
const MT       = 25;    // top margin
const MB       = 25;    // bottom margin
const CONTENT_W = PAGE_W - ML - MR;

// ─── Logo ─────────────────────────────────────────────────────────────────────

// Adjust this path to wherever the logo lives in your project.
// The file is resolved relative to the project root at runtime.
const LOGO_PATH = path.resolve('static/images/mom-logo.png');
const LOGO_W    = 100;
const LOGO_H    = 50;

// ─── Typography ───────────────────────────────────────────────────────────────

const F = {
  regular: 'Helvetica',
  bold:    'Helvetica-Bold',
} as const;

// ─── Colours (print-neutral) ──────────────────────────────────────────────────

const C = {
  black:       '#000000',
  headerBg:    '#D3D3D3',   // light grey — header row fill
  gridLine:    '#A9A9A9',   // grid / rule lines
  muted:       '#4A4A4A',
} as const;

// ─── Table column definitions ─────────────────────────────────────────────────

type Col = { label: string; width: number; align: 'left' | 'center' | 'right' };

const COLS: Col[] = [
  { label: 'Meal',       width: 230, align: 'left'   },
  { label: 'Category',   width: 90,  align: 'left'   },
  { label: 'Qty',        width: 36,  align: 'center' },
  { label: 'Temp',       width: 58,  align: 'center' },
  { label: 'Note',       width: 178, align: 'left'   },
];

// Total should equal CONTENT_W (592). Tweak if columns change.
// 230+90+36+58+178 = 592 ✓

const ROW_H        = 18;   // pts per data row
const HEADER_ROW_H = 20;   // pts for the column-header row
const CELL_PAD_X   = 4;    // horizontal text inset inside each cell
const CELL_PAD_Y   = 5;    // vertical text inset (from top of cell)
const TABLE_FONT_SIZE = 8;

// ─── Types ────────────────────────────────────────────────────────────────────

type SlipGroup = {
  companyId:       string;
  companyName:     string;
  shippingAddress: string;
  shipmentDate:    string | Date;
  orders:          OrderDetail[];
};

// ─── Date helpers ─────────────────────────────────────────────────────────────

function formatDate(iso: string | Date): string {
  const datePart = (iso instanceof Date ? iso.toISOString() : iso).slice(0, 10);
  const d = new Date(`${datePart}T12:00:00Z`);
  return d.toLocaleDateString('en-CA', {
    weekday: 'long', year: 'numeric', month: 'long', day: 'numeric',
    timeZone: 'UTC',
  });
}

function slugify(str: string): string {
  return str.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
}

/** Week bounds (Mon–Sun) in EST, returned as YYYY-MM-DD strings. */
function weekBoundsEST(date: Date): { from: string; to: string } {
  // Shift to EST (UTC-5 standard; use UTC-5 as a safe constant for week math)
  const est = new Date(date.getTime() - 5 * 60 * 60 * 1000);
  const day = est.getUTCDay(); // 0=Sun
  const diffToMon = day === 0 ? -6 : 1 - day;
  const mon = new Date(est);
  mon.setUTCDate(est.getUTCDate() + diffToMon);
  const sun = new Date(mon);
  sun.setUTCDate(mon.getUTCDate() + 6);
  const fmt = (d: Date) => d.toISOString().slice(0, 10);
  return { from: fmt(mon), to: fmt(sun) };
}

// ─── Low-level drawing helpers ────────────────────────────────────────────────

/** Draw a full-width horizontal rule. */
function hRule(doc: PDFKit.PDFDocument, y: number) {
  doc.save()
    .strokeColor(C.gridLine)
    .lineWidth(0.5)
    .moveTo(ML, y)
    .lineTo(PAGE_W - MR, y)
    .stroke()
    .restore();
}

/** Draw the table grid for a single row (borders only, no fill). */
function drawRowBorder(doc: PDFKit.PDFDocument, y: number, rowH: number) {
  doc.save().strokeColor(C.gridLine).lineWidth(0.5);

  // Outer rect
  doc.rect(ML, y, CONTENT_W, rowH).stroke();

  // Vertical column dividers
  let x = ML;
  for (let i = 0; i < COLS.length - 1; i++) {
    x += COLS[i].width;
    doc.moveTo(x, y).lineTo(x, y + rowH).stroke();
  }

  doc.restore();
}

/** Place text inside a cell, clipped to avoid overflow. */
function cellText(
  doc: PDFKit.PDFDocument,
  text: string,
  cellX: number,
  cellY: number,
  col: Col,
  opts: { bold?: boolean; fontSize?: number } = {}
) {
  const fontSize = opts.fontSize ?? TABLE_FONT_SIZE;
  doc
    .font(opts.bold ? F.bold : F.regular)
    .fontSize(fontSize)
    .fillColor(C.black)
    .text(text, cellX + CELL_PAD_X, cellY + CELL_PAD_Y, {
      width:    col.width - CELL_PAD_X * 2,
      height:   ROW_H - CELL_PAD_Y,
      align:    col.align,
      ellipsis: true,
      lineBreak: false,
    });
}

// ─── Header (logo + title block) ─────────────────────────────────────────────

/**
 * Draws the two-column page header and returns the Y position immediately
 * below it (ready for the next content block).
 */
function drawPageHeader(
  doc: PDFKit.PDFDocument,
  group: SlipGroup,
  hasLogo: boolean
): number {
  const logoColW  = hasLogo ? LOGO_W + 10 : 0;
  const titleColW = CONTENT_W - logoColW;

  // Logo (right-aligned in its column)
  if (hasLogo) {
    const logoX = PAGE_W - MR - LOGO_W;
    doc.image(LOGO_PATH, logoX, MT, { fit: [LOGO_W, LOGO_H] });
  }

  // Title block (left column)
  let y = MT + 4;

  doc.font(F.bold).fontSize(16).fillColor(C.black)
    .text('PACKING SLIP', ML, y, { width: titleColW });
  y += 22;

  doc.font(F.regular).fontSize(10).fillColor(C.black)
    .text(`Delivery Date: ${formatDate(group.shipmentDate)}`, ML, y, { width: titleColW });
  y += 14;

  // Separator
  const sepY = Math.max(y, MT + LOGO_H + 4);
  hRule(doc, sepY);

  return sepY + 8;
}

// ─── Company sub-header ───────────────────────────────────────────────────────

function drawCompanyBlock(
  doc: PDFKit.PDFDocument,
  group: SlipGroup,
  y: number
): number {
  doc.font(F.bold).fontSize(12).fillColor(C.black)
    .text(group.companyName, ML, y, { width: CONTENT_W });
  y = doc.y + 3;

  if (group.shippingAddress?.trim()) {
    doc.font(F.regular).fontSize(9).fillColor(C.muted)
      .text(group.shippingAddress.trim(), ML, y, { width: CONTENT_W });
    y = doc.y + 8;
  }

  return y;
}

// ─── Table ────────────────────────────────────────────────────────────────────

function drawTableHeader(doc: PDFKit.PDFDocument, y: number): number {
  // Grey background for header row
  doc.save()
    .rect(ML, y, CONTENT_W, HEADER_ROW_H)
    .fill(C.headerBg)
    .restore();

  drawRowBorder(doc, y, HEADER_ROW_H);

  let x = ML;
  for (const col of COLS) {
    cellText(doc, col.label.toUpperCase(), x, y, col, { bold: true, fontSize: 8 });
    x += col.width;
  }

  return y + HEADER_ROW_H;
}

function drawOrderRow(
  doc: PDFKit.PDFDocument,
  order: OrderDetail,
  y: number
): number {
  drawRowBorder(doc, y, ROW_H);

  const cells: string[] = [
    order.recipe?.name ?? '—',
    order.meal_category ?? '—',
    String(order.amount),
    order.frozen ? 'Frozen' : 'Chilled',
    order.client_note?.trim() || '',
  ];

  let x = ML;
  cells.forEach((text, i) => {
    cellText(doc, text, x, y, COLS[i]);
    x += COLS[i].width;
  });

  return y + ROW_H;
}

// ─── Footer ───────────────────────────────────────────────────────────────────

/**
 * Stamps "Page N of T" on every page of a finished doc buffer.
 * PDFKit doesn't support "total pages" natively, so we render once to get
 * the page count, then re-render with the count injected.
 *
 * Since we can't do a true two-pass with PDFKit streams, we instead use a
 * well-known workaround: embed a placeholder string and patch it via
 * Buffer.replace once we know the total. This is safe as long as the
 * placeholder is unique and the same byte length as the final string.
 *
 * For robustness we instead just render the footer as "Page N" and rely on
 * the total-pages stamp being added in a second PDFDocument pass using
 * the page-count we derive from counting the placeholder occurrences.
 */

const FOOTER_PLACEHOLDER = '##TOTAL##';
const FOOTER_PLACEHOLDER_LEN = FOOTER_PLACEHOLDER.length; // 9 chars

/**
 * Draw "Page N of ##TOTAL##" footer on the current page.
 * The placeholder is later patched with the real total.
 */
function drawPageFooter(doc: PDFKit.PDFDocument, pageNum: number) {
  const text = `Page ${pageNum} of ${FOOTER_PLACEHOLDER}`;
  const footerY = PAGE_H - MB + 6;

  doc.save()
    .font(F.regular)
    .fontSize(9)
    .fillColor(C.muted)
    .text(text, ML, footerY, { width: CONTENT_W, align: 'right' })
    .restore();
}

/**
 * Patch every occurrence of the placeholder in the raw PDF buffer
 * with the zero-padded real total (same byte length to avoid shifting offsets).
 */
function patchPageTotal(buf: Buffer, totalPages: number): Buffer {
  const placeholder = Buffer.from(FOOTER_PLACEHOLDER);
  // Zero-pad so replacement is the same length as placeholder (9 chars)
  const replacement = Buffer.from(String(totalPages).padStart(FOOTER_PLACEHOLDER_LEN, ' '));

  let result = buf;
  let idx = result.indexOf(placeholder);
  while (idx !== -1) {
    replacement.copy(result, idx);
    idx = result.indexOf(placeholder, idx + 1);
  }
  return result;
}

// ─── Signature line ───────────────────────────────────────────────────────────

function drawSignatureLine(doc: PDFKit.PDFDocument, y: number): number {
  y += 12;
  doc.font(F.regular).fontSize(10).fillColor(C.black)
    .text(
      'Packed By: ________________________   Checked By: ________________________',
      ML, y, { width: CONTENT_W }
    );
  return doc.y + 8;
}

// ─── Single slip renderer ─────────────────────────────────────────────────────

function renderSlip(
  doc: PDFKit.PDFDocument,
  group: SlipGroup,
  isFirst: boolean,
  pageCounter: { n: number },
  hasLogo: boolean
) {
  if (!isFirst) doc.addPage();

  drawPageFooter(doc, pageCounter.n++);

  let y = drawPageHeader(doc, group, hasLogo);
  y = drawCompanyBlock(doc, group, y);
  y += 6;
  y = drawTableHeader(doc, y);

  const bottomLimit = PAGE_H - MB - 30; // leave room for footer + sig line

  for (const order of group.orders) {
    // Start a new page if this row won't fit
    if (y + ROW_H > bottomLimit) {
      doc.addPage();
      drawPageFooter(doc, pageCounter.n++);
      y = drawPageHeader(doc, group, hasLogo);
      y = drawTableHeader(doc, y);
    }

    y = drawOrderRow(doc, order, y);
  }

  // Bottom border of last row is already drawn by drawOrderRow.
  // Draw a slightly heavier bottom rule to close the table.
  doc.save()
    .strokeColor(C.gridLine)
    .lineWidth(1)
    .moveTo(ML, y)
    .lineTo(PAGE_W - MR, y)
    .stroke()
    .restore();

  drawSignatureLine(doc, y);
}

// ─── Main export ──────────────────────────────────────────────────────────────

/**
 * Generates one PDF per unique company × shipment-date pair within the week
 * containing `referenceDate` (week bounds computed in EST).
 *
 * Returns an array of `{ filename, buffer }` tuples ready to be served or
 * zipped by the calling route.
 */
export async function generatePackingSlips(
  db: Pool,
  referenceDate: Date
): Promise<Array<{ filename: string; companyName: string; buffer: Buffer }>> {
  const { from, to } = weekBoundsEST(referenceDate);

  // 1. Fetch all orders for the week
  const orders = await getOrders(db, {
    serving_date_from: from,
    serving_date_to:   to,
  });

  // 2. Fetch full OrderDetail for each order (in parallel)
  const details = (
    await Promise.all(orders.map((o) => getOrderById(db, o.id)))
  ).filter(Boolean) as OrderDetail[];

  // 3. Group by company × shipment date
  const groupMap = new Map<string, SlipGroup>();

  for (const detail of details) {
    const key = `${detail.company.id}::${detail.shipment_date}`;

    if (!groupMap.has(key)) {
      groupMap.set(key, {
        companyId:       detail.company.id,
        companyName:     detail.company.name,
        shippingAddress: detail.company.shipping_address ?? '',
        shipmentDate:    detail.shipment_date,
        orders:          [],
      });
    }

    groupMap.get(key)!.orders.push(detail);
  }

  // 4. Check logo availability once
  const hasLogo = fs.existsSync(LOGO_PATH);

  // 5. Render one PDF per group
	const results: Array<{ filename: string; companyName: string; buffer: Buffer }> = [];

  for (const group of groupMap.values()) {
    const rawBuf = await new Promise<Buffer>((resolve, reject) => {
      const doc = new PDFDocument({
        size:    [PAGE_W, PAGE_H],
        margin:  0,
        info: {
          Title:   `Packing Slip — ${group.companyName}`,
          Subject: `Delivery ${group.shipmentDate}`,
        },
        // Disable auto first-page so we control it in renderSlip
        autoFirstPage: false,
      });

      const chunks: Buffer[] = [];
      doc.on('data',  (c) => chunks.push(c));
      doc.on('end',   () => resolve(Buffer.concat(chunks)));
      doc.on('error', reject);

      // Add the first page manually so renderSlip's isFirst=true path works
      doc.addPage();

      const pageCounter = { n: 1 };
      renderSlip(doc, group, true, pageCounter, hasLogo);
      doc.end();
    });

    // Patch page totals into the buffer
    const totalPages = rawBuf.toString('latin1').split('Page ').length - 1;
    // More reliable: count placeholder occurrences in the raw buffer
    const placeholder = Buffer.from(FOOTER_PLACEHOLDER);
    let count = 0;
    let idx = rawBuf.indexOf(placeholder);
    while (idx !== -1) {
      count++;
      idx = rawBuf.indexOf(placeholder, idx + 1);
    }

    const finalBuf = patchPageTotal(rawBuf, count);

		results.push({
			filename:    `packing-slip-${group.shipmentDate}.pdf`,
			companyName: group.companyName,
			buffer:      finalBuf,
		});
  }

  return results;
}
