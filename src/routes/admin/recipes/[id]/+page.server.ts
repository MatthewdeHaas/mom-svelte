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
import { parseFormData } from '$lib/server/form';
import { RecipeNameSchema, RecipeIngredientSchema } from '$lib/server/schemas/recipe';

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
    const parsed = parseFormData(RecipeNameSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'updateName',
        errors: parsed.errors,
        values: { name: form.get('name')?.toString() ?? '' },
      });
    }

    const { name } = parsed.data;

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
    const parsed = parseFormData(RecipeIngredientSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'addIngredient',
        errors: parsed.errors,
        values: Object.fromEntries(form),
      });
    }

    const { ingredient_id, amount, unit } = parsed.data;

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
