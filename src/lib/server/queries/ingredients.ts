import type { Pool, PoolClient } from 'pg';
import { ConflictError } from '$lib/server/errors';
import type {
  Ingredient,
  InsertIngredient,
  UpdateIngredient,
  IngredientWithAllergens,
  Allergen,
} from '$lib/types';
import type { Db } from '$lib/server/db';
import {
  type PaginationParams,
  type PaginatedResult,
  paginationSQL,
  buildPaginatedResult,
} from './pagination';
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
// passed AND `use_rust_backend_ingredients` is on for that caller,
// the call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — threading
// `caller` into `src/routes/admin/ingredients/*` is a follow-up pass
// (per orders/recipes/companies precedent).
//
// Ingredients are admin-only across the board: every Rust handler
// hard-rejects client callers with 403. The dispatcher doesn't need
// any scoping logic — the server enforces it.
//
// The wire shapes for `Ingredient` and `IngredientWithAllergens`
// match the Rust `IngredientRow` / `IngredientWithAllergensRow`
// field-for-field, so decoded JSON from `internalJson` can be
// returned as-is.
// -------------------------------------------------------------

/**
 * 404-aware wrapper around `internalJson`. The TS SQL path returns
 * `null` for reads/mutations that miss (getIngredientById,
 * getIngredientWithAllergens, updateIngredient, deactivateIngredient);
 * the Rust path returns 404. Translate so the dispatcher is
 * behaviourally transparent.
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

/**
 * 409-aware wrapper for create where the TS surface throws
 * `ConflictError('An ingredient with this name already exists')` on
 * a UNIQUE-name violation. Rust returns 409 with the same message;
 * translate so callers can't tell the paths apart.
 */
async function internalJsonConflictAware<T>(
  path: string,
  caller: InternalCaller,
  opts?: Parameters<typeof internalJson>[2]
): Promise<T> {
  try {
    return await internalJson<T>(path, caller, opts);
  } catch (err) {
    if (isInternalApiError(err) && err.status === 409) {
      throw new ConflictError('An ingredient with this name already exists');
    }
    throw err;
  }
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getIngredients(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Ingredient[]> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJson<Ingredient[]>(`/api/ingredients${qs}`, caller);
  }

  const result = await db.query<Ingredient>(
    `SELECT id, name, active, cost_per_unit
     FROM ingredient
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

/** Lightweight: returns only ingredient names for fuzzy matching during import. */
export async function getIngredientNames(
  db: Db,
  caller?: InternalCaller
): Promise<string[]> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    return internalJson<string[]>('/api/ingredients/names', caller);
  }

  const { rows } = await db.query<{ name: string }>(
    `SELECT name FROM ingredient ORDER BY name`
  );
  return rows.map((r) => r.name);
}

export async function getIngredientById(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Ingredient | null> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJsonOrNull<Ingredient>(`/api/ingredients/${id}${qs}`, caller);
  }

  const result = await db.query<Ingredient>(
    `SELECT id, name, active, cost_per_unit
     FROM ingredient
     WHERE id = $1
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getIngredientWithAllergens(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<IngredientWithAllergens | null> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJsonOrNull<IngredientWithAllergens>(
      `/api/ingredients/${id}/with-allergens${qs}`,
      caller
    );
  }

  // Fetch ingredient row first — avoids returning a shell when id is missing
  const ingredient = await getIngredientById(db, id, opts);
  if (!ingredient) return null;

  const allergenResult = await db.query<Allergen>(
    `SELECT a.id, a.name
     FROM allergen a
     JOIN ingredient_allergen ia ON ia.allergen_id = a.id
     WHERE ia.ingredient_id = $1
     ORDER BY a.name ASC`,
    [id]
  );

  return { ...ingredient, allergen: allergenResult.rows };
}

export async function getIngredientsWithAllergens(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<IngredientWithAllergens[]> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJson<IngredientWithAllergens[]>(
      `/api/ingredients/with-allergens${qs}`,
      caller
    );
  }

  // Single query with LEFT JOIN + aggregation to avoid N+1
  const result = await db.query<
    Ingredient & { allergen_ids: string[] | null; allergen_names: string[] | null }
  >(
    `SELECT
       i.id,
       i.name,
       i.active,
       i.cost_per_unit,
       ARRAY_AGG(a.id ORDER BY a.name)   FILTER (WHERE a.id IS NOT NULL)   AS allergen_ids,
       ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names
     FROM ingredient i
     LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
     LEFT JOIN allergen a ON a.id = ia.allergen_id
     ${opts.includeInactive ? '' : 'WHERE i.active = TRUE'}
     GROUP BY i.id
     ORDER BY i.name ASC`
  );

  return result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    active: row.active,
    cost_per_unit: row.cost_per_unit,
    allergen: (row.allergen_ids ?? []).map((aid, idx) => ({
      id: aid,
      name: (row.allergen_names ?? [])[idx],
    })),
  }));
}

export async function getIngredientsWithAllergensPaginated(
  db: Db,
  opts: { includeInactive?: boolean; search?: string },
  pagination: PaginationParams,
  caller?: InternalCaller
): Promise<PaginatedResult<IngredientWithAllergens>> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    const merged = {
      include_inactive: opts.includeInactive,
      search: opts.search,
      page: pagination.page,
      per_page: pagination.perPage,
    };
    const qs = filtersToQueryString(merged);
    return internalJson<PaginatedResult<IngredientWithAllergens>>(
      `/api/ingredients/paginated${qs}`,
      caller
    );
  }

  const conditions: string[] = [];
  const params: unknown[] = [];

  if (!opts.includeInactive) {
    conditions.push('i.active = TRUE');
  }
  if (opts.search) {
    params.push(`%${opts.search}%`);
    conditions.push(`LOWER(i.name) LIKE LOWER($${params.length})`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  const countResult = await db.query<{ count: string }>(
    `SELECT COUNT(*) AS count FROM ingredient i ${where}`,
    params
  );
  const total = parseInt(countResult.rows[0].count, 10);

  const pag = paginationSQL(pagination, params.length + 1);
  const result = await db.query<
    Ingredient & { allergen_ids: string[] | null; allergen_names: string[] | null }
  >(
    `SELECT
       i.id,
       i.name,
       i.active,
       i.cost_per_unit,
       ARRAY_AGG(a.id ORDER BY a.name)   FILTER (WHERE a.id IS NOT NULL)   AS allergen_ids,
       ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names
     FROM ingredient i
     LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
     LEFT JOIN allergen a ON a.id = ia.allergen_id
     ${where}
     GROUP BY i.id
     ORDER BY i.name ASC
     ${pag.sql}`,
    [...params, ...pag.values]
  );

  const data = result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    active: row.active,
    cost_per_unit: row.cost_per_unit,
    allergen: (row.allergen_ids ?? []).map((aid, idx) => ({
      id: aid,
      name: (row.allergen_names ?? [])[idx],
    })),
  }));

  return buildPaginatedResult(data, total, pagination);
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createIngredient(
  db: Db,
  data: InsertIngredient,
  caller?: InternalCaller
): Promise<Ingredient> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    return internalJsonConflictAware<Ingredient>('/api/ingredients', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  try {
    const result = await db.query<Ingredient>(
      `INSERT INTO ingredient (name, active)
       VALUES ($1, $2)
       RETURNING id, name, active, cost_per_unit`,
      [data.name.trim(), data.active ?? true]
    );
    return result.rows[0];
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('An ingredient with this name already exists');
    throw err;
  }
}

export async function updateIngredient(
  db: Db,
  id: string,
  data: UpdateIngredient,
  caller?: InternalCaller
): Promise<Ingredient | null> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    // Three-state cost_per_unit: present+value → set, present+null → clear,
    // absent → leave alone. `normalizeBody` strips `undefined`, but `null`
    // passes through — so use the `in` sentinel to decide inclusion.
    const body: Record<string, unknown> = { ...(data as Record<string, unknown>) };
    if (!('cost_per_unit' in data)) {
      // Explicit delete in case the caller spread `{...data, cost_per_unit: undefined}`.
      delete body.cost_per_unit;
    }

    // 404 → null; 409 → ConflictError (parity with SQL path).
    try {
      return await internalJson<Ingredient>(`/api/ingredients/${id}`, caller, {
        method: 'PATCH',
        body: JSON.stringify(normalizeBody(body)),
      });
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return null;
      if (isInternalApiError(err) && err.status === 409) {
        throw new ConflictError('An ingredient with this name already exists');
      }
      throw err;
    }
  }

  try {
    const result = await db.query<Ingredient>(
      `UPDATE ingredient
       SET
         name          = COALESCE($1, name),
         active        = COALESCE($2, active),
         cost_per_unit = CASE WHEN $4 THEN $3::numeric ELSE cost_per_unit END
       WHERE id = $5
       RETURNING id, name, active, cost_per_unit`,
      [
        data.name?.trim() ?? null,
        data.active ?? null,
        data.cost_per_unit ?? null,
        'cost_per_unit' in data,
        id,
      ]
    );
    return result.rows[0] ?? null;
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('An ingredient with this name already exists');
    throw err;
  }
}

// Soft delete — sets active = false rather than removing the row.
// Hard deletes are unsafe: recipe_ingredient has ON DELETE RESTRICT.
export async function deactivateIngredient(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Ingredient | null> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    return internalJsonOrNull<Ingredient>(`/api/ingredients/${id}/deactivate`, caller, {
      method: 'POST',
    });
  }

  return updateIngredient(db, id, { active: false });
}

// -------------------------------------------------------------
// ALLERGEN ASSOCIATIONS
// Callers managing allergen on an ingredient use these.
// For full replacement (edit form), use setIngredientAllergens.
// -------------------------------------------------------------

export async function addIngredientAllergen(
  db: Db,
  ingredientId: string,
  allergenId: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    await internalJson<void>(
      `/api/ingredients/${ingredientId}/allergens/${allergenId}`,
      caller,
      { method: 'POST' }
    );
    return;
  }

  await db.query(
    `INSERT INTO ingredient_allergen (ingredient_id, allergen_id)
     VALUES ($1, $2)
     ON CONFLICT DO NOTHING`,
    [ingredientId, allergenId]
  );
}

export async function removeIngredientAllergen(
  db: Db,
  ingredientId: string,
  allergenId: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    await internalJson<void>(
      `/api/ingredients/${ingredientId}/allergens/${allergenId}`,
      caller,
      { method: 'DELETE' }
    );
    return;
  }

  await db.query(
    `DELETE FROM ingredient_allergen
     WHERE ingredient_id = $1 AND allergen_id = $2`,
    [ingredientId, allergenId]
  );
}

// Replace the full allergen set for an ingredient in one transaction.
// Accepts a PoolClient so the caller can compose it into a larger transaction,
// or pass db directly for standalone use (Pool satisfies PoolClient's query API).
// When `caller` is passed and the flag is on, forwards to the Rust service —
// no in-repo caller passes a PoolClient today, so losing the ability to compose
// with an outer transaction on the Rust path is acceptable.
export async function setIngredientAllergens(
  client: Pool | PoolClient,
  ingredientId: string,
  allergenIds: string[],
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('ingredients', caller))) {
    await internalJson<void>(
      `/api/ingredients/${ingredientId}/allergens`,
      caller,
      {
        method: 'PUT',
        body: JSON.stringify({ allergen_ids: allergenIds }),
      }
    );
    return;
  }

  await client.query(
    `DELETE FROM ingredient_allergen WHERE ingredient_id = $1`,
    [ingredientId]
  );

  if (allergenIds.length === 0) return;

  // Build multi-row INSERT — ($1, $2), ($1, $3), ...
  const values = allergenIds.map((aid, i) => `($1, $${i + 2})`).join(', ');
  await client.query(
    `INSERT INTO ingredient_allergen (ingredient_id, allergen_id)
     VALUES ${values}`,
    [ingredientId, ...allergenIds]
  );
}
