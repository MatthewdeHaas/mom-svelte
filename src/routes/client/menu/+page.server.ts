import {
  getMenusWithRecipes,
  getOrders,
  getPersonsWithAllergensByCompany,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import type { MenuWithRecipes, PersonWithAllergens, OrderSummary, DayOfWeek } from '$lib/types';
import type { PageServerLoad } from './$types';

// The menu uses a cycle_week (1–4) + day_of_week model.
// Given a menu's start_date and a target calendar date, compute
// which cycle_week that date falls into.
function getCycleWeek(startDate: Date, targetDate: Date): 1 | 2 | 3 | 4 {
  const msPerDay = 86400000;
  const diffDays = Math.floor((targetDate.getTime() - startDate.getTime()) / msPerDay);
  // 0-indexed week within the 4-week cycle
  const weekIndex = Math.floor(diffDays / 7) % 4;
  return (weekIndex + 1) as 1 | 2 | 3 | 4;
}

const DAY_NAMES: DayOfWeek[] = [
  'sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday',
];

export interface MenuSlotForDate {
  date: string;          // YYYY-MM-DD
  dayLabel: string;      // "Mon", "Tue", etc.
  cycleWeek: 1 | 2 | 3 | 4;
  dayOfWeek: DayOfWeek;
  slots: {
    menuRecipeId: string;
    recipeId: string;
    recipeName: string;
    mealCategory: string;
    // allergen names from this company's persons that match this recipe
    allergenWarnings: string[];
    alreadyOrdered: boolean;
  }[];
}

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;
  const caller = callerFromLocals(locals);

  const today = new Date();
  today.setHours(0, 0, 0, 0);

  // Look 4 weeks ahead
  const horizon = new Date(today.getTime() + 28 * 86400000);

  const [menus, persons, existingOrders] = await Promise.all([
    getMenusWithRecipes({}, caller),
    getPersonsWithAllergensByCompany(companyId, {}, caller),
    getOrders(
      {
        company_id: companyId,
        serving_date_from: today,
        serving_date_to: horizon,
      },
      caller,
    ),
  ]);

  // Build a set of "recipe_id|serving_date" already ordered so we can mark slots
  const orderedKeys = new Set(
    existingOrders
      .filter((o) => o.status !== 'cancelled')
      .map((o) => `${o.recipe_id}|${formatDate(o.serving_date)}`)
  );

  // Collect all allergen names across the company's roster
  const rosterAllergenNames = new Set<string>();
  for (const p of persons) {
    for (const a of p.allergen) {
      rosterAllergenNames.add(a.name);
    }
  }

  // Build calendar: iterate each day from today to horizon
  const calendarDays: MenuSlotForDate[] = [];

  for (let d = new Date(today); d <= horizon; d = new Date(d.getTime() + 86400000)) {
    const dow = DAY_NAMES[d.getDay()];
    const dateStr = formatDate(d);

    // Find the active menu covering this date
    const menu = menus.find(
      (m) =>
        m.is_active &&
        m.start_date &&
        m.end_date &&
        new Date(m.start_date) <= d &&
        new Date(m.end_date) >= d
    );

    if (!menu) continue;

    const cycleWeek = getCycleWeek(new Date(menu.start_date!), d);

    // Find recipe slots for this day + cycle_week
    const daySlots = menu.recipe.filter(
      (r) => r.cycle_week === cycleWeek && r.day_of_week === dow
    );

    if (daySlots.length === 0) continue;

    calendarDays.push({
      date: dateStr,
      dayLabel: d.toLocaleDateString('en-CA', { weekday: 'short', month: 'short', day: 'numeric' }),
      cycleWeek,
      dayOfWeek: dow,
      slots: daySlots.map((slot) => ({
        menuRecipeId: slot.id,
        recipeId: slot.recipe_id,
        recipeName: slot.recipe_name,
        mealCategory: slot.meal_category,
        allergenWarnings: [], // recipes don't carry allergens directly — stub for now
        alreadyOrdered: orderedKeys.has(`${slot.recipe_id}|${dateStr}`),
      })),
    });
  }

  return { calendarDays, persons };
};

function formatDate(d: Date | string): string {
  // Always format in UTC to avoid timezone shifts when pg returns dates as
  // midnight UTC and we extract year/month/day in a negative-offset timezone.
  const dt = new Date(d);
  return dt.toISOString().slice(0, 10);
}
