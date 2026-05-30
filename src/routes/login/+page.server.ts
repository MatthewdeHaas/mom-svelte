import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  if (locals.user?.role === 'admin') {
    redirect(302, '/admin/dashboard');
  }
  if (locals.user) {
    redirect(302, '/client');
  }
  return {};
};
