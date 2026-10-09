import { Pool } from 'pg';
import { randomBytes, randomUUID, createHmac, createHash } from 'crypto';
import { execSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'fs';
import { join } from 'path';

/**
 * Produce the signed cookie value Better Auth expects.
 *
 * Better Auth signs cookies with HMAC-SHA256 using BETTER_AUTH_SECRET and stores:
 *   `${rawToken}.${base64(hmac)}`
 *
 * The raw token is stored in the DB; the signed string goes in the cookie.
 * Playwright's addCookies takes the plain (not URL-encoded) value.
 */
function signToken(rawToken: string, secret: string): string {
  const hmac = createHmac('sha256', secret);
  hmac.update(rawToken);
  const sig = hmac.digest('base64');
  return `${rawToken}.${sig}`;
}

export const E2E_ADMIN_EMAIL = 'e2e-admin@test.internal';
export const E2E_CLIENT_EMAIL = 'e2e-client@test.internal';
export const E2E_PENDING_EMAIL = 'e2e-pending@test.internal';
export const E2E_COMPANY_NAME = '__e2e_test_company__';

// Client portal fixtures
export const E2E_MENU_NAME = '__e2e_test_menu__';
export const E2E_MENU_RECIPE_NAME = '__e2e_test_menu_recipe__';
// Valid invite token (unused, not expired)
export const E2E_INVITE_TOKEN = 'e2e-valid-invite-token-000000000000000';
// A second valid token used only for the unauthenticated-redirect test
// (the first valid token is consumed by the pending-user stamp test)
export const E2E_INVITE_TOKEN_UNAUTH = 'e2e-valid-invite-token-unauth-00000';
// Expired invite token
export const E2E_INVITE_TOKEN_EXPIRED = 'e2e-expired-invite-token-00000000000';
// Already-used invite token
export const E2E_INVITE_TOKEN_USED = 'e2e-used-invite-token-000000000000000';

// Fixed far-future date used for export test orders.
// serving_date and shipment_date are the same (constraint: shipment <= serving).
// This date falls in the week 2099-01-12→2099-01-18, so the packing-slips
// endpoint with ?shipment_date=2099-01-15 (week-of) will include it.
export const E2E_EXPORT_DATE = '2099-01-15';
export const E2E_RECIPE_NAME = '__e2e_test_recipe__';
export const E2E_INGREDIENT_NAME = '__e2e_test_ingredient__';
export const E2E_ALLERGEN_NAME = '__e2e_test_allergen__';

function makePool() {
  return new Pool({
    connectionString: `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST ?? 'localhost'}:5432/${process.env.POSTGRES_DB}`,
  });
}

async function cleanup(pool: Pool) {
  const emails = [E2E_ADMIN_EMAIL, E2E_CLIENT_EMAIL, E2E_PENDING_EMAIL];
  // Sessions cascade-delete from "user" via ON DELETE CASCADE, but delete
  // explicitly to be safe in case constraints differ between environments.
  await pool.query(
    `DELETE FROM "session" WHERE "userId" IN (SELECT id FROM "user" WHERE email = ANY($1))`,
    [emails]
  );
  await pool.query(`DELETE FROM "user" WHERE email = ANY($1)`, [emails]);
  // Export test data: order → recipe_ingredient → recipe/ingredient/allergen
  await pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name = $1)`,
    [E2E_COMPANY_NAME]
  );
  await pool.query(
    `DELETE FROM recipe_ingredient WHERE recipe_id IN (SELECT id FROM recipe WHERE name = $1)`,
    [E2E_RECIPE_NAME]
  );
  await pool.query(`DELETE FROM recipe WHERE name = $1`, [E2E_RECIPE_NAME]);
  await pool.query(
    `DELETE FROM ingredient_allergen WHERE ingredient_id IN (SELECT id FROM ingredient WHERE name = $1)`,
    [E2E_INGREDIENT_NAME]
  );
  await pool.query(`DELETE FROM ingredient WHERE name = $1`, [E2E_INGREDIENT_NAME]);
  await pool.query(`DELETE FROM allergen WHERE name = $1`, [E2E_ALLERGEN_NAME]);
  await pool.query(`DELETE FROM company WHERE name = $1`, [E2E_COMPANY_NAME]);
  // Client portal fixtures: menus and invite tokens
  await pool.query(`DELETE FROM invite WHERE token = ANY($1)`, [
    [E2E_INVITE_TOKEN, E2E_INVITE_TOKEN_UNAUTH, E2E_INVITE_TOKEN_EXPIRED, E2E_INVITE_TOKEN_USED],
  ]);
  await pool.query(
    `DELETE FROM menu_recipe WHERE menu_id IN (SELECT id FROM menu WHERE name = $1)`,
    [E2E_MENU_NAME]
  );
  await pool.query(`DELETE FROM menu WHERE name = $1`, [E2E_MENU_NAME]);
  await pool.query(`DELETE FROM recipe WHERE name = $1`, [E2E_MENU_RECIPE_NAME]);
}

/**
 * Build the Rust API release binary once per `pnpm test:e2e` run. Playwright's
 * second `webServer` entry (see `playwright.config.ts`) launches this binary;
 * doing the build here means the webServer start isn't a cold compile.
 *
 * First run: ~2 min from scratch. Subsequent runs: incremental, usually a few
 * hundred ms. If the binary already exists we still invoke `cargo build` so
 * source edits picked up — cargo decides what work is actually needed.
 *
 * `cargo` must be on PATH. In this repo, direnv's flake profile puts it there
 * when the user runs `pnpm test:e2e` from a direnv-activated shell. If this
 * step fails with "cargo: command not found", run `direnv allow` once in the
 * repo root (or source `.direnv/flake-profile-*.rc`) and retry.
 */
function buildRustApi() {
  const rustDir = join(process.cwd(), 'admin-operations-rust');
  if (!existsSync(rustDir)) {
    throw new Error(`globalSetup: ${rustDir} not found — Rust workspace missing`);
  }
  console.log('[globalSetup] cargo build --release -p api …');
  execSync('cargo build --release -p api', {
    cwd: rustDir,
    stdio: 'inherit',
  });
}

/**
 * Reconcile the two migration ledgers before the Rust API starts up.
 *
 * The TS side (`scripts/migrate.ts`) tracks applied files in a hand-rolled
 * `migrations` table; sqlx tracks them in `_sqlx_migrations`. Both migrators
 * run the same files, but neither knows about the other's ledger. In e2e
 * the DB is pre-migrated by `pnpm run migrate`, so sqlx on startup sees an
 * empty `_sqlx_migrations` and tries to re-run everything — the very first
 * statement (`CREATE TABLE "user"`) explodes because the table already
 * exists.
 *
 * Fix: stamp `_sqlx_migrations` with the checksum sqlx would compute for
 * each file. For files the TS migrator already applied, we only stamp
 * (schema is already there). For files it hasn't (currently just 024, a
 * Rust-only `DELETE FROM feature_flag` cleanup), execute the SQL first,
 * then stamp — in a single transaction so a crash doesn't leave the ledger
 * out of sync with the schema.
 *
 * Idempotent: `ON CONFLICT (version) DO NOTHING` so re-running
 * `pnpm test:e2e` is safe. sqlx's checksum algorithm is SHA-384 over the
 * raw bytes of each `.sql` file (see sqlx-core's `migrate/migration.rs`).
 *
 * In production there is only one migrator (sqlx), so none of this runs —
 * this is strictly a dev/e2e-side bridge for the two-ledger period.
 */
async function backfillSqlxLedger(pool: Pool) {
  const migrationsDir = join(process.cwd(), 'admin-operations-rust/migrations');
  if (!existsSync(migrationsDir)) {
    throw new Error(
      `globalSetup: ${migrationsDir} not found — did cargo build sync the migrations?`
    );
  }

  const files = readdirSync(migrationsDir)
    .filter((f) => f.endsWith('.sql'))
    .sort(); // lexicographic — relies on 001_, 002_ prefixes

  // Which files has the TS migrator already applied?
  // The table may not exist on a brand-new DB; treat the absence as "none applied".
  let appliedByTs = new Set<string>();
  try {
    const res = await pool.query<{ filename: string }>(
      'SELECT filename FROM migrations'
    );
    appliedByTs = new Set(res.rows.map((r) => r.filename));
  } catch {
    // `migrations` table doesn't exist yet — fine, we'll execute everything.
  }

  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    for (const filename of files) {
      // "024_drop_use_rust_backend_exports.sql" → version 24, "drop use rust backend exports"
      const match = /^(\d+)_(.+)\.sql$/.exec(filename);
      if (!match) continue;
      const version = Number.parseInt(match[1], 10);
      const description = match[2].replace(/_/g, ' ');

      // Skip the stamp if this version is already recorded.
      const existing = await client.query<{ version: string }>(
        'SELECT version FROM _sqlx_migrations WHERE version = $1',
        [version]
      );
      if (existing.rowCount && existing.rowCount > 0) continue;

      const sql = readFileSync(join(migrationsDir, filename), 'utf8');
      // sqlx uses SHA-384 of the file bytes.
      const checksum = createHash('sha384').update(sql, 'utf8').digest();

      // If the TS migrator hasn't applied this file, we need to actually
      // execute it before stamping — otherwise the schema and the ledger
      // disagree and the next real migration will act on stale state.
      if (!appliedByTs.has(filename)) {
        console.log(`[globalSetup] executing TS-skipped migration ${filename}`);
        await client.query(sql);
      }

      await client.query(
        `INSERT INTO _sqlx_migrations
           (version, description, installed_on, success, checksum, execution_time)
         VALUES ($1, $2, now(), true, $3, 0)
         ON CONFLICT (version) DO NOTHING`,
        [version, description, checksum]
      );
    }
    await client.query('COMMIT');
  } catch (err) {
    await client.query('ROLLBACK');
    throw err;
  } finally {
    client.release();
  }
}

export default async function globalSetup() {
  // Pre-build the Rust API binary before Playwright spawns webServers.
  // Playwright awaits globalSetup fully before launching webServers, so the
  // binary is guaranteed to exist by the time the Rust webServer command
  // tries to exec it.
  buildRustApi();

  const pool = makePool();

  try {
    // Reconcile the two migration ledgers so Rust's `db::migrate` is a
    // no-op on startup. See `backfillSqlxLedger` for the full story.
    await backfillSqlxLedger(pool);

    // Clean up any leftover state from a previous interrupted run
    await cleanup(pool);

    // --- Company ---
    const companyId = randomUUID();
    await pool.query(
      `INSERT INTO company (id, name, active, shipping_address)
       VALUES ($1, $2, true, '123 Test Street, Test City')`,
      [companyId, E2E_COMPANY_NAME]
    );

    // --- Admin user ---
    const adminId = randomUUID();
    await pool.query(
      `INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
       VALUES ($1, 'E2E Admin', $2, true, 'admin', now(), now())`,
      [adminId, E2E_ADMIN_EMAIL]
    );

    // --- Client user assigned to the test company ---
    const clientId = randomUUID();
    await pool.query(
      `INSERT INTO "user" (id, name, email, "emailVerified", role, company_id, "createdAt", "updatedAt")
       VALUES ($1, 'E2E Client', $2, true, 'user', $3, now(), now())`,
      [clientId, E2E_CLIENT_EMAIL, companyId]
    );

    // --- Client user with no company (pending access) ---
    const pendingId = randomUUID();
    await pool.query(
      `INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
       VALUES ($1, 'E2E Pending', $2, true, 'user', now(), now())`,
      [pendingId, E2E_PENDING_EMAIL]
    );

    // --- Sessions: one per test user, valid for 7 days ---
    // Better Auth stores the raw token in the DB but the cookie must be signed:
    //   cookie value = encodeURIComponent(`${rawToken}.${base64HmacSha256(rawToken, secret)}`)
    const secret = process.env.BETTER_AUTH_SECRET ?? 'e2e-test-secret';
    const adminRaw = randomBytes(32).toString('hex');
    const clientRaw = randomBytes(32).toString('hex');
    const pendingRaw = randomBytes(32).toString('hex');
    const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000);

    for (const [userId, rawToken] of [
      [adminId, adminRaw],
      [clientId, clientRaw],
      [pendingId, pendingRaw],
    ] as [string, string][]) {
      await pool.query(
        `INSERT INTO "session" (id, "expiresAt", token, "createdAt", "updatedAt", "userId")
         VALUES ($1, $2, $3, now(), now(), $4)`,
        [randomUUID(), expiresAt, rawToken, userId]
      );
    }

    // Store the signed cookie values so fixtures can inject them directly.
    const adminToken = signToken(adminRaw, secret);
    const clientToken = signToken(clientRaw, secret);
    const pendingToken = signToken(pendingRaw, secret);

    // --- Persist tokens so fixtures can inject them as cookies ---
    const stateDir = join(process.cwd(), 'tests/e2e/.auth');
    mkdirSync(stateDir, { recursive: true });
    writeFileSync(
      join(stateDir, 'state.json'),
      JSON.stringify({ adminToken, clientToken, pendingToken, companyId }, null, 2)
    );

    // --- Export test seed data ---
    // allergen → ingredient → recipe → recipe_ingredient → order
    // serving_date = shipment_date = E2E_EXPORT_DATE (2099-01-15)
    // ingredient amount per recipe = 2 kg, order amount = 3 → shopping list total = 6 kg
    const allergenId = randomUUID();
    await pool.query(
      `INSERT INTO allergen (id, name) VALUES ($1, $2)`,
      [allergenId, E2E_ALLERGEN_NAME]
    );

    const ingredientId = randomUUID();
    await pool.query(
      `INSERT INTO ingredient (id, name, active) VALUES ($1, $2, true)`,
      [ingredientId, E2E_INGREDIENT_NAME]
    );

    await pool.query(
      `INSERT INTO ingredient_allergen (ingredient_id, allergen_id) VALUES ($1, $2)`,
      [ingredientId, allergenId]
    );

    const recipeId = randomUUID();
    await pool.query(
      `INSERT INTO recipe (id, name, active, created_at, updated_at)
       VALUES ($1, $2, true, now(), now())`,
      [recipeId, E2E_RECIPE_NAME]
    );

    // 2 kg of E2E ingredient per recipe serving
    await pool.query(
      `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
       VALUES ($1, $2, 2, 'kg')`,
      [recipeId, ingredientId]
    );

    // 1 order: amount=3, frozen=false, chilled=true, meal_category='lunch'
    // total shopping list contribution: 2 kg × 3 = 6 kg
    await pool.query(
      `INSERT INTO "order" (id, company_id, recipe_id, created_by, order_source, status,
         meal_category, amount, serving_date, shipment_date, frozen, chilled)
       VALUES ($1, $2, $3, $4, 'admin_manual', 'confirmed',
         'lunch', 3, $5, $5, false, true)`,
      [randomUUID(), companyId, recipeId, adminId, E2E_EXPORT_DATE]
    );

    // ---------------------------------------------------------------
    // Client portal seed data
    // ---------------------------------------------------------------

    // A recipe that will appear in the test menu
    const menuRecipeId = randomUUID();
    await pool.query(
      `INSERT INTO recipe (id, name, active, created_at, updated_at)
       VALUES ($1, $2, true, now(), now())`,
      [menuRecipeId, E2E_MENU_RECIPE_NAME]
    );

    // A menu active from today spanning 28 days, with one slot on each
    // cycle_week=1, day_of_week matching today+1 (tomorrow, guaranteed future).
    // We pick 'monday' as a concrete day — tests that need a specific date
    // must compute the next monday dynamically.
    const menuId = randomUUID();
    const todayStr = new Date().toISOString().slice(0, 10);
    // end_date = today + 56 days (well past the 4-week look-ahead)
    const horizonStr = new Date(Date.now() + 56 * 86400000).toISOString().slice(0, 10);
    await pool.query(
      `INSERT INTO menu (id, name, is_active, start_date, end_date, created_at)
       VALUES ($1, $2, true, $3, $4, now())`,
      [menuId, E2E_MENU_NAME, todayStr, horizonStr]
    );

    // Add a slot for every day-of-week so tests can hit the menu on any day.
    // cycle_week=1 for all — the test menu is a simple 1-week repeating cycle.
    const days = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday'];
    for (const day of days) {
      await pool.query(
        `INSERT INTO menu_recipe (id, menu_id, recipe_id, cycle_week, day_of_week, meal_category)
         VALUES ($1, $2, $3, 1, $4, 'Lunch')`,
        [randomUUID(), menuId, menuRecipeId, day]
      );
    }

    // A confirmed order for the client — serving_date = tomorrow.
    // Used to verify attendance dates are populated from orders.
    const tomorrowStr = new Date(Date.now() + 86400000).toISOString().slice(0, 10);
    await pool.query(
      `INSERT INTO "order" (id, company_id, recipe_id, created_by, order_source, status,
         meal_category, amount, serving_date, shipment_date, frozen, chilled)
       VALUES ($1, $2, $3, $4, 'client_portal', 'confirmed',
         'Lunch', 10, $5, $5, false, false)`,
      [randomUUID(), companyId, menuRecipeId, clientId, tomorrowStr]
    );

    // Invite tokens for invite flow tests
    // 1. Valid, unused token pointing to the test company
    await pool.query(
      `INSERT INTO invite (id, company_id, email, token, expires_at)
       VALUES ($1, $2, 'invited@test.internal', $3, now() + interval '48 hours')`,
      [randomUUID(), companyId, E2E_INVITE_TOKEN]
    );
    // 1b. Second valid token used only by the unauthenticated-redirect test
    //     (the primary token is consumed by the pending-user stamp test)
    await pool.query(
      `INSERT INTO invite (id, company_id, email, token, expires_at)
       VALUES ($1, $2, 'invited2@test.internal', $3, now() + interval '48 hours')`,
      [randomUUID(), companyId, E2E_INVITE_TOKEN_UNAUTH]
    );
    // 2. Expired token
    await pool.query(
      `INSERT INTO invite (id, company_id, email, token, expires_at)
       VALUES ($1, $2, 'expired@test.internal', $3, now() - interval '1 hour')`,
      [randomUUID(), companyId, E2E_INVITE_TOKEN_EXPIRED]
    );
    // 3. Already-used token
    await pool.query(
      `INSERT INTO invite (id, company_id, email, token, expires_at, used_at)
       VALUES ($1, $2, 'used@test.internal', $3, now() + interval '48 hours', now())`,
      [randomUUID(), companyId, E2E_INVITE_TOKEN_USED]
    );

    // Persist extra IDs needed by tests
    const state = JSON.parse(
      (await import('fs')).readFileSync(join(process.cwd(), 'tests/e2e/.auth/state.json'), 'utf-8')
    );
    (await import('fs')).writeFileSync(
      join(process.cwd(), 'tests/e2e/.auth/state.json'),
      JSON.stringify({ ...state, menuId, menuRecipeId, tomorrowStr }, null, 2)
    );

    // Pre-warm Vite dev-mode routes so the first browser navigation to each
    // route doesn't trigger an SSR HMR reload that cancels in-flight navigations.
    const baseUrl = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
    const warmupRoutes = [
      '/client',
      '/client/roster',
      `/client/roster/${companyId}`, // intentionally 404 — still compiles the module
      '/client/menu',
      '/client/orders',
      '/client/orders/new',
      '/client/attendance',
      '/client/settings',
      '/admin',
      '/admin/companies',
      `/admin/companies/${companyId}`,
      '/admin/orders',
      '/admin/orders/import',
      '/admin/ingredients',
      '/invite/warmup-placeholder',
      '/login',
    ];
    await Promise.allSettled(
      warmupRoutes.map((path) =>
        fetch(`${baseUrl}${path}`, { signal: AbortSignal.timeout(8000) }).catch(() => {})
      )
    );
    // Give Vite a moment to finish any SSR invalidation signals before tests begin
    await new Promise((resolve) => setTimeout(resolve, 500));

  } finally {
    await pool.end();
  }

  // Playwright calls the returned function as globalTeardown
  return async () => {
    const pool = makePool();
    try {
      await cleanup(pool);
    } finally {
      await pool.end();
    }
  };
}
