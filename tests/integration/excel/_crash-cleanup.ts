import type { Pool } from 'pg';

/**
 * Vitest `afterAll` only runs on normal suite completion. If the process is
 * killed mid-run (SIGINT from Ctrl-C, SIGTERM from the watcher), rows seeded
 * via `ctx.pool` outside a per-test transaction survive on the shared
 * `meals_test` DB and bleed into every subsequent run — most visibly into
 * `tests/integration/core/orders.test.ts`'s "empty" assertions.
 *
 * Each pollution-prone suite calls this once in `beforeAll` with its own
 * namespace prefix (`TDS…_`, `T…_`, etc). On SIGINT/SIGTERM we best-effort
 * delete every row whose parent company's name starts with that prefix, then
 * exit with the signal's canonical code so the shell still reports the
 * interrupt. Normal afterAll cleanup is unaffected.
 */
export function registerCrashCleanup(pool: Pool, nsPrefix: string): void {
  const cleanup = async (signal: NodeJS.Signals) => {
    try {
      await pool.query(
        `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name LIKE $1)`,
        [`${nsPrefix}%`],
      );
      await pool.query(`DELETE FROM recipe  WHERE name LIKE $1`, [`${nsPrefix}%`]);
      await pool.query(`DELETE FROM company WHERE name LIKE $1`, [`${nsPrefix}%`]);
    } catch {
      // best effort — the DB may already be gone
    }
    // 130 = 128 + SIGINT(2); 143 = 128 + SIGTERM(15). Shell convention.
    process.exit(signal === 'SIGINT' ? 130 : 143);
  };
  process.once('SIGINT', () => cleanup('SIGINT'));
  process.once('SIGTERM', () => cleanup('SIGTERM'));
}
