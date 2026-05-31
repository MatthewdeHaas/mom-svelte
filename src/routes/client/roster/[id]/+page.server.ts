import { fail, error, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import { pool } from '$lib/server/db';
import {
  getPersonWithAllergens,
  updatePerson,
  deactivatePerson,
  setPersonAllergens,
} from '$lib/server/queries/persons';
import { getAllergens } from '$lib/server/queries/allergens';
import { parseFormData } from '$lib/server/form';
import type { Actions, PageServerLoad } from './$types';

const UpdatePersonSchema = z.object({
  name: z
    .string()
    .min(1, 'Name is required')
    .max(100, 'Name must be 100 characters or fewer')
    .transform((v) => v.trim()),
});

export const load: PageServerLoad = async ({ params, locals }) => {
  const companyId = locals.user!.companyId!;

  const [person, allAllergens] = await Promise.all([
    getPersonWithAllergens(pool, params.id, { includeInactive: true }),
    getAllergens(pool),
  ]);

  if (!person) error(404, 'Person not found');
  // Ensure this person belongs to the authenticated client's company
  if (person.company_id !== companyId) error(403, 'Forbidden');

  return { person, allAllergens };
};

export const actions: Actions = {
  update: async ({ request, params, locals }) => {
    const companyId = locals.user!.companyId!;
    const form = await request.formData();

    // Verify ownership before any write
    const person = await getPersonWithAllergens(pool, params.id, { includeInactive: true });
    if (!person || person.company_id !== companyId) error(403, 'Forbidden');

    const parsed = parseFormData(UpdatePersonSchema, form);
    if (!parsed.success) {
      return fail(422, { action: 'update', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    // Allergens come as multiple values — getAll, not Object.fromEntries
    const allergenIds = form.getAll('allergen_ids').map(String).filter(Boolean);

    await Promise.all([
      updatePerson(pool, params.id, { name: parsed.data.name }),
      setPersonAllergens(pool, params.id, allergenIds),
    ]);

    return { action: 'update', success: true, name: parsed.data.name };
  },

  deactivate: async ({ params, locals }) => {
    const companyId = locals.user!.companyId!;
    const person = await getPersonWithAllergens(pool, params.id, { includeInactive: true });
    if (!person || person.company_id !== companyId) error(403, 'Forbidden');
    if (!person.active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivatePerson(pool, params.id);
    redirect(303, '/client/roster');
  },

  reactivate: async ({ params, locals }) => {
    const companyId = locals.user!.companyId!;
    const person = await getPersonWithAllergens(pool, params.id, { includeInactive: true });
    if (!person || person.company_id !== companyId) error(403, 'Forbidden');
    if (person.active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updatePerson(pool, params.id, { active: true });
    return { action: 'reactivate', success: true };
  },
};
