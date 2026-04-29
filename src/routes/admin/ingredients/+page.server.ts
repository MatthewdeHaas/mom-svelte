import type { PageServerLoad } from './$types';
import { getIngredientsWithAllergens } from '$lib/server/queries/ingredients';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ url }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q')?.toLowerCase() ?? '';

  const ingredients = await getIngredientsWithAllergens(pool, { includeInactive: showInactive });

  const filtered = search
    ? ingredients.filter(i => i.name.toLowerCase().includes(search))
    : ingredients;

  return { ingredients: filtered, showInactive, search };
};
