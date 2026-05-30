import { pool } from '$lib/server/db';
import { getOrders } from '$lib/server/queries/orders';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;

  const today = new Date();
  today.setHours(0, 0, 0, 0);

  const upcoming = await getOrders(pool, {
    company_id: companyId,
    serving_date_from: today,
    status: 'confirmed',
  });

  const recent = await getOrders(pool, {
    company_id: companyId,
    serving_date_to: today,
  });

  return {
    upcoming: upcoming.slice(0, 5),
    recentCount: recent.length,
  };
};
