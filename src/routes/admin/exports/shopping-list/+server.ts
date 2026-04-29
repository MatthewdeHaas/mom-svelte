import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url }) => {
  const shipmentDate = url.searchParams.get('shipment_date');
  if (!shipmentDate?.match(/^\d{4}-\d{2}-\d{2}$/)) {
    throw error(400, 'Invalid or missing shipment_date');
  }

  const buffer = await generateShoppingList(pool, shipmentDate);

  return new Response(buffer as ArrayBuffer, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': `attachment; filename="shopping-list-${shipmentDate}.xlsx"`,
    },
  });
};
