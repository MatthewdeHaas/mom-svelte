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
import { parseFormData } from '$lib/server/form';
import { MenuSchema, MenuSlotSchema, MenuClearSlotSchema } from '$lib/server/schemas/menu';
import type { DayOfWeek } from '$lib/types';

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
    const parsed = parseFormData(MenuSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'updateDetails', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const { name, start_date, end_date } = parsed.data;

    await updateMenu(pool, params.id, {
      name,
      start_date: start_date ? new Date(start_date) : null,
      end_date:   end_date   ? new Date(end_date)   : null,
    });

    return { action: 'updateDetails', success: true };
  },

  assignSlot: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(MenuSlotSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'assignSlot', error: 'Invalid slot data' });
    }

    const { cycle_week, day_of_week, meal_category, recipe_id } = parsed.data;

    await setMenuSlot(pool, {
      menu_id: params.id,
      cycle_week: cycle_week as 1 | 2 | 3 | 4,
      day_of_week: day_of_week as DayOfWeek,
      meal_category,
      recipe_id,
    });

    return { action: 'assignSlot', success: true };
  },

  clearSlot: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(MenuClearSlotSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'clearSlot', error: 'Invalid slot data' });
    }

    const { cycle_week, day_of_week, meal_category } = parsed.data;

    await clearMenuSlot(pool, params.id, cycle_week, day_of_week as DayOfWeek, meal_category);

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
