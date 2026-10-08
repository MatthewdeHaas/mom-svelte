import type { RequestHandler } from './$types';
import { getOrders } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { weekBoundsEST } from '$lib/utils/dates';
import { error, json } from '@sveltejs/kit';

function resolve(url: URL): string {
  const weekOf = url.searchParams.get('week_of');
  const single = url.searchParams.get('shipment_date');
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (weekOf?.match(dateRe)) return weekOf;
  if (single?.match(dateRe)) return single;
  throw error(400, 'Provide either week_of or shipment_date (YYYY-MM-DD)');
}

export const GET: RequestHandler = async ({ url, locals }) => {
  const dateParam = resolve(url);
  const { from, to } = weekBoundsEST(dateParam);

  const caller = callerFromLocals(locals);
  const orders = await getOrders(
    {
      serving_date_from: new Date(from),
      serving_date_to: new Date(to),
    },
    caller,
  );

  const pairs = new Set(orders.map((o) => `${o.company_id}::${o.shipment_date}`));
  const companies = new Set(orders.map((o) => o.company_id));

  return json({ slips: pairs.size, companies: companies.size });
};
