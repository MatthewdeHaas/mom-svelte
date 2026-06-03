import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getAllergens,
  createAllergen,
  updateAllergen,
  deleteAllergen,
  getAllergenByName,
  getAllergenRefCount,
} from '$lib/server/queries/allergens';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async () => {
  const allergens = await getAllergens(pool);
  return { allergens };
};

export const actions: Actions = {
  create: async ({ request }) => {
    const form = await request.formData();
    const name = form.get('name')?.toString().trim() ?? '';

    if (!name) {
      return fail(422, { error: 'Name is required', action: 'create' as const });
    }

    const existing = await getAllergenByName(pool, name);
    if (existing) {
      return fail(422, { error: 'An allergen with this name already exists', action: 'create' as const });
    }

    await createAllergen(pool, { name });
    return { success: true };
  },

  rename: async ({ request }) => {
    const form = await request.formData();
    const id = form.get('id')?.toString() ?? '';
    const name = form.get('name')?.toString().trim() ?? '';

    if (!name) {
      return fail(422, { error: 'Name is required', action: 'rename' as const, id });
    }

    const existing = await getAllergenByName(pool, name);
    if (existing && existing.id !== id) {
      return fail(422, { error: 'An allergen with this name already exists', action: 'rename' as const, id });
    }

    await updateAllergen(pool, id, { name });
    return { success: true };
  },

  delete: async ({ request }) => {
    const form = await request.formData();
    const id = form.get('id')?.toString() ?? '';

    const refs = await getAllergenRefCount(pool, id);
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

    await deleteAllergen(pool, id);
    return { success: true };
  },
};
