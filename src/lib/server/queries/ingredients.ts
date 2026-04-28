import type { Pool, PoolClient } from 'pg';
import type {
  Ingredient,
  InsertIngredient,
  UpdateIngredient,
  IngredientWithAllergens,
  Allergen,
} from '$lib/types';

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getIngredients(
  db: Pool,
  opts: { includeInactive?: boolean } = {}
): Promise<Ingredient[]> {
  const result = await db.query<Ingredient>(
    `SELECT id, name, active
     FROM ingredients
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getIngredientById(
  db: Pool,
  id: string,
  opts: { includeInactive?: boolean } = {}
): Promise<Ingredient | null> {
  const result = await db.query<Ingredient>(
    `SELECT id, name, active
     FROM ingredients
     WHERE id = $1
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getIngredientWithAllergens(
  db: Pool,
  id: string,
  opts: { includeInactive?: boolean } = {}
): Promise<IngredientWithAllergens | null> {
  // Fetch ingredient row first — avoids returning a shell when id is missing
  const ingredient = await getIngredientById(db, id, opts);
  if (!ingredient) return null;

  const allergenResult = await db.query<Allergen>(
    `SELECT a.id, a.name
     FROM allergens a
     JOIN ingredient_allergens ia ON ia.allergen_id = a.id
     WHERE ia.ingredient_id = $1
     ORDER BY a.name ASC`,
    [id]
  );

  return { ...ingredient, allergens: allergenResult.rows };
}

export async function getIngredientsWithAllergens(
  db: Pool,
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
       ARRAY_AGG(a.id ORDER BY a.name)   FILTER (WHERE a.id IS NOT NULL)   AS allergen_ids,
       ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names
     FROM ingredients i
     LEFT JOIN ingredient_allergens ia ON ia.ingredient_id = i.id
     LEFT JOIN allergens a ON a.id = ia.allergen_id
     ${opts.includeInactive ? '' : 'WHERE i.active = TRUE'}
     GROUP BY i.id
     ORDER BY i.name ASC`
  );

  return result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    active: row.active,
    allergens: (row.allergen_ids ?? []).map((aid, idx) => ({
      id: aid,
      name: (row.allergen_names ?? [])[idx],
    })),
  }));
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createIngredient(
  db: Pool,
  data: InsertIngredient
): Promise<Ingredient> {
  const result = await db.query<Ingredient>(
    `INSERT INTO ingredients (name, active)
     VALUES ($1, $2)
     RETURNING id, name, active`,
    [data.name.trim(), data.active ?? true]
  );
  return result.rows[0];
}

export async function updateIngredient(
  db: Pool,
  id: string,
  data: UpdateIngredient
): Promise<Ingredient | null> {
  const result = await db.query<Ingredient>(
    `UPDATE ingredients
     SET
       name   = COALESCE($1, name),
       active = COALESCE($2, active)
     WHERE id = $3
     RETURNING id, name, active`,
    [data.name?.trim() ?? null, data.active ?? null, id]
  );
  return result.rows[0] ?? null;
}

// Soft delete — sets active = false rather than removing the row.
// Hard deletes are unsafe: recipe_ingredients has ON DELETE RESTRICT.
export async function deactivateIngredient(db: Pool, id: string): Promise<Ingredient | null> {
  return updateIngredient(db, id, { active: false });
}

// -------------------------------------------------------------
// ALLERGEN ASSOCIATIONS
// Callers managing allergens on an ingredient use these.
// For full replacement (edit form), use setIngredientAllergens.
// -------------------------------------------------------------

export async function addIngredientAllergen(
  db: Pool,
  ingredientId: string,
  allergenId: string
): Promise<void> {
  await db.query(
    `INSERT INTO ingredient_allergens (ingredient_id, allergen_id)
     VALUES ($1, $2)
     ON CONFLICT DO NOTHING`,
    [ingredientId, allergenId]
  );
}

export async function removeIngredientAllergen(
  db: Pool,
  ingredientId: string,
  allergenId: string
): Promise<void> {
  await db.query(
    `DELETE FROM ingredient_allergens
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
    `DELETE FROM ingredient_allergens WHERE ingredient_id = $1`,
    [ingredientId]
  );

  if (allergenIds.length === 0) return;

  // Build multi-row INSERT — ($1, $2), ($1, $3), ...
  const values = allergenIds.map((aid, i) => `($1, $${i + 2})`).join(', ');
  await client.query(
    `INSERT INTO ingredient_allergens (ingredient_id, allergen_id)
     VALUES ${values}`,
    [ingredientId, ...allergenIds]
  );
}
