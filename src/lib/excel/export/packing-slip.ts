import PDFDocument from 'pdfkit';
import type { OrderDetail } from '$lib/types';
import { getOrders, getOrderById } from '$lib/server/queries';
import type { Pool } from 'pg';

// ─── Constants ───────────────────────────────────────────────────────────────

const MARGIN = 48;
const PAGE_WIDTH = 595.28; // A4
const CONTENT_WIDTH = PAGE_WIDTH - MARGIN * 2;

const COLORS = {
  brand:      '#2E75B6',
  text:       '#1A1A1A',
  muted:      '#6B7280',
  rule:       '#E5E7EB',
  checkmark:  '#16A34A',
  noteLabel:  '#92400E',
  noteBg:     '#FFFBEB',
};

const FONTS = {
  regular: 'Helvetica',
  bold:    'Helvetica-Bold',
};

// ─── Types ────────────────────────────────────────────────────────────────────

type SlipGroup = {
  companyId: number;
  companyName: string;
  shippingAddress: string;
  shipmentDate: string;        // ISO date string e.g. '2024-03-01'
  orders: OrderDetail[];
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-GB', {
    day: '2-digit', month: 'long', year: 'numeric',
  });
}

function slugify(str: string): string {
  return str.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
}

/** ISO week date range: Mon–Sun */
function weekBounds(date: Date): { from: string; to: string } {
  const d = new Date(date);
  const day = d.getDay();
  const diffToMon = (day === 0 ? -6 : 1 - day);
  const mon = new Date(d);
  mon.setDate(d.getDate() + diffToMon);
  const sun = new Date(mon);
  sun.setDate(mon.getDate() + 6);
  const fmt = (dt: Date) => dt.toISOString().slice(0, 10);
  return { from: fmt(mon), to: fmt(sun) };
}

// ─── PDF drawing helpers ──────────────────────────────────────────────────────

function rule(doc: PDFKit.PDFDocument, y: number, color = COLORS.rule) {
  doc.save()
    .strokeColor(color)
    .lineWidth(0.5)
    .moveTo(MARGIN, y)
    .lineTo(PAGE_WIDTH - MARGIN, y)
    .stroke()
    .restore();
}

function ensureSpace(doc: PDFKit.PDFDocument, needed: number) {
  const bottom = doc.page.height - MARGIN;
  if (doc.y + needed > bottom) doc.addPage();
}

// ─── Slip renderer ────────────────────────────────────────────────────────────

function renderSlip(doc: PDFKit.PDFDocument, group: SlipGroup, isFirst: boolean) {
  if (!isFirst) doc.addPage();

  let y = MARGIN;

  // ── Header bar ──────────────────────────────────────────────────────────────
  doc.save()
    .rect(0, 0, PAGE_WIDTH, 56)
    .fill(COLORS.brand)
    .restore();

  doc.font(FONTS.bold)
    .fontSize(16)
    .fillColor('#FFFFFF')
    .text('Packing Slip', MARGIN, 18, { continued: true })
    .font(FONTS.regular)
    .fontSize(11)
    .text(`  ·  ${formatDate(group.shipmentDate)}`, { align: 'left' });

  y = 72;

  // ── Company block ────────────────────────────────────────────────────────────
  doc.font(FONTS.bold).fontSize(13).fillColor(COLORS.text)
    .text(group.companyName, MARGIN, y);
  y = doc.y + 4;

  doc.font(FONTS.regular).fontSize(10).fillColor(COLORS.muted)
    .text(group.shippingAddress, MARGIN, y, { width: CONTENT_WIDTH });
  y = doc.y + 16;

  rule(doc, y);
  y += 12;

  // ── Orders ───────────────────────────────────────────────────────────────────
  for (const order of group.orders) {
    // Estimate height needed for this order block (rough)
    const hasNote = Boolean(order.client_note?.trim());
    const blockHeight = hasNote ? 92 : 68;
    ensureSpace(doc, blockHeight);

    y = doc.y + 4;

    // Recipe name
    doc.font(FONTS.bold).fontSize(12).fillColor(COLORS.text)
      .text(order.recipe.name, MARGIN, y, { width: CONTENT_WIDTH - 80 });

    // Frozen / Chilled badge — top-right of recipe line
    const badge = order.recipe.frozen ? 'FROZEN' : 'CHILLED';
    const badgeColor = order.recipe.frozen ? '#1D4ED8' : '#0F766E';
    doc.save()
      .roundedRect(PAGE_WIDTH - MARGIN - 60, y - 1, 60, 16, 3)
      .fill(badgeColor)
      .restore();
    doc.font(FONTS.bold).fontSize(8).fillColor('#FFFFFF')
      .text(badge, PAGE_WIDTH - MARGIN - 58, y + 2, { width: 56, align: 'center' });

    y = doc.y + 4;

    // Meal category + amount row
    doc.font(FONTS.regular).fontSize(10).fillColor(COLORS.muted)
      .text(order.recipe.meal_category, MARGIN, y, { continued: true })
      .fillColor(COLORS.text)
      .text(`   ·   Qty: `, { continued: true })
      .font(FONTS.bold)
      .text(String(order.amount));

    y = doc.y + 6;

    // Client note
    if (hasNote) {
      ensureSpace(doc, 32);
      const noteY = doc.y;
      const noteText = order.client_note!.trim();

      doc.save()
        .rect(MARGIN, noteY, CONTENT_WIDTH, 22)
        .fill(COLORS.noteBg)
        .restore();

      doc.font(FONTS.bold).fontSize(8).fillColor(COLORS.noteLabel)
        .text('NOTE  ', MARGIN + 6, noteY + 6, { continued: true })
        .font(FONTS.regular).fillColor(COLORS.text)
        .text(noteText, { width: CONTENT_WIDTH - 12 });

      y = doc.y + 8;
    }

    rule(doc, doc.y + 4, COLORS.rule);
    doc.moveDown(0.8);
  }
}

// ─── Main export ──────────────────────────────────────────────────────────────

/**
 * Generates one PDF per unique company × shipment date pair within the week
 * containing `referenceDate`. Returns an array of { filename, buffer } tuples.
 */
export async function generatePackingSlips(
  db: pool,
  referenceDate: Date
): Promise<Array<{ filename: string; buffer: Buffer }>> {
  const { from, to } = weekBounds(referenceDate);

  // 1. Fetch all orders for the week
  const orders = await getOrders(db, {
    serving_date_from: from,
    serving_date_to: to,
  });

  // 2. Fetch full OrderDetail for each order
  const details = await Promise.all(
    orders.map((o) => getOrderById(db, o.id))
  );

  const validDetails = details.filter(Boolean) as OrderDetail[];

  // 3. Group by company × shipment date
  const groupMap = new Map<string, SlipGroup>();

  for (const detail of validDetails) {
    const key = `${detail.company.id}::${detail.shipment_date}`;

    if (!groupMap.has(key)) {
      groupMap.set(key, {
        companyId:       detail.company.id,
        companyName:     detail.company.name,
        shippingAddress: detail.company.shipping_address,
        shipmentDate:    detail.shipment_date,
        orders:          [],
      });
    }

    groupMap.get(key)!.orders.push(detail);
  }

  // 4. Render one PDF per group
  const results: Array<{ filename: string; buffer: Buffer }> = [];

  for (const group of groupMap.values()) {
    const buffer = await new Promise<Buffer>((resolve, reject) => {
      const doc = new PDFDocument({ margin: 0, size: 'A4' });
      const chunks: Buffer[] = [];

      doc.on('data', (chunk) => chunks.push(chunk));
      doc.on('end', () => resolve(Buffer.concat(chunks)));
      doc.on('error', reject);

      renderSlip(doc, group, true);
      doc.end();
    });

    const dateSlug = group.shipmentDate; // already YYYY-MM-DD
    const companySlug = slugify(group.companyName);
    results.push({
      filename: `packing-slip-${companySlug}-${dateSlug}.pdf`,
      buffer,
    });
  }

  return results;
}
