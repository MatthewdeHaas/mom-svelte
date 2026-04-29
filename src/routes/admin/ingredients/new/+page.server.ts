import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createIngredient, setIngredientAllergens } from '$lib/server/queries/ingredients';
import { getAllergens } from '$lib/server/queries/allergens';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async () => {
  const allergens = await getAllergens(pool);
  return { allergens };
};

export const actions: Actions = {
  default: async ({ request }) => {
    const form = await request.formData();

    const name = form.get('name')?.toString().trim() ?? '';
    const allergenIds = form.getAll('allergen_ids').map(v => v.toString());

    if (!name) {
      return fail(422, {
        errors: { name: 'Ingredient name is required' },
        values: { name, allergenIds },
      });
    }

    let ingredient;
    try {
      ingredient = await createIngredient(pool, { name, active: true });
    } catch (err: any) {
      if (err?.code === '23505') {
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
