import type { Pool } from 'pg';
import type { Allergen, InsertAllergen } from '$lib/types';

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getAllergens(db: Pool): Promise<Allergen[]> {
  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergens
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getAllergenById(db: Pool, id: string): Promise<Allergen | null> {
  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergens
     WHERE id = $1`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getAllergenByName(db: Pool, name: string): Promise<Allergen | null> {
  const result = await db.query<Allergen>(
    `SELECT id, name
     FROM allergens
     WHERE LOWER(name) = LOWER($1)`,
    [name]
  );
  return result.rows[0] ?? null;
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createAllergen(db: Pool, data: InsertAllergen): Promise<Allergen> {
  const result = await db.query<Allergen>(
    `INSERT INTO allergens (name)
     VALUES ($1)
     RETURNING id, name`,
    [data.name.trim()]
  );
  return result.rows[0];
}

export async function updateAllergen(
  db: Pool,
  id: string,
  data: InsertAllergen
): Promise<Allergen | null> {
  const result = await db.query<Allergen>(
    `UPDATE allergens
     SET name = $1
     WHERE id = $2
     RETURNING id, name`,
    [data.name.trim(), id]
  );
  return result.rows[0] ?? null;
}

// Hard delete — safe only if no ingredient references this allergen.
// The FK on ingredient_allergens is CASCADE so rows there are cleaned up,
// but person_allergens is also CASCADE. Callers should confirm intent.
export async function deleteAllergen(db: Pool, id: string): Promise<boolean> {
  const result = await db.query(
    `DELETE FROM allergens WHERE id = $1`,
    [id]
  );
  return (result.rowCount ?? 0) > 0;
}
