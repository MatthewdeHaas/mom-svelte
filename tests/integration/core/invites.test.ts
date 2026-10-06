import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getInviteByToken,
  getInvitesByCompany,
  createInvite,
  markInviteUsed,
  revokeInvite,
} from '$lib/server/queries/invites';
import { createTestContext } from './helpers';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// -------------------------------------------------------------
// Helpers
// -------------------------------------------------------------

async function seedCompany(name = 'TestCo') {
  const result = await ctx.client.query<{ id: string }>(
    `INSERT INTO company (name, active) VALUES ($1, TRUE) RETURNING id`,
    [name]
  );
  return result.rows[0].id;
}

function futureExpiry(): Date {
  // 48h in the future — well outside any reasonable test runtime so
  // nothing we insert ever expires mid-suite.
  return new Date(Date.now() + 48 * 60 * 60 * 1000);
}

async function seedInvite(
  companyId: string,
  email = 'invitee@example.com',
  token = 'tok-' + Math.random().toString(36).slice(2),
  expiresAt: Date = futureExpiry()
) {
  return await createInvite(ctx.client, {
    company_id: companyId,
    email,
    token,
    expires_at: expiresAt,
  });
}

/**
 * Insert an invite with an explicit `created_at`. All vitest cases share
 * a single transaction (`createTestContext` wraps each test in BEGIN/ROLLBACK),
 * which pins `now()` to the txn start — so sequential `createInvite` calls
 * all land with identical `created_at`, and ORDER BY ties fall back to the
 * row's physical order. Tests that assert strict `created_at DESC` need to
 * stamp distinct timestamps themselves.
 */
async function seedInviteAt(
  companyId: string,
  email: string,
  token: string,
  createdAt: Date,
  expiresAt: Date = futureExpiry()
): Promise<string> {
  const result = await ctx.client.query<{ id: string }>(
    `INSERT INTO invite (company_id, email, token, expires_at, created_at)
     VALUES ($1, $2, $3, $4, $5) RETURNING id`,
    [companyId, email.toLowerCase().trim(), token, expiresAt, createdAt]
  );
  return result.rows[0].id;
}

// ---------------------------------------------------------------------------
// getInviteByToken
// ---------------------------------------------------------------------------

describe('getInviteByToken', () => {
  it('returns null when no invite matches the token', async () => {
    const result = await getInviteByToken(ctx.client, 'does-not-exist');
    expect(result).toBeNull();
  });

  it('returns the matching invite row', async () => {
    const company = await seedCompany('TokenLookupCo');
    const inserted = await seedInvite(company, 'lookup@example.com', 'tok-lookup');

    const found = await getInviteByToken(ctx.client, 'tok-lookup');

    expect(found).not.toBeNull();
    expect(found!.id).toBe(inserted.id);
    expect(found!.token).toBe('tok-lookup');
    expect(found!.email).toBe('lookup@example.com');
    expect(found!.company_id).toBe(company);
  });

  it('is case-sensitive on the token', async () => {
    // Mirrors `invites.ts:11` — `WHERE token = $1`, no LOWER(…).
    const company = await seedCompany('CaseCo');
    await seedInvite(company, 'case@example.com', 'AbcDef');

    const miss = await getInviteByToken(ctx.client, 'abcdef');
    expect(miss).toBeNull();

    const hit = await getInviteByToken(ctx.client, 'AbcDef');
    expect(hit).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// getInvitesByCompany
// ---------------------------------------------------------------------------

describe('getInvitesByCompany', () => {
  it('returns an empty array when the company has no invites', async () => {
    const company = await seedCompany('EmptyCo');
    const rows = await getInvitesByCompany(ctx.client, company);
    expect(rows).toEqual([]);
  });

  it('returns only invites for the requested company', async () => {
    const a = await seedCompany('CoA');
    const b = await seedCompany('CoB');
    await seedInvite(a, 'a1@x.com', 'tok-a1');
    await seedInvite(a, 'a2@x.com', 'tok-a2');
    await seedInvite(b, 'b1@x.com', 'tok-b1');

    const rows = await getInvitesByCompany(ctx.client, a);
    expect(rows).toHaveLength(2);
    expect(rows.every((r) => r.company_id === a)).toBe(true);
  });

  it('orders rows by created_at DESC', async () => {
    const company = await seedCompany('OrderCo');
    // All vitest tests share one transaction, so `now()` is pinned —
    // use `seedInviteAt` to stamp distinct `created_at` values and
    // actually exercise the ORDER BY clause.
    const t0 = new Date('2025-01-01T00:00:00Z');
    const t1 = new Date('2025-01-02T00:00:00Z');
    const t2 = new Date('2025-01-03T00:00:00Z');
    await seedInviteAt(company, 'first@x.com', 'tok-first', t0);
    await seedInviteAt(company, 'second@x.com', 'tok-second', t1);
    await seedInviteAt(company, 'third@x.com', 'tok-third', t2);

    const rows = await getInvitesByCompany(ctx.client, company);
    const emails = rows.map((r) => r.email);
    expect(emails).toEqual(['third@x.com', 'second@x.com', 'first@x.com']);
  });
});

// ---------------------------------------------------------------------------
// createInvite
// ---------------------------------------------------------------------------

describe('createInvite', () => {
  it('inserts a new invite and returns the full row', async () => {
    const company = await seedCompany('CreateCo');
    const expires = futureExpiry();

    const row = await createInvite(ctx.client, {
      company_id: company,
      email: 'new@example.com',
      token: 'tok-create',
      expires_at: expires,
    });

    expect(row.id).toBeDefined();
    expect(row.company_id).toBe(company);
    expect(row.email).toBe('new@example.com');
    expect(row.token).toBe('tok-create');
    expect(row.used_at).toBeNull();
    expect(row.created_at).toBeDefined();
  });

  it('lowercases and trims the email', async () => {
    // Mirrors `invites.ts:36` (`data.email.toLowerCase().trim()`).
    const company = await seedCompany('EmailCo');

    const row = await createInvite(ctx.client, {
      company_id: company,
      email: '  MixedCase@EXAMPLE.com  ',
      token: 'tok-email',
      expires_at: futureExpiry(),
    });

    expect(row.email).toBe('mixedcase@example.com');
  });

  it('throws on duplicate token (unique constraint)', async () => {
    const company = await seedCompany('DupCo');
    await createInvite(ctx.client, {
      company_id: company,
      email: 'first@x.com',
      token: 'tok-dup',
      expires_at: futureExpiry(),
    });

    await expect(
      createInvite(ctx.client, {
        company_id: company,
        email: 'second@x.com',
        token: 'tok-dup',
        expires_at: futureExpiry(),
      })
    ).rejects.toThrow();
  });

  it('throws on bad company_id (FK violation)', async () => {
    await expect(
      createInvite(ctx.client, {
        company_id: '00000000-0000-0000-0000-000000000000',
        email: 'orphan@x.com',
        token: 'tok-orphan',
        expires_at: futureExpiry(),
      })
    ).rejects.toThrow();
  });

  it('persists so the invite appears in subsequent list', async () => {
    const company = await seedCompany('PersistCo');
    await createInvite(ctx.client, {
      company_id: company,
      email: 'persist@example.com',
      token: 'tok-persist',
      expires_at: futureExpiry(),
    });

    const rows = await getInvitesByCompany(ctx.client, company);
    expect(rows).toHaveLength(1);
    expect(rows[0].email).toBe('persist@example.com');
  });
});

// ---------------------------------------------------------------------------
// markInviteUsed
// ---------------------------------------------------------------------------

describe('markInviteUsed', () => {
  it('stamps used_at on the matching invite', async () => {
    const company = await seedCompany('MarkCo');
    const invite = await seedInvite(company, 'mark@x.com', 'tok-mark');
    expect(invite.used_at).toBeNull();

    await markInviteUsed(ctx.client, invite.id);

    const after = await getInviteByToken(ctx.client, 'tok-mark');
    expect(after).not.toBeNull();
    expect(after!.used_at).not.toBeNull();
  });

  it('is a no-op for a missing id (no throw)', async () => {
    // The TS SQL path returns `void` with no row-count check — the Rust
    // service maps the same UPDATE to 404, but the SQL path intentionally
    // stays silent so the handler sees the pre-existing semantics.
    await expect(
      markInviteUsed(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).resolves.toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// revokeInvite — same SQL as markInviteUsed
// ---------------------------------------------------------------------------

describe('revokeInvite', () => {
  it('stamps used_at on the matching invite', async () => {
    const company = await seedCompany('RevokeCo');
    const invite = await seedInvite(company, 'rev@x.com', 'tok-rev');

    await revokeInvite(ctx.client, invite.id);

    const after = await getInviteByToken(ctx.client, 'tok-rev');
    expect(after).not.toBeNull();
    expect(after!.used_at).not.toBeNull();
  });

  it('is a no-op for a missing id (no throw)', async () => {
    await expect(
      revokeInvite(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).resolves.toBeUndefined();
  });
});
