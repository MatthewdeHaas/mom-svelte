import { fail, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import { pool } from '$lib/server/db';
import {
  getPersonsWithAllergensByCompany,
  createPerson,
} from '$lib/server/queries/persons';
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
  const persons = await getPersonsWithAllergensByCompany(pool, companyId, { includeInactive: true });
  return { persons };
};

export const actions: Actions = {
  create: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const form = await request.formData();
    const parsed = parseFormData(CreatePersonSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'create', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const person = await createPerson(pool, {
      company_id: companyId,
      name: parsed.data.name,
    });

    redirect(303, `/client/roster/${person.id}`);
  },
};
