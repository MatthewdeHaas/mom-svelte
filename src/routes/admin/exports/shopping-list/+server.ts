import type { RequestHandler } from './$types';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { weekBoundsEST } from '$lib/utils/dates';
import { error } from '@sveltejs/kit';
import type { InternalCaller } from '$lib/server/api-client';

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

export const GET: RequestHandler = async ({ url, locals }) => {
  if (!locals.user) throw error(401, 'Unauthorised');

  const caller: InternalCaller = {
    userId: locals.user.id,
    role: locals.user.role === 'client' ? 'client' : 'admin',
    companyId: locals.user.companyId ?? null,
  };

  const { from, to, label } = resolve(url);
  const buffer = await generateShoppingList(from, to, caller);

  return new Response(buffer as unknown as BodyInit, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': `attachment; filename="shopping-${label}.xlsx"`,
    },
  });
};
