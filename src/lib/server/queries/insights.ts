import type { Db } from '$lib/server/db';
import type { OrderStatus, OrderSource } from '$lib/types';

// -------------------------------------------------------------
// TYPES
// -------------------------------------------------------------

export interface KPIMetrics {
  totalMeals: number;
  totalMealsPriorWeek: number;
  weekOverWeekChange: number | null; // percentage, null if no prior data
  statusCounts: Record<OrderStatus, number>;
  activeCompanies: number;
}

export interface VolumeSeries {
  date: string;
  total: number;
}

export interface TopRecipe {
  name: string;
  order_count: number;
  total_amount: number;
  percent_of_total: number;
}

export interface CompanyBreakdown {
  name: string;
  total_amount: number;
  order_count: number;
}

export interface SourceMix {
  order_source: OrderSource;
  total_amount: number;
  order_count: number;
}

export interface IngredientDemand {
  name: string;
  total_amount: number;
  unit: string;
  allergens: string;
}

export interface AttendanceDayRate {
  date: string;
  attending: number;
  total: number;
  rate: number;
}

export interface AttendanceByCompany {
  company_name: string;
  attending: number;
  total: number;
  rate: number;
}

// -------------------------------------------------------------
// KPI METRICS (fast scalar queries for server load)
// -------------------------------------------------------------

export async function getKPIMetrics(
  db: Db,
  from: string,
  to: string,
  priorFrom: string,
  priorTo: string
): Promise<KPIMetrics> {
  const [mealsResult, priorMealsResult, statusResult, companiesResult] = await Promise.all([
    db.query<{ total: string }>(
      `SELECT COALESCE(SUM(amount), 0) AS total
       FROM "order"
       WHERE serving_date >= $1::date AND serving_date <= $2::date
         AND status != 'cancelled'`,
      [from, to]
    ),
    db.query<{ total: string }>(
      `SELECT COALESCE(SUM(amount), 0) AS total
       FROM "order"
       WHERE serving_date >= $1::date AND serving_date <= $2::date
         AND status != 'cancelled'`,
      [priorFrom, priorTo]
    ),
    db.query<{ status: OrderStatus; count: string }>(
      `SELECT status, COUNT(*) AS count
       FROM "order"
       WHERE serving_date >= $1::date AND serving_date <= $2::date
       GROUP BY status`,
      [from, to]
    ),
    db.query<{ count: string }>(
      `SELECT COUNT(DISTINCT company_id) AS count
       FROM "order"
       WHERE serving_date >= $1::date AND serving_date <= $2::date
         AND status != 'cancelled'`,
      [from, to]
    ),
  ]);

  const totalMeals = parseFloat(mealsResult.rows[0]?.total ?? '0');
  const totalMealsPriorWeek = parseFloat(priorMealsResult.rows[0]?.total ?? '0');

  let weekOverWeekChange: number | null = null;
  if (totalMealsPriorWeek > 0) {
    weekOverWeekChange = Math.round(
      ((totalMeals - totalMealsPriorWeek) / totalMealsPriorWeek) * 100
    );
  }

  const statusCounts: Record<OrderStatus, number> = {
    pending: 0,
    confirmed: 0,
    fulfilled: 0,
    cancelled: 0,
  };
  for (const row of statusResult.rows) {
    statusCounts[row.status] = parseInt(row.count, 10);
  }

  return {
    totalMeals,
    totalMealsPriorWeek,
    weekOverWeekChange,
    statusCounts,
    activeCompanies: parseInt(companiesResult.rows[0]?.count ?? '0', 10),
  };
}

// -------------------------------------------------------------
// ORDER VOLUME SERIES
// -------------------------------------------------------------

export async function getOrderVolumeSeries(
  db: Db,
  from: string,
  to: string,
  granularity: 'daily' | 'weekly' = 'daily'
): Promise<VolumeSeries[]> {
  if (granularity === 'weekly') {
    const result = await db.query<{ date: string; total: string }>(
      `SELECT DATE_TRUNC('week', serving_date)::date::text AS date,
              SUM(amount) AS total
       FROM "order"
       WHERE serving_date >= $1::date AND serving_date <= $2::date
         AND status != 'cancelled'
       GROUP BY DATE_TRUNC('week', serving_date)
       ORDER BY date`,
      [from, to]
    );
    return result.rows.map((r) => ({ date: r.date, total: parseFloat(r.total) }));
  }

  const result = await db.query<{ date: string; total: string }>(
    `SELECT serving_date::date::text AS date,
            SUM(amount) AS total
     FROM "order"
     WHERE serving_date >= $1::date AND serving_date <= $2::date
       AND status != 'cancelled'
     GROUP BY serving_date::date
     ORDER BY date`,
    [from, to]
  );
  return result.rows.map((r) => ({ date: r.date, total: parseFloat(r.total) }));
}

// -------------------------------------------------------------
// TOP RECIPES
// -------------------------------------------------------------

export async function getTopRecipes(
  db: Db,
  from: string,
  to: string,
  limit: number = 10
): Promise<TopRecipe[]> {
  const result = await db.query<{ name: string; order_count: string; total_amount: string }>(
    `SELECT r.name,
            COUNT(o.id) AS order_count,
            SUM(o.amount) AS total_amount
     FROM "order" o
     JOIN recipe r ON r.id = o.recipe_id
     WHERE o.serving_date >= $1::date AND o.serving_date <= $2::date
       AND o.status != 'cancelled'
     GROUP BY r.id, r.name
     ORDER BY total_amount DESC
     LIMIT $3`,
    [from, to, limit]
  );

  const rows = result.rows.map((r) => ({
    name: r.name,
    order_count: parseInt(r.order_count, 10),
    total_amount: parseFloat(r.total_amount),
    percent_of_total: 0,
  }));

  const grandTotal = rows.reduce((sum, r) => sum + r.total_amount, 0);
  if (grandTotal > 0) {
    for (const row of rows) {
      row.percent_of_total = Math.round((row.total_amount / grandTotal) * 1000) / 10;
    }
  }

  return rows;
}

// -------------------------------------------------------------
// COMPANY BREAKDOWN
// -------------------------------------------------------------

export async function getCompanyBreakdown(
  db: Db,
  from: string,
  to: string
): Promise<CompanyBreakdown[]> {
  const result = await db.query<{ name: string; total_amount: string; order_count: string }>(
    `SELECT c.name,
            SUM(o.amount) AS total_amount,
            COUNT(o.id) AS order_count
     FROM "order" o
     JOIN company c ON c.id = o.company_id
     WHERE o.serving_date >= $1::date AND o.serving_date <= $2::date
       AND o.status != 'cancelled'
     GROUP BY c.id, c.name
     ORDER BY total_amount DESC`,
    [from, to]
  );

  return result.rows.map((r) => ({
    name: r.name,
    total_amount: parseFloat(r.total_amount),
    order_count: parseInt(r.order_count, 10),
  }));
}

// -------------------------------------------------------------
// SOURCE MIX
// -------------------------------------------------------------

export async function getSourceMix(
  db: Db,
  from: string,
  to: string
): Promise<SourceMix[]> {
  const result = await db.query<{ order_source: OrderSource; total_amount: string; order_count: string }>(
    `SELECT order_source,
            SUM(amount) AS total_amount,
            COUNT(*) AS order_count
     FROM "order"
     WHERE serving_date >= $1::date AND serving_date <= $2::date
       AND status != 'cancelled'
     GROUP BY order_source`,
    [from, to]
  );

  return result.rows.map((r) => ({
    order_source: r.order_source,
    total_amount: parseFloat(r.total_amount),
    order_count: parseInt(r.order_count, 10),
  }));
}

// -------------------------------------------------------------
// INGREDIENT DEMAND
// -------------------------------------------------------------

export async function getIngredientDemand(
  db: Db,
  from: string,
  to: string
): Promise<IngredientDemand[]> {
  const result = await db.query<{
    name: string;
    total_amount: string;
    unit: string | null;
    allergens: string | null;
  }>(
    `SELECT
       i.name,
       SUM(ri.amount * o.amount) AS total_amount,
       ri.unit,
       STRING_AGG(DISTINCT a.name, ', ' ORDER BY a.name) AS allergens
     FROM "order" o
     JOIN recipe r ON r.id = o.recipe_id
     JOIN recipe_ingredient ri ON ri.recipe_id = r.id
     JOIN ingredient i ON i.id = ri.ingredient_id
     LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
     LEFT JOIN allergen a ON a.id = ia.allergen_id
     WHERE o.serving_date >= $1::date AND o.serving_date <= $2::date
       AND o.status IN ('pending', 'confirmed')
     GROUP BY i.id, i.name, ri.unit
     ORDER BY i.name`,
    [from, to]
  );

  return result.rows.map((r) => ({
    name: r.name,
    total_amount: parseFloat(r.total_amount),
    unit: r.unit ?? '',
    allergens: r.allergens ?? '',
  }));
}

// -------------------------------------------------------------
// ATTENDANCE RATE (daily trend)
// -------------------------------------------------------------

export async function getAttendanceRate(
  db: Db,
  from: string,
  to: string
): Promise<AttendanceDayRate[]> {
  const result = await db.query<{
    date: string;
    attending: string;
    total: string;
    rate: string;
  }>(
    `SELECT a.serving_date::date::text AS date,
            COUNT(*) FILTER (WHERE a.attending = true) AS attending,
            COUNT(*) AS total,
            ROUND(
              COUNT(*) FILTER (WHERE a.attending = true)::numeric
              / NULLIF(COUNT(*), 0) * 100, 1
            ) AS rate
     FROM attendance a
     JOIN person p ON p.id = a.person_id
     WHERE a.serving_date >= $1::date AND a.serving_date <= $2::date
       AND p.active = true
     GROUP BY a.serving_date::date
     ORDER BY date`,
    [from, to]
  );

  return result.rows.map((r) => ({
    date: r.date,
    attending: parseInt(r.attending, 10),
    total: parseInt(r.total, 10),
    rate: parseFloat(r.rate),
  }));
}

// -------------------------------------------------------------
// ATTENDANCE BY COMPANY
// -------------------------------------------------------------

export async function getAttendanceByCompany(
  db: Db,
  from: string,
  to: string
): Promise<AttendanceByCompany[]> {
  const result = await db.query<{
    company_name: string;
    attending: string;
    total: string;
    rate: string;
  }>(
    `SELECT c.name AS company_name,
            COUNT(*) FILTER (WHERE a.attending = true) AS attending,
            COUNT(*) AS total,
            ROUND(
              COUNT(*) FILTER (WHERE a.attending = true)::numeric
              / NULLIF(COUNT(*), 0) * 100, 1
            ) AS rate
     FROM attendance a
     JOIN person p ON p.id = a.person_id
     JOIN company c ON c.id = p.company_id
     WHERE a.serving_date >= $1::date AND a.serving_date <= $2::date
       AND p.active = true
     GROUP BY c.id, c.name
     ORDER BY rate DESC`,
    [from, to]
  );

  return result.rows.map((r) => ({
    company_name: r.company_name,
    attending: parseInt(r.attending, 10),
    total: parseInt(r.total, 10),
    rate: parseFloat(r.rate),
  }));
}
