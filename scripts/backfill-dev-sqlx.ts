// One-shot: stamp `_sqlx_migrations` from the embedded Rust migration
// files so the Rust service's sqlx migrator is a no-op on startup when
// the TS migrator has already applied the schema.
//
// Mirrors `tests/e2e/global-setup.ts::backfillSqlxLedger`. Kept as a
// standalone script for ad-hoc dev use (e.g. verifying `/healthz/deep`
// against a dev DB that was migrated via `pnpm migrate`).
import { Pool } from 'pg';
import { readdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';

async function main() {
  const pool = new Pool({
    connectionString: `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST}:5432/${process.env.POSTGRES_DB}`,
  });
  const migDir = join(process.cwd(), 'admin-operations-rust/migrations');
  const files = readdirSync(migDir)
    .filter((f) => f.endsWith('.sql'))
    .sort();

  const client = await pool.connect();
  try {
    await client.query(`
      CREATE TABLE IF NOT EXISTS _sqlx_migrations (
        version BIGINT PRIMARY KEY,
        description TEXT NOT NULL,
        installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        success BOOLEAN NOT NULL,
        checksum BYTEA NOT NULL,
        execution_time BIGINT NOT NULL
      )
    `);
    for (const filename of files) {
      const m = /^(\d+)_(.+)\.sql$/.exec(filename);
      if (!m) continue;
      const version = Number.parseInt(m[1], 10);
      const description = m[2].replace(/_/g, ' ');
      const sql = readFileSync(join(migDir, filename), 'utf8');
      const checksum = createHash('sha384').update(sql, 'utf8').digest();
      await client.query(
        `INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
         VALUES ($1, $2, now(), true, $3, 0)
         ON CONFLICT (version) DO NOTHING`,
        [version, description, checksum],
      );
    }
    console.log(`backfilled ${files.length} rows`);
  } finally {
    client.release();
    await pool.end();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
