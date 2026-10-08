import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getAllergens,
  createAllergen,
  updateAllergen,
  deleteAllergen,
  getAllergenByName,
  getAllergenRefCount,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';

export const load: PageServerLoad = async ({ locals }) => {
  const caller = callerFromLocals(locals);
  const allergens = await getAllergens(caller);
  return { allergens };
};

export const actions: Actions = {
  create: async ({ request, locals }) => {
    const caller = callerFromLocals(locals);
    const form = await request.formData();
    const name = form.get('name')?.toString().trim() ?? '';

    if (!name) {
      return fail(422, { error: 'Name is required', action: 'create' as const });
    }

    const existing = await getAllergenByName(name, caller);
    if (existing) {
      return fail(422, { error: 'An allergen with this name already exists', action: 'create' as const });
    }

    await createAllergen({ name }, caller);
    return { success: true };
  },

  rename: async ({ request, locals }) => {
    const caller = callerFromLocals(locals);
    const form = await request.formData();
    const id = form.get('id')?.toString() ?? '';
    const name = form.get('name')?.toString().trim() ?? '';

    if (!name) {
      return fail(422, { error: 'Name is required', action: 'rename' as const, id });
    }

    const existing = await getAllergenByName(name, caller);
    if (existing && existing.id !== id) {
      return fail(422, { error: 'An allergen with this name already exists', action: 'rename' as const, id });
    }

    await updateAllergen(id, { name }, caller);
    return { success: true };
  },

  delete: async ({ request, locals }) => {
    const caller = callerFromLocals(locals);
    const form = await request.formData();
    const id = form.get('id')?.toString() ?? '';

    const refs = await getAllergenRefCount(id, caller);
    if (refs.ingredients > 0 || refs.persons > 0) {
      const parts: string[] = [];
      if (refs.ingredients > 0) parts.push(`${refs.ingredients} ingredient${refs.ingredients > 1 ? 's' : ''}`);
      if (refs.persons > 0) parts.push(`${refs.persons} person${refs.persons > 1 ? 's' : ''}`);
      return fail(422, {
        error: `Cannot delete: allergen is referenced by ${parts.join(' and ')}`,
        action: 'delete' as const,
        id,
      });
    }

    await deleteAllergen(id, caller);
    return { success: true };
  },
};
