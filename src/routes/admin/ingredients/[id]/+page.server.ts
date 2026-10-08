import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getIngredientWithAllergens,
  updateIngredient,
  deactivateIngredient,
  setIngredientAllergens,
  getAllergens,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { IngredientSchema } from '$lib/server/schemas/ingredient';

export const load: PageServerLoad = async ({ params, locals }) => {
  const caller = callerFromLocals(locals);
  const [ingredient, allergens] = await Promise.all([
    getIngredientWithAllergens(params.id, { includeInactive: true }, caller),
    getAllergens(caller),
  ]);

  if (!ingredient) error(404, 'Ingredient not found');

  return { ingredient, allergens };
};

export const actions: Actions = {
  update: async ({ request, params, locals }) => {
    const form = await request.formData();
    const allergenIds = form.getAll('allergen_ids').map(v => v.toString());
    const parsed = parseFormData(IngredientSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'update' as const,
        errors: parsed.errors as Record<string, string>,
        values: { name: form.get('name')?.toString() ?? '', allergenIds },
      });
    }

    const { name } = parsed.data;

    // Parse cost_per_unit — empty string means clear (set to null)
    const rawCost = form.get('cost_per_unit')?.toString()?.trim() ?? '';
    const cost_per_unit = rawCost === '' ? null : parseFloat(rawCost);
    if (rawCost !== '' && (isNaN(cost_per_unit!) || cost_per_unit! < 0)) {
      return fail(422, {
        action: 'update' as const,
        errors: { cost_per_unit: 'Must be a positive number' } as Record<string, string>,
        values: { name, allergenIds },
      });
    }

    const caller = callerFromLocals(locals);
    try {
      await updateIngredient(params.id, { name, cost_per_unit }, caller);
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, {
          action: 'update' as const,
          errors: { name: 'An ingredient with this name already exists' } as Record<string, string>,
          values: { name, allergenIds },
        });
      }
      throw err;
    }

    await setIngredientAllergens(params.id, allergenIds, caller);

    return { action: 'update' as const, success: true };
  },

  deactivate: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const ingredient = await getIngredientWithAllergens(params.id, { includeInactive: true }, caller);
    if (!ingredient) error(404, 'Ingredient not found');
    if (!ingredient.active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivateIngredient(params.id, caller);

    return { action: 'deactivate', success: true };
  },

  reactivate: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const ingredient = await getIngredientWithAllergens(params.id, { includeInactive: true }, caller);
    if (!ingredient) error(404, 'Ingredient not found');
    if (ingredient.active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updateIngredient(params.id, { active: true }, caller);

    return { action: 'reactivate', success: true };
  },
};
