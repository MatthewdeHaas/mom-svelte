import type { RequestHandler } from './$types';
import { getOrders } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { weekBoundsEST } from '$lib/utils/dates';
import { error, json } from '@sveltejs/kit';

function resolve(url: URL): { from: string; to: string } {
  const weekOf = url.searchParams.get('week_of');
  const single = url.searchParams.get('shipment_date');
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (weekOf?.match(dateRe)) return weekBoundsEST(weekOf);
  if (single?.match(dateRe)) return { from: single, to: single };
  throw error(400, 'Provide either week_of or shipment_date (YYYY-MM-DD)');
}

export const GET: RequestHandler = async ({ url, locals }) => {
  const { from, to } = resolve(url);
  const caller = callerFromLocals(locals);
  const orders = await getOrders(
    {
      serving_date_from: new Date(from),
      serving_date_to: new Date(to),
    },
    caller,
  );

  const recipeIds = [...new Set(orders.map((o) => o.recipe_id))];
  const totalMeals = orders.reduce((sum, o) => sum + Number(o.amount), 0);

  return json({ recipes: recipeIds.length, totalMeals });
};
