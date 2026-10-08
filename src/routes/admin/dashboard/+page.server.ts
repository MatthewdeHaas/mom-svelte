import type { PageServerLoad } from './$types';
import {
  getCompanies,
  getOrderCountsByStatus,
  getOrders,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';

export const load: PageServerLoad = async ({ locals }) => {
  const caller = callerFromLocals(locals);
  const today = new Date();
  today.setHours(0, 0, 0, 0);

  const tomorrow = new Date(today);
  tomorrow.setDate(tomorrow.getDate() + 1);

  const [statusCounts, companies, recentOrders, todaysOrders] = await Promise.all([
    getOrderCountsByStatus({}, caller),
    getCompanies({}, caller),
    getOrders({}, caller),
    getOrders(
      {
        serving_date_from: today,
        serving_date_to: tomorrow,
      },
      caller,
    ),
  ]);

  return {
    stats: {
      ordersToday: todaysOrders.length,
      pending: statusCounts.pending ?? 0,
      confirmed: statusCounts.confirmed ?? 0,
      companies: companies.filter(c => c.active).length,
    },
    recentOrders,
  };
};
