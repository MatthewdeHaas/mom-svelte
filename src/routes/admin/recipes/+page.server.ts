import type { PageServerLoad } from './$types';
import { getRecipesWithIngredientsPaginated } from '$lib/server/queries/recipes';
import { parsePaginationParams } from '$lib/server/queries/pagination';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ url }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q') ?? '';
  const pagination = parsePaginationParams(url, { perPage: 25 });

  const result = await getRecipesWithIngredientsPaginated(
    pool,
    { includeInactive: showInactive, search: search || undefined },
    pagination
  );

  return {
    recipes: result.data,
    pagination: {
      page: result.page,
      perPage: result.perPage,
      total: result.total,
      totalPages: result.totalPages,
    },
    showInactive,
    search,
  };
};
