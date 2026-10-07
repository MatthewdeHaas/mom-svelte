/**
 * Users table reads/writes forward to the Rust service — see
 * `admin-operations-rust/crates/db/src/queries/users.rs`. Better
 * Auth internal-adapter calls (`createUser`, `linkAccount`,
 * `createAccount`, `updatePassword`, `findAccounts`) stay in TS
 * because `auth.$context` is a server-side Node API, not an HTTP
 * surface. Scope line: this route owns the auth stack; the Rust
 * service owns the data layer.
 */
import { fail, error, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import { randomUUID } from 'crypto';
import type { Actions, PageServerLoad } from './$types';
import {
  getCompanyById,
  getUsersByCompany,
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
import {
  internalJson,
  isInternalApiError,
  filtersToQueryString,
  type InternalCaller,
} from '$lib/server/queries/_rust-dispatch';
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

// Build an InternalCaller from the request's authenticated user. Shared
// across load + every action that forwards to the Rust service. The
// `role` cast is safe: Better Auth only ever stores 'admin' or 'user'
// (client); narrowing here matches the Rust `Role` enum.
function callerFromLocals(locals: App.Locals): InternalCaller {
  return {
    userId: locals.user!.id,
    role: locals.user!.role as 'admin' | 'client',
    companyId: locals.user!.companyId ?? null,
  };
}

export const load: PageServerLoad = async ({ params, locals }) => {
  const caller = callerFromLocals(locals);
  const [company, persons, orders, invites, users] = await Promise.all([
    getCompanyById(pool, params.id, caller),
    getPersonsByCompany(pool, params.id, { includeInactive: false }, caller),
    getOrders(pool, { company_id: params.id }, caller),
    getInvitesByCompany(pool, params.id, caller),
    getUsersByCompany(pool, params.id, caller),
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
  createUserWithPassword: async ({ request, params, locals }) => {
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
    const caller = callerFromLocals(locals);

    // Pre-insert duplicate-email probe. Rust returns 404 on miss; we
    // treat anything else as a hard failure.
    let existing: { id: string } | null = null;
    try {
      existing = await internalJson<{ id: string }>(
        `/api/users/by-email${filtersToQueryString({ email })}`,
        caller,
      );
    } catch (err) {
      if (!(isInternalApiError(err) && err.status === 404)) throw err;
    }
    if (existing) {
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

    // Stamp company_id on the freshly-created user row via the Rust service.
    await internalJson<{ id: string; company_id: string | null }>(
      `/api/users/${created.id}/assign-company`,
      caller,
      { method: 'POST', body: JSON.stringify({ company_id: params.id }) },
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
  resetUserPassword: async ({ request, params, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(ResetPasswordSchema, form);
    if (!parsed.success) {
      return fail(422, { action: 'resetUserPassword', errors: parsed.errors });
    }

    const caller = callerFromLocals(locals);

    // Ownership-scoped lookup: Rust returns 404 both when the user
    // doesn't exist and when they belong to a different company —
    // deliberately indistinguishable to stop a /companies/X admin
    // from probing for ids owned by /companies/Y.
    let owned: { id: string; email: string } | null = null;
    try {
      owned = await internalJson<{ id: string; email: string }>(
        `/api/users/${parsed.data.user_id}${filtersToQueryString({ company_id: params.id })}`,
        caller,
      );
    } catch (err) {
      if (!(isInternalApiError(err) && err.status === 404)) throw err;
    }
    if (!owned) {
      return fail(404, {
        action: 'resetUserPassword',
        errors: { user_id: 'User not found for this company' },
      });
    }

    const ctx = await auth.$context;
    const password = generatePassword();
    const hashed = await ctx.password.hash(password);

    const accounts = await ctx.internalAdapter.findAccounts(owned.id);
    const credential = accounts.find((a) => a.providerId === 'credential');
    if (!credential) {
      await ctx.internalAdapter.createAccount({
        userId: owned.id,
        providerId: 'credential',
        accountId: owned.id,
        password: hashed,
      });
    } else {
      await ctx.internalAdapter.updatePassword(owned.id, hashed);
    }

    return {
      action: 'resetUserPassword',
      success: true,
      email: owned.email,
      user_id: owned.id,
      password,
    };
  },
};
