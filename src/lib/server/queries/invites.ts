import type { Db } from '$lib/server/db';
import type { Invite, InsertInvite } from '$lib/types';

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getInviteByToken(db: Db, token: string): Promise<Invite | null> {
  const result = await db.query<Invite>(
    `SELECT id, company_id, email, token, expires_at, used_at, created_at
     FROM invite
     WHERE token = $1`,
    [token]
  );
  return result.rows[0] ?? null;
}

export async function getInvitesByCompany(db: Db, companyId: string): Promise<Invite[]> {
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

export async function createInvite(db: Db, data: InsertInvite): Promise<Invite> {
  const result = await db.query<Invite>(
    `INSERT INTO invite (company_id, email, token, expires_at)
     VALUES ($1, $2, $3, $4)
     RETURNING id, company_id, email, token, expires_at, used_at, created_at`,
    [data.company_id, data.email.toLowerCase().trim(), data.token, data.expires_at]
  );
  return result.rows[0];
}

export async function markInviteUsed(db: Db, id: string): Promise<void> {
  await db.query(`UPDATE invite SET used_at = now() WHERE id = $1`, [id]);
}

export async function revokeInvite(db: Db, id: string): Promise<void> {
  await db.query(`UPDATE invite SET used_at = now() WHERE id = $1`, [id]);
}
