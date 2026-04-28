import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getCompanies,
  getCompanyById,
  getCompanyByName,
  getUsersByCompany,
  createCompany,
  updateCompany,
  deactivateCompany,
} from '$lib/server/queries/companies';
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
function uniqueName(prefix = 'Company') {
  return `${prefix}_${++counter}_${Date.now()}`;
}

async function seedCompany(overrides: Partial<Parameters<typeof createCompany>[1]> = {}) {
  return createCompany(ctx.client, {
    name: uniqueName(),
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// getCompanies
// ---------------------------------------------------------------------------

describe('getCompanies', () => {
  it('excludes inactive companies by default', async () => {
    const active = await seedCompany();
    const inactive = await seedCompany();
    await deactivateCompany(ctx.client, inactive.id);

    const ids = (await getCompanies(ctx.client)).map((r) => r.id);

    expect(ids).toContain(active.id);
    expect(ids).not.toContain(inactive.id);
  });

  it('includes inactive companies when includeInactive is true', async () => {
    const inactive = await seedCompany();
    await deactivateCompany(ctx.client, inactive.id);

    const ids = (await getCompanies(ctx.client, { includeInactive: true })).map((r) => r.id);
    expect(ids).toContain(inactive.id);
  });

  it('returns rows ordered by name ascending', async () => {
    await seedCompany({ name: 'Zeta Corp' });
    await seedCompany({ name: 'Alpha Corp' });

    const rows = await getCompanies(ctx.client);
    const names = rows.map((r) => r.name);

    expect(names.indexOf('Alpha Corp')).toBeLessThan(names.indexOf('Zeta Corp'));
  });

  it('returns rows with the expected shape', async () => {
    const c = await seedCompany({ name: 'ShapeCo', contact_email: 'a@b.com' });
    const rows = await getCompanies(ctx.client);
    const found = rows.find((r) => r.id === c.id);

    expect(found).toBeDefined();
    expect(typeof found!.id).toBe('string');
    expect(found!.name).toBe('ShapeCo');
    expect(found!.contact_email).toBe('a@b.com');
    expect(found!.active).toBe(true);
    expect(found!.created_at).toBeInstanceOf(Date);
  });
});

// ---------------------------------------------------------------------------
// getCompanyById
// ---------------------------------------------------------------------------

describe('getCompanyById', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getCompanyById(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns inactive companies (no active filter on this query)', async () => {
    const c = await seedCompany();
    await deactivateCompany(ctx.client, c.id);

    const found = await getCompanyById(ctx.client, c.id);
    expect(found).not.toBeNull();
    expect(found!.active).toBe(false);
  });

  it('returns the correct company with all fields', async () => {
    const c = await createCompany(ctx.client, {
      name: uniqueName('Full'),
      contact_email: 'full@example.com',
      phone: '555-1234',
      shipping_address: '123 Main St',
    });

    const found = await getCompanyById(ctx.client, c.id);

    expect(found).not.toBeNull();
    expect(found!.id).toBe(c.id);
    expect(found!.contact_email).toBe('full@example.com');
    expect(found!.phone).toBe('555-1234');
    expect(found!.shipping_address).toBe('123 Main St');
  });
});

// ---------------------------------------------------------------------------
// getCompanyByName
// ---------------------------------------------------------------------------

describe('getCompanyByName', () => {
  it('returns null for a non-existent name', async () => {
    expect(await getCompanyByName(ctx.client, 'Definitely Not Real')).toBeNull();
  });

  it('returns the correct company by exact name', async () => {
    const c = await seedCompany({ name: 'Exact Match Co' });

    const found = await getCompanyByName(ctx.client, 'Exact Match Co');
    expect(found!.id).toBe(c.id);
  });

  it('is case-insensitive', async () => {
    const c = await seedCompany({ name: 'Case Test Co' });

    const found = await getCompanyByName(ctx.client, 'case test co');
    expect(found!.id).toBe(c.id);
  });
});

// ---------------------------------------------------------------------------
// createCompany
// ---------------------------------------------------------------------------

describe('createCompany', () => {
  it('returns the inserted company with a string id', async () => {
    const name = uniqueName('NewCo');
    const result = await createCompany(ctx.client, { name });

    expect(typeof result.id).toBe('string');
    expect(result.name).toBe(name);
    expect(result.active).toBe(true);
  });

  it('trims whitespace from the name', async () => {
    const result = await createCompany(ctx.client, { name: '  Trimmed Co  ' });
    expect(result.name).toBe('Trimmed Co');
  });

  it('nulls optional fields when not provided', async () => {
    const result = await createCompany(ctx.client, { name: uniqueName('Minimal') });

    expect(result.contact_email).toBeNull();
    expect(result.phone).toBeNull();
    expect(result.shipping_address).toBeNull();
  });

  it('respects explicit active: false', async () => {
    const result = await createCompany(ctx.client, { name: uniqueName('Inactive'), active: false });
    expect(result.active).toBe(false);
  });

  it('throws on duplicate name (UNIQUE constraint)', async () => {
    const name = uniqueName('Dup');
    await createCompany(ctx.client, { name });

    await expect(createCompany(ctx.client, { name })).rejects.toThrow();
  });
});

// ---------------------------------------------------------------------------
// updateCompany
// ---------------------------------------------------------------------------

describe('updateCompany', () => {
  it('updates name only, leaving other fields unchanged', async () => {
    const c = await createCompany(ctx.client, {
      name: uniqueName('Before'),
      phone: '555-0000',
    });
    const newName = uniqueName('After');

    const updated = await updateCompany(ctx.client, c.id, { name: newName });

    expect(updated!.name).toBe(newName);
    expect(updated!.phone).toBe('555-0000');
  });

  it('updates optional fields independently', async () => {
    const c = await seedCompany();

    const updated = await updateCompany(ctx.client, c.id, {
      contact_email: 'new@example.com',
      phone: '555-9999',
      shipping_address: '99 New St',
    });

    expect(updated!.contact_email).toBe('new@example.com');
    expect(updated!.phone).toBe('555-9999');
    expect(updated!.shipping_address).toBe('99 New St');
  });

  it('trims whitespace from updated name', async () => {
    const c = await seedCompany();
    const updated = await updateCompany(ctx.client, c.id, { name: '  Trimmed  ' });
    expect(updated!.name).toBe('Trimmed');
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await updateCompany(ctx.client, '00000000-0000-0000-0000-000000000000', { name: 'Ghost' })
    ).toBeNull();
  });

  it('change is reflected in a subsequent read', async () => {
    const c = await seedCompany();
    const newName = uniqueName('ReadBack');

    await updateCompany(ctx.client, c.id, { name: newName });

    const fetched = await getCompanyById(ctx.client, c.id);
    expect(fetched!.name).toBe(newName);
  });
});

// ---------------------------------------------------------------------------
// deactivateCompany
// ---------------------------------------------------------------------------

describe('deactivateCompany', () => {
  it('sets active to false and returns the row', async () => {
    const c = await seedCompany();
    const result = await deactivateCompany(ctx.client, c.id);

    expect(result).not.toBeNull();
    expect(result!.active).toBe(false);
  });

  it('deactivated company is excluded from getCompanies by default', async () => {
    const c = await seedCompany();
    await deactivateCompany(ctx.client, c.id);

    const ids = (await getCompanies(ctx.client)).map((r) => r.id);
    expect(ids).not.toContain(c.id);
  });

  it('deactivated company is still retrievable by id', async () => {
    const c = await seedCompany();
    await deactivateCompany(ctx.client, c.id);

    const found = await getCompanyById(ctx.client, c.id);
    expect(found).not.toBeNull();
    expect(found!.active).toBe(false);
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await deactivateCompany(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('does not affect other companies', async () => {
    const a = await seedCompany();
    const b = await seedCompany();

    await deactivateCompany(ctx.client, b.id);

    const found = await getCompanyById(ctx.client, a.id);
    expect(found!.active).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// getUsersByCompany
// ---------------------------------------------------------------------------

describe('getUsersByCompany', () => {
  it('returns an empty array for a company with no users', async () => {
    const c = await seedCompany();
    expect(await getUsersByCompany(ctx.client, c.id)).toEqual([]);
  });

  it('returns users belonging to the company with company embedded', async () => {
    const c = await seedCompany({ name: uniqueName('UserCo') });

    await ctx.client.query(
      `INSERT INTO users (id, company_id, email, role) VALUES ($1, $2, $3, $4)`,
      ['test-user-1', c.id, 'user@example.com', 'client']
    );

    const users = await getUsersByCompany(ctx.client, c.id);

    expect(users).toHaveLength(1);
    expect(users[0].email).toBe('user@example.com');
    expect(users[0].company.id).toBe(c.id);
    expect(users[0].company.name).toBe(c.name);
  });

  it('does not return users from a different company', async () => {
    const c1 = await seedCompany();
    const c2 = await seedCompany();

    await ctx.client.query(
      `INSERT INTO users (id, company_id, email, role) VALUES ($1, $2, $3, $4)`,
      ['test-user-z', c2.id, 'other@example.com', 'client']
    );

    const users = await getUsersByCompany(ctx.client, c1.id);
    expect(users.some((u) => u.email === 'other@example.com')).toBe(false);
  });

  it('returns users ordered by email ascending', async () => {
    const c = await seedCompany();

    await ctx.client.query(
      `INSERT INTO users (id, company_id, email, role) VALUES ($1, $2, $3, $4), ($5, $2, $6, $4)`,
      ['test-user-z', c.id, 'z@example.com', 'client', 'test-user-a', 'a@example.com']
    );

    const users = await getUsersByCompany(ctx.client, c.id);
    const emails = users.map((u) => u.email);

    expect(emails.indexOf('a@example.com')).toBeLessThan(emails.indexOf('z@example.com'));
  });
});
