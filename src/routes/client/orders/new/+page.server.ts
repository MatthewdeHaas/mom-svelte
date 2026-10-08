import { fail, redirect } from '@sveltejs/kit';
import { z } from 'zod';
import {
  createOrder,
  getMenusWithRecipes,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parseFormData } from '$lib/server/form';
import type { Actions, PageServerLoad } from './$types';

const ClientOrderSchema = z.object({
  menu_recipe_id: z.string().min(1),
  recipe_id: z.string().uuid('Invalid recipe'),
  serving_date: z
    .string()
    .regex(/^\d{4}-\d{2}-\d{2}$/, 'Invalid serving date')
    .refine((d) => d >= new Date().toLocaleDateString('en-CA'), { message: 'Serving date must be today or in the future' }),
  meal_category: z.string().min(1, 'Meal category is required'),
  amount: z.coerce
    .number({ error: 'Amount must be a positive number' })
    .int('Amount must be a whole number')
    .gt(0, 'Amount must be at least 1'),
  client_note: z
    .string()
    .transform((v) => v.trim() || null)
    .nullable()
    .optional()
    .transform((v) => v ?? null),
  storage: z
    .enum(['frozen', 'chilled', 'ambient'])
    .optional()
    .default('ambient'),
});

export const load: PageServerLoad = async ({ url, locals }) => {
  const menuRecipeId = url.searchParams.get('menu_recipe_id');
  const servingDate = url.searchParams.get('serving_date');
  const recipeName = url.searchParams.get('recipe_name') ?? '';
  const mealCategory = url.searchParams.get('meal_category') ?? '';

  if (!menuRecipeId || !servingDate) {
    redirect(302, '/client/menu');
  }

  // Compare as date strings (YYYY-MM-DD) so timezone offsets don't cause today's slots to be rejected.
  const todayStr = new Date().toLocaleDateString('en-CA'); // 'en-CA' gives YYYY-MM-DD in local time
  if (servingDate < todayStr) {
    redirect(302, '/client/menu');
  }

  // Resolve the actual recipe_id from the menu_recipe row
  const caller = callerFromLocals(locals);
  const menus = await getMenusWithRecipes({}, caller);
  let recipeId: string | null = null;
  for (const menu of menus) {
    const slot = menu.recipe.find((r) => r.id === menuRecipeId);
    if (slot) { recipeId = slot.recipe_id; break; }
  }

  if (!recipeId) {
    redirect(302, '/client/menu');
  }

  return { menuRecipeId, servingDate, recipeName, mealCategory, recipeId };
};

export const actions: Actions = {
  create: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const form = await request.formData();
    const parsed = parseFormData(ClientOrderSchema, form);

    if (!parsed.success) {
      return fail(422, { action: 'create', errors: parsed.errors, values: Object.fromEntries(form) });
    }

    const { recipe_id, serving_date, meal_category, amount, client_note, storage } = parsed.data;

    const caller = callerFromLocals(locals);
    await createOrder(
      {
        company_id: companyId,
        recipe_id,
        created_by: locals.user!.id,
        order_source: 'client_portal',
        status: 'pending',
        meal_category,
        amount,
        serving_date,
        shipment_date: serving_date, // admin adjusts shipment date later
        frozen: storage === 'frozen',
        chilled: storage === 'chilled',
        client: client_note,
      },
      caller,
    );

    redirect(303, '/client/orders');
  },
};
