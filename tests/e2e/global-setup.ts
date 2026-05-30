import { Pool } from 'pg';
import { randomBytes, randomUUID, createHmac } from 'crypto';
import { mkdirSync, writeFileSync } from 'fs';
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
}

export default async function globalSetup() {
  const pool = makePool();

  try {
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
