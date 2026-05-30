import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createIngredient, setIngredientAllergens } from '$lib/server/queries/ingredients';
import { getAllergens } from '$lib/server/queries/allergens';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { IngredientSchema } from '$lib/server/schemas/ingredient';

export const load: PageServerLoad = async () => {
  const allergens = await getAllergens(pool);
  return { allergens };
};

export const actions: Actions = {
  default: async ({ request }) => {
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

    let ingredient;
    try {
      ingredient = await createIngredient(pool, { name, active: true });
    } catch (err: any) {
      if (err instanceof ConflictError) {
        return fail(422, {
          errors: { name: 'An ingredient with this name already exists' },
          values: { name, allergenIds },
        });
      }
      throw err;
    }

    if (allergenIds.length) {
      await setIngredientAllergens(pool, ingredient.id, allergenIds);
    }

    redirect(303, `/admin/ingredients/${ingredient.id}`);
  },
};
