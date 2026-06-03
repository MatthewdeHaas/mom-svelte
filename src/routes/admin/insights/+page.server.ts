import type { PageServerLoad } from './$types';
import { pool } from '$lib/server/db';
import { getKPIMetrics } from '$lib/server/queries/insights';
import { todayEST, weekBoundsEST, shiftDate } from '$lib/utils/dates';

export const load: PageServerLoad = async () => {
  const today = todayEST();
  const { from, to } = weekBoundsEST(today);
  const priorFrom = shiftDate(from, -7);
  const priorTo = shiftDate(to, -7);

  const kpi = await getKPIMetrics(pool, from, to, priorFrom, priorTo);

  return { kpi, weekFrom: from, weekTo: to };
};
