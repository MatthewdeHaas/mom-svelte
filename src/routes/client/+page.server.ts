import { getOrders } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;
  const caller = callerFromLocals(locals);

  const today = new Date();
  today.setHours(0, 0, 0, 0);

  const upcoming = await getOrders(
    {
      company_id: companyId,
      serving_date_from: today,
      status: 'confirmed',
    },
    caller,
  );

  const recent = await getOrders(
    {
      company_id: companyId,
      serving_date_to: today,
    },
    caller,
  );

  return {
    upcoming: upcoming.slice(0, 5),
    recentCount: recent.length,
  };
};
