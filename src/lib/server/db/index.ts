import { Pool, type PoolClient } from 'pg';

// Build the connection string from discrete POSTGRES_* env vars (SvelteKit
// doesn't support variable interpolation in .env files; Docker does).
//
// POSTGRES_PORT is honored and defaults to 5432. The port MUST NOT be
// hardcoded: Railway's TCP proxy (used by `scripts/seed-admin-interactive.sh`
// to reach prod Postgres from a laptop) listens on a random high port, and
// anything that hardcodes :5432 silently times out against it.
//
// If the discrete vars aren't set but DATABASE_URL is, fall through to that —
// matches the convention used by `scripts/seed-admin.ts`'s own pool and keeps
// one-off tools (migrations, seeds) working with a single env var.
const host = process.env.POSTGRES_HOST;
const port = process.env.POSTGRES_PORT ?? '5432';
const user = process.env.POSTGRES_USER;
const password = process.env.POSTGRES_PASSWORD;
const database = process.env.POSTGRES_DB;

const connectionString =
	host && user && password && database
		? `postgresql://${user}:${password}@${host}:${port}/${database}`
		: process.env.DATABASE_URL;

export const pool = new Pool({ connectionString, max: 5 });

// Queries accept either a Pool (normal use) or a PoolClient (inside a transaction,
// e.g. integration tests that roll back after each test).
export type Db = Pool | PoolClient;
