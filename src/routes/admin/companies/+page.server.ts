import type { PageServerLoad } from './$types';
import { getCompaniesPaginated } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parsePaginationParams } from '$lib/server/queries/pagination';

export const load: PageServerLoad = async ({ url, locals }) => {
  const caller = callerFromLocals(locals);
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q') ?? '';
  const pagination = parsePaginationParams(url, { perPage: 25 });

  const result = await getCompaniesPaginated(
    { includeInactive: showInactive, search: search || undefined },
    pagination,
    caller,
  );

  return {
    companies: result.data,
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
