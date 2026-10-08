import type { PageServerLoad } from './$types';
import { getKPIMetrics } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { todayEST, weekBoundsEST, shiftDate } from '$lib/utils/dates';

export const load: PageServerLoad = async ({ locals }) => {
  const today = todayEST();
  const { from, to } = weekBoundsEST(today);
  const priorFrom = shiftDate(from, -7);
  const priorTo = shiftDate(to, -7);

  const caller = callerFromLocals(locals);
  const kpi = await getKPIMetrics(from, to, priorFrom, priorTo, caller);

  return { kpi, weekFrom: from, weekTo: to };
};
