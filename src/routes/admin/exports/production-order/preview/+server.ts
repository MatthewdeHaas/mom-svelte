import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { getOrdersByShipmentDate } from '$lib/server/queries/orders';
import { getRecipeWithIngredients } from '$lib/server/queries/recipes';
import { error, json } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url }) => {
  const shipmentDate = url.searchParams.get('shipment_date');
  if (!shipmentDate?.match(/^\d{4}-\d{2}-\d{2}$/)) {
    throw error(400, 'Invalid or missing shipment_date');
  }

  const orders = await getOrdersByShipmentDate(pool, new Date(shipmentDate));
  const recipeIds = [...new Set(orders.map((o) => o.recipe_id))];
  const totalMeals = orders.reduce((sum, o) => sum + o.amount, 0);

  return json({
    recipes: recipeIds.length,
    totalMeals,
  });
};
