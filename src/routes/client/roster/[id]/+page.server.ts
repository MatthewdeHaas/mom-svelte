import { fail, error, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import {
  getPersonWithAllergens,
  updatePerson,
  deactivatePerson,
  setPersonAllergens,
  getAllergens,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
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
  const caller = callerFromLocals(locals);

  const [person, allAllergens] = await Promise.all([
    getPersonWithAllergens(params.id, { includeInactive: true }, caller),
    getAllergens(caller),
  ]);

  if (!person) error(404, 'Person not found');
  // Ensure this person belongs to the authenticated client's company
  if (person.company_id !== companyId) error(403, 'Forbidden');

  return { person, allAllergens };
};

export const actions: Actions = {
  update: async ({ request, params, locals }) => {
    const companyId = locals.user!.companyId!;
    const caller = callerFromLocals(locals);
    const form = await request.formData();

    // Verify ownership before any write
    const person = await getPersonWithAllergens(params.id, { includeInactive: true }, caller);
    if (!person || person.company_id !== companyId) error(403, 'Forbidden');

    const parsed = parseFormData(UpdatePersonSchema, form);
    if (!parsed.success) {
      return fail(422, { action: 'update', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    // Allergens come as multiple values — getAll, not Object.fromEntries
    const allergenIds = form.getAll('allergen_ids').map(String).filter(Boolean);

    await Promise.all([
      updatePerson(params.id, { name: parsed.data.name }, caller),
      setPersonAllergens(params.id, allergenIds, caller),
    ]);

    return { action: 'update', success: true, name: parsed.data.name };
  },

  deactivate: async ({ params, locals }) => {
    const companyId = locals.user!.companyId!;
    const caller = callerFromLocals(locals);
    const person = await getPersonWithAllergens(params.id, { includeInactive: true }, caller);
    if (!person || person.company_id !== companyId) error(403, 'Forbidden');
    if (!person.active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivatePerson(params.id, caller);
    redirect(303, '/client/roster');
  },

  reactivate: async ({ params, locals }) => {
    const companyId = locals.user!.companyId!;
    const caller = callerFromLocals(locals);
    const person = await getPersonWithAllergens(params.id, { includeInactive: true }, caller);
    if (!person || person.company_id !== companyId) error(403, 'Forbidden');
    if (person.active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updatePerson(params.id, { active: true }, caller);
    return { action: 'reactivate', success: true };
  },
};
