import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getAllergens,
  getAllergenById,
  createAllergen,
  updateAllergen,
  deleteAllergen,
} from '$lib/server/queries/allergens';
import { createTestContext } from './helpers';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

async function seedAllergen(name = 'TestNut') {
  return await createAllergen(ctx.client, { name });
}

// ---------------------------------------------------------------------------
// getAllergens
// ---------------------------------------------------------------------------

describe('getAllergens', () => {
  it('returns an empty array when no allergens exist', async () => {
    const rows = await getAllergens(ctx.client);
    expect(rows).toEqual([]);
  });

  it('returns all inserted allergens', async () => {
    await seedAllergen('Peanut');
    await seedAllergen('Gluten');

    const rows = await getAllergens(ctx.client);
    const names = rows.map((r) => r.name);

    expect(names).toContain('Peanut');
    expect(names).toContain('Gluten');
  });

  it('returns rows with the expected shape', async () => {
    const inserted = await seedAllergen('Soy');
    const rows = await getAllergens(ctx.client);
    const row = rows.find((r) => r.id === inserted.id);

    expect(row).toBeDefined();
    expect(row!.name).toBe('Soy');
    expect(typeof row!.id).toBe('string');
  });
});

// ---------------------------------------------------------------------------
// getAllergenById
// ---------------------------------------------------------------------------

describe('getAllergenById', () => {
  it('returns null for a non-existent id', async () => {
    const result = await getAllergenById(ctx.client, '00000000-0000-0000-0000-000000000000');
    expect(result).toBeNull();
  });

  it('returns the correct allergen by id', async () => {
    const inserted = await seedAllergen('Shellfish');

    const found = await getAllergenById(ctx.client, inserted.id);

    expect(found).not.toBeNull();
    expect(found!.id).toBe(inserted.id);
    expect(found!.name).toBe('Shellfish');
  });
});

// ---------------------------------------------------------------------------
// createAllergen
// ---------------------------------------------------------------------------

describe('createAllergen', () => {
  it('inserts a new allergen and returns it with an id', async () => {
    const result = await createAllergen(ctx.client, { name: 'Wheat' });

    expect(result.name).toBe('Wheat');
    expect(typeof result.id).toBe('string');
  });

  it('trims whitespace from the name', async () => {
    const result = await createAllergen(ctx.client, { name: '  Sesame  ' });
    expect(result.name).toBe('Sesame');
  });

  it('persists so it appears in getAllergens', async () => {
    await createAllergen(ctx.client, { name: 'Mustard' });

    const all = await getAllergens(ctx.client);
    expect(all.some((r) => r.name === 'Mustard')).toBe(true);
  });

  it('throws on a duplicate name (UNIQUE constraint)', async () => {
    await createAllergen(ctx.client, { name: 'Lupin' });
    await expect(createAllergen(ctx.client, { name: 'Lupin' })).rejects.toThrow();
  });
});

// ---------------------------------------------------------------------------
// updateAllergen
// ---------------------------------------------------------------------------

describe('updateAllergen', () => {
  it('updates the name and returns the updated row', async () => {
    const inserted = await seedAllergen('OldName');

    const updated = await updateAllergen(ctx.client, inserted.id, { name: 'NewName' });

    expect(updated).not.toBeNull();
    expect(updated!.name).toBe('NewName');
    expect(updated!.id).toBe(inserted.id);
  });

  it('returns null when updating a non-existent id', async () => {
    const result = await updateAllergen(
      ctx.client,
      '00000000-0000-0000-0000-000000000000',
      { name: 'Ghost' }
    );
    expect(result).toBeNull();
  });

  it('trims whitespace from the updated name', async () => {
    const inserted = await seedAllergen('Before');
    const updated = await updateAllergen(ctx.client, inserted.id, { name: '  After  ' });
    expect(updated!.name).toBe('After');
  });

  it('the change is reflected in a subsequent read', async () => {
    const inserted = await seedAllergen('Before');
    await updateAllergen(ctx.client, inserted.id, { name: 'After' });

    const fetched = await getAllergenById(ctx.client, inserted.id);
    expect(fetched!.name).toBe('After');
  });
});

// ---------------------------------------------------------------------------
// deleteAllergen
// ---------------------------------------------------------------------------

describe('deleteAllergen', () => {
  it('returns true when deleting an existing allergen', async () => {
    const inserted = await seedAllergen('Celery');

    const result = await deleteAllergen(ctx.client, inserted.id);
    expect(result).toBe(true);
  });

  it('returns false when deleting a non-existent id', async () => {
    const result = await deleteAllergen(ctx.client, '00000000-0000-0000-0000-000000000000');
    expect(result).toBe(false);
  });

  it('allergen is no longer retrievable after deletion', async () => {
    const inserted = await seedAllergen('Lupin');
    await deleteAllergen(ctx.client, inserted.id);

    const fetched = await getAllergenById(ctx.client, inserted.id);
    expect(fetched).toBeNull();
  });

  it('does not delete other allergens', async () => {
    const a = await seedAllergen('Sulphites');
    const b = await seedAllergen('Molluscs');

    await deleteAllergen(ctx.client, a.id);

    const remaining = await getAllergenById(ctx.client, b.id);
    expect(remaining).not.toBeNull();
  });
});
