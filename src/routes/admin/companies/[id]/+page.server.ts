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
import { auth, sendInviteEmail } from '$lib/server/auth';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { CompanySchema } from '$lib/server/schemas/company';
import { generatePassword } from '$lib/utils/generate-password';

const InviteSchema = z.object({
  email: z.string().email('A valid email address is required').transform((v) => v.toLowerCase().trim()),
});

const RevokeInviteSchema = z.object({
  invite_id: z.string().uuid('Invalid invite ID'),
});

const CreateUserSchema = z.object({
  name: z.string().min(1, 'Name is required').transform((v) => v.trim()),
  email: z.string().email('A valid email address is required').transform((v) => v.toLowerCase().trim()),
});

const ResetPasswordSchema = z.object({
  user_id: z.string().min(1, 'User id is required'),
});

type CompanyUser = { id: string; email: string; name: string; created_at: Date };

async function getUsersByCompany(companyId: string): Promise<CompanyUser[]> {
  const result = await pool.query<CompanyUser>(
    `SELECT id, email, name, "createdAt" AS created_at
     FROM "user"
     WHERE company_id = $1
     ORDER BY "createdAt" ASC`,
    [companyId]
  );
  return result.rows;
}

export const load: PageServerLoad = async ({ params }) => {
  const [company, persons, orders, invites, users] = await Promise.all([
    getCompanyById(pool, params.id),
    getPersonsByCompany(pool, params.id, { includeInactive: false }),
    getOrders(pool, { company_id: params.id }),
    getInvitesByCompany(pool, params.id),
    getUsersByCompany(params.id),
  ]);

  if (!company) error(404, 'Company not found');

  return { company, persons, orders, invites, users };
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

  // Admin provisions a client user directly: creates the auth user row,
  // attaches a `credential` account with a freshly-generated password, and
  // returns the plaintext ONCE so the admin can share it out-of-band. The
  // password is never persisted in cleartext and never shown again — a
  // subsequent reset generates a new one.
  createUserWithPassword: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(CreateUserSchema, form);
    if (!parsed.success) {
      return fail(422, {
        action: 'createUserWithPassword',
        errors: parsed.errors,
        values: Object.fromEntries(form),
      });
    }

    const { email, name } = parsed.data;

    const existing = await pool.query<{ id: string }>(
      `SELECT id FROM "user" WHERE email = $1`,
      [email]
    );
    if (existing.rows.length) {
      return fail(422, {
        action: 'createUserWithPassword',
        errors: { email: 'A user with this email already exists' },
        values: Object.fromEntries(form),
      });
    }

    const ctx = await auth.$context;
    const password = generatePassword();
    const hashed = await ctx.password.hash(password);

    // Mirrors admin plugin's createUser: internalAdapter.createUser → linkAccount.
    // We bypass the HTTP route so we can stamp company_id atomically (it's
    // marked `input: false` on the auth config and can't flow in via the API).
    const created = await ctx.internalAdapter.createUser({
      email,
      name,
      role: 'user',
    });
    if (!created) {
      return fail(500, {
        action: 'createUserWithPassword',
        errors: { email: 'Failed to create user' },
        values: Object.fromEntries(form),
      });
    }
    await ctx.internalAdapter.linkAccount({
      accountId: created.id,
      providerId: 'credential',
      password: hashed,
      userId: created.id,
    });
    await pool.query(
      `UPDATE "user" SET company_id = $1 WHERE id = $2`,
      [params.id, created.id]
    );

    return {
      action: 'createUserWithPassword',
      success: true,
      email,
      // Returned exactly once — the UI must display and let the admin copy
      // it before leaving the page. On refresh/navigation it's gone.
      password,
    };
  },

  // Reset an existing user's password. Scoped to users belonging to THIS
  // company so an admin viewing /companies/X can't reset a user attached
  // to company Y via a crafted form POST.
  resetUserPassword: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(ResetPasswordSchema, form);
    if (!parsed.success) {
      return fail(422, { action: 'resetUserPassword', errors: parsed.errors });
    }

    const owned = await pool.query<{ id: string; email: string }>(
      `SELECT id, email FROM "user" WHERE id = $1 AND company_id = $2`,
      [parsed.data.user_id, params.id]
    );
    if (!owned.rows[0]) {
      return fail(404, {
        action: 'resetUserPassword',
        errors: { user_id: 'User not found for this company' },
      });
    }

    const ctx = await auth.$context;
    const password = generatePassword();
    const hashed = await ctx.password.hash(password);

    const accounts = await ctx.internalAdapter.findAccounts(owned.rows[0].id);
    const credential = accounts.find((a) => a.providerId === 'credential');
    if (!credential) {
      await ctx.internalAdapter.createAccount({
        userId: owned.rows[0].id,
        providerId: 'credential',
        accountId: owned.rows[0].id,
        password: hashed,
      });
    } else {
      await ctx.internalAdapter.updatePassword(owned.rows[0].id, hashed);
    }

    return {
      action: 'resetUserPassword',
      success: true,
      email: owned.rows[0].email,
      user_id: owned.rows[0].id,
      password,
    };
  },
};
