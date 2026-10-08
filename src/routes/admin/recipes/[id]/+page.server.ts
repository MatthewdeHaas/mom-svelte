import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getRecipeWithIngredients,
  updateRecipe,
  deactivateRecipe,
  addRecipeIngredient,
  removeRecipeIngredient,
  getIngredients,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { ConflictError } from '$lib/server/errors';
import { parseFormData } from '$lib/server/form';
import { RecipeNameSchema, RecipeIngredientSchema } from '$lib/server/schemas/recipe';

export const load: PageServerLoad = async ({ params, locals }) => {
  const caller = callerFromLocals(locals);
  const [recipe, ingredients] = await Promise.all([
    getRecipeWithIngredients(params.id, { includeInactive: true }, caller),
    getIngredients({ includeInactive: false }, caller),
  ]);

  if (!recipe) error(404, 'Recipe not found');

  // Exclude ingredients already on the recipe from the add selector
  const usedIds = new Set(recipe.ingredients?.map((i: { ingredient_id: string }) => i.ingredient_id) ?? []);
  const availableIngredients = ingredients.filter(i => !usedIds.has(i.id));

  return { recipe, availableIngredients };
};

export const actions: Actions = {
  updateName: async ({ request, params, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(RecipeNameSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'updateName' as const,
        errors: parsed.errors as Record<string, string>,
        values: { name: form.get('name')?.toString() ?? '' } as Record<string, string>,
      });
    }

    const { name } = parsed.data;

    const caller = callerFromLocals(locals);
    try {
      await updateRecipe(params.id, { name }, caller);
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, {
          action: 'updateName' as const,
          errors: { name: 'A recipe with this name already exists' } as Record<string, string>,
          values: { name } as Record<string, string>,
        });
      }
      throw err;
    }

    return { action: 'updateName' as const, success: true };
  },

  updatePrice: async ({ request, params, locals }) => {
    const form = await request.formData();
    const rawPrice = form.get('price_per_meal')?.toString()?.trim() ?? '';
    const price_per_meal = rawPrice === '' ? null : parseFloat(rawPrice);

    if (rawPrice !== '' && (isNaN(price_per_meal!) || price_per_meal! < 0)) {
      return fail(422, {
        action: 'updatePrice' as const,
        errors: { price_per_meal: 'Must be a positive number' } as Record<string, string>,
        values: { price_per_meal: rawPrice } as Record<string, string>,
      });
    }

    const caller = callerFromLocals(locals);
    await updateRecipe(params.id, { price_per_meal }, caller);

    return { action: 'updatePrice' as const, success: true };
  },

  addIngredient: async ({ request, params, locals }) => {
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

    const caller = callerFromLocals(locals);
    await addRecipeIngredient(params.id, ingredient_id, amount, unit, caller);

    return { action: 'addIngredient', success: true };
  },

  removeIngredient: async ({ request, params, locals }) => {
    const form = await request.formData();
    const ingredient_id = form.get('ingredient_id')?.toString() ?? '';

    if (!ingredient_id) return fail(422, { action: 'removeIngredient', error: 'Missing ingredient_id' });

    const caller = callerFromLocals(locals);
    await removeRecipeIngredient(params.id, ingredient_id, caller);

    return { action: 'removeIngredient', success: true };
  },

  deactivate: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const recipe = await getRecipeWithIngredients(params.id, { includeInactive: true }, caller);
    if (!recipe) error(404, 'Recipe not found');
    if (!recipe.active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivateRecipe(params.id, caller);

    return { action: 'deactivate', success: true };
  },

  reactivate: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const recipe = await getRecipeWithIngredients(params.id, { includeInactive: true }, caller);
    if (!recipe) error(404, 'Recipe not found');
    if (recipe.active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updateRecipe(params.id, { active: true }, caller);

    return { action: 'reactivate', success: true };
  },
};
