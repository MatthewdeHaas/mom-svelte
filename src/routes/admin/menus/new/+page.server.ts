import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createMenu } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parseFormData } from '$lib/server/form';
import { MenuSchema } from '$lib/server/schemas/menu';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  default: async ({ request, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(MenuSchema, form);

    if (!parsed.success) {
      return fail(422, { errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const { name, start_date, end_date } = parsed.data;

    const caller = callerFromLocals(locals);
    const menu = await createMenu({
      name,
      is_active: true,
      start_date: start_date ? new Date(start_date) : null,
      end_date:   end_date   ? new Date(end_date)   : null,
    }, caller);

    redirect(303, `/admin/menus/${menu.id}`);
  },
};
