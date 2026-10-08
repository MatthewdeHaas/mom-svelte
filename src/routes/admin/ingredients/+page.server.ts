import type { PageServerLoad } from './$types';
import { getIngredientsWithAllergensPaginated } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parsePaginationParams } from '$lib/server/queries/pagination';

export const load: PageServerLoad = async ({ url, locals }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q') ?? '';
  const pagination = parsePaginationParams(url, { perPage: 25 });

  const caller = callerFromLocals(locals);
  const result = await getIngredientsWithAllergensPaginated(
    { includeInactive: showInactive, search: search || undefined },
    pagination,
    caller,
  );

  return {
    ingredients: result.data,
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
