import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getIngredients,
  getIngredientById,
  getIngredientWithAllergens,
  getIngredientsWithAllergens,
  createIngredient,
  updateIngredient,
  deactivateIngredient,
  addIngredientAllergen,
  removeIngredientAllergen,
  setIngredientAllergens,
} from '$lib/server/queries/ingredients';
import { createAllergen } from '$lib/server/queries/allergens';
import { createTestContext } from './helpers';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function seedIngredient(name = 'Test Ingredient', active = true) {
  return createIngredient(ctx.client, { name, active });
}

async function seedAllergen(name: string) {
  return createAllergen(ctx.client, { name });
}

// ---------------------------------------------------------------------------
// getIngredients
// ---------------------------------------------------------------------------

describe('getIngredients', () => {
  it('returns only active ingredients by default', async () => {
    const active = await seedIngredient('Active');
    const inactive = await seedIngredient('Inactive');
    await deactivateIngredient(ctx.client, inactive.id);

    const rows = await getIngredients(ctx.client);
    const ids = rows.map((r) => r.id);

    expect(ids).toContain(active.id);
    expect(ids).not.toContain(inactive.id);
  });

  it('returns inactive ingredients when includeInactive is true', async () => {
    const inactive = await seedIngredient('Inactive');
    await deactivateIngredient(ctx.client, inactive.id);

    const rows = await getIngredients(ctx.client, { includeInactive: true });
    const ids = rows.map((r) => r.id);

    expect(ids).toContain(inactive.id);
  });

  it('returns rows with id, name, and active fields', async () => {
    await seedIngredient('Flour');

    const rows = await getIngredients(ctx.client);
    const flour = rows.find((r) => r.name === 'Flour');

    expect(flour).toBeDefined();
    expect(typeof flour!.id).toBe('string');
    expect(flour!.active).toBe(true);
  });

  it('returns results ordered by name ascending', async () => {
    await seedIngredient('Zucchini');
    await seedIngredient('Apple');

    const rows = await getIngredients(ctx.client);
    const names = rows.map((r) => r.name);
    const appleIdx = names.indexOf('Apple');
    const zucchiniIdx = names.indexOf('Zucchini');

    expect(appleIdx).toBeLessThan(zucchiniIdx);
  });
});

// ---------------------------------------------------------------------------
// getIngredientById
// ---------------------------------------------------------------------------

describe('getIngredientById', () => {
  it('returns null for a non-existent id', async () => {
    expect(await getIngredientById(ctx.client, '00000000-0000-0000-0000-000000000000')).toBeNull();
  });

  it('returns null for an inactive ingredient by default', async () => {
    const ing = await seedIngredient('Inactive');
    await deactivateIngredient(ctx.client, ing.id);

    expect(await getIngredientById(ctx.client, ing.id)).toBeNull();
  });

  it('returns inactive ingredient when includeInactive is true', async () => {
    const ing = await seedIngredient('Inactive');
    await deactivateIngredient(ctx.client, ing.id);

    const found = await getIngredientById(ctx.client, ing.id, { includeInactive: true });
    expect(found).not.toBeNull();
    expect(found!.active).toBe(false);
  });

  it('returns the correct ingredient', async () => {
    const ing = await seedIngredient('Salt');

    const found = await getIngredientById(ctx.client, ing.id);

    expect(found).not.toBeNull();
    expect(found!.id).toBe(ing.id);
    expect(found!.name).toBe('Salt');
    expect(found!.active).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// getIngredientWithAllergens
// ---------------------------------------------------------------------------

describe('getIngredientWithAllergens', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getIngredientWithAllergens(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns the ingredient with an empty allergens array when none are set', async () => {
    const ing = await seedIngredient('Plain');

    const found = await getIngredientWithAllergens(ctx.client, ing.id);

    expect(found).not.toBeNull();
    expect(found!.allergen).toEqual([]);
  });

  it('returns the ingredient with its allergens', async () => {
    const ing = await seedIngredient('Bread');
    const gluten = await seedAllergen('Gluten');
    const wheat = await seedAllergen('Wheat');

    await setIngredientAllergens(ctx.client, ing.id, [gluten.id, wheat.id]);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    const allergenIds = found!.allergen.map((a) => a.id);

    expect(allergenIds).toContain(gluten.id);
    expect(allergenIds).toContain(wheat.id);
  });

  it('allergens are ordered by name ascending', async () => {
    const ing = await seedIngredient('Pasta');
    const z = await seedAllergen('Zzallergen');
    const a = await seedAllergen('Aallergen');

    await setIngredientAllergens(ctx.client, ing.id, [z.id, a.id]);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    const names = found!.allergen.map((a) => a.name);

    expect(names[0]).toBe('Aallergen');
    expect(names[1]).toBe('Zzallergen');
  });
});

// ---------------------------------------------------------------------------
// getIngredientsWithAllergens
// ---------------------------------------------------------------------------

describe('getIngredientsWithAllergens', () => {
  it('returns ingredients with empty allergens arrays when none are set', async () => {
    await seedIngredient('Plain');

    const rows = await getIngredientsWithAllergens(ctx.client);
    expect(rows.every((r) => Array.isArray(r.allergen))).toBe(true);
  });

  it('correctly attaches allergens to each ingredient', async () => {
    const ing1 = await seedIngredient('Bread');
    const ing2 = await seedIngredient('Milk');
    const gluten = await seedAllergen('GlutenB');
    const dairy = await seedAllergen('DairyB');

    await setIngredientAllergens(ctx.client, ing1.id, [gluten.id]);
    await setIngredientAllergens(ctx.client, ing2.id, [dairy.id]);

    const rows = await getIngredientsWithAllergens(ctx.client);
    const bread = rows.find((r) => r.id === ing1.id);
    const milk = rows.find((r) => r.id === ing2.id);

    expect(bread!.allergen.map((a) => a.id)).toContain(gluten.id);
    expect(milk!.allergen.map((a) => a.id)).toContain(dairy.id);
    expect(bread!.allergen.map((a) => a.id)).not.toContain(dairy.id);
  });

  it('excludes inactive ingredients by default', async () => {
    const inactive = await seedIngredient('Inactive');
    await deactivateIngredient(ctx.client, inactive.id);

    const rows = await getIngredientsWithAllergens(ctx.client);
    expect(rows.some((r) => r.id === inactive.id)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// createIngredient
// ---------------------------------------------------------------------------

describe('createIngredient', () => {
  it('returns the inserted ingredient with a string id', async () => {
    const result = await createIngredient(ctx.client, { name: 'Pepper' });

    expect(result.name).toBe('Pepper');
    expect(typeof result.id).toBe('string');
    expect(result.active).toBe(true);
  });

  it('trims whitespace from the name', async () => {
    const result = await createIngredient(ctx.client, { name: '  Basil  ' });
    expect(result.name).toBe('Basil');
  });

  it('respects explicit active: false', async () => {
    const result = await createIngredient(ctx.client, { name: 'Inactive', active: false });
    expect(result.active).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// updateIngredient
// ---------------------------------------------------------------------------

describe('updateIngredient', () => {
  it('updates name only when only name is provided', async () => {
    const ing = await seedIngredient('OldName');
    const updated = await updateIngredient(ctx.client, ing.id, { name: 'NewName' });

    expect(updated!.name).toBe('NewName');
    expect(updated!.active).toBe(true); // unchanged
  });

  it('updates active only when only active is provided', async () => {
    const ing = await seedIngredient('StillActive');
    const updated = await updateIngredient(ctx.client, ing.id, { active: false });

    expect(updated!.active).toBe(false);
    expect(updated!.name).toBe('StillActive'); // unchanged
  });

  it('trims whitespace from updated name', async () => {
    const ing = await seedIngredient('Before');
    const updated = await updateIngredient(ctx.client, ing.id, { name: '  After  ' });
    expect(updated!.name).toBe('After');
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await updateIngredient(ctx.client, '00000000-0000-0000-0000-000000000000', { name: 'Ghost' })
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// deactivateIngredient
// ---------------------------------------------------------------------------

describe('deactivateIngredient', () => {
  it('sets active to false and returns the row', async () => {
    const ing = await seedIngredient('WillDeactivate');
    const result = await deactivateIngredient(ctx.client, ing.id);

    expect(result).not.toBeNull();
    expect(result!.active).toBe(false);
  });

  it('deactivated ingredient is not returned by getIngredients', async () => {
    const ing = await seedIngredient('Hidden');
    await deactivateIngredient(ctx.client, ing.id);

    const rows = await getIngredients(ctx.client);
    expect(rows.some((r) => r.id === ing.id)).toBe(false);
  });

  it('deactivated ingredient is still retrievable with includeInactive', async () => {
    const ing = await seedIngredient('StillThere');
    await deactivateIngredient(ctx.client, ing.id);

    const found = await getIngredientById(ctx.client, ing.id, { includeInactive: true });
    expect(found).not.toBeNull();
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await deactivateIngredient(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('does not affect other ingredients', async () => {
    const a = await seedIngredient('KeepMe');
    const b = await seedIngredient('DeactivateMe');

    await deactivateIngredient(ctx.client, b.id);

    expect(await getIngredientById(ctx.client, a.id)).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// addIngredientAllergen / removeIngredientAllergen
// ---------------------------------------------------------------------------

describe('addIngredientAllergen', () => {
  it('associates an allergen with an ingredient', async () => {
    const ing = await seedIngredient('Rye');
    const allergen = await seedAllergen('RyeAllergen');

    await addIngredientAllergen(ctx.client, ing.id, allergen.id);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    expect(found!.allergen.map((a) => a.id)).toContain(allergen.id);
  });

  it('is idempotent — does not throw on duplicate (ON CONFLICT DO NOTHING)', async () => {
    const ing = await seedIngredient('Oat');
    const allergen = await seedAllergen('OatAllergen');

    await addIngredientAllergen(ctx.client, ing.id, allergen.id);
    await expect(
      addIngredientAllergen(ctx.client, ing.id, allergen.id)
    ).resolves.not.toThrow();
  });
});

describe('removeIngredientAllergen', () => {
  it('removes a specific allergen association', async () => {
    const ing = await seedIngredient('Corn');
    const a1 = await seedAllergen('CornAllergen1');
    const a2 = await seedAllergen('CornAllergen2');

    await setIngredientAllergens(ctx.client, ing.id, [a1.id, a2.id]);
    await removeIngredientAllergen(ctx.client, ing.id, a1.id);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    const ids = found!.allergen.map((a) => a.id);

    expect(ids).not.toContain(a1.id);
    expect(ids).toContain(a2.id);
  });

  it('does not throw when removing a non-existent association', async () => {
    const ing = await seedIngredient('Rice');
    const allergen = await seedAllergen('RiceAllergen');

    await expect(
      removeIngredientAllergen(ctx.client, ing.id, allergen.id)
    ).resolves.not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// setIngredientAllergens
// ---------------------------------------------------------------------------

describe('setIngredientAllergens', () => {
  it('sets allergens and they appear on the ingredient', async () => {
    const ing = await seedIngredient('Bread2');
    const a1 = await seedAllergen('SetAllergen1');
    const a2 = await seedAllergen('SetAllergen2');

    await setIngredientAllergens(ctx.client, ing.id, [a1.id, a2.id]);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    const ids = found!.allergen.map((a) => a.id);

    expect(ids).toContain(a1.id);
    expect(ids).toContain(a2.id);
  });

  it('replaces existing allergens on a second call', async () => {
    const ing = await seedIngredient('Pasta2');
    const old = await seedAllergen('OldSetAllergen');
    const next = await seedAllergen('NewSetAllergen');

    await setIngredientAllergens(ctx.client, ing.id, [old.id]);
    await setIngredientAllergens(ctx.client, ing.id, [next.id]);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    const ids = found!.allergen.map((a) => a.id);

    expect(ids).not.toContain(old.id);
    expect(ids).toContain(next.id);
  });

  it('clears all allergens when called with an empty array', async () => {
    const ing = await seedIngredient('PlainRice');
    const a = await seedAllergen('ToClear');

    await setIngredientAllergens(ctx.client, ing.id, [a.id]);
    await setIngredientAllergens(ctx.client, ing.id, []);

    const found = await getIngredientWithAllergens(ctx.client, ing.id);
    expect(found!.allergen).toHaveLength(0);
  });
});
