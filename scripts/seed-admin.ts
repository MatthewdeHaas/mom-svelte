import pg from 'pg';

const pool = new pg.Pool({
  host:     process.env.POSTGRES_HOST     ?? process.env.PGHOST,
  port:     Number(process.env.POSTGRES_PORT ?? process.env.PGPORT ?? 5432),
  user:     process.env.POSTGRES_USER     ?? process.env.PGUSER,
  password: process.env.POSTGRES_PASSWORD ?? process.env.PGPASSWORD,
  database: process.env.POSTGRES_DB       ?? process.env.PGDATABASE,
  connectionString: process.env.DATABASE_URL,
  ssl: process.env.DATABASE_URL ? { rejectUnauthorized: false } : false,
});

const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map(e => e.trim())
  .filter(Boolean);

if (ADMIN_EMAILS.length === 0) {
  console.warn('[seed:admin] ADMIN_EMAIL / ADMIN_EMAILS not set — skipping');
  process.exit(0);
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
