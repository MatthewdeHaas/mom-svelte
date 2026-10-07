import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getKPIMetrics,
  getOrderVolumeSeries,
  getTopRecipes,
  getCompanyBreakdown,
  getSourceMix,
  getIngredientDemand,
  getAttendanceRate,
  getAttendanceByCompany,
} from '$lib/server/queries/insights';
import { createCompany } from '$lib/server/queries/companies';
import { createRecipe } from '$lib/server/queries/recipes';
import { createOrder } from '$lib/server/queries/orders';
import { createTestContext } from './helpers';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let counter = 0;
function uniqueName(prefix = 'Co') {
  return `${prefix}_${++counter}_${Date.now()}`;
}

async function seedCompany() {
  return createCompany(ctx.client, { name: uniqueName('Company') });
}

async function seedRecipe() {
  return createRecipe(ctx.client, { name: uniqueName('Recipe') });
}

async function seedOrderForDate(
  companyId: string,
  recipeId: string | null,
  servingDate: string,
  amount: number,
  status: 'pending' | 'confirmed' | 'fulfilled' | 'cancelled' = 'confirmed'
) {
  return createOrder(ctx.client, {
    company_id: companyId,
    recipe_id: recipeId,
    order_source: 'admin_manual',
    status,
    amount,
    // Pass the ISO string straight through. `new Date('2025-06-10')` parses
    // as UTC midnight, but the pg driver serializes a `Date` using the
    // client's local TZ (e.g. `2025-06-09 17:00-07`), and the subsequent
    // `::date` cast keeps that local-TZ interpretation — so the row lands
    // on the previous day. The DB column is `date`, so a plain ISO string
    // round-trips as expected regardless of client TZ.
    serving_date: servingDate,
    shipment_date: servingDate,
  });
}

// ---------------------------------------------------------------------------
// getKPIMetrics
// ---------------------------------------------------------------------------

describe('getKPIMetrics', () => {
  it('returns zeros when no orders exist', async () => {
    const kpi = await getKPIMetrics(ctx.client, '2025-06-09', '2025-06-15', '2025-06-02', '2025-06-08');
    expect(kpi.totalMeals).toBe(0);
    expect(kpi.totalMealsPriorWeek).toBe(0);
    expect(kpi.weekOverWeekChange).toBeNull();
    expect(kpi.activeCompanies).toBe(0);
    expect(kpi.statusCounts.pending).toBe(0);
  });

  it('computes totals and week-over-week change', async () => {
    const company = await seedCompany();

    // Current week orders
    await seedOrderForDate(company.id, null, '2025-06-10', 50);
    await seedOrderForDate(company.id, null, '2025-06-12', 30);

    // Prior week orders
    await seedOrderForDate(company.id, null, '2025-06-03', 40);

    const kpi = await getKPIMetrics(ctx.client, '2025-06-09', '2025-06-15', '2025-06-02', '2025-06-08');
    expect(kpi.totalMeals).toBe(80);
    expect(kpi.totalMealsPriorWeek).toBe(40);
    expect(kpi.weekOverWeekChange).toBe(100); // +100%
    expect(kpi.activeCompanies).toBe(1);
  });

  it('excludes cancelled orders from meal totals', async () => {
    const company = await seedCompany();
    await seedOrderForDate(company.id, null, '2025-06-10', 50, 'confirmed');
    await seedOrderForDate(company.id, null, '2025-06-11', 20, 'cancelled');

    const kpi = await getKPIMetrics(ctx.client, '2025-06-09', '2025-06-15', '2025-06-02', '2025-06-08');
    expect(kpi.totalMeals).toBe(50);
    // cancelled still counts in status breakdown
    expect(kpi.statusCounts.cancelled).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// getOrderVolumeSeries
// ---------------------------------------------------------------------------

describe('getOrderVolumeSeries', () => {
  it('returns daily totals grouped by date', async () => {
    const company = await seedCompany();
    await seedOrderForDate(company.id, null, '2025-06-10', 10);
    await seedOrderForDate(company.id, null, '2025-06-10', 5);
    await seedOrderForDate(company.id, null, '2025-06-12', 20);

    const series = await getOrderVolumeSeries(ctx.client, '2025-06-09', '2025-06-15', 'daily');
    expect(series.length).toBe(2);
    expect(series[0]).toEqual({ date: '2025-06-10', total: 15 });
    expect(series[1]).toEqual({ date: '2025-06-12', total: 20 });
  });

  it('returns weekly totals when granularity is weekly', async () => {
    const company = await seedCompany();
    await seedOrderForDate(company.id, null, '2025-06-10', 10);
    await seedOrderForDate(company.id, null, '2025-06-12', 20);

    const series = await getOrderVolumeSeries(ctx.client, '2025-06-01', '2025-06-30', 'weekly');
    expect(series.length).toBeGreaterThanOrEqual(1);
    const week = series.find((s) => s.total === 30);
    expect(week).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// getTopRecipes
// ---------------------------------------------------------------------------

describe('getTopRecipes', () => {
  it('returns recipes ranked by total amount', async () => {
    const company = await seedCompany();
    const recipe1 = await seedRecipe();
    const recipe2 = await seedRecipe();

    await seedOrderForDate(company.id, recipe1.id, '2025-06-10', 100);
    await seedOrderForDate(company.id, recipe2.id, '2025-06-11', 50);
    await seedOrderForDate(company.id, recipe1.id, '2025-06-12', 25);

    const top = await getTopRecipes(ctx.client, '2025-06-09', '2025-06-15', 10);
    expect(top.length).toBe(2);
    expect(top[0].name).toBe(recipe1.name);
    expect(top[0].total_amount).toBe(125);
    expect(top[1].name).toBe(recipe2.name);
    expect(top[1].total_amount).toBe(50);
    // Percentages should sum to ~100 of the top N
    expect(top[0].percent_of_total + top[1].percent_of_total).toBeCloseTo(100, 0);
  });

  it('respects the limit parameter', async () => {
    const company = await seedCompany();
    const recipe1 = await seedRecipe();
    const recipe2 = await seedRecipe();

    await seedOrderForDate(company.id, recipe1.id, '2025-06-10', 100);
    await seedOrderForDate(company.id, recipe2.id, '2025-06-11', 50);

    const top = await getTopRecipes(ctx.client, '2025-06-09', '2025-06-15', 1);
    expect(top.length).toBe(1);
    expect(top[0].name).toBe(recipe1.name);
  });
});

// ---------------------------------------------------------------------------
// getCompanyBreakdown
// ---------------------------------------------------------------------------

describe('getCompanyBreakdown', () => {
  it('returns companies ranked by volume', async () => {
    const company1 = await seedCompany();
    const company2 = await seedCompany();

    await seedOrderForDate(company1.id, null, '2025-06-10', 100);
    await seedOrderForDate(company2.id, null, '2025-06-10', 200);

    const breakdown = await getCompanyBreakdown(ctx.client, '2025-06-09', '2025-06-15');
    expect(breakdown.length).toBe(2);
    expect(breakdown[0].name).toBe(company2.name);
    expect(breakdown[0].total_amount).toBe(200);
    expect(breakdown[1].name).toBe(company1.name);
    expect(breakdown[1].total_amount).toBe(100);
  });
});

// ---------------------------------------------------------------------------
// getSourceMix
// ---------------------------------------------------------------------------

describe('getSourceMix', () => {
  it('groups orders by source', async () => {
    const company = await seedCompany();
    await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 10,
      serving_date: new Date('2025-06-10'),
      shipment_date: new Date('2025-06-10'),
      status: 'confirmed',
    });
    await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'client_portal',
      amount: 25,
      serving_date: new Date('2025-06-11'),
      shipment_date: new Date('2025-06-11'),
      status: 'confirmed',
    });

    const mix = await getSourceMix(ctx.client, '2025-06-09', '2025-06-15');
    const admin = mix.find((m) => m.order_source === 'admin_manual');
    const client = mix.find((m) => m.order_source === 'client_portal');
    expect(admin?.total_amount).toBe(10);
    expect(client?.total_amount).toBe(25);
  });
});

// ---------------------------------------------------------------------------
// getIngredientDemand
// ---------------------------------------------------------------------------

describe('getIngredientDemand', () => {
  it('aggregates ingredient quantities from pending/confirmed orders', async () => {
    const company = await seedCompany();
    const recipe = await seedRecipe();

    // Add an ingredient to the recipe
    await ctx.client.query(
      `INSERT INTO ingredient (id, name, active) VALUES (gen_random_uuid(), 'Flour', true) RETURNING id`
    );
    const ingResult = await ctx.client.query<{ id: string }>(
      `SELECT id FROM ingredient WHERE name = 'Flour'`
    );
    const ingredientId = ingResult.rows[0].id;

    await ctx.client.query(
      `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) VALUES ($1, $2, 2.5, 'kg')`,
      [recipe.id, ingredientId]
    );

    // Place a confirmed order for 10 servings
    await seedOrderForDate(company.id, recipe.id, '2025-06-10', 10, 'confirmed');
    // Place a cancelled order (should be excluded)
    await seedOrderForDate(company.id, recipe.id, '2025-06-11', 5, 'cancelled');

    const demand = await getIngredientDemand(ctx.client, '2025-06-09', '2025-06-15');
    expect(demand.length).toBe(1);
    expect(demand[0].name).toBe('Flour');
    expect(demand[0].total_amount).toBe(25); // 2.5 * 10
    expect(demand[0].unit).toBe('kg');
  });
});

// ---------------------------------------------------------------------------
// getAttendanceRate
// ---------------------------------------------------------------------------

describe('getAttendanceRate', () => {
  it('returns daily attendance rates', async () => {
    const company = await seedCompany();

    // Create persons
    const p1 = await ctx.client.query<{ id: string }>(
      `INSERT INTO person (id, company_id, name, active) VALUES (gen_random_uuid(), $1, 'Alice', true) RETURNING id`,
      [company.id]
    );
    const p2 = await ctx.client.query<{ id: string }>(
      `INSERT INTO person (id, company_id, name, active) VALUES (gen_random_uuid(), $1, 'Bob', true) RETURNING id`,
      [company.id]
    );

    // Record attendance
    await ctx.client.query(
      `INSERT INTO attendance (id, person_id, serving_date, attending) VALUES
        (gen_random_uuid(), $1, '2025-06-10', true),
        (gen_random_uuid(), $2, '2025-06-10', false)`,
      [p1.rows[0].id, p2.rows[0].id]
    );

    const rates = await getAttendanceRate(ctx.client, '2025-06-09', '2025-06-15');
    expect(rates.length).toBe(1);
    expect(rates[0].date).toBe('2025-06-10');
    expect(rates[0].attending).toBe(1);
    expect(rates[0].total).toBe(2);
    expect(rates[0].rate).toBe(50);
  });
});

// ---------------------------------------------------------------------------
// getAttendanceByCompany
// ---------------------------------------------------------------------------

describe('getAttendanceByCompany', () => {
  it('returns per-company attendance rates', async () => {
    const company1 = await seedCompany();
    const company2 = await seedCompany();

    // Company 1: 2 people, both attending
    const p1 = await ctx.client.query<{ id: string }>(
      `INSERT INTO person (id, company_id, name, active) VALUES (gen_random_uuid(), $1, 'Alice', true) RETURNING id`,
      [company1.id]
    );
    const p2 = await ctx.client.query<{ id: string }>(
      `INSERT INTO person (id, company_id, name, active) VALUES (gen_random_uuid(), $1, 'Bob', true) RETURNING id`,
      [company1.id]
    );

    // Company 2: 1 person, not attending
    const p3 = await ctx.client.query<{ id: string }>(
      `INSERT INTO person (id, company_id, name, active) VALUES (gen_random_uuid(), $1, 'Charlie', true) RETURNING id`,
      [company2.id]
    );

    await ctx.client.query(
      `INSERT INTO attendance (id, person_id, serving_date, attending) VALUES
        (gen_random_uuid(), $1, '2025-06-10', true),
        (gen_random_uuid(), $2, '2025-06-10', true),
        (gen_random_uuid(), $3, '2025-06-10', false)`,
      [p1.rows[0].id, p2.rows[0].id, p3.rows[0].id]
    );

    const byCompany = await getAttendanceByCompany(ctx.client, '2025-06-09', '2025-06-15');
    expect(byCompany.length).toBe(2);
    // Sorted by rate DESC, so company1 (100%) first
    expect(byCompany[0].company_name).toBe(company1.name);
    expect(byCompany[0].rate).toBe(100);
    expect(byCompany[1].company_name).toBe(company2.name);
    expect(byCompany[1].rate).toBe(0);
  });
});
