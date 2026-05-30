import { pool } from '$lib/server/db';
import { getPersonsWithAllergensByCompany } from '$lib/server/queries/persons';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;

  const persons = await getPersonsWithAllergensByCompany(pool, companyId);

  return { persons };
};
