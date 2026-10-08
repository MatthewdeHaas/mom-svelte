import { fail } from '@sveltejs/kit';
import { z } from 'zod';
import { getCompanyById } from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parseFormData } from '$lib/server/form';
import { auth } from '$lib/server/auth';
import type { Actions, PageServerLoad } from './$types';

const UpdateProfileSchema = z.object({
  name: z
    .string()
    .min(1, 'Name is required')
    .max(100, 'Name must be 100 characters or fewer')
    .transform((v) => v.trim()),
});

export const load: PageServerLoad = async ({ locals }) => {
  const caller = callerFromLocals(locals);
  const companyId = locals.user!.companyId!;
  const company = await getCompanyById(companyId, caller);
  return { user: locals.user!, company };
};

export const actions: Actions = {
  update: async ({ request, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(UpdateProfileSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'update', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    await auth.api.updateUser({
      body: { name: parsed.data.name },
      headers: request.headers,
    });

    return { action: 'update', success: true };
  },
};
