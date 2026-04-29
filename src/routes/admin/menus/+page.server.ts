import type { PageServerLoad } from './$types';
import { getMenus } from '$lib/server/queries/menus';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ url }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q')?.toLowerCase() ?? '';

  const menus = await getMenus(pool, { includeInactive: showInactive });

  const filtered = search
    ? menus.filter(m => m.name.toLowerCase().includes(search))
    : menus;

  return { menus: filtered, showInactive, search };
};
