import type { PageServerLoad } from './$types';
import { getKPIMetrics } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { requireFlag } from '$lib/server/flags';
import { todayEST, weekBoundsEST, shiftDate } from '$lib/utils/dates';

export const load: PageServerLoad = async ({ locals }) => {
  const caller = callerFromLocals(locals);

  // Gate the insights surface behind the `insights` feature flag. 404
  // when disabled so a disabled surface is indistinguishable from one
  // we've never shipped. The 7 /api/insights/* JSON endpoints in this
  // directory re-check the flag themselves (see api/*/+server.ts) so
  // a disabled flag closes both the HTML page and the fetchers.
  await requireFlag('insights', caller);

  const today = todayEST();
  const { from, to } = weekBoundsEST(today);
  const priorFrom = shiftDate(from, -7);
  const priorTo = shiftDate(to, -7);

  const kpi = await getKPIMetrics(from, to, priorFrom, priorTo, caller);

  return { kpi, weekFrom: from, weekTo: to };
};
