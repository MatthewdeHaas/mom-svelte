import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createCompany } from '$lib/server/queries/companies';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  default: async ({ request }) => {
    const form = await request.formData();

    const name            = form.get('name')?.toString().trim() ?? '';
    const contact_email   = form.get('contact_email')?.toString().trim() || null;
    const phone           = form.get('phone')?.toString().trim() || null;
    const shipping_address = form.get('shipping_address')?.toString().trim() || null;

    if (!name) {
      return fail(422, { errors: { name: 'Company name is required' }, values: { name, contact_email, phone, shipping_address } });
    }

    let company;
    try {
      company = await createCompany(pool, {
        name,
        contact_email,
        phone,
        shipping_address,
        active: true,
      });
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, { errors: { name: 'A company with this name already exists' }, values: { name, contact_email, phone, shipping_address } });
      }
      throw err;
    }

    redirect(303, `/admin/companies/${company.id}`);
  },
};
