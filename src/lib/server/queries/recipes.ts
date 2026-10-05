import type { Pool, PoolClient } from 'pg';
import { ConflictError } from '$lib/server/errors';
import type {
  Recipe,
  InsertRecipe,
  UpdateRecipe,
  RecipeWithIngredients,
  RecipeIngredient,
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
  internalFetch,
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
// passed AND `use_rust_backend_recipes` is on for that caller, the
// call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — adding
// `caller` to specific routes is a follow-up pass once we're ready
// to flip the flag per-company.
//
// Recipes are global master data (no company scoping). Clients get
// 403 on every mutation; reads are unrestricted. The Rust handlers
// enforce this server-side; the dispatcher only needs to forward.
//
// The wire shapes for `Recipe` / `RecipeWithIngredients` / `RecipeIngredient`
// match the Rust `RecipeRow` / `RecipeWithIngredientsRow` / `RecipeIngredientRow`
// field-for-field, so decoded JSON from `internalJson` can be returned as-is.
// -------------------------------------------------------------

/**
 * 404-aware wrapper around `internalJson`. The TS SQL path returns
 * `null` when a mutation targets a non-existent row (updateRecipe,
 * deactivateRecipe); the Rust path returns 404. Translate so the
 * dispatcher is behaviourally transparent.
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
 * 409-aware wrapper for create/update where TS surface throws
 * `ConflictError('A recipe with this name already exists')` on
 * UNIQUE-name violation. Rust returns 409 with the same message in
 * the body; translate so callers can't tell the paths apart.
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
      throw new ConflictError('A recipe with this name already exists');
    }
    throw err;
  }
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getRecipes(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Recipe[]> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJson<Recipe[]>(`/api/recipes${qs}`, caller);
  }

  const result = await db.query<Recipe>(
    `SELECT id, name, active, created_at, price_per_meal
     FROM recipe
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getRecipeById(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Recipe | null> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJsonOrNull<Recipe>(`/api/recipes/${id}${qs}`, caller);
  }

  const result = await db.query<Recipe>(
    `SELECT id, name, active, created_at, price_per_meal
     FROM recipe
     WHERE id = $1
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getRecipeByName(
  db: Db,
  name: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Recipe | null> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const qs = filtersToQueryString({ name, include_inactive: opts.includeInactive });
    return internalJsonOrNull<Recipe>(`/api/recipes/by-name${qs}`, caller);
  }

  const result = await db.query<Recipe>(
    `SELECT id, name, active, created_at, price_per_meal
     FROM recipe
     WHERE LOWER(name) = LOWER($1)
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [name]
  );
  return result.rows[0] ?? null;
}

export async function getRecipeWithIngredients(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<RecipeWithIngredients | null> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJsonOrNull<RecipeWithIngredients>(
      `/api/recipes/${id}/with-ingredients${qs}`,
      caller
    );
  }

  const recipe = await getRecipeById(db, id, opts);
  if (!recipe) return null;

  const ingredients = await getRecipeIngredients(db, id);
  return { ...recipe, ingredients };
}

export async function getRecipesWithIngredientsByIds(
  db: Db,
  ids: string[],
  caller?: InternalCaller
): Promise<Map<string, RecipeWithIngredients>> {
  if (ids.length === 0) return new Map();

  if (caller && (await useRustBackend('recipes', caller))) {
    // Rust returns a flat Vec; wrap it back into a Map keyed by id to
    // preserve the SQL surface.
    const qs = filtersToQueryString({ ids: ids.join(',') });
    const rows = await internalJson<RecipeWithIngredients[]>(
      `/api/recipes/by-ids${qs}`,
      caller
    );
    const map = new Map<string, RecipeWithIngredients>();
    for (const r of rows) map.set(r.id, r);
    return map;
  }

  const result = await db.query<
    Recipe & {
      ingredient_ids: string[] | null;
      ingredient_names: string[] | null;
      ingredient_amounts: string[] | null;
      ingredient_units: string[] | null;
      allergen_id_sets: string[] | null;
      allergen_name_sets: string[] | null;
    }
  >(
    `SELECT
       r.id,
       r.name,
       r.active,
       r.created_at,
       r.price_per_meal,
       ARRAY_AGG(i.id      ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_ids,
       ARRAY_AGG(i.name    ORDER BY i.name) FILTER (WHERE i.name IS NOT NULL)   AS ingredient_names,
       ARRAY_AGG(ri.amount ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_amounts,
       ARRAY_AGG(ri.unit   ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_units,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.id::TEXT, ',' ORDER BY a.name)
          FROM ingredient_allergen ia
          JOIN allergen a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_id_sets,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.name, ',' ORDER BY a.name)
          FROM ingredient_allergen ia
          JOIN allergen a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_name_sets
     FROM recipe r
     LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
     LEFT JOIN ingredient i ON i.id = ri.ingredient_id
     WHERE r.id = ANY($1)
     GROUP BY r.id
     ORDER BY r.name ASC`,
    [ids]
  );

  const map = new Map<string, RecipeWithIngredients>();
  for (const row of result.rows) {
    map.set(row.id, {
      id: row.id,
      name: row.name,
      active: row.active,
      created_at: row.created_at,
      price_per_meal: row.price_per_meal,
      ingredients: (row.ingredient_ids ?? []).map((iid, idx) => {
        const allergenIdSet = (row.allergen_id_sets ?? [])[idx];
        const allergenNameSet = (row.allergen_name_sets ?? [])[idx];

        const allergenIds = allergenIdSet ? allergenIdSet.split(',') : [];
        const allergenNames = allergenNameSet ? allergenNameSet.split(',') : [];

        const allergen: Allergen[] = allergenIds.map((aid, aidx) => ({
          id: aid,
          name: allergenNames[aidx],
        }));

        return {
          ingredient_id: iid,
          name: (row.ingredient_names ?? [])[idx],
          amount: parseFloat((row.ingredient_amounts ?? [])[idx]),
          unit: (row.ingredient_units ?? [])[idx],
          allergen,
        } satisfies RecipeIngredient;
      }),
    });
  }
  return map;
}

export async function getRecipesWithIngredients(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<RecipeWithIngredients[]> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJson<RecipeWithIngredients[]>(
      `/api/recipes/with-ingredients${qs}`,
      caller
    );
  }

  // Single query — aggregates ingredient and their allergen per recipe.
  // Each ingredient row carries its allergen arrays, which we zip together
  // after the fact the same way as getIngredientsWithAllergens.
  const result = await db.query<
    Recipe & {
      ingredient_ids: string[] | null;
      ingredient_names: string[] | null;
      ingredient_amounts: string[] | null; // NUMERIC comes back as string from pg
      ingredient_units: string[] | null;
      allergen_id_sets: string[] | null;   // packed as "aid1,aid2" per ingredient
      allergen_name_sets: string[] | null; // packed as "name1,name2" per ingredient
    }
  >(
    `SELECT
       r.id,
       r.name,
       r.active,
       r.created_at,
       r.price_per_meal,
       ARRAY_AGG(i.id      ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_ids,
       ARRAY_AGG(i.name    ORDER BY i.name) FILTER (WHERE i.name IS NOT NULL)   AS ingredient_names,
       ARRAY_AGG(ri.amount ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_amounts,
       ARRAY_AGG(ri.unit   ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_units,
       -- Allergens are aggregated per ingredient into a comma-separated string,
       -- then those strings are collected into an array aligned with ingredient_ids.
       ARRAY_AGG(
         (SELECT STRING_AGG(a.id::TEXT, ',' ORDER BY a.name)
          FROM ingredient_allergen ia
          JOIN allergen a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_id_sets,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.name, ',' ORDER BY a.name)
          FROM ingredient_allergen ia
          JOIN allergen a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_name_sets
     FROM recipe r
     LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
     LEFT JOIN ingredient i ON i.id = ri.ingredient_id
     ${opts.includeInactive ? '' : 'WHERE r.active = TRUE'}
     GROUP BY r.id
     ORDER BY r.name ASC`,
  );

  return result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    active: row.active,
    created_at: row.created_at,
    price_per_meal: row.price_per_meal,
    ingredients: (row.ingredient_ids ?? []).map((iid, idx) => {
      const allergenIdSet = (row.allergen_id_sets ?? [])[idx];
      const allergenNameSet = (row.allergen_name_sets ?? [])[idx];

      const allergenIds = allergenIdSet ? allergenIdSet.split(',') : [];
      const allergenNames = allergenNameSet ? allergenNameSet.split(',') : [];

      const allergen: Allergen[] = allergenIds.map((aid, aidx) => ({
        id: aid,
        name: allergenNames[aidx],
      }));

      return {
        ingredient_id: iid,
        name: (row.ingredient_names ?? [])[idx],
        amount: parseFloat((row.ingredient_amounts ?? [])[idx]),
        unit: (row.ingredient_units ?? [])[idx],
        allergen,
      } satisfies RecipeIngredient;
    }),
  }));
}

export async function getRecipesWithIngredientsPaginated(
  db: Db,
  opts: { includeInactive?: boolean; search?: string },
  pagination: PaginationParams,
  caller?: InternalCaller
): Promise<PaginatedResult<RecipeWithIngredients>> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const merged = {
      include_inactive: opts.includeInactive,
      search: opts.search,
      page: pagination.page,
      per_page: pagination.perPage,
    };
    const qs = filtersToQueryString(merged);
    return internalJson<PaginatedResult<RecipeWithIngredients>>(
      `/api/recipes/paginated${qs}`,
      caller
    );
  }

  const conditions: string[] = [];
  const params: unknown[] = [];

  if (!opts.includeInactive) {
    conditions.push('r.active = TRUE');
  }
  if (opts.search) {
    params.push(`%${opts.search}%`);
    conditions.push(`LOWER(r.name) LIKE LOWER($${params.length})`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  const countResult = await db.query<{ count: string }>(
    `SELECT COUNT(*) AS count FROM recipe r ${where}`,
    params
  );
  const total = parseInt(countResult.rows[0].count, 10);

  const pag = paginationSQL(pagination, params.length + 1);
  const result = await db.query<
    Recipe & {
      ingredient_ids: string[] | null;
      ingredient_names: string[] | null;
      ingredient_amounts: string[] | null;
      ingredient_units: string[] | null;
      allergen_id_sets: string[] | null;
      allergen_name_sets: string[] | null;
    }
  >(
    `SELECT
       r.id,
       r.name,
       r.active,
       r.created_at,
       r.price_per_meal,
       ARRAY_AGG(i.id      ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_ids,
       ARRAY_AGG(i.name    ORDER BY i.name) FILTER (WHERE i.name IS NOT NULL)   AS ingredient_names,
       ARRAY_AGG(ri.amount ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_amounts,
       ARRAY_AGG(ri.unit   ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_units,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.id::TEXT, ',' ORDER BY a.name)
          FROM ingredient_allergen ia
          JOIN allergen a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_id_sets,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.name, ',' ORDER BY a.name)
          FROM ingredient_allergen ia
          JOIN allergen a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_name_sets
     FROM recipe r
     LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
     LEFT JOIN ingredient i ON i.id = ri.ingredient_id
     ${where}
     GROUP BY r.id
     ORDER BY r.name ASC
     ${pag.sql}`,
    [...params, ...pag.values]
  );

  const data = result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    active: row.active,
    created_at: row.created_at,
    price_per_meal: row.price_per_meal,
    ingredients: (row.ingredient_ids ?? []).map((iid, idx) => {
      const allergenIdSet = (row.allergen_id_sets ?? [])[idx];
      const allergenNameSet = (row.allergen_name_sets ?? [])[idx];

      const allergenIds = allergenIdSet ? allergenIdSet.split(',') : [];
      const allergenNames = allergenNameSet ? allergenNameSet.split(',') : [];

      const allergen: Allergen[] = allergenIds.map((aid, aidx) => ({
        id: aid,
        name: allergenNames[aidx],
      }));

      return {
        ingredient_id: iid,
        name: (row.ingredient_names ?? [])[idx],
        amount: parseFloat((row.ingredient_amounts ?? [])[idx]),
        unit: (row.ingredient_units ?? [])[idx],
        allergen,
      } satisfies RecipeIngredient;
    }),
  }));

  return buildPaginatedResult(data, total, pagination);
}

// Returns just the ingredient for a recipe — used internally and
// useful when the caller already has the recipe row.
export async function getRecipeIngredients(
  db: Db,
  recipeId: string,
  caller?: InternalCaller
): Promise<RecipeIngredient[]> {
  if (caller && (await useRustBackend('recipes', caller))) {
    return internalJson<RecipeIngredient[]>(
      `/api/recipes/${recipeId}/ingredients`,
      caller
    );
  }

  const result = await db.query<
    RecipeIngredient & {
      allergen_ids: string[] | null;
      allergen_names: string[] | null;
    }
  >(
    `SELECT
       i.id        AS ingredient_id,
       i.name,
       ri.amount,
       ri.unit,
       ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id IS NOT NULL)   AS allergen_ids,
       ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names
     FROM recipe_ingredient ri
     JOIN ingredient i ON i.id = ri.ingredient_id
     LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
     LEFT JOIN allergen a ON a.id = ia.allergen_id
     WHERE ri.recipe_id = $1
     GROUP BY i.id, i.name, ri.amount, ri.unit
     ORDER BY i.name ASC`,
    [recipeId]
  );

  return result.rows.map((row) => ({
    ingredient_id: row.ingredient_id,
    name: row.name,
    amount: parseFloat(row.amount as unknown as string),
    unit: row.unit,
    allergen: (row.allergen_ids ?? []).map((aid, idx) => ({
      id: aid,
      name: (row.allergen_names ?? [])[idx],
    })),
  }));
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createRecipe(
  db: Db,
  data: InsertRecipe,
  caller?: InternalCaller
): Promise<Recipe> {
  if (caller && (await useRustBackend('recipes', caller))) {
    return internalJsonConflictAware<Recipe>('/api/recipes', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  try {
    const result = await db.query<Recipe>(
      `INSERT INTO recipe (name, active)
       VALUES ($1, $2)
       RETURNING id, name, active, created_at, price_per_meal`,
      [data.name.trim(), data.active ?? true]
    );
    return result.rows[0];
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('A recipe with this name already exists');
    throw err;
  }
}

export async function updateRecipe(
  db: Db,
  id: string,
  data: UpdateRecipe,
  caller?: InternalCaller
): Promise<Recipe | null> {
  if (caller && (await useRustBackend('recipes', caller))) {
    // 409 → ConflictError (parity with SQL path); 404 → null.
    try {
      return await internalJson<Recipe>(`/api/recipes/${id}`, caller, {
        method: 'PATCH',
        body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
      });
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return null;
      if (isInternalApiError(err) && err.status === 409) {
        throw new ConflictError('A recipe with this name already exists');
      }
      throw err;
    }
  }

  try {
    const result = await db.query<Recipe>(
      `UPDATE recipe
       SET
         name           = COALESCE($1, name),
         active         = COALESCE($2, active),
         price_per_meal = CASE WHEN $4 THEN $3::numeric ELSE price_per_meal END
       WHERE id = $5
       RETURNING id, name, active, created_at, price_per_meal`,
      [
        data.name?.trim() ?? null,
        data.active ?? null,
        data.price_per_meal ?? null,
        'price_per_meal' in data,
        id,
      ]
    );
    return result.rows[0] ?? null;
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('A recipe with this name already exists');
    throw err;
  }
}

// Soft delete only. Hard delete is blocked by ON DELETE RESTRICT on orders.
// Even if no orders exist yet, we never hard delete recipe —
// historical data integrity depends on this.
export async function deactivateRecipe(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Recipe | null> {
  if (caller && (await useRustBackend('recipes', caller))) {
    return internalJsonOrNull<Recipe>(`/api/recipes/${id}/deactivate`, caller, {
      method: 'POST',
    });
  }

  return updateRecipe(db, id, { active: false });
}

// -------------------------------------------------------------
// INGREDIENT ASSOCIATIONS
// -------------------------------------------------------------

export async function addRecipeIngredient(
  client: Pool | PoolClient,
  recipeId: string,
  ingredientId: string,
  amount: number,
  unit: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('recipes', caller))) {
    await internalFetch(
      `/api/recipes/${recipeId}/ingredients/${ingredientId}`,
      caller,
      { method: 'POST', body: JSON.stringify({ amount, unit }) }
    );
    return;
  }

  await client.query(
    `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (recipe_id, ingredient_id)
     DO UPDATE SET amount = EXCLUDED.amount, unit = EXCLUDED.unit`,
    [recipeId, ingredientId, amount, unit]
  );
}

export async function removeRecipeIngredient(
  client: Pool | PoolClient,
  recipeId: string,
  ingredientId: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('recipes', caller))) {
    await internalFetch(
      `/api/recipes/${recipeId}/ingredients/${ingredientId}`,
      caller,
      { method: 'DELETE' }
    );
    return;
  }

  await client.query(
    `DELETE FROM recipe_ingredient
     WHERE recipe_id = $1 AND ingredient_id = $2`,
    [recipeId, ingredientId]
  );
}

// Replace the full ingredient set for a recipe atomically.
// Pass a PoolClient to compose into a larger transaction,
// or pass Pool directly for standalone use.
export async function setRecipeIngredients(
  client: Pool | PoolClient,
  recipeId: string,
  ingredient: Array<{ ingredient_id: string; amount: number; unit: string }>,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('recipes', caller))) {
    await internalFetch(`/api/recipes/${recipeId}/ingredients`, caller, {
      method: 'PUT',
      body: JSON.stringify({ ingredient }),
    });
    return;
  }

  await client.query(
    `DELETE FROM recipe_ingredient WHERE recipe_id = $1`,
    [recipeId]
  );

  if (ingredient.length === 0) return;

  const values = ingredient
    .map((_, i) => `($1, $${i * 3 + 2}, $${i * 3 + 3}, $${i * 3 + 4})`)
    .join(', ');

  const params = ingredient.flatMap((ing) => [
    ing.ingredient_id,
    ing.amount,
    ing.unit,
  ]);

  await client.query(
    `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
     VALUES ${values}`,
    [recipeId, ...params]
  );
}

// -------------------------------------------------------------
// REFERENCE CHECKS
// Used by import validation to confirm a recipe is valid for use.
// -------------------------------------------------------------

// Returns true if the recipe exists and is active.
// The import layer calls this after the Zod parse to confirm
// recipe names from the sheet resolve to usable records.
export async function isRecipeActive(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<boolean> {
  if (caller && (await useRustBackend('recipes', caller))) {
    const r = await internalJson<{ active: boolean }>(
      `/api/recipes/${id}/active`,
      caller
    );
    return r.active;
  }

  const result = await db.query<{ active: boolean }>(
    `SELECT active FROM recipe WHERE id = $1`,
    [id]
  );
  return result.rows[0]?.active ?? false;
}
