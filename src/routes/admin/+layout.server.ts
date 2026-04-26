import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = async ({ locals, url }) => {
  if (!locals.user) {
    if (url.pathname !== '/login') {
      redirect(302, '/login');
    }
  }
  return { user: locals.user };
};
