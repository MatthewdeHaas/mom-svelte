import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createMenu } from '$lib/server/queries/menus';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  default: async ({ request }) => {
    const form = await request.formData();

    const name       = form.get('name')?.toString().trim() ?? '';
    const start_date = form.get('start_date')?.toString() || null;
    const end_date   = form.get('end_date')?.toString() || null;

    const errors: Record<string, string> = {};
    if (!name) errors.name = 'Menu name is required';
    if (start_date && end_date && new Date(start_date) > new Date(end_date)) {
      errors.end_date = 'End date must be on or after start date';
    }

    if (Object.keys(errors).length) {
      return fail(422, { errors, values: { name, start_date, end_date } });
    }

    const menu = await createMenu(pool, {
      name,
      is_active: true,
      start_date: start_date ? new Date(start_date) : null,
      end_date:   end_date   ? new Date(end_date)   : null,
    });

    redirect(303, `/admin/menus/${menu.id}`);
  },
};
