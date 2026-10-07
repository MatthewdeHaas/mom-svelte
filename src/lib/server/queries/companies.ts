import { ConflictError } from '$lib/server/errors';
import type { Company, InsertCompany, UpdateCompany, UserWithCompany } from '$lib/types';
import type { Db } from '$lib/server/db';
import {
  type PaginationParams,
  type PaginatedResult,
  paginationSQL,
  buildPaginatedResult,
} from './pagination';
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
// passed AND `use_rust_backend_companies` is on for that caller, the
// call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — adding
// `caller` to specific routes is a follow-up pass once we're ready
// to flip the flag per-company.
//
// Companies are global master data, but scoping differs from recipes:
// clients can only read their OWN company row. The Rust handlers
// enforce this server-side; the dispatcher only needs to forward.
//
// The wire shapes for `Company` and `UserWithCompany` match the Rust
// `CompanyRow` / `UserWithCompanyRow` field-for-field, so decoded JSON
// from `internalJson` can be returned as-is.
// -------------------------------------------------------------

/**
 * 404-aware wrapper around `internalJson`. The TS SQL path returns
 * `null` for mutations targeting a missing row (updateCompany,
 * deactivateCompany) and for reads that miss (getCompanyById,
 * getCompanyByName); the Rust path returns 404. Translate so the
 * dispatcher is behaviourally transparent.
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

/**
 * 409-aware wrapper for create/update where the TS surface throws
 * `ConflictError('A company with this name already exists')` on a
 * UNIQUE-name violation. Rust returns 409 with the same message in
 * the body; translate so callers can't tell the paths apart.
 */
async function internalJsonConflictAware<T>(
  path: string,
  caller: InternalCaller,
  opts?: Parameters<typeof internalJson>[2]
): Promise<T> {
  try {
    return await internalJson<T>(path, caller, opts);
  } catch (err) {
    if (isInternalApiError(err) && err.status === 409) {
      throw new ConflictError('A company with this name already exists');
    }
    throw err;
  }
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getCompanies(
  db: Db,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Company[]> {
  if (caller && (await useRustBackend('companies', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJson<Company[]>(`/api/companies${qs}`, caller);
  }

  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM company
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getCompaniesPaginated(
  db: Db,
  opts: { includeInactive?: boolean; search?: string },
  pagination: PaginationParams,
  caller?: InternalCaller
): Promise<PaginatedResult<Company>> {
  if (caller && (await useRustBackend('companies', caller))) {
    const merged = {
      include_inactive: opts.includeInactive,
      search: opts.search,
      page: pagination.page,
      per_page: pagination.perPage,
    };
    const qs = filtersToQueryString(merged);
    return internalJson<PaginatedResult<Company>>(`/api/companies/paginated${qs}`, caller);
  }

  const conditions: string[] = [];
  const params: unknown[] = [];

  if (!opts.includeInactive) {
    conditions.push('active = TRUE');
  }
  if (opts.search) {
    params.push(`%${opts.search}%`);
    conditions.push(`LOWER(name) LIKE LOWER($${params.length})`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  const countResult = await db.query<{ count: string }>(
    `SELECT COUNT(*) AS count FROM company ${where}`,
    params
  );
  const total = parseInt(countResult.rows[0].count, 10);

  const pag = paginationSQL(pagination, params.length + 1);
  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM company
     ${where}
     ORDER BY name ASC
     ${pag.sql}`,
    [...params, ...pag.values]
  );

  return buildPaginatedResult(result.rows, total, pagination);
}

export async function getCompanyById(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Company | null> {
  if (caller && (await useRustBackend('companies', caller))) {
    return internalJsonOrNull<Company>(`/api/companies/${id}`, caller);
  }

  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM company
     WHERE id = $1`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getCompanyByName(
  db: Db,
  name: string,
  caller?: InternalCaller
): Promise<Company | null> {
  if (caller && (await useRustBackend('companies', caller))) {
    const qs = filtersToQueryString({ name });
    return internalJsonOrNull<Company>(`/api/companies/by-name${qs}`, caller);
  }

  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM company
     WHERE LOWER(name) = LOWER($1)`,
    [name]
  );
  return result.rows[0] ?? null;
}

// Returns all users belonging to a company, each with the company embedded.
// Useful for the company detail page's user list.
export async function getUsersByCompany(
  db: Db,
  companyId: string,
  caller?: InternalCaller
): Promise<UserWithCompany[]> {
  if (caller && (await useRustBackend('companies', caller))) {
    return internalJson<UserWithCompany[]>(`/api/companies/${companyId}/users`, caller);
  }

  const result = await db.query<UserWithCompany & {
    company_name: string;
    company_contact_email: string | null;
    company_phone: string | null;
    company_shipping_address: string | null;
    company_active: boolean;
    company_created_at: Date;
  }>(
    `SELECT
       u.id, u.company_id, u.email, u.name, u.role, u."createdAt" AS created_at,
       c.name              AS company_name,
       c.contact_email     AS company_contact_email,
       c.phone             AS company_phone,
       c.shipping_address  AS company_shipping_address,
       c.active            AS company_active,
       c.created_at        AS company_created_at
     FROM "user" u
     JOIN company c ON c.id = u.company_id
     WHERE u.company_id = $1
     ORDER BY u.email ASC`,
    [companyId]
  );

  return result.rows.map((row) => ({
    id: row.id,
    company_id: row.company_id,
    email: row.email,
    name: row.name,
    role: row.role,
    created_at: row.created_at,
    company: {
      id: row.company_id!,
      name: row.company_name,
      contact_email: row.company_contact_email,
      phone: row.company_phone,
      shipping_address: row.company_shipping_address,
      active: row.company_active,
      created_at: row.company_created_at,
    },
  }));
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createCompany(
  db: Db,
  data: InsertCompany,
  caller?: InternalCaller
): Promise<Company> {
  if (caller && (await useRustBackend('companies', caller))) {
    return internalJsonConflictAware<Company>('/api/companies', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  try {
    const result = await db.query<Company>(
      `INSERT INTO company (name, contact_email, phone, shipping_address, active)
       VALUES ($1, $2, $3, $4, $5)
       RETURNING id, name, contact_email, phone, shipping_address, active, created_at`,
      [
        data.name.trim(),
        data.contact_email ?? null,
        data.phone ?? null,
        data.shipping_address ?? null,
        data.active ?? true,
      ]
    );
    return result.rows[0];
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('A company with this name already exists');
    throw err;
  }
}

export async function updateCompany(
  db: Db,
  id: string,
  data: UpdateCompany,
  caller?: InternalCaller
): Promise<Company | null> {
  if (caller && (await useRustBackend('companies', caller))) {
    // 404 → null; 409 → ConflictError (parity with SQL path).
    try {
      return await internalJson<Company>(`/api/companies/${id}`, caller, {
        method: 'PATCH',
        body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
      });
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return null;
      if (isInternalApiError(err) && err.status === 409) {
        throw new ConflictError('A company with this name already exists');
      }
      throw err;
    }
  }

  try {
    const result = await db.query<Company>(
      `UPDATE company
       SET
         name             = COALESCE($1, name),
         contact_email    = COALESCE($2, contact_email),
         phone            = COALESCE($3, phone),
         shipping_address = COALESCE($4, shipping_address),
         active           = COALESCE($5, active)
       WHERE id = $6
       RETURNING id, name, contact_email, phone, shipping_address, active, created_at`,
      [
        data.name?.trim() ?? null,
        data.contact_email ?? null,
        data.phone ?? null,
        data.shipping_address ?? null,
        data.active ?? null,
        id,
      ]
    );
    return result.rows[0] ?? null;
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('A company with this name already exists');
    throw err;
  }
}

// Soft delete — company own persons and orders so hard delete is destructive.
// Setting active = false preserves all historical data.
export async function deactivateCompany(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Company | null> {
  if (caller && (await useRustBackend('companies', caller))) {
    return internalJsonOrNull<Company>(`/api/companies/${id}/deactivate`, caller, {
      method: 'POST',
    });
  }

  return updateCompany(db, id, { active: false });
}
