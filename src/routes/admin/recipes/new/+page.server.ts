import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createRecipe } from '$lib/server/queries/recipes';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  default: async ({ request }) => {
    const form = await request.formData();
    const name = form.get('name')?.toString().trim() ?? '';

    if (!name) {
      return fail(422, {
        errors: { name: 'Recipe name is required' },
        values: { name },
      });
    }

    let recipe;
    try {
      recipe = await createRecipe(pool, { name, active: true });
    } catch (err: any) {
      if (err?.code === '23505') {
        return fail(422, {
          errors: { name: 'A recipe with this name already exists' },
          values: { name },
        });
      }
      throw err;
    }

    redirect(303, `/admin/recipes/${recipe.id}`);
  },
};
