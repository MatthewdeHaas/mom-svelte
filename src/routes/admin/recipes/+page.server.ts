import type { PageServerLoad } from './$types';
import { getRecipes } from '$lib/server/queries/recipes';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ url }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q')?.toLowerCase() ?? '';

  const recipes = await getRecipes(pool, { includeInactive: showInactive });

  const filtered = search
    ? recipes.filter(r => r.name.toLowerCase().includes(search))
    : recipes;

  return { recipes: filtered, showInactive, search };
};
