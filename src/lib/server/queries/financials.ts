import type { Db } from '$lib/server/db';
import {
  type PaginationParams,
  type PaginatedResult,
  paginationSQL,
  buildPaginatedResult,
} from './pagination';

// -------------------------------------------------------------
// TYPES
// -------------------------------------------------------------

export interface RecipeMargin {
  id: string;
  name: string;
  price_per_meal: number | null;
  ingredient_cost: number | null;
  margin: number | null;
  margin_pct: number | null;
}

export interface InvoiceLineItem {
  order_id: string;
  recipe_name: string;
  serving_date: Date;
  amount: number;
  unit_price: number;
  line_total: number;
}

export interface InvoiceSummary {
  company_id: string;
  company_name: string;
  shipping_address: string | null;
  line_items: InvoiceLineItem[];
  subtotal: number;
}

// -------------------------------------------------------------
// RECIPE MARGINS
// Rolls up ingredient costs per recipe and compares to price.
// -------------------------------------------------------------

export async function getRecipesWithMargins(db: Db): Promise<RecipeMargin[]> {
  const result = await db.query<{
    id: string;
    name: string;
    price_per_meal: string | null;
    ingredient_cost: string | null;
  }>(
    `SELECT
       r.id,
       r.name,
       r.price_per_meal::TEXT,
       SUM(ri.amount * i.cost_per_unit)::TEXT AS ingredient_cost
     FROM recipe r
     LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
     LEFT JOIN ingredient i ON i.id = ri.ingredient_id
     WHERE r.active = TRUE
     GROUP BY r.id
     ORDER BY r.name ASC`
  );

  return result.rows.map((row) => {
    const price = row.price_per_meal ? parseFloat(row.price_per_meal) : null;
    const cost = row.ingredient_cost ? parseFloat(row.ingredient_cost) : null;
    const margin = price != null && cost != null ? price - cost : null;
    const margin_pct = price != null && cost != null && price > 0
      ? ((price - cost) / price) * 100
      : null;

    return { id: row.id, name: row.name, price_per_meal: price, ingredient_cost: cost, margin, margin_pct };
  });
}

export async function getRecipesWithMarginsPaginated(
  db: Db,
  pagination: PaginationParams
): Promise<PaginatedResult<RecipeMargin>> {
  const countResult = await db.query<{ count: string }>(
    `SELECT COUNT(*) AS count FROM recipe WHERE active = TRUE`
  );
  const total = parseInt(countResult.rows[0].count, 10);

  const pag = paginationSQL(pagination, 1);
  const result = await db.query<{
    id: string;
    name: string;
    price_per_meal: string | null;
    ingredient_cost: string | null;
  }>(
    `SELECT
       r.id,
       r.name,
       r.price_per_meal::TEXT,
       SUM(ri.amount * i.cost_per_unit)::TEXT AS ingredient_cost
     FROM recipe r
     LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
     LEFT JOIN ingredient i ON i.id = ri.ingredient_id
     WHERE r.active = TRUE
     GROUP BY r.id
     ORDER BY r.name ASC
     ${pag.sql}`,
    [...pag.values]
  );

  const data = result.rows.map((row) => {
    const price = row.price_per_meal ? parseFloat(row.price_per_meal) : null;
    const cost = row.ingredient_cost ? parseFloat(row.ingredient_cost) : null;
    const margin = price != null && cost != null ? price - cost : null;
    const margin_pct = price != null && cost != null && price > 0
      ? ((price - cost) / price) * 100
      : null;

    return { id: row.id, name: row.name, price_per_meal: price, ingredient_cost: cost, margin, margin_pct };
  });

  return buildPaginatedResult(data, total, pagination);
}

// -------------------------------------------------------------
// INVOICE LINE ITEMS
// Fulfilled orders for a company in a date range.
// -------------------------------------------------------------

export async function getInvoiceData(
  db: Db,
  companyId: string,
  from: string,
  to: string
): Promise<InvoiceSummary | null> {
  const companyResult = await db.query<{ id: string; name: string; shipping_address: string | null }>(
    `SELECT id, name, shipping_address FROM company WHERE id = $1`,
    [companyId]
  );
  const company = companyResult.rows[0];
  if (!company) return null;

  const result = await db.query<{
    order_id: string;
    recipe_name: string;
    serving_date: Date;
    amount: string;
    unit_price: string;
    line_total: string;
  }>(
    `SELECT
       o.id AS order_id,
       r.name AS recipe_name,
       o.serving_date,
       o.amount::TEXT,
       r.price_per_meal::TEXT AS unit_price,
       (o.amount * r.price_per_meal)::TEXT AS line_total
     FROM "order" o
     JOIN recipe r ON r.id = o.recipe_id
     WHERE o.company_id = $1
       AND o.status IN ('confirmed', 'fulfilled')
       AND o.serving_date >= $2::date
       AND o.serving_date <= $3::date
       AND r.price_per_meal IS NOT NULL
     ORDER BY o.serving_date, r.name`,
    [companyId, from, to]
  );

  const line_items: InvoiceLineItem[] = result.rows.map((row) => ({
    order_id: row.order_id,
    recipe_name: row.recipe_name,
    serving_date: row.serving_date,
    amount: parseFloat(row.amount),
    unit_price: parseFloat(row.unit_price),
    line_total: parseFloat(row.line_total),
  }));

  const subtotal = line_items.reduce((sum, item) => sum + item.line_total, 0);

  return {
    company_id: company.id,
    company_name: company.name,
    shipping_address: company.shipping_address,
    line_items,
    subtotal,
  };
}
