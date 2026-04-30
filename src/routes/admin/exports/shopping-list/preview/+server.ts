import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { getOrders } from '$lib/server/queries/orders';
import { getRecipeWithIngredients } from '$lib/server/queries/recipes';
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

export const GET: RequestHandler = async ({ url }) => {
  const { from, to } = resolve(url);
  const orders = await getOrders(pool, {
    serving_date_from: new Date(from),
    serving_date_to: new Date(to),
  });

  const recipeIds = [...new Set(orders.map((o) => o.recipe_id))];
  const recipes = await Promise.all(
    recipeIds.map((id) => getRecipeWithIngredients(pool, id))
  );

  const ingredientKeys = new Set<string>();
  for (const recipe of recipes) {
    if (!recipe) continue;
    for (const ing of recipe.ingredients) {
      ingredientKeys.add(`${ing.name}::${ing.unit ?? ''}`);
    }
  }

  return json({ ingredients: ingredientKeys.size, orders: orders.length });
};
