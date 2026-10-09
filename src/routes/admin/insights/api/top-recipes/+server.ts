import type { RequestHandler } from './$types';
import { getTopRecipes } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { requireFlag } from '$lib/server/flags';
import { json, error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url, locals }) => {
  const from = url.searchParams.get('from');
  const to = url.searchParams.get('to');
  const limit = parseInt(url.searchParams.get('limit') ?? '10', 10);
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (!from?.match(dateRe) || !to?.match(dateRe)) {
    throw error(400, 'Provide from and to as YYYY-MM-DD');
  }

  const caller = callerFromLocals(locals);
  await requireFlag('insights', caller);
  const data = await getTopRecipes(from, to, limit, caller);
  return json(data);
};
