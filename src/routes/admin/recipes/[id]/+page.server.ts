import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getRecipeWithIngredients,
  updateRecipe,
  deactivateRecipe,
  addRecipeIngredient,
  removeRecipeIngredient,
} from '$lib/server/queries/recipes';
import { getIngredients } from '$lib/server/queries/ingredients';
import { pool } from '$lib/server/db';
import { ConflictError } from '$lib/server/errors';

export const load: PageServerLoad = async ({ params }) => {
  const [recipe, ingredients] = await Promise.all([
    getRecipeWithIngredients(pool, params.id, { includeInactive: true }),
    getIngredients(pool, { includeInactive: false }),
  ]);

  if (!recipe) error(404, 'Recipe not found');

  // Exclude ingredients already on the recipe from the add selector
  const usedIds = new Set(recipe.ingredients?.map((i: { ingredient_id: string }) => i.ingredient_id) ?? []);
  const availableIngredients = ingredients.filter(i => !usedIds.has(i.id));

  return { recipe, availableIngredients };
};

export const actions: Actions = {
  updateName: async ({ request, params }) => {
    const form = await request.formData();
    const name = form.get('name')?.toString().trim() ?? '';

    if (!name) {
      return fail(422, {
        action: 'updateName',
        errors: { name: 'Recipe name is required' },
        values: { name },
      });
    }

    try {
      await updateRecipe(pool, params.id, { name });
    } catch (err: any) {
      if (err instanceof ConflictError) {
        return fail(422, {
          action: 'updateName',
          errors: { name: 'A recipe with this name already exists' },
          values: { name },
        });
      }
      throw err;
    }

    return { action: 'updateName', success: true };
  },

  addIngredient: async ({ request, params }) => {
    const form = await request.formData();

    const ingredient_id = form.get('ingredient_id')?.toString() ?? '';
    const amount        = parseFloat(form.get('amount')?.toString() ?? '');
    const unit          = form.get('unit')?.toString().trim() ?? '';

    const errors: Record<string, string> = {};
    if (!ingredient_id) errors.ingredient_id = 'Please select an ingredient';
    if (isNaN(amount) || amount <= 0) errors.amount = 'Amount must be a positive number';
    if (!unit) errors.unit = 'Unit is required';

    if (Object.keys(errors).length) {
      return fail(422, {
        action: 'addIngredient',
        errors,
        values: { ingredient_id, amount, unit },
      });
    }

    await addRecipeIngredient(pool, params.id, ingredient_id, amount, unit);

    return { action: 'addIngredient', success: true };
  },

  removeIngredient: async ({ request, params }) => {
    const form = await request.formData();
    const ingredient_id = form.get('ingredient_id')?.toString() ?? '';

    if (!ingredient_id) return fail(422, { action: 'removeIngredient', error: 'Missing ingredient_id' });

    await removeRecipeIngredient(pool, params.id, ingredient_id);

    return { action: 'removeIngredient', success: true };
  },

  deactivate: async ({ params }) => {
    const recipe = await getRecipeWithIngredients(pool, params.id, { includeInactive: true });
    if (!recipe) error(404, 'Recipe not found');
    if (!recipe.active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivateRecipe(pool, params.id);

    return { action: 'deactivate', success: true };
  },

  reactivate: async ({ params }) => {
    const recipe = await getRecipeWithIngredients(pool, params.id, { includeInactive: true });
    if (!recipe) error(404, 'Recipe not found');
    if (recipe.active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updateRecipe(pool, params.id, { active: true });

    return { action: 'reactivate', success: true };
  },
};
