import { Pool } from 'pg';
import type { PoolClient } from 'pg';

export interface TestContext {
  /** The shared pool — use ctx.client inside tests, not this directly. */
  pool: Pool;
  /** A per-test client with an open transaction. Use this as the `db` arg. */
  client: PoolClient;
  setup: () => Promise<void>;
  teardown: () => Promise<void>;
  begin: () => Promise<void>;
  rollback: () => Promise<void>;
}

/**
 * Reads connection details from env vars (same ones Docker Compose exports).
 * Override in your .env.test or CI environment as needed.
 */
function makePool(): Pool {
  const connectionString = `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST}:5432/${process.env.POSTGRES_DB}`;
  return new Pool({ connectionString });
}

export function createTestContext(): TestContext {
  // Typed as any initially; assigned in setup/begin.
  const ctx = {} as TestContext;

  ctx.setup = async () => {
    ctx.pool = makePool();
    // Verify connectivity early so failures surface in beforeAll, not mid-suite.
    const probe = await ctx.pool.connect();
    await probe.query('SELECT 1');
    probe.release();
  };

  ctx.teardown = async () => {
    await ctx.pool.end();
  };

  ctx.begin = async () => {
    ctx.client = await ctx.pool.connect();
    // REPEATABLE READ, not the Postgres default READ COMMITTED. Vitest's
    // default `pool: 'forks'` runs test files in parallel across workers,
    // and the pollution-prone excel suites (downstream-identical,
    // real-client, normalization) commit through `ctx.pool` *outside*
    // any transaction. Under READ COMMITTED, every statement in this
    // transaction gets a fresh snapshot, so a sibling worker's commit
    // becomes visible mid-test — e.g. financials' `activeRecipeCount()`
    // baseline and the paginated-margins `COUNT(*)` disagree by one.
    // REPEATABLE READ pins the snapshot to `BEGIN`, so cross-worker
    // commits are invisible for the test's lifetime. The rolled-back
    // writes stay invisible to everyone else either way.
    await ctx.client.query('BEGIN ISOLATION LEVEL REPEATABLE READ');
  };

  ctx.rollback = async () => {
    await ctx.client.query('ROLLBACK');
    ctx.client.release();
  };

  return ctx;
}
