import { fail, error, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import { randomUUID } from 'crypto';
import type { Actions, PageServerLoad } from './$types';
import {
  getCompanyById,
  updateCompany,
  deactivateCompany,
} from '$lib/server/queries/companies';
import { getPersonsByCompany } from '$lib/server/queries/persons';
import { getOrders } from '$lib/server/queries/orders';
import {
  createInvite,
  getInvitesByCompany,
  revokeInvite,
} from '$lib/server/queries/invites';
import { sendInviteEmail } from '$lib/server/auth';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { CompanySchema } from '$lib/server/schemas/company';

const InviteSchema = z.object({
  email: z.string().email('A valid email address is required').transform((v) => v.toLowerCase().trim()),
});

const RevokeInviteSchema = z.object({
  invite_id: z.string().uuid('Invalid invite ID'),
});

export const load: PageServerLoad = async ({ params }) => {
  const [company, persons, orders, invites] = await Promise.all([
    getCompanyById(pool, params.id),
    getPersonsByCompany(pool, params.id, { includeInactive: false }),
    getOrders(pool, { company_id: params.id }),
    getInvitesByCompany(pool, params.id),
  ]);

  if (!company) error(404, 'Company not found');

  return { company, persons, orders, invites };
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

  invite: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(InviteSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'invite', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const token = randomUUID();
    const expiresAt = new Date(Date.now() + 48 * 60 * 60 * 1000);

    await createInvite(pool, {
      company_id: params.id,
      email: parsed.data.email,
      token,
      expires_at: expiresAt,
    });

    const baseUrl = process.env.BETTER_AUTH_URL ?? '';
    await sendInviteEmail(parsed.data.email, `${baseUrl}/invite/${token}`);

    return { action: 'invite', success: true };
  },

  revokeInvite: async ({ request }) => {
    const form = await request.formData();
    const parsed = parseFormData(RevokeInviteSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'revokeInvite', errors: parsed.errors });
    }

    await revokeInvite(pool, parsed.data.invite_id);

    return { action: 'revokeInvite', success: true };
  },
};
