import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getMenus,
  getMenuById,
  getMenuWithRecipes,
  getMenusWithRecipes,
  getMenuRecipes,
  getActiveMenuForDate,
  getMenuSlot,
  createMenu,
  updateMenu,
  deactivateMenu,
  setMenuSlot,
  clearMenuSlot,
  setAllMenuSlots,
} from '$lib/server/queries/menus';
import { createRecipe } from '$lib/server/queries/recipes';
import { createTestContext } from './helpers';
import type { InsertMenu, InsertMenuRecipe } from '$lib/types';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let counter = 0;
function uniqueName(prefix = 'Menu') {
  return `${prefix}_${++counter}_${Date.now()}`;
}

async function seedRecipe() {
  return createRecipe(ctx.client, { name: uniqueName('Recipe') });
}

async function seedMenu(overrides: Partial<InsertMenu> = {}) {
  return createMenu(ctx.client, {
    name: uniqueName(),
    start_date: new Date('2025-06-01'),
    end_date: new Date('2025-06-30'),
    ...overrides,
  });
}

async function seedSlot(overrides: Partial<InsertMenuRecipe> & { menu_id: string; recipe_id: string }): Promise<ReturnType<typeof setMenuSlot>> {
  return setMenuSlot(ctx.client, {
    cycle_week: 1,
    day_of_week: 'monday',
    meal_category: 'lunch',
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// getMenus
// ---------------------------------------------------------------------------

describe('getMenus', () => {
  it('returns an empty array when no menus exist', async () => {
    expect(await getMenus(ctx.client)).toEqual([]);
  });

  it('excludes inactive menus by default', async () => {
    const active = await seedMenu();
    const inactive = await seedMenu();
    await deactivateMenu(ctx.client, inactive.id);

    const ids = (await getMenus(ctx.client)).map((m) => m.id);

    expect(ids).toContain(active.id);
    expect(ids).not.toContain(inactive.id);
  });

  it('includes inactive menus when includeInactive is true', async () => {
    const inactive = await seedMenu();
    await deactivateMenu(ctx.client, inactive.id);

    const ids = (await getMenus(ctx.client, { includeInactive: true })).map((m) => m.id);
    expect(ids).toContain(inactive.id);
  });

  it('returns rows ordered by start_date DESC', async () => {
    await seedMenu({ name: uniqueName('Early'), start_date: new Date('2025-01-01'), end_date: new Date('2025-01-31') });
    await seedMenu({ name: uniqueName('Late'), start_date: new Date('2025-12-01'), end_date: new Date('2025-12-31') });

    const rows = await getMenus(ctx.client);
    const dates = rows.map((m) => m.start_date);

    expect(dates[0] >= dates[1]).toBe(true);
  });

  it('returns rows with the expected shape', async () => {
    const m = await seedMenu();
    const rows = await getMenus(ctx.client);
    const found = rows.find((r) => r.id === m.id);

    expect(found).toBeDefined();
    expect(typeof found!.id).toBe('string');
    expect(found!.is_active).toBe(true);
    expect(found!.start_date).toBeInstanceOf(Date);
    expect(found!.end_date).toBeInstanceOf(Date);
    expect(found!.created_at).toBeInstanceOf(Date);
  });
});

// ---------------------------------------------------------------------------
// getMenuById
// ---------------------------------------------------------------------------

describe('getMenuById', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getMenuById(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns null for an inactive menu by default', async () => {
    const m = await seedMenu();
    await deactivateMenu(ctx.client, m.id);

    expect(await getMenuById(ctx.client, m.id)).toBeNull();
  });

  it('returns inactive menu when includeInactive is true', async () => {
    const m = await seedMenu();
    await deactivateMenu(ctx.client, m.id);

    const found = await getMenuById(ctx.client, m.id, { includeInactive: true });
    expect(found).not.toBeNull();
    expect(found!.is_active).toBe(false);
  });

  it('returns the correct menu', async () => {
    const m = await seedMenu({ name: 'Specific Menu' });

    const found = await getMenuById(ctx.client, m.id);
    expect(found!.id).toBe(m.id);
    expect(found!.name).toBe('Specific Menu');
  });
});

// ---------------------------------------------------------------------------
// getMenuRecipes
// ---------------------------------------------------------------------------

describe('getMenuRecipes', () => {
  it('returns an empty array for a menu with no slots', async () => {
    const m = await seedMenu();
    expect(await getMenuRecipes(ctx.client, m.id)).toEqual([]);
  });

  it('returns slots with recipe_name and recipe_active', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await seedSlot({ menu_id: m.id, recipe_id: r.id });

    const slots = await getMenuRecipes(ctx.client, m.id);

    expect(slots).toHaveLength(1);
    expect(slots[0].recipe_name).toBe(r.name);
    expect(slots[0].recipe_active).toBe(true);
  });

  it('returns slots ordered by cycle_week, day_of_week, meal_category', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r.id, cycle_week: 2, day_of_week: 'wednesday', meal_category: 'dinner' });
    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' });

    const slots = await getMenuRecipes(ctx.client, m.id);

    expect(slots[0].cycle_week).toBe(1);
    expect(slots[1].cycle_week).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// getMenuWithRecipes
// ---------------------------------------------------------------------------

describe('getMenuWithRecipes', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getMenuWithRecipes(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns the menu with an empty recipes array when no slots set', async () => {
    const m = await seedMenu();
    const found = await getMenuWithRecipes(ctx.client, m.id);

    expect(found).not.toBeNull();
    expect(found!.recipes).toEqual([]);
  });

  it('returns the menu with its recipe slots', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await seedSlot({ menu_id: m.id, recipe_id: r.id });

    const found = await getMenuWithRecipes(ctx.client, m.id);
    expect(found!.recipes).toHaveLength(1);
    expect(found!.recipes[0].recipe_id).toBe(r.id);
  });
});

// ---------------------------------------------------------------------------
// getMenusWithRecipes
// ---------------------------------------------------------------------------

describe('getMenusWithRecipes', () => {
  it('returns an empty array when no menus exist', async () => {
    expect(await getMenusWithRecipes(ctx.client)).toEqual([]);
  });

  it('returns menus each with a recipes array', async () => {
    await seedMenu();
    const rows = await getMenusWithRecipes(ctx.client);

    expect(rows.every((m) => Array.isArray(m.recipes))).toBe(true);
  });

  it('correctly attaches slots to each menu without cross-contamination', async () => {
    const m1 = await seedMenu();
    const m2 = await seedMenu();
    const r1 = await seedRecipe();
    const r2 = await seedRecipe();

    await seedSlot({ menu_id: m1.id, recipe_id: r1.id });
    await setMenuSlot(ctx.client, { menu_id: m2.id, recipe_id: r2.id, cycle_week: 1, day_of_week: 'tuesday', meal_category: 'lunch' });

    const rows = await getMenusWithRecipes(ctx.client);
    const row1 = rows.find((m) => m.id === m1.id);
    const row2 = rows.find((m) => m.id === m2.id);

    expect(row1!.recipes.some((s) => s.recipe_id === r1.id)).toBe(true);
    expect(row2!.recipes.some((s) => s.recipe_id === r2.id)).toBe(true);
    expect(row1!.recipes.some((s) => s.recipe_id === r2.id)).toBe(false);
  });

  it('excludes inactive menus by default', async () => {
    const inactive = await seedMenu();
    await deactivateMenu(ctx.client, inactive.id);

    const rows = await getMenusWithRecipes(ctx.client);
    expect(rows.some((m) => m.id === inactive.id)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// getActiveMenuForDate
// ---------------------------------------------------------------------------

describe('getActiveMenuForDate', () => {
  it('returns null when no menu covers the date', async () => {
    expect(
      await getActiveMenuForDate(ctx.client, new Date('2025-06-15'))
    ).toBeNull();
  });

  it('returns a menu whose date range covers the given date', async () => {
    const m = await seedMenu({
      start_date: new Date('2025-06-01'),
      end_date: new Date('2025-06-30'),
    });

    const found = await getActiveMenuForDate(ctx.client, new Date('2025-06-15'));
    expect(found!.id).toBe(m.id);
  });

  it('returns null when the menu is inactive even if the date is in range', async () => {
    const m = await seedMenu({
      start_date: new Date('2025-06-01'),
      end_date: new Date('2025-06-30'),
    });
    await deactivateMenu(ctx.client, m.id);

    expect(
      await getActiveMenuForDate(ctx.client, new Date('2025-06-15'))
    ).toBeNull();
  });

  it('returns null for a date before the menu start_date', async () => {
    await seedMenu({
      start_date: new Date('2025-06-01'),
      end_date: new Date('2025-06-30'),
    });

    expect(
      await getActiveMenuForDate(ctx.client, new Date('2025-05-31'))
    ).toBeNull();
  });

  it('returns null for a date after the menu end_date', async () => {
    await seedMenu({
      start_date: new Date('2025-06-01'),
      end_date: new Date('2025-06-30'),
    });

    expect(
      await getActiveMenuForDate(ctx.client, new Date('2025-07-01'))
    ).toBeNull();
  });

  it('matches on start_date and end_date boundaries exactly', async () => {
    const m = await seedMenu({
      start_date: new Date('2025-06-01'),
      end_date: new Date('2025-06-30'),
    });

    expect((await getActiveMenuForDate(ctx.client, new Date('2025-06-01')))!.id).toBe(m.id);
    expect((await getActiveMenuForDate(ctx.client, new Date('2025-06-30')))!.id).toBe(m.id);
  });
});

// ---------------------------------------------------------------------------
// getMenuSlot
// ---------------------------------------------------------------------------

describe('getMenuSlot', () => {
  it('returns null for a non-existent slot', async () => {
    const m = await seedMenu();
    expect(
      await getMenuSlot(ctx.client, m.id, 1, 'monday', 'lunch')
    ).toBeNull();
  });

  it('returns the correct slot', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await setMenuSlot(ctx.client, {
      menu_id: m.id,
      recipe_id: r.id,
      cycle_week: 2,
      day_of_week: 'friday',
      meal_category: 'dinner',
    });

    const found = await getMenuSlot(ctx.client, m.id, 2, 'friday', 'dinner');

    expect(found).not.toBeNull();
    expect(found!.recipe_id).toBe(r.id);
    expect(found!.recipe_name).toBe(r.name);
  });

  it('does not return a slot for a different day', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await setMenuSlot(ctx.client, {
      menu_id: m.id,
      recipe_id: r.id,
      cycle_week: 1,
      day_of_week: 'monday',
      meal_category: 'lunch',
    });

    expect(
      await getMenuSlot(ctx.client, m.id, 1, 'tuesday', 'lunch')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// createMenu
// ---------------------------------------------------------------------------

describe('createMenu', () => {
  it('returns the inserted menu with a string id', async () => {
    const m = await seedMenu({ name: 'New Menu' });

    expect(typeof m.id).toBe('string');
    expect(m.name).toBe('New Menu');
    expect(m.is_active).toBe(true);
    expect(m.created_at).toBeInstanceOf(Date);
  });

  it('trims whitespace from the name', async () => {
    const m = await createMenu(ctx.client, {
      name: '  Trimmed  ',
      start_date: new Date('2025-06-01'),
      end_date: new Date('2025-06-30'),
    });
    expect(m.name).toBe('Trimmed');
  });

  it('respects explicit is_active: false', async () => {
    const m = await seedMenu({ is_active: false });
    expect(m.is_active).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// updateMenu
// ---------------------------------------------------------------------------

describe('updateMenu', () => {
  it('updates name only, leaving other fields unchanged', async () => {
    const m = await seedMenu();
    const updated = await updateMenu(ctx.client, m.id, { name: uniqueName('Updated') });

    expect(updated!.is_active).toBe(true);
    expect(updated!.start_date).toEqual(m.start_date);
  });

  it('updates is_active only', async () => {
    const m = await seedMenu();
    const updated = await updateMenu(ctx.client, m.id, { is_active: false });

    expect(updated!.is_active).toBe(false);
    expect(updated!.name).toBe(m.name);
  });

  it('trims whitespace from updated name', async () => {
    const m = await seedMenu();
    const updated = await updateMenu(ctx.client, m.id, { name: '  Trimmed  ' });
    expect(updated!.name).toBe('Trimmed');
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await updateMenu(ctx.client, '00000000-0000-0000-0000-000000000000', { name: 'Ghost' })
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// deactivateMenu
// ---------------------------------------------------------------------------

describe('deactivateMenu', () => {
  it('sets is_active to false and returns the row', async () => {
    const m = await seedMenu();
    const result = await deactivateMenu(ctx.client, m.id);

    expect(result!.is_active).toBe(false);
  });

  it('deactivated menu is excluded from getMenus by default', async () => {
    const m = await seedMenu();
    await deactivateMenu(ctx.client, m.id);

    const ids = (await getMenus(ctx.client)).map((r) => r.id);
    expect(ids).not.toContain(m.id);
  });

  it('deactivated menu is still retrievable with includeInactive', async () => {
    const m = await seedMenu();
    await deactivateMenu(ctx.client, m.id);

    const found = await getMenuById(ctx.client, m.id, { includeInactive: true });
    expect(found!.is_active).toBe(false);
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await deactivateMenu(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// setMenuSlot
// ---------------------------------------------------------------------------

describe('setMenuSlot', () => {
  it('inserts a new slot and returns it', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    const slot = await setMenuSlot(ctx.client, {
      menu_id: m.id,
      recipe_id: r.id,
      cycle_week: 1,
      day_of_week: 'monday',
      meal_category: 'lunch',
    });

    expect(typeof slot.id).toBe('string');
    expect(slot.recipe_id).toBe(r.id);
  });

  it('replaces the recipe when the same slot is set again (upsert)', async () => {
    const m = await seedMenu();
    const r1 = await seedRecipe();
    const r2 = await seedRecipe();

    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r1.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' });
    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r2.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' });

    const slots = await getMenuRecipes(ctx.client, m.id);

    expect(slots).toHaveLength(1);
    expect(slots[0].recipe_id).toBe(r2.id);
  });

  it('allows different slots for the same menu', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' });
    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r.id, cycle_week: 1, day_of_week: 'tuesday', meal_category: 'lunch' });

    expect(await getMenuRecipes(ctx.client, m.id)).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// clearMenuSlot
// ---------------------------------------------------------------------------

describe('clearMenuSlot', () => {
  it('removes an existing slot and returns true', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await seedSlot({ menu_id: m.id, recipe_id: r.id });
    const result = await clearMenuSlot(ctx.client, m.id, 1, 'monday', 'lunch');

    expect(result).toBe(true);
    expect(await getMenuRecipes(ctx.client, m.id)).toHaveLength(0);
  });

  it('returns false for a non-existent slot', async () => {
    const m = await seedMenu();
    expect(
      await clearMenuSlot(ctx.client, m.id, 1, 'monday', 'lunch')
    ).toBe(false);
  });

  it('does not affect other slots on the same menu', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' });
    await setMenuSlot(ctx.client, { menu_id: m.id, recipe_id: r.id, cycle_week: 1, day_of_week: 'tuesday', meal_category: 'lunch' });

    await clearMenuSlot(ctx.client, m.id, 1, 'monday', 'lunch');

    const slots = await getMenuRecipes(ctx.client, m.id);
    expect(slots).toHaveLength(1);
    expect(slots[0].day_of_week).toBe('tuesday');
  });
});

// ---------------------------------------------------------------------------
// setAllMenuSlots
// ---------------------------------------------------------------------------

describe('setAllMenuSlots', () => {
  it('sets all slots and they appear on the menu', async () => {
    const m = await seedMenu();
    const r1 = await seedRecipe();
    const r2 = await seedRecipe();

    await setAllMenuSlots(ctx.client, m.id, [
      { menu_id: m.id, recipe_id: r1.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' },
      { menu_id: m.id, recipe_id: r2.id, cycle_week: 1, day_of_week: 'tuesday', meal_category: 'lunch' },
    ]);

    const slots = await getMenuRecipes(ctx.client, m.id);
    const ids = slots.map((s) => s.recipe_id);

    expect(ids).toContain(r1.id);
    expect(ids).toContain(r2.id);
  });

  it('replaces existing slots on a second call', async () => {
    const m = await seedMenu();
    const old = await seedRecipe();
    const next = await seedRecipe();

    await setAllMenuSlots(ctx.client, m.id, [
      { menu_id: m.id, recipe_id: old.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' },
    ]);
    await setAllMenuSlots(ctx.client, m.id, [
      { menu_id: m.id, recipe_id: next.id, cycle_week: 1, day_of_week: 'wednesday', meal_category: 'dinner' },
    ]);

    const slots = await getMenuRecipes(ctx.client, m.id);
    const ids = slots.map((s) => s.recipe_id);

    expect(ids).not.toContain(old.id);
    expect(ids).toContain(next.id);
  });

  it('clears all slots when called with an empty array', async () => {
    const m = await seedMenu();
    const r = await seedRecipe();

    await setAllMenuSlots(ctx.client, m.id, [
      { menu_id: m.id, recipe_id: r.id, cycle_week: 1, day_of_week: 'monday', meal_category: 'lunch' },
    ]);
    await setAllMenuSlots(ctx.client, m.id, []);

    expect(await getMenuRecipes(ctx.client, m.id)).toHaveLength(0);
  });
});
