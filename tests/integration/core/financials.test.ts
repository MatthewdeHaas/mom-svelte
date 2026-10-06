import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getRecipesWithMargins,
  getRecipesWithMarginsPaginated,
  getInvoiceData,
} from '$lib/server/queries/financials';
import { createTestContext } from './helpers';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// -------------------------------------------------------------
// Helpers — mirror the Rust-side seed_* helpers from
// crates/api/tests/financials.rs so the two suites exercise the
// exact same shapes.
//
// NOTE: tests run against a dev DB with baseline seed data, so
// everything is scoped by id/name rather than by total count. The
// Rust suite uses `#[sqlx::test]` to get a fresh DB per test — not
// available on the TS side.
// -------------------------------------------------------------

let counter = 0;
function uniqueName(prefix: string) {
  return `${prefix}_${++counter}_${Date.now()}`;
}

async function seedCompany(name = uniqueName('TestCo'), shippingAddress: string | null = null) {
  const result = await ctx.client.query<{ id: string }>(
    `INSERT INTO company (name, shipping_address, active)
     VALUES ($1, $2, TRUE) RETURNING id`,
    [name, shippingAddress]
  );
  return { id: result.rows[0].id, name };
}

async function seedRecipe(name: string, price: number | null, active = true) {
  const result = await ctx.client.query<{ id: string }>(
    `INSERT INTO recipe (name, active, price_per_meal)
     VALUES ($1, $2, $3) RETURNING id`,
    [name, active, price]
  );
  return result.rows[0].id;
}

async function seedIngredient(name: string, cost: number | null) {
  const result = await ctx.client.query<{ id: string }>(
    `INSERT INTO ingredient (name, active, cost_per_unit)
     VALUES ($1, TRUE, $2) RETURNING id`,
    [name, cost]
  );
  return result.rows[0].id;
}

async function attachIngredient(
  recipeId: string,
  ingredientId: string,
  amount: number,
  unit: string
) {
  await ctx.client.query(
    `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
     VALUES ($1, $2, $3, $4)`,
    [recipeId, ingredientId, amount, unit]
  );
}

async function seedOrder(
  companyId: string,
  recipeId: string,
  status: string,
  amount: number,
  servingDate: string
) {
  const result = await ctx.client.query<{ id: string }>(
    `INSERT INTO "order"
       (company_id, recipe_id, order_source, status, meal_category,
        amount, serving_date, shipment_date, frozen, chilled, recurring)
     VALUES ($1, $2, 'admin_manual', $3, 'Lunch',
             $4, $5::date, $5::date, FALSE, FALSE, FALSE)
     RETURNING id`,
    [companyId, recipeId, status, amount, servingDate]
  );
  return result.rows[0].id;
}

const BOGUS = '00000000-0000-0000-0000-000000000000';

// ---------------------------------------------------------------------------
// getRecipesWithMargins
// ---------------------------------------------------------------------------

describe('getRecipesWithMargins', () => {
  it('includes a seeded recipe with no price (margin=null)', async () => {
    const name = uniqueName('NoPriceRecipe');
    await seedRecipe(name, null);
    const rows = await getRecipesWithMargins(ctx.client);
    const row = rows.find((r) => r.name === name);
    expect(row).toBeDefined();
    expect(row!.price_per_meal).toBeNull();
    expect(row!.ingredient_cost).toBeNull();
    expect(row!.margin).toBeNull();
    expect(row!.margin_pct).toBeNull();
  });

  it('returns ingredient_cost=null and margin=null when the recipe has no ingredients', async () => {
    const name = uniqueName('Priced');
    await seedRecipe(name, 12.5);
    const rows = await getRecipesWithMargins(ctx.client);
    const row = rows.find((r) => r.name === name);
    expect(row).toBeDefined();
    expect(row!.price_per_meal).toBe(12.5);
    expect(row!.ingredient_cost).toBeNull();
    expect(row!.margin).toBeNull();
    expect(row!.margin_pct).toBeNull();
  });

  it('computes margin and margin_pct', async () => {
    // Price 10.00; ingredient cost = (2 × 1.50) + (3 × 1.00) = 6.00
    // Margin = 4.00; pct = 40.0
    const name = uniqueName('BigMargin');
    const r = await seedRecipe(name, 10);
    const a = await seedIngredient(uniqueName('Flour'), 1.5);
    const b = await seedIngredient(uniqueName('Oil'), 1);
    await attachIngredient(r, a, 2, 'cup');
    await attachIngredient(r, b, 3, 'tbsp');

    const rows = await getRecipesWithMargins(ctx.client);
    const row = rows.find((r) => r.name === name);
    expect(row).toBeDefined();
    expect(row!.price_per_meal).toBe(10);
    expect(row!.ingredient_cost).toBe(6);
    expect(row!.margin).toBe(4);
    expect(row!.margin_pct).toBe(40);
  });

  it('excludes inactive recipes', async () => {
    const activeName = uniqueName('Active');
    const inactiveName = uniqueName('Inactive');
    await seedRecipe(activeName, 5);
    await seedRecipe(inactiveName, 99, false);

    const rows = await getRecipesWithMargins(ctx.client);
    expect(rows.find((r) => r.name === activeName)).toBeDefined();
    expect(rows.find((r) => r.name === inactiveName)).toBeUndefined();
  });

  it('orders rows by name ASC', async () => {
    // Unique prefix forces the three seeded rows to be adjacent in
    // the globally-ordered result set, so we can assert their relative
    // ordering without caring about baseline data.
    const prefix = uniqueName('zzz_order');
    const zeta = `${prefix}_Zeta`;
    const alpha = `${prefix}_Alpha`;
    const mu = `${prefix}_Mu`;
    await seedRecipe(zeta, null);
    await seedRecipe(alpha, null);
    await seedRecipe(mu, null);

    const rows = await getRecipesWithMargins(ctx.client);
    const seeded = rows.filter((r) => r.name.startsWith(prefix)).map((r) => r.name);
    expect(seeded).toEqual([alpha, mu, zeta]);
  });
});

// ---------------------------------------------------------------------------
// getRecipesWithMarginsPaginated
// ---------------------------------------------------------------------------

describe('getRecipesWithMarginsPaginated', () => {
  // Baseline count is non-zero in the dev DB; capture it per-test and
  // assert on deltas instead of absolute totals.
  async function activeRecipeCount(): Promise<number> {
    const r = await ctx.client.query<{ count: string }>(
      `SELECT COUNT(*) AS count FROM recipe WHERE active = TRUE`
    );
    return parseInt(r.rows[0].count, 10);
  }

  it('reports total matching COUNT(*) and respects perPage', async () => {
    const before = await activeRecipeCount();
    for (let i = 0; i < 3; i++) {
      await seedRecipe(uniqueName(`PagR${i}`), 1);
    }
    const page = await getRecipesWithMarginsPaginated(ctx.client, { page: 1, perPage: 100 });
    expect(page.total).toBe(before + 3);
    expect(page.page).toBe(1);
    expect(page.perPage).toBe(100);
    // totalPages = ceil(total / perPage)
    expect(page.totalPages).toBe(Math.max(1, Math.ceil((before + 3) / 100)));
    expect(page.data.length).toBeLessThanOrEqual(100);
  });

  it('respects page and perPage boundaries', async () => {
    // Set perPage small enough that we land on >1 page regardless of baseline.
    const perPage = 10;
    const p1 = await getRecipesWithMarginsPaginated(ctx.client, { page: 1, perPage });
    const total = p1.total;
    const expectedTotalPages = Math.max(1, Math.ceil(total / perPage));
    expect(p1.page).toBe(1);
    expect(p1.perPage).toBe(perPage);
    expect(p1.totalPages).toBe(expectedTotalPages);
    expect(p1.data).toHaveLength(Math.min(perPage, total));

    if (expectedTotalPages > 1) {
      const p2 = await getRecipesWithMarginsPaginated(ctx.client, { page: 2, perPage });
      expect(p2.page).toBe(2);
      const page2Expected = Math.min(perPage, total - perPage);
      expect(p2.data).toHaveLength(page2Expected);
    }
  });

  it('count ignores the ingredients join', async () => {
    // One recipe with 5 ingredients — total must grow by exactly 1,
    // not by 5. (The count query is `COUNT(*) FROM recipe`, not through
    // the recipe_ingredient join that the data query uses.)
    const before = await activeRecipeCount();
    const name = uniqueName('Multi');
    const r = await seedRecipe(name, 10);
    for (let i = 0; i < 5; i++) {
      const ing = await seedIngredient(uniqueName(`I${i}`), 0.5);
      await attachIngredient(r, ing, 1, 'g');
    }

    const page = await getRecipesWithMarginsPaginated(ctx.client, { page: 1, perPage: 100 });
    expect(page.total).toBe(before + 1);
    const seeded = page.data.find((row) => row.name === name);
    // May or may not be on page 1 depending on alphabetical slot;
    // when it is, confirm the ingredient_cost rolled up correctly.
    if (seeded) expect(seeded.ingredient_cost).toBe(2.5);
  });
});

// ---------------------------------------------------------------------------
// getInvoiceData
// ---------------------------------------------------------------------------

describe('getInvoiceData', () => {
  it('returns null when the company does not exist', async () => {
    const inv = await getInvoiceData(ctx.client, BOGUS, '2026-01-01', '2026-12-31');
    expect(inv).toBeNull();
  });

  it('returns empty line_items when the company has no matching orders', async () => {
    const c = await seedCompany(uniqueName('EmptyInvCo'));
    const inv = await getInvoiceData(ctx.client, c.id, '2026-01-01', '2026-12-31');
    expect(inv).not.toBeNull();
    expect(inv!.company_id).toBe(c.id);
    expect(inv!.company_name).toBe(c.name);
    expect(inv!.subtotal).toBe(0);
    expect(inv!.line_items).toEqual([]);
  });

  it('aggregates confirmed and fulfilled orders, excluding others', async () => {
    const c = await seedCompany(uniqueName('AggCo'));
    const r1 = await seedRecipe(uniqueName('RecipeA'), 10);
    const r2 = await seedRecipe(uniqueName('RecipeB'), 5);

    // In range + eligible status
    await seedOrder(c.id, r1, 'confirmed', 3, '2026-06-10');
    await seedOrder(c.id, r2, 'fulfilled', 4, '2026-06-20');
    // Out of range
    await seedOrder(c.id, r1, 'fulfilled', 10, '2026-07-01');
    // Wrong status
    await seedOrder(c.id, r1, 'pending', 10, '2026-06-15');
    await seedOrder(c.id, r2, 'cancelled', 10, '2026-06-15');

    const inv = await getInvoiceData(ctx.client, c.id, '2026-06-01', '2026-06-30');
    expect(inv).not.toBeNull();
    expect(inv!.line_items).toHaveLength(2);
    // (3 × 10) + (4 × 5) = 50
    expect(inv!.subtotal).toBe(50);
    // Ordered by serving_date ASC — RecipeA (Jun 10) first.
    expect(inv!.line_items[0].amount).toBe(3);
    expect(inv!.line_items[0].unit_price).toBe(10);
    expect(inv!.line_items[0].line_total).toBe(30);
    expect(inv!.line_items[1].line_total).toBe(20);
  });

  it('excludes orders against recipes without a price', async () => {
    const c = await seedCompany(uniqueName('UnpricedCo'));
    const pricedName = uniqueName('Priced');
    const priced = await seedRecipe(pricedName, 5);
    const unpriced = await seedRecipe(uniqueName('Unpriced'), null);

    await seedOrder(c.id, priced, 'confirmed', 2, '2026-06-10');
    await seedOrder(c.id, unpriced, 'confirmed', 2, '2026-06-11');

    const inv = await getInvoiceData(ctx.client, c.id, '2026-06-01', '2026-06-30');
    expect(inv!.line_items).toHaveLength(1);
    expect(inv!.line_items[0].recipe_name).toBe(pricedName);
    expect(inv!.subtotal).toBe(10);
  });

  it('preserves shipping_address on the response', async () => {
    const c = await seedCompany(uniqueName('ShipCo'), '123 Delivery Lane');

    const inv = await getInvoiceData(ctx.client, c.id, '2026-01-01', '2026-12-31');
    expect(inv!.shipping_address).toBe('123 Delivery Lane');
  });
});
