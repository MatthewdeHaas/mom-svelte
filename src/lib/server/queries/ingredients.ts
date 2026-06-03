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

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getIngredients(
  db: Db,
  opts: { includeInactive?: boolean } = {}
): Promise<Ingredient[]> {
  const result = await db.query<Ingredient>(
    `SELECT id, name, active, cost_per_unit
     FROM ingredient
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

/** Lightweight: returns only ingredient names for fuzzy matching during import. */
export async function getIngredientNames(db: Db): Promise<string[]> {
  const { rows } = await db.query<{ name: string }>(
    `SELECT name FROM ingredient ORDER BY name`
  );
  return rows.map((r) => r.name);
}

export async function getIngredientById(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {}
): Promise<Ingredient | null> {
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
  opts: { includeInactive?: boolean } = {}
): Promise<IngredientWithAllergens | null> {
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
  opts: { includeInactive?: boolean } = {}
): Promise<IngredientWithAllergens[]> {
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
  pagination: PaginationParams
): Promise<PaginatedResult<IngredientWithAllergens>> {
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
  data: InsertIngredient
): Promise<Ingredient> {
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
  data: UpdateIngredient
): Promise<Ingredient | null> {
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
export async function deactivateIngredient(db: Db, id: string): Promise<Ingredient | null> {
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
  allergenId: string
): Promise<void> {
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
  allergenId: string
): Promise<void> {
  await db.query(
    `DELETE FROM ingredient_allergen
     WHERE ingredient_id = $1 AND allergen_id = $2`,
    [ingredientId, allergenId]
  );
}

// Replace the full allergen set for an ingredient in one transaction.
// Accepts a PoolClient so the caller can compose it into a larger transaction,
// or pass db directly for standalone use (Pool satisfies PoolClient's query API).
export async function setIngredientAllergens(
  client: Pool | PoolClient,
  ingredientId: string,
  allergenIds: string[]
): Promise<void> {
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
