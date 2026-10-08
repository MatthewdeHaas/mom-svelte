import type { Actions } from './$types';
import { fail } from '@sveltejs/kit';
import { generateProductionOrderSheet } from '$lib/excel/export/production-order';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { generatePackingSlips } from '$lib/pdf/packing-slip';
import { parseFormData } from '$lib/server/form';
import { ExportDateSchema } from '$lib/server/schemas/order';
import { callerFromLocals } from '$lib/server/api-client';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function excelResponse(buffer: Buffer, filename: string): Response {
  return new Response(buffer as unknown as BodyInit, {
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

  productionOrder: async ({ request, locals }) => {
    const data = await request.formData();
    const parsed = parseFormData(ExportDateSchema, data);
    if (!parsed.success) return fail(400, { error: parsed.errors.shipment_date ?? 'Invalid or missing shipment date.' });

    const { shipment_date } = parsed.data;
    const caller = callerFromLocals(locals);
    const buffer = await generateProductionOrderSheet(shipment_date, shipment_date, caller);
    return excelResponse(buffer, `production-${shipment_date}.xlsx`);
  },

  shoppingList: async ({ request, locals }) => {
    const data = await request.formData();
    const parsed = parseFormData(ExportDateSchema, data);
    if (!parsed.success) return fail(400, { error: parsed.errors.shipment_date ?? 'Invalid or missing shipment date.' });

    const { shipment_date } = parsed.data;
    const caller = callerFromLocals(locals);
    const buffer = await generateShoppingList(shipment_date, shipment_date, caller);
    return excelResponse(buffer, `shopping-${shipment_date}.xlsx`);
  },

  packingSlips: async ({ request, locals }) => {
    const data = await request.formData();
    const parsed = parseFormData(ExportDateSchema, data);
    if (!parsed.success) return fail(400, { error: parsed.errors.shipment_date ?? 'Invalid or missing shipment date.' });

    const { shipment_date } = parsed.data;
    const caller = callerFromLocals(locals);
    const slips = await generatePackingSlips(new Date(shipment_date), caller);

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
