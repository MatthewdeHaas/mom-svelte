import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getMenuById,
  updateMenu,
  deactivateMenu,
  getMenuRecipes,
  setMenuSlot,
  clearMenuSlot,
} from '$lib/server/queries/menus';
import { getRecipes } from '$lib/server/queries/recipes';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ params }) => {
  const [menu, slots, recipes] = await Promise.all([
    getMenuById(pool, params.id, { includeInactive: true }),
    getMenuRecipes(pool, params.id),
    getRecipes(pool, { includeInactive: false }),
  ]);

  if (!menu) error(404, 'Menu not found');

  return { menu, slots, recipes };
};

export const actions: Actions = {
  updateDetails: async ({ request, params }) => {
    const form = await request.formData();

    const name       = form.get('name')?.toString().trim() ?? '';
    const start_date = form.get('start_date')?.toString() || null;
    const end_date   = form.get('end_date')?.toString() || null;

    const errors: Record<string, string> = {};
    if (!name) errors.name = 'Menu name is required';
    if (start_date && end_date && new Date(start_date) > new Date(end_date)) {
      errors.end_date = 'End date must be on or after start date';
    }

    if (Object.keys(errors).length) {
      return fail(422, { action: 'updateDetails', errors, values: { name, start_date, end_date } });
    }

    await updateMenu(pool, params.id, {
      name,
      start_date: start_date ? new Date(start_date) : null,
      end_date:   end_date   ? new Date(end_date)   : null,
    });

    return { action: 'updateDetails', success: true };
  },

  assignSlot: async ({ request, params }) => {
    const form = await request.formData();

    const cycle_week    = parseInt(form.get('cycle_week')?.toString() ?? '');
    const day_of_week   = parseInt(form.get('day_of_week')?.toString() ?? '');
    const meal_category = form.get('meal_category')?.toString() ?? '';
    const recipe_id     = form.get('recipe_id')?.toString() ?? '';

    if (!recipe_id || isNaN(cycle_week) || isNaN(day_of_week) || !meal_category) {
      return fail(422, { action: 'assignSlot', error: 'Invalid slot data' });
    }

    await setMenuSlot(pool, {
      menu_id: params.id,
      cycle_week,
      day_of_week,
      meal_category,
      recipe_id,
    });

    return { action: 'assignSlot', success: true };
  },

  clearSlot: async ({ request, params }) => {
    const form = await request.formData();

    const cycle_week    = parseInt(form.get('cycle_week')?.toString() ?? '');
    const day_of_week   = parseInt(form.get('day_of_week')?.toString() ?? '');
    const meal_category = form.get('meal_category')?.toString() ?? '';

    if (isNaN(cycle_week) || isNaN(day_of_week) || !meal_category) {
      return fail(422, { action: 'clearSlot', error: 'Invalid slot data' });
    }

    await clearMenuSlot(pool, params.id, cycle_week, day_of_week, meal_category);

    return { action: 'clearSlot', success: true };
  },

  deactivate: async ({ params }) => {
    const menu = await getMenuById(pool, params.id, { includeInactive: true });
    if (!menu) error(404, 'Menu not found');
    if (!menu.is_active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivateMenu(pool, params.id);

    return { action: 'deactivate', success: true };
  },

  reactivate: async ({ params }) => {
    const menu = await getMenuById(pool, params.id, { includeInactive: true });
    if (!menu) error(404, 'Menu not found');
    if (menu.is_active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updateMenu(pool, params.id, { is_active: true });

    return { action: 'reactivate', success: true };
  },
};
