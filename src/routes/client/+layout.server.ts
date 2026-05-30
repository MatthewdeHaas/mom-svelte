import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = async ({ locals, url }) => {
  if (!locals.user) {
    redirect(302, '/login');
  }

  // Admins should use the admin portal
  if (locals.user.role === 'admin') {
    redirect(302, '/admin/dashboard');
  }

  // User is authenticated but not yet assigned to a company
  if (!locals.user.companyId && url.pathname !== '/client/pending') {
    redirect(302, '/client/pending');
  }

  return { user: locals.user };
};
