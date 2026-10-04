import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { weekBoundsEST } from '$lib/utils/dates';
import { error } from '@sveltejs/kit';

function resolve(url: URL): { from: string; to: string; label: string } {
  const weekOf = url.searchParams.get('week_of');
  const single = url.searchParams.get('shipment_date');
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (weekOf?.match(dateRe)) {
    const { from, to } = weekBoundsEST(weekOf);
    return { from, to, label: `week-of-${from}` };
  }
  if (single?.match(dateRe)) {
    return { from: single, to: single, label: single };
  }
  throw error(400, 'Provide either week_of or shipment_date (YYYY-MM-DD)');
}

export const GET: RequestHandler = async ({ url }) => {
  const { from, to, label } = resolve(url);
  const buffer = await generateShoppingList(pool, from, to);

  return new Response(buffer as unknown as BodyInit, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': `attachment; filename="shopping-${label}.xlsx"`,
    },
  });
};
