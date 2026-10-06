import type { Pool, PoolClient } from 'pg';
import type {
  Person,
  InsertPerson,
  UpdatePerson,
  PersonWithAllergens,
  Allergen,
  UpsertAttendance,
  AttendanceWithPerson,
  Attendance,
} from '$lib/types';
import type { Db } from '$lib/server/db';
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
// passed AND `use_rust_backend_persons` is on for that caller, the
// call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — threading
// `caller` into `src/routes/admin/companies/[id]`, the client
// roster/attendance/menu pages, etc. is a follow-up pass per the
// allergens precedent.
//
// Persons + attendance are per-company data and the Rust handlers
// enforce admin-vs-client scoping (admin unrestricted; client
// restricted to its own `company_id`). The dispatcher doesn't need
// extra scoping logic — the server lies-about-existence for
// cross-company probes, same as orders/companies.
//
// Wire-shape normalisation:
//  - `Person` / `PersonWithAllergens` / `Allergen` row shapes match
//    the Rust `PersonRow` / `PersonWithAllergensRow` / `AllergenEmbed`
//    field-for-field — decoded JSON passes through as-is.
//  - `Attendance.serving_date` is a `Date` in the TS surface (the pg
//    driver maps `DATE` → `Date`), but Rust emits ISO `YYYY-MM-DD`.
//    `tests/integration/core/persons.test.ts:502` asserts
//    `instanceof Date` — so we rehydrate every attendance row
//    coming off the wire before returning it.
//  - Date filters / body dates (`serving_date` on upsert bodies) are
//    sent as ISO date-only to keep the Rust `NaiveDate` parser happy.
// -------------------------------------------------------------

/**
 * 404-aware wrapper. The TS SQL path returns `null` on miss for
 * single-row reads/mutations; the Rust path returns 404. Translate so
 * the dispatcher is behaviourally transparent.
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

/** Narrow a wire string (`YYYY-MM-DD`) back to a `Date`. */
function isoToDate(iso: unknown): Date {
  if (iso instanceof Date) return iso;
  // `new Date("YYYY-MM-DD")` parses as UTC midnight — exactly what the
  // pg driver produces for a `DATE` column. Matches the SQL path.
  return new Date(iso as string);
}

/** Rehydrate `serving_date` on a single attendance payload. */
function hydrateAttendance<T extends { serving_date: unknown }>(row: T): T {
  return { ...row, serving_date: isoToDate(row.serving_date) };
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getPersonsByCompany(
  db: Db,
  companyId: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Person[]> {
  if (caller && (await useRustBackend('persons', caller))) {
    const qs = filtersToQueryString({
      company_id: companyId,
      include_inactive: opts.includeInactive,
    });
    return internalJson<Person[]>(`/api/persons${qs}`, caller);
  }

  const result = await db.query<Person>(
    `SELECT id, company_id, name, active
     FROM person
     WHERE company_id = $1
     ${opts.includeInactive ? '' : 'AND active = TRUE'}
     ORDER BY name ASC`,
    [companyId]
  );
  return result.rows;
}

export async function getPersonById(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<Person | null> {
  if (caller && (await useRustBackend('persons', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJsonOrNull<Person>(`/api/persons/${id}${qs}`, caller);
  }

  const result = await db.query<Person>(
    `SELECT id, company_id, name, active
     FROM person
     WHERE id = $1
     ${opts.includeInactive ? '' : 'AND active = TRUE'}`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getPersonWithAllergens(
  db: Db,
  id: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<PersonWithAllergens | null> {
  if (caller && (await useRustBackend('persons', caller))) {
    const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
    return internalJsonOrNull<PersonWithAllergens>(
      `/api/persons/${id}/with-allergens${qs}`,
      caller
    );
  }

  const person = await getPersonById(db, id, opts);
  if (!person) return null;

  const allergen = await getPersonAllergens(db, id);
  return { ...person, allergen };
}

export async function getPersonsWithAllergensByCompany(
  db: Db,
  companyId: string,
  opts: { includeInactive?: boolean } = {},
  caller?: InternalCaller
): Promise<PersonWithAllergens[]> {
  if (caller && (await useRustBackend('persons', caller))) {
    const qs = filtersToQueryString({
      company_id: companyId,
      include_inactive: opts.includeInactive,
    });
    return internalJson<PersonWithAllergens[]>(
      `/api/persons/with-allergens${qs}`,
      caller
    );
  }

  const result = await db.query<
    Person & { allergen_ids: string[] | null; allergen_names: string[] | null }
  >(
    `SELECT
       p.id,
       p.company_id,
       p.name,
       p.active,
       ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id IS NOT NULL)   AS allergen_ids,
       ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names
     FROM person p
     LEFT JOIN person_allergen pa ON pa.person_id = p.id
     LEFT JOIN allergen a ON a.id = pa.allergen_id
     WHERE p.company_id = $1
     ${opts.includeInactive ? '' : 'AND p.active = TRUE'}
     GROUP BY p.id
     ORDER BY p.name ASC`,
    [companyId]
  );

  return result.rows.map((row) => ({
    id: row.id,
    company_id: row.company_id,
    name: row.name,
    active: row.active,
    allergen: (row.allergen_ids ?? []).map((aid, idx) => ({
      id: aid,
      name: (row.allergen_names ?? [])[idx],
    })),
  }));
}

export async function getPersonAllergens(
  db: Db,
  personId: string,
  caller?: InternalCaller
): Promise<Allergen[]> {
  if (caller && (await useRustBackend('persons', caller))) {
    return internalJson<Allergen[]>(`/api/persons/${personId}/allergens`, caller);
  }

  const result = await db.query<Allergen>(
    `SELECT a.id, a.name
     FROM allergen a
     JOIN person_allergen pa ON pa.allergen_id = a.id
     WHERE pa.person_id = $1
     ORDER BY a.name ASC`,
    [personId]
  );
  return result.rows;
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createPerson(
  db: Db,
  data: InsertPerson,
  caller?: InternalCaller
): Promise<Person> {
  if (caller && (await useRustBackend('persons', caller))) {
    return internalJson<Person>('/api/persons', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  const result = await db.query<Person>(
    `INSERT INTO person (company_id, name, active)
     VALUES ($1, $2, $3)
     RETURNING id, company_id, name, active`,
    [data.company_id, data.name.trim(), data.active ?? true]
  );
  return result.rows[0];
}

export async function updatePerson(
  db: Db,
  id: string,
  data: UpdatePerson,
  caller?: InternalCaller
): Promise<Person | null> {
  if (caller && (await useRustBackend('persons', caller))) {
    return internalJsonOrNull<Person>(`/api/persons/${id}`, caller, {
      method: 'PATCH',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  const result = await db.query<Person>(
    `UPDATE person
     SET
       name   = COALESCE($1, name),
       active = COALESCE($2, active)
     WHERE id = $3
     RETURNING id, company_id, name, active`,
    [data.name?.trim() ?? null, data.active ?? null, id]
  );
  return result.rows[0] ?? null;
}

// Soft delete — preserves attendance history and allergen records.
export async function deactivatePerson(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Person | null> {
  if (caller && (await useRustBackend('persons', caller))) {
    return internalJsonOrNull<Person>(`/api/persons/${id}/deactivate`, caller, {
      method: 'POST',
    });
  }

  return updatePerson(db, id, { active: false });
}

// -------------------------------------------------------------
// ALLERGEN ASSOCIATIONS
// -------------------------------------------------------------

export async function addPersonAllergen(
  client: Pool | PoolClient,
  personId: string,
  allergenId: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('persons', caller))) {
    await internalJson<void>(
      `/api/persons/${personId}/allergens/${allergenId}`,
      caller,
      { method: 'POST' }
    );
    return;
  }

  await client.query(
    `INSERT INTO person_allergen (person_id, allergen_id)
     VALUES ($1, $2)
     ON CONFLICT DO NOTHING`,
    [personId, allergenId]
  );
}

export async function removePersonAllergen(
  client: Pool | PoolClient,
  personId: string,
  allergenId: string,
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('persons', caller))) {
    await internalJson<void>(
      `/api/persons/${personId}/allergens/${allergenId}`,
      caller,
      { method: 'DELETE' }
    );
    return;
  }

  await client.query(
    `DELETE FROM person_allergen
     WHERE person_id = $1 AND allergen_id = $2`,
    [personId, allergenId]
  );
}

export async function setPersonAllergens(
  client: Pool | PoolClient,
  personId: string,
  allergenIds: string[],
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('persons', caller))) {
    await internalJson<void>(`/api/persons/${personId}/allergens`, caller, {
      method: 'PUT',
      body: JSON.stringify({ allergen_ids: allergenIds }),
    });
    return;
  }

  await client.query(
    `DELETE FROM person_allergen WHERE person_id = $1`,
    [personId]
  );

  if (allergenIds.length === 0) return;

  const values = allergenIds.map((_, i) => `($1, $${i + 2})`).join(', ');
  await client.query(
    `INSERT INTO person_allergen (person_id, allergen_id)
     VALUES ${values}`,
    [personId, ...allergenIds]
  );
}

// -------------------------------------------------------------
// ATTENDANCE
// -------------------------------------------------------------

export async function getAttendanceByDate(
  db: Db,
  companyId: string,
  servingDate: Date,
  caller?: InternalCaller
): Promise<AttendanceWithPerson[]> {
  if (caller && (await useRustBackend('persons', caller))) {
    const qs = filtersToQueryString({
      company_id: companyId,
      serving_date: servingDate,
    });
    const rows = await internalJson<AttendanceWithPerson[]>(
      `/api/attendance${qs}`,
      caller
    );
    // Rehydrate `serving_date` back to `Date` so callers can keep
    // treating it as the pg driver's shape (test line 502 asserts
    // `instanceof Date` on the single-row upsert; this list surface
    // leaks the same type).
    return rows.map(hydrateAttendance);
  }

  const result = await db.query<
    Attendance & {
      person_name: string;
      person_active: boolean;
      allergen_ids: string[] | null;
      allergen_names: string[] | null;
    }
  >(
    `SELECT
       att.id,
       att.person_id,
       att.serving_date,
       att.attending,
       p.name   AS person_name,
       p.active AS person_active,
       ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id IS NOT NULL)   AS allergen_ids,
       ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names
     FROM attendance att
     JOIN person p ON p.id = att.person_id
     LEFT JOIN person_allergen pa ON pa.person_id = p.id
     LEFT JOIN allergen a ON a.id = pa.allergen_id
     WHERE p.company_id  = $1
       AND att.serving_date = $2
     GROUP BY att.id, p.id
     ORDER BY p.name ASC`,
    [companyId, servingDate]
  );

  return result.rows.map((row) => ({
    id: row.id,
    person_id: row.person_id,
    serving_date: row.serving_date,
    attending: row.attending,
    person: {
      id: row.person_id,
      company_id: companyId,
      name: row.person_name,
      active: row.person_active,
      allergen: (row.allergen_ids ?? []).map((aid, idx) => ({
        id: aid,
        name: (row.allergen_names ?? [])[idx],
      })),
    },
  }));
}

// Upsert attendance for a single person on a given date.
// The unique constraint on (person_id, serving_date) makes this safe to call
// repeatedly — toggling attendance on an existing record just updates it.
export async function upsertAttendance(
  db: Db,
  data: UpsertAttendance,
  caller?: InternalCaller
): Promise<Attendance> {
  if (caller && (await useRustBackend('persons', caller))) {
    const row = await internalJson<Attendance>('/api/attendance', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
    return hydrateAttendance(row);
  }

  const result = await db.query<Attendance>(
    `INSERT INTO attendance (person_id, serving_date, attending)
     VALUES ($1, $2, $3)
     ON CONFLICT (person_id, serving_date)
     DO UPDATE SET attending = EXCLUDED.attending
     RETURNING id, person_id, serving_date, attending`,
    [data.person_id, data.serving_date, data.attending]
  );
  return result.rows[0];
}

// Bulk upsert — used when a company submits attendance for a full day at once.
export async function upsertAttendanceBatch(
  client: Pool | PoolClient,
  records: UpsertAttendance[],
  caller?: InternalCaller
): Promise<void> {
  if (caller && (await useRustBackend('persons', caller))) {
    // Rust `/api/attendance/batch` returns 204 for empty + populated;
    // forward both paths through the same request.
    await internalJson<void>('/api/attendance/batch', caller, {
      method: 'POST',
      body: JSON.stringify({
        records: records.map((r) =>
          normalizeBody(r as unknown as Record<string, unknown>)
        ),
      }),
    });
    return;
  }

  if (records.length === 0) return;

  const values = records
    .map((_, i) => `($${i * 3 + 1}, $${i * 3 + 2}, $${i * 3 + 3})`)
    .join(', ');

  const params = records.flatMap((r) => [r.person_id, r.serving_date, r.attending]);

  await client.query(
    `INSERT INTO attendance (person_id, serving_date, attending)
     VALUES ${values}
     ON CONFLICT (person_id, serving_date)
     DO UPDATE SET attending = EXCLUDED.attending`,
    params
  );
}
