import type { Db } from '$lib/server/db';
import type { Invite, InsertInvite } from '$lib/types';
import {
  useRustBackend,
  internalJson,
  isInternalApiError,
  filtersToQueryString,
  normalizeBody,
  type InternalCaller,
} from './_rust-dispatch';

// -------------------------------------------------------------
// RUST DISPATCH
//
// Every exported function below takes an optional `caller`. When
// passed AND `use_rust_backend_invites` is on for that caller, the
// call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — threading
// `caller` into `src/routes/admin/*` is a follow-up pass per the
// allergens/persons precedent. The one unauthenticated TS consumer
// (`src/routes/invite/[token]/+page.server.ts`, the token-acceptance
// landing page) keeps the SQL path in this commit; threading `caller`
// through that route is a separate scope decision.
//
// Invites are admin-only across the board on the Rust side — every
// handler hard-rejects client callers with 403. The dispatcher doesn't
// need any scoping logic — the server enforces it.
//
// The wire shape for `Invite` matches the Rust `InviteRow`
// field-for-field (timestamps come back as RFC3339 strings; the TS
// `Invite` type's `expires_at`/`used_at`/`created_at` are typed as
// `string | Date` on the pg side too, so decoded JSON passes through
// unmodified). The `markInviteUsed`/`revokeInvite` 404→no-op mapping
// below preserves the SQL path's silence on missing ids.
// -------------------------------------------------------------

/**
 * 404-aware wrapper. The TS SQL path returns `null` for `getInviteByToken`
 * misses; the Rust path returns 404. Translate so the dispatcher is
 * behaviourally transparent.
 */
async function internalJsonOrNull<T>(
  path: string,
  caller: InternalCaller,
  opts?: Parameters<typeof internalJson>[2]
): Promise<T | null> {
  try {
    return await internalJson<T>(path, caller, opts);
  } catch (err) {
    if (isInternalApiError(err) && err.status === 404) return null;
    throw err;
  }
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getInviteByToken(
  db: Db,
  token: string,
  caller?: InternalCaller
): Promise<Invite | null> {
  if (caller && (await useRustBackend('invites', caller))) {
    const qs = filtersToQueryString({ token });
    return internalJsonOrNull<Invite>(`/api/invites/by-token${qs}`, caller);
  }

  const result = await db.query<Invite>(
    `SELECT id, company_id, email, token, expires_at, used_at, created_at
     FROM invite
     WHERE token = $1`,
    [token]
  );
  return result.rows[0] ?? null;
}

export async function getInvitesByCompany(
  db: Db,
  companyId: string,
  caller?: InternalCaller
): Promise<Invite[]> {
  if (caller && (await useRustBackend('invites', caller))) {
    const qs = filtersToQueryString({ company_id: companyId });
    return internalJson<Invite[]>(`/api/invites${qs}`, caller);
  }

  const result = await db.query<Invite>(
    `SELECT id, company_id, email, token, expires_at, used_at, created_at
     FROM invite
     WHERE company_id = $1
     ORDER BY created_at DESC`,
    [companyId]
  );
  return result.rows;
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createInvite(
  db: Db,
  data: InsertInvite,
  caller?: InternalCaller
): Promise<Invite> {
  if (caller && (await useRustBackend('invites', caller))) {
    // Rust handler already lowercases + trims on its side (mirrors
    // `invites.ts:38`); send the raw body and let it do the work.
    return internalJson<Invite>('/api/invites', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  const result = await db.query<Invite>(
    `INSERT INTO invite (company_id, email, token, expires_at)
     VALUES ($1, $2, $3, $4)
     RETURNING id, company_id, email, token, expires_at, used_at, created_at`,
    [data.company_id, data.email.toLowerCase().trim(), data.token, data.expires_at]
  );
  return result.rows[0];
}

export async function markInviteUsed(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('invites', caller))) {
    // SQL path is silent on missing ids (no row-count check); Rust
    // returns 404. Swallow 404 to preserve the void-no-op contract.
    try {
      await internalJson<void>(`/api/invites/${id}/mark-used`, caller, { method: 'POST' });
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return;
      throw err;
    }
    return;
  }

  await db.query(`UPDATE invite SET used_at = now() WHERE id = $1`, [id]);
}

export async function revokeInvite(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('invites', caller))) {
    try {
      await internalJson<void>(`/api/invites/${id}/revoke`, caller, { method: 'POST' });
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return;
      throw err;
    }
    return;
  }

  await db.query(`UPDATE invite SET used_at = now() WHERE id = $1`, [id]);
}
