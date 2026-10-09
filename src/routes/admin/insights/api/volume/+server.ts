import type { RequestHandler } from './$types';
import { getOrderVolumeSeries } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { requireFlag } from '$lib/server/flags';
import { json, error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url, locals }) => {
  const from = url.searchParams.get('from');
  const to = url.searchParams.get('to');
  const granularity = url.searchParams.get('granularity') === 'weekly' ? 'weekly' : 'daily';
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (!from?.match(dateRe) || !to?.match(dateRe)) {
    throw error(400, 'Provide from and to as YYYY-MM-DD');
  }

  const caller = callerFromLocals(locals);
  // Mirror the page-level gate — a disabled flag closes both the HTML
  // page and these JSON fetchers so a stale client tab can't keep
  // scraping data after `insights` is turned off.
  await requireFlag('insights', caller);
  const data = await getOrderVolumeSeries(from, to, granularity, caller);
  return json(data);
};
