import type { Actions } from './$types';
import { fail } from '@sveltejs/kit';
import { pool } from '$lib/server/db';
import { generateProductionOrderSheet } from '$lib/excel/export/production-order';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { generatePackingSlips } from '$lib/pdf/packing-slip';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function requireDate(data: FormData, field: string): string | null {
  const val = data.get(field);
  if (typeof val !== 'string' || !val.match(/^\d{4}-\d{2}-\d{2}$/)) return null;
  return val;
}

function excelResponse(buffer: ExcelJS.Buffer, filename: string): Response {
  return new Response(buffer as ArrayBuffer, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': `attachment; filename="${filename}"`,
    },
  });
}

function pdfResponse(buffer: Buffer, filename: string): Response {
  return new Response(buffer, {
    headers: {
      'Content-Type': 'application/pdf',
      'Content-Disposition': `attachment; filename="${filename}"`,
    },
  });
}

// ─── Actions ──────────────────────────────────────────────────────────────────

export const actions: Actions = {

  productionOrder: async ({ request }) => {
    const data = await request.formData();
    const shipmentDate = requireDate(data, 'shipment_date');
    if (!shipmentDate) return fail(400, { error: 'Invalid or missing shipment date.' });

    const buffer = await generateProductionOrderSheet(pool, shipmentDate, shipmentDate);
    return excelResponse(buffer, `production-${shipmentDate}.xlsx`);
  },

  shoppingList: async ({ request }) => {
    const data = await request.formData();
    const shipmentDate = requireDate(data, 'shipment_date');
    if (!shipmentDate) return fail(400, { error: 'Invalid or missing shipment date.' });

    const buffer = await generateShoppingList(pool, shipmentDate, shipmentDate);
    return excelResponse(buffer, `shopping-${shipmentDate}.xlsx`);
  },

  packingSlips: async ({ request }) => {
    const data = await request.formData();
    const dateParam = requireDate(data, 'shipment_date');
    if (!dateParam) return fail(400, { error: 'Invalid or missing shipment date.' });

    const slips = await generatePackingSlips(pool, new Date(dateParam));

    if (slips.length === 0) {
      return fail(404, { error: 'No orders found for that week.' });
    }

    if (slips.length === 1) {
      // Single slip — serve directly
      return pdfResponse(slips[0].buffer, slips[0].filename);
    }

    // Multiple slips — zip them
    const { default: JSZip } = await import('jszip');
    const zip = new JSZip();
    for (const slip of slips) {
      zip.file(slip.filename, slip.buffer);
    }
    const zipBuffer = await zip.generateAsync({ type: 'nodebuffer' });
    const weekLabel = dateParam; // YYYY-MM-DD of the reference date

    return new Response(zipBuffer, {
      headers: {
        'Content-Type': 'application/zip',
        'Content-Disposition': `attachment; filename="packing-slips-${weekLabel}.zip"`,
      },
    });
  },

};
