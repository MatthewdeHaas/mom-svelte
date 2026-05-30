import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getPersonsByCompany,
  getPersonById,
  getPersonWithAllergens,
  getPersonsWithAllergensByCompany,
  getPersonAllergens,
  createPerson,
  updatePerson,
  deactivatePerson,
  addPersonAllergen,
  removePersonAllergen,
  setPersonAllergens,
  getAttendanceByDate,
  upsertAttendance,
  upsertAttendanceBatch,
} from '$lib/server/queries/persons';
import { createCompany } from '$lib/server/queries/companies';
import { createAllergen } from '$lib/server/queries/allergens';
import { createTestContext } from './helpers';
import type { InsertPerson } from '$lib/types';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let counter = 0;
function uniqueName(prefix = 'Person') {
  return `${prefix}_${++counter}_${Date.now()}`;
}

async function seedCompany() {
  return createCompany(ctx.client, { name: uniqueName('Company') });
}

async function seedAllergen() {
  return createAllergen(ctx.client, { name: uniqueName('Allergen') });
}

async function seedPerson(companyId: string, overrides: Partial<InsertPerson> = {}) {
  return createPerson(ctx.client, {
    company_id: companyId,
    name: uniqueName(),
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// getPersonsByCompany
// ---------------------------------------------------------------------------

describe('getPersonsByCompany', () => {
  it('returns an empty array for a company with no persons', async () => {
    const c = await seedCompany();
    expect(await getPersonsByCompany(ctx.client, c.id)).toEqual([]);
  });

  it('returns only active persons by default', async () => {
    const c = await seedCompany();
    const active = await seedPerson(c.id);
    const inactive = await seedPerson(c.id);
    await deactivatePerson(ctx.client, inactive.id);

    const ids = (await getPersonsByCompany(ctx.client, c.id)).map((p) => p.id);

    expect(ids).toContain(active.id);
    expect(ids).not.toContain(inactive.id);
  });

  it('includes inactive persons when includeInactive is true', async () => {
    const c = await seedCompany();
    const inactive = await seedPerson(c.id);
    await deactivatePerson(ctx.client, inactive.id);

    const ids = (await getPersonsByCompany(ctx.client, c.id, { includeInactive: true })).map((p) => p.id);
    expect(ids).toContain(inactive.id);
  });

  it('does not return persons from a different company', async () => {
    const c1 = await seedCompany();
    const c2 = await seedCompany();
    await seedPerson(c2.id);

    const rows = await getPersonsByCompany(ctx.client, c1.id);
    expect(rows).toHaveLength(0);
  });

  it('returns rows ordered by name ascending', async () => {
    const c = await seedCompany();
    await createPerson(ctx.client, { company_id: c.id, name: 'Zara' });
    await createPerson(ctx.client, { company_id: c.id, name: 'Alice' });

    const rows = await getPersonsByCompany(ctx.client, c.id);
    const names = rows.map((p) => p.name);

    expect(names.indexOf('Alice')).toBeLessThan(names.indexOf('Zara'));
  });
});

// ---------------------------------------------------------------------------
// getPersonById
// ---------------------------------------------------------------------------

describe('getPersonById', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getPersonById(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns null for an inactive person by default', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    await deactivatePerson(ctx.client, p.id);

    expect(await getPersonById(ctx.client, p.id)).toBeNull();
  });

  it('returns inactive person when includeInactive is true', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    await deactivatePerson(ctx.client, p.id);

    const found = await getPersonById(ctx.client, p.id, { includeInactive: true });
    expect(found!.active).toBe(false);
  });

  it('returns the correct person', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const found = await getPersonById(ctx.client, p.id);
    expect(found!.id).toBe(p.id);
    expect(found!.company_id).toBe(c.id);
  });
});

// ---------------------------------------------------------------------------
// getPersonWithAllergens
// ---------------------------------------------------------------------------

describe('getPersonWithAllergens', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getPersonWithAllergens(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns person with empty allergens array when none set', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const found = await getPersonWithAllergens(ctx.client, p.id);
    expect(found!.allergen).toEqual([]);
  });

  it('returns person with allergens', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a = await seedAllergen();

    await addPersonAllergen(ctx.client, p.id, a.id);

    const found = await getPersonWithAllergens(ctx.client, p.id);
    expect(found!.allergen.map((al) => al.id)).toContain(a.id);
  });
});

// ---------------------------------------------------------------------------
// getPersonsWithAllergensByCompany
// ---------------------------------------------------------------------------

describe('getPersonsWithAllergensByCompany', () => {
  it('returns an empty array for a company with no persons', async () => {
    const c = await seedCompany();
    expect(await getPersonsWithAllergensByCompany(ctx.client, c.id)).toEqual([]);
  });

  it('returns persons each with an allergens array', async () => {
    const c = await seedCompany();
    await seedPerson(c.id);

    const rows = await getPersonsWithAllergensByCompany(ctx.client, c.id);
    expect(rows.every((p) => Array.isArray(p.allergen))).toBe(true);
  });

  it('correctly attaches allergens without cross-contamination', async () => {
    const c = await seedCompany();
    const p1 = await seedPerson(c.id);
    const p2 = await seedPerson(c.id);
    const a1 = await seedAllergen();
    const a2 = await seedAllergen();

    await addPersonAllergen(ctx.client, p1.id, a1.id);
    await addPersonAllergen(ctx.client, p2.id, a2.id);

    const rows = await getPersonsWithAllergensByCompany(ctx.client, c.id);
    const row1 = rows.find((p) => p.id === p1.id);
    const row2 = rows.find((p) => p.id === p2.id);

    expect(row1!.allergen.map((a) => a.id)).toContain(a1.id);
    expect(row2!.allergen.map((a) => a.id)).toContain(a2.id);
    expect(row1!.allergen.map((a) => a.id)).not.toContain(a2.id);
  });

  it('excludes inactive persons by default', async () => {
    const c = await seedCompany();
    const inactive = await seedPerson(c.id);
    await deactivatePerson(ctx.client, inactive.id);

    const rows = await getPersonsWithAllergensByCompany(ctx.client, c.id);
    expect(rows.some((p) => p.id === inactive.id)).toBe(false);
  });

  it('does not return persons from other companies', async () => {
    const c1 = await seedCompany();
    const c2 = await seedCompany();
    await seedPerson(c2.id);

    const rows = await getPersonsWithAllergensByCompany(ctx.client, c1.id);
    expect(rows).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// getPersonAllergens
// ---------------------------------------------------------------------------

describe('getPersonAllergens', () => {
  it('returns an empty array when person has no allergens', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    expect(await getPersonAllergens(ctx.client, p.id)).toEqual([]);
  });

  it('returns allergens ordered by name ascending', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const z = await createAllergen(ctx.client, { name: 'ZZZAllergen' });
    const a = await createAllergen(ctx.client, { name: 'AAAAllergen' });

    await setPersonAllergens(ctx.client, p.id, [z.id, a.id]);

    const allergens = await getPersonAllergens(ctx.client, p.id);
    expect(allergens[0].name).toBe('AAAAllergen');
    expect(allergens[1].name).toBe('ZZZAllergen');
  });
});

// ---------------------------------------------------------------------------
// createPerson
// ---------------------------------------------------------------------------

describe('createPerson', () => {
  it('returns the person with a string id and active: true', async () => {
    const c = await seedCompany();
    const p = await createPerson(ctx.client, { company_id: c.id, name: 'Alice' });

    expect(typeof p.id).toBe('string');
    expect(p.active).toBe(true);
    expect(p.company_id).toBe(c.id);
  });

  it('trims whitespace from the name', async () => {
    const c = await seedCompany();
    const p = await createPerson(ctx.client, { company_id: c.id, name: '  Bob  ' });
    expect(p.name).toBe('Bob');
  });

  it('respects explicit active: false', async () => {
    const c = await seedCompany();
    const p = await createPerson(ctx.client, { company_id: c.id, name: 'Inactive', active: false });
    expect(p.active).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// updatePerson
// ---------------------------------------------------------------------------

describe('updatePerson', () => {
  it('updates name only, leaving active unchanged', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const updated = await updatePerson(ctx.client, p.id, { name: 'NewName' });

    expect(updated!.name).toBe('NewName');
    expect(updated!.active).toBe(true);
  });

  it('updates active only, leaving name unchanged', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const updated = await updatePerson(ctx.client, p.id, { active: false });

    expect(updated!.active).toBe(false);
    expect(updated!.name).toBe(p.name);
  });

  it('trims whitespace from updated name', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const updated = await updatePerson(ctx.client, p.id, { name: '  Trimmed  ' });
    expect(updated!.name).toBe('Trimmed');
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await updatePerson(ctx.client, '00000000-0000-0000-0000-000000000000', { name: 'Ghost' })
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// deactivatePerson
// ---------------------------------------------------------------------------

describe('deactivatePerson', () => {
  it('sets active to false and returns the row', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const result = await deactivatePerson(ctx.client, p.id);
    expect(result!.active).toBe(false);
  });

  it('deactivated person is excluded from getPersonsByCompany by default', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    await deactivatePerson(ctx.client, p.id);

    const ids = (await getPersonsByCompany(ctx.client, c.id)).map((r) => r.id);
    expect(ids).not.toContain(p.id);
  });

  it('deactivated person is still retrievable with includeInactive', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    await deactivatePerson(ctx.client, p.id);

    const found = await getPersonById(ctx.client, p.id, { includeInactive: true });
    expect(found!.active).toBe(false);
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await deactivatePerson(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// addPersonAllergen / removePersonAllergen / setPersonAllergens
// ---------------------------------------------------------------------------

describe('addPersonAllergen', () => {
  it('associates an allergen with a person', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a = await seedAllergen();

    await addPersonAllergen(ctx.client, p.id, a.id);

    const allergens = await getPersonAllergens(ctx.client, p.id);
    expect(allergens.map((al) => al.id)).toContain(a.id);
  });

  it('is idempotent (ON CONFLICT DO NOTHING)', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a = await seedAllergen();

    await addPersonAllergen(ctx.client, p.id, a.id);
    await expect(addPersonAllergen(ctx.client, p.id, a.id)).resolves.not.toThrow();

    const allergens = await getPersonAllergens(ctx.client, p.id);
    expect(allergens).toHaveLength(1);
  });
});

describe('removePersonAllergen', () => {
  it('removes a specific allergen', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a1 = await seedAllergen();
    const a2 = await seedAllergen();

    await setPersonAllergens(ctx.client, p.id, [a1.id, a2.id]);
    await removePersonAllergen(ctx.client, p.id, a1.id);

    const allergens = await getPersonAllergens(ctx.client, p.id);
    const ids = allergens.map((a) => a.id);

    expect(ids).not.toContain(a1.id);
    expect(ids).toContain(a2.id);
  });

  it('does not throw when removing a non-existent association', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a = await seedAllergen();

    await expect(removePersonAllergen(ctx.client, p.id, a.id)).resolves.not.toThrow();
  });
});

describe('setPersonAllergens', () => {
  it('sets allergens and they appear on the person', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a1 = await seedAllergen();
    const a2 = await seedAllergen();

    await setPersonAllergens(ctx.client, p.id, [a1.id, a2.id]);

    const allergens = await getPersonAllergens(ctx.client, p.id);
    const ids = allergens.map((a) => a.id);

    expect(ids).toContain(a1.id);
    expect(ids).toContain(a2.id);
  });

  it('replaces existing allergens on a second call', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const old = await seedAllergen();
    const next = await seedAllergen();

    await setPersonAllergens(ctx.client, p.id, [old.id]);
    await setPersonAllergens(ctx.client, p.id, [next.id]);

    const allergens = await getPersonAllergens(ctx.client, p.id);
    const ids = allergens.map((a) => a.id);

    expect(ids).not.toContain(old.id);
    expect(ids).toContain(next.id);
  });

  it('clears all allergens when called with an empty array', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a = await seedAllergen();

    await setPersonAllergens(ctx.client, p.id, [a.id]);
    await setPersonAllergens(ctx.client, p.id, []);

    expect(await getPersonAllergens(ctx.client, p.id)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// upsertAttendance
// ---------------------------------------------------------------------------

describe('upsertAttendance', () => {
  it('inserts a new attendance record', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const result = await upsertAttendance(ctx.client, {
      person_id: p.id,
      serving_date: new Date('2025-06-15'),
      attending: true,
    });

    expect(result.person_id).toBe(p.id);
    expect(result.attending).toBe(true);
  });

  it('updates attending on conflict (same person + date)', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const date = new Date('2025-06-15');

    await upsertAttendance(ctx.client, { person_id: p.id, serving_date: date, attending: true });
    const result = await upsertAttendance(ctx.client, { person_id: p.id, serving_date: date, attending: false });

    expect(result.attending).toBe(false);
  });

  it('returns the record with serving_date as a Date', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    const result = await upsertAttendance(ctx.client, {
      person_id: p.id,
      serving_date: new Date('2025-06-15'),
      attending: true,
    });

    expect(result.serving_date).toBeInstanceOf(Date);
  });
});

// ---------------------------------------------------------------------------
// upsertAttendanceBatch
// ---------------------------------------------------------------------------

describe('upsertAttendanceBatch', () => {
  it('returns without error when given an empty array', async () => {
    await expect(upsertAttendanceBatch(ctx.client, [])).resolves.not.toThrow();
  });

  it('inserts multiple attendance records', async () => {
    const c = await seedCompany();
    const p1 = await seedPerson(c.id);
    const p2 = await seedPerson(c.id);
    const date = new Date('2025-06-15');

    await upsertAttendanceBatch(ctx.client, [
      { person_id: p1.id, serving_date: date, attending: true },
      { person_id: p2.id, serving_date: date, attending: false },
    ]);

    const records = await getAttendanceByDate(ctx.client, c.id, date);
    expect(records).toHaveLength(2);
  });

  it('updates existing records on conflict', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const date = new Date('2025-06-15');

    await upsertAttendanceBatch(ctx.client, [
      { person_id: p.id, serving_date: date, attending: true },
    ]);
    await upsertAttendanceBatch(ctx.client, [
      { person_id: p.id, serving_date: date, attending: false },
    ]);

    const records = await getAttendanceByDate(ctx.client, c.id, date);
    expect(records[0].attending).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// getAttendanceByDate
// ---------------------------------------------------------------------------

describe('getAttendanceByDate', () => {
  it('returns an empty array when no attendance exists for the date', async () => {
    const c = await seedCompany();
    expect(
      await getAttendanceByDate(ctx.client, c.id, new Date('2025-06-15'))
    ).toEqual([]);
  });

  it('returns attendance records with embedded person', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const date = new Date('2025-06-15');

    await upsertAttendance(ctx.client, { person_id: p.id, serving_date: date, attending: true });

    const records = await getAttendanceByDate(ctx.client, c.id, date);

    expect(records).toHaveLength(1);
    expect(records[0].person.id).toBe(p.id);
    expect(records[0].person.name).toBe(p.name);
    expect(records[0].attending).toBe(true);
  });

  it('includes allergens on the embedded person', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);
    const a = await seedAllergen();
    const date = new Date('2025-06-15');

    await addPersonAllergen(ctx.client, p.id, a.id);
    await upsertAttendance(ctx.client, { person_id: p.id, serving_date: date, attending: true });

    const records = await getAttendanceByDate(ctx.client, c.id, date);
    const allergenIds = records[0].person.allergen.map((al) => al.id);

    expect(allergenIds).toContain(a.id);
  });

  it('does not return attendance from a different company', async () => {
    const c1 = await seedCompany();
    const c2 = await seedCompany();
    const p2 = await seedPerson(c2.id);
    const date = new Date('2025-06-15');

    await upsertAttendance(ctx.client, { person_id: p2.id, serving_date: date, attending: true });

    const records = await getAttendanceByDate(ctx.client, c1.id, date);
    expect(records).toHaveLength(0);
  });

  it('does not return attendance from a different date', async () => {
    const c = await seedCompany();
    const p = await seedPerson(c.id);

    await upsertAttendance(ctx.client, {
      person_id: p.id,
      serving_date: new Date('2025-06-15'),
      attending: true,
    });

    const records = await getAttendanceByDate(ctx.client, c.id, new Date('2025-06-16'));
    expect(records).toHaveLength(0);
  });

  it('returns records ordered by person name ascending', async () => {
    const c = await seedCompany();
    const zara = await createPerson(ctx.client, { company_id: c.id, name: 'Zara' });
    const alice = await createPerson(ctx.client, { company_id: c.id, name: 'Alice' });
    const date = new Date('2025-06-15');

    await upsertAttendanceBatch(ctx.client, [
      { person_id: zara.id, serving_date: date, attending: true },
      { person_id: alice.id, serving_date: date, attending: true },
    ]);

    const records = await getAttendanceByDate(ctx.client, c.id, date);
    const names = records.map((r) => r.person.name);

    expect(names.indexOf('Alice')).toBeLessThan(names.indexOf('Zara'));
  });
});
