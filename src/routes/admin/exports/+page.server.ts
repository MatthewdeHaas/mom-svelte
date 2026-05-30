import type { Actions } from './$types';
import { fail } from '@sveltejs/kit';
import { pool } from '$lib/server/db';
import { generateProductionOrderSheet } from '$lib/excel/export/production-order';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { generatePackingSlips } from '$lib/pdf/packing-slip';
import { parseFormData } from '$lib/server/form';
import { ExportDateSchema } from '$lib/server/schemas/order';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function excelResponse(buffer: ArrayBuffer, filename: string): Response {
  return new Response(buffer, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': `attachment; filename="${filename}"`,
    },
  });
}

function pdfResponse(buffer: Buffer, filename: string): Response {
  return new Response(buffer as unknown as BodyInit, {
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
    const parsed = parseFormData(ExportDateSchema, data);
    if (!parsed.success) return fail(400, { error: parsed.errors.shipment_date ?? 'Invalid or missing shipment date.' });

    const { shipment_date } = parsed.data;
    const buffer = await generateProductionOrderSheet(pool, shipment_date, shipment_date);
    return excelResponse(buffer, `production-${shipment_date}.xlsx`);
  },

  shoppingList: async ({ request }) => {
    const data = await request.formData();
    const parsed = parseFormData(ExportDateSchema, data);
    if (!parsed.success) return fail(400, { error: parsed.errors.shipment_date ?? 'Invalid or missing shipment date.' });

    const { shipment_date } = parsed.data;
    const buffer = await generateShoppingList(pool, shipment_date, shipment_date);
    return excelResponse(buffer, `shopping-${shipment_date}.xlsx`);
  },

  packingSlips: async ({ request }) => {
    const data = await request.formData();
    const parsed = parseFormData(ExportDateSchema, data);
    if (!parsed.success) return fail(400, { error: parsed.errors.shipment_date ?? 'Invalid or missing shipment date.' });

    const { shipment_date } = parsed.data;
    const slips = await generatePackingSlips(pool, new Date(shipment_date));

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

    return new Response(zipBuffer as unknown as BodyInit, {
      headers: {
        'Content-Type': 'application/zip',
        'Content-Disposition': `attachment; filename="packing-slips-${shipment_date}.zip"`,
      },
    });
  },

};
