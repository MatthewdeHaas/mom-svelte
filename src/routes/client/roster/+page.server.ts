import { fail, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import {
  getPersonsWithAllergensByCompany,
  createPerson,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parseFormData } from '$lib/server/form';
import type { Actions, PageServerLoad } from './$types';

const CreatePersonSchema = z.object({
  name: z
    .string()
    .min(1, 'Name is required')
    .max(100, 'Name must be 100 characters or fewer')
    .transform((v) => v.trim()),
});

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;
  const caller = callerFromLocals(locals);
  const persons = await getPersonsWithAllergensByCompany(companyId, { includeInactive: true }, caller);
  return { persons };
};

export const actions: Actions = {
  create: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const caller = callerFromLocals(locals);
    const form = await request.formData();
    const parsed = parseFormData(CreatePersonSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'create', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const person = await createPerson({
      company_id: companyId,
      name: parsed.data.name,
    }, caller);

    redirect(303, `/client/roster/${person.id}`);
  },
};
