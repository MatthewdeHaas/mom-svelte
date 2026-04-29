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

	const recipes = await Promise.all(
		recipeIds.map((id) => getRecipeWithIngredients(pool, id))
	);

	const ingredientKeys = new Set<string>();
	for (const recipe of recipes) {
		if (!recipe) continue;
		const orderAmounts = orders
			.filter((o) => o.recipe_id === recipe.id)
			.reduce((sum, o) => sum + o.amount, 0);
		if (orderAmounts === 0) continue;
		for (const ing of recipe.ingredients) {
			ingredientKeys.add(`${ing.name}::${ing.unit ?? ''}`);
		}
	}

	return json({
		ingredients: ingredientKeys.size,
		orders: orders.length,
	});
};
