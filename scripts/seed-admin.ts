import { Pool } from 'pg';
import { randomUUID } from 'crypto';
import { auth } from '../src/lib/server/auth';

const pool = new Pool({
  host:     process.env.POSTGRES_HOST     ?? process.env.PGHOST,
  port:     Number(process.env.POSTGRES_PORT ?? process.env.PGPORT ?? 5432),
  user:     process.env.POSTGRES_USER     ?? process.env.PGUSER,
  password: process.env.POSTGRES_PASSWORD ?? process.env.PGPASSWORD,
  database: process.env.POSTGRES_DB       ?? process.env.PGDATABASE,
  connectionString: process.env.DATABASE_URL,
  ssl: process.env.DATABASE_URL && !process.env.DATABASE_URL.includes('@db:') && !process.env.DATABASE_URL.includes('@localhost')
    ? { rejectUnauthorized: false }
    : false,
});

const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map(e => e.trim())
  .filter(Boolean);

if (ADMIN_EMAILS.length === 0) {
  console.warn('[seed:admin] ADMIN_EMAIL / ADMIN_EMAILS not set — skipping');
  process.exit(0);
}

const ADMIN_INITIAL_PASSWORD = process.env.ADMIN_INITIAL_PASSWORD?.trim() || null;
if (!ADMIN_INITIAL_PASSWORD) {
  console.warn(
    '[seed:admin] ADMIN_INITIAL_PASSWORD not set — users will have no credential account and cannot log in with password. ' +
    'Set ADMIN_INITIAL_PASSWORD and re-run, or set it from the admin UI once logged in.'
  );
}

// Better Auth exposes an officially-supported internal context via `auth.$context`
// (used, e.g., by the password-reset route in `better-auth/dist/api/routes/password.mjs`).
// This gives us the configured password hasher and the internal adapter without
// requiring a session — essential for a seed script.
const ctx = ADMIN_INITIAL_PASSWORD ? await auth.$context : null;

for (const email of ADMIN_EMAILS) {
  console.log(`[seed:admin] Upserting admin for ${email}...`);
  const result = await pool.query<{ id: string }>(
    `INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
     VALUES ($1, 'Admin', $2, true, 'admin', now(), now())
     ON CONFLICT (email) DO UPDATE SET role = 'admin', "updatedAt" = now()
     RETURNING id`,
    [randomUUID(), email]
  );
  const userId = result.rows[0].id;

  if (ctx && ADMIN_INITIAL_PASSWORD) {
    // Mirror better-auth's own reset-password behavior: hash → if no credential
    // account yet, create one; otherwise update the password in place.
    const hashed = await ctx.password.hash(ADMIN_INITIAL_PASSWORD);
    const accounts = await ctx.internalAdapter.findAccounts(userId);
    const credential = accounts.find((a) => a.providerId === 'credential');
    if (!credential) {
      await ctx.internalAdapter.createAccount({
        userId,
        providerId: 'credential',
        accountId: userId,
        password: hashed,
      });
      console.log(`[seed:admin] Credential account created for ${email}`);
    } else {
      await ctx.internalAdapter.updatePassword(userId, hashed);
      console.log(`[seed:admin] Credential password refreshed for ${email}`);
    }
  }

  console.log(`[seed:admin] Done: ${email}`);
}

await pool.end();
console.log('[seed:admin] All admins seeded successfully');
