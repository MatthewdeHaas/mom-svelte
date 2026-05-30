import { pool } from '$lib/server/db';
import { getOrders } from '$lib/server/queries/orders';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;

  const orders = await getOrders(pool, { company_id: companyId });

  return { orders };
};
