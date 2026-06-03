import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { getTopRecipes } from '$lib/server/queries/insights';
import { json, error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url }) => {
  const from = url.searchParams.get('from');
  const to = url.searchParams.get('to');
  const limit = parseInt(url.searchParams.get('limit') ?? '10', 10);
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (!from?.match(dateRe) || !to?.match(dateRe)) {
    throw error(400, 'Provide from and to as YYYY-MM-DD');
  }

  const data = await getTopRecipes(pool, from, to, limit);
  return json(data);
};
