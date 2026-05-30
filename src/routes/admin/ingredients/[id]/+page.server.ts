import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getIngredientWithAllergens,
  updateIngredient,
  deactivateIngredient,
  setIngredientAllergens,
} from '$lib/server/queries/ingredients';
import { getAllergens } from '$lib/server/queries/allergens';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { IngredientSchema } from '$lib/server/schemas/ingredient';

export const load: PageServerLoad = async ({ params }) => {
  const [ingredient, allergens] = await Promise.all([
    getIngredientWithAllergens(pool, params.id, { includeInactive: true }),
    getAllergens(pool),
  ]);

  if (!ingredient) error(404, 'Ingredient not found');

  return { ingredient, allergens };
};

export const actions: Actions = {
  update: async ({ request, params }) => {
    const form = await request.formData();
    const allergenIds = form.getAll('allergen_ids').map(v => v.toString());
    const parsed = parseFormData(IngredientSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'update',
        errors: parsed.errors,
        values: { name: form.get('name')?.toString() ?? '', allergenIds },
      });
    }

    const { name } = parsed.data;

    try {
      await updateIngredient(pool, params.id, { name });
    } catch (err: any) {
      if (err instanceof ConflictError) {
        return fail(422, {
          action: 'update',
          errors: { name: 'An ingredient with this name already exists' },
          values: { name, allergenIds },
        });
      }
      throw err;
    }

    await setIngredientAllergens(pool, params.id, allergenIds);

    return { action: 'update', success: true };
  },

  deactivate: async ({ params }) => {
    const ingredient = await getIngredientWithAllergens(pool, params.id, { includeInactive: true });
    if (!ingredient) error(404, 'Ingredient not found');
    if (!ingredient.active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivateIngredient(pool, params.id);

    return { action: 'deactivate', success: true };
  },

  reactivate: async ({ params }) => {
    const ingredient = await getIngredientWithAllergens(pool, params.id, { includeInactive: true });
    if (!ingredient) error(404, 'Ingredient not found');
    if (ingredient.active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updateIngredient(pool, params.id, { active: true });

    return { action: 'reactivate', success: true };
  },
};
