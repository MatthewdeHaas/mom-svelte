import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { generatePackingSlips } from '$lib/excel/export/shopping-list';
import { error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url }) => {
  const shipmentDate = url.searchParams.get('shipment_date');
  if (!shipmentDate?.match(/^\d{4}-\d{2}-\d{2}$/)) {
    throw error(400, 'Invalid or missing shipment_date');
  }

  const buffer = await generatePackingSlips(pool, shipmentDate);

  return new Response(buffer as ArrayBuffer, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': `attachment; filename="packing-slips-${shipmentDate}.xlsx"`,
    },
  });
};
