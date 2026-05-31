import { Pool, type PoolClient } from 'pg';

// SvelteKit doesn't support variable interpolation in .env files (but Docker does)
const connectionString = `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST}:5432/${process.env.POSTGRES_DB}`;

export const pool = new Pool({ connectionString, max: 5 });

// Queries accept either a Pool (normal use) or a PoolClient (inside a transaction,
// e.g. integration tests that roll back after each test).
export type Db = Pool | PoolClient;
