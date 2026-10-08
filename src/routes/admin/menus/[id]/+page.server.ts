import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getMenuById,
  updateMenu,
  deactivateMenu,
  getMenuRecipes,
  setMenuSlot,
  clearMenuSlot,
  getRecipes,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parseFormData } from '$lib/server/form';
import { MenuSchema, MenuSlotSchema, MenuClearSlotSchema } from '$lib/server/schemas/menu';
import type { DayOfWeek } from '$lib/types';

export const load: PageServerLoad = async ({ params, locals }) => {
  const caller = callerFromLocals(locals);
  const [menu, slots, recipes] = await Promise.all([
    getMenuById(params.id, { includeInactive: true }, caller),
    getMenuRecipes(params.id, caller),
    getRecipes({ includeInactive: false }, caller),
  ]);

  if (!menu) error(404, 'Menu not found');

  return { menu, slots, recipes };
};

export const actions: Actions = {
  updateDetails: async ({ request, params, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(MenuSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'updateDetails', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const { name, start_date, end_date } = parsed.data;

    const caller = callerFromLocals(locals);
    await updateMenu(params.id, {
      name,
      start_date: start_date ? new Date(start_date) : null,
      end_date:   end_date   ? new Date(end_date)   : null,
    }, caller);

    return { action: 'updateDetails', success: true };
  },

  assignSlot: async ({ request, params, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(MenuSlotSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'assignSlot', error: 'Invalid slot data' });
    }

    const { cycle_week, day_of_week, meal_category, recipe_id } = parsed.data;

    const caller = callerFromLocals(locals);
    await setMenuSlot({
      menu_id: params.id,
      cycle_week: cycle_week as 1 | 2 | 3 | 4,
      day_of_week: day_of_week as DayOfWeek,
      meal_category,
      recipe_id,
    }, caller);

    return { action: 'assignSlot', success: true };
  },

  clearSlot: async ({ request, params, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(MenuClearSlotSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'clearSlot', error: 'Invalid slot data' });
    }

    const { cycle_week, day_of_week, meal_category } = parsed.data;

    const caller = callerFromLocals(locals);
    await clearMenuSlot(params.id, cycle_week as 1 | 2 | 3 | 4, day_of_week as DayOfWeek, meal_category, caller);

    return { action: 'clearSlot', success: true };
  },

  deactivate: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const menu = await getMenuById(params.id, { includeInactive: true }, caller);
    if (!menu) error(404, 'Menu not found');
    if (!menu.is_active) return fail(422, { action: 'deactivate', error: 'Already inactive' });

    await deactivateMenu(params.id, caller);

    return { action: 'deactivate', success: true };
  },

  reactivate: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const menu = await getMenuById(params.id, { includeInactive: true }, caller);
    if (!menu) error(404, 'Menu not found');
    if (menu.is_active) return fail(422, { action: 'reactivate', error: 'Already active' });

    await updateMenu(params.id, { is_active: true }, caller);

    return { action: 'reactivate', success: true };
  },
};
