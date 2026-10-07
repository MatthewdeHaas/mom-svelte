import type { Pool, PoolClient } from 'pg';
import type {
  Menu,
  InsertMenu,
  UpdateMenu,
  MenuRecipe,
  InsertMenuRecipe,
  MenuRecipeWithName,
  MenuWithRecipes,
  DayOfWeek,
} from '$lib/types';
import type { Db } from '$lib/server/db';
import {
  useRustBackend,
  internalJson,
  isInternalApiError,
  filtersToQueryString,
  normalizeBody,
  type InternalCaller,
} from './_rust-dispatch';

// -------------------------------------------------------------
// RUST DISPATCH
//
// Every exported function below takes an optional `caller`. When
// passed AND `use_rust_backend_menus` is on for that caller, the
// call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — threading
// `caller` into `src/routes/admin/menus/*` and the client menu view
// is a follow-up pass per the recipes precedent.
//
// Menus are GLOBAL master data (no company scoping). Reads are
// unrestricted (clients consume them for the menu view and new-order
// picker); every mutation returns 403 for client callers. The Rust
// handlers enforce this; the dispatcher only needs to forward.
//
// Wire-shape normalisation:
//  - `Menu` carries `start_date` / `end_date` / `created_at` which the
//    pg driver maps to `Date`. Rust emits them as ISO strings
//    (`YYYY-MM-DD` for the two dates, full ISO-8601 for the timestamp).
//    `tests/integration/core/menus.test.ts` asserts
//    `instanceof Date` on all three — so we rehydrate every row coming
//    off the wire before returning it.
//  - `MenuWithRecipes` nests slot rows under `recipe`, but those rows
//    are pure strings / numbers / UUIDs — no Date fields to hydrate.
//    Only the menu-level dates need rehydration.
//  - Date filters / body dates on create/update are sent as ISO
//    date-only via `normalizeBody` to keep the Rust `NaiveDate` parser
//    happy. The `getActiveMenuForDate` query param goes through
//    `filtersToQueryString` which produces the same ISO format.
// -------------------------------------------------------------

/**
 * 404-aware wrapper. The TS SQL path returns `null` on miss for
 * single-row reads/mutations; the Rust path returns 404. Translate so
 * the dispatcher is behaviourally transparent.
 */
async function internalJsonOrNull<T>(
  path: string,
  caller: InternalCaller,
  opts?: Parameters<typeof internalJson>[2]
): Promise<T | null> {
  try {
    return await internalJson<T>(path, caller, opts);
  } catch (err) {
    if (isInternalApiError(err) && err.status === 404) return null;
    throw err;
  }
}

/** Narrow a wire string to a `Date`. Idempotent for `Date` inputs. */
function isoToDate(iso: unknown): Date {
  if (iso instanceof Date) return iso;
  return new Date(iso as string);
}

/**
 * Rehydrate the three Date fields on a menu row. Matches the pg
 * driver's shape so callers can't tell SQL and Rust paths apart.
 */
function hydrateMenu<T extends { start_date: unknown; end_date: unknown; created_at: unknown }>(
  row: T
): T {
  return {
    ...row,
    start_date: isoToDate(row.start_date),
    end_date: isoToDate(row.end_date),
    created_at: isoToDate(row.created_at),
  };
}

/**
 * Rehydrate a nested `MenuWithRecipes`. Only the parent-level dates
 * need touching — slot rows under `recipe` are pure data (UUIDs,
 * strings, numbers) and pass through as-is.
 */
function hydrateMenuWithRecipes(row: MenuWithRecipes): MenuWithRecipes {
  return hydrateMenu(row);
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getMenus(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Menu[]> {
  if (caller && (await useRustBackend('menus', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    const rows = await internalJson<Menu[]>(`/api/menus${qs}`, caller);
    return rows.map(hydrateMenu);
  }

  const result = await db.query<Menu>(
    `SELECT id, name, is_active, start_date, end_date, created_at
     FROM menu
     ${opts.includeInactive ? '' : 'WHERE is_active = TRUE'}
     ORDER BY start_date DESC`
  );
  return result.rows;
}

export async function getMenuById(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Menu | null> {
  if (caller && (await useRustBackend('menus', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    const row = await internalJsonOrNull<Menu>(`/api/menus/${id}${qs}`, caller);
    return row ? hydrateMenu(row) : null;
  }

  const result = await db.query<Menu>(
    `SELECT id, name, is_active, start_date, end_date, created_at
     FROM menu
     WHERE id = $1
     ${opts.includeInactive ? '' : 'AND is_active = TRUE'}`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getMenuWithRecipes(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<MenuWithRecipes | null> {
  if (caller && (await useRustBackend('menus', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    const row = await internalJsonOrNull<MenuWithRecipes>(
      `/api/menus/${id}/with-recipes${qs}`,
      caller
    );
    return row ? hydrateMenuWithRecipes(row) : null;
  }

  const menu = await getMenuById(db, id, opts);
  if (!menu) return null;

  const recipe = await getMenuRecipes(db, id);
  return { ...menu, recipe };
}

// Returns all menu with their recipe slots — useful for the menu list
// page where you want to show slot counts or recipe names at a glance.
export async function getMenusWithRecipes(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<MenuWithRecipes[]> {
  if (caller && (await useRustBackend('menus', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    const rows = await internalJson<MenuWithRecipes[]>(
      `/api/menus/with-recipes${qs}`,
      caller
    );
    return rows.map(hydrateMenuWithRecipes);
  }

  const menu = await getMenus(db, opts);
  if (menu.length === 0) return [];

  const menuIds = menu.map((m) => m.id);

  const result = await db.query<MenuRecipeWithName & { menu_id: string }>(
    `SELECT
       mr.id,
       mr.menu_id,
       mr.recipe_id,
       mr.cycle_week,
       mr.day_of_week,
       mr.meal_category,
       r.name   AS recipe_name,
       r.active AS recipe_active
     FROM menu_recipe mr
     JOIN recipe r ON r.id = mr.recipe_id
     WHERE mr.menu_id = ANY($1)
     ORDER BY mr.menu_id, mr.cycle_week, mr.day_of_week, mr.meal_category`,
    [menuIds]
  );

  // Group recipe rows by menu_id
  const recipesByMenu = new Map<string, MenuRecipeWithName[]>();
  for (const row of result.rows) {
    const existing = recipesByMenu.get(row.menu_id) ?? [];
    existing.push({
      id: row.id,
      menu_id: row.menu_id,
      recipe_id: row.recipe_id,
      cycle_week: row.cycle_week,
      day_of_week: row.day_of_week,
      meal_category: row.meal_category,
      recipe_name: row.recipe_name,
      recipe_active: row.recipe_active,
    });
    recipesByMenu.set(row.menu_id, existing);
  }

  return menu.map((menu) => ({
    ...menu,
    recipe: recipesByMenu.get(menu.id) ?? [],
  }));
}

// Returns all recipe slots for a given menu, ordered for display.
export async function getMenuRecipes(
  db: Db,
  menuId: string,
  caller?: InternalCaller
): Promise<MenuRecipeWithName[]> {
  if (caller && (await useRustBackend('menus', caller))) {
    return internalJson<MenuRecipeWithName[]>(`/api/menus/${menuId}/recipes`, caller);
  }

  const result = await db.query<MenuRecipeWithName>(
    `SELECT
       mr.id,
       mr.menu_id,
       mr.recipe_id,
       mr.cycle_week,
       mr.day_of_week,
       mr.meal_category,
       r.name   AS recipe_name,
       r.active AS recipe_active
     FROM menu_recipe mr
     JOIN recipe r ON r.id = mr.recipe_id
     WHERE mr.menu_id = $1
     ORDER BY mr.cycle_week ASC, mr.day_of_week ASC, mr.meal_category ASC`,
    [menuId]
  );
  return result.rows;
}

// Returns the active menu covering a given date, if one exists.
// Used by the order creation flow to suggest the relevant menu.
export async function getActiveMenuForDate(
  db: Db,
  date: Date,
  caller?: InternalCaller
): Promise<Menu | null> {
  if (caller && (await useRustBackend('menus', caller))) {
    const qs = filtersToQueryString({ date });
    const row = await internalJsonOrNull<Menu>(`/api/menus/active-for-date${qs}`, caller);
    return row ? hydrateMenu(row) : null;
  }

  const result = await db.query<Menu>(
    `SELECT id, name, is_active, start_date, end_date, created_at
     FROM menu
     WHERE is_active = TRUE
       AND start_date <= $1
       AND end_date >= $1
     ORDER BY start_date DESC
     LIMIT 1`,
    [date]
  );
  return result.rows[0] ?? null;
}

// Returns the specific recipe slot for a given menu, week, day, and meal.
// Used to validate that an order's recipe matches the menu for that slot.
export async function getMenuSlot(
  db: Db,
  menuId: string,
  cycleWeek: number,
  dayOfWeek: DayOfWeek,
  mealCategory: string,
  caller?: InternalCaller
): Promise<MenuRecipeWithName | null> {
  if (caller && (await useRustBackend('menus', caller))) {
    const qs = filtersToQueryString({
      cycle_week: cycleWeek,
      day_of_week: dayOfWeek,
      meal_category: mealCategory,
    });
    return internalJsonOrNull<MenuRecipeWithName>(
      `/api/menus/${menuId}/slots/by-coord${qs}`,
      caller
    );
  }

  const result = await db.query<MenuRecipeWithName>(
    `SELECT
       mr.id,
       mr.menu_id,
       mr.recipe_id,
       mr.cycle_week,
       mr.day_of_week,
       mr.meal_category,
       r.name   AS recipe_name,
       r.active AS recipe_active
     FROM menu_recipe mr
     JOIN recipe r ON r.id = mr.recipe_id
     WHERE mr.menu_id      = $1
       AND mr.cycle_week   = $2
       AND mr.day_of_week  = $3
       AND mr.meal_category = $4`,
    [menuId, cycleWeek, dayOfWeek, mealCategory]
  );
  return result.rows[0] ?? null;
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createMenu(
  db: Db,
  data: InsertMenu,
  caller?: InternalCaller
): Promise<Menu> {
  if (caller && (await useRustBackend('menus', caller))) {
    const row = await internalJson<Menu>('/api/menus', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
    return hydrateMenu(row);
  }

  const result = await db.query<Menu>(
    `INSERT INTO menu (name, is_active, start_date, end_date)
     VALUES ($1, $2, $3, $4)
     RETURNING id, name, is_active, start_date, end_date, created_at`,
    [data.name.trim(), data.is_active ?? true, data.start_date, data.end_date]
  );
  return result.rows[0];
}

export async function updateMenu(
  db: Db,
  id: string,
  data: UpdateMenu,
  caller?: InternalCaller
): Promise<Menu | null> {
  if (caller && (await useRustBackend('menus', caller))) {
    const row = await internalJsonOrNull<Menu>(`/api/menus/${id}`, caller, {
      method: 'PATCH',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
    return row ? hydrateMenu(row) : null;
  }

  const result = await db.query<Menu>(
    `UPDATE menu
     SET
       name       = COALESCE($1, name),
       is_active  = COALESCE($2, is_active),
       start_date = COALESCE($3, start_date),
       end_date   = COALESCE($4, end_date)
     WHERE id = $5
     RETURNING id, name, is_active, start_date, end_date, created_at`,
    [
      data.name?.trim() ?? null,
      data.is_active ?? null,
      data.start_date ?? null,
      data.end_date ?? null,
      id,
    ]
  );
  return result.rows[0] ?? null;
}

export async function deactivateMenu(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Menu | null> {
  if (caller && (await useRustBackend('menus', caller))) {
    const row = await internalJsonOrNull<Menu>(`/api/menus/${id}/deactivate`, caller, {
      method: 'POST',
    });
    return row ? hydrateMenu(row) : null;
  }

  return updateMenu(db, id, { is_active: false });
}

// -------------------------------------------------------------
// SLOT MANAGEMENT
// -------------------------------------------------------------

// Upserts a single recipe into a menu slot.
// ON CONFLICT targets the unique constraint (menu_id, cycle_week, day_of_week, meal_category)
// so assigning a new recipe to an occupied slot replaces it cleanly.
export async function setMenuSlot(
  client: Pool | PoolClient,
  data: InsertMenuRecipe,
  caller?: InternalCaller
): Promise<MenuRecipe> {
  if (caller && (await useRustBackend('menus', caller))) {
    return internalJson<MenuRecipe>(`/api/menus/${data.menu_id}/slots`, caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  const result = await client.query<MenuRecipe>(
    `INSERT INTO menu_recipe (menu_id, recipe_id, cycle_week, day_of_week, meal_category)
     VALUES ($1, $2, $3, $4, $5)
     ON CONFLICT (menu_id, cycle_week, day_of_week, meal_category)
     DO UPDATE SET recipe_id = EXCLUDED.recipe_id
     RETURNING id, menu_id, recipe_id, cycle_week, day_of_week, meal_category`,
    [data.menu_id, data.recipe_id, data.cycle_week, data.day_of_week, data.meal_category]
  );
  return result.rows[0];
}

export async function clearMenuSlot(
  client: Pool | PoolClient,
  menuId: string,
  cycleWeek: number,
  dayOfWeek: DayOfWeek,
  mealCategory: string,
  caller?: InternalCaller
): Promise<boolean> {
  if (caller && (await useRustBackend('menus', caller))) {
    const resp = await internalJson<{ cleared: boolean }>(
      `/api/menus/${menuId}/slots/clear`,
      caller,
      {
        method: 'POST',
        body: JSON.stringify({
          cycle_week: cycleWeek,
          day_of_week: dayOfWeek,
          meal_category: mealCategory,
        }),
      }
    );
    return resp.cleared;
  }

  const result = await client.query(
    `DELETE FROM menu_recipe
     WHERE menu_id       = $1
       AND cycle_week    = $2
       AND day_of_week   = $3
       AND meal_category = $4`,
    [menuId, cycleWeek, dayOfWeek, mealCategory]
  );
  return (result.rowCount ?? 0) > 0;
}

// Replaces all slots for a menu atomically — useful for bulk edit flows
// where the UI sends a complete new week/cycle layout.
export async function setAllMenuSlots(
  client: Pool | PoolClient,
  menuId: string,
  slots: InsertMenuRecipe[],
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('menus', caller))) {
    await internalJson<void>(`/api/menus/${menuId}/slots`, caller, {
      method: 'PUT',
      body: JSON.stringify({
        slots: slots.map((s) => normalizeBody(s as unknown as Record<string, unknown>)),
      }),
    });
    return;
  }

  await client.query(
    `DELETE FROM menu_recipe WHERE menu_id = $1`,
    [menuId]
  );

  if (slots.length === 0) return;

  const values = slots
    .map((_, i) => `($${i * 5 + 1}, $${i * 5 + 2}, $${i * 5 + 3}, $${i * 5 + 4}, $${i * 5 + 5})`)
    .join(', ');

  const params = slots.flatMap((s) => [
    s.menu_id,
    s.recipe_id,
    s.cycle_week,
    s.day_of_week,
    s.meal_category,
  ]);

  await client.query(
    `INSERT INTO menu_recipe (menu_id, recipe_id, cycle_week, day_of_week, meal_category)
     VALUES ${values}`,
    params
  );
}
