import type { PageServerLoad } from './$types';
import { getMenus } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';

export const load: PageServerLoad = async ({ url, locals }) => {
  const showInactive = url.searchParams.get('inactive') === 'true';
  const search = url.searchParams.get('q')?.toLowerCase() ?? '';

  const caller = callerFromLocals(locals);
  const menus = await getMenus({ includeInactive: showInactive }, caller);

  const filtered = search
    ? menus.filter(m => m.name.toLowerCase().includes(search))
    : menus;

  return { menus: filtered, showInactive, search };
};
