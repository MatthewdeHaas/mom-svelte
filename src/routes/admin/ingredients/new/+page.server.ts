import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  createIngredient,
  setIngredientAllergens,
  getAllergens,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { IngredientSchema } from '$lib/server/schemas/ingredient';

export const load: PageServerLoad = async ({ locals }) => {
  const caller = callerFromLocals(locals);
  const allergens = await getAllergens(caller);
  return { allergens };
};

export const actions: Actions = {
  default: async ({ request, locals }) => {
    const form = await request.formData();
    const allergenIds = form.getAll('allergen_ids').map(v => v.toString());
    const parsed = parseFormData(IngredientSchema, form);

    if (!parsed.success) {
      return fail(422, {
        errors: parsed.errors,
        values: { name: form.get('name')?.toString() ?? '', allergenIds },
      });
    }

    const { name } = parsed.data;

    const caller = callerFromLocals(locals);
    let ingredient;
    try {
      ingredient = await createIngredient({ name, active: true }, caller);
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, {
          errors: { name: 'An ingredient with this name already exists' },
          values: { name, allergenIds },
        });
      }
      throw err;
    }

    if (allergenIds.length) {
      await setIngredientAllergens(ingredient.id, allergenIds, caller);
    }

    redirect(303, `/admin/ingredients/${ingredient.id}`);
  },
};
