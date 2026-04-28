import type { Pool } from 'pg';
import type { Company, InsertCompany, UpdateCompany, UserWithCompany } from '$lib/types';

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getCompanies(
  db: Pool,
  opts: { includeInactive?: boolean } = {}
): Promise<Company[]> {
  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM companies
     ${opts.includeInactive ? '' : 'WHERE active = TRUE'}
     ORDER BY name ASC`
  );
  return result.rows;
}

export async function getCompanyById(db: Pool, id: string): Promise<Company | null> {
  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM companies
     WHERE id = $1`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function getCompanyByName(db: Pool, name: string): Promise<Company | null> {
  const result = await db.query<Company>(
    `SELECT id, name, contact_email, phone, shipping_address, active, created_at
     FROM companies
     WHERE LOWER(name) = LOWER($1)`,
    [name]
  );
  return result.rows[0] ?? null;
}

// Returns all users belonging to a company, each with the company embedded.
// Useful for the company detail page's user list.
export async function getUsersByCompany(db: Pool, companyId: string): Promise<UserWithCompany[]> {
  const result = await db.query<UserWithCompany & {
    company_name: string;
    company_contact_email: string | null;
    company_phone: string | null;
    company_shipping_address: string | null;
    company_active: boolean;
    company_created_at: Date;
  }>(
    `SELECT
       u.id, u.company_id, u.email, u.role, u.created_at,
       c.name              AS company_name,
       c.contact_email     AS company_contact_email,
       c.phone             AS company_phone,
       c.shipping_address  AS company_shipping_address,
       c.active            AS company_active,
       c.created_at        AS company_created_at
     FROM users u
     JOIN companies c ON c.id = u.company_id
     WHERE u.company_id = $1
     ORDER BY u.email ASC`,
    [companyId]
  );

  return result.rows.map((row) => ({
    id: row.id,
    company_id: row.company_id,
    email: row.email,
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

export async function createCompany(db: Pool, data: InsertCompany): Promise<Company> {
  const result = await db.query<Company>(
    `INSERT INTO companies (name, contact_email, phone, shipping_address, active)
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
}

export async function updateCompany(
  db: Pool,
  id: string,
  data: UpdateCompany
): Promise<Company | null> {
  const result = await db.query<Company>(
    `UPDATE companies
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
}

// Soft delete — companies own persons and orders so hard delete is destructive.
// Setting active = false preserves all historical data.
export async function deactivateCompany(db: Pool, id: string): Promise<Company | null> {
  return updateCompany(db, id, { active: false });
}
