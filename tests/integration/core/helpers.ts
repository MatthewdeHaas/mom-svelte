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
    await ctx.client.query('BEGIN');
  };

  ctx.rollback = async () => {
    await ctx.client.query('ROLLBACK');
    ctx.client.release();
  };

  return ctx;
}
