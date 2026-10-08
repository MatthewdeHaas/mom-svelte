import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createCompany } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { CompanySchema } from '$lib/server/schemas/company';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  default: async ({ request, locals }) => {
    const caller = callerFromLocals(locals);
    const form = await request.formData();
    const parsed = parseFormData(CompanySchema, form);

    if (!parsed.success) {
      return fail(422, { errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const { name, contact_email, phone, shipping_address } = parsed.data;

    let company;
    try {
      company = await createCompany({
        name,
        contact_email,
        phone,
        shipping_address,
        active: true,
      }, caller);
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, {
          errors: { name: 'A company with this name already exists' },
          values: Object.fromEntries(form),
        });
      }
      throw err;
    }

    redirect(303, `/admin/companies/${company.id}`);
  },
};
