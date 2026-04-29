import type { PageServerLoad } from './$types';
import { getOrderCountsByStatus, getOrders } from '$lib/server/queries/orders';
import { getCompanies } from '$lib/server/queries/companies';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async () => {
  const today = new Date();
  today.setHours(0, 0, 0, 0);

  const tomorrow = new Date(today);
  tomorrow.setDate(tomorrow.getDate() + 1);

  const [statusCounts, companies, recentOrders, todaysOrders] = await Promise.all([
    getOrderCountsByStatus(pool),
    getCompanies(pool),
    getOrders(pool, { limit: 8 } as any),
    getOrders(pool, {
      serving_date_from: today,
      serving_date_to: tomorrow,
    }),
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
