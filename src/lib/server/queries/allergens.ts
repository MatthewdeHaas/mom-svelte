import { ConflictError } from '$lib/server/errors';
import type { Allergen, InsertAllergen } from '$lib/types';
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
// passed AND `use_rust_backend_allergens` is on for that caller,
// the call forwards to the Rust service; otherwise the existing SQL
// runs unchanged. Routes stay untouched in this commit — threading
// `caller` into `src/routes/admin/allergens/*` (and the client
// roster page) is a follow-up pass per the ingredients precedent.
//
// Allergens are admin-only across the board: every Rust handler
// hard-rejects client callers with 403. The dispatcher doesn't need
// any scoping logic — the server enforces it.
//
// The wire shapes for `Allergen` and the ref-count payload match the
// Rust `AllergenRow` / `RefCount` field-for-field, so decoded JSON
// from `internalJson` is returned as-is. One serialisation detail:
// `RefCount` fields are `i64` on the Rust side which serde emits as
// JSON number — identical to the TS `Number(row.ingredients)` cast,
// no translation needed.
// -------------------------------------------------------------

/**
 * 404-aware wrapper. The TS SQL path returns `null` for reads/mutations
 * that miss (getAllergenById, getAllergenByName, updateAllergen);
 * the Rust path returns 404. Translate so the dispatcher is
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

/**
 * 409-aware wrapper for create where the TS surface throws
 * `ConflictError('An allergen with this name already exists')` on
 * a UNIQUE-name violation. Rust returns 409 with the same message;
 * translate so callers can't tell the paths apart.
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
      throw new ConflictError('An allergen with this name already exists');
    }
    throw err;
  }
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getAllergens(
  db: Db,
  caller?: InternalCaller
): Promise<Allergen[]> {
  if (caller && (await useRustBackend('allergens', caller))) {
    return internalJson<Allergen[]>('/api/allergens', caller);
  }

  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergen
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getAllergenById(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<Allergen | null> {
  if (caller && (await useRustBackend('allergens', caller))) {
    return internalJsonOrNull<Allergen>(`/api/allergens/${id}`, caller);
  }

  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergen
     WHERE id = $1`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getAllergenByName(
  db: Db,
  name: string,
  caller?: InternalCaller
): Promise<Allergen | null> {
  if (caller && (await useRustBackend('allergens', caller))) {
    const qs = filtersToQueryString({ name });
    return internalJsonOrNull<Allergen>(`/api/allergens/by-name${qs}`, caller);
  }

  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergen
     WHERE LOWER(name) = LOWER($1)`,
    [name]
  );
  return result.rows[0] ?? null;
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createAllergen(
  db: Db,
  data: InsertAllergen,
  caller?: InternalCaller
): Promise<Allergen> {
  if (caller && (await useRustBackend('allergens', caller))) {
    return internalJsonConflictAware<Allergen>('/api/allergens', caller, {
      method: 'POST',
      body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
    });
  }

  try {
    const result = await db.query<Allergen>(
      `INSERT INTO allergen (name)
       VALUES ($1)
       RETURNING id, name`,
      [data.name.trim()]
    );
    return result.rows[0];
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('An allergen with this name already exists');
    throw err;
  }
}

export async function updateAllergen(
  db: Db,
  id: string,
  data: InsertAllergen,
  caller?: InternalCaller
): Promise<Allergen | null> {
  if (caller && (await useRustBackend('allergens', caller))) {
    // 404 → null; 409 → ConflictError (parity with SQL path).
    try {
      return await internalJson<Allergen>(`/api/allergens/${id}`, caller, {
        method: 'PATCH',
        body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
      });
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return null;
      if (isInternalApiError(err) && err.status === 409) {
        throw new ConflictError('An allergen with this name already exists');
      }
      throw err;
    }
  }

  try {
    const result = await db.query<Allergen>(
      `UPDATE allergen
       SET name = $1
       WHERE id = $2
       RETURNING id, name`,
      [data.name.trim(), id]
    );
    return result.rows[0] ?? null;
  } catch (err: any) {
    if (err?.code === '23505') throw new ConflictError('An allergen with this name already exists');
    throw err;
  }
}

/** Count how many ingredients and persons reference this allergen. */
export async function getAllergenRefCount(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<{ ingredients: number; persons: number }> {
  if (caller && (await useRustBackend('allergens', caller))) {
    // Rust returns `{ ingredients, persons }` as JSON numbers (i64 →
    // serde number); the shape matches the SQL path's `Number()`-cast
    // output, so pass through as-is.
    return internalJson<{ ingredients: number; persons: number }>(
      `/api/allergens/${id}/ref-count`,
      caller
    );
  }

  const result = await db.query<{ ingredients: string; persons: string }>(
    `SELECT
       (SELECT COUNT(*) FROM ingredient_allergen WHERE allergen_id = $1) AS ingredients,
       (SELECT COUNT(*) FROM person_allergen WHERE allergen_id = $1) AS persons`,
    [id]
  );
  const row = result.rows[0];
  return { ingredients: Number(row.ingredients), persons: Number(row.persons) };
}

export async function deleteAllergen(
  db: Db,
  id: string,
  caller?: InternalCaller
): Promise<boolean> {
  if (caller && (await useRustBackend('allergens', caller))) {
    // Rust returns 204 on success, 404 when nothing matched. Mirror
    // the SQL path's `boolean` by catching 404 → false.
    try {
      await internalJson<void>(`/api/allergens/${id}`, caller, { method: 'DELETE' });
      return true;
    } catch (err) {
      if (isInternalApiError(err) && err.status === 404) return false;
      throw err;
    }
  }

  const result = await db.query(
    `DELETE FROM allergen WHERE id = $1`,
    [id]
  );
  return (result.rowCount ?? 0) > 0;
}
