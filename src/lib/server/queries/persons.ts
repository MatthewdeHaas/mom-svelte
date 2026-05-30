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

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getPersonsByCompany(
  db: Db,
  companyId: string,
  opts: { includeInactive?: boolean } = {}
): Promise<Person[]> {
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
  opts: { includeInactive?: boolean } = {}
): Promise<Person | null> {
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
  opts: { includeInactive?: boolean } = {}
): Promise<PersonWithAllergens | null> {
  const person = await getPersonById(db, id, opts);
  if (!person) return null;

  const allergen = await getPersonAllergens(db, id);
  return { ...person, allergen };
}

export async function getPersonsWithAllergensByCompany(
  db: Db,
  companyId: string,
  opts: { includeInactive?: boolean } = {}
): Promise<PersonWithAllergens[]> {
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

export async function getPersonAllergens(db: Db, personId: string): Promise<Allergen[]> {
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

export async function createPerson(db: Db, data: InsertPerson): Promise<Person> {
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
  data: UpdatePerson
): Promise<Person | null> {
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
export async function deactivatePerson(db: Db, id: string): Promise<Person | null> {
  return updatePerson(db, id, { active: false });
}

// -------------------------------------------------------------
// ALLERGEN ASSOCIATIONS
// -------------------------------------------------------------

export async function addPersonAllergen(
  client: Pool | PoolClient,
  personId: string,
  allergenId: string
): Promise<void> {
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
  allergenId: string
): Promise<void> {
  await client.query(
    `DELETE FROM person_allergen
     WHERE person_id = $1 AND allergen_id = $2`,
    [personId, allergenId]
  );
}

export async function setPersonAllergens(
  client: Pool | PoolClient,
  personId: string,
  allergenIds: string[]
): Promise<void> {
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
  servingDate: Date
): Promise<AttendanceWithPerson[]> {
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
  data: UpsertAttendance
): Promise<Attendance> {
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
  records: UpsertAttendance[]
): Promise<void> {
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
