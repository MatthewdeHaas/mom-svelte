import { fail, error, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getCompanyById,
  updateCompany,
  deactivateCompany,
} from '$lib/server/queries/companies';
import { getPersonsByCompany } from '$lib/server/queries/persons';
import { getOrders } from '$lib/server/queries/orders';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { CompanySchema } from '$lib/server/schemas/company';

export const load: PageServerLoad = async ({ params }) => {
  const [company, persons, orders] = await Promise.all([
    getCompanyById(pool, params.id),
    getPersonsByCompany(pool, params.id, { includeInactive: false }),
    getOrders(pool, { company_id: params.id }),
  ]);

  if (!company) error(404, 'Company not found');

  return { company, persons, orders };
};

export const actions: Actions = {
  update: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(CompanySchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'update', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const { name, contact_email, phone, shipping_address } = parsed.data;

    try {
      await updateCompany(pool, params.id, { name, contact_email, phone, shipping_address });
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, {
          action: 'update',
          errors: { name: 'A company with this name already exists' },
          values: Object.fromEntries(form),
        });
      }
      throw err;
    }

    return { action: 'update', success: true };
  },

  deactivate: async ({ params }) => {
    const company = await getCompanyById(pool, params.id);
    if (!company) error(404, 'Company not found');
    if (!company.active) return fail(422, { action: 'deactivate', error: 'Company is already inactive' });

    await deactivateCompany(pool, params.id);

    redirect(303, '/admin/companies');
  },

  reactivate: async ({ params }) => {
    const company = await getCompanyById(pool, params.id);
    if (!company) error(404, 'Company not found');
    if (company.active) return fail(422, { action: 'reactivate', error: 'Company is already active' });

    await updateCompany(pool, params.id, { active: true });

    return { action: 'reactivate', success: true };
  },
};
