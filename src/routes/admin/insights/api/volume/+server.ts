import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { getOrderVolumeSeries } from '$lib/server/queries/insights';
import { json, error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url }) => {
  const from = url.searchParams.get('from');
  const to = url.searchParams.get('to');
  const granularity = url.searchParams.get('granularity') === 'weekly' ? 'weekly' : 'daily';
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (!from?.match(dateRe) || !to?.match(dateRe)) {
    throw error(400, 'Provide from and to as YYYY-MM-DD');
  }

  const data = await getOrderVolumeSeries(pool, from, to, granularity);
  return json(data);
};
