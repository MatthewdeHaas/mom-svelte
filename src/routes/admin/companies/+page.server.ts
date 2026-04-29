import type { PageServerLoad } from './$types';
import { getCompanies } from '$lib/server/queries/companies';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ url }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q')?.toLowerCase() ?? '';

  const companies = await getCompanies(pool, { includeInactive: showInactive });

  const filtered = search
    ? companies.filter(c => c.name.toLowerCase().includes(search))
    : companies;

  return { companies: filtered, showInactive, search };
};
