import { pool } from '../src/lib/server/db';

const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map(e => e.trim())
  .filter(Boolean);

if (ADMIN_EMAILS.length === 0) {
  console.error('[seed:admin] ERROR: ADMIN_EMAIL or ADMIN_EMAILS must be set');
  process.exit(1);
}

for (const email of ADMIN_EMAILS) {
  console.log(`[seed:admin] Upserting admin for ${email}...`);
  await pool.query(
		`INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
		 VALUES (gen_random_uuid(), 'Admin', $1, true, 'admin', now(), now())
		 ON CONFLICT (email) DO UPDATE SET role = 'admin', "updatedAt" = now()`,
    [email]
  );
  console.log(`[seed:admin] Done: ${email}`);
}

await pool.end();
console.log('[seed:admin] All admins seeded successfully');
