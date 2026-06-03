import type { Allergen, InsertAllergen } from '$lib/types';
import type { Db } from '$lib/server/db';

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getAllergens(db: Db): Promise<Allergen[]> {
  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergen
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getAllergenById(db: Db, id: string): Promise<Allergen | null> {
  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergen
     WHERE id = $1`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getAllergenByName(db: Db, name: string): Promise<Allergen | null> {
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

export async function createAllergen(db: Db, data: InsertAllergen): Promise<Allergen> {
  const result = await db.query<Allergen>(
    `INSERT INTO allergen (name)
     VALUES ($1)
     RETURNING id, name`,
    [data.name.trim()]
  );
  return result.rows[0];
}

export async function updateAllergen(
  db: Db,
  id: string,
  data: InsertAllergen
): Promise<Allergen | null> {
  const result = await db.query<Allergen>(
    `UPDATE allergen
     SET name = $1
     WHERE id = $2
     RETURNING id, name`,
    [data.name.trim(), id]
  );
  return result.rows[0] ?? null;
}

/** Count how many ingredients and persons reference this allergen. */
export async function getAllergenRefCount(
  db: Db,
  id: string
): Promise<{ ingredients: number; persons: number }> {
  const result = await db.query<{ ingredients: string; persons: string }>(
    `SELECT
       (SELECT COUNT(*) FROM ingredient_allergen WHERE allergen_id = $1) AS ingredients,
       (SELECT COUNT(*) FROM person_allergen WHERE allergen_id = $1) AS persons`,
    [id]
  );
  const row = result.rows[0];
  return { ingredients: Number(row.ingredients), persons: Number(row.persons) };
}

export async function deleteAllergen(db: Db, id: string): Promise<boolean> {
  const result = await db.query(
    `DELETE FROM allergen WHERE id = $1`,
    [id]
  );
  return (result.rowCount ?? 0) > 0;
}
