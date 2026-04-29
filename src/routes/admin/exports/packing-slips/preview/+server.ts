import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { getOrders } from '$lib/server/queries/orders';
import { error, json } from '@sveltejs/kit';

function weekBounds(date: Date): { from: Date; to: Date } {
  const d = new Date(date);
  const day = d.getDay();
  const diffToMon = day === 0 ? -6 : 1 - day;
  const mon = new Date(d);
  mon.setDate(d.getDate() + diffToMon);
  const sun = new Date(mon);
  sun.setDate(mon.getDate() + 6);
  return { from: mon, to: sun };
}

export const GET: RequestHandler = async ({ url }) => {
  const shipmentDate = url.searchParams.get('shipment_date');
  if (!shipmentDate?.match(/^\d{4}-\d{2}-\d{2}$/)) {
    throw error(400, 'Invalid or missing shipment_date');
  }

  const { from, to } = weekBounds(new Date(shipmentDate));
  const orders = await getOrders(pool, {
    serving_date_from: from,
    serving_date_to: to,
  });

  // Count unique company × shipment_date pairs = number of slips
  const pairs = new Set(orders.map((o) => `${o.company_id}::${o.shipment_date}`));
  const companies = new Set(orders.map((o) => o.company_id));

  return json({
    slips: pairs.size,
    companies: companies.size,
  });
};
