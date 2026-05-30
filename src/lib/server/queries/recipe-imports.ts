import type { Pool, PoolClient } from 'pg';
import type { RecipeExcelRowParsed, RecipeImportResult } from '$lib/types';

// ─── Batch upserts → name-to-id maps ─────────────────────────────────────────

interface RecipeUpsertResult {
  nameToId: Map<string, number>;
  recipes_created: number;
  recipes_updated: number;
}

/**
 * Upsert recipe by name.
 * ON CONFLICT: sets active = true (re-activates soft-deleted recipe).
 * xmax = 0 → row was inserted (created); non-zero → row was updated.
 * No updated_at column required.
 */
async function upsertRecipes(
  client: PoolClient,
  names: string[],
): Promise<RecipeUpsertResult> {
  if (!names.length) return { nameToId: new Map(), recipes_created: 0, recipes_updated: 0 };

  const placeholders = names.map((_, i) => `($${i + 1})`).join(', ');

  const { rows } = await client.query<{ id: number; name: string; created: boolean }>(
    `INSERT INTO recipe (name)
     VALUES ${placeholders}
     ON CONFLICT (name) DO UPDATE SET active = true
     RETURNING id, name, (xmax = 0) AS created`,
    names,
  );

  const nameToId = new Map(rows.map((r) => [r.name, r.id]));
  const recipes_created = rows.filter((r) => r.created).length;
  const recipes_updated = rows.length - recipes_created;

  return { nameToId, recipes_created, recipes_updated };
}

/**
 * Upsert ingredient by name.
 * ON CONFLICT: no-op (ingredient has no other mutable fields here).
 * Returns a map of name → id.
 */
async function upsertIngredients(
  client: PoolClient,
  names: string[],
): Promise<Map<string, number>> {
  if (!names.length) return new Map();

  const placeholders = names.map((_, i) => `($${i + 1})`).join(', ');

  const { rows } = await client.query<{ id: number; name: string }>(
    `INSERT INTO ingredient (name)
     VALUES ${placeholders}
     ON CONFLICT (name) DO NOTHING
     RETURNING id, name`,
    names,
  );

  // DO NOTHING rows aren't returned — fetch the ones that already existed
  const returnedNames = new Set(rows.map((r) => r.name));
  const missing = names.filter((n) => !returnedNames.has(n));

  if (missing.length) {
    const mp = missing.map((_, i) => `$${i + 1}`).join(', ');
    const { rows: existing } = await client.query<{ id: number; name: string }>(
      `SELECT id, name FROM ingredient WHERE name IN (${mp})`,
      missing,
    );
    rows.push(...existing);
  }

  return new Map(rows.map((r) => [r.name, r.id]));
}

/**
 * Upsert allergen by name.
 * Returns a map of name → id.
 */
async function upsertAllergens(
  client: PoolClient,
  names: string[],
): Promise<Map<string, number>> {
  if (!names.length) return new Map();

  const placeholders = names.map((_, i) => `($${i + 1})`).join(', ');

  const { rows } = await client.query<{ id: number; name: string }>(
    `INSERT INTO allergen (name)
     VALUES ${placeholders}
     ON CONFLICT (name) DO NOTHING
     RETURNING id, name`,
    names,
  );

  const returnedNames = new Set(rows.map((r) => r.name));
  const missing = names.filter((n) => !returnedNames.has(n));

  if (missing.length) {
    const mp = missing.map((_, i) => `$${i + 1}`).join(', ');
    const { rows: existing } = await client.query<{ id: number; name: string }>(
      `SELECT id, name FROM allergen WHERE name IN (${mp})`,
      missing,
    );
    rows.push(...existing);
  }

  return new Map(rows.map((r) => [r.name, r.id]));
}

// ─── Orchestration ────────────────────────────────────────────────────────────

/**
 * Full recipe import pipeline.
 *
 * Expects pre-validated rows from parseRecipeExcel().
 * All DB writes happen inside a single transaction.
 *
 * Steps:
 *   1. Collect unique names for all three entity types
 *   2. Batch upsert recipe, ingredient, allergen → id maps
 *   3. Group rows by recipe
 *   4. Per recipe: full-replace recipe_ingredient
 *   5. Per ingredient: full-replace ingredient_allergen
 */
export async function importRecipesFromRows(
  db: Pool,
  rows: RecipeExcelRowParsed[],
  _createdBy: string, // available for audit logging if needed
): Promise<RecipeImportResult> {
  // ── 1. Collect unique entity names ──────────────────────────────────────────
  const recipeNames     = [...new Set(rows.map((r) => r.recipe))];
  const ingredientNames = [...new Set(rows.map((r) => r.ingredient).filter((n): n is string => !!n))];
  const allergenNames   = [
    ...new Set(
      rows
        .flatMap((r) => (r.allergens ? r.allergens.split(',').map((a: string) => a.trim()) : []))
        .filter(Boolean),
    ),
  ];

  const client = await db.connect();

  try {
    await client.query('BEGIN');

    // ── 2. Batch upserts ───────────────────────────────────────────────────────
    const { nameToId: recipeMap, recipes_created, recipes_updated } = await upsertRecipes(client, recipeNames);
    const ingredientMap = await upsertIngredients(client, ingredientNames);
    const allergenMap   = await upsertAllergens(client, allergenNames);

    // ── 4. Group rows by recipe ────────────────────────────────────────────────
    const rowsByRecipe = new Map<string, RecipeExcelRowParsed[]>();
    for (const row of rows) {
      if (!rowsByRecipe.has(row.recipe)) rowsByRecipe.set(row.recipe, []);
      rowsByRecipe.get(row.recipe)!.push(row);
    }

    // ── 5. Replace recipe_ingredient per recipe ───────────────────────────────
    for (const [recipeName, recipeRows] of rowsByRecipe) {
      const recipeId = recipeMap.get(recipeName);
      if (!recipeId) continue;

			const seen = new Set<number>();
			const ingredient = recipeRows
				.filter((r) => r.ingredient != null)
				.reduce<{ ingredient_id: number; amount: number; unit: string }[]>((acc, r) => {
					const ingredient_id = ingredientMap.get(r.ingredient!)!;
					if (!seen.has(ingredient_id)) {
						seen.add(ingredient_id);
						acc.push({ ingredient_id, amount: r.amount!, unit: r.unit! });
					}
					return acc;
				}, []);

      await setRecipeIngredients(client, recipeId, ingredient);
    }

    // ── 6. Replace ingredient_allergen per ingredient ─────────────────────────
    // Build ingredient → allergen ids map from rows
    const allergensByIngredient = new Map<string, number[]>();
    for (const row of rows) {
      if (!row.ingredient || !row.allergens) continue;
      const ids = row.allergens
        .split(',')
        .map((a) => a.trim())
        .filter(Boolean)
        .map((a) => allergenMap.get(a))
        .filter((id): id is number => id != null);
      allergensByIngredient.set(row.ingredient, ids);
    }

    for (const [ingredientName, allergenIds] of allergensByIngredient) {
      const ingredientId = ingredientMap.get(ingredientName);
      if (!ingredientId) continue;
      await setIngredientAllergens(client, ingredientId, allergenIds);
    }

    await client.query('COMMIT');

    return {
      recipes_created,
      recipes_updated,
      ingredients_created: ingredientNames.length, // approximation; refine if needed
      rows_processed: rows.length,
      errors: [],
    };
  } catch (err) {
    await client.query('ROLLBACK');
    throw err;
  } finally {
    client.release();
  }
}

// ─── Local implementations of setRecipeIngredients / setIngredientAllergens ──
// These mirror the existing query functions but accept a PoolClient for use
// inside the transaction above. If your existing functions already accept
// Pool | PoolClient, import and use those instead.

interface RecipeIngredientInput {
  ingredient_id: number;
  amount: number;
  unit: string;
}

async function setRecipeIngredients(
  client: PoolClient,
  recipeId: number,
  ingredient: RecipeIngredientInput[],
): Promise<void> {
  await client.query('DELETE FROM recipe_ingredient WHERE recipe_id = $1', [recipeId]);

  if (!ingredient.length) return;

  const placeholders = ingredient
    .map((_, i) => `($1, $${i * 3 + 2}, $${i * 3 + 3}, $${i * 3 + 4})`)
    .join(', ');
  const values: unknown[] = [recipeId];
  for (const ing of ingredient) {
    values.push(ing.ingredient_id, ing.amount, ing.unit);
  }

  await client.query(
    `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
     VALUES ${placeholders}`,
    values,
  );
}

async function setIngredientAllergens(
  client: PoolClient,
  ingredientId: number,
  allergenIds: number[],
): Promise<void> {
  await client.query('DELETE FROM ingredient_allergen WHERE ingredient_id = $1', [ingredientId]);

  if (!allergenIds.length) return;

  const placeholders = allergenIds.map((_, i) => `($1, $${i + 2})`).join(', ');
  await client.query(
    `INSERT INTO ingredient_allergen (ingredient_id, allergen_id)
     VALUES ${placeholders}`,
    [ingredientId, ...allergenIds],
  );
}
