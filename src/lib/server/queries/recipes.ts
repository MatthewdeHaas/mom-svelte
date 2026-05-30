import type { Pool, PoolClient } from 'pg';
import type {
  Recipe,
  InsertRecipe,
  UpdateRecipe,
  RecipeWithIngredients,
  RecipeIngredient,
  Allergen,
} from '$lib/types';

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getRecipes(
  db: Pool,
  opts: { includeInactive?: boolean } = {}
): Promise<Recipe[]> {
  const result = await db.query<Recipe>(
    `SELECT id, name, active, created_at
     FROM recipes
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getRecipeById(
  db: Pool,
  id: string,
  opts: { includeInactive?: boolean } = {}
): Promise<Recipe | null> {
  const result = await db.query<Recipe>(
    `SELECT id, name, active, created_at
     FROM recipes
     WHERE id = $1
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getRecipeByName(
  db: Pool,
  name: string,
  opts: { includeInactive?: boolean } = {}
): Promise<Recipe | null> {
  const result = await db.query<Recipe>(
    `SELECT id, name, active, created_at
     FROM recipes
     WHERE LOWER(name) = LOWER($1)
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [name]
  );
  return result.rows[0] ?? null;
}

export async function getRecipeWithIngredients(
  db: Pool,
  id: string,
  opts: { includeInactive?: boolean } = {}
): Promise<RecipeWithIngredients | null> {
  const recipe = await getRecipeById(db, id, opts);
  if (!recipe) return null;

  const ingredients = await getRecipeIngredients(db, id);
  return { ...recipe, ingredients };
}

export async function getRecipesWithIngredientsByIds(
  db: Pool,
  ids: number[]
): Promise<Map<number, RecipeWithIngredients>> {
  if (ids.length === 0) return new Map();

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
       ARRAY_AGG(i.id      ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_ids,
       ARRAY_AGG(i.name    ORDER BY i.name) FILTER (WHERE i.name IS NOT NULL)   AS ingredient_names,
       ARRAY_AGG(ri.amount ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_amounts,
       ARRAY_AGG(ri.unit   ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_units,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.id::TEXT, ',' ORDER BY a.name)
          FROM ingredient_allergens ia
          JOIN allergens a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_id_sets,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.name, ',' ORDER BY a.name)
          FROM ingredient_allergens ia
          JOIN allergens a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_name_sets
     FROM recipes r
     LEFT JOIN recipe_ingredients ri ON ri.recipe_id = r.id
     LEFT JOIN ingredients i ON i.id = ri.ingredient_id
     WHERE r.id = ANY($1)
     GROUP BY r.id
     ORDER BY r.name ASC`,
    [ids]
  );

  const map = new Map<number, RecipeWithIngredients>();
  for (const row of result.rows) {
    map.set(row.id, {
      id: row.id,
      name: row.name,
      active: row.active,
      created_at: row.created_at,
      ingredients: (row.ingredient_ids ?? []).map((iid, idx) => {
        const allergenIdSet = (row.allergen_id_sets ?? [])[idx];
        const allergenNameSet = (row.allergen_name_sets ?? [])[idx];

        const allergenIds = allergenIdSet ? allergenIdSet.split(',') : [];
        const allergenNames = allergenNameSet ? allergenNameSet.split(',') : [];

        const allergens: Allergen[] = allergenIds.map((aid, aidx) => ({
          id: aid,
          name: allergenNames[aidx],
        }));

        return {
          ingredient_id: iid,
          name: (row.ingredient_names ?? [])[idx],
          amount: parseFloat((row.ingredient_amounts ?? [])[idx]),
          unit: (row.ingredient_units ?? [])[idx],
          allergens,
        } satisfies RecipeIngredient;
      }),
    });
  }
  return map;
}

export async function getRecipesWithIngredients(
  db: Pool,
  opts: { includeInactive?: boolean } = {}
): Promise<RecipeWithIngredients[]> {
  // Single query — aggregates ingredients and their allergens per recipe.
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
       ARRAY_AGG(i.id      ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_ids,
       ARRAY_AGG(i.name    ORDER BY i.name) FILTER (WHERE i.name IS NOT NULL)   AS ingredient_names,
       ARRAY_AGG(ri.amount ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_amounts,
       ARRAY_AGG(ri.unit   ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)     AS ingredient_units,
       -- Allergens are aggregated per ingredient into a comma-separated string,
       -- then those strings are collected into an array aligned with ingredient_ids.
       ARRAY_AGG(
         (SELECT STRING_AGG(a.id::TEXT, ',' ORDER BY a.name)
          FROM ingredient_allergens ia
          JOIN allergens a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_id_sets,
       ARRAY_AGG(
         (SELECT STRING_AGG(a.name, ',' ORDER BY a.name)
          FROM ingredient_allergens ia
          JOIN allergens a ON a.id = ia.allergen_id
          WHERE ia.ingredient_id = i.id)
         ORDER BY i.name
       ) FILTER (WHERE i.id IS NOT NULL) AS allergen_name_sets
     FROM recipes r
     LEFT JOIN recipe_ingredients ri ON ri.recipe_id = r.id
     LEFT JOIN ingredients i ON i.id = ri.ingredient_id
     ${opts.includeInactive ? '' : 'WHERE r.active = TRUE'}
     GROUP BY r.id
     ORDER BY r.name ASC`,
  );

  return result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    active: row.active,
    created_at: row.created_at,
    ingredients: (row.ingredient_ids ?? []).map((iid, idx) => {
      const allergenIdSet = (row.allergen_id_sets ?? [])[idx];
      const allergenNameSet = (row.allergen_name_sets ?? [])[idx];

      const allergenIds = allergenIdSet ? allergenIdSet.split(',') : [];
      const allergenNames = allergenNameSet ? allergenNameSet.split(',') : [];

      const allergens: Allergen[] = allergenIds.map((aid, aidx) => ({
        id: aid,
        name: allergenNames[aidx],
      }));

      return {
        ingredient_id: iid,
        name: (row.ingredient_names ?? [])[idx],
        amount: parseFloat((row.ingredient_amounts ?? [])[idx]),
        unit: (row.ingredient_units ?? [])[idx],
        allergens,
      } satisfies RecipeIngredient;
    }),
  }));
}

// Returns just the ingredients for a recipe — used internally and
// useful when the caller already has the recipe row.
export async function getRecipeIngredients(
  db: Pool,
  recipeId: string
): Promise<RecipeIngredient[]> {
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
     FROM recipe_ingredients ri
     JOIN ingredients i ON i.id = ri.ingredient_id
     LEFT JOIN ingredient_allergens ia ON ia.ingredient_id = i.id
     LEFT JOIN allergens a ON a.id = ia.allergen_id
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
    allergens: (row.allergen_ids ?? []).map((aid, idx) => ({
      id: aid,
      name: (row.allergen_names ?? [])[idx],
    })),
  }));
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createRecipe(db: Pool, data: InsertRecipe): Promise<Recipe> {
  const result = await db.query<Recipe>(
    `INSERT INTO recipes (name, active)
     VALUES ($1, $2)
     RETURNING id, name, active, created_at`,
    [data.name.trim(), data.active ?? true]
  );
  return result.rows[0];
}

export async function updateRecipe(
  db: Pool,
  id: string,
  data: UpdateRecipe
): Promise<Recipe | null> {
  const result = await db.query<Recipe>(
    `UPDATE recipes
     SET
       name   = COALESCE($1, name),
       active = COALESCE($2, active)
     WHERE id = $3
     RETURNING id, name, active, created_at`,
    [data.name?.trim() ?? null, data.active ?? null, id]
  );
  return result.rows[0] ?? null;
}

// Soft delete only. Hard delete is blocked by ON DELETE RESTRICT on orders.
// Even if no orders exist yet, we never hard delete recipes —
// historical data integrity depends on this.
export async function deactivateRecipe(db: Pool, id: string): Promise<Recipe | null> {
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
  unit: string
): Promise<void> {
  await client.query(
    `INSERT INTO recipe_ingredients (recipe_id, ingredient_id, amount, unit)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (recipe_id, ingredient_id)
     DO UPDATE SET amount = EXCLUDED.amount, unit = EXCLUDED.unit`,
    [recipeId, ingredientId, amount, unit]
  );
}

export async function removeRecipeIngredient(
  client: Pool | PoolClient,
  recipeId: string,
  ingredientId: string
): Promise<void> {
  await client.query(
    `DELETE FROM recipe_ingredients
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
  ingredients: Array<{ ingredient_id: string; amount: number; unit: string }>
): Promise<void> {
  await client.query(
    `DELETE FROM recipe_ingredients WHERE recipe_id = $1`,
    [recipeId]
  );

  if (ingredients.length === 0) return;

  const values = ingredients
    .map((_, i) => `($1, $${i * 3 + 2}, $${i * 3 + 3}, $${i * 3 + 4})`)
    .join(', ');

  const params = ingredients.flatMap((ing) => [
    ing.ingredient_id,
    ing.amount,
    ing.unit,
  ]);

  await client.query(
    `INSERT INTO recipe_ingredients (recipe_id, ingredient_id, amount, unit)
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
export async function isRecipeActive(db: Pool, id: string): Promise<boolean> {
  const result = await db.query<{ active: boolean }>(
    `SELECT active FROM recipes WHERE id = $1`,
    [id]
  );
  return result.rows[0]?.active ?? false;
}
