import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getRecipes,
  getRecipeById,
  getRecipeByName,
  getRecipeWithIngredients,
  getRecipesWithIngredients,
  getRecipeIngredients,
  createRecipe,
  updateRecipe,
  deactivateRecipe,
  addRecipeIngredient,
  removeRecipeIngredient,
  setRecipeIngredients,
  isRecipeActive,
} from '$lib/server/queries/recipes';
import { createIngredient } from '$lib/server/queries/ingredients';
import { createAllergen } from '$lib/server/queries/allergens';
import { setIngredientAllergens } from '$lib/server/queries/ingredients';
import { createTestContext } from './helpers';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let counter = 0;
function uniqueName(prefix = 'Recipe') {
  return `${prefix}_${++counter}_${Date.now()}`;
}

async function seedRecipe(overrides: Partial<Parameters<typeof createRecipe>[1]> = {}) {
  return createRecipe(ctx.client, { name: uniqueName(), ...overrides });
}

async function seedIngredient(name = uniqueName('Ing')) {
  return createIngredient(ctx.client, { name, unit: 'g' });
}

async function seedAllergen(name = uniqueName('Allergen')) {
  return createAllergen(ctx.client, { name });
}

// ---------------------------------------------------------------------------
// getRecipes
// ---------------------------------------------------------------------------

describe('getRecipes', () => {
  it('excludes inactive recipes by default', async () => {
    const active = await seedRecipe();
    const inactive = await seedRecipe();
    await deactivateRecipe(ctx.client, inactive.id);

    const ids = (await getRecipes(ctx.client)).map((r) => r.id);

    expect(ids).toContain(active.id);
    expect(ids).not.toContain(inactive.id);
  });

  it('includes inactive recipes when includeInactive is true', async () => {
    const inactive = await seedRecipe();
    await deactivateRecipe(ctx.client, inactive.id);

    const ids = (await getRecipes(ctx.client, { includeInactive: true })).map((r) => r.id);
    expect(ids).toContain(inactive.id);
  });

  it('returns rows ordered by name ascending', async () => {
    await seedRecipe({ name: 'Zucchini Soup' });
    await seedRecipe({ name: 'Apple Salad' });

    const rows = await getRecipes(ctx.client);
    const names = rows.map((r) => r.name);

    expect(names.indexOf('Apple Salad')).toBeLessThan(names.indexOf('Zucchini Soup'));
  });

  it('returns rows with the expected shape', async () => {
    const r = await seedRecipe({ name: uniqueName('Shape') });
    const rows = await getRecipes(ctx.client);
    const found = rows.find((row) => row.id === r.id);

    expect(found).toBeDefined();
    expect(typeof found!.id).toBe('string');
    expect(found!.active).toBe(true);
    expect(found!.created_at).toBeInstanceOf(Date);
  });
});

// ---------------------------------------------------------------------------
// getRecipeById
// ---------------------------------------------------------------------------

describe('getRecipeById', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getRecipeById(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns null for an inactive recipe by default', async () => {
    const r = await seedRecipe();
    await deactivateRecipe(ctx.client, r.id);

    expect(await getRecipeById(ctx.client, r.id)).toBeNull();
  });

  it('returns inactive recipe when includeInactive is true', async () => {
    const r = await seedRecipe();
    await deactivateRecipe(ctx.client, r.id);

    const found = await getRecipeById(ctx.client, r.id, { includeInactive: true });
    expect(found).not.toBeNull();
    expect(found!.active).toBe(false);
  });

  it('returns the correct recipe with all fields', async () => {
    const r = await seedRecipe({ name: uniqueName('ById') });

    const found = await getRecipeById(ctx.client, r.id);

    expect(found!.id).toBe(r.id);
    expect(found!.name).toBe(r.name);
    expect(found!.active).toBe(true);
    expect(found!.created_at).toBeInstanceOf(Date);
  });
});

// ---------------------------------------------------------------------------
// getRecipeByName
// ---------------------------------------------------------------------------

describe('getRecipeByName', () => {
  it('returns null for a non-existent name', async () => {
    expect(await getRecipeByName(ctx.client, 'Definitely Not Real')).toBeNull();
  });

  it('returns null for an inactive recipe by default', async () => {
    const r = await seedRecipe({ name: uniqueName('InactiveName') });
    await deactivateRecipe(ctx.client, r.id);

    expect(await getRecipeByName(ctx.client, r.name)).toBeNull();
  });

  it('returns inactive recipe when includeInactive is true', async () => {
    const r = await seedRecipe({ name: uniqueName('InactiveName') });
    await deactivateRecipe(ctx.client, r.id);

    const found = await getRecipeByName(ctx.client, r.name, { includeInactive: true });
    expect(found!.id).toBe(r.id);
  });

  it('is case-insensitive', async () => {
    const r = await seedRecipe({ name: 'Case Soup' });

    const found = await getRecipeByName(ctx.client, 'case soup');
    expect(found!.id).toBe(r.id);
  });
});

// ---------------------------------------------------------------------------
// getRecipeIngredients
// ---------------------------------------------------------------------------

describe('getRecipeIngredients', () => {
  it('returns an empty array for a recipe with no ingredients', async () => {
    const r = await seedRecipe();
    expect(await getRecipeIngredients(ctx.client, r.id)).toEqual([]);
  });

  it('returns ingredients with correct fields', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 150, 'g');

    const result = await getRecipeIngredients(ctx.client, r.id);

    expect(result).toHaveLength(1);
    expect(result[0].ingredient_id).toBe(ing.id);
    expect(result[0].name).toBe(ing.name);
    expect(result[0].unit).toBe('g');
  });

  it('returns amount as a number (parseFloat applied)', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 42.5, 'ml');

    const result = await getRecipeIngredients(ctx.client, r.id);

    expect(typeof result[0].amount).toBe('number');
    expect(result[0].amount).toBeCloseTo(42.5);
  });

  it('returns ingredients ordered by name ascending', async () => {
    const r = await seedRecipe();
    const z = await seedIngredient('Zucchini');
    const a = await seedIngredient('Aubergine');

    await addRecipeIngredient(ctx.client, r.id, z.id, 10, 'g');
    await addRecipeIngredient(ctx.client, r.id, a.id, 20, 'g');

    const result = await getRecipeIngredients(ctx.client, r.id);
    const names = result.map((i) => i.name);

    expect(names.indexOf('Aubergine')).toBeLessThan(names.indexOf('Zucchini'));
  });

  it('includes allergens on each ingredient', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();
    const allergen = await seedAllergen();

    await setIngredientAllergens(ctx.client, ing.id, [allergen.id]);
    await addRecipeIngredient(ctx.client, r.id, ing.id, 10, 'g');

    const result = await getRecipeIngredients(ctx.client, r.id);
    const allergenIds = result[0].allergens.map((a) => a.id);

    expect(allergenIds).toContain(allergen.id);
  });

  it('returns empty allergens array when ingredient has none', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 10, 'g');

    const result = await getRecipeIngredients(ctx.client, r.id);
    expect(result[0].allergens).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// getRecipeWithIngredients
// ---------------------------------------------------------------------------

describe('getRecipeWithIngredients', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getRecipeWithIngredients(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns null for an inactive recipe by default', async () => {
    const r = await seedRecipe();
    await deactivateRecipe(ctx.client, r.id);

    expect(await getRecipeWithIngredients(ctx.client, r.id)).toBeNull();
  });

  it('returns the recipe with an empty ingredients array when none set', async () => {
    const r = await seedRecipe();

    const found = await getRecipeWithIngredients(ctx.client, r.id);

    expect(found).not.toBeNull();
    expect(found!.ingredients).toEqual([]);
  });

  it('returns the recipe with its ingredients', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 100, 'g');

    const found = await getRecipeWithIngredients(ctx.client, r.id);
    expect(found!.ingredients.some((i) => i.ingredient_id === ing.id)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// getRecipesWithIngredients
// ---------------------------------------------------------------------------

describe('getRecipesWithIngredients', () => {
  it('excludes inactive recipes by default', async () => {
    const inactive = await seedRecipe();
    await deactivateRecipe(ctx.client, inactive.id);

    const rows = await getRecipesWithIngredients(ctx.client);
    expect(rows.some((r) => r.id === inactive.id)).toBe(false);
  });

  it('returns ingredients arrays for all recipes', async () => {
    await seedRecipe();
    const rows = await getRecipesWithIngredients(ctx.client);
    expect(rows.every((r) => Array.isArray(r.ingredients))).toBe(true);
  });

  it('correctly attaches ingredients to each recipe without cross-contamination', async () => {
    const r1 = await seedRecipe();
    const r2 = await seedRecipe();
    const ing1 = await seedIngredient();
    const ing2 = await seedIngredient();

    await addRecipeIngredient(ctx.client, r1.id, ing1.id, 10, 'g');
    await addRecipeIngredient(ctx.client, r2.id, ing2.id, 20, 'g');

    const rows = await getRecipesWithIngredients(ctx.client);
    const row1 = rows.find((r) => r.id === r1.id);
    const row2 = rows.find((r) => r.id === r2.id);

    expect(row1!.ingredients.some((i) => i.ingredient_id === ing1.id)).toBe(true);
    expect(row2!.ingredients.some((i) => i.ingredient_id === ing2.id)).toBe(true);
    expect(row1!.ingredients.some((i) => i.ingredient_id === ing2.id)).toBe(false);
  });

  it('correctly unpacks allergens per ingredient', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();
    const allergen = await seedAllergen();

    await setIngredientAllergens(ctx.client, ing.id, [allergen.id]);
    await addRecipeIngredient(ctx.client, r.id, ing.id, 10, 'g');

    const rows = await getRecipesWithIngredients(ctx.client);
    const found = rows.find((row) => row.id === r.id);
    const ingRow = found!.ingredients.find((i) => i.ingredient_id === ing.id);

    expect(ingRow!.allergens.some((a) => a.id === allergen.id)).toBe(true);
  });

  it('amount is a number not a string', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 99.9, 'ml');

    const rows = await getRecipesWithIngredients(ctx.client);
    const found = rows.find((row) => row.id === r.id);
    const ingRow = found!.ingredients[0];

    expect(typeof ingRow.amount).toBe('number');
    expect(ingRow.amount).toBeCloseTo(99.9);
  });
});

// ---------------------------------------------------------------------------
// createRecipe
// ---------------------------------------------------------------------------

describe('createRecipe', () => {
  it('returns the inserted recipe with a string id', async () => {
    const name = uniqueName('New');
    const result = await createRecipe(ctx.client, { name });

    expect(typeof result.id).toBe('string');
    expect(result.name).toBe(name);
    expect(result.active).toBe(true);
    expect(result.created_at).toBeInstanceOf(Date);
  });

  it('trims whitespace from the name', async () => {
    const result = await createRecipe(ctx.client, { name: '  Trimmed Recipe  ' });
    expect(result.name).toBe('Trimmed Recipe');
  });

  it('respects explicit active: false', async () => {
    const result = await createRecipe(ctx.client, { name: uniqueName('Inactive'), active: false });
    expect(result.active).toBe(false);
  });

  it('throws on duplicate name (UNIQUE constraint)', async () => {
    const name = uniqueName('Dup');
    await createRecipe(ctx.client, { name });
    await expect(createRecipe(ctx.client, { name })).rejects.toThrow();
  });
});

// ---------------------------------------------------------------------------
// updateRecipe
// ---------------------------------------------------------------------------

describe('updateRecipe', () => {
  it('updates name only, leaving active unchanged', async () => {
    const r = await seedRecipe();
    const newName = uniqueName('Updated');

    const updated = await updateRecipe(ctx.client, r.id, { name: newName });

    expect(updated!.name).toBe(newName);
    expect(updated!.active).toBe(true);
  });

  it('updates active only, leaving name unchanged', async () => {
    const r = await seedRecipe();

    const updated = await updateRecipe(ctx.client, r.id, { active: false });

    expect(updated!.active).toBe(false);
    expect(updated!.name).toBe(r.name);
  });

  it('trims whitespace from updated name', async () => {
    const r = await seedRecipe();
    const updated = await updateRecipe(ctx.client, r.id, { name: '  Trimmed  ' });
    expect(updated!.name).toBe('Trimmed');
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await updateRecipe(ctx.client, '00000000-0000-0000-0000-000000000000', { name: 'Ghost' })
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// deactivateRecipe
// ---------------------------------------------------------------------------

describe('deactivateRecipe', () => {
  it('sets active to false and returns the row', async () => {
    const r = await seedRecipe();
    const result = await deactivateRecipe(ctx.client, r.id);

    expect(result).not.toBeNull();
    expect(result!.active).toBe(false);
  });

  it('deactivated recipe is excluded from getRecipes by default', async () => {
    const r = await seedRecipe();
    await deactivateRecipe(ctx.client, r.id);

    const ids = (await getRecipes(ctx.client)).map((row) => row.id);
    expect(ids).not.toContain(r.id);
  });

  it('deactivated recipe is still retrievable with includeInactive', async () => {
    const r = await seedRecipe();
    await deactivateRecipe(ctx.client, r.id);

    const found = await getRecipeById(ctx.client, r.id, { includeInactive: true });
    expect(found).not.toBeNull();
    expect(found!.active).toBe(false);
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await deactivateRecipe(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// addRecipeIngredient / removeRecipeIngredient
// ---------------------------------------------------------------------------

describe('addRecipeIngredient', () => {
  it('associates an ingredient with amount and unit', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 200, 'g');

    const ings = await getRecipeIngredients(ctx.client, r.id);
    expect(ings.some((i) => i.ingredient_id === ing.id)).toBe(true);
  });

  it('upserts on conflict — updates amount and unit rather than throwing', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing.id, 100, 'g');
    await addRecipeIngredient(ctx.client, r.id, ing.id, 250, 'ml');

    const ings = await getRecipeIngredients(ctx.client, r.id);
    const row = ings.find((i) => i.ingredient_id === ing.id);

    expect(ings).toHaveLength(1);
    expect(row!.amount).toBeCloseTo(250);
    expect(row!.unit).toBe('ml');
  });
});

describe('removeRecipeIngredient', () => {
  it('removes a specific ingredient association', async () => {
    const r = await seedRecipe();
    const ing1 = await seedIngredient();
    const ing2 = await seedIngredient();

    await addRecipeIngredient(ctx.client, r.id, ing1.id, 10, 'g');
    await addRecipeIngredient(ctx.client, r.id, ing2.id, 20, 'g');
    await removeRecipeIngredient(ctx.client, r.id, ing1.id);

    const ings = await getRecipeIngredients(ctx.client, r.id);
    const ids = ings.map((i) => i.ingredient_id);

    expect(ids).not.toContain(ing1.id);
    expect(ids).toContain(ing2.id);
  });

  it('does not throw when removing a non-existent association', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await expect(removeRecipeIngredient(ctx.client, r.id, ing.id)).resolves.not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// setRecipeIngredients
// ---------------------------------------------------------------------------

describe('setRecipeIngredients', () => {
  it('sets ingredients and they appear on the recipe', async () => {
    const r = await seedRecipe();
    const ing1 = await seedIngredient();
    const ing2 = await seedIngredient();

    await setRecipeIngredients(ctx.client, r.id, [
      { ingredient_id: ing1.id, amount: 100, unit: 'g' },
      { ingredient_id: ing2.id, amount: 200, unit: 'ml' },
    ]);

    const ings = await getRecipeIngredients(ctx.client, r.id);
    const ids = ings.map((i) => i.ingredient_id);

    expect(ids).toContain(ing1.id);
    expect(ids).toContain(ing2.id);
  });

  it('replaces existing ingredients on a second call', async () => {
    const r = await seedRecipe();
    const old = await seedIngredient();
    const next = await seedIngredient();

    await setRecipeIngredients(ctx.client, r.id, [{ ingredient_id: old.id, amount: 10, unit: 'g' }]);
    await setRecipeIngredients(ctx.client, r.id, [{ ingredient_id: next.id, amount: 20, unit: 'g' }]);

    const ings = await getRecipeIngredients(ctx.client, r.id);
    const ids = ings.map((i) => i.ingredient_id);

    expect(ids).not.toContain(old.id);
    expect(ids).toContain(next.id);
  });

  it('clears all ingredients when called with an empty array', async () => {
    const r = await seedRecipe();
    const ing = await seedIngredient();

    await setRecipeIngredients(ctx.client, r.id, [{ ingredient_id: ing.id, amount: 5, unit: 'g' }]);
    await setRecipeIngredients(ctx.client, r.id, []);

    expect(await getRecipeIngredients(ctx.client, r.id)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// isRecipeActive
// ---------------------------------------------------------------------------

describe('isRecipeActive', () => {
  it('returns true for an active recipe', async () => {
    const r = await seedRecipe();
    expect(await isRecipeActive(ctx.client, r.id)).toBe(true);
  });

  it('returns false for a deactivated recipe', async () => {
    const r = await seedRecipe();
    await deactivateRecipe(ctx.client, r.id);

    expect(await isRecipeActive(ctx.client, r.id)).toBe(false);
  });

  it('returns false for a non-existent id', async () => {
    expect(await isRecipeActive(ctx.client, '00000000-0000-0000-0000-000000000000')).toBe(false);
  });

  it('returns false even when recipe was created with active: false', async () => {
    const r = await createRecipe(ctx.client, { name: uniqueName('InactiveFromStart'), active: false });
    expect(await isRecipeActive(ctx.client, r.id)).toBe(false);
  });
});
